use {
    crate::helpers::{
        add_sol_oracle, add_usdc_mint, add_usdc_oracle, get_token_balance, TestLendingMarket,
        TestMint, TestObligation, TestOracle, TestReserve,
    },
    solana_program_test::{processor, BanksClient, ProgramTest, ProgramTestContext},
    solana_sdk::{pubkey::Pubkey, signature::Keypair},
    spl_token_lending::processor::process_instruction,
    std::collections::HashMap,
};

pub(crate) const BORROWER_NAME_LIST: [&str; 2] = ["alice", "bob"];
pub(crate) const CURRENCY_TYPE: [&str; 2] = ["usdc", "sol"];

pub(crate) struct IntegrationTest {
    pub test_context: ProgramTestContext,
    pub oracles: HashMap<&'static str, TestOracle>,
    pub usdc_mint: TestMint,
    pub lending_market: Option<TestLendingMarket>,
    pub user_accounts_owner: Keypair,
    pub reserves: HashMap<&'static str, TestReserve>,
    pub borrowers: HashMap<&'static str, Borrower>,
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

        let oracles = HashMap::from([("sol", sol_oracle), ("usdc", usdc_oracle)]);

        IntegrationTest {
            test_context: test.start_with_context().await,
            oracles,
            usdc_mint,
            lending_market: None,
            user_accounts_owner: Keypair::new(),
            reserves: HashMap::default(),
            borrowers,
        }
    }
    pub async fn get_borrower_balance(
        &self,
        borrower: &str,
        currency: &str,
        is_collateral_account: bool,
    ) -> u64 {
        get_token_balance(
            &self.test_context.banks_client,
            self.borrowers
                .get(borrower)
                .unwrap()
                .accounts
                .get(currency)
                .unwrap()
                .get_account(is_collateral_account),
        )
        .await
    }
}
#[derive(Debug, Clone, Copy)]
pub struct BorrowerAccounts {
    pub token_account: Pubkey,
    pub collateral_account: Pubkey,
}

impl BorrowerAccounts {
    pub fn get_account(&self, is_collateral_account: bool) -> Pubkey {
        if is_collateral_account {
            self.collateral_account
        } else {
            self.token_account
        }
    }
}

pub(crate) struct Borrower {
    #[allow(dead_code)]
    pub name: &'static str,
    pub obligation: Option<TestObligation>,
    pub keypair: Keypair, /* usually used as owner for entities like the obligation, token
                           * accounts of this u, etc. */
    pub user_transfer_authority: Keypair, //showcase to delegate the authority of the owner
    pub accounts: HashMap<&'static str, BorrowerAccounts>,
}
