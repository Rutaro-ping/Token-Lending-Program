use solana_program_test::{processor, BanksClient, ProgramTest};
use solana_sdk::{
    hash::Hash, msg, native_token::LAMPORTS_PER_SOL, program_pack::Pack, pubkey::Pubkey,
    signature::Keypair, signer::Signer, transaction::Transaction,
};
use spl_token::state::Account as TokenAccount;
use spl_token_lending::{
    instruction::builder::set_lending_market_owner, processor::process_instruction,
};

use crate::helpers::{get_token_balance, MarketInitParams};

use super::{
    add_sol_oracle, add_usdc_mint, add_usdc_oracle, create_and_mint_to_token_account,
    TestLendingMarket, TestMint, TestObligation, TestOracle, TestReserve, FRACTIONAL_TO_USDC,
    TEST_RESERVE_CONFIG,
};

pub(crate) const INIT_RESERVE_SOL_AMOUNT: u64 = 10 * LAMPORTS_PER_SOL;
pub(crate) const INIT_RESERVE_USDC_AMOUNT: u64 = 10 * FRACTIONAL_TO_USDC;

pub(crate) struct IntegrationTest {
    banks_client: BanksClient,
    payer: Keypair,
    recent_blockhash: Hash,
    sol_oracle: TestOracle,
    usdc_oracle: TestOracle,
    usdc_mint: TestMint,
    pub lending_market: Option<TestLendingMarket>,
    user_accounts_owner: Keypair,
    init_sol_user_liquidity_account: Pubkey,
    init_usdc_user_liquidity_account: Pubkey,
    sol_reserve: Option<TestReserve>,
    usdc_reserve: Option<TestReserve>,
    bob_obligation: Option<TestObligation>,
    alice_obligation: Option<TestObligation>,
}

impl IntegrationTest {
    pub async fn new() -> Self {
        let mut test = ProgramTest::new(
            "spl_token_lending",
            spl_token_lending::id(),
            processor!(process_instruction),
        );

        let sol_oracle = add_sol_oracle(&mut test);
        let usdc_oracle = add_usdc_oracle(&mut test);
        let usdc_mint = add_usdc_mint(&mut test);

        test.set_compute_max_units(80_000);

        let (banks_client, payer, recent_blockhash) = test.start().await;

        IntegrationTest {
            banks_client,
            payer,
            recent_blockhash,
            sol_oracle,
            usdc_oracle,
            usdc_mint,
            lending_market: None,
            user_accounts_owner: Keypair::new(),

            init_sol_user_liquidity_account: Pubkey::default(),
            init_usdc_user_liquidity_account: Pubkey::default(),
            sol_reserve: None,
            usdc_reserve: None,
            bob_obligation: None,
            alice_obligation: None,
        }
    }

    pub async fn create_market(&mut self) {
        let temp_lending_market_owner = Keypair::new();
        let temp_lending_market_keypair = Keypair::new();
        let test_lending_market = TestLendingMarket::init(
            &mut self.banks_client,
            &self.payer,
            Some(MarketInitParams {
                lending_market_owner: Some(temp_lending_market_owner),
                lending_market_keypair: Some(temp_lending_market_keypair),
                ..Default::default()
            }),
        )
        .await;
        let market = test_lending_market.get_state(&mut self.banks_client).await;
        assert_eq!(market.owner, test_lending_market.owner.pubkey());
        // self.lending_market = Some(test_lending_market);
        self.lending_market = Some(test_lending_market);
    }
    pub async fn change_market_owner(&mut self, market_owner: Keypair) {
        let lending_market = self.lending_market.as_mut().unwrap();
        let mut transaction = Transaction::new_with_payer(
            &[set_lending_market_owner(
                spl_token_lending::id(),
                lending_market.pubkey,
                lending_market.owner.pubkey(),
                market_owner.pubkey(),
            )],
            Some(&self.payer.pubkey()),
        );
        // //update the owner of the lending market after it updated onchain.

        transaction.sign(&[&self.payer, &lending_market.owner], self.recent_blockhash);
        self.banks_client
            .process_transaction(transaction)
            .await
            .map_err(|e| e.unwrap())
            .unwrap();

        let market = lending_market.get_state(&mut self.banks_client).await;
        assert_eq!(market.owner, market_owner.pubkey());
        assert_ne!(lending_market.owner.pubkey(), market.owner);
        //update the owner of the lending market after it updated onchain.
        lending_market.owner = market_owner;
    }

    pub async fn create_init_user_supply_accounts(&mut self) {
        let init_sol_user_liquidity_account = create_and_mint_to_token_account(
            &mut self.banks_client,
            spl_token::native_mint::id(),
            None,
            &self.payer,
            self.user_accounts_owner.pubkey(),
            INIT_RESERVE_SOL_AMOUNT,
        )
        .await;

        let init_usdc_user_liquidity_account = create_and_mint_to_token_account(
            &mut self.banks_client,
            self.usdc_mint.pubkey,
            Some(&self.usdc_mint.authority),
            &self.payer,
            self.user_accounts_owner.pubkey(),
            INIT_RESERVE_USDC_AMOUNT,
        )
        .await;

        let sol_balance =
            get_token_balance(&mut self.banks_client, init_sol_user_liquidity_account).await;
        let sol_balance_lamports = self
            .banks_client
            .get_balance(init_sol_user_liquidity_account)
            .await
            .unwrap();
        assert_eq!(sol_balance, INIT_RESERVE_SOL_AMOUNT);
        let rent = self.banks_client.get_rent().await.unwrap();
        let lamports = rent.minimum_balance(TokenAccount::LEN) + INIT_RESERVE_SOL_AMOUNT;
        //native SOL token account total lamports = rent + init_sol_amount
        assert_eq!(sol_balance_lamports, lamports);

        let usdc_balance =
            get_token_balance(&mut self.banks_client, init_usdc_user_liquidity_account).await;
        assert_eq!(usdc_balance, INIT_RESERVE_USDC_AMOUNT);
        self.init_sol_user_liquidity_account = init_sol_user_liquidity_account;
        self.init_usdc_user_liquidity_account = init_usdc_user_liquidity_account;
    }

    pub async fn create_reserves(&mut self) {
        let lending_market = self.lending_market.as_ref().unwrap();
        let sol_reserve = TestReserve::init(
            "sol".to_owned(),
            &mut self.banks_client,
            lending_market,
            &self.sol_oracle,
            INIT_RESERVE_SOL_AMOUNT,
            TEST_RESERVE_CONFIG,
            spl_token::native_mint::id(),
            self.init_sol_user_liquidity_account,
            &self.payer,
            &self.user_accounts_owner,
        )
        .await
        .unwrap();
        self.sol_reserve = Some(sol_reserve);
        let usdc_reserve = TestReserve::init(
            "usdc".to_owned(),
            &mut self.banks_client,
            lending_market,
            &self.usdc_oracle,
            INIT_RESERVE_USDC_AMOUNT,
            TEST_RESERVE_CONFIG,
            self.usdc_mint.pubkey,
            self.init_usdc_user_liquidity_account,
            &self.payer,
            &self.user_accounts_owner,
        )
        .await
        .unwrap();
        self.usdc_reserve = Some(usdc_reserve);
    }
    pub async fn create_obligations(&mut self) {
        let lending_market = self.lending_market.as_ref().unwrap();
        let bob_obligation = TestObligation::init(
            &mut self.banks_client,
            lending_market,
            &Keypair::new(),
            &self.payer,
        )
        .await
        .unwrap();
        bob_obligation.validate_state(&mut self.banks_client).await;
        self.bob_obligation = Some(bob_obligation);
        let alice_obligation = TestObligation::init(
            &mut self.banks_client,
            lending_market,
            &Keypair::new(),
            &self.payer,
        )
        .await
        .unwrap();
        alice_obligation
            .validate_state(&mut self.banks_client)
            .await;
        let alice_obligation_state = alice_obligation.get_state(&mut self.banks_client).await;
        msg!("alice_obligation_state: {:#?}", alice_obligation_state);
        self.alice_obligation = Some(alice_obligation);
    }
}
