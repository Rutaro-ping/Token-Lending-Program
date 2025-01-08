//methods for the lending instruction enum, mainly used for processing
// instructions in solana programs
use {
    super::LendingInstruction,
    crate::{
        error::LendingError,
        state::reserve::{ReserveConfig, ReserveFees},
    },
    solana_program::{
        msg,
        program_error::ProgramError,
        pubkey::{Pubkey, PUBKEY_BYTES},
    },
};

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
            3 => Self::RefreshReserve,
            4 => {
                let (liquidity_amount, _) = Self::unpack_u64(rest)?;
                Self::DepositReserveLiquidity { liquidity_amount }
            }
            6 => Self::InitObligation,
            7 => Self::RefreshObligation,
            8 => {
                let (collateral_amount, _) = Self::unpack_u64(rest)?;
                Self::DepositObligationCollateral { collateral_amount }
            }
            9 => {
                let (collateral_amount, _rest) = Self::unpack_u64(rest)?;
                Self::WithdrawObligationCollateral { collateral_amount }
            }
            10 => {
                let (liquidity_amount, rest) = Self::unpack_u64(rest)?;
                let (slippage_limit, _rest) = Self::unpack_u64(rest).unwrap_or((0, &[]));
                Self::BorrowObligationLiquidity {
                    liquidity_amount,
                    slippage_limit,
                }
            }
            11 => {
                let (liquidity_amount, _rest) = Self::unpack_u64(rest)?;
                Self::RepayObligationLiquidity { liquidity_amount }
            }
            12 => {
                let (collateral_amount, _rest) = Self::unpack_u64(rest)?;
                Self::RedeemReserveCollateral { collateral_amount }
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
            Self::InitReserve {
                liquidity_amount,
                config,
            } => {
                buf.push(2);
                buf.extend_from_slice(&liquidity_amount.to_le_bytes());
                Self::extend_buffer_from_reserve_config(&mut buf, &config);
            }
            Self::RefreshReserve => {
                buf.push(3);
            }
            Self::DepositReserveLiquidity { liquidity_amount } => {
                buf.push(4);
                buf.extend_from_slice(&liquidity_amount.to_le_bytes());
            }
            Self::InitObligation => {
                buf.push(6);
            }
            Self::RefreshObligation => {
                buf.push(7);
            }
            Self::DepositObligationCollateral { collateral_amount } => {
                buf.push(8);
                buf.extend_from_slice(&collateral_amount.to_le_bytes());
            }
            Self::BorrowObligationLiquidity {
                liquidity_amount,
                slippage_limit,
            } => {
                buf.push(10);
                buf.extend_from_slice(&liquidity_amount.to_le_bytes());
                buf.extend_from_slice(&slippage_limit.to_le_bytes());
            }
            Self::RepayObligationLiquidity { liquidity_amount } => {
                buf.push(11);
                buf.extend_from_slice(&liquidity_amount.to_le_bytes());
            }
            Self::RedeemReserveCollateral { collateral_amount } => {
                buf.push(12);
                buf.extend_from_slice(&collateral_amount.to_le_bytes());
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
