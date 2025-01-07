#![allow(clippy::arithmetic_side_effects)]
#![cfg(feature = "test-sbf")]

mod helpers;
use {
    helpers::*,
    solana_program_test::*,
    solana_sdk::{signature::Keypair, signer::Signer},
    spl_token_lending::{processor::process_instruction, state::INITIAL_COLLATERAL_RATIO},
};

#[tokio::test]
async fn test_success() {
    let mut test = ProgramTest::new(
        "spl_token_lending",
        spl_token_lending::id(),
        processor!(process_instruction),
    );
    // limit to track compute unit increase
    test.set_compute_max_units(170_000);
    const FEE_AMOUNT: u64 = 100;
    const HOST_FEE_AMOUNT: u64 = 20;

    const SOL_DEPOSIT_AMOUNT_LAMPORTS: u64 = 100 * LAMPORTS_TO_SOL * INITIAL_COLLATERAL_RATIO;
    const SOL_RESERVE_COLLATERAL_LAMPORTS: u64 = SOL_DEPOSIT_AMOUNT_LAMPORTS;

    const USDC_RESERVE_LIQUIDITY_FRACTIONAL: u64 = 1_000 * FRACTIONAL_TO_USDC;
    const USDC_BORROW_AMOUNT_FRACTIONAL: u64 = USDC_RESERVE_LIQUIDITY_FRACTIONAL - FEE_AMOUNT;
    const USDC_REPAY_AMOUNT_FRACTIONAL: u64 = USDC_RESERVE_LIQUIDITY_FRACTIONAL;

    let user_accounts_owner = Keypair::new();
    let user_accounts_owner_pubkey = user_accounts_owner.pubkey();

    let user_transfer_authority = Keypair::new();
    let user_transfer_authority_pubkey = user_transfer_authority.pubkey();
    let obligation_keypair = Keypair::new();
    let obligation_pubkey = obligation_keypair.pubkey();
    let lending_market = add_lending_market(&mut test);
    let mut reserve_config = TEST_RESERVE_CONFIG;
    reserve_config.loan_to_value_ratio = 50;
    let sol_oracle = add_sol_oracle(&mut test);
    let sol_test_reserve = add_reserve(
        &mut test,
        &lending_market,
        &sol_oracle,
        &user_accounts_owner,
        AddReserveArgs {
            user_liquidity_amount: SOL_RESERVE_COLLATERAL_LAMPORTS,
            liquidity_amount: SOL_RESERVE_COLLATERAL_LAMPORTS,
            liquidity_mint_pubkey: spl_token::native_mint::id(),
            liquidity_mint_decimals: 9,
            config: reserve_config,
            ..AddReserveArgs::default()
        },
    );
    println!("lending_market: {:#?}", lending_market);
    println!("================================================");
    println!("sol_test_reserve: {:#?}", sol_test_reserve);
}
