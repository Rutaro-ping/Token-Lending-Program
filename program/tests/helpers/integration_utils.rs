use std::collections::HashMap;

use solana_program_test::{
    processor, BanksClient, BanksClientError, ProgramTest, ProgramTestContext,
};
use solana_sdk::{
    instruction::InstructionError,
    msg,
    native_token::LAMPORTS_PER_SOL,
    program_option::COption,
    program_pack::Pack,
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    system_instruction,
    transaction::{Transaction, TransactionError},
};
use spl_token::{
    instruction::{approve, mint_to, sync_native},
    state::{Account as TokenAccount, Mint},
};
use spl_token_lending::{
    error::LendingError,
    instruction::builder::{
        borrow_obligation_liquidity, deposit_obligation_collateral, deposit_reserve_liquidity,
        refresh_obligation, refresh_reserve, set_lending_market_owner,
    },
    processor::process_instruction,
};

use crate::helpers::{get_token_balance, MarketInitParams, LAMPORTS_TO_SOL};

use super::{
    add_sol_oracle, add_usdc_mint, add_usdc_oracle, create_and_mint_to_token_account,
    create_token_account, get_state, TestLendingMarket, TestMint, TestObligation, TestOracle,
    TestReserve, FRACTIONAL_TO_USDC, TEST_RESERVE_CONFIG,
};

pub(crate) const INIT_RESERVE_SOL_AMOUNT: u64 = 10 * LAMPORTS_PER_SOL;
pub(crate) const INIT_RESERVE_USDC_AMOUNT: u64 = 10 * FRACTIONAL_TO_USDC;
pub(crate) const BORROWER_NAME_LIST: [&str; 2] = ["alice", "bob"];
pub(crate) const CURRENCY_TYPE: [&str; 2] = ["usd", "sol"];

pub(crate) struct BorrowerAccounts {
    pub token_account: Pubkey,
    pub collateral_account: Pubkey,
}

pub(crate) struct Borrower {
    pub name: &'static str,
    pub obligation: Option<TestObligation>,
    pub keypair: Keypair, // usually used as owner for entities like obligation, token accounts of this u, etc.
    pub user_transfer_authority: Keypair, //showcase to delegate the authority of the owner
    pub accounts: HashMap<&'static str, BorrowerAccounts>,
}
pub(crate) struct IntegrationTest {
    test_context: ProgramTestContext,
    sol_oracle: TestOracle,
    usdc_oracle: TestOracle,
    usdc_mint: TestMint,
    lending_market: Option<TestLendingMarket>,
    user_accounts_owner: Keypair,
    init_sol_user_liquidity_account: Pubkey,
    init_usdc_user_liquidity_account: Pubkey,
    sol_reserve: Option<TestReserve>,
    usdc_reserve: Option<TestReserve>,
    borrowers: HashMap<&'static str, Borrower>,
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

        let borrowers = BORROWER_NAME_LIST
            .iter()
            .map(|name| {
                (
                    name.to_owned(),
                    Borrower {
                        name: name.to_owned(),
                        obligation: None,
                        keypair: Keypair::new(),
                        user_transfer_authority: Keypair::new(),
                        accounts: HashMap::default(),
                    },
                )
            })
            .collect::<HashMap<&str, Borrower>>();

        IntegrationTest {
            test_context: test.start_with_context().await,
            sol_oracle,
            usdc_oracle,
            usdc_mint,
            lending_market: None,
            user_accounts_owner: Keypair::new(),
            //below two accounts used to be init supply for reserves,
            // one for sol reserve, one for usdc reserve
            init_sol_user_liquidity_account: Pubkey::default(),
            init_usdc_user_liquidity_account: Pubkey::default(),
            sol_reserve: None,
            usdc_reserve: None,
            borrowers,
        }
    }

    pub async fn open_accounts(&mut self) {
        const OPEN_ACCOUNT_AMOUNT: u64 = 1;

        async fn setup_accounts(
            banks_client: &mut BanksClient,
            payer: &Keypair,
            borrower: &mut Borrower,
            usdc_mint: &TestMint,
        ) -> (Pubkey, Pubkey) {
            let usdc_account = create_and_mint_to_token_account(
                banks_client,
                usdc_mint.pubkey,
                Some(&usdc_mint.authority),
                payer,
                borrower.keypair.pubkey(),
                OPEN_ACCOUNT_AMOUNT,
            )
            .await;

            let sol_account = create_and_mint_to_token_account(
                banks_client,
                spl_token::native_mint::id(),
                None,
                payer,
                borrower.keypair.pubkey(),
                OPEN_ACCOUNT_AMOUNT,
            )
            .await;
            let usdc_account_info = get_state::<TokenAccount>(usdc_account, banks_client)
                .await
                .unwrap();
            assert_eq!(usdc_account_info.amount, OPEN_ACCOUNT_AMOUNT);
            assert_eq!(usdc_account_info.mint, usdc_mint.pubkey);
            assert_eq!(usdc_account_info.owner, borrower.keypair.pubkey());
            assert_eq!(usdc_account_info.is_native, COption::None);
            let sol_account_info = get_state::<TokenAccount>(sol_account, banks_client)
                .await
                .unwrap();
            assert_eq!(sol_account_info.amount, OPEN_ACCOUNT_AMOUNT);
            assert_eq!(sol_account_info.mint, spl_token::native_mint::id());
            assert_eq!(sol_account_info.owner, borrower.keypair.pubkey());
            assert_eq!(sol_account_info.is_native, COption::Some(2039280)); //which the rent-exempt amount
            (usdc_account, sol_account)
        }
        let sol_colletaral_mint = self.sol_reserve.as_ref().unwrap().collateral_mint_pubkey;
        let usdc_colletaral_mint = self.usdc_reserve.as_ref().unwrap().collateral_mint_pubkey;
        for name in ["alice", "bob"] {
            let borrower = self.borrowers.get_mut(name).unwrap();
            let (usdc_account, sol_account) = setup_accounts(
                &mut self.test_context.banks_client,
                &self.test_context.payer,
                borrower,
                &self.usdc_mint,
            )
            .await;
            let sol_collateral_account = create_token_account(
                &self.test_context.banks_client,
                sol_colletaral_mint,
                &self.test_context.payer,
                Some(borrower.keypair.pubkey()),
                None,
            )
            .await;
            let usdc_collateral_account = create_token_account(
                &self.test_context.banks_client,
                usdc_colletaral_mint,
                &self.test_context.payer,
                Some(borrower.keypair.pubkey()),
                None,
            )
            .await;
            borrower.accounts = HashMap::from([
                (
                    "usdc",
                    BorrowerAccounts {
                        token_account: usdc_account,
                        collateral_account: usdc_collateral_account,
                    },
                ),
                (
                    "sol",
                    BorrowerAccounts {
                        token_account: sol_account,
                        collateral_account: sol_collateral_account,
                    },
                ),
            ]);
        }
    }

    pub async fn create_market(&mut self) {
        let temp_lending_market_owner = Keypair::new();
        let temp_lending_market_keypair = Keypair::new();
        let test_lending_market = TestLendingMarket::init(
            &mut self.test_context.banks_client,
            &self.test_context.payer,
            Some(MarketInitParams {
                lending_market_owner: Some(temp_lending_market_owner),
                lending_market_keypair: Some(temp_lending_market_keypair),
                ..Default::default()
            }),
        )
        .await;
        let market = test_lending_market
            .get_state(&mut self.test_context.banks_client)
            .await;
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
            Some(&self.test_context.payer.pubkey()),
        );
        // //update the owner of the lending market after it updated onchain.

        transaction.sign(
            &[&self.test_context.payer, &lending_market.owner],
            self.test_context.last_blockhash,
        );
        self.test_context
            .banks_client
            .process_transaction(transaction)
            .await
            .map_err(|e| e.unwrap())
            .unwrap();

        let market = lending_market
            .get_state(&mut self.test_context.banks_client)
            .await;
        assert_eq!(market.owner, market_owner.pubkey());
        assert_ne!(lending_market.owner.pubkey(), market.owner);
        //update the owner of the lending market after it updated onchain.
        lending_market.owner = market_owner;
    }

    pub async fn create_init_user_supply_accounts(&mut self) {
        let init_sol_user_liquidity_account = create_and_mint_to_token_account(
            &mut self.test_context.banks_client,
            spl_token::native_mint::id(),
            None,
            &self.test_context.payer,
            self.user_accounts_owner.pubkey(),
            INIT_RESERVE_SOL_AMOUNT,
        )
        .await;

        let init_usdc_user_liquidity_account = create_and_mint_to_token_account(
            &mut self.test_context.banks_client,
            self.usdc_mint.pubkey,
            Some(&self.usdc_mint.authority),
            &self.test_context.payer,
            self.user_accounts_owner.pubkey(),
            INIT_RESERVE_USDC_AMOUNT,
        )
        .await;

        let sol_balance = get_token_balance(
            &mut self.test_context.banks_client,
            init_sol_user_liquidity_account,
        )
        .await;
        let sol_balance_lamports = self
            .test_context
            .banks_client
            .get_balance(init_sol_user_liquidity_account)
            .await
            .unwrap();
        assert_eq!(sol_balance, INIT_RESERVE_SOL_AMOUNT);
        let rent = self.test_context.banks_client.get_rent().await.unwrap();
        let lamports = rent.minimum_balance(TokenAccount::LEN) + INIT_RESERVE_SOL_AMOUNT;
        //native SOL token account total lamports = rent + init_sol_amount
        assert_eq!(sol_balance_lamports, lamports);

        let usdc_balance = get_token_balance(
            &mut self.test_context.banks_client,
            init_usdc_user_liquidity_account,
        )
        .await;
        assert_eq!(usdc_balance, INIT_RESERVE_USDC_AMOUNT);
        self.init_sol_user_liquidity_account = init_sol_user_liquidity_account;
        self.init_usdc_user_liquidity_account = init_usdc_user_liquidity_account;
    }

    pub async fn create_reserves(&mut self) {
        let lending_market = self.lending_market.as_ref().unwrap();
        let sol_reserve = TestReserve::init(
            "sol".to_owned(),
            &mut self.test_context.banks_client,
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
            &mut self.test_context.banks_client,
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

    pub async fn refresh_reserves(&mut self) {
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
        transaction.sign(
            &[&self.test_context.payer],
            self.test_context.last_blockhash,
        );
        assert!(self
            .test_context
            .banks_client
            .process_transaction(transaction)
            .await
            .is_ok());
    }

    pub async fn create_obligations(&mut self) {
        let borrower_bob = self.borrowers.get_mut("bob").unwrap();
        let lending_market = self.lending_market.as_ref().unwrap();
        let bob_obligation = TestObligation::init(
            &mut self.test_context.banks_client,
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
            &mut self.test_context.banks_client,
            lending_market,
            &borrower_alice.keypair,
            &self.test_context.payer,
        )
        .await
        .unwrap();
        alice_obligation
            .validate_state(&mut self.test_context.banks_client)
            .await;
        borrower_alice.obligation = Some(alice_obligation);
    }

    pub async fn alice_borrow_sol_without_collateral(&self) {
        let reserve = self.sol_reserve.as_ref().unwrap();
        let alice_borrower = self.borrowers.get("alice").unwrap();
        let result = self
            .borrow_obligation_liquidity(reserve, alice_borrower)
            .await;
        assert_eq!(
            result.unwrap_err().unwrap(),
            TransactionError::InstructionError(
                2,
                InstructionError::Custom(LendingError::ObligationDepositsEmpty as u32)
            )
        );
    }
    pub async fn alice_deposit_usdc_reserve(&self, amount: u64) {
        let usdc_reserve = self.usdc_reserve.as_ref().unwrap();
        let alice_borrower = self.borrowers.get("alice").unwrap();
        self.deposit_reserve_liquidity(
            usdc_reserve,
            alice_borrower,
            amount,
            &alice_borrower.user_transfer_authority,
            "usdc",
        )
        .await;
    }
    pub async fn alice_deposit_usdc_collateral_to_obligations(&self, amount: u64) {
        let usdc_reserve = self.usdc_reserve.as_ref().unwrap();
        let alice_borrower = self.borrowers.get("alice").unwrap();
        self.deposit_obligations(alice_borrower, usdc_reserve, "usdc", amount)
            .await;
    }
    pub async fn alice_deposit_sol_collateral(&mut self) {
        let sol_reserve = self.sol_reserve.as_ref().unwrap();
        let alice_borrower = self.borrowers.get("alice").unwrap();
        let alice_obligation = alice_borrower.obligation.as_ref().unwrap();
        let token_account = get_state::<TokenAccount>(
            sol_reserve.user_collateral_pubkey,
            &mut self.test_context.banks_client,
        )
        .await
        .unwrap();
        msg!("token_account: {:#?}", token_account);
        let mint = get_state::<Mint>(token_account.mint, &mut self.test_context.banks_client)
            .await
            .unwrap();
        msg!("mint: {:#?}", mint);
        const SOL_DEPOSIT_AMOUNT_LAMPORTS: u64 = 1000 * LAMPORTS_PER_SOL;
        let mut transaction = Transaction::new_with_payer(
            &[deposit_obligation_collateral(
                spl_token_lending::id(),
                SOL_DEPOSIT_AMOUNT_LAMPORTS,
                sol_reserve.user_collateral_pubkey,
                sol_reserve.collateral_supply_pubkey,
                sol_reserve.pubkey,
                alice_obligation.pubkey,
                self.lending_market.as_ref().unwrap().pubkey,
                alice_obligation.owner,
                self.user_accounts_owner.pubkey(),
            )],
            Some(&self.test_context.payer.pubkey()),
        );
        transaction.sign(
            &[
                &self.test_context.payer,
                &self.user_accounts_owner,
                &alice_borrower.keypair,
            ],
            self.test_context.last_blockhash,
        );
        self.test_context
            .banks_client
            .process_transaction(transaction)
            .await
            .unwrap();
    }

    pub async fn top_up_token_accounts(&mut self) {
        const TOP_UP_AMOUNT: u64 = 1000;
        for name in BORROWER_NAME_LIST {
            let borrower = self.borrowers.get(name).unwrap();
            self.airdrop_native_sol(
                TOP_UP_AMOUNT,
                borrower.accounts.get("sol").unwrap().token_account,
            )
            .await;

            self.airdrop_usdc(
                TOP_UP_AMOUNT,
                borrower.accounts.get("usdc").unwrap().token_account,
            )
            .await;

            let sol_account = get_state::<TokenAccount>(
                borrower.accounts.get("sol").unwrap().token_account,
                &mut self.test_context.banks_client,
            )
            .await
            .unwrap();
            assert!(sol_account.amount >= TOP_UP_AMOUNT * LAMPORTS_PER_SOL);

            let usdc_account = get_state::<TokenAccount>(
                borrower.accounts.get("usdc").unwrap().token_account,
                &mut self.test_context.banks_client,
            )
            .await
            .unwrap();
            assert!(usdc_account.amount >= TOP_UP_AMOUNT * FRACTIONAL_TO_USDC);
        }
    }
    async fn borrow_obligation_liquidity(
        &self,
        reserve: &TestReserve,
        borrower: &Borrower,
    ) -> Result<(), BanksClientError> {
        let obligation = borrower.obligation.as_ref().unwrap();
        let lending_market = self.lending_market.as_ref().unwrap();
        let mut transaction = Transaction::new_with_payer(
            &[
                refresh_reserve(
                    spl_token_lending::id(),
                    reserve.pubkey,
                    self.sol_oracle.price_pubkey,
                ),
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
        transaction.sign(
            &[&self.test_context.payer, &borrower.keypair],
            self.test_context.last_blockhash,
        );
        self.test_context
            .banks_client
            .process_transaction(transaction)
            .await
    }
    //provide airdrop for native SOL
    async fn airdrop_native_sol(&self, amount: u64, to_account: Pubkey) {
        //implement transfer lamports from payer, then sync the account
        let mut transaction = Transaction::new_with_payer(
            &[
                system_instruction::transfer(
                    &self.test_context.payer.pubkey(),
                    &to_account,
                    amount * LAMPORTS_TO_SOL,
                ),
                sync_native(&spl_token::id(), &to_account).unwrap(),
            ],
            Some(&self.test_context.payer.pubkey()),
        );
        transaction.sign(
            &[&self.test_context.payer],
            self.test_context.last_blockhash,
        );
        let result = self
            .test_context
            .banks_client
            .process_transaction(transaction)
            .await;
        assert!(result.is_ok());
    }

    //provide airdrop for USDC
    pub async fn airdrop_usdc(&self, amount: u64, to_account: Pubkey) {
        let mut transaction = Transaction::new_with_payer(
            &[mint_to(
                &spl_token::id(),
                &self.usdc_mint.pubkey,
                &to_account,
                &self.usdc_mint.authority.pubkey(),
                &[],
                amount * FRACTIONAL_TO_USDC,
            )
            .unwrap()],
            Some(&self.test_context.payer.pubkey()),
        );
        transaction.sign(
            &[&self.test_context.payer, &self.usdc_mint.authority],
            self.test_context.last_blockhash,
        );
        let result = self
            .test_context
            .banks_client
            .process_transaction(transaction)
            .await;
        assert!(result.is_ok());
    }

    async fn deposit_reserve_liquidity(
        &self,
        reserve: &TestReserve,
        borrower: &Borrower,
        liquidity_amount: u64,
        user_transfer_authority: &Keypair,
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
        let recent_blockhash = self
            .test_context
            .banks_client
            .get_latest_blockhash()
            .await
            .unwrap();
        transaction.sign(
            &[payer, &borrower.keypair, &user_transfer_authority],
            recent_blockhash,
        );
        self.test_context
            .banks_client
            .process_transaction(transaction)
            .await
            .unwrap();
    }
    pub async fn deposit_obligations(
        &self,
        borrower: &Borrower,
        reserve: &TestReserve,
        currency: &str, //"sol" or "usdc"
        collateral_amount: u64,
    ) {
        let obligation = borrower.obligation.as_ref().unwrap();
        let lending_market = self.lending_market.as_ref().unwrap();
        let accounts = borrower.accounts.get(currency).unwrap();
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
        let recent_blockhash = self
            .test_context
            .banks_client
            .get_latest_blockhash()
            .await
            .unwrap();
        transaction.sign(
            &[
                &self.test_context.payer,
                &borrower.keypair,
                &borrower.user_transfer_authority,
            ],
            recent_blockhash,
        );
        self.test_context
            .banks_client
            .process_transaction(transaction)
            .await
            .unwrap();
    }
}
