use {
    crate::{
        error::LendingError,
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
pub(super) fn process_repay_obligation_liquidity(
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
    let destination_liquidity_info = next_account_info(account_info_iter)?;
    let repay_reserve_info = next_account_info(account_info_iter)?;
    let obligation_info = next_account_info(account_info_iter)?;
    let lending_market_info = next_account_info(account_info_iter)?;
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

    let repay_reserve = Reserve::unpack(&repay_reserve_info.data.borrow())?;
    if repay_reserve_info.owner != program_id {
        msg!("Repay reserve provided is not owned by the lending program");
        return Err(LendingError::InvalidAccountOwner.into());
    }
    if &repay_reserve.lending_market != lending_market_info.key {
        msg!("Repay reserve lending market does not match the lending market provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    if &repay_reserve.liquidity.supply_pubkey == source_liquidity_info.key {
        msg!("Repay reserve liquidity supply cannot be used as the source liquidity provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    if &repay_reserve.liquidity.supply_pubkey != destination_liquidity_info.key {
        msg!("Repay reserve liquidity supply must be used as the destination liquidity provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    if repay_reserve.last_update.is_stale(clock.slot)? {
        msg!("Repay reserve is stale and must be refreshed in the current slot");
        return Err(LendingError::ReserveStale.into());
    }
    let obligation = Obligation::unpack(&obligation_info.data.borrow())?;
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
    let (liquidity, liquidity_index) =
        obligation.find_liquidity_in_borrows(*repay_reserve_info.key)?;
    Ok(())
}
