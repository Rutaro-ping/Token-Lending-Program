use {
    super::{last_update::LastUpdate, PROGRAM_VERSION},
    crate::{
        error::LendingError,
        math::{Decimal, Rate, TryDiv, TryMul, TrySub},
    },
    solana_program::{
        clock::Slot, entrypoint::ProgramResult, msg, program_error::ProgramError, pubkey::Pubkey,
    },
    std::cmp::Ordering,
};

/// Obligation liquidity state
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObligationLiquidity {
    /// Reserve liquidity is borrowed from
    pub borrow_reserve: Pubkey,
    /// Borrow rate used for calculating interest
    pub cumulative_borrow_rate_wads: Decimal,
    /// Amount of liquidity borrowed plus interest
    pub borrowed_amount_wads: Decimal,
    /// Liquidity market value in quote currency
    pub market_value: Decimal,
}

impl ObligationLiquidity {
    pub fn new(borrow_reserve: Pubkey) -> Self {
        Self {
            borrow_reserve,
            ..Default::default()
        }
    }
    /// Decrease borrowed liquidity
    pub fn repay(&mut self, settle_amount: Decimal) -> ProgramResult {
        self.borrowed_amount_wads = self.borrowed_amount_wads.try_sub(settle_amount)?;
        Ok(())
    }

    pub fn accrue_interest(&mut self, cumulative_borrow_rate_wads: Decimal) -> ProgramResult {
        println!(
            "compare result:{:?}",
            cumulative_borrow_rate_wads.cmp(&self.cumulative_borrow_rate_wads)
        );
        match cumulative_borrow_rate_wads.cmp(&self.cumulative_borrow_rate_wads) {
            Ordering::Less => {
                msg!("Interest rate cannot be negative");
                return Err(LendingError::NegativeInterestRate.into());
            }
            Ordering::Equal => Ok(()),
            Ordering::Greater => {
                let compounded_interest_rate: Rate = cumulative_borrow_rate_wads
                    .try_div(self.cumulative_borrow_rate_wads)?
                    .try_into()?;
                println!("compounded_interest_rate: {:?}", compounded_interest_rate);
                self.borrowed_amount_wads = self
                    .borrowed_amount_wads
                    .try_mul(compounded_interest_rate)?;
                self.cumulative_borrow_rate_wads = cumulative_borrow_rate_wads;
                Ok(())
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Obligation {
    /// Version of the struct
    pub version: u8,
    /// Last update to collateral, liquidity, or their market values
    pub last_update: LastUpdate,
    /// Lending market address
    pub lending_market: Pubkey,
    /// Owner authority which can borrow liquidity
    pub owner: Pubkey,
    /// Deposited collateral for the obligation, unique by deposit reserve
    /// address
    pub deposits: Vec<ObligationCollateral>,
    /// Borrowed liquidity for the obligation, unique by borrow reserve address
    pub borrows: Vec<ObligationLiquidity>,
    /// Market value of deposits
    pub deposited_value: Decimal,
    /// Market value of borrows
    pub borrowed_value: Decimal,
    /// The maximum borrow value at the weighted average loan to value ratio
    pub allowed_borrow_value: Decimal,
    /// The dangerous borrow value at the weighted average liquidation threshold
    pub unhealthy_borrow_value: Decimal,
}
impl Obligation {
    pub fn new(params: InitObligationParams) -> Self {
        let mut obligation = Self::default();
        Self::init(&mut obligation, params);
        obligation
    }
    /// Initialize an obligation
    pub fn init(&mut self, params: InitObligationParams) {
        self.version = PROGRAM_VERSION;
        self.last_update = LastUpdate::new(params.current_slot);
        self.lending_market = params.lending_market;
        self.owner = params.owner;
        self.deposits = params.deposits;
        self.borrows = params.borrows;
    }

    pub fn load_to_value(&self) -> Result<Decimal, ProgramError> {
        self.borrowed_value.try_div(self.deposited_value)
    }
    pub fn repay(&mut self, settle_amount: Decimal, liquidity_index: usize) -> ProgramResult {
        let liquidity = &mut self.borrows[liquidity_index];
        if settle_amount == liquidity.borrowed_amount_wads {
            self.borrows.remove(liquidity_index);
        } else {
            liquidity.repay(settle_amount);
        }
        Ok(())
    }
}

/// Initialize an obligation
pub struct InitObligationParams {
    /// Last update to collateral, liquidity, or their market values
    pub current_slot: Slot,
    /// Lending market address
    pub lending_market: Pubkey,
    /// Owner authority which can borrow liquidity
    pub owner: Pubkey,
    /// Deposited collateral for the obligation, unique by deposit reserve
    /// address
    pub deposits: Vec<ObligationCollateral>,
    /// Borrowed liquidity for the obligation, unique by borrow reserve address
    pub borrows: Vec<ObligationLiquidity>,
}

/// Obligation collateral state
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObligationCollateral {
    /// Reserve collateral is deposited to
    pub deposit_reserve: Pubkey,
    /// Amount of collateral deposited
    pub deposited_amount: u64,
    /// Collateral market value in quote currency
    pub market_value: Decimal,
}

impl ObligationCollateral {}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{error::LendingError, math::WAD},
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
        obligation.repay(repay_amount_wads, 0)?;
        assert!(obligation.borrows[0].borrowed_amount_wads < borrowed_amount_wads);
        assert!(obligation.borrows[0].borrowed_amount_wads > Decimal::zero());
      }
    }
}
