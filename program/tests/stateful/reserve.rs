use {
    super::{IntegrationTest, MIN_OPEN_ACCOUNT_AMOUNT},
    crate::{
        helpers::{get_token_balance, TestReserve, FRACTIONAL_TO_USDC, TEST_RESERVE_CONFIG},
        sign_and_execute, CURRENCY_TYPE,
    },
    solana_program_test::BanksClientError,
    solana_sdk::{native_token::LAMPORTS_PER_SOL, signer::Signer, transaction::Transaction},
    spl_token::instruction::approve,
    spl_token_lending::{
        instruction::builder::{
            deposit_reserve_liquidity, redeem_reserve_collateral, refresh_reserve,
        },
        state::ReserveConfig,
    },
    std::collections::HashMap,
};

#[derive(Default, Debug)]
pub(crate) struct CreateReserveParams {
    pub init_sol_amount: Option<u64>,
    pub init_usdc_amount: Option<u64>,
    pub usdc_reserve_config: Option<ReserveConfig>,
    pub sol_reserve_config: Option<ReserveConfig>,
}

impl IntegrationTest {
    //it refreshes reserves after creating them
    pub async fn create_reserves(&mut self, params: Option<CreateReserveParams>) {
        let CreateReserveParams {
            init_sol_amount,
            init_usdc_amount,
            usdc_reserve_config,
            sol_reserve_config,
        } = params.unwrap_or_default();
        let init_sol_amount =
            init_sol_amount.map_or(MIN_OPEN_ACCOUNT_AMOUNT, |amount| amount * LAMPORTS_PER_SOL);
        let init_usdc_amount = init_usdc_amount.map_or(MIN_OPEN_ACCOUNT_AMOUNT, |amount| {
            amount * FRACTIONAL_TO_USDC
        });
        let lending_market = self.lending_market.as_ref().unwrap();
        let (init_sol_user_liquidity_account, init_usdc_user_liquidity_account) = self
            .create_init_user_supply_accounts(init_sol_amount, init_usdc_amount)
            .await;

        let sol_reserve = TestReserve::init(
            "sol".to_owned(),
            &self.test_context.banks_client,
            lending_market,
            self.oracles.get("sol").unwrap(),
            init_sol_amount,
            sol_reserve_config.unwrap_or(TEST_RESERVE_CONFIG),
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
            init_usdc_amount,
            usdc_reserve_config.unwrap_or(TEST_RESERVE_CONFIG),
            self.usdc_mint.pubkey,
            init_usdc_user_liquidity_account,
            &self.test_context.payer,
            &self.user_accounts_owner,
        )
        .await
        .unwrap();

        self.reserves = HashMap::from([("sol", sol_reserve), ("usdc", usdc_reserve)]);
        self.refresh_reserves().await;
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

    pub(crate) async fn deposit_reserve_liquidity(
        &self,
        borrower: &str,
        currency: &str, //"sol" or "usdc"
        liquidity_amount: u64,
    ) -> u64 {
        let payer = &self.test_context.payer;
        let borrower = self.borrowers.get(borrower).unwrap();
        let accounts = borrower.accounts.get(currency).unwrap();
        let reserve = self.reserves.get(currency).unwrap();
        let decimals = match currency {
            "sol" => LAMPORTS_PER_SOL,
            "usdc" => FRACTIONAL_TO_USDC,
            _ => unreachable!(),
        };
        let liquidity_amount = liquidity_amount * decimals;
        let before_balance =
            get_token_balance(&self.test_context.banks_client, accounts.collateral_account).await;
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
        let result = sign_and_execute!(
            self,
            transaction,
            &borrower.keypair,
            &borrower.user_transfer_authority
        );
        assert!(result.is_ok());
        let after_balance =
            get_token_balance(&self.test_context.banks_client, accounts.collateral_account).await;
        after_balance - before_balance
    }

    pub async fn redeem_reserve_liquidity(
        &self,
        borrower: &str,
        currency: &str,
        amount: u64,
    ) -> Result<(), BanksClientError> {
        let reserve = self.reserves.get(currency).unwrap();
        let borrower = self.borrowers.get(borrower).unwrap();
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
