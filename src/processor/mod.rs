mod process_borrow_obligation_liquidity;
mod process_deposit_obligation_collateral;
mod process_deposit_reserve_liquidity;
mod process_init_lending_market;
mod process_init_obligation;
mod process_init_reserve;
mod process_liquidate_obligation;
mod process_modify_reserve_config;
mod process_redeem_reserve_collateral;
mod process_refresh_obligation;
mod process_refresh_reserve;
mod process_repay_obligation_liquidity;
mod process_set_lending_market_owner;
mod process_withdraw_obligation_collateral;
mod token_helper;
mod utils;

use {
    crate::{error::LendingError, instruction::LendingInstruction},
    process_borrow_obligation_liquidity::*,
    process_deposit_obligation_collateral::*,
    process_deposit_reserve_liquidity::*,
    process_init_lending_market::*,
    process_init_obligation::*,
    process_init_reserve::*,
    process_liquidate_obligation::*,
    process_modify_reserve_config::*,
    process_redeem_reserve_collateral::*,
    process_refresh_obligation::*,
    process_refresh_reserve::*,
    process_repay_obligation_liquidity::*,
    process_set_lending_market_owner::*,
    process_withdraw_obligation_collateral::*,
    solana_program::{account_info::AccountInfo, entrypoint::ProgramResult, msg, pubkey::Pubkey},
    token_helper::*,
    utils::*,
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
        LendingInstruction::SetLendingMarketOwner { new_owner } => {
            msg!("Instruction: Set Lending Market Owner");
            process_set_lending_market_owner(program_id, new_owner, accounts)
        }
        LendingInstruction::DepositReserveLiquidity { liquidity_amount } => {
            msg!("Instruction: Deposit Reserve Liquidity");
            process_deposit_reserve_liquidity(program_id, liquidity_amount, accounts)
        }
        LendingInstruction::InitReserve {
            liquidity_amount,
            config,
        } => {
            msg!("Instruction: Init Reserve");
            process_init_reserve(program_id, liquidity_amount, config, accounts)
        }
        LendingInstruction::RefreshReserve => {
            msg!("Instruction: Refresh Reserve");
            process_refresh_reserve(program_id, accounts)
        }

        LendingInstruction::InitObligation => {
            msg!("Instruction: Init Obligation");
            process_init_obligation(program_id, accounts)
        }
        LendingInstruction::DepositObligationCollateral { collateral_amount } => {
            msg!("Instruction: Deposit Obligation Collateral");
            process_deposit_obligation_collateral(program_id, collateral_amount, accounts)
        }
        LendingInstruction::WithdrawObligationCollateral { collateral_amount } => {
            msg!("Instruction: Withdraw Obligation Collateral");
            process_withdraw_obligation_collateral(program_id, collateral_amount, accounts)
        }
        LendingInstruction::RefreshObligation => {
            msg!("Instruction: Refresh Obligation");
            process_refresh_obligation(program_id, accounts)
        }
        LendingInstruction::BorrowObligationLiquidity {
            liquidity_amount,
            slippage_limit,
        } => {
            msg!("Instruction: Borrow Obligation Liquidity");
            process_borrow_obligation_liquidity(
                program_id,
                liquidity_amount,
                slippage_limit,
                accounts,
            )
        }
        LendingInstruction::LiquidateObligation { liquidity_amount } => {
            msg!("Instruction: Liquidate Obligation");
            process_liquidate_obligation(program_id, liquidity_amount, accounts)
        }
        LendingInstruction::RepayObligationLiquidity { liquidity_amount } => {
            msg!("Instruction: Repay Obligation Liquidity");
            process_repay_obligation_liquidity(program_id, liquidity_amount, accounts)
        }
        LendingInstruction::RedeemReserveCollateral { collateral_amount } => {
            msg!("Instruction: Redeem Reserve Collateral");
            process_redeem_reserve_collateral(program_id, collateral_amount, accounts)
        }
        LendingInstruction::ModifyReserveConfig { new_config } => {
            msg!("Instruction: Modify Reserve Config");
            process_modify_reserve_config(program_id, new_config, accounts)
        }
        _ => {
            msg!("Unsupported instruction");
            Err(LendingError::NotRentExempt.into())
        }
    }
}
