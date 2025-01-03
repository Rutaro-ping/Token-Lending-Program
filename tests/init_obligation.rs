#![allow(clippy::arithmetic_side_effects)]
#![cfg(feature = "test-sbf")]

mod helpers;
use {
    helpers::{add_lending_market, TestObligation}, solana_program_test::*, solana_sdk::signature::Keypair,
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
    test.set_compute_max_units(8_000);
    let user_accounts_owner = Keypair::new();
    let lending_market = add_lending_market(&mut test);
    let (mut banks_client, payer, _recent_blockhash) = test.start().await;
    let obligation = TestObligation::init(
        &mut banks_client,
        &lending_market,
        &user_accounts_owner,
        &payer,
    )
    .await
    .unwrap();
  obligation.validate_state(&mut banks_client).await;
}
