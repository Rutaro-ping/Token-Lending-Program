#![cfg(feature = "test-sbf")]
mod helpers;
use helpers::integration_utils;
use solana_program_test::*;
use solana_sdk::signature::Keypair;

#[tokio::test]
async fn integration_test() {
    let market_owner = Keypair::new();
    let mut test = integration_utils::IntegrationTest::new().await;
    //open usdc and sol token accounts for both alice and bob
    test.open_usdc_sol_accounts().await;
    //create a market
    test.create_market().await;
    //change the market owner to the market owner
    test.change_market_owner(market_owner).await;
    //create init user supply accounts
    test.create_init_user_supply_accounts().await;
    //create reserves
    test.create_reserves().await;
    //refresh reserves
    test.refresh_reserves().await;
    //create obligations
    test.create_obligations().await;

    test.alice_borrow_sol_without_collateral().await;


}
