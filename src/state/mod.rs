mod reserve;
mod lending_market;
pub use reserve::*;
pub use lending_market::*;

pub const PROGRAM_VERSION: u8 = 1;

/// Accounts are created with data zeroed out, so uninitialized state instances
/// will have the version set to 0.
pub const UNINITIALIZED_VERSION: u8 = 0;

fn pack_bool(bool: bool, dst: &mut [u8; 1]) {
    *dst = (bool as u8).to_le_bytes();
}

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
