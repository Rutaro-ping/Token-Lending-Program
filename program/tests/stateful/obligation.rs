use {
    super::{Borrower, IntegrationTest},
    crate::{
        helpers::{TestObligation, FRACTIONAL_TO_USDC, LAMPORTS_TO_SOL},
        sign_and_execute,
    },
    solana_program_test::BanksClientError,
    solana_sdk::{msg, pubkey::Pubkey, signer::Signer, transaction::Transaction},
    spl_token::instruction::approve,
    spl_token_lending::instruction::builder::{
        borrow_obligation_liquidity, deposit_obligation_collateral, liquidate_obligation,
        refresh_obligation, refresh_reserve, repay_obligation_liquidity,
        withdraw_obligation_collateral,
    },
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

    pub async fn deposit_collateral_to_obligations(
        &self,
        borrower: &str,
        currency: &str, //"sol" or "usdc"
        collateral_amount: u64,
    ) -> Result<(), BanksClientError> {
        let borrower = self.borrowers.get(borrower).unwrap();
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
        sign_and_execute!(
            self,
            transaction,
            &borrower.keypair,
            &borrower.user_transfer_authority
        )
    }

    /**
     * @param currency: "sol" or "usdc"
     * @param borrower: the borrower
     * @param borrow_amount: the amount of liquidity to borrow, default is
     * u64::MAX @param slippage_limit: the slippage limit, default is
     * None
     */
    pub(super) async fn borrow_obligation_liquidity(
        &self,
        currency: &str,
        borrower: &Borrower,
        borrow_amount: Option<u64>,
        slippage_limit: Option<u64>,
    ) -> Result<(), BanksClientError> {
        let obligation = borrower.obligation.as_ref().unwrap();
        let lending_market = self.lending_market.as_ref().unwrap();
        let oracle = self.oracles.get(currency).unwrap();
        let reserve = self.reserves.get(currency).unwrap();
        let borrow_amount = if let Some(amount) = borrow_amount {
            amount
                * match currency {
                    "sol" => LAMPORTS_TO_SOL,
                    "usdc" => FRACTIONAL_TO_USDC,
                    _ => unreachable!(),
                }
        } else {
            u64::MAX
        };

        let borrower_accounts = borrower.accounts.get(currency).unwrap();

        let reserve_pubkeys = self.get_refered_reserves_for_obligation(obligation).await;
        let mut transaction = Transaction::new_with_payer(
            &[
                refresh_reserve(spl_token_lending::id(), reserve.pubkey, oracle.price_pubkey),
                refresh_obligation(spl_token_lending::id(), obligation.pubkey, reserve_pubkeys),
                borrow_obligation_liquidity(
                    spl_token_lending::id(),
                    borrow_amount,
                    slippage_limit,
                    reserve.liquidity_supply_pubkey,
                    borrower_accounts.token_account,
                    reserve.pubkey,
                    reserve.liquidity_fee_receiver_pubkey,
                    obligation.pubkey,
                    lending_market.pubkey,
                    borrower.keypair.pubkey(),
                    None,
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
    async fn withdraw_obligation_liquidity(
        &self,
        borrower: &Borrower,
        currency: &str,
        amount: u64,
    ) -> Result<(), BanksClientError> {
        let obligation = borrower.obligation.as_ref().unwrap();
        let reserve = self.reserves.get(currency).unwrap();
        let accounts = borrower.accounts.get(currency).unwrap();
        let reserve_pubkeys = self.get_refered_reserves_for_obligation(obligation).await;
        let mut transaction = Transaction::new_with_payer(
            &[
                refresh_obligation(spl_token_lending::id(), obligation.pubkey, reserve_pubkeys),
                withdraw_obligation_collateral(
                    spl_token_lending::id(),
                    amount,
                    accounts.collateral_account,
                    accounts.token_account,
                    reserve.pubkey,
                    obligation.pubkey,
                    self.lending_market.as_ref().unwrap().pubkey,
                    obligation.owner,
                ),
            ],
            Some(&self.test_context.payer.pubkey()),
        );
        sign_and_execute!(self, transaction, &borrower.keypair)
    }

    pub async fn repay_obligation_liquidity(
        &self,
        borrower: &str,
        currency: &str,
        amount: u64,
    ) -> Result<(), BanksClientError> {
        let borrower = self.borrowers.get(borrower).unwrap();
        let obligation = borrower.obligation.as_ref().unwrap();
        let reserve = self.reserves.get(currency).unwrap();
        let accounts = borrower.accounts.get(currency).unwrap();
        let contained_reserves = self.get_refered_reserves_for_obligation(obligation).await;
        let obligation_state = obligation.get_state(&self.test_context.banks_client).await;
        msg!("obligation_state: {:#?}", contained_reserves);

        let mut transaction = Transaction::new_with_payer(
            &[
                approve(
                    &spl_token::id(),
                    &accounts.token_account,
                    &borrower.user_transfer_authority.pubkey(),
                    &borrower.keypair.pubkey(),
                    &[],
                    amount,
                )
                .unwrap(),
                refresh_obligation(
                    spl_token_lending::id(),
                    obligation.pubkey,
                    contained_reserves,
                ),
                repay_obligation_liquidity(
                    spl_token_lending::id(),
                    amount,
                    accounts.token_account,
                    reserve.liquidity_supply_pubkey,
                    reserve.pubkey,
                    obligation.pubkey,
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

    async fn liquidate_obligation_liquidity(
        &self,
        borrower: &Borrower,
        currency: &str,
        amount: u64,
    ) -> Result<(), BanksClientError> {
        let obligation = borrower.obligation.as_ref().unwrap();
        let reserve = self.reserves.get(currency).unwrap();
        let reserve_pubkeys = self.get_refered_reserves_for_obligation(obligation).await;
        let mut transaction = Transaction::new_with_payer(
            &[
                approve(
                    &spl_token::id(),
                    &reserve.user_liquidity_pubkey,
                    &borrower.user_transfer_authority.pubkey(),
                    &borrower.keypair.pubkey(),
                    &[],
                    amount,
                )
                .unwrap(),
                refresh_obligation(spl_token_lending::id(), obligation.pubkey, reserve_pubkeys),
                liquidate_obligation(
                    spl_token_lending::id(),
                    amount,
                    reserve.user_liquidity_pubkey,
                    reserve.user_collateral_pubkey,
                    reserve.pubkey,
                    reserve.liquidity_supply_pubkey,
                    reserve.pubkey,
                    reserve.collateral_supply_pubkey,
                    obligation.pubkey,
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
    async fn get_refered_reserves_for_obligation(
        &self,
        obligation: &TestObligation,
    ) -> Vec<Pubkey> {
        let obligation_state = obligation.get_state(&self.test_context.banks_client).await;
        let borrow_reserves = obligation_state
            .borrows
            .iter()
            .map(|borrow| borrow.borrow_reserve)
            .collect::<Vec<Pubkey>>();
        let deposit_reserves = obligation_state
            .deposits
            .iter()
            .map(|deposit| deposit.deposit_reserve)
            .collect::<Vec<Pubkey>>();
        let mut reserves = deposit_reserves;
        reserves.extend(borrow_reserves);
        reserves
    }
    //provide airdrop for native SOL
}
