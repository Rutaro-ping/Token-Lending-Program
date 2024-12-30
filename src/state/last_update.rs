use {
    crate::error::LendingError,
    solana_program::{clock::Slot, program_error::ProgramError},
};

/// Number of slots to consider stale after
pub const STALE_AFTER_SLOTS_ELAPSED: u64 = 1;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LastUpdate {
    pub slot: Slot,
    pub stale: bool,
}

impl LastUpdate {
    pub fn new(slot: Slot) -> Self {
        Self { slot, stale: true }
    }
    pub fn slots_elapsed(&self, slot: Slot) -> Result<u64, ProgramError> {
        let elapsed = slot
            .checked_sub(self.slot)
            .ok_or(LendingError::MathOverflow)?;
        Ok(elapsed)
    }

    /// Set last update slot
    pub fn update_slot(&mut self, slot: Slot) {
        self.slot = slot;
        self.stale = false;
    }

    /// Set stale to true
    pub fn mark_stale(&mut self) {
        self.stale = true;
    }
    /// Check if marked stale or last update slot is too long ago
    pub fn is_stale(&self, slot: Slot) -> Result<bool, ProgramError> {
        Ok(self.stale || self.slots_elapsed(slot)? >= STALE_AFTER_SLOTS_ELAPSED)
    }
}
