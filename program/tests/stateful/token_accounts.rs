use std::collections::HashMap;

use super::*;
use crate::{
    helpers::{
        create_and_mint_to_token_account, create_token_account, get_state, get_token_balance,
        TestMint, FRACTIONAL_TO_USDC, LAMPORTS_TO_SOL,
    },
    sign_and_execute,
};
use solana_program_test::BanksClient;
use solana_sdk::{
    native_token::LAMPORTS_PER_SOL, program_option::COption, program_pack::Pack, pubkey::Pubkey,
    signature::Keypair, signer::Signer, system_instruction, transaction::Transaction,
};
use spl_token::{
    instruction::{mint_to, sync_native},
    state::Account as TokenAccount,
};

impl IntegrationTest {
    pub async fn open_accounts(&mut self) {
        const OPEN_ACCOUNT_AMOUNT: u64 = 1;

        async fn setup_accounts(
            banks_client: &BanksClient,
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
                &self.test_context.banks_client,
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

    pub async fn create_init_user_supply_accounts(&mut self) {
        let init_sol_user_liquidity_account = create_and_mint_to_token_account(
            &self.test_context.banks_client,
            spl_token::native_mint::id(),
            None,
            &self.test_context.payer,
            self.user_accounts_owner.pubkey(),
            INIT_RESERVE_SOL_AMOUNT,
        )
        .await;

        let init_usdc_user_liquidity_account = create_and_mint_to_token_account(
            &self.test_context.banks_client,
            self.usdc_mint.pubkey,
            Some(&self.usdc_mint.authority),
            &self.test_context.payer,
            self.user_accounts_owner.pubkey(),
            INIT_RESERVE_USDC_AMOUNT,
        )
        .await;

        let sol_balance = get_token_balance(
            &self.test_context.banks_client,
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
            &self.test_context.banks_client,
            init_usdc_user_liquidity_account,
        )
        .await;
        assert_eq!(usdc_balance, INIT_RESERVE_USDC_AMOUNT);
        self.init_sol_user_liquidity_account = init_sol_user_liquidity_account;
        self.init_usdc_user_liquidity_account = init_usdc_user_liquidity_account;
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
                &self.test_context.banks_client,
            )
            .await
            .unwrap();
            assert!(sol_account.amount >= TOP_UP_AMOUNT * LAMPORTS_PER_SOL);

            let usdc_account = get_state::<TokenAccount>(
                borrower.accounts.get("usdc").unwrap().token_account,
                &self.test_context.banks_client,
            )
            .await
            .unwrap();
            assert!(usdc_account.amount >= TOP_UP_AMOUNT * FRACTIONAL_TO_USDC);
        }
    }
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
        let result = sign_and_execute!(self, transaction, &self.test_context.payer);
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

        let result = sign_and_execute!(
            self,
            transaction,
            &self.test_context.payer,
            &self.usdc_mint.authority
        );
        assert!(result.is_ok());
    }
}
