mod misc;
mod pack;
mod reserve_collateral;
mod reserve_liquidity;
mod test;

use {
    super::{last_update::LastUpdate, PROGRAM_VERSION},
    crate::{
        error::LendingError,
        math::{Decimal, Rate, TryAdd, TryDiv, TryMul, TrySub},
    },
    solana_program::{
        clock::Slot, entrypoint::ProgramResult, msg, program_error::ProgramError, pubkey::Pubkey,
    },
};
pub use {misc::*, reserve_collateral::*, reserve_liquidity::*};

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
    pub fn deposit_liquidity(&mut self, liquidity_amount: u64) -> Result<u64, ProgramError> {
        let collateral_amount = self
            .collateral_exchange_rate()?
            .liquidity_to_collateral(liquidity_amount)?;
        self.liquidity.deposit(liquidity_amount)?;
        self.collateral.mint(collateral_amount)?;
        Ok(collateral_amount)
    }
    /// Record redeemed collateral and return amount of liquidity to withdraw
    pub fn redeem_collateral(&mut self, collateral_amount: u64) -> Result<u64, ProgramError> {
        let collateral_exchange_rate = self.collateral_exchange_rate()?;
        let liquididy_amount =
            collateral_exchange_rate.collateral_to_liquidity(collateral_amount)?;
        self.collateral.burn(collateral_amount)?;
        self.liquidity.withdraw(liquididy_amount)?;

        Ok(liquididy_amount)
    }

    //*
    //* Calculate the current borrow rate
    //*  This design uses Piecewise Function to encourage:
    //* 1. When utilization is low: Gentle rate increases to encourage borrowing
    //* 2. When utilization is high: Sharp rate increases to discourage borrowing and protect the liquidity pool
    //*
    pub fn current_borrow_rate(&self) -> Result<Rate, ProgramError> {
        let utilization_rate = self.liquidity.utilization_rate()?;
        let optimal_utilization_rate = Rate::from_percent(self.config.optimal_utilization_rate);

        //low utilization is when the utilization rate is less than the optimal utilization rate.

        let low_utilization = utilization_rate < optimal_utilization_rate;

        // When it's low utilization:
        //* `borrow_rate = normalized_rate * rate_range + min_rate`.
        //* where:
        //* normalized_rate = utilization_rate / optimal_utilization_rate
        
        if low_utilization || self.config.optimal_utilization_rate == 100 {
            //normalized rate is how much the utilization rate is close to the optimal utilization rate.
            let normalized_rate = utilization_rate.try_div(optimal_utilization_rate)?;
            let min_rate = Rate::from_percent(self.config.min_borrow_rate);
            let rate_range = Rate::from_percent(
                self.config
                    .optimal_borrow_rate
                    .checked_sub(self.config.min_borrow_rate)
                    .ok_or(LendingError::MathOverflow)?,
            );

            Ok(normalized_rate.try_mul(rate_range)?.try_add(min_rate)?)
        } else {
          
            //* When is high utilization: borrow_rate = normalized_rate * rate_range + min_rate.
            //* where:
            //* normalized_rate = (utilization_rate - optimal_utilization_rate) / (100 - optimal_utilization_rate)
            let normalized_rate = utilization_rate
                .try_sub(optimal_utilization_rate)?
                .try_div(Rate::from_percent(
                    100u8
                        .checked_sub(self.config.optimal_utilization_rate)
                        .ok_or(LendingError::MathOverflow)?,
                ))?;
            let min_rate = Rate::from_percent(self.config.optimal_borrow_rate);
            let rate_range = Rate::from_percent(
                self.config
                    .max_borrow_rate
                    .checked_sub(self.config.optimal_borrow_rate)
                    .ok_or(LendingError::MathOverflow)?,
            );

            Ok(normalized_rate.try_mul(rate_range)?.try_add(min_rate)?)
        }
    }

    pub fn collateral_exchange_rate(&self) -> Result<CollateralExchangeRate, ProgramError> {
        let total_liquidity = self.liquidity.total_supply()?;
        self.collateral.exchange_rate(total_liquidity)
    }
    /// Update borrow rate and accrue interest
    pub fn accrue_interest(&mut self, current_slot: Slot) -> ProgramResult {
        let slots_elapsed = self.last_update.slots_elapsed(current_slot)?;
        if slots_elapsed > 0 {
            let current_borrow_rate = self.current_borrow_rate()?;
            self.liquidity
                .compound_interest(current_borrow_rate, slots_elapsed)?;
        }
        Ok(())
    }

    /// Borrow liquidity up to a maximum market value
    pub fn calculate_borrow(
        &self,
        amount_to_borrow: u64,
        max_borrow_value: Decimal,
    ) -> Result<CalculateBorrowResult, ProgramError> {
        // @TODO: add lookup table https://git.io/JOCYq
        let decimals = 10u64
            .checked_pow(self.liquidity.mint_decimals as u32)
            .ok_or(LendingError::MathOverflow)?;
        if amount_to_borrow == u64::MAX {
            let borrow_amount = max_borrow_value
                .try_mul(decimals)?
                .try_div(self.liquidity.market_price)?
                .min(self.liquidity.available_amount.into());
            let (borrow_fee, host_fee) = self
                .config
                .fees
                .calculate_borrow_fees(borrow_amount, FeeCalculation::Inclusive)?;
            let receive_amount = borrow_amount
                .try_floor_u64()?
                .checked_sub(borrow_fee)
                .ok_or(LendingError::MathOverflow)?;
            Ok(CalculateBorrowResult {
                borrow_amount,
                receive_amount,
                borrow_fee,
                host_fee,
            })
        } else {
            let receive_amount = amount_to_borrow;
            let borrow_amount = Decimal::from(receive_amount);
            let (borrow_fee, host_fee) = self
                .config
                .fees
                .calculate_borrow_fees(borrow_amount, FeeCalculation::Exclusive)?;
            let borrow_amount = borrow_amount.try_add(borrow_fee.into())?;
            let borrow_value = borrow_amount
                .try_mul(self.liquidity.market_price)?
                .try_div(decimals)?;
            if borrow_value > max_borrow_value {
                msg!("Borrow value cannot exceed maximum borrow value");
                return Err(LendingError::BorrowTooLarge.into());
            }
            Ok(CalculateBorrowResult {
                borrow_amount,
                receive_amount,
                borrow_fee,
                host_fee,
            })
        }
    }
    pub fn calculate_repay(
        &self,
        amount_to_repay: u64,
        borrowed_amount: Decimal,
    ) -> Result<CalculateRepayResult, ProgramError> {
        let settle_amount = if amount_to_repay == u64::MAX {
            borrowed_amount
        } else {
            Decimal::from(amount_to_repay).min(borrowed_amount)
        };
        let repay_amount = settle_amount.try_ceil_u64()?;
        Ok(CalculateRepayResult {
            settle_amount,
            repay_amount,
        })
    }
}

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
