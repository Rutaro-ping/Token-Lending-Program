use instruction::LendingInstruction;
use solana_program::account_info::AccountInfo;

use solana_program::entrypoint::ProgramResult;
use solana_program::pubkey::Pubkey;

pub mod entrypoint;
pub mod error;
pub mod instruction;
pub mod processor;
pub mod state;
solana_program::declare_id!("6TvznH3B2e3p2mbhufNBpgSrLx6UkgvxtVQvopEZ2kuH");

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let instruction =  LendingInstruction::unpack(instruction_data)?;
    todo!()
}
