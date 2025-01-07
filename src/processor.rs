use {
    crate::{
        error::LendingError,
        instruction::LendingInstruction,
        math::{Decimal, Rate, TryAdd, TryDiv, TryMul},
        pyth,
        state::{
            CalculateBorrowResult, InitLendingMarketParams, InitObligationParams,
            InitReserveParams, LendingMarket, NewReserveCollateralParams,
            NewReserveLiquidityParams, Obligation, Reserve, ReserveCollateral, ReserveConfig,
            ReserveLiquidity,
        },
    },
    num_traits::FromPrimitive,
    solana_program::{
        account_info::{next_account_info, AccountInfo},
        clock::Clock,
        decode_error::DecodeError,
        entrypoint::ProgramResult,
        instruction::Instruction,
        msg,
        program::{invoke, invoke_signed},
        program_error::{PrintProgramError, ProgramError},
        program_pack::{IsInitialized, Pack},
        pubkey::Pubkey,
        rent::Rent,
        sysvar::Sysvar,
    },
    spl_token::state::Mint,
};

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    input: &[u8],
) -> ProgramResult {
    let instruction = LendingInstruction::unpack(input)?;
    match instruction {
        LendingInstruction::InitLendingMarket {
            owner,
            quote_currency,
        } => {
            msg!("Instruction: Init Lending Market");
            process_init_lending_market(program_id, owner, quote_currency, accounts)
        }
        LendingInstruction::SetLendingMarketOwner { new_owner } => {
            msg!("Instruction: Set Lending Market Owner");
            process_set_lending_market_owner(program_id, new_owner, accounts)
        }
        LendingInstruction::DepositReserveLiquidity { liquidity_amount } => {
            msg!("Instruction: Deposit Reserve Liquidity");
            process_deposit_reserve_liquidity(program_id, liquidity_amount, accounts)
        }
        LendingInstruction::InitReserve {
            liquidity_amount,
            config,
        } => {
            msg!("Instruction: Init Reserve");
            process_init_reserve(program_id, liquidity_amount, config, accounts)
        }
        LendingInstruction::RefreshReserve => {
            msg!("Instruction: Refresh Reserve");
            process_refresh_reserve(program_id, accounts)
        }

        LendingInstruction::InitObligation => {
            msg!("Instruction: Init Obligation");
            process_init_obligation(program_id, accounts)
        }
        LendingInstruction::DepositObligationCollateral { collateral_amount } => {
            msg!("Instruction: Deposit Obligation Collateral");
            process_deposit_obligation_collateral(program_id, collateral_amount, accounts)
        }
        LendingInstruction::WithdrawObligationCollateral { collateral_amount } => {
            msg!("Instruction: Withdraw Obligation Collateral");
            process_withdraw_obligation_collateral(program_id, collateral_amount, accounts)
        }
        LendingInstruction::RefreshObligation => {
            msg!("Instruction: Refresh Obligation");
            process_refresh_obligation(program_id, accounts)
        }
        LendingInstruction::BorrowObligationLiquidity {
            liquidity_amount,
            slippage_limit,
        } => {
            msg!("Instruction: Borrow Obligation Liquidity");
            process_borrow_obligation_liquidity(
                program_id,
                liquidity_amount,
                slippage_limit,
                accounts,
            )
        }
        LendingInstruction::RepayObligationLiquidity { liquidity_amount } => {
            msg!("Instruction: Repay Obligation Liquidity");
            process_repay_obligation_liquidity(program_id, liquidity_amount, accounts)
        }
        _ => {
            msg!("Unsupported instruction");
            Err(LendingError::NotRentExempt.into())
        }
    }
}

fn process_refresh_reserve(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
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

fn process_init_lending_market(
    program_id: &Pubkey,
    owner: Pubkey,
    quote_currency: [u8; 32],
    accounts: &[AccountInfo],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let lending_market_info = next_account_info(account_info_iter)?;
    // next_account_info(account_info_iter)?; //fixme: clean up this account
    let rent = &Rent::from_account_info(next_account_info(account_info_iter)?)?;
    let token_program_id = next_account_info(account_info_iter)?;
    let oracle_program_id = next_account_info(account_info_iter)?;
    assert_rent_exempt(rent, lending_market_info)?;

    let mut lending_market = assert_uninitialized::<LendingMarket>(lending_market_info)?;

    if lending_market_info.owner != program_id {
        msg!("Lending market provided is not owned by the lending program");
        return Err(LendingError::InvalidAccountOwner)?;
    }
    lending_market.init(InitLendingMarketParams {
        bump_seed: Pubkey::find_program_address(&[lending_market_info.key.as_ref()], program_id).1,
        owner,
        quote_currency,
        token_program_id: *token_program_id.key,
        oracle_program_id: *oracle_program_id.key,
    });
    LendingMarket::pack(lending_market, &mut lending_market_info.data.borrow_mut())?;
    Ok(())
}

fn process_deposit_reserve_liquidity(
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
    let reserve_info = next_account_info(account_info_iter)?;
    let reserve_liquidity_supply_info = next_account_info(account_info_iter)?;
    let reserve_collateral_mint_info = next_account_info(account_info_iter)?;
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
    let mut reserve = Reserve::unpack(&reserve_info.data.borrow())?;
    if reserve_info.owner != program_id {
        msg!("Reserve provided is not owned by the lending program");
        return Err(LendingError::InvalidAccountOwner.into());
    }
    if &reserve.lending_market != lending_market_info.key {
        msg!("Reserve lending market does not match the lending market provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    if &reserve.liquidity.supply_pubkey != reserve_liquidity_supply_info.key {
        msg!("Reserve liquidity supply does not match the reserve liquidity supply provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    if &reserve.collateral.mint_pubkey != reserve_collateral_mint_info.key {
        msg!("Reserve collateral mint does not match the reserve collateral mint provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    if &reserve.liquidity.supply_pubkey == source_liquidity_info.key {
        msg!("Reserve liquidity supply cannot be used as the source liquidity provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    if &reserve.collateral.supply_pubkey == destination_collateral_info.key {
        msg!("Reserve collateral supply cannot be used as the destination collateral provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    if reserve.last_update.is_stale(clock.slot)? {
        msg!("Reserve is stale and must be refreshed in the current slot");
        return Err(LendingError::ReserveStale.into());
    }
    let authority_signer_seeds = &[
        lending_market_info.key.as_ref(),
        &[lending_market.bump_seed],
    ];
    let lending_market_authority_pubkey =
        Pubkey::create_program_address(authority_signer_seeds, program_id)?;
    if &lending_market_authority_pubkey != lending_market_authority_info.key {
        msg!(
            "Derived lending market authority {} does not match the lending market authority provided {}",
            &lending_market_authority_pubkey.to_string(),
            &lending_market_authority_info.key.to_string(),
        );
        return Err(LendingError::InvalidMarketAuthority.into());
    }
    let collateral_amount = reserve.deposit_liquidity(liquidity_amount)?;
    reserve.last_update.mark_stale();
    Reserve::pack(reserve, &mut reserve_info.data.borrow_mut())?;

    spl_token_transfer(TokenTransferParams {
        source: source_liquidity_info.clone(),
        destination: reserve_liquidity_supply_info.clone(),
        amount: liquidity_amount,
        authority: user_transfer_authority_info.clone(),
        authority_signer_seeds: &[],
        token_program: token_program_id.clone(),
    })?;

    spl_token_mint_to(TokenMintToParams {
        mint: reserve_collateral_mint_info.clone(),
        destination: destination_collateral_info.clone(),
        amount: collateral_amount,
        authority: lending_market_authority_info.clone(),
        authority_signer_seeds,
        token_program: token_program_id.clone(),
    })?;
    Ok(())
}

fn process_refresh_obligation(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
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
        // @TODO: add lookup table https://git.io/JOCYq
        let decimals = 10u64
            .checked_pow(deposit_reserve.liquidity.mint_decimals as u32)
            .ok_or(LendingError::MathOverflow)?;

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

        // @TODO: add lookup table https://git.io/JOCYq
        let decimals = 10u64
            .checked_pow(borrow_reserve.liquidity.mint_decimals as u32)
            .ok_or(LendingError::MathOverflow)?;

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

#[inline(never)] // avoid stack frame limit
fn process_withdraw_obligation_collateral(
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
    let withdraw_reserve_info = next_account_info(account_info_iter)?;
    let obligation_info = next_account_info(account_info_iter)?;
    let lending_market_info = next_account_info(account_info_iter)?;
    let lending_market_authority_info = next_account_info(account_info_iter)?;
    let obligation_owner_info = next_account_info(account_info_iter)?;
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

    let withdraw_reserve = Reserve::unpack(&withdraw_reserve_info.data.borrow())?;
    if withdraw_reserve_info.owner != program_id {
        msg!("Withdraw reserve provided is not owned by the lending program");
        return Err(LendingError::InvalidAccountOwner.into());
    }
    if &withdraw_reserve.lending_market != lending_market_info.key {
        msg!("Withdraw reserve lending market does not match the lending market provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    if &withdraw_reserve.collateral.supply_pubkey != source_collateral_info.key {
        msg!("Withdraw reserve collateral supply must be used as the source collateral provided");
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

    let (collateral, collateral_index) =
        obligation.find_collateral_in_deposits(*withdraw_reserve_info.key)?;
    if collateral.deposited_amount == 0 {
        msg!("Collateral deposited amount is zero");
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

    let withdraw_amount = if obligation.borrows.is_empty() {
        if collateral_amount == u64::MAX {
            collateral.deposited_amount
        } else {
            collateral.deposited_amount.min(collateral_amount)
        }
    } else if obligation.deposited_value == Decimal::zero() {
        msg!("Obligation deposited value is zero");
        return Err(LendingError::ObligationDepositsZero.into());
    } else {
        let max_withdraw_value = obligation.max_withdraw_value(Rate::from_percent(
            withdraw_reserve.config.loan_to_value_ratio,
        ))?;
        if max_withdraw_value == Decimal::zero() {
            msg!("Maximum withdraw value is zero");
            return Err(LendingError::WithdrawTooLarge.into());
        }

        let withdraw_amount = if collateral_amount == u64::MAX {
            let withdraw_value = max_withdraw_value.min(collateral.market_value);
            let withdraw_pct = withdraw_value.try_div(collateral.market_value)?;
            withdraw_pct
                .try_mul(collateral.deposited_amount)?
                .try_floor_u64()?
                .min(collateral.deposited_amount)
        } else {
            let withdraw_amount = collateral_amount.min(collateral.deposited_amount);
            let withdraw_pct =
                Decimal::from(withdraw_amount).try_div(collateral.deposited_amount)?;
            let withdraw_value = collateral.market_value.try_mul(withdraw_pct)?;
            if withdraw_value > max_withdraw_value {
                msg!("Withdraw value cannot exceed maximum withdraw value");
                return Err(LendingError::WithdrawTooLarge.into());
            }
            withdraw_amount
        };
        if withdraw_amount == 0 {
            msg!("Withdraw amount is too small to transfer collateral");
            return Err(LendingError::WithdrawTooSmall.into());
        }
        withdraw_amount
    };

    obligation.withdraw(withdraw_amount, collateral_index)?;
    obligation.last_update.mark_stale();
    Obligation::pack(obligation, &mut obligation_info.data.borrow_mut())?;

    spl_token_transfer(TokenTransferParams {
        source: source_collateral_info.clone(),
        destination: destination_collateral_info.clone(),
        amount: withdraw_amount,
        authority: lending_market_authority_info.clone(),
        authority_signer_seeds,
        token_program: token_program_id.clone(),
    })?;

    Ok(())
}

fn process_init_reserve(
    program_id: &Pubkey,
    liquidity_amount: u64,
    config: ReserveConfig,
    accounts: &[AccountInfo],
) -> ProgramResult {
    if liquidity_amount == 0 {
        msg!("Reserve must be initialized with liquidity");
        return Err(LendingError::InvalidAmount.into());
    }
    config.validate()?;
    let account_info_iter = &mut accounts.iter().peekable();
    let source_liquidity_info = next_account_info(account_info_iter)?;
    let destination_collateral_info = next_account_info(account_info_iter)?;
    let reserve_info = next_account_info(account_info_iter)?;
    let reserve_liquidity_mint_info = next_account_info(account_info_iter)?;
    let reserve_liquidity_supply_info = next_account_info(account_info_iter)?;
    let reserve_liquidity_fee_receiver_info = next_account_info(account_info_iter)?;
    let reserve_collateral_mint_info = next_account_info(account_info_iter)?;
    let reserve_collateral_supply_info = next_account_info(account_info_iter)?;
    let pyth_product_info = next_account_info(account_info_iter)?;
    let pyth_price_info = next_account_info(account_info_iter)?;
    let lending_market_info = next_account_info(account_info_iter)?;
    let lending_market_authority_info = next_account_info(account_info_iter)?;
    let lending_market_owner_info = next_account_info(account_info_iter)?;
    let user_transfer_authority_info = next_account_info(account_info_iter)?;
    let clock = &Clock::from_account_info(next_account_info(account_info_iter)?)?;
    let rent_info = next_account_info(account_info_iter)?;
    let rent = &Rent::from_account_info(rent_info)?;
    let token_program_id = next_account_info(account_info_iter)?;
    assert_rent_exempt(rent, reserve_info)?;
    let mut reserve = assert_uninitialized::<Reserve>(reserve_info)?;
    if reserve_info.owner != program_id {
        msg!("Reserve provided is not owned by the lending program");
        return Err(LendingError::InvalidAccountOwner.into());
    }
    if reserve_liquidity_supply_info.key == source_liquidity_info.key {
        msg!("Reserve liquidity supply cannot be used as the source liquidity provided");
        return Err(LendingError::InvalidAccountInput.into());
    }
    let lending_market = LendingMarket::unpack(&lending_market_info.data.borrow())?;
    if lending_market_info.owner != program_id {
        msg!("Lending market provided is not owned by the lending program");
        return Err(LendingError::InvalidAccountOwner.into());
    }
    if &lending_market.token_program_id != token_program_id.key {
        msg!("Lending market token program does not match the token program provided");
        return Err(LendingError::InvalidTokenProgram.into());
    }
    if &lending_market.owner != lending_market_owner_info.key {
        msg!("Lending market owner does not match the lending market owner provided");
        return Err(LendingError::InvalidMarketOwner.into());
    }
    if !lending_market_owner_info.is_signer {
        msg!("Lending market owner provided must be a signer");
        return Err(LendingError::InvalidSigner.into());
    }

    if &lending_market.oracle_program_id != pyth_product_info.owner {
        msg!("Pyth product account provided is not owned by the lending market oracle program");
        return Err(LendingError::InvalidOracleConfig.into());
    }
    if &lending_market.oracle_program_id != pyth_price_info.owner {
        msg!("Pyth price account provided is not owned by the lending market oracle program");
        return Err(LendingError::InvalidOracleConfig.into());
    }
    let pyth_product_data = pyth_product_info.try_borrow_data()?;
    let pyth_product = pyth::load::<pyth::Product>(&pyth_product_data)
        .map_err(|_| ProgramError::InvalidAccountData)?;
    if pyth_product.magic != pyth::MAGIC {
        msg!("Pyth product account provided is not a valid Pyth account");
        return Err(LendingError::InvalidOracleConfig.into());
    }
    if pyth_product.ver != pyth::VERSION_2 {
        msg!("Pyth product account provided has a different version than expected");
        return Err(LendingError::InvalidOracleConfig.into());
    }
    if pyth_product.atype != pyth::AccountType::Product as u32 {
        msg!("Pyth product account provided is not a valid Pyth product account");
        return Err(LendingError::InvalidOracleConfig.into());
    }
    let pyth_price_pubkey_bytes: &[u8; 32] = pyth_price_info
        .key
        .as_ref()
        .try_into()
        .map_err(|_| LendingError::InvalidAccountInput)?;
    if &pyth_product.px_acc.val != pyth_price_pubkey_bytes {
        msg!("Pyth product price account does not match the Pyth price provided");
        return Err(LendingError::InvalidOracleConfig.into());
    }
    let quote_currency = get_pyth_product_quote_currency(pyth_product)?;
    if lending_market.quote_currency != quote_currency {
        msg!("Lending market quote currency does not match the oracle quote currency");
        return Err(LendingError::InvalidOracleConfig.into());
    }

    let market_price = get_pyth_price(pyth_price_info, clock)?;
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

    let reserve_liquidity_mint = unpack_mint(&reserve_liquidity_mint_info.data.borrow())?;
    if reserve_liquidity_mint_info.owner != token_program_id.key {
        msg!("Reserve liquidity mint is not owned by the token program provided");
        return Err(LendingError::InvalidTokenOwner.into());
    }
    reserve.init(InitReserveParams {
        current_slot: clock.slot,
        lending_market: *lending_market_info.key,
        liquidity: ReserveLiquidity::new(NewReserveLiquidityParams {
            mint_pubkey: *reserve_liquidity_mint_info.key,
            mint_decimals: reserve_liquidity_mint.decimals,
            supply_pubkey: *reserve_liquidity_supply_info.key,
            fee_receiver: *reserve_liquidity_fee_receiver_info.key,
            oracle_pubkey: *pyth_price_info.key,
            market_price,
        }),
        collateral: ReserveCollateral::new(NewReserveCollateralParams {
            mint_pubkey: *reserve_collateral_mint_info.key,
            supply_pubkey: *reserve_collateral_supply_info.key,
        }),
        config,
    });
    let collateral_amount = reserve.deposit_liquidity(liquidity_amount)?;
    Reserve::pack(reserve, &mut reserve_info.data.borrow_mut())?;
    spl_token_init_account(TokenInitializeAccountParams {
        account: reserve_liquidity_supply_info.clone(),
        mint: reserve_liquidity_mint_info.clone(),
        owner: lending_market_authority_info.clone(),
        rent: rent_info.clone(),
        token_program: token_program_id.clone(),
    })?;

    spl_token_init_account(TokenInitializeAccountParams {
        account: reserve_liquidity_fee_receiver_info.clone(),
        mint: reserve_liquidity_mint_info.clone(),
        owner: lending_market_authority_info.clone(),
        rent: rent_info.clone(),
        token_program: token_program_id.clone(),
    })?;

    spl_token_init_mint(TokenInitializeMintParams {
        mint: reserve_collateral_mint_info.clone(),
        authority: lending_market_authority_info.key,
        rent: rent_info.clone(),
        decimals: reserve_liquidity_mint.decimals,
        token_program: token_program_id.clone(),
    })?;

    spl_token_init_account(TokenInitializeAccountParams {
        account: reserve_collateral_supply_info.clone(),
        mint: reserve_collateral_mint_info.clone(),
        owner: lending_market_authority_info.clone(),
        rent: rent_info.clone(),
        token_program: token_program_id.clone(),
    })?;

    spl_token_init_account(TokenInitializeAccountParams {
        account: destination_collateral_info.clone(),
        mint: reserve_collateral_mint_info.clone(),
        owner: user_transfer_authority_info.clone(),
        rent: rent_info.clone(),
        token_program: token_program_id.clone(),
    })?;

    spl_token_transfer(TokenTransferParams {
        source: source_liquidity_info.clone(),
        destination: reserve_liquidity_supply_info.clone(),
        amount: liquidity_amount,
        authority: user_transfer_authority_info.clone(),
        authority_signer_seeds: &[],
        token_program: token_program_id.clone(),
    })?;

    spl_token_mint_to(TokenMintToParams {
        mint: reserve_collateral_mint_info.clone(),
        destination: destination_collateral_info.clone(),
        amount: collateral_amount,
        authority: lending_market_authority_info.clone(),
        authority_signer_seeds,
        token_program: token_program_id.clone(),
    })?;
    Ok(())
}
#[inline(never)] // avoid stack frame limit
fn process_init_obligation(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let obligation_info = next_account_info(account_info_iter)?;
    let lending_market_info = next_account_info(account_info_iter)?;
    let obligation_owner_info = next_account_info(account_info_iter)?;
    let clock = &Clock::from_account_info(next_account_info(account_info_iter)?)?;
    let rent = &Rent::from_account_info(next_account_info(account_info_iter)?)?;
    let token_program_id = next_account_info(account_info_iter)?;
    assert_rent_exempt(rent, obligation_info)?;
    let mut obligation = assert_uninitialized::<Obligation>(obligation_info)?;
    let mut obligation = assert_uninitialized::<Obligation>(obligation_info)?;
    if obligation_info.owner != program_id {
        msg!("Obligation provided is not owned by the lending program");
        return Err(LendingError::InvalidAccountOwner.into());
    }

    let lending_market = LendingMarket::unpack(&lending_market_info.data.borrow())?;
    if lending_market_info.owner != program_id {
        msg!("Lending market provided is not owned by the lending program");
        return Err(LendingError::InvalidAccountOwner.into());
    }
    if &lending_market.token_program_id != token_program_id.key {
        msg!("Lending market token program does not match the token program provided");
        return Err(LendingError::InvalidTokenProgram.into());
    }

    if !obligation_owner_info.is_signer {
        msg!("Obligation owner provided must be a signer");
        return Err(LendingError::InvalidSigner.into());
    }
    obligation.init(InitObligationParams {
        current_slot: clock.slot,
        lending_market: *lending_market_info.key,
        owner: *obligation_owner_info.key,
        deposits: vec![],
        borrows: vec![],
    });
    Obligation::pack(obligation, &mut obligation_info.data.borrow_mut())?;
    Ok(())
}

fn assert_rent_exempt(rent: &Rent, account_info: &AccountInfo) -> ProgramResult {
    if !rent.is_exempt(account_info.lamports(), account_info.data_len()) {
        msg!(&rent.minimum_balance(account_info.data_len()).to_string());
        Err(LendingError::NotRentExempt.into())
    } else {
        Ok(())
    }
}

fn assert_uninitialized<T: IsInitialized + Pack>(
    account_info: &AccountInfo,
) -> Result<T, ProgramError> {
    let account = T::unpack_unchecked(&account_info.data.borrow())?;

    if account.is_initialized() {
        msg!("Account is already initialized");
        Err(LendingError::AlreadyInitialized.into())
    } else {
        Ok(account)
    }
}

#[inline(never)] // avoid stack frame limit
fn process_borrow_obligation_liquidity(
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
    Reserve::pack(borrow_reserve, &mut borrow_reserve_info.data.borrow_mut())?;

    obligation
        .find_or_add_liquidity_to_borrows(*borrow_reserve_info.key)?
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

#[inline(never)] // avoid stack frame limit
fn process_set_lending_market_owner(
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

impl PrintProgramError for LendingError {
    fn print<E>(&self)
    where
        E: 'static + std::error::Error + DecodeError<E> + PrintProgramError + FromPrimitive,
    {
        msg!(&self.to_string());
    }
}

fn get_pyth_product_quote_currency(pyth_product: &pyth::Product) -> Result<[u8; 32], ProgramError> {
    // 1 byte for key length
    // N bytes for key content
    // 1 byte for value length
    // M bytes for value content
    const LEN: usize = 14;
    const KEY: &[u8; LEN] = b"quote_currency";

    let mut start = 0;
    while start < pyth::PROD_ATTR_SIZE {
        let mut length = pyth_product.attr[start] as usize;
        start += 1;

        if length == LEN {
            let mut end = start + length;
            if end > pyth::PROD_ATTR_SIZE {
                msg!("Pyth product attribute key length too long");
                return Err(LendingError::InvalidOracleConfig.into());
            }

            let key = &pyth_product.attr[start..end];
            if key == KEY {
                start += length;
                length = pyth_product.attr[start] as usize;
                start += 1;

                end = start + length;
                if length > 32 || end > pyth::PROD_ATTR_SIZE {
                    msg!("Pyth product quote currency value too long");
                    return Err(LendingError::InvalidOracleConfig.into());
                }

                let mut value = [0u8; 32];
                value[0..length].copy_from_slice(&pyth_product.attr[start..end]);
                return Ok(value);
            }
        }

        start += length;
        start += 1 + pyth_product.attr[start] as usize;
    }

    msg!("Pyth product quote currency not found");
    Err(LendingError::InvalidOracleConfig.into())
}

fn get_pyth_price(pyth_price_info: &AccountInfo, clock: &Clock) -> Result<Decimal, ProgramError> {
    const STALE_AFTER_SLOTS_ELAPSED: u64 = 5;
    let pyth_price_data = pyth_price_info.try_borrow_data()?;
    let pyth_price = pyth::load::<pyth::Price>(&pyth_price_data)
        .map_err(|_| ProgramError::InvalidAccountData)?;

    if pyth_price.ptype != pyth::PriceType::Price {
        msg!("Oracle price type is invalid");
        return Err(LendingError::InvalidOracleConfig.into());
    }

    if pyth_price.agg.status != pyth::PriceStatus::Trading {
        msg!("Oracle price status is invalid");
        return Err(LendingError::InvalidOracleConfig.into());
    }

    let slots_elapsed = clock
        .slot
        .checked_sub(pyth_price.valid_slot)
        .ok_or(LendingError::MathOverflow)?;
    if slots_elapsed >= STALE_AFTER_SLOTS_ELAPSED {
        msg!("Oracle price is stale");
        return Err(LendingError::InvalidOracleConfig.into());
    }

    let price: u64 = pyth_price.agg.price.try_into().map_err(|_| {
        msg!("Oracle price cannot be negative");
        LendingError::InvalidOracleConfig
    })?;

    let market_price = if pyth_price.expo >= 0 {
        let exponent = pyth_price
            .expo
            .try_into()
            .map_err(|_| LendingError::MathOverflow)?;
        let zeros = 10u64
            .checked_pow(exponent)
            .ok_or(LendingError::MathOverflow)?;
        Decimal::from(price).try_mul(zeros)?
    } else {
        let exponent = pyth_price
            .expo
            .checked_abs()
            .ok_or(LendingError::MathOverflow)?
            .try_into()
            .map_err(|_| LendingError::MathOverflow)?;
        let decimals = 10u64
            .checked_pow(exponent)
            .ok_or(LendingError::MathOverflow)?;
        Decimal::from(price).try_div(decimals)?
    };

    Ok(market_price)
}

#[inline(always)]
fn spl_token_init_account(params: TokenInitializeAccountParams<'_>) -> ProgramResult {
    let TokenInitializeAccountParams {
        account,
        mint,
        owner,
        rent,
        token_program,
    } = params;
    let ix = spl_token::instruction::initialize_account(
        token_program.key,
        account.key,
        mint.key,
        owner.key,
    )?;
    invoke(&ix, &[account, mint, owner, rent, token_program])
        .map_err(|_| LendingError::TokenInitializeAccountFailed.into())
}
#[inline(always)]
fn spl_token_init_mint(params: TokenInitializeMintParams<'_, '_>) -> ProgramResult {
    let TokenInitializeMintParams {
        mint,
        rent,
        authority,
        token_program,
        decimals,
    } = params;
    let ix = spl_token::instruction::initialize_mint(
        token_program.key,
        mint.key,
        authority,
        None,
        decimals,
    )?;
    let result = invoke(&ix, &[mint, rent, token_program]);
    result.map_err(|_| LendingError::TokenInitializeMintFailed.into())
}

/// Issue a spl_token `Transfer` instruction.
#[inline(always)]
fn spl_token_transfer(params: TokenTransferParams<'_, '_>) -> ProgramResult {
    let TokenTransferParams {
        source,
        destination,
        authority,
        token_program,
        amount,
        authority_signer_seeds,
    } = params;
    let result = invoke_optionally_signed(
        &spl_token::instruction::transfer(
            token_program.key,
            source.key,
            destination.key,
            authority.key,
            &[],
            amount,
        )?,
        &[source, destination, authority, token_program],
        authority_signer_seeds,
    );
    result.map_err(|_| LendingError::TokenTransferFailed.into())
}

/// Issue a spl_token `MintTo` instruction.
fn spl_token_mint_to(params: TokenMintToParams<'_, '_>) -> ProgramResult {
    let TokenMintToParams {
        mint,
        destination,
        authority,
        token_program,
        amount,
        authority_signer_seeds,
    } = params;
    let result = invoke_optionally_signed(
        &spl_token::instruction::mint_to(
            token_program.key,
            mint.key,
            destination.key,
            authority.key,
            &[],
            amount,
        )?,
        &[mint, destination, authority, token_program],
        authority_signer_seeds,
    );
    result.map_err(|_| LendingError::TokenMintToFailed.into())
}

/// Issue a spl_token `Burn` instruction.
#[inline(always)]
fn spl_token_burn(params: TokenBurnParams<'_, '_>) -> ProgramResult {
    let TokenBurnParams {
        mint,
        source,
        authority,
        token_program,
        amount,
        authority_signer_seeds,
    } = params;
    let result = invoke_optionally_signed(
        &spl_token::instruction::burn(
            token_program.key,
            source.key,
            mint.key,
            authority.key,
            &[],
            amount,
        )?,
        &[source, mint, authority, token_program],
        authority_signer_seeds,
    );
    result.map_err(|_| LendingError::TokenBurnFailed.into())
}

struct TokenInitializeAccountParams<'a> {
    account: AccountInfo<'a>,
    mint: AccountInfo<'a>,
    owner: AccountInfo<'a>,
    rent: AccountInfo<'a>,
    token_program: AccountInfo<'a>,
}

struct TokenInitializeMintParams<'a: 'b, 'b> {
    mint: AccountInfo<'a>,
    rent: AccountInfo<'a>,
    authority: &'b Pubkey,
    decimals: u8,
    token_program: AccountInfo<'a>,
}

struct TokenMintToParams<'a: 'b, 'b> {
    mint: AccountInfo<'a>,
    destination: AccountInfo<'a>,
    amount: u64,
    authority: AccountInfo<'a>,
    authority_signer_seeds: &'b [&'b [u8]],
    token_program: AccountInfo<'a>,
}

struct TokenTransferParams<'a: 'b, 'b> {
    source: AccountInfo<'a>,
    destination: AccountInfo<'a>,
    amount: u64,
    authority: AccountInfo<'a>,
    authority_signer_seeds: &'b [&'b [u8]],
    token_program: AccountInfo<'a>,
}

struct TokenBurnParams<'a: 'b, 'b> {
    mint: AccountInfo<'a>,
    source: AccountInfo<'a>,
    amount: u64,
    authority: AccountInfo<'a>,
    authority_signer_seeds: &'b [&'b [u8]],
    token_program: AccountInfo<'a>,
}

#[inline(always)]
fn invoke_optionally_signed(
    ix: &Instruction,
    accounts: &[AccountInfo],
    authority_signer_seeds: &[&[u8]],
) -> ProgramResult {
    if authority_signer_seeds.is_empty() {
        invoke(ix, accounts)
    } else {
        invoke_signed(ix, accounts, &[authority_signer_seeds])
    }
}

/// Unpacks a spl_token `Mint`.
fn unpack_mint(data: &[u8]) -> Result<Mint, LendingError> {
    Mint::unpack(data).map_err(|_| LendingError::InvalidTokenMint)
}

#[inline(never)] // avoid stack frame limit
fn process_repay_obligation_liquidity(
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

    let mut repay_reserve = Reserve::unpack(&repay_reserve_info.data.borrow())?;
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
    Ok(())
}

#[inline(never)] // avoid stack frame limit
fn process_deposit_obligation_collateral(
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

    spl_token_transfer(TokenTransferParams {
        source: source_collateral_info.clone(),
        destination: destination_collateral_info.clone(),
        amount: collateral_amount,
        authority: user_transfer_authority_info.clone(),
        authority_signer_seeds: &[],
        token_program: token_program_id.clone(),
    })?;

    Ok(())
}
