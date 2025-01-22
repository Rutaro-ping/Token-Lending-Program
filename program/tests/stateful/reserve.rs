use solana_sdk::{signer::Signer, transaction::Transaction};
use spl_token::instruction::approve;
use spl_token_lending::instruction::builder::{deposit_reserve_liquidity, refresh_reserve};

use super::{Borrower, IntegrationTest, INIT_RESERVE_SOL_AMOUNT, INIT_RESERVE_USDC_AMOUNT};
use crate::{
    helpers::{TestReserve, TEST_RESERVE_CONFIG},
    sign_and_execute,
};

impl IntegrationTest {
    pub async fn create_reserves(&mut self) {
        let lending_market = self.lending_market.as_ref().unwrap();
        let sol_reserve = TestReserve::init(
            "sol".to_owned(),
            &self.test_context.banks_client,
            lending_market,
            &self.sol_oracle,
            INIT_RESERVE_SOL_AMOUNT,
            TEST_RESERVE_CONFIG,
            spl_token::native_mint::id(),
            self.init_sol_user_liquidity_account,
            &self.test_context.payer,
            &self.user_accounts_owner,
        )
        .await
        .unwrap();
        self.sol_reserve = Some(sol_reserve);
        let usdc_reserve = TestReserve::init(
            "usdc".to_owned(),
            &self.test_context.banks_client,
            lending_market,
            &self.usdc_oracle,
            INIT_RESERVE_USDC_AMOUNT,
            TEST_RESERVE_CONFIG,
            self.usdc_mint.pubkey,
            self.init_usdc_user_liquidity_account,
            &self.test_context.payer,
            &self.user_accounts_owner,
        )
        .await
        .unwrap();
        self.usdc_reserve = Some(usdc_reserve);
    }

    pub async fn refresh_reserves(&self) {
        let sol_reserve = self.sol_reserve.as_ref().unwrap();
        let usdc_reserve = self.usdc_reserve.as_ref().unwrap();

        let mut transaction = Transaction::new_with_payer(
            &[
                refresh_reserve(
                    spl_token_lending::id(),
                    sol_reserve.pubkey,
                    self.sol_oracle.price_pubkey,
                ),
                refresh_reserve(
                    spl_token_lending::id(),
                    usdc_reserve.pubkey,
                    self.usdc_oracle.price_pubkey,
                ),
            ],
            Some(&self.test_context.payer.pubkey()),
        );

        assert!(sign_and_execute!(self, transaction).is_ok());
    }

    pub(super) async fn deposit_reserve_liquidity(
        &self,
        reserve: &TestReserve,
        borrower: &Borrower,
        liquidity_amount: u64,
        currency: &str, //"sol" or "usdc"
    ) {
        let payer = &self.test_context.payer;
        let accounts = borrower.accounts.get(currency).unwrap();

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
}
