use {
    crate::{
        error::LendingError,
        math::{Decimal, Rate, TryAdd, TryDiv, TryMul},
        state::{Obligation, Reserve},
        utils::get_pow,
        // utils::get_pow,
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

#[inline(never)]
pub(super) fn process_refresh_obligation(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter().peekable();
    let obligation_info = next_account_info(account_info_iter)?;
    let clock = &Clock::from_account_info(next_account_info(account_info_iter)?)?;
    let mut obligation = Obligation::unpack(&obligation_info.data.borrow())?;
    if obligation_info.owner != program_id {
        msg!("Obligation provided is not owned by the lending program");
        return Err(LendingError::InvalidAccountOwner.into());
    }
    let mut deposited_value = Decimal::zero();
    let mut borrowed_value = Decimal::zero();
    let mut allowed_borrow_value = Decimal::zero();
    let mut unhealthy_borrow_value = Decimal::zero();
    for (index, collateral) in obligation.deposits.iter_mut().enumerate() {
        let deposit_reserve_info = next_account_info(account_info_iter)?;
        if deposit_reserve_info.owner != program_id {
            msg!(
                "Deposit reserve provided for collateral {} is not owned by the lending program",
                index
            );
            return Err(LendingError::InvalidAccountOwner.into());
        }
        if collateral.deposit_reserve != *deposit_reserve_info.key {
            msg!(
                "Deposit reserve of collateral {} does not match the deposit reserve provided",
                index
            );
            return Err(LendingError::InvalidAccountInput.into());
        }
        let deposit_reserve = Reserve::unpack(&deposit_reserve_info.data.borrow())?;
        if deposit_reserve.last_update.is_stale(clock.slot)? {
            msg!(
              "Deposit reserve provided for collateral {} is stale and must be refreshed in the current slot",
              index
          );
            return Err(LendingError::ReserveStale.into());
        }

        let decimals = get_pow(deposit_reserve.liquidity.mint_decimals as u32)?;
        let market_value = deposit_reserve
            .collateral_exchange_rate()?
            .decimal_collateral_to_liquidity(collateral.deposited_amount.into())?
            .try_mul(deposit_reserve.liquidity.market_price)?
            .try_div(decimals)?;
        collateral.market_value = market_value;

        let loan_to_value_rate = Rate::from_percent(deposit_reserve.config.loan_to_value_ratio);
        let liquidation_threshold_rate =
            Rate::from_percent(deposit_reserve.config.liquidation_threshold);
        deposited_value = deposited_value.try_add(market_value)?;
        allowed_borrow_value =
            allowed_borrow_value.try_add(market_value.try_mul(loan_to_value_rate)?)?;
        unhealthy_borrow_value =
            unhealthy_borrow_value.try_add(market_value.try_mul(liquidation_threshold_rate)?)?;
    }
    for (index, liquidity) in obligation.borrows.iter_mut().enumerate() {
        let borrow_reserve_info = next_account_info(account_info_iter)?;
        if borrow_reserve_info.owner != program_id {
            msg!(
                "Borrow reserve provided for liquidity {} is not owned by the lending program",
                index
            );
            return Err(LendingError::InvalidAccountOwner.into());
        }
        if liquidity.borrow_reserve != *borrow_reserve_info.key {
            msg!(
                "Borrow reserve of liquidity {} does not match the borrow reserve provided",
                index
            );
            return Err(LendingError::InvalidAccountInput.into());
        }

        let borrow_reserve = Reserve::unpack(&borrow_reserve_info.data.borrow())?;
        if borrow_reserve.last_update.is_stale(clock.slot)? {
            msg!(
              "Borrow reserve provided for liquidity {} is stale and must be refreshed in the current slot",
              index
          );
            return Err(LendingError::ReserveStale.into());
        }

        liquidity.accrue_interest(borrow_reserve.liquidity.cumulative_borrow_rate_wads)?;

        let decimals = get_pow(borrow_reserve.liquidity.mint_decimals as u32)?;
        let market_value = liquidity
            .borrowed_amount_wads
            .try_mul(borrow_reserve.liquidity.market_price)?
            .try_div(decimals)?;
        liquidity.market_value = market_value;

        borrowed_value = borrowed_value.try_add(market_value)?;
    }

    if account_info_iter.peek().is_some() {
        msg!("Too many obligation deposit or borrow reserves provided");
        return Err(LendingError::InvalidAccountInput.into());
    }

    obligation.deposited_value = deposited_value;
    obligation.borrowed_value = borrowed_value;
    obligation.allowed_borrow_value = allowed_borrow_value;
    obligation.unhealthy_borrow_value = unhealthy_borrow_value;

    obligation.last_update.update_slot(clock.slot);
    Obligation::pack(obligation, &mut obligation_info.data.borrow_mut())?;
    Ok(())
}
