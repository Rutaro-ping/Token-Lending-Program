#![cfg(feature = "test-sbf")]
mod helpers;
use solana_program_test::*;
use spl_token_lending::processor::process_instruction;

use helpers::TestLendingMarket;

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
    // let market = test_lending_market.get_state(&mut banks_client).await;
    // let balance = banks_client.get_balance(payer.pubkey()).await.unwrap();
    // let instructions = vec![create_account(
    //     &payer.pubkey(),
    //     &lending_market_pubkey,
    //     rent.minimum_balance(LendingMarket::LEN),
    //     LendingMarket::LEN as u64,
    //     &spl_token_lending::id(),
    // )];
    // Transaction::new_with_payer(instructions, payer)
}
