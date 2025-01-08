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
pub(super) fn process_deposit_obligation_collateral(
    program_id: &Pubkey,
    collateral_amount: u64,
    accounts: &[AccountInfo],
) -> ProgramResult {
    if collateral_amount == 0 {
        msg!("Collateral amount provided cannot be zero");
        return Err(LendingError::InvalidAmount.into());
    }
    let account_info_iter = &mut accounts.iter();
    let source_collateral_info = next_account_info(account_info_iter)?;
    let destination_collateral_info = next_account_info(account_info_iter)?;
    let deposit_reserve_info = next_account_info(account_info_iter)?;
    let obligation_info = next_account_info(account_info_iter)?;
    let lending_market_info = next_account_info(account_info_iter)?;
    let obligation_owner_info = next_account_info(account_info_iter)?;
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

    let deposit_reserve = Reserve::unpack(&deposit_reserve_info.data.borrow())?;
    if deposit_reserve_info.owner != program_id {
        msg!("Deposit reserve provided is not owned by the lending program");
        return Err(LendingError::InvalidAccountOwner.into());
    }
    if &deposit_reserve.lending_market != lending_market_info.key {
        msg!("Deposit reserve lending market does not match the lending market provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    if &deposit_reserve.collateral.supply_pubkey == source_collateral_info.key {
        msg!("Deposit reserve collateral supply cannot be used as the source collateral provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    if &deposit_reserve.collateral.supply_pubkey != destination_collateral_info.key {
        msg!(
            "Deposit reserve collateral supply must be used as the destination collateral provided"
        );
        return Err(LendingError::InvalidAccountInput.into());
    }
    if deposit_reserve.last_update.is_stale(clock.slot)? {
        msg!("Deposit reserve is stale and must be refreshed in the current slot");
        return Err(LendingError::ReserveStale.into());
    }
    if deposit_reserve.config.loan_to_value_ratio == 0 {
        msg!("Deposit reserve has collateral disabled for borrowing");
        return Err(LendingError::ReserveCollateralDisabled.into());
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
    obligation
        .find_or_add_collateral_to_deposits(*deposit_reserve_info.key)?
        .deposit(collateral_amount)?;
    obligation.last_update.mark_stale();
    Obligation::pack(obligation, &mut obligation_info.data.borrow_mut())?;

    // spl_token_transfer(TokenTransferParams {
    //     source: source_collateral_info.clone(),
    //     destination: destination_collateral_info.clone(),
    //     amount: collateral_amount,
    //     authority: user_transfer_authority_info.clone(),
    //     authority_signer_seeds: &[],
    //     token_program: token_program_id.clone(),
    // })?;
    //todo
    Ok(())
}
