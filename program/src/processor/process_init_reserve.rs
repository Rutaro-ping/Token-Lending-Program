use {
    super::{
        assert_rent_exempt, assert_uninitialized, get_pyth_price, get_pyth_product_quote_currency,
        spl_token_init_account, spl_token_init_mint, spl_token_mint_to, spl_token_transfer,
        unpack_mint, TokenInitializeAccountParams, TokenInitializeMintParams, TokenMintToParams,
        TokenTransferParams,
    },
    crate::{
        assert_equal, assert_is_signer, assert_key_equal, assert_key_not_equal,
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
    let token_program_id = next_account_info(account_info_iter)?;
    assert_rent_exempt(reserve_info)?;
    let mut reserve = assert_uninitialized::<Reserve>(reserve_info)?;
    assert_key_equal!(
        reserve_info.owner,
        program_id,
        "Reserve provided is not owned by the lending program",
        LendingError::InvalidAccountOwner
    );
    assert_key_not_equal!(
        reserve_liquidity_supply_info.key,
        source_liquidity_info.key,
        "Reserve liquidity supply cannot be used as the source liquidity provided",
        LendingError::InvalidAccountInput
    );
    let lending_market = LendingMarket::unpack(&lending_market_info.data.borrow())?;
    assert_key_equal!(
        lending_market_info.owner,
        program_id,
        "Lending market provided is not owned by the lending program",
        LendingError::InvalidAccountOwner
    );
    assert_key_equal!(
        &lending_market.token_program_id,
        token_program_id.key,
        "Lending market token program does not match the token program provided",
        LendingError::InvalidTokenProgram
    );
    assert_key_equal!(
        &lending_market.owner,
        lending_market_owner_info.key,
        "Lending market owner does not match the lending market owner provided",
        LendingError::InvalidMarketOwner
    );
    assert_is_signer!(lending_market_owner_info, "Lending market owner provided");
    assert_key_equal!(
        &lending_market.oracle_program_id,
        pyth_product_info.owner,
        "Pyth product account provided is not owned by the lending market oracle program",
        LendingError::InvalidOracleConfig
    );
    assert_key_equal!(
        &lending_market.oracle_program_id,
        pyth_price_info.owner,
        "Pyth price account provided is not owned by the lending market oracle program",
        LendingError::InvalidOracleConfig
    );
    let pyth_product_data = pyth_product_info.try_borrow_data()?;
    let pyth_product = pyth::load::<pyth::Product>(&pyth_product_data)
        .map_err(|_| ProgramError::InvalidAccountData)?;
    assert_equal!(
        pyth_product.magic,
        pyth::MAGIC,
        "Pyth product account provided is not a valid Pyth account",
        LendingError::InvalidOracleConfig
    );
    assert_equal!(
        pyth_product.ver,
        pyth::VERSION_2,
        "Pyth product account provided has a different version than expected",
        LendingError::InvalidOracleConfig
    );
    assert_equal!(
        pyth_product.atype,
        pyth::AccountType::Product as u32,
        "Pyth product account provided is not a valid Pyth product account",
        LendingError::InvalidOracleConfig
    );
    let pyth_price_pubkey_bytes: &[u8; 32] = pyth_price_info
        .key
        .as_ref()
        .try_into()
        .map_err(|_| LendingError::InvalidAccountInput)?;
    assert_equal!(
        &pyth_product.px_acc.val,
        pyth_price_pubkey_bytes,
        "Pyth product price account does not match the Pyth price provided",
        LendingError::InvalidOracleConfig
    );
    let quote_currency = get_pyth_product_quote_currency(pyth_product)?;
    assert_equal!(
        lending_market.quote_currency,
        quote_currency,
        "Lending market quote currency does not match the oracle quote currency",
        LendingError::InvalidOracleConfig
    );
    let clock = Clock::get()?;
    let market_price = get_pyth_price(pyth_price_info, &clock)?;
    let authority_signer_seeds = &[
        lending_market_info.key.as_ref(),
        &[lending_market.bump_seed],
    ];
    let lending_market_authority_pubkey =
        Pubkey::create_program_address(authority_signer_seeds, program_id)?;
    assert_key_equal!(
        &lending_market_authority_pubkey,
        lending_market_authority_info.key,
        "Derived lending market authority does not match the lending market authority provided",
        LendingError::InvalidMarketAuthority
    );

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
        token_program: token_program_id.clone(),
    })?;

    spl_token_init_account(TokenInitializeAccountParams {
        account: reserve_liquidity_fee_receiver_info.clone(),
        mint: reserve_liquidity_mint_info.clone(),
        owner: lending_market_authority_info.clone(),
        token_program: token_program_id.clone(),
    })?;

    spl_token_init_mint(TokenInitializeMintParams {
        mint: reserve_collateral_mint_info.clone(),
        authority: lending_market_authority_info.key,
        decimals: reserve_liquidity_mint.decimals,
        token_program: token_program_id.clone(),
    })?;

    spl_token_init_account(TokenInitializeAccountParams {
        account: reserve_collateral_supply_info.clone(),
        mint: reserve_collateral_mint_info.clone(),
        owner: lending_market_authority_info.clone(),
        token_program: token_program_id.clone(),
    })?;

    spl_token_init_account(TokenInitializeAccountParams {
        account: destination_collateral_info.clone(),
        mint: reserve_collateral_mint_info.clone(),
        owner: user_transfer_authority_info.clone(),
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
