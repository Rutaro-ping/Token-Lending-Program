use {
    crate::{
        error::LendingError,
        state::{LendingMarket, Reserve, ReserveConfig},
    },
    solana_program::{
        account_info::{next_account_info, AccountInfo},
        entrypoint::ProgramResult,
        msg,
        program_pack::Pack,
        pubkey::Pubkey,
    },
};

pub(super) fn process_modify_reserve_config(
    program_id: &Pubkey,
    new_config: ReserveConfig,
    accounts: &[AccountInfo],
) -> ProgramResult {
    new_config.validate()?;

    let account_info_iter = &mut accounts.iter().peekable();
    let reserve_info = next_account_info(account_info_iter)?;
    let lending_market_info = next_account_info(account_info_iter)?;
    let lending_market_owner_info = next_account_info(account_info_iter)?;

    if reserve_info.owner != program_id {
        msg!("Reserve provided is not owned by the lending program");
        return Err(LendingError::InvalidAccountOwner.into());
    }

    let lending_market = LendingMarket::unpack(&lending_market_info.data.borrow())?;
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

    let mut reserve = Reserve::unpack(&reserve_info.data.borrow_mut())?;
    // Validate that the reserve account corresponds to the correct lending market,
    // after validating above that the lending market and lending market owner
    // correspond, to prevent one compromised lending market owner from changing
    // configs on other lending markets
    if reserve.lending_market != *lending_market_info.key {
        msg!("Reserve account does not match the lending market");
        return Err(LendingError::InvalidAccountInput.into());
    }

    reserve.config = new_config;

    Reserve::pack(reserve, &mut reserve_info.data.borrow_mut())?;

    Ok(())
}
