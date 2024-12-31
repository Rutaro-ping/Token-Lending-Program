mod last_update;
mod lending_market;
mod obligation;
mod reserve;
use solana_program::clock::DEFAULT_TICKS_PER_SECOND;
use solana_program::clock::DEFAULT_TICKS_PER_SLOT;
use solana_program::clock::SECONDS_PER_DAY;
pub use {crate::math::WAD, lending_market::*, reserve::*};

pub const PROGRAM_VERSION: u8 = 1;

/// Collateral tokens are initially valued at a ratio of 5:1
/// (collateral:liquidity)
// @FIXME: restore to 5
pub const INITIAL_COLLATERAL_RATIO: u64 = 1;
const INITIAL_COLLATERAL_RATE: u64 = INITIAL_COLLATERAL_RATIO * WAD;

/// Accounts are created with data zeroed out, so uninitialized state instances
/// will have the version set to 0.
pub const UNINITIALIZED_VERSION: u8 = 0;

fn pack_bool(bool: bool, dst: &mut [u8; 1]) {
    *dst = (bool as u8).to_le_bytes();
}

/// Number of slots per year
pub const SLOTS_PER_YEAR: u64 =
    DEFAULT_TICKS_PER_SECOND / DEFAULT_TICKS_PER_SLOT * SECONDS_PER_DAY * 365;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pack_bool() {
        let mut dst = [0; 1];
        pack_bool(true, &mut dst);
        assert_eq!(dst, [1]);
    }
}
