#![allow(dead_code)]

use solana_program_test::BanksClient;
use solana_sdk::msg;
use {
    assert_matches::*,
    solana_sdk::{
        program_pack::Pack,
        pubkey::Pubkey,
        signature::{read_keypair_file, Keypair},
        signer::Signer,
        system_instruction::create_account,
        transaction::Transaction,
    },
    spl_token_lending::{instruction::init_lending_market, state::LendingMarket},
};

pub const QUOTE_CURRENCY: [u8; 32] =
    *b"USD\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";

pub struct TestLendingMarket {
    pub pubkey: Pubkey,
    pub owner: Keypair,
    pub authority: Pubkey,
    pub quote_currency: [u8; 32],
    pub oracle_program_id: Pubkey,
}

impl TestLendingMarket {
    pub async fn init(banks_client: &mut BanksClient, payer: &Keypair) -> Self {
        let lending_market_owner =
            read_keypair_file("tests/fixtures/lending_market_owner.json").unwrap();
        let oracle_program_id = read_keypair_file("tests/fixtures/oracle_program_id.json")
            .unwrap()
            .pubkey();
        let lending_market_keypair = Keypair::new();
        let lending_market_pubkey = lending_market_keypair.pubkey();
        let (lending_market_authority, _bump_seed) = Pubkey::find_program_address(
            &[&lending_market_pubkey.to_bytes()[..32]],
            &spl_token_lending::id(),
        );
        let rent = banks_client.get_rent().await.unwrap();
        let mut transaction = Transaction::new_with_payer(
            &[
                create_account(
                    &payer.pubkey(),
                    &lending_market_pubkey,
                    rent.minimum_balance(LendingMarket::LEN),
                    LendingMarket::LEN as u64,
                    &spl_token_lending::id(),
                ),
                init_lending_market(
                    spl_token_lending::id(),
                    lending_market_owner.pubkey(),
                    QUOTE_CURRENCY,
                    lending_market_pubkey,
                    oracle_program_id,
                ),
            ],
            Some(&payer.pubkey()),
        );
        let recent_blockhash = banks_client.get_latest_blockhash().await.unwrap();
        transaction.sign(&[payer, &lending_market_keypair], recent_blockhash);
        assert_matches!(banks_client.process_transaction(transaction).await, Ok(()));
        TestLendingMarket {
            owner: lending_market_owner,
            pubkey: lending_market_pubkey,
            authority: lending_market_authority,
            quote_currency: QUOTE_CURRENCY,
            oracle_program_id,
        }
    }

    pub async fn get_state(&self, banks_client: &mut BanksClient) -> LendingMarket {
        let lending_market_account = banks_client
            .get_account(self.pubkey)
            .await
            .unwrap()
            .unwrap();
        match LendingMarket::unpack(&lending_market_account.data) {
            Ok(lending_market) => lending_market,
            Err(e) => {
                panic!("Failed to unpack lending market: {:?}", e.to_string());
            }
        }
    }
    pub async fn validate_state(&self, banks_client: &mut BanksClient) {
        let lending_market = self.get_state(banks_client).await;
        assert_eq!(lending_market.owner, self.owner.pubkey());
        assert_eq!(lending_market.quote_currency, QUOTE_CURRENCY);
        assert_eq!(lending_market.oracle_program_id, self.oracle_program_id);
    }
}
