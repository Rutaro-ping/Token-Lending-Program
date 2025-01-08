use {
    super::{assert_rent_exempt, assert_uninitialized},
    crate::{
        error::LendingError,
        state::{InitLendingMarketParams, LendingMarket},
    },
    solana_program::{
        account_info::{next_account_info, AccountInfo},
        entrypoint::ProgramResult,
        msg,
        program_pack::Pack,
        pubkey::Pubkey,
        rent::Rent,
        sysvar::Sysvar,
    },
};
pub(super) fn process_init_lending_market(
    program_id: &Pubkey,
    owner: Pubkey,
    quote_currency: [u8; 32],
    accounts: &[AccountInfo],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let lending_market_info = next_account_info(account_info_iter)?;
    // next_account_info(account_info_iter)?; //fixme: clean up this account
    let rent = &Rent::from_account_info(next_account_info(account_info_iter)?)?;
    let token_program_id = next_account_info(account_info_iter)?;
    let oracle_program_id = next_account_info(account_info_iter)?;
    assert_rent_exempt(rent, lending_market_info)?;

    let mut lending_market = assert_uninitialized::<LendingMarket>(lending_market_info)?;

    if lending_market_info.owner != program_id {
        msg!("Lending market provided is not owned by the lending program");
        return Err(LendingError::InvalidAccountOwner)?;
    }
    lending_market.init(InitLendingMarketParams {
        bump_seed: Pubkey::find_program_address(&[lending_market_info.key.as_ref()], program_id).1,
        owner,
        quote_currency,
        token_program_id: *token_program_id.key,
        oracle_program_id: *oracle_program_id.key,
    });
    LendingMarket::pack(lending_market, &mut lending_market_info.data.borrow_mut())?;
    Ok(())
}
