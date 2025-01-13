mod obligation_collateral;
mod obligation_liquidity;
mod pack;
mod test;

use {
    super::{last_update::LastUpdate, LIQUIDATION_CLOSE_FACTOR, PROGRAM_VERSION},
    crate::{
        error::LendingError,
        math::{Decimal, Rate, TryDiv, TryMul, TrySub},
    },
    solana_program::{
        clock::Slot, entrypoint::ProgramResult, msg, program_error::ProgramError, pubkey::Pubkey,
    },
};
pub use {obligation_collateral::*, obligation_liquidity::*};

/// Max number of collateral and liquidity reserve accounts combined for an
/// obligation
pub const MAX_OBLIGATION_RESERVES: usize = 10;

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
            liquidity.repay(settle_amount)?;
        }
        Ok(())
    }
    pub fn withdraw(&mut self, withdraw_amount: u64, collateral_index: usize) -> ProgramResult {
        let collateral = &mut self.deposits.get_mut(collateral_index).unwrap();
        if withdraw_amount == collateral.deposited_amount {
            self.deposits.remove(collateral_index);
        } else {
            collateral.withdraw(withdraw_amount)?;
        }
        Ok(())
    }

    /// Calculate the maximum liquidity value that can be borrowed
    pub fn remaining_borrow_value(&self) -> Result<Decimal, ProgramError> {
        self.allowed_borrow_value.try_sub(self.borrowed_value)
    }
    pub fn find_or_add_liquidity_to_borrows(
        &mut self,
        borrow_reserve: Pubkey,
    ) -> Result<&mut ObligationLiquidity, ProgramError> {
        if let Some(liquidity_index) = self._find_liquidity_index_in_borrows(borrow_reserve) {
            return Ok(&mut self.borrows[liquidity_index]);
        }
        if self.deposits.len() + self.borrows.len() >= MAX_OBLIGATION_RESERVES {
            msg!(
                "Obligation cannot have more than {} deposits and borrows combined",
                MAX_OBLIGATION_RESERVES
            );
            return Err(LendingError::ObligationReserveLimit.into());
        }
        let liquidity = ObligationLiquidity::new(borrow_reserve);
        self.borrows.push(liquidity);
        Ok(self.borrows.last_mut().unwrap())
    }
    fn _find_liquidity_index_in_borrows(&self, borrow_reserve: Pubkey) -> Option<usize> {
        self.borrows
            .iter()
            .position(|liquidity| liquidity.borrow_reserve == borrow_reserve)
    }
    fn _find_collateral_index_in_deposits(&self, deposit_reserve: Pubkey) -> Option<usize> {
        self.deposits
            .iter()
            .position(|collateral| collateral.deposit_reserve == deposit_reserve)
    }
    /// Find collateral by deposit reserve
    pub fn find_collateral_in_deposits(
        &self,
        deposit_reserve: Pubkey,
    ) -> Result<(&ObligationCollateral, usize), ProgramError> {
        if self.deposits.is_empty() {
            msg!("Obligation has no deposits");
            return Err(LendingError::ObligationDepositsEmpty.into());
        }
        let collateral_index = self
            ._find_collateral_index_in_deposits(deposit_reserve)
            .ok_or(LendingError::InvalidObligationCollateral)?;
        Ok((&self.deposits[collateral_index], collateral_index))
    }
    pub fn find_liquidity_in_borrows(
        &self,
        borrow_reserve: Pubkey,
    ) -> Result<(&ObligationLiquidity, usize), ProgramError> {
        let liquidity_index = self
            ._find_liquidity_index_in_borrows(borrow_reserve)
            .ok_or(LendingError::InvalidObligationLiquidity)?;
        Ok((&self.borrows[liquidity_index], liquidity_index))
    }
    /// Calculate the maximum collateral value that can be withdrawn
    pub fn max_withdraw_value(
        &self,
        withdraw_collateral_ltv: Rate,
    ) -> Result<Decimal, ProgramError> {
        if self.allowed_borrow_value <= self.borrowed_value {
            return Ok(Decimal::zero());
        }
        if withdraw_collateral_ltv == Rate::zero() {
            return Ok(self.deposited_value);
        }
        self.allowed_borrow_value
            .try_sub(self.borrowed_value)?
            .try_div(withdraw_collateral_ltv)
    }
    /// Find or add collateral by deposit reserve
    pub fn find_or_add_collateral_to_deposits(
        &mut self,
        deposit_reserve: Pubkey,
    ) -> Result<&mut ObligationCollateral, ProgramError> {
        if let Some(collateral_index) = self._find_collateral_index_in_deposits(deposit_reserve) {
            return Ok(&mut self.deposits[collateral_index]);
        }
        if self.deposits.len() + self.borrows.len() >= MAX_OBLIGATION_RESERVES {
            msg!(
                "Obligation cannot have more than {} deposits and borrows combined",
                MAX_OBLIGATION_RESERVES
            );
            return Err(LendingError::ObligationReserveLimit.into());
        }
        let collateral = ObligationCollateral::new(deposit_reserve);
        self.deposits.push(collateral);
        Ok(self.deposits.last_mut().unwrap())
    }

    /// Calculate the maximum liquidation amount for a given liquidity
    pub fn max_liquidation_amount(
        &self,
        liquidity: &ObligationLiquidity,
    ) -> Result<Decimal, ProgramError> {
        let max_liquidation_value = self
            .borrowed_value
            .try_mul(Rate::from_percent(LIQUIDATION_CLOSE_FACTOR))?
            .min(liquidity.market_value);
        let max_liquidation_pct = max_liquidation_value.try_div(liquidity.market_value)?;
        liquidity.borrowed_amount_wads.try_mul(max_liquidation_pct)
    }
}

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
