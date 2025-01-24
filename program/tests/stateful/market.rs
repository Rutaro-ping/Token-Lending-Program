use super::IntegrationTest;
use crate::{
    helpers::{MarketInitParams, TestLendingMarket},
    sign_and_execute,
};
use solana_sdk::{signature::Keypair, signer::Signer, transaction::Transaction};
use spl_token_lending::instruction::builder::set_lending_market_owner;

impl IntegrationTest {
    //if market_owner is not provided, it will use the default market owner loaded from the fixture file.
    pub async fn create_market(&mut self, market_owner: Option<Keypair>) {
        let temp_lending_market_keypair = Keypair::new();
        let test_lending_market = TestLendingMarket::init(
            &self.test_context.banks_client,
            &self.test_context.payer,
            Some(MarketInitParams {
                lending_market_owner: market_owner,
                lending_market_keypair: Some(temp_lending_market_keypair),
                ..Default::default()
            }),
        )
        .await;
        let market = test_lending_market
            .get_state(&self.test_context.banks_client)
            .await;
        assert_eq!(market.owner, test_lending_market.owner.pubkey());
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

        sign_and_execute!(self, transaction, &lending_market.owner).unwrap();

        let market = lending_market
            .get_state(&mut self.test_context.banks_client)
            .await;
        assert_eq!(market.owner, market_owner.pubkey());
        assert_ne!(lending_market.owner.pubkey(), market.owner);
        //update the owner of the lending market after it updated onchain.
        lending_market.owner = market_owner;
    }
}
