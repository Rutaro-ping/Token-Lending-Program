use {
    super::{spl_token_transfer, TokenTransferParams},
    crate::{
        error::LendingError,
        math::Decimal,
        state::{CalculateBorrowResult, LendingMarket, Obligation, Reserve},
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
pub(super) fn process_borrow_obligation_liquidity(
    program_id: &Pubkey,
    liquidity_amount: u64,
    slippage_limit: u64,
    accounts: &[AccountInfo],
) -> ProgramResult {
    if liquidity_amount == 0 {
        msg!("Liquidity amount provided cannot be zero");
        return Err(LendingError::InvalidAmount.into());
    }

    let account_info_iter = &mut accounts.iter();
    let source_liquidity_info = next_account_info(account_info_iter)?;
    let destination_liquidity_info = next_account_info(account_info_iter)?;
    let borrow_reserve_info = next_account_info(account_info_iter)?;
    let borrow_reserve_liquidity_fee_receiver_info = next_account_info(account_info_iter)?;
    let obligation_info = next_account_info(account_info_iter)?;
    let lending_market_info = next_account_info(account_info_iter)?;
    let lending_market_authority_info = next_account_info(account_info_iter)?;
    let obligation_owner_info = next_account_info(account_info_iter)?;
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

    let mut borrow_reserve = Reserve::unpack(&borrow_reserve_info.data.borrow())?;
    if borrow_reserve_info.owner != program_id {
        msg!("Borrow reserve provided is not owned by the lending program");
        return Err(LendingError::InvalidAccountOwner.into());
    }
    if &borrow_reserve.lending_market != lending_market_info.key {
        msg!("Borrow reserve lending market does not match the lending market provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    if &borrow_reserve.liquidity.supply_pubkey != source_liquidity_info.key {
        msg!("Borrow reserve liquidity supply must be used as the source liquidity provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    if &borrow_reserve.liquidity.supply_pubkey == destination_liquidity_info.key {
        msg!(
            "Borrow reserve liquidity supply cannot be used as the destination liquidity provided"
        );
        return Err(LendingError::InvalidAccountInput.into());
    }
    if &borrow_reserve.liquidity.fee_receiver != borrow_reserve_liquidity_fee_receiver_info.key {
        msg!("Borrow reserve liquidity fee receiver does not match the borrow reserve liquidity fee receiver provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    let clock = Clock::get()?;
    if borrow_reserve.last_update.is_stale(clock.slot)? {
        msg!("Borrow reserve is stale and must be refreshed in the current slot");
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
    if &obligation.owner != obligation_owner_info.key {
        msg!("Obligation owner does not match the obligation owner provided");
        return Err(LendingError::InvalidObligationOwner.into());
    }
    if !obligation_owner_info.is_signer {
        msg!("Obligation owner provided must be a signer");
        return Err(LendingError::InvalidSigner.into());
    }
    if obligation.last_update.is_stale(clock.slot)? {
        msg!("Obligation is stale and must be refreshed in the current slot");
        return Err(LendingError::ObligationStale.into());
    }
    if obligation.deposits.is_empty() {
        msg!("Obligation has no deposits to borrow against");
        return Err(LendingError::ObligationDepositsEmpty.into());
    }
    if obligation.deposited_value == Decimal::zero() {
        msg!("Obligation deposits have zero value");
        return Err(LendingError::ObligationDepositsZero.into());
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

    let remaining_borrow_value = obligation.remaining_borrow_value()?;
    if remaining_borrow_value == Decimal::zero() {
        msg!("Remaining borrow value is zero");
        return Err(LendingError::BorrowTooLarge.into());
    }

    let CalculateBorrowResult {
        borrow_amount,
        receive_amount,
        borrow_fee,
        host_fee,
    } = borrow_reserve.calculate_borrow(liquidity_amount, remaining_borrow_value)?;

    if receive_amount == 0 {
        msg!("Borrow amount is too small to receive liquidity after fees");
        return Err(LendingError::BorrowTooSmall.into());
    }

    if liquidity_amount == u64::MAX && receive_amount < slippage_limit {
        msg!("Received liquidity would be smaller than the desired slippage limit");
        return Err(LendingError::ExceededSlippage.into());
    }

    borrow_reserve.liquidity.borrow(borrow_amount)?;
    borrow_reserve.last_update.mark_stale();
    let current_cumulative_borrow_rate_wads = borrow_reserve.liquidity.cumulative_borrow_rate_wads;
    Reserve::pack(borrow_reserve, &mut borrow_reserve_info.data.borrow_mut())?;

    obligation
        .find_or_add_liquidity_to_borrows(
            *borrow_reserve_info.key,
            current_cumulative_borrow_rate_wads,
        )?
        .borrow(borrow_amount)?;

    obligation.last_update.mark_stale();
    Obligation::pack(obligation, &mut obligation_info.data.borrow_mut())?;

    let mut owner_fee = borrow_fee;
    if let Ok(host_fee_receiver_info) = next_account_info(account_info_iter) {
        if host_fee > 0 {
            owner_fee = owner_fee
                .checked_sub(host_fee)
                .ok_or(LendingError::MathOverflow)?;

            spl_token_transfer(TokenTransferParams {
                source: source_liquidity_info.clone(),
                destination: host_fee_receiver_info.clone(),
                amount: host_fee,
                authority: lending_market_authority_info.clone(),
                authority_signer_seeds,
                token_program: token_program_id.clone(),
            })?;
        }
    }
    if owner_fee > 0 {
        spl_token_transfer(TokenTransferParams {
            source: source_liquidity_info.clone(),
            destination: borrow_reserve_liquidity_fee_receiver_info.clone(),
            amount: owner_fee,
            authority: lending_market_authority_info.clone(),
            authority_signer_seeds,
            token_program: token_program_id.clone(),
        })?;
    }

    spl_token_transfer(TokenTransferParams {
        source: source_liquidity_info.clone(),
        destination: destination_liquidity_info.clone(),
        amount: receive_amount,
        authority: lending_market_authority_info.clone(),
        authority_signer_seeds,
        token_program: token_program_id.clone(),
    })?;

    Ok(())
}
