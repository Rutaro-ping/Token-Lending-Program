use std::collections::HashMap;

use solana_program_test::{processor, ProgramTest, ProgramTestContext};
use solana_sdk::{native_token::LAMPORTS_PER_SOL, pubkey::Pubkey, signature::Keypair};
use spl_token_lending::processor::process_instruction;

use crate::helpers::{
    add_sol_oracle, add_usdc_mint, add_usdc_oracle, TestLendingMarket, TestMint, TestObligation,
    TestOracle, TestReserve, FRACTIONAL_TO_USDC,
};

pub(crate) const INIT_RESERVE_SOL_AMOUNT: u64 = 10 * LAMPORTS_PER_SOL;
pub(crate) const INIT_RESERVE_USDC_AMOUNT: u64 = 10 * FRACTIONAL_TO_USDC;
pub(crate) const BORROWER_NAME_LIST: [&str; 2] = ["alice", "bob"];
pub(crate) const CURRENCY_TYPE: [&str; 2] = ["usdc", "sol"];

pub(crate) struct IntegrationTest {
    pub test_context: ProgramTestContext,
    pub oracles: HashMap<&'static str, TestOracle>,
    pub usdc_mint: TestMint,
    pub lending_market: Option<TestLendingMarket>,
    pub user_accounts_owner: Keypair,
    pub init_sol_user_liquidity_account: Pubkey,
    pub init_usdc_user_liquidity_account: Pubkey,
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
            init_sol_user_liquidity_account: Pubkey::default(),
            init_usdc_user_liquidity_account: Pubkey::default(),
            reserves: HashMap::default(),
            borrowers,
        }
    }
}
pub(crate) struct BorrowerAccounts {
    pub token_account: Pubkey,
    pub collateral_account: Pubkey,
}

pub(crate) struct Borrower {
    pub name: &'static str,
    pub obligation: Option<TestObligation>,
    pub keypair: Keypair, /* usually used as owner for entities like the obligation, token accounts
                           * of this u, etc. */
    pub user_transfer_authority: Keypair, //showcase to delegate the authority of the owner
    pub accounts: HashMap<&'static str, BorrowerAccounts>,
}
