use {
    crate::{
        error::LendingError,
        state::{ReserveConfig, ReserveFees},
    },
    solana_program::{
        instruction::{AccountMeta, Instruction},
        msg,
        program_error::ProgramError,
        pubkey::{Pubkey, PUBKEY_BYTES},
        sysvar,
    },
    std::{convert::TryInto, mem::size_of},
};

/// Instructions supported by the lending program.
#[derive(Debug, PartialEq)]
pub enum LendingInstruction {
    // 0
    /// Initializes a new lending market.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   0. `[writable]` Lending market account - uninitialized.
    ///   1. `[]` Rent sysvar.
    ///   2. `[]` Token program id.
    ///   3. `[]` Oracle program id.
    InitLendingMarket {
        /// Owner authority which can add new reserves
        owner: Pubkey,
        /// Currency market prices are quoted in
        /// e.g. "USD" null padded
        /// (`*b"USD\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0"
        /// `) or SPL token mint pubkey
        quote_currency: [u8; 32],
    },
    // 1
    /// Sets the new owner of a lending market.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   0. `[writable]` Lending market account.
    ///   1. `[signer]` Current owner.
    SetLendingMarketOwner {
        /// The new owner
        new_owner: Pubkey,
    },
    // 2
    /// Initializes a new lending market reserve.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   0. `[writable]` Source liquidity token account. $authority can
    ///      transfer $liquidity_amount.
    ///   1. `[writable]` Destination collateral token account - uninitialized.
    ///   2. `[writable]` Reserve account - uninitialized.
    ///   3. `[]` Reserve liquidity SPL Token mint.
    ///   4. `[writable]` Reserve liquidity supply SPL Token account -
    ///      uninitialized.
    ///   5. `[writable]` Reserve liquidity fee receiver - uninitialized.
    ///   6. `[writable]` Reserve collateral SPL Token mint - uninitialized.
    ///   7. `[writable]` Reserve collateral token supply - uninitialized.
    ///   8. `[]` Pyth product account.
    ///   9. `[]` Pyth price account. This will be used as the reserve liquidity
    ///      oracle account.
    ///   10. `[]` Lending market account.
    ///   11. `[]` Derived lending market authority.
    ///   12. `[signer]` Lending market owner.
    ///   13. `[signer]` User transfer authority ($authority).
    ///   14. `[]` Clock sysvar.
    ///   15. `[]` Rent sysvar.
    ///   16. `[]` Token program id.
    InitReserve {
        /// Initial amount of liquidity to deposit into the new reserve
        liquidity_amount: u64,
        /// Reserve configuration values
        config: ReserveConfig,
    },
    // 3
    /// Accrue interest and update market price of liquidity on a reserve.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   0. `[writable]` Reserve account.
    ///   1. `[]` Reserve liquidity oracle account. Must be the Pyth price
    ///      account specified at InitReserve.
    ///   2. `[]` Clock sysvar.
    RefreshReserve,
    // 4
    /// Deposit liquidity into a reserve in exchange for collateral. Collateral
    /// represents a share of the reserve liquidity pool.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   0. `[writable]` Source liquidity token account. $authority can
    ///      transfer $liquidity_amount.
    ///   1. `[writable]` Destination collateral token account.
    ///   2. `[writable]` Reserve account.
    ///   3. `[writable]` Reserve liquidity supply SPL Token account.
    ///   4. `[writable]` Reserve collateral SPL Token mint.
    ///   5. `[]` Lending market account.
    ///   6. `[]` Derived lending market authority.
    ///   7. `[signer]` User transfer authority ($authority).
    ///   8. `[]` Clock sysvar.
    ///   9. `[]` Token program id.
    DepositReserveLiquidity {
        /// Amount of liquidity to deposit in exchange for collateral tokens
        liquidity_amount: u64,
    },
    // 5
    /// Redeem collateral from a reserve in exchange for liquidity.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   0. `[writable]` Source collateral token account. $authority can
    ///      transfer $collateral_amount.
    ///   1. `[writable]` Destination liquidity token account.
    ///   2. `[writable]` Reserve account.
    ///   3. `[writable]` Reserve collateral SPL Token mint.
    ///   4. `[writable]` Reserve liquidity supply SPL Token account.
    ///   5. `[]` Lending market account.
    ///   6. `[]` Derived lending market authority.
    ///   7. `[signer]` User transfer authority ($authority).
    ///   8. `[]` Clock sysvar.
    ///   9. `[]` Token program id.
    RedeemReserveCollateral {
        /// Amount of collateral tokens to redeem in exchange for liquidity
        collateral_amount: u64,
    },
    // 6
    /// Initializes a new lending market obligation.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   0. `[writable]` Obligation account - uninitialized.
    ///   1. `[]` Lending market account.
    ///   2. `[signer]` Obligation owner.
    ///   3. `[]` Clock sysvar.
    ///   4. `[]` Rent sysvar.
    ///   5. `[]` Token program id.
    InitObligation,
    // 7
    /// Refresh an obligation's accrued interest and collateral and liquidity
    /// prices. Requires refreshed reserves, as all obligation collateral
    /// deposit reserves in order, followed by all liquidity borrow reserves
    /// in order.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   0. `[writable]` Obligation account.
    ///   1. `[]` Clock sysvar.
    ///   2. .. `[]` Collateral deposit reserve accounts - refreshed, all, in
    ///      order.
    ///   3. .. `[]` Liquidity borrow reserve accounts - refreshed, all, in
    ///      order.
    RefreshObligation,
    // 10
    /// Borrow liquidity from a reserve by depositing collateral tokens.
    /// Requires a refreshed obligation and reserve.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   0. `[writable]` Source borrow reserve liquidity supply SPL Token
    ///      account.
    ///   1. `[writable]` Destination liquidity token account. Minted by borrow
    ///      reserve liquidity mint.
    ///   2. `[writable]` Borrow reserve account - refreshed.
    ///   3. `[writable]` Borrow reserve liquidity fee receiver account. Must be
    ///      the fee account specified at InitReserve.
    ///   4. `[writable]` Obligation account - refreshed.
    ///   5. `[]` Lending market account.
    ///   6. `[]` Derived lending market authority.
    ///   7. `[signer]` Obligation owner.
    ///   8. `[]` Clock sysvar.
    ///   9. `[]` Token program id.
    ///   10. `[optional, writable]` Host fee receiver account.
    BorrowObligationLiquidity {
        /// Amount of liquidity to borrow - u64::MAX for 100% of borrowing power
        liquidity_amount: u64,
        /// Minimum amount of liquidity to receive, if borrowing 100% of
        /// borrowing power
        slippage_limit: u64,
    },
}

impl LendingInstruction {
    pub fn unpack(input: &[u8]) -> Result<Self, ProgramError> {
        let (&tag, rest) = input
            .split_first()
            .ok_or(LendingError::InstructionUnpackError)?;
        Ok(match tag {
            0 => {
                let (owner, rest) = Self::unpack_pubkey(rest)?;
                let (quote_currency, _) = Self::unpack_bytes32(rest)?;
                Self::InitLendingMarket {
                    owner,
                    quote_currency: *quote_currency,
                }
            }
            1 => {
                let (new_owner, _rest) = Self::unpack_pubkey(rest)?;
                Self::SetLendingMarketOwner { new_owner }
            }
            2 => {
                let (liquidity_amount, rest) = Self::unpack_u64(rest)?;
                let config = Self::unpack_reserve_config(rest)?;
                Self::InitReserve {
                    liquidity_amount,
                    config,
                }
            }
            6 => Self::InitObligation,
            7 => Self::RefreshObligation,
            10 => {
                let (liquidity_amount, rest) = Self::unpack_u64(rest)?;
                let (slippage_limit, _rest) = Self::unpack_u64(rest).unwrap_or((0, &[]));
                Self::BorrowObligationLiquidity {
                    liquidity_amount,
                    slippage_limit,
                }
            }

            _ => unreachable!(),
        })
    }

    pub fn pack(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(size_of::<Self>());
        match *self {
            LendingInstruction::InitLendingMarket {
                owner,
                quote_currency,
            } => {
                buf.push(0);
                buf.extend_from_slice(owner.as_ref());
                buf.extend_from_slice(quote_currency.as_ref());
            }
            Self::SetLendingMarketOwner { new_owner } => {
                buf.push(1);
                buf.extend_from_slice(new_owner.as_ref());
            }
            LendingInstruction::InitReserve {
                liquidity_amount,
                config,
            } => {
                buf.push(2);
                buf.extend_from_slice(&liquidity_amount.to_le_bytes());
                Self::extend_buffer_from_reserve_config(&mut buf, &config);
            }
            Self::InitObligation => {
                buf.push(6);
            }
            Self::RefreshObligation => {
                buf.push(7);
            }
            Self::BorrowObligationLiquidity {
                liquidity_amount,
                slippage_limit,
            } => {
                buf.push(10);
                buf.extend_from_slice(&liquidity_amount.to_le_bytes());
                buf.extend_from_slice(&slippage_limit.to_le_bytes());
            }

            _ => unreachable!(),
        }

        buf
    }

    pub fn unpack_pubkey(input: &[u8]) -> Result<(Pubkey, &[u8]), ProgramError> {
        if input.len() < PUBKEY_BYTES {
            return Err(LendingError::InstructionUnpackError)?;
        }
        let (pubkey, rest) = input.split_at(PUBKEY_BYTES);
        let pubkey = Pubkey::try_from(pubkey).map_err(|_| LendingError::InstructionUnpackError)?;
        Ok((pubkey, rest))
    }

    fn unpack_u8(input: &[u8]) -> Result<(u8, &[u8]), ProgramError> {
        if input.is_empty() {
            msg!("u8 cannot be unpacked");
            return Err(LendingError::InstructionUnpackError.into());
        }
        let (bytes, rest) = input.split_at(1);
        let value = bytes
            .get(..1)
            .and_then(|b| b.try_into().ok())
            .map(u8::from_le_bytes)
            .ok_or(LendingError::InstructionUnpackError)?;
        Ok((value, rest))
    }

    fn unpack_bytes32(input: &[u8]) -> Result<(&[u8; 32], &[u8]), ProgramError> {
        if input.len() < 32 {
            msg!("32 bytes cannot be unpacked");
            return Err(LendingError::InstructionUnpackError.into());
        }
        let (bytes, rest) = input.split_at(32);
        Ok((
            bytes
                .try_into()
                .map_err(|_| LendingError::InstructionUnpackError)?,
            rest,
        ))
    }
    fn unpack_u64(input: &[u8]) -> Result<(u64, &[u8]), ProgramError> {
        if input.len() < 8 {
            msg!("u64 cannot be unpacked");
            return Err(LendingError::InstructionUnpackError.into());
        }
        let (bytes, rest) = input.split_at(8);
        let value = bytes
            .get(..8)
            .and_then(|slice| slice.try_into().ok())
            .map(u64::from_le_bytes)
            .ok_or(LendingError::InstructionUnpackError)?;
        Ok((value, rest))
    }
    fn unpack_reserve_config(input: &[u8]) -> Result<ReserveConfig, ProgramError> {
        let (optimal_utilization_rate, rest) = Self::unpack_u8(input)?;
        let (loan_to_value_ratio, rest) = Self::unpack_u8(rest)?;
        let (liquidation_bonus, rest) = Self::unpack_u8(rest)?;
        let (liquidation_threshold, rest) = Self::unpack_u8(rest)?;
        let (min_borrow_rate, rest) = Self::unpack_u8(rest)?;
        let (optimal_borrow_rate, rest) = Self::unpack_u8(rest)?;
        let (max_borrow_rate, rest) = Self::unpack_u8(rest)?;
        let (borrow_fee_wad, rest) = Self::unpack_u64(rest)?;
        let (flash_loan_fee_wad, rest) = Self::unpack_u64(rest)?;
        let (host_fee_percentage, _rest) = Self::unpack_u8(rest)?;

        Ok(ReserveConfig {
            optimal_utilization_rate,
            loan_to_value_ratio,
            liquidation_bonus,
            liquidation_threshold,
            min_borrow_rate,
            optimal_borrow_rate,
            max_borrow_rate,
            fees: ReserveFees {
                borrow_fee_wad,
                flash_loan_fee_wad,
                host_fee_percentage,
            },
        })
    }
    // Helper function to pack a ReserveConfig into a Vec<u8> buffer
    fn extend_buffer_from_reserve_config(buf: &mut Vec<u8>, config: &ReserveConfig) {
        buf.extend_from_slice(&config.optimal_utilization_rate.to_le_bytes());
        buf.extend_from_slice(&config.loan_to_value_ratio.to_le_bytes());
        buf.extend_from_slice(&config.liquidation_bonus.to_le_bytes());
        buf.extend_from_slice(&config.liquidation_threshold.to_le_bytes());
        buf.extend_from_slice(&config.min_borrow_rate.to_le_bytes());
        buf.extend_from_slice(&config.optimal_borrow_rate.to_le_bytes());
        buf.extend_from_slice(&config.max_borrow_rate.to_le_bytes());
        buf.extend_from_slice(&config.fees.borrow_fee_wad.to_le_bytes());
        buf.extend_from_slice(&config.fees.flash_loan_fee_wad.to_le_bytes());
        buf.extend_from_slice(&config.fees.host_fee_percentage.to_le_bytes());
    }
}

pub fn init_lending_market(
    program_id: Pubkey,
    owner: Pubkey,
    quote_currency: [u8; 32],
    lending_market_pubkey: Pubkey,
    oracle_program_id: Pubkey,
) -> Instruction {
    println!(
        "init_lending_market lending_market_pubkey: {:?}",
        lending_market_pubkey
    );
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(lending_market_pubkey, false),
            AccountMeta::new_readonly(sysvar::rent::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(oracle_program_id, false),
        ],
        data: LendingInstruction::InitLendingMarket {
            owner,
            quote_currency,
        }
        .pack(),
    }
}
/// Creates a 'SetLendingMarketOwner' instruction.
pub fn set_lending_market_owner(
    program_id: Pubkey,
    lending_market_pubkey: Pubkey,
    lending_market_owner: Pubkey,
    new_owner: Pubkey,
) -> Instruction {
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(lending_market_pubkey, false),
            AccountMeta::new_readonly(lending_market_owner, true),
        ],
        data: LendingInstruction::SetLendingMarketOwner { new_owner }.pack(),
    }
}
pub fn init_reserve(
    program_id: Pubkey,
    liquidity_amount: u64,
    config: ReserveConfig,
    source_liquidity_pubkey: Pubkey,
    destination_collateral_pubkey: Pubkey,
    reserve_pubkey: Pubkey,
    reserve_liquidity_mint_pubkey: Pubkey,
    reserve_liquidity_supply_pubkey: Pubkey,
    reserve_liquidity_fee_receiver_pubkey: Pubkey,
    reserve_collateral_mint_pubkey: Pubkey,
    reserve_collateral_supply_pubkey: Pubkey,
    pyth_product_pubkey: Pubkey,
    pyth_price_pubkey: Pubkey,
    lending_market_pubkey: Pubkey,
    lending_market_owner_pubkey: Pubkey,
    user_transfer_authority_pubkey: Pubkey,
) -> Instruction {
    let (lending_market_authority_pubkey, _bump_seed) = Pubkey::find_program_address(
        &[&lending_market_pubkey.to_bytes()[..PUBKEY_BYTES]],
        &program_id,
    );
    let accounts = vec![
        AccountMeta::new(source_liquidity_pubkey, false),
        AccountMeta::new(destination_collateral_pubkey, false),
        AccountMeta::new(reserve_pubkey, false),
        AccountMeta::new_readonly(reserve_liquidity_mint_pubkey, false),
        AccountMeta::new(reserve_liquidity_supply_pubkey, false),
        AccountMeta::new(reserve_liquidity_fee_receiver_pubkey, false),
        AccountMeta::new(reserve_collateral_mint_pubkey, false),
        AccountMeta::new(reserve_collateral_supply_pubkey, false),
        AccountMeta::new_readonly(pyth_product_pubkey, false),
        AccountMeta::new_readonly(pyth_price_pubkey, false),
        AccountMeta::new_readonly(lending_market_pubkey, false),
        AccountMeta::new_readonly(lending_market_authority_pubkey, false),
        AccountMeta::new_readonly(lending_market_owner_pubkey, true),
        AccountMeta::new_readonly(user_transfer_authority_pubkey, true),
        AccountMeta::new_readonly(sysvar::clock::id(), false),
        AccountMeta::new_readonly(sysvar::rent::id(), false),
        AccountMeta::new_readonly(spl_token::id(), false),
    ];
    Instruction {
        program_id,
        accounts,
        data: LendingInstruction::InitReserve {
            liquidity_amount,
            config,
        }
        .pack(),
    }
}

/// Creates an 'InitObligation' instruction.
#[allow(clippy::too_many_arguments)]
pub fn init_obligation(
    program_id: Pubkey,
    obligation_pubkey: Pubkey,
    lending_market_pubkey: Pubkey,
    obligation_owner_pubkey: Pubkey,
) -> Instruction {
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(obligation_pubkey, false),
            AccountMeta::new_readonly(lending_market_pubkey, false),
            AccountMeta::new_readonly(obligation_owner_pubkey, true),
            AccountMeta::new_readonly(sysvar::clock::id(), false),
            AccountMeta::new_readonly(sysvar::rent::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: LendingInstruction::InitObligation.pack(),
    }
}
pub fn refresh_obligation(
    program_id: Pubkey,
    obligation_pubkey: Pubkey,
    reserve_pubkeys: Vec<Pubkey>,
) -> Instruction {
    let mut accounts = vec![
        AccountMeta::new(obligation_pubkey, false),
        AccountMeta::new_readonly(sysvar::clock::id(), false),
    ];
    accounts.extend(
        reserve_pubkeys
            .into_iter()
            .map(|pubkey| AccountMeta::new_readonly(pubkey, false)),
    );

    Instruction {
        program_id,
        accounts,
        data: LendingInstruction::RefreshObligation.pack(),
    }
}

/// Creates a 'BorrowObligationLiquidity' instruction.
#[allow(clippy::too_many_arguments)]
pub fn borrow_obligation_liquidity(
    program_id: Pubkey,
    liquidity_amount: u64,
    slippage_limit: Option<u64>,
    source_liquidity_pubkey: Pubkey,
    destination_liquidity_pubkey: Pubkey,
    borrow_reserve_pubkey: Pubkey,
    borrow_reserve_liquidity_fee_receiver_pubkey: Pubkey,
    obligation_pubkey: Pubkey,
    lending_market_pubkey: Pubkey,
    obligation_owner_pubkey: Pubkey,
    host_fee_receiver_pubkey: Option<Pubkey>,
) -> Instruction {
    let (lending_market_authority_pubkey, _bump_seed) = Pubkey::find_program_address(
        &[&lending_market_pubkey.to_bytes()[..PUBKEY_BYTES]],
        &program_id,
    );
    let mut accounts = vec![
        AccountMeta::new(source_liquidity_pubkey, false),
        AccountMeta::new(destination_liquidity_pubkey, false),
        AccountMeta::new(borrow_reserve_pubkey, false),
        AccountMeta::new(borrow_reserve_liquidity_fee_receiver_pubkey, false),
        AccountMeta::new(obligation_pubkey, false),
        AccountMeta::new_readonly(lending_market_pubkey, false),
        AccountMeta::new_readonly(lending_market_authority_pubkey, false),
        AccountMeta::new_readonly(obligation_owner_pubkey, true),
        AccountMeta::new_readonly(sysvar::clock::id(), false),
        AccountMeta::new_readonly(spl_token::id(), false),
    ];
    if let Some(host_fee_receiver_pubkey) = host_fee_receiver_pubkey {
        accounts.push(AccountMeta::new(host_fee_receiver_pubkey, false));
    }
    let slippage_limit = slippage_limit.unwrap_or(0);
    Instruction {
        program_id,
        accounts,
        data: LendingInstruction::BorrowObligationLiquidity {
            liquidity_amount,
            slippage_limit,
        }
        .pack(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_init_lending_market() {
        let program_id = Pubkey::new_unique();
        let owner = Pubkey::new_unique();
        let lending_market_pubkey = Pubkey::new_unique();
        let oracle_program_id = Pubkey::new_unique();
        let currency = "USD";
        let mut quote_currency = [0u8; 32];
        quote_currency[0..currency.len()].clone_from_slice(currency.as_bytes());
        let instruction = init_lending_market(
            program_id,
            owner,
            quote_currency,
            lending_market_pubkey,
            oracle_program_id,
        );
        assert_eq!(instruction.program_id, program_id);
        assert_eq!(instruction.accounts.len(), 3);

        // Print instruction data
        let data = instruction.data.clone();
        println!("Instruction data: {:?}", data);

        assert_eq!(
            instruction.data,
            LendingInstruction::InitLendingMarket {
                owner,
                quote_currency,
            }
            .pack()
        );
    }
    #[test]
    fn test_set_lending_market_owner() {
        let program_id = Pubkey::new_unique();
        let lending_market_pubkey = Pubkey::new_unique();
        let lending_market_owner = Pubkey::new_unique();
        let new_owner = Pubkey::new_unique();
        let instruction = set_lending_market_owner(
            program_id,
            lending_market_pubkey,
            lending_market_owner,
            new_owner,
        );
        assert_eq!(instruction.program_id, program_id);
        assert_eq!(instruction.accounts.len(), 2);
        assert_eq!(
            instruction.data,
            LendingInstruction::SetLendingMarketOwner { new_owner }.pack()
        );
    }
}
