use crate::instruction::LendingInstruction;
use crate::state::InitLendingMarketParams;
use crate::{error::LendingError, state::LendingMarket};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    program_pack::{IsInitialized, Pack},
    pubkey::Pubkey,
    rent::Rent,
    sysvar::Sysvar,
};

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    input: &[u8],
) -> ProgramResult {
    let instruction = LendingInstruction::unpack(input)?;
    match instruction {
        LendingInstruction::InitLendingMarket {
            owner,
            quote_currency,
        } => {
            msg!("Instruction: Init Lending Market");
            process_init_lending_market(program_id, owner, quote_currency, accounts)
        }
        _ => {
            msg!("Unsupported instruction");
            Err(LendingError::NotRentExempt.into())
        }
    }
}

fn process_init_lending_market(
    program_id: &Pubkey,
    owner: Pubkey,
    quote_currency: [u8; 32],
    accounts: &[AccountInfo],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let lending_market_info = next_account_info(account_info_iter)?;
    // next_account_info(account_info_iter)?; //fixme: clean up this account

    let token_program_id = next_account_info(account_info_iter)?;
    let oracle_program_id = next_account_info(account_info_iter)?;
    assert_rent_exempt(lending_market_info)?;
    let mut lending_market = assert_uninitialized::<LendingMarket>(lending_market_info)?;
    if lending_market_info.owner != program_id {
        msg!("Lending market provided is not owned by the lending program");
        return Err(LendingError::InvalidAccountOwner.into());
    }
    lending_market.init(InitLendingMarketParams {
        bump_seed: Pubkey::find_program_address(&[lending_market_info.key.as_ref()], program_id).1,
        owner,
        quote_currency,
        token_program_id: *token_program_id.key,
        oracle_program_id: *oracle_program_id.key,
    });
    Ok(())
}

fn assert_rent_exempt(account_info: &AccountInfo) -> ProgramResult {
    let rent = &Rent::get()?;
    if !rent.is_exempt(account_info.lamports(), account_info.data_len()) {
        msg!(&rent.minimum_balance(account_info.data_len()).to_string());
        Err(LendingError::NotRentExempt)?
    } else {
        Ok(())
    }
}

fn assert_uninitialized<T: IsInitialized + Pack>(
    account_info: &AccountInfo,
) -> Result<T, ProgramError> {
    let account = T::unpack(&account_info.data.borrow())?;
    if account.is_initialized() {
        Err(LendingError::AlreadyInitialized)?
    } else {
        Ok(account)
    }
}
