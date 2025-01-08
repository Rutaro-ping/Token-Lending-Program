use {
    crate::{
        error::LendingError,
        math::Decimal,
        state::{LendingMarket, Obligation, Reserve},
    },
    solana_program::{
        account_info::{next_account_info, AccountInfo},
        clock::Clock,
        entrypoint::ProgramResult,
        msg,
        program_pack::Pack,
        pubkey::Pubkey,
        sysvar::Sysvar,
    },
};

#[inline(never)] // avoid stack frame limit
pub(super) fn process_liquidate_obligation(
    program_id: &Pubkey,
    liquidity_amount: u64,
    accounts: &[AccountInfo],
) -> ProgramResult {
    if liquidity_amount == 0 {
        msg!("Liquidity amount provided cannot be zero");
        return Err(LendingError::InvalidAmount.into());
    }
    let account_info_iter = &mut accounts.iter();
    let source_liquidity_info = next_account_info(account_info_iter)?;
    let destination_collateral_info = next_account_info(account_info_iter)?;
    let repay_reserve_info = next_account_info(account_info_iter)?;
    let repay_reserve_liquidity_supply_info = next_account_info(account_info_iter)?;
    let withdraw_reserve_info = next_account_info(account_info_iter)?;
    let withdraw_reserve_collateral_supply_info = next_account_info(account_info_iter)?;
    let obligation_info = next_account_info(account_info_iter)?;
    let lending_market_info = next_account_info(account_info_iter)?;
    let lending_market_authority_info = next_account_info(account_info_iter)?;
    let user_transfer_authority_info = next_account_info(account_info_iter)?;
    let clock = &Clock::from_account_info(next_account_info(account_info_iter)?)?;
    let token_program_id = next_account_info(account_info_iter)?;
    let lending_market = LendingMarket::unpack(&lending_market_info.data.borrow())?;
    if lending_market_info.owner != program_id {
        msg!("Lending market provided is not owned by the lending program");
        return Err(LendingError::InvalidAccountOwner.into());
    }

    if &lending_market.token_program_id != token_program_id.key {
        msg!("Lending market token program does not match the token program provided");
        return Err(LendingError::InvalidTokenProgram.into());
    }
    let mut repay_reserve = Reserve::unpack(&repay_reserve_info.data.borrow())?;
    if repay_reserve_info.owner != program_id {
        msg!("Repay reserve provided is not owned by the lending program");
        return Err(LendingError::InvalidAccountOwner.into());
    }
    if &repay_reserve.lending_market != lending_market_info.key {
        msg!("Repay reserve lending market does not match the lending market provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    if &repay_reserve.liquidity.supply_pubkey != repay_reserve_liquidity_supply_info.key {
        msg!("Repay reserve liquidity supply does not match the repay reserve liquidity supply provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    if &repay_reserve.liquidity.supply_pubkey == source_liquidity_info.key {
        msg!("Repay reserve liquidity supply cannot be used as the source liquidity provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    if &repay_reserve.collateral.supply_pubkey == destination_collateral_info.key {
        msg!(
            "Repay reserve collateral supply cannot be used as the destination collateral provided"
        );
        return Err(LendingError::InvalidAccountInput.into());
    }
    if repay_reserve.last_update.is_stale(clock.slot)? {
        msg!("Repay reserve is stale and must be refreshed in the current slot");
        return Err(LendingError::ReserveStale.into());
    }
    let withdraw_reserve = Reserve::unpack(&withdraw_reserve_info.data.borrow())?;
    if withdraw_reserve_info.owner != program_id {
        msg!("Withdraw reserve provided is not owned by the lending program");
        return Err(LendingError::InvalidAccountOwner.into());
    }
    if &withdraw_reserve.lending_market != lending_market_info.key {
        msg!("Withdraw reserve lending market does not match the lending market provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    if &withdraw_reserve.collateral.supply_pubkey != withdraw_reserve_collateral_supply_info.key {
        msg!("Withdraw reserve collateral supply does not match the withdraw reserve collateral supply provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    if &withdraw_reserve.liquidity.supply_pubkey == source_liquidity_info.key {
        msg!("Withdraw reserve liquidity supply cannot be used as the source liquidity provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    if &withdraw_reserve.collateral.supply_pubkey == destination_collateral_info.key {
        msg!("Withdraw reserve collateral supply cannot be used as the destination collateral provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    if withdraw_reserve.last_update.is_stale(clock.slot)? {
        msg!("Withdraw reserve is stale and must be refreshed in the current slot");
        return Err(LendingError::ReserveStale.into());
    }

    let mut obligation = Obligation::unpack(&obligation_info.data.borrow())?;
    if obligation_info.owner != program_id {
        msg!("Obligation provided is not owned by the lending program");
        return Err(LendingError::InvalidAccountOwner.into());
    }
    if &obligation.lending_market != lending_market_info.key {
        msg!("Obligation lending market does not match the lending market provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    if obligation.last_update.is_stale(clock.slot)? {
        msg!("Obligation is stale and must be refreshed in the current slot");
        return Err(LendingError::ObligationStale.into());
    }
    if obligation.deposited_value == Decimal::zero() {
        msg!("Obligation deposited value is zero");
        return Err(LendingError::ObligationDepositsZero.into());
    }
    if obligation.borrowed_value == Decimal::zero() {
        msg!("Obligation borrowed value is zero");
        return Err(LendingError::ObligationBorrowsZero.into());
    }
    if obligation.borrowed_value < obligation.unhealthy_borrow_value {
        msg!("Obligation is healthy and cannot be liquidated");
        return Err(LendingError::ObligationHealthy.into());
    }
    let (liquidity, liquidity_index) =
        obligation.find_liquidity_in_borrows(*repay_reserve_info.key)?;
    if liquidity.market_value == Decimal::zero() {
        msg!("Obligation borrow value is zero");
        return Err(LendingError::ObligationLiquidityEmpty.into());
    }
    let (collateral, collateral_index) =
        obligation.find_collateral_in_deposits(*withdraw_reserve_info.key)?;
    if collateral.market_value == Decimal::zero() {
        msg!("Obligation deposit value is zero");
        return Err(LendingError::ObligationCollateralEmpty.into());
    }
    let authority_signer_seeds = &[
        lending_market_info.key.as_ref(),
        &[lending_market.bump_seed],
    ];
    let lending_market_authority_pubkey =
        Pubkey::create_program_address(authority_signer_seeds, program_id)?;
    if &lending_market_authority_pubkey != lending_market_authority_info.key {
        msg!(
            "Derived lending market authority does not match the lending market authority provided"
        );
        return Err(LendingError::InvalidMarketAuthority.into());
    }
    // let CalculateLiquidationResult {
    //     settle_amount,
    //     repay_amount,
    //     withdraw_amount,
    // } = withdraw_reserve.calculate_liquidation(
    //     liquidity_amount,
    //     &obligation,
    //     liquidity,
    //     collateral,
    // )?;
    Ok(())
}
