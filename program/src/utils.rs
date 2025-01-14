#[macro_export]
macro_rules! assert_is_signer {
    ($signer:expr, $msg_prefix:expr) => {
        if !$signer.is_signer {
            msg!("{} must be a signer", $msg_prefix);
            return Err(LendingError::InvalidSigner.into());
        }
    };
}

/**
 * Assert that an operation on two values is true, otherwise return the privided error and using msg! to log the provided message.
 *
 * ```rust
 * assert_compare!(a, >, b, "a must be greater than b", LendingError::InvalidOperation);
 * ```
 */
#[macro_export]
macro_rules! assert_compare {
    // For direct comparisons with custom operator
    ($left:expr, $op:tt, $right:expr, $msg:expr, $error:expr) => {
        if !($left $op $right) {
            msg!($msg);
            return Err($error.into());
        }
    };
    // For type conversions with custom operator
    ($left:expr, $op:tt, $right:tt as $type:ty, $msg:expr, $error:expr) => {
        if !($left $op $right as $type) {
            msg!($msg);
            return Err($error.into());
        }
    };
}

#[macro_export]
macro_rules! assert_equal {
    // For direct comparisons
    ($left:expr, $right:expr, $msg:expr, $error:expr) => {
        crate::assert_compare!($left, ==, $right, $msg, $error)
    };

    // For type conversions
    ($left:expr, $right:tt as $type:ty, $msg:expr, $error:expr) => {
        crate::assert_compare!($left, ==, $right as $type, $msg, $error)
    };
}

#[macro_export]
macro_rules! assert_not_equal {
    // For direct comparisons
    ($left:expr, $right:expr, $msg:expr, $error:expr) => {
        crate::assert_compare!($left, !=, $right, $msg, $error)
    };

    // For type conversions
    ($left:expr, $right:tt as $type:ty, $msg:expr, $error:expr) => {
        crate::assert_compare!($left, !=, $right as $type, $msg, $error)
    };
}

#[macro_export]
macro_rules! assert_key_equal {
    ($left:expr, $right:expr, $msg:expr, $error:expr) => {
        crate::assert_equal!($left, $right, $msg, $error)
    };
}

#[macro_export]
macro_rules! assert_key_not_equal {
    ($left:expr, $right:expr, $msg:expr, $error:expr) => {
        crate::assert_not_equal!($left, $right, $msg, $error)
    };
}
