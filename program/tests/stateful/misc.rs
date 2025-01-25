use {
    super::IntegrationTest,
    solana_sdk::{instruction::InstructionError, msg, transaction::TransactionError},
    spl_token_lending::error::LendingError,
};

impl IntegrationTest {
    pub async fn go_to_slot(&mut self, slot: u64) {
        self.test_context.warp_to_slot(slot).unwrap();
    }

    pub async fn alice_borrow_sol_without_collateral(&self) {
        let alice_borrower = self.borrowers.get("alice").unwrap();
        let result = self
            .borrow_obligation_liquidity("sol", alice_borrower, None, None)
            .await;
        assert_eq!(
            result.unwrap_err().unwrap(),
            TransactionError::InstructionError(
                1,
                InstructionError::Custom(LendingError::ObligationDepositsEmpty as u32)
            )
        );
    }

    pub async fn alice_borrow_sol_with_usdc_collateral(&mut self) {
        let alice_borrower = self.borrowers.get("alice").unwrap();
        let result = self
            .borrow_obligation_liquidity("sol", alice_borrower, None, None)
            .await;
        assert!(result.is_ok());
    }
    pub async fn alice_repay_sol_to_obligation(&mut self) {
        let result = self.repay_obligation_liquidity("alice", "sol", 1).await;
        assert!(result.is_ok());
    }
}

#[macro_export]
macro_rules! sign_and_execute {
  ($self:expr, $transaction:expr $(, $signer:expr)* $(,)?) => {{
      $transaction.sign(
          &[&$self.test_context.payer $(, $signer)*],
          $self.test_context.last_blockhash,
      );

      $self.test_context
          .banks_client
          .process_transaction($transaction)
          .await
  }};
}
