use {
    super::get_pyth_price,
    crate::{error::LendingError, state::Reserve},
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
pub(super) fn process_refresh_reserve(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter().peekable();
    let reserve_info = next_account_info(account_info_iter)?;
    let reserve_liquidity_oracle_info = next_account_info(account_info_iter)?;
    let clock = &Clock::from_account_info(next_account_info(account_info_iter)?)?;
    let mut reserve = Reserve::unpack(&reserve_info.data.borrow())?;
    if reserve_info.owner != program_id {
        msg!("Reserve provided is not owned by the lending program");
        return Err(LendingError::InvalidAccountOwner.into());
    }
    if &reserve.liquidity.oracle_pubkey != reserve_liquidity_oracle_info.key {
        msg!("Reserve liquidity oracle does not match the reserve liquidity oracle provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    reserve.liquidity.market_price = get_pyth_price(reserve_liquidity_oracle_info, clock)?;
    reserve.accrue_interest(clock.slot)?;
    reserve.last_update.update_slot(clock.slot);

    Reserve::pack(reserve, &mut reserve_info.data.borrow_mut())?;

    Ok(())
}
