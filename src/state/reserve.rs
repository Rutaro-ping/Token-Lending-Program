use {
    super::{last_update::LastUpdate, WAD},
    crate::{
        error::LendingError,
        math::{Decimal, Rate, TryAdd, TryDiv, TryMul},
    },
    solana_program::{entrypoint::ProgramResult, msg, program_error::ProgramError, pubkey::Pubkey},
};

/// Reserve configuration values
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReserveConfig {
    /// Optimal utilization rate, as a percentage
    pub optimal_utilization_rate: u8,
    /// Target ratio of the value of borrows to deposits, as a percentage
    /// 0 if use as collateral is disabled
    pub loan_to_value_ratio: u8,
    /// Bonus a liquidator gets when repaying part of an unhealthy obligation,
    /// as a percentage
    pub liquidation_bonus: u8,
    /// Loan to value ratio at which an obligation can be liquidated, as a
    /// percentage
    pub liquidation_threshold: u8,
    /// Min borrow APY
    pub min_borrow_rate: u8,
    /// Optimal (utilization) borrow APY
    pub optimal_borrow_rate: u8,
    /// Max borrow APY
    pub max_borrow_rate: u8,
    /// Program owner fees assessed, separate from gains due to interest accrual
    pub fees: ReserveFees,
}

impl ReserveConfig {
    pub fn validate(&self) -> ProgramResult {
        if self.optimal_utilization_rate > 100 {
            msg!("Optimal utilization rate must be in range [0, 100]");
            return Err(LendingError::InvalidConfig.into());
        }
        if self.loan_to_value_ratio >= 100 {
            msg!("Loan to value ratio must be in range [0, 100)");
            return Err(LendingError::InvalidConfig.into());
        }
        if self.liquidation_bonus > 100 {
            msg!("Liquidation bonus must be in range [0, 100]");
            return Err(LendingError::InvalidConfig.into());
        }
        if self.liquidation_threshold <= self.loan_to_value_ratio
            || self.liquidation_threshold > 100
        {
            msg!("Liquidation threshold must be in range (LTV, 100]");
            return Err(LendingError::InvalidConfig.into());
        }
        if self.optimal_borrow_rate < self.min_borrow_rate {
            msg!("Optimal borrow rate must be >= min borrow rate");
            return Err(LendingError::InvalidConfig.into());
        }
        if self.optimal_borrow_rate > self.max_borrow_rate {
            msg!("Optimal borrow rate must be <= max borrow rate");
            return Err(LendingError::InvalidConfig.into());
        }
        if self.fees.borrow_fee_wad >= WAD {
            msg!("Borrow fee must be in range [0, 1_000_000_000_000_000_000)");
            return Err(LendingError::InvalidConfig.into());
        }
        if self.fees.flash_loan_fee_wad >= WAD {
            msg!("Flash loan fee must be in range [0, 1_000_000_000_000_000_000)");
            return Err(LendingError::InvalidConfig.into());
        }
        if self.fees.host_fee_percentage > 100 {
            msg!("Host fee percentage must be in range [0, 100]");
            return Err(LendingError::InvalidConfig.into());
        }

        Ok(())
    }
}

/// Calculate fees exclusive or inclusive of an amount
pub enum FeeCalculation {
    /// Fee added to amount: fee = rate * amount
    Exclusive,
    /// Fee included in amount: fee = (rate / (1 + rate)) * amount
    Inclusive,
}

/// Additional fee information on a reserve
///
/// These exist separately from interest accrual fees, and are specifically for
/// the program owner and frontend host. The fees are paid out as a percentage
/// of liquidity token amounts during repayments and liquidations.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReserveFees {
    /// Fee assessed on `BorrowObligationLiquidity`, expressed as a Wad.
    /// Must be between 0 and 10^18, such that 10^18 = 1.  A few examples for
    /// clarity:
    /// 1% = 10_000_000_000_000_000
    /// 0.01% (1 basis point) = 100_000_000_000_000
    /// 0.00001% (Aave borrow fee) = 100_000_000_000
    pub borrow_fee_wad: u64,
    /// Fee for flash loan, expressed as a Wad.
    /// 0.3% (Aave flash loan fee) = 3_000_000_000_000_000
    pub flash_loan_fee_wad: u64,
    /// Amount of fee going to host account, if provided in liquidate and repay
    pub host_fee_percentage: u8,
}

impl ReserveFees {
    pub fn calculate_borrow_fees(
        &self,
        borrow_amount: Decimal,
        fee_calculation: FeeCalculation,
    ) -> Result<(u64, u64), ProgramError> {
        self.calculate_fees(borrow_amount, self.borrow_fee_wad, fee_calculation)
    }
    pub fn calculate_fees(
        &self,
        amount: Decimal,
        fee_wad: u64,
        fee_calculation: FeeCalculation,
    ) -> Result<(u64, u64), ProgramError> {
        let borrow_fee_rate = Rate::from_scaled_val(fee_wad);
        let host_fee_rate = Rate::from_percent(self.host_fee_percentage);
        if borrow_fee_rate > Rate::zero() && amount > Decimal::zero() {
            let need_to_assess_host_fee = host_fee_rate > Rate::zero();
            let minimum_fee: u64 = if need_to_assess_host_fee {
                2 //1 token to owner, 1 to host
            } else {
                1 // 1 token to owner, nothing else
            };

            let borrow_fee_amount = match fee_calculation {
                // Calculate fee to be added to borrow: fee = amount * rate
                FeeCalculation::Exclusive => amount.try_mul(borrow_fee_rate)?,
                // Calculate fee to be subtracted from borrow: fee = amount * (rate / (rate + 1))
                FeeCalculation::Inclusive => {
                    let borrow_fee_rate =
                        borrow_fee_rate.try_div(borrow_fee_rate.try_add(Rate::one())?)?;
                    amount.try_mul(borrow_fee_rate)?
                }
            };

            let borrow_fee_decimal = borrow_fee_amount.max(minimum_fee.into());
            println!(
                "borrow_fee_decimal: {}, amount: {}",
                borrow_fee_decimal, amount
            );
            if borrow_fee_decimal >= amount {
                msg!("Borrow amount is too small to receive liquidity after fees");
                return Err(LendingError::BorrowTooSmall.into());
            }
            let borrow_fee = borrow_fee_decimal.try_round_u64()?;
            let host_fee = if need_to_assess_host_fee {
                borrow_fee_decimal
                    .try_mul(host_fee_rate)?
                    .try_round_u64()?
                    .max(1)
            } else {
                0
            };
            return Ok((borrow_fee, host_fee));
        }
        Ok((0, 0))
    }
}

/// Lending market reserve state
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reserve {
    /// Version of the struct
    pub version: u8,
    /// Last slot when supply and rates updated
    pub last_update: LastUpdate,
    /// Lending market address
    pub lending_market: Pubkey,
    /// Reserve liquidity
    pub liquidity: ReserveLiquidity,
    /// Reserve collateral
    pub collateral: ReserveCollateral,
    /// Reserve configuration values
    pub config: ReserveConfig,
}

/// Reserve liquidity
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReserveLiquidity {
    /// Reserve liquidity mint address
    pub mint_pubkey: Pubkey,
    /// Reserve liquidity mint decimals
    pub mint_decimals: u8,
    /// Reserve liquidity supply address
    pub supply_pubkey: Pubkey,
    /// Reserve liquidity fee receiver address
    pub fee_receiver: Pubkey,
    /// Reserve liquidity oracle account
    pub oracle_pubkey: Pubkey,
    /// Reserve liquidity available
    pub available_amount: u64,
    /// Reserve liquidity borrowed
    pub borrowed_amount_wads: Decimal,
    /// Reserve liquidity cumulative borrow rate
    pub cumulative_borrow_rate_wads: Decimal,
    /// Reserve liquidity market price in quote currency
    pub market_price: Decimal,
}

impl ReserveLiquidity {
    pub fn new(params: NewReserveLiquidityParams) -> Self {
        Self {
            mint_pubkey: params.mint_pubkey,
            mint_decimals: params.mint_decimals,
            supply_pubkey: params.supply_pubkey,
            fee_receiver: params.fee_receiver,
            oracle_pubkey: params.oracle_pubkey,
            available_amount: 0,
            borrowed_amount_wads: Decimal::zero(),
            cumulative_borrow_rate_wads: Decimal::one(),
            market_price: params.market_price,
        }
    }
}

pub struct NewReserveLiquidityParams {
    /// Reserve liquidity mint address
    pub mint_pubkey: Pubkey,
    /// Reserve liquidity mint decimals
    pub mint_decimals: u8,
    /// Reserve liquidity supply address
    pub supply_pubkey: Pubkey,
    /// Reserve liquidity fee receiver address
    pub fee_receiver: Pubkey,
    /// Reserve liquidity oracle account
    pub oracle_pubkey: Pubkey,
    /// Reserve liquidity market price in quote currency
    pub market_price: Decimal,
}

/// Reserve collateral
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReserveCollateral {
    /// Reserve collateral mint address
    pub mint_pubkey: Pubkey,
    /// Reserve collateral mint supply, used for exchange rate
    pub mint_total_supply: u64,
    /// Reserve collateral supply address
    pub supply_pubkey: Pubkey,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn borrow_fee_calculation_min_host() {
        let fees = ReserveFees {
            borrow_fee_wad: 10_000_000_000_000_000, // 1%
            flash_loan_fee_wad: 0,
            host_fee_percentage: 20,
        };
        // only 2 tokens borrowed, get error
        let err = fees
            .calculate_borrow_fees(Decimal::from(2u64), FeeCalculation::Exclusive)
            .unwrap_err();
        assert_eq!(err, LendingError::BorrowTooSmall.into()); // minimum of 3

        // only 1 token borrowed, get error
        let err = fees
            .calculate_borrow_fees(Decimal::one(), FeeCalculation::Exclusive)
            .unwrap_err();
        assert_eq!(err, LendingError::BorrowTooSmall.into());

        let (total_fee, host_fee) = fees
            .calculate_borrow_fees(Decimal::zero(), FeeCalculation::Exclusive)
            .unwrap();
        assert_eq!(total_fee, 0);
        assert_eq!(host_fee, 0);
    }
}
