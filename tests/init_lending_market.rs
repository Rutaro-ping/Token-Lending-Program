#![cfg(feature = "test-sbf")]
mod helpers;
use {
    helpers::TestLendingMarket, solana_program_test::*,
    spl_token_lending::processor::process_instruction,
};

#[tokio::test]
async fn test_success() {
    let mut test = ProgramTest::new(
        "spl_token_lending",
        spl_token_lending::id(),
        processor!(process_instruction),
    );
    test.set_compute_max_units(20_000);
    let (mut banks_client, payer, _recent_blockhash) = test.start().await;
    let test_lending_market = TestLendingMarket::init(&mut banks_client, &payer).await;
    test_lending_market.validate_state(&mut banks_client).await;
}
