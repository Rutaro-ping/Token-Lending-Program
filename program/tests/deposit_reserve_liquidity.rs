#![allow(clippy::arithmetic_side_effects)]
#![cfg(feature = "test-sbf")]

mod helpers;
use {
    helpers::{
        add_lending_market, add_reserve, add_usdc_mint, add_usdc_oracle, get_token_balance,
        AddReserveArgs, FRACTIONAL_TO_USDC, TEST_RESERVE_CONFIG,
    },
    solana_program_test::*,
    solana_sdk::{msg, signature::Keypair},
    spl_token_lending::processor::process_instruction,
};

#[tokio::test]
async fn test_success() {
    let mut test = ProgramTest::new(
        "spl_token_lending",
        spl_token_lending::id(),
        processor!(process_instruction),
    );
    // limit to track compute unit increase
    test.set_compute_max_units(50_000);
    let user_accounts_owner = Keypair::new();
    let lending_market = add_lending_market(&mut test);
    let usdc_mint = add_usdc_mint(&mut test);
    let usdc_oracle = add_usdc_oracle(&mut test);
    let usdc_test_reserve = add_reserve(
        &mut test,
        &lending_market,
        &usdc_oracle,
        &user_accounts_owner,
        AddReserveArgs {
            user_liquidity_amount: 100 * FRACTIONAL_TO_USDC,
            liquidity_amount: 10_000 * FRACTIONAL_TO_USDC,
            liquidity_mint_decimals: usdc_mint.decimals,
            liquidity_mint_pubkey: usdc_mint.pubkey,
            config: TEST_RESERVE_CONFIG,
            mark_fresh: true,
            ..AddReserveArgs::default()
        },
    );

    let (mut banks_client, payer, _recent_blockhash) = test.start().await;
    let initial_user_liquidity_balance =
        get_token_balance(&mut banks_client, usdc_test_reserve.user_liquidity_pubkey).await;
    let initial_liquidity_supply_balance =
        get_token_balance(&mut banks_client, usdc_test_reserve.liquidity_supply_pubkey).await;
    let initial_user_collateral_balance =
        get_token_balance(&mut banks_client, usdc_test_reserve.user_collateral_pubkey).await;
    let initial_collateral_supply_balance = get_token_balance(
        &mut banks_client,
        usdc_test_reserve.collateral_supply_pubkey,
    )
    .await;
    let reserve_state_before = usdc_test_reserve.get_state(&mut banks_client).await;
    lending_market
        .deposit(
            &mut banks_client,
            &user_accounts_owner,
            &payer,
            &usdc_test_reserve,
            100 * FRACTIONAL_TO_USDC,
        )
        .await;
    let final_user_liquidity_balance =
        get_token_balance(&mut banks_client, usdc_test_reserve.user_liquidity_pubkey).await;
    let final_liquidity_supply_balance =
        get_token_balance(&mut banks_client, usdc_test_reserve.liquidity_supply_pubkey).await;
    let final_user_collateral_balance =
        get_token_balance(&mut banks_client, usdc_test_reserve.user_collateral_pubkey).await;
    let final_collateral_supply_balance = get_token_balance(
        &mut banks_client,
        usdc_test_reserve.collateral_supply_pubkey,
    )
    .await;

    let reserve_state_after = usdc_test_reserve.get_state(&mut banks_client).await;
    assert_eq!(
        initial_user_liquidity_balance - 100 * FRACTIONAL_TO_USDC,
        final_user_liquidity_balance
    );
    assert_eq!(
        initial_liquidity_supply_balance + 100 * FRACTIONAL_TO_USDC,
        final_liquidity_supply_balance
    );

    assert_eq!(initial_user_collateral_balance, 10_000 * FRACTIONAL_TO_USDC);
    assert_eq!(
        initial_collateral_supply_balance,
        final_collateral_supply_balance
    );
    assert_eq!(
        final_user_collateral_balance - initial_user_collateral_balance,
        reserve_state_after.collateral.mint_total_supply
            - reserve_state_before.collateral.mint_total_supply
    );
}
