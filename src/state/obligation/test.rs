/// Initialize an obligation

#[cfg(test)]
mod tests {
    use {
        crate::{
            error::LendingError,
            math::{Decimal, TryAdd, WAD},
            state::{Obligation, ObligationLiquidity},
        },
        proptest::prelude::*,
    };
    const MAX_COMPOUNDED_INTEREST: u64 = 100; // 10,000%
    #[test]
    fn obligation_accrue_interest_failure() {
        assert_eq!(
            ObligationLiquidity {
                cumulative_borrow_rate_wads: Decimal::zero(),
                ..ObligationLiquidity::default()
            }
            .accrue_interest(Decimal::one()),
            Err(LendingError::MathOverflow.into())
        );
        assert_eq!(
            ObligationLiquidity {
                cumulative_borrow_rate_wads: Decimal::from(2u64),
                ..ObligationLiquidity::default()
            }
            .accrue_interest(Decimal::one()),
            Err(LendingError::NegativeInterestRate.into())
        );
        assert_eq!(
            ObligationLiquidity {
                cumulative_borrow_rate_wads: Decimal::one(),
                borrowed_amount_wads: Decimal::from(u64::MAX),
                ..ObligationLiquidity::default()
            }
            .accrue_interest(Decimal::from(10 * MAX_COMPOUNDED_INTEREST)),
            Err(LendingError::MathOverflow.into())
        );
    }
    // Creates rates (r1, r2) where 0 < r1 <= r2 <= 100*r1
    prop_compose! {
        fn cumulative_rates()(rate in 1..=u128::MAX)(
            current_rate in Just(rate),
            max_new_rate in rate..=rate.saturating_mul(MAX_COMPOUNDED_INTEREST as u128),
        ) -> (u128, u128) {
            (current_rate, max_new_rate)
        }
    }
    const MAX_BORROWED: u128 = u64::MAX as u128 * WAD as u128;
    prop_compose! {
        fn repay_partial_amounts()(amount in 1..=u64::MAX)(
            repay_amount in Just(WAD as u128 * amount as u128),
            borrowed_amount in (WAD as u128 * amount as u128 + 1)..=MAX_BORROWED,
        ) -> (u128, u128) {
            (repay_amount, borrowed_amount)
        }
    }
    // Creates liquidity amounts (repay, borrow) where repay >= borrow
    prop_compose! {
      fn repay_full_amounts()(amount in 1..=u64::MAX)(
          repay_amount in Just(WAD as u128 * amount as u128),
      ) -> (u128, u128) {
          (repay_amount, repay_amount)
      }
    }
    proptest! {
      #[test]
      fn repay_partial(
        (repay_amount, borrowed_amount) in repay_partial_amounts(),
      ){
        let borrowed_amount_wads = Decimal::from_scaled_val(borrowed_amount);
        let repay_amount_wads = Decimal::from_scaled_val(repay_amount);
        let mut obligation = Obligation {
          borrows: vec![ObligationLiquidity {
            borrowed_amount_wads,
            ..ObligationLiquidity::default()
          }],
          ..Obligation::default()
        };
       let _ = obligation.repay(repay_amount_wads, 0)?;
        assert!(obligation.borrows[0].borrowed_amount_wads < borrowed_amount_wads);
        assert!(obligation.borrows[0].borrowed_amount_wads > Decimal::zero());
      }
      #[test]
      fn repay_full(
          (repay_amount, borrowed_amount) in repay_full_amounts(),
      ) {
          let borrowed_amount_wads = Decimal::from_scaled_val(borrowed_amount);
          let repay_amount_wads = Decimal::from_scaled_val(repay_amount);
          let mut obligation = Obligation {
              borrows: vec![ObligationLiquidity {
                  borrowed_amount_wads,
                  ..ObligationLiquidity::default()
              }],
              ..Obligation::default()
          };

         let _ = obligation.repay(repay_amount_wads, 0)?;
          assert_eq!(obligation.borrows.len(), 0);
      }
      #[test]
        fn accrue_interest(
            (current_borrow_rate, new_borrow_rate) in cumulative_rates(),
            borrowed_amount in 0..=u64::MAX,
        ) {
            let cumulative_borrow_rate_wads = Decimal::one().try_add(Decimal::from_scaled_val(current_borrow_rate))?;
            let borrowed_amount_wads = Decimal::from(borrowed_amount);
            let mut liquidity = ObligationLiquidity {
                cumulative_borrow_rate_wads,
                borrowed_amount_wads,
                ..ObligationLiquidity::default()
            };

            let next_cumulative_borrow_rate = Decimal::one().try_add(Decimal::from_scaled_val(new_borrow_rate))?;
            let _ = liquidity.accrue_interest(next_cumulative_borrow_rate)?;

            if next_cumulative_borrow_rate > cumulative_borrow_rate_wads {
                assert!(liquidity.borrowed_amount_wads > borrowed_amount_wads);
            } else {
                assert!(liquidity.borrowed_amount_wads == borrowed_amount_wads);
            }
        }
    }
}
