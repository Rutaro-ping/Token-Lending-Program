#![cfg(feature = "test-sbf")]
mod helpers;
mod stateful;
use {
    helpers::{get_state, FRACTIONAL_TO_USDC},
    solana_program_test::*,
    solana_sdk::{
        msg,
        signature::{read_keypair_file, Keypair},
    },
    spl_token::state::Account,
    stateful::*,
};

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
    test.refresh_obligation("alice").await;
    test.alice_borrow_sol_without_collateral().await;
    //It airdrop 1000 tokens to each account of each borrower in respect
    // mint type.
    test.top_up_token_accounts(Some(1000)).await;

    let collateral_usdc_amount = test.deposit_reserve_liquidity("bob", "usdc", 100).await;
    //in the beginning of exchange rate from token to collateral is 1:1
    assert_eq!(collateral_usdc_amount, 100 * FRACTIONAL_TO_USDC);
    test.transfer_bewteen_borrowers(collateral_usdc_amount, "bob", "alice", "usdc", true)
        .await;
    test.deposit_reserve_liquidity("bob", "sol", 500).await;
    test.go_to_slot(2).await;
    test.refresh_reserves().await;

    let alice_deposit_result = test
        .deposit_collateral_to_obligations("alice", "usdc", 100)
        .await;
    assert!(alice_deposit_result.is_ok());
    test.refresh_obligation("alice").await;
    test.alice_borrow_sol_with_usdc_collateral().await;

    test.go_to_slot(100).await;
    test.refresh_reserves().await;
    test.refresh_obligation("alice").await;
    test.alice_repay_sol_to_obligation().await;
    test.refresh_obligation("alice").await;
    let result = test.redeem_reserve_liquidity("alice", "usdc", 4).await;
    assert!(result.is_ok());
}

// #[tokio::test]
// async fn alice_can_brorow_sol_but_got_liquidited_from_bob() {
//     let mut test = IntegrationTest::new().await;
//     test.create_market(None).await;
//     test.create_reserves(None).await;
//     test.open_accounts().await;
//     test.create_obligations().await;
//     test.alice_borrow_sol_without_collateral().await;
//     test.top_up_token_accounts(Some(1000)).await;
// }
