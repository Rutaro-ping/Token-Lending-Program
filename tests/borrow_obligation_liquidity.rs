#![allow(clippy::arithmetic_side_effects)]
#![cfg(feature = "test-sbf")]
mod helpers;
use {
    helpers::{
        add_lending_market, add_usdc_mint, add_usdc_oracle, FRACTIONAL_TO_USDC, LAMPORTS_TO_SOL,
        TEST_RESERVE_CONFIG,
    },
    solana_program_test::*,
    solana_sdk::signature::{Keypair, Signer},
    spl_token_lending::{processor::process_instruction, state::INITIAL_COLLATERAL_RATIO},
};

#[tokio::test]
async fn test_borrow_usdc_fixed_amount() {
    let mut test = ProgramTest::new(
        "spl_token_lending",
        spl_token_lending::id(),
        processor!(process_instruction),
    );
    // limit to track compute unit increase
    test.set_compute_max_units(60_000);

    const USDC_TOTAL_BORROW_FRACTIONAL: u64 = 1_000 * FRACTIONAL_TO_USDC;
    const FEE_AMOUNT: u64 = 100;
    const HOST_FEE_AMOUNT: u64 = 20;

    const SOL_DEPOSIT_AMOUNT_LAMPORTS: u64 = 100 * LAMPORTS_TO_SOL * INITIAL_COLLATERAL_RATIO;
    const USDC_BORROW_AMOUNT_FRACTIONAL: u64 = USDC_TOTAL_BORROW_FRACTIONAL - FEE_AMOUNT;
    const SOL_RESERVE_COLLATERAL_LAMPORTS: u64 = 2 * SOL_DEPOSIT_AMOUNT_LAMPORTS;
    const USDC_RESERVE_LIQUIDITY_FRACTIONAL: u64 = 2 * USDC_TOTAL_BORROW_FRACTIONAL;
    let user_accounts_owner = Keypair::new();
    let lending_market: helpers::TestLendingMarket = add_lending_market(&mut test);
    let mut reserve_config = TEST_RESERVE_CONFIG;
    reserve_config.loan_to_value_ratio = 50;
    let usdc_mint = add_usdc_mint(&mut test);
    let usdc_oracle = add_usdc_oracle(&mut test);
}
