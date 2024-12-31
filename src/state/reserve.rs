use {
    super::{last_update::LastUpdate, INITIAL_COLLATERAL_RATE, PROGRAM_VERSION, WAD},
    crate::{
        error::LendingError,
        math::{Decimal, Rate, TryAdd, TryDiv, TryMul, TrySub},
    },
    solana_program::{entrypoint::ProgramResult, msg, program_error::ProgramError, pubkey::Pubkey},
    solana_sdk::clock::Slot,
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

    /// Calculate the owner and host fees on flash loan
    pub fn calculate_flash_loan_fees(
        &self,
        flash_loan_amount: Decimal,
    ) -> Result<(u64, u64), ProgramError> {
        self.calculate_fees(
            flash_loan_amount,
            self.flash_loan_fee_wad,
            FeeCalculation::Exclusive,
        )
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
impl Reserve {
    /// Create a new reserve
    pub fn new(params: InitReserveParams) -> Self {
        let mut reserve = Self::default();
        Self::init(&mut reserve, params);
        reserve
    }
    /// Initialize a reserve
    pub fn init(&mut self, params: InitReserveParams) {
        self.version = PROGRAM_VERSION;
        self.last_update = LastUpdate::new(params.current_slot);
        self.lending_market = params.lending_market;
        self.liquidity = params.liquidity;
        self.collateral = params.collateral;
        self.config = params.config;
    }
    /// Record deposited liquidity and return amount of collateral tokens to
    /// mint
    pub fn deposit_liquidity(&mut self, liquidity_amount: u64) -> ProgramResult {
        let collateral_amount = self.collateral_exchange_rate()?;
        //todo
        Ok(())
    }

}

/// Initialize a reserve
pub struct InitReserveParams {
    /// Last slot when supply and rates updated
    pub current_slot: Slot,
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
    pub fn total_supply(&self) -> Result<Decimal, ProgramError> {
        Decimal::from(self.available_amount).try_add(self.borrowed_amount_wads)
    }

    /// Add liquidity to available amount
    pub fn deposit(&mut self, liquidity_amount: u64) -> ProgramResult {
        self.available_amount = self
            .available_amount
            .checked_add(liquidity_amount)
            .ok_or(LendingError::MathOverflow)?;
        Ok(())
    }
    /// Remove liquidity from available amount
    pub fn withdraw(&mut self, liquidity_amount: u64) -> ProgramResult {
        if liquidity_amount > self.available_amount {
            msg!("Withdraw amount cannot exceed available amount");
            return Err(LendingError::InsufficientLiquidity.into());
        }
        self.available_amount = self
            .available_amount
            .checked_sub(liquidity_amount)
            .ok_or(LendingError::MathOverflow)?;
        Ok(())
    }
    pub fn borrow(&mut self, borrow_decimal: Decimal) -> ProgramResult {
        let borrow_amount = borrow_decimal.try_floor_u64()?;
        if borrow_amount > self.available_amount {
            msg!("Borrow amount cannot exceed available amount");
            return Err(LendingError::InsufficientLiquidity.into());
        }
        self.available_amount = self
            .available_amount
            .checked_sub(borrow_amount)
            .ok_or(LendingError::MathOverflow)?;
        self.borrowed_amount_wads = self.borrowed_amount_wads.try_add(borrow_decimal)?;

        Ok(())
    }
    /// Add repay amount to available liquidity and subtract settle amount from
    /// total borrows
    pub fn repay(&mut self, repay_amount: u64, settle_amount: Decimal) -> ProgramResult {
        self.available_amount = self
            .available_amount
            .checked_add(repay_amount)
            .ok_or(LendingError::MathOverflow)?;
        self.borrowed_amount_wads = self.borrowed_amount_wads.try_sub(settle_amount)?;

        Ok(())
    }
    /// Calculate the liquidity utilization rate of the reserve
    pub fn utilization_rate(&self) -> Result<Rate, ProgramError> {
        let total_supply = self.total_supply()?;
        if total_supply == Decimal::zero() {
            return Ok(Rate::zero());
        }
        self.borrowed_amount_wads.try_div(total_supply)?.try_into()
    }
    /// Compound current borrow rate over elapsed slots
    fn compound_interest(
        &mut self,
        current_borrow_rate: Rate,
        slots_elapsed: u64,
    ) -> ProgramResult {
        let slot_interest_rate = current_borrow_rate.try_div(slots_elapsed)?;
        let compounded_interest_rate = Rate::one()
            .try_add(slot_interest_rate)?
            .try_pow(slots_elapsed)?;
        self.cumulative_borrow_rate_wads = self
            .cumulative_borrow_rate_wads
            .try_mul(compounded_interest_rate)?;
        self.borrowed_amount_wads = self
            .borrowed_amount_wads
            .try_mul(compounded_interest_rate)?;
        Ok(())
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

/// Collateral exchange rate
#[derive(Clone, Copy, Debug)]
pub struct CollateralExchangeRate(Rate);

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
impl ReserveCollateral {
    pub fn new(params: NewReserveCollateralParams) -> Self {
        Self {
            mint_pubkey: params.mint_pubkey,
            mint_total_supply: 0,
            supply_pubkey: params.supply_pubkey,
        }
    }
    /// Add collateral to total supply
    pub fn mint(&mut self, collateral_amount: u64) -> ProgramResult {
        self.mint_total_supply = self
            .mint_total_supply
            .checked_add(collateral_amount)
            .ok_or(LendingError::MathOverflow)?;
        Ok(())
    }

    /// Remove collateral from total supply
    pub fn burn(&mut self, collateral_amount: u64) -> ProgramResult {
        self.mint_total_supply = self
            .mint_total_supply
            .checked_sub(collateral_amount)
            .ok_or(LendingError::MathOverflow)?;
        Ok(())
    }
    fn exchange_rate(
        &self,
        total_liquidity: Decimal,
    ) -> Result<CollateralExchangeRate, ProgramError> {
        let rate = if self.mint_total_supply == 0 || total_liquidity == Decimal::zero() {
            Rate::from_scaled_val(INITIAL_COLLATERAL_RATE)
        } else {
            let mint_total_supply = Decimal::from(self.mint_total_supply);
            Rate::try_from(mint_total_supply.try_div(total_liquidity)?)?
        };

        Ok(CollateralExchangeRate(rate))
    }
}

pub struct NewReserveCollateralParams {
    /// Reserve collateral mint address
    pub mint_pubkey: Pubkey,
    /// Reserve collateral supply address
    pub supply_pubkey: Pubkey,
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use crate::{math::PERCENT_SCALER, state::SLOTS_PER_YEAR};

    use super::*;
    use proptest::prelude::*;

    const MAX_LIQUIDITY: u64 = u64::MAX / 5;

    prop_compose! {
         // Creates rates (min, opt, max) where 0 <= min <= opt <= max <= MAX
        fn borrow_rates()(optimal_rate in 0..=u8::MAX)(
            min_rate in 0..=optimal_rate,
            optimal_rate in Just(optimal_rate),
            max_rate in optimal_rate..=u8::MAX,
        ) -> (u8, u8, u8) {
            (min_rate, optimal_rate, max_rate)
        }
    }

    // Creates rates (threshold, ltv) where 2 <= threshold <= 100 and threshold <=
    // ltv <= 1,000%
    prop_compose! {
        fn unhealthy_rates()(threshold in 2..=100u8)(
            ltv_rate in threshold as u64..=1000u64,
            threshold in Just(threshold),
        ) -> (Decimal, u8) {
            (Decimal::from_scaled_val(ltv_rate as u128 * PERCENT_SCALER as u128), threshold)
        }
    }
    // Creates a range of reasonable token conversion rates
    prop_compose! {
        fn token_conversion_rate()(
            conversion_rate in 1..=u16::MAX,
            invert_conversion_rate: bool,
        ) -> Decimal {
            let conversion_rate = Decimal::from(conversion_rate as u64);
            if invert_conversion_rate {
                Decimal::one().try_div(conversion_rate).unwrap()
            } else {
                conversion_rate
            }
        }
    }
    // Creates a range of reasonable collateral exchange rates
    prop_compose! {
        fn collateral_exchange_rate_range()(percent in 1..=500u64) -> CollateralExchangeRate {
            CollateralExchangeRate(Rate::from_scaled_val(percent * PERCENT_SCALER))
        }
    }

    proptest! {
        #[test]
        fn current_borrow_rate(
            total_liquidity in 0..=MAX_LIQUIDITY,
            borrowed_percent in 0..=WAD,
            optimal_utilization_rate in 0..=100u8,
            (min_borrow_rate, optimal_borrow_rate, max_borrow_rate) in borrow_rates(),
        ) {
            let borrowed_amount_wads = Decimal::from(total_liquidity).try_mul(Rate::from_scaled_val(borrowed_percent))?;
            let reserve = Reserve {
                liquidity: ReserveLiquidity {
                    borrowed_amount_wads,
                    available_amount: total_liquidity - borrowed_amount_wads.try_round_u64()?,
                    ..ReserveLiquidity::default()
                },
                config: ReserveConfig { optimal_utilization_rate, min_borrow_rate, optimal_borrow_rate, max_borrow_rate, ..ReserveConfig::default() },
                ..Reserve::default()
            };

            let current_borrow_rate = reserve.current_borrow_rate()?;
            assert!(current_borrow_rate >= Rate::from_percent(min_borrow_rate));
            assert!(current_borrow_rate <= Rate::from_percent(max_borrow_rate));

            let optimal_borrow_rate = Rate::from_percent(optimal_borrow_rate);
            let current_rate = reserve.liquidity.utilization_rate()?;
            match current_rate.cmp(&Rate::from_percent(optimal_utilization_rate)) {
                Ordering::Less => {
                    if min_borrow_rate == reserve.config.optimal_borrow_rate {
                        assert_eq!(current_borrow_rate, optimal_borrow_rate);
                    } else {
                        assert!(current_borrow_rate < optimal_borrow_rate);
                    }
                }
                Ordering::Equal => assert!(current_borrow_rate == optimal_borrow_rate),
                Ordering::Greater => {
                    if max_borrow_rate == reserve.config.optimal_borrow_rate {
                        assert_eq!(current_borrow_rate, optimal_borrow_rate);
                    } else {
                        assert!(current_borrow_rate > optimal_borrow_rate);
                    }
                }
            }
        }

        #[test]
        fn current_utilization_rate(
            total_liquidity in 0..=MAX_LIQUIDITY,
            borrowed_percent in 0..=WAD,
        ) {
            let borrowed_amount_wads = Decimal::from(total_liquidity).try_mul(Rate::from_scaled_val(borrowed_percent))?;
            let liquidity = ReserveLiquidity {
                borrowed_amount_wads,
                available_amount: total_liquidity - borrowed_amount_wads.try_round_u64()?,
                ..ReserveLiquidity::default()
            };

            let current_rate = liquidity.utilization_rate()?;
            assert!(current_rate <= Rate::one());
        }

        #[test]
        fn collateral_exchange_rate(
            total_liquidity in 0..=MAX_LIQUIDITY,
            borrowed_percent in 0..=WAD,
            collateral_multiplier in 0..=(5*WAD),
            borrow_rate in 0..=u8::MAX,
        ) {
            let borrowed_liquidity_wads = Decimal::from(total_liquidity).try_mul(Rate::from_scaled_val(borrowed_percent))?;
            let available_liquidity = total_liquidity - borrowed_liquidity_wads.try_round_u64()?;
            let mint_total_supply = Decimal::from(total_liquidity).try_mul(Rate::from_scaled_val(collateral_multiplier))?.try_round_u64()?;

            let mut reserve = Reserve {
                collateral: ReserveCollateral {
                    mint_total_supply,
                    ..ReserveCollateral::default()
                },
                liquidity: ReserveLiquidity {
                    borrowed_amount_wads: borrowed_liquidity_wads,
                    available_amount: available_liquidity,
                    ..ReserveLiquidity::default()
                },
                config: ReserveConfig {
                    min_borrow_rate: borrow_rate,
                    optimal_borrow_rate: borrow_rate,
                    optimal_utilization_rate: 100,
                    ..ReserveConfig::default()
                },
                ..Reserve::default()
            };

            let exchange_rate = reserve.collateral_exchange_rate()?;
            assert!(exchange_rate.0.to_scaled_val() <= 5u128 * WAD as u128);

            // After interest accrual, total liquidity increases and collateral are worth more
            reserve.accrue_interest(1)?;

            let new_exchange_rate = reserve.collateral_exchange_rate()?;
            if borrow_rate > 0 && total_liquidity > 0 && borrowed_percent > 0 {
                assert!(new_exchange_rate.0 < exchange_rate.0);
            } else {
                assert_eq!(new_exchange_rate.0, exchange_rate.0);
            }
        }

        #[test]
        fn compound_interest(
            slots_elapsed in 0..=SLOTS_PER_YEAR,
            borrow_rate in 0..=u8::MAX,
        ) {
            let mut reserve = Reserve::default();
            let borrow_rate = Rate::from_percent(borrow_rate);

            // Simulate running for max 1000 years, assuming that interest is
            // compounded at least once a year
            for _ in 0..1000 {
                reserve.liquidity.compound_interest(borrow_rate, slots_elapsed)?;
                reserve.liquidity.cumulative_borrow_rate_wads.to_scaled_val()?;
            }
        }

        #[test]
        fn reserve_accrue_interest(
            slots_elapsed in 0..=SLOTS_PER_YEAR,
            borrowed_liquidity in 0..=u64::MAX,
            borrow_rate in 0..=u8::MAX,
        ) {
            let borrowed_amount_wads = Decimal::from(borrowed_liquidity);
            let mut reserve = Reserve {
                liquidity: ReserveLiquidity {
                    borrowed_amount_wads,
                    ..ReserveLiquidity::default()
                },
                config: ReserveConfig {
                    max_borrow_rate: borrow_rate,
                    ..ReserveConfig::default()
                },
                ..Reserve::default()
            };

            reserve.accrue_interest(slots_elapsed)?;

            if borrow_rate > 0 && slots_elapsed > 0 {
                assert!(reserve.liquidity.borrowed_amount_wads > borrowed_amount_wads);
            } else {
                assert!(reserve.liquidity.borrowed_amount_wads == borrowed_amount_wads);
            }
        }

        #[test]
        fn borrow_fee_calculation(
            borrow_fee_wad in 0..WAD, // at WAD, fee == borrow amount, which fails
            flash_loan_fee_wad in 0..WAD, // at WAD, fee == borrow amount, which fails
            host_fee_percentage in 0..=100u8,
            borrow_amount in 3..=u64::MAX, // start at 3 to ensure calculation success
                                           // 0, 1, and 2 are covered in the minimum tests
                                           // @FIXME: ^ no longer true
        ) {
            let fees = ReserveFees {
                borrow_fee_wad,
                flash_loan_fee_wad,
                host_fee_percentage,
            };
            let (total_fee, host_fee) = fees.calculate_borrow_fees(Decimal::from(borrow_amount), FeeCalculation::Exclusive)?;

            // The total fee can't be greater than the amount borrowed, as long
            // as amount borrowed is greater than 2.
            // At a borrow amount of 2, we can get a total fee of 2 if a host
            // fee is also specified.
            assert!(total_fee <= borrow_amount);

            // the host fee can't be greater than the total fee
            assert!(host_fee <= total_fee);

            // for all fee rates greater than 0, we must have some fee
            if borrow_fee_wad > 0 {
                assert!(total_fee > 0);
            }

            if host_fee_percentage == 100 {
                // if the host fee percentage is maxed at 100%, it should get all the fee
                assert_eq!(host_fee, total_fee);
            }

            // if there's a host fee and some borrow fee, host fee must be greater than 0
            if host_fee_percentage > 0 && borrow_fee_wad > 0 {
                assert!(host_fee > 0);
            } else {
                assert_eq!(host_fee, 0);
            }
        }

        #[test]
        fn flash_loan_fee_calculation(
            borrow_fee_wad in 0..WAD, // at WAD, fee == borrow amount, which fails
            flash_loan_fee_wad in 0..WAD, // at WAD, fee == borrow amount, which fails
            host_fee_percentage in 0..=100u8,
            borrow_amount in 3..=u64::MAX, // start at 3 to ensure calculation success
                                           // 0, 1, and 2 are covered in the minimum tests
                                           // @FIXME: ^ no longer true
        ) {
            let fees = ReserveFees {
                borrow_fee_wad,
                flash_loan_fee_wad,
                host_fee_percentage,
            };
            let (total_fee, host_fee) = fees.calculate_flash_loan_fees(Decimal::from(borrow_amount))?;

            // The total fee can't be greater than the amount borrowed, as long
            // as amount borrowed is greater than 2.
            // At a borrow amount of 2, we can get a total fee of 2 if a host
            // fee is also specified.
            assert!(total_fee <= borrow_amount);

            // the host fee can't be greater than the total fee
            assert!(host_fee <= total_fee);

            // for all fee rates greater than 0, we must have some fee
            if borrow_fee_wad > 0 {
                assert!(total_fee > 0);
            }

            if host_fee_percentage == 100 {
                // if the host fee percentage is maxed at 100%, it should get all the fee
                assert_eq!(host_fee, total_fee);
            }

            // if there's a host fee and some borrow fee, host fee must be greater than 0
            if host_fee_percentage > 0 && borrow_fee_wad > 0 {
                assert!(host_fee > 0);
            } else {
                assert_eq!(host_fee, 0);
            }
        }
    }

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
    #[test]
    fn borrow_fee_calculation_min_no_host() {
        let fees = ReserveFees {
            borrow_fee_wad: 10_000_000_000_000_000, // 1%
            flash_loan_fee_wad: 0,
            host_fee_percentage: 0,
        };
        let (total_fee, host_fee) = fees
            .calculate_borrow_fees(Decimal::from(2u64), FeeCalculation::Exclusive)
            .unwrap();
        assert_eq!(total_fee, 1);
        assert_eq!(host_fee, 0);
        let err = fees
            .calculate_borrow_fees(Decimal::one(), FeeCalculation::Exclusive)
            .unwrap_err();
        assert_eq!(err, LendingError::BorrowTooSmall.into()); // minimum of 2 tokens

        // 0 amount borrowed, 0 fee
        let (total_fee, host_fee) = fees
            .calculate_borrow_fees(Decimal::zero(), FeeCalculation::Exclusive)
            .unwrap();
        assert_eq!(total_fee, 0);
        assert_eq!(host_fee, 0);
    }
    #[test]
    fn borrow_fee_calculation_host() {
        let fees = ReserveFees {
            borrow_fee_wad: 10_000_000_000_000_000, // 1%
            flash_loan_fee_wad: 0,
            host_fee_percentage: 20,
        };

        let (total_fee, host_fee) = fees
            .calculate_borrow_fees(Decimal::from(1000u64), FeeCalculation::Exclusive)
            .unwrap();

        assert_eq!(total_fee, 10); // 1% of 1000
        assert_eq!(host_fee, 2); // 20% of 10
    }
    #[test]
    fn borrow_fee_calculation_no_host() {
        let fees = ReserveFees {
            borrow_fee_wad: 10_000_000_000_000_000, // 1%
            flash_loan_fee_wad: 0,
            host_fee_percentage: 0,
        };

        let (total_fee, host_fee) = fees
            .calculate_borrow_fees(Decimal::from(1000u64), FeeCalculation::Exclusive)
            .unwrap();

        assert_eq!(total_fee, 10); // 1% of 1000
        assert_eq!(host_fee, 0); // 0 host fee
    }
}
