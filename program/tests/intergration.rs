#![cfg(feature = "test-sbf")]
mod helpers;
mod stateful;
use solana_sdk::signature::read_keypair_file;
use stateful::*;

use {solana_program_test::*, solana_sdk::signature::Keypair};

#[tokio::test]
async fn alice_can_brorow_sol_and_repay() {
    let mut test = IntegrationTest::new().await;
    let temp_lending_market_owner = Keypair::new();
    //create a market
    test.create_market(Some(temp_lending_market_owner)).await;
    //change the market owner to the market owner
    let lending_market_owner =
        read_keypair_file("tests/fixtures/lending_market_owner.json").unwrap();
    test.change_market_owner(lending_market_owner).await;
    //create init user supply accounts
    //create reserves
    test.create_reserves(None).await;

    //open usdc and sol token accounts, and collateral accounts for both alice and
    // bob
    test.open_accounts().await;
    //create obligations
    test.create_obligations().await;

    test.alice_borrow_sol_without_collateral().await;
    //by default it airdrop 1000 tokens to each account of each borrower in respect
    // mint type.
    test.top_up_token_accounts(None).await;

    test.go_to_slot(3).await;
    test.alice_deposit_usdc_collateral_to_obligations(1000)
        .await;
    test.alice_borrow_sol_with_collateral().await;
    // test.bob_deposit_sol_reserve(1000).await;
    // test.go_to_slot(5).await;
    // test.refresh_reserves().await;
    // test.bob_deposit_sol_collateral_to_obligations(1000).await;
    // test.alice_borrow_sol_with_collateral().await;
}
