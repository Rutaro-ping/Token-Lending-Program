use solana_program_test::BanksClientError;
use solana_sdk::{signer::Signer, transaction::Transaction};
use spl_token::instruction::approve;
use spl_token_lending::instruction::builder::{
    borrow_obligation_liquidity, deposit_obligation_collateral, refresh_obligation, refresh_reserve,
};

use super::{Borrower, IntegrationTest};
use crate::{
    helpers::{TestObligation, TestReserve},
    sign_and_execute,
};

impl IntegrationTest {
    pub async fn create_obligations(&mut self) {
        let borrower_bob = self.borrowers.get_mut("bob").unwrap();
        let lending_market = self.lending_market.as_ref().unwrap();
        let bob_obligation = TestObligation::init(
            &self.test_context.banks_client,
            lending_market,
            &borrower_bob.keypair,
            &self.test_context.payer,
        )
        .await
        .unwrap();
        bob_obligation
            .validate_state(&mut self.test_context.banks_client)
            .await;
        borrower_bob.obligation = Some(bob_obligation);
        let borrower_alice = self.borrowers.get_mut("alice").unwrap();
        let alice_obligation = TestObligation::init(
            &self.test_context.banks_client,
            lending_market,
            &borrower_alice.keypair,
            &self.test_context.payer,
        )
        .await
        .unwrap();
        alice_obligation
            .validate_state(&self.test_context.banks_client)
            .await;
        borrower_alice.obligation = Some(alice_obligation);
    }

    pub async fn deposit_obligations(
        &self,
        borrower: &Borrower,

        currency: &str, //"sol" or "usdc"
        collateral_amount: u64,
    ) {
        let obligation = borrower.obligation.as_ref().unwrap();
        let lending_market = self.lending_market.as_ref().unwrap();
        let accounts = borrower.accounts.get(currency).unwrap();
        let reserve = self.reserves.get(currency).unwrap();
        let mut transaction = Transaction::new_with_payer(
            &[
                approve(
                    &spl_token::id(),
                    &accounts.collateral_account,
                    &borrower.user_transfer_authority.pubkey(),
                    &borrower.keypair.pubkey(),
                    &[],
                    collateral_amount,
                )
                .unwrap(),
                deposit_obligation_collateral(
                    spl_token_lending::id(),
                    collateral_amount,
                    accounts.collateral_account,
                    reserve.collateral_supply_pubkey,
                    reserve.pubkey,
                    obligation.pubkey,
                    lending_market.pubkey,
                    obligation.owner,
                    borrower.user_transfer_authority.pubkey(),
                ),
            ],
            Some(&self.test_context.payer.pubkey()),
        );
        let result = sign_and_execute!(
            self,
            transaction,
            &borrower.keypair,
            &borrower.user_transfer_authority
        );
        assert!(result.is_ok());
    }
    pub(super) async fn borrow_obligation_liquidity(
        &self,
        reserve: &TestReserve,
        borrower: &Borrower,
    ) -> Result<(), BanksClientError> {
        let obligation = borrower.obligation.as_ref().unwrap();
        let lending_market = self.lending_market.as_ref().unwrap();
        let oracle = self.oracles.get("sol").unwrap();
        let mut transaction = Transaction::new_with_payer(
            &[
                refresh_reserve(spl_token_lending::id(), reserve.pubkey, oracle.price_pubkey),
                refresh_obligation(spl_token_lending::id(), obligation.pubkey, vec![]),
                borrow_obligation_liquidity(
                    spl_token_lending::id(),
                    u64::MAX,
                    None,
                    reserve.liquidity_supply_pubkey,
                    reserve.user_liquidity_pubkey,
                    reserve.pubkey,
                    reserve.liquidity_fee_receiver_pubkey,
                    obligation.pubkey,
                    lending_market.pubkey,
                    borrower.keypair.pubkey(),
                    Some(reserve.liquidity_host_pubkey),
                ),
            ],
            Some(&self.test_context.payer.pubkey()),
        );
        sign_and_execute!(
            self,
            transaction,
            &self.test_context.payer,
            &borrower.keypair
        )
    }
    //provide airdrop for native SOL
}
