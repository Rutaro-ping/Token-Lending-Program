#![cfg(feature = "test-sbf")]
mod helpers;
use helpers::integration_utils;
use solana_program_test::*;
use solana_sdk::signature::Keypair;

#[tokio::test]
async fn integration_test() {
    let market_owner = Keypair::new();
    let mut test = integration_utils::IntegrationTest::new().await;
    test.create_market().await;
    test.change_market_owner(market_owner).await;
    test.create_init_user_supply_accounts().await;
    test.create_reserves().await;
    test.refresh_reserves().await;
    test.create_obligations().await;
    test.borrow_obligation_liquidity().await;
}
