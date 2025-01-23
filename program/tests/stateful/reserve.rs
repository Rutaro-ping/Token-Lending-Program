use std::collections::HashMap;

use solana_program_test::BanksClientError;
use solana_sdk::{signer::Signer, transaction::Transaction};
use spl_token::instruction::approve;
use spl_token_lending::instruction::builder::{
    deposit_reserve_liquidity, redeem_reserve_collateral, refresh_reserve,
};

use super::{Borrower, IntegrationTest, INIT_RESERVE_SOL_AMOUNT, INIT_RESERVE_USDC_AMOUNT};
use crate::{
    helpers::{TestReserve, TEST_RESERVE_CONFIG},
    sign_and_execute, CURRENCY_TYPE,
};

impl IntegrationTest {
    pub async fn create_reserves(&mut self) {
        let lending_market = self.lending_market.as_ref().unwrap();
        let (init_sol_user_liquidity_account, init_usdc_user_liquidity_account) =
            self.create_init_user_supply_accounts().await;

        let sol_reserve = TestReserve::init(
            "sol".to_owned(),
            &self.test_context.banks_client,
            lending_market,
            self.oracles.get("sol").unwrap(),
            INIT_RESERVE_SOL_AMOUNT,
            TEST_RESERVE_CONFIG,
            spl_token::native_mint::id(),
            init_sol_user_liquidity_account,
            &self.test_context.payer,
            &self.user_accounts_owner,
        )
        .await
        .unwrap();

        let usdc_reserve = TestReserve::init(
            "usdc".to_owned(),
            &self.test_context.banks_client,
            lending_market,
            self.oracles.get("usdc").unwrap(),
            INIT_RESERVE_USDC_AMOUNT,
            TEST_RESERVE_CONFIG,
            self.usdc_mint.pubkey,
            init_usdc_user_liquidity_account,
            &self.test_context.payer,
            &self.user_accounts_owner,
        )
        .await
        .unwrap();

        self.reserves = HashMap::from([("sol", sol_reserve), ("usdc", usdc_reserve)]);
    }

    pub async fn refresh_reserves(&self) {
        let mut transaction = Transaction::new_with_payer(
            &CURRENCY_TYPE
                .iter()
                .map(|&currency| {
                    refresh_reserve(
                        spl_token_lending::id(),
                        self.reserves.get(currency).unwrap().pubkey,
                        self.oracles.get(currency).unwrap().price_pubkey,
                    )
                })
                .collect::<Vec<_>>(),
            Some(&self.test_context.payer.pubkey()),
        );

        assert!(sign_and_execute!(self, transaction).is_ok());
    }

    pub(super) async fn deposit_reserve_liquidity(
        &self,
        borrower: &Borrower,
        liquidity_amount: u64,
        currency: &str, //"sol" or "usdc"
    ) {
        let payer = &self.test_context.payer;
        let accounts = borrower.accounts.get(currency).unwrap();
        let reserve = self.reserves.get(currency).unwrap();
        let mut transaction = Transaction::new_with_payer(
            &[
                approve(
                    &spl_token::id(),
                    &accounts.token_account,
                    &borrower.user_transfer_authority.pubkey(),
                    &borrower.keypair.pubkey(),
                    &[],
                    liquidity_amount,
                )
                .unwrap(),
                deposit_reserve_liquidity(
                    spl_token_lending::id(),
                    liquidity_amount,
                    accounts.token_account,
                    accounts.collateral_account,
                    reserve.pubkey,
                    reserve.liquidity_supply_pubkey,
                    reserve.collateral_mint_pubkey,
                    self.lending_market.as_ref().unwrap().pubkey,
                    borrower.user_transfer_authority.pubkey(),
                ),
            ],
            Some(&payer.pubkey()),
        );
        sign_and_execute!(
            self,
            transaction,
            &borrower.keypair,
            &borrower.user_transfer_authority
        )
        .unwrap()
    }

    pub async fn redeem_reserve_liquidity(
        &self,
        amount: u64,
        borrower: &Borrower,
        currency: &str,
    ) -> Result<(), BanksClientError> {
        let reserve = self.reserves.get(currency).unwrap();
        let accounts = borrower.accounts.get(currency).unwrap();
        let mut transaction = Transaction::new_with_payer(
            &[
                approve(
                    &spl_token::id(),
                    &accounts.collateral_account,
                    &borrower.user_transfer_authority.pubkey(),
                    &borrower.keypair.pubkey(),
                    &[],
                    amount,
                )
                .unwrap(),
                redeem_reserve_collateral(
                    spl_token_lending::id(),
                    amount,
                    accounts.collateral_account,
                    accounts.token_account,
                    reserve.pubkey,
                    reserve.collateral_mint_pubkey,
                    reserve.liquidity_supply_pubkey,
                    self.lending_market.as_ref().unwrap().pubkey,
                    borrower.user_transfer_authority.pubkey(),
                ),
            ],
            Some(&self.test_context.payer.pubkey()),
        );

        sign_and_execute!(
            self,
            transaction,
            &borrower.keypair,
            &borrower.user_transfer_authority
        )
    }
}
