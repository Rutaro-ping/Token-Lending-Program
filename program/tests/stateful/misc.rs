use solana_sdk::{instruction::InstructionError, transaction::TransactionError};
use spl_token_lending::error::LendingError;

use super::IntegrationTest;

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
                2,
                InstructionError::Custom(LendingError::ObligationDepositsEmpty as u32)
            )
        );
    }

    pub async fn alice_deposit_usdc_collateral_to_obligations(&mut self, amount: u64) {
        self.refresh_reserves().await;
        let alice_borrower = self.borrowers.get("alice").unwrap();
        self.deposit_obligations(alice_borrower, "usdc", amount)
            .await;
    }
    pub async fn alice_borrow_sol_with_collateral(&mut self) {
        let alice_borrower = self.borrowers.get("alice").unwrap();
        let result = self
            .borrow_obligation_liquidity("sol", alice_borrower, Some(100), None)
            .await;

        assert_eq!(
            result.unwrap_err().unwrap(),
            TransactionError::InstructionError(
                2,
                InstructionError::Custom(LendingError::BorrowTooLarge as u32)
            )
        );
    }
    pub async fn bob_deposit_sol_reserve(&self, amount: u64) {
        let bob_borrower = self.borrowers.get("bob").unwrap();
        self.deposit_reserve_liquidity(bob_borrower, amount, "sol")
            .await;
    }
    pub async fn bob_deposit_sol_collateral_to_obligations(&mut self, amount: u64) {
        let bob_borrower = self.borrowers.get("bob").unwrap();
        self.deposit_obligations(bob_borrower, "sol", amount).await;
    }
}

#[macro_export]
macro_rules! sign_and_execute {
  ($self:expr, $transaction:expr $(, $signer:expr)* $(,)?) => {{
      // let recent_blockhash = $self
      //     .test_context
      //     .banks_client
      //     .get_latest_blockhash()
      //     .await
      //     .unwrap();

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
