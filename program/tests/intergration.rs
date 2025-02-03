#![cfg(feature = "test-sbf")]
mod helpers;
mod stateful;
use {
    helpers::{create_test_reserve_config, FRACTIONAL_TO_USDC},
    solana_program_test::*,
    solana_sdk::{
        msg,
        native_token::LAMPORTS_PER_SOL,
        signature::{read_keypair_file, Keypair},
    },
    spl_token_lending::state::SLOTS_PER_YEAR,
    stateful::*,
    std::u64,
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
    test.refresh_reserves().await;
    test.refresh_obligation("alice").await;
    test.alice_borrow_sol_without_collateral().await;
    //It airdrop 5000 tokens to each account of each borrower in respect
    // mint type.
    test.top_up_token_accounts(Some(5000)).await;

    let bob_first_get_usdc_collateral = test.deposit_reserve_liquidity("bob", "usdc", 100).await;

    //in the beginning of exchange rate from token to collateral is 1:1
    assert_eq!(bob_first_get_usdc_collateral, 100 * FRACTIONAL_TO_USDC);
    test.transfer_bewteen_borrowers(bob_first_get_usdc_collateral, "bob", "alice", "usdc", true)
        .await;
    test.deposit_reserve_liquidity("bob", "sol", 500).await;
    test.go_to_slot(2).await;
    test.refresh_reserves().await;

    let alice_deposit_result = test
        .deposit_collateral_to_obligations("alice", "usdc", 100 * FRACTIONAL_TO_USDC)
        .await;
    assert!(alice_deposit_result.is_ok());
    test.refresh_obligation("alice").await;
    let alice_sol_balance_before_borrow = test.get_borrower_balance("alice", "sol", false).await;
    test.alice_borrow_sol_with_usdc_collateral().await;
    let alice_sol_balance_after_borrow = test.get_borrower_balance("alice", "sol", false).await;
    let alice_borrowed_sol = alice_sol_balance_after_borrow - alice_sol_balance_before_borrow;
    msg!("alice_borrowed_sol: {}", alice_borrowed_sol);
    assert!(alice_borrowed_sol > 0);
    test.go_to_slot(SLOTS_PER_YEAR).await;
    test.refresh_reserves().await;
    test.refresh_obligation("alice").await;
    // assert_eq!(alice_borrowed_sol ;
    let alice_sol_balance_before_repay = test.get_borrower_balance("alice", "sol", false).await;
    test.alice_repay_sol_to_obligation().await;
    let alice_sol_balance_after_repay = test.get_borrower_balance("alice", "sol", false).await;
    let alice_repayed_sol = alice_sol_balance_before_repay - alice_sol_balance_after_repay;
    msg!("alice_repayed_sol: {}", alice_repayed_sol);
    assert!(alice_repayed_sol > alice_borrowed_sol);
    test.go_to_slot(SLOTS_PER_YEAR + 1).await;
    test.refresh_obligation("bob").await;
    test.refresh_reserves().await;
    let result = test
        .redeem_reserve_liquidity("bob", "sol", 500 * LAMPORTS_PER_SOL)
        .await;
    assert!(result.is_ok());
    let bob_sol_balance_after_redeem = test.get_borrower_balance("bob", "sol", false).await;
    // msg!("can be redeemed: {}", bob_sol_balance_after_redeem);
    assert!(bob_sol_balance_after_redeem > 0);
    let withdraw_result = test
        .withdraw_obligation_liquidity("alice", "usdc", u64::MAX)
        .await;
    // msg!("withdraw_result: {:?}", withdraw_result);
    assert!(withdraw_result.is_ok());
}

#[tokio::test]
async fn alice_can_brorow_sol_but_got_liquidited_from_bob() {
    let mut test = IntegrationTest::new().await;
    test.create_market(None).await;
    let sol_reserve_config = Some(create_test_reserve_config(90, 99));
    test.create_reserves(Some(CreateReserveParams {
        sol_reserve_config,
        ..Default::default()
    }))
    .await;
    test.open_accounts().await;
    test.create_obligations().await;
    test.top_up_token_accounts(Some(1000)).await;
    test.refresh_reserves().await;
    test.deposit_reserve_liquidity("bob", "usdc", 100).await;
    test.deposit_reserve_liquidity("bob", "sol", 50).await;
    test.transfer_bewteen_borrowers(100 * FRACTIONAL_TO_USDC, "bob", "alice", "usdc", true)
        .await;
    test.go_to_slot(3).await;
    test.refresh_reserves().await;
    test.refresh_obligation("alice").await;
    test.refresh_obligation("bob").await;
    test.deposit_collateral_to_obligations("alice", "usdc", 100 * FRACTIONAL_TO_USDC)
        .await
        .unwrap();
    test.go_to_slot(4).await;
    test.refresh_reserves().await;
    test.refresh_obligation("alice").await;

    test.alice_borrow_sol_with_usdc_collateral().await;
    let new_usdc_reseve_config = create_test_reserve_config(5, 6);
    test.modify_reserve_config("usdc", new_usdc_reseve_config)
        .await
        .unwrap();
    test.go_to_slot(8).await;
    // test.go_to_slot(2 * SLOTS_PER_YEAR).await; //alternatively, you can wait for the obligation to be liquidated when the interest is too high
    test.refresh_reserves().await;
    test.refresh_obligation("alice").await;

    //1. Initially,alice borrowed SOL, with usdc collateral as deposit.
    //2. for reasons like her collateral market price drops, or she hasn't repaid her obligation for a long time,
    //which makes the accumulated interest too high, it can make her collateral be able to be liquidated by bob.
    //3. for bob to liquidate alice's collateral, he needs to pay back alice's obligation in SOL, and get alice's USDC collateral in return with a discount.
    let result = test
        .liquidate_obligation_liquidity("bob", "alice", "sol", "usdc", u64::MAX)
        .await;
    assert!(result.is_ok());
}
