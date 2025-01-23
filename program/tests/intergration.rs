#![cfg(feature = "test-sbf")]
mod helpers;
mod stateful;
use stateful::*;
use {solana_program_test::*, solana_sdk::signature::Keypair};

#[tokio::test]
async fn integration_test() {
    let market_owner = Keypair::new();
    let mut test = IntegrationTest::new().await;

    //create a market
    test.create_market().await;
    //change the market owner to the market owner
    test.change_market_owner(market_owner).await;
    //create init user supply accounts
    //create reserves
    test.create_reserves().await;
    //refresh reserves
    test.refresh_reserves().await;
    //open usdc and sol token accounts, and collateral accounts for both alice and
    // bob
    test.open_accounts().await;
    //create obligations
    test.create_obligations().await;

    test.alice_borrow_sol_without_collateral().await;
    //by default it airdrop 1000 tokens to each account of each borrower in respect
    // mint type.
    test.top_up_token_accounts().await;
    test.alice_deposit_usdc_reserve(1000).await;
    test.go_to_slot(3).await;
    test.alice_deposit_usdc_collateral_to_obligations(1000)
        .await;
    test.alice_borrow_sol_with_collateral().await;
    test.bob_deposit_sol_reserve(1000).await;
    // test.go_to_slot(5).await;
    // test.refresh_reserves().await;
    // test.bob_deposit_sol_collateral_to_obligations(1000).await;
    // test.alice_borrow_sol_with_collateral().await;
}
