use {
    super::{
        assert_rent_exempt, assert_uninitialized, get_pyth_price, get_pyth_product_quote_currency,
        spl_token_init_account, spl_token_init_mint, spl_token_mint_to, spl_token_transfer,
        unpack_mint, TokenInitializeAccountParams, TokenInitializeMintParams, TokenMintToParams,
        TokenTransferParams,
    },
    crate::{
        error::LendingError,
        pyth,
        state::{
            InitReserveParams, LendingMarket, NewReserveCollateralParams,
            NewReserveLiquidityParams, Reserve, ReserveCollateral, ReserveConfig, ReserveLiquidity,
        },
    },
    solana_program::{
        account_info::{next_account_info, AccountInfo},
        clock::Clock,
        entrypoint::ProgramResult,
        msg,
        program_error::ProgramError,
        program_pack::Pack,
        pubkey::Pubkey,
        rent::Rent,
        sysvar::Sysvar,
    },
};

pub(super) fn process_init_reserve(
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
