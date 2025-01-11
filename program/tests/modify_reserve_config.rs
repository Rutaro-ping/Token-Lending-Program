#![allow(clippy::arithmetic_side_effects)]
#![cfg(feature = "test-sbf")]

mod helpers;
use {
    helpers::*,
    solana_program_test::*,
    solana_sdk::{signature::Keypair, signer::Signer, transaction::Transaction},
    spl_token_lending::{
        instruction::builder::modify_reserve_config,
        processor::process_instruction,
        state::{ReserveConfig, ReserveFees, INITIAL_COLLATERAL_RATIO},
    },
};

#[tokio::test]
async fn modify_reserve_config_success() {
    let mut test = ProgramTest::new(
        "spl_token_lending",
        spl_token_lending::id(),
        processor!(process_instruction),
    );

    test.set_compute_max_units(70_000);

    let user_accounts_owner = Keypair::new();
    let lending_market = add_lending_market(&mut test);
    let sol_oracle = add_sol_oracle(&mut test);

    const SOL_DEPOSIT_AMOUNT_LAMPORTS: u64 = 10 * LAMPORTS_TO_SOL * INITIAL_COLLATERAL_RATIO;
    const SOL_RESERVE_COLLATERAL_LAMPORTS: u64 = 2 * SOL_DEPOSIT_AMOUNT_LAMPORTS;

    let sol_test_reserve = add_reserve(
        &mut test,
        &lending_market,
        &sol_oracle,
        &user_accounts_owner,
        AddReserveArgs {
            user_liquidity_amount: SOL_RESERVE_COLLATERAL_LAMPORTS,
            liquidity_amount: SOL_RESERVE_COLLATERAL_LAMPORTS,
            liquidity_mint_decimals: 9,
            liquidity_mint_pubkey: spl_token::native_mint::id(),
            config: TEST_RESERVE_CONFIG,
            mark_fresh: true,
            ..AddReserveArgs::default()
        },
    );
    let (mut banks_client, payer, recent_blockhash) = test.start().await;
    const OPTIMAL_UTILIZATION_RATE_CHANGE: u8 = 10;
    let new_config = ReserveConfig {
        optimal_utilization_rate: TEST_RESERVE_CONFIG.optimal_utilization_rate
            - OPTIMAL_UTILIZATION_RATE_CHANGE,
        loan_to_value_ratio: 50,
        liquidation_bonus: 5,
        liquidation_threshold: 55,
        min_borrow_rate: 0,
        optimal_borrow_rate: 4,
        max_borrow_rate: 30,
        fees: ReserveFees {
            borrow_fee_wad: 100_000_000_000,
            flash_loan_fee_wad: 3_000_000_000_000_000,
            host_fee_percentage: 20,
        },
    };
    let mut transaction = Transaction::new_with_payer(
        &[modify_reserve_config(
            spl_token_lending::id(),
            new_config,
            sol_test_reserve.pubkey,
            lending_market.pubkey,
            lending_market.owner.pubkey(),
        )],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[&payer, &lending_market.owner], recent_blockhash);
    banks_client
        .process_transaction(transaction)
        .await
        .map_err(|e| e.unwrap())
        .unwrap();
    let reserve_info = sol_test_reserve.get_state(&mut banks_client).await;
    assert_eq!(reserve_info.config, new_config);
}
