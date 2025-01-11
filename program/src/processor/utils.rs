use {
    crate::error::LendingError,
    solana_program::{
        account_info::AccountInfo,
        entrypoint::ProgramResult,
        msg,
        program_error::ProgramError,
        program_pack::{IsInitialized, Pack},
        rent::Rent,
    },
};

pub(super) fn assert_uninitialized<T: IsInitialized + Pack>(
    account_info: &AccountInfo,
) -> Result<T, ProgramError> {
    let account = T::unpack_unchecked(&account_info.data.borrow())?;

    if account.is_initialized() {
        msg!("Account is already initialized");
        Err(LendingError::AlreadyInitialized.into())
    } else {
        Ok(account)
    }
}

pub(super) fn assert_rent_exempt(rent: &Rent, account_info: &AccountInfo) -> ProgramResult {
    if !rent.is_exempt(account_info.lamports(), account_info.data_len()) {
        msg!(&rent.minimum_balance(account_info.data_len()).to_string());
        Err(LendingError::NotRentExempt.into())
    } else {
        Ok(())
    }
}
