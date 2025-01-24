#![cfg(feature = "test-sbf")]
mod helpers;
mod stateful;
use helpers::{get_token_balance, TestObligation, FRACTIONAL_TO_USDC};
use solana_sdk::{msg, signature::read_keypair_file};
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

    let alice_borrower = test.borrowers.get("alice").unwrap();
    let bob_borrower = test.borrowers.get("bob").unwrap();
    let alice_obligation = alice_borrower.obligation.as_ref().unwrap();
    let alice_accounts = &alice_borrower.accounts.clone();
    let obligation = alice_obligation
        .get_state(&test.test_context.banks_client)
        .await;
    let bob_accounts = &bob_borrower.accounts.clone();

    // println!("alice obligation before: {:#?}", obligation);

    test.alice_borrow_sol_without_collateral().await;
    //by default it airdrop 100 tokens to each account of each borrower in respect
    // mint type.
    test.top_up_token_accounts(None).await;

    let collateral_usdc_amount = test.deposit_reserve_liquidity("bob", "usdc", 100).await;
    //in the beginning of exchange rate from token to collateral is 1:1
    assert_eq!(collateral_usdc_amount, 100 * FRACTIONAL_TO_USDC);
    test.transfer_bewteen_borrowers(100, "bob", "alice", "usdc", true)
        .await;

    // test.alice_borrow_sol_with_collateral().await;
    // test.bob_deposit_sol_reserve(1000).await;
    // test.go_to_slot(5).await;
    // test.refresh_reserves().await;
    // test.bob_deposit_sol_collateral_to_obligations(1000).await;
    // test.alice_borrow_sol_with_collateral().await;
}
