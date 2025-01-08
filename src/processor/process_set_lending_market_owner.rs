use {
    crate::{error::LendingError, state::LendingMarket},
    solana_program::{
        account_info::{next_account_info, AccountInfo},
        entrypoint::ProgramResult,
        msg,
        program_pack::Pack,
        pubkey::Pubkey,
    },
};
#[inline(never)] // avoid stack frame limit
pub(super) fn process_set_lending_market_owner(
    program_id: &Pubkey,
    new_owner: Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let lending_market_info = next_account_info(account_info_iter)?;
    let lending_market_owner_info = next_account_info(account_info_iter)?;

    let mut lending_market = LendingMarket::unpack(&lending_market_info.data.borrow())?;
    if lending_market_info.owner != program_id {
        msg!("Lending market provided is not owned by the lending program");
        return Err(LendingError::InvalidAccountOwner.into());
    }
    if &lending_market.owner != lending_market_owner_info.key {
        msg!("Lending market owner does not match the lending market owner provided");
        return Err(LendingError::InvalidMarketOwner.into());
    }
    if !lending_market_owner_info.is_signer {
        msg!("Lending market owner provided must be a signer");
        return Err(LendingError::InvalidSigner.into());
    }
    lending_market.owner = new_owner;
    LendingMarket::pack(lending_market, &mut lending_market_info.data.borrow_mut())?;
    Ok(())
}
