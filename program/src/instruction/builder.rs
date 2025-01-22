//a helper for rust clients to create instructions
use {
    super::LendingInstruction,
    crate::state::ReserveConfig,
    solana_program::{
        instruction::{AccountMeta, Instruction},
        pubkey::{Pubkey, PUBKEY_BYTES},
    },
};

/// Creates a 'DepositObligationCollateral' instruction.
#[allow(clippy::too_many_arguments)]
pub fn deposit_obligation_collateral(
    program_id: Pubkey,
    collateral_amount: u64,
    source_collateral_pubkey: Pubkey,
    destination_collateral_pubkey: Pubkey,
    deposit_reserve_pubkey: Pubkey,
    obligation_pubkey: Pubkey,
    lending_market_pubkey: Pubkey,
    obligation_owner_pubkey: Pubkey,
    user_transfer_authority_pubkey: Pubkey,
) -> Instruction {
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(source_collateral_pubkey, false),
            AccountMeta::new(destination_collateral_pubkey, false),
            AccountMeta::new_readonly(deposit_reserve_pubkey, false),
            AccountMeta::new(obligation_pubkey, false),
            AccountMeta::new_readonly(lending_market_pubkey, false),
            AccountMeta::new_readonly(obligation_owner_pubkey, true),
            AccountMeta::new_readonly(user_transfer_authority_pubkey, true),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: LendingInstruction::DepositObligationCollateral { collateral_amount }.pack(),
    }
}

/// Creates a `LiquidateObligation` instruction
#[allow(clippy::too_many_arguments)]
pub fn liquidate_obligation(
    program_id: Pubkey,
    liquidity_amount: u64,
    source_liquidity_pubkey: Pubkey,
    destination_collateral_pubkey: Pubkey,
    repay_reserve_pubkey: Pubkey,
    repay_reserve_liquidity_supply_pubkey: Pubkey,
    withdraw_reserve_pubkey: Pubkey,
    withdraw_reserve_collateral_supply_pubkey: Pubkey,
    obligation_pubkey: Pubkey,
    lending_market_pubkey: Pubkey,
    user_transfer_authority_pubkey: Pubkey,
) -> Instruction {
    let (lending_market_authority_pubkey, _bump_seed) = Pubkey::find_program_address(
        &[&lending_market_pubkey.to_bytes()[..PUBKEY_BYTES]],
        &program_id,
    );
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(source_liquidity_pubkey, false),
            AccountMeta::new(destination_collateral_pubkey, false),
            AccountMeta::new(repay_reserve_pubkey, false),
            AccountMeta::new(repay_reserve_liquidity_supply_pubkey, false),
            AccountMeta::new_readonly(withdraw_reserve_pubkey, false),
            AccountMeta::new(withdraw_reserve_collateral_supply_pubkey, false),
            AccountMeta::new(obligation_pubkey, false),
            AccountMeta::new_readonly(lending_market_pubkey, false),
            AccountMeta::new_readonly(lending_market_authority_pubkey, false),
            AccountMeta::new_readonly(user_transfer_authority_pubkey, true),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: LendingInstruction::LiquidateObligation { liquidity_amount }.pack(),
    }
}

/// Creates a 'ModifyReserveConfig` instruction.
#[allow(clippy::too_many_arguments)]
pub fn modify_reserve_config(
    program_id: Pubkey,
    config: ReserveConfig,
    reserve_pubkey: Pubkey,
    lending_market_pubkey: Pubkey,
    lending_market_owner_pubkey: Pubkey,
) -> Instruction {
    let accounts = vec![
        AccountMeta::new(reserve_pubkey, false),
        AccountMeta::new(lending_market_pubkey, false),
        AccountMeta::new(lending_market_owner_pubkey, true),
    ];
    Instruction {
        program_id,
        accounts,
        data: LendingInstruction::ModifyReserveConfig { new_config: config }.pack(),
    }
}

/// Creates a 'WithdrawObligationCollateral' instruction.
#[allow(clippy::too_many_arguments)]
pub fn withdraw_obligation_collateral(
    program_id: Pubkey,
    collateral_amount: u64,
    source_collateral_pubkey: Pubkey,
    destination_collateral_pubkey: Pubkey,
    withdraw_reserve_pubkey: Pubkey,
    obligation_pubkey: Pubkey,
    lending_market_pubkey: Pubkey,
    obligation_owner_pubkey: Pubkey,
) -> Instruction {
    let (lending_market_authority_pubkey, _bump_seed) = Pubkey::find_program_address(
        &[&lending_market_pubkey.to_bytes()[..PUBKEY_BYTES]],
        &program_id,
    );
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(source_collateral_pubkey, false),
            AccountMeta::new(destination_collateral_pubkey, false),
            AccountMeta::new_readonly(withdraw_reserve_pubkey, false),
            AccountMeta::new(obligation_pubkey, false),
            AccountMeta::new_readonly(lending_market_pubkey, false),
            AccountMeta::new_readonly(lending_market_authority_pubkey, false),
            AccountMeta::new_readonly(obligation_owner_pubkey, true),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: LendingInstruction::WithdrawObligationCollateral { collateral_amount }.pack(),
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
    let mut accounts = vec![AccountMeta::new(obligation_pubkey, false)];
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

/// Creates a 'DepositReserveLiquidity' instruction.
#[allow(clippy::too_many_arguments)]
pub fn deposit_reserve_liquidity(
    program_id: Pubkey,
    liquidity_amount: u64,
    source_liquidity_pubkey: Pubkey,
    destination_collateral_pubkey: Pubkey,
    reserve_pubkey: Pubkey,
    reserve_liquidity_supply_pubkey: Pubkey,
    reserve_collateral_mint_pubkey: Pubkey,
    lending_market_pubkey: Pubkey,
    user_transfer_authority_pubkey: Pubkey,
) -> Instruction {
    let (lending_market_authority_pubkey, _bump_seed) = Pubkey::find_program_address(
        &[&lending_market_pubkey.to_bytes()[..PUBKEY_BYTES]],
        &program_id,
    );
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(source_liquidity_pubkey, false),
            AccountMeta::new(destination_collateral_pubkey, false),
            AccountMeta::new(reserve_pubkey, false),
            AccountMeta::new(reserve_liquidity_supply_pubkey, false),
            AccountMeta::new(reserve_collateral_mint_pubkey, false),
            AccountMeta::new_readonly(lending_market_pubkey, false),
            AccountMeta::new_readonly(lending_market_authority_pubkey, false),
            AccountMeta::new_readonly(user_transfer_authority_pubkey, true),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: LendingInstruction::DepositReserveLiquidity { liquidity_amount }.pack(),
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

pub fn refresh_reserve(
    program_id: Pubkey,
    reserve_pubkey: Pubkey,
    reserve_liquidity_oracle_pubkey: Pubkey,
) -> Instruction {
    let accounts = vec![
        AccountMeta::new(reserve_pubkey, false),
        AccountMeta::new(reserve_liquidity_oracle_pubkey, false),
    ];

    Instruction {
        program_id,
        accounts,
        data: LendingInstruction::RefreshReserve.pack(),
    }
}
/// Creates a `RepayObligationLiquidity` instruction
#[allow(clippy::too_many_arguments)]
pub fn repay_obligation_liquidity(
    program_id: Pubkey,
    liquidity_amount: u64,
    source_liquidity_pubkey: Pubkey,
    destination_liquidity_pubkey: Pubkey,
    repay_reserve_pubkey: Pubkey,
    obligation_pubkey: Pubkey,
    lending_market_pubkey: Pubkey,
    user_transfer_authority_pubkey: Pubkey,
) -> Instruction {
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(source_liquidity_pubkey, false),
            AccountMeta::new(destination_liquidity_pubkey, false),
            AccountMeta::new(repay_reserve_pubkey, false),
            AccountMeta::new(obligation_pubkey, false),
            AccountMeta::new_readonly(lending_market_pubkey, false),
            AccountMeta::new_readonly(user_transfer_authority_pubkey, true),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: LendingInstruction::RepayObligationLiquidity { liquidity_amount }.pack(),
    }
}

/// Creates a 'RedeemReserveCollateral' instruction.
#[allow(clippy::too_many_arguments)]
pub fn redeem_reserve_collateral(
    program_id: Pubkey,
    collateral_amount: u64,
    source_collateral_pubkey: Pubkey,
    destination_liquidity_pubkey: Pubkey,
    reserve_pubkey: Pubkey,
    reserve_collateral_mint_pubkey: Pubkey,
    reserve_liquidity_supply_pubkey: Pubkey,
    lending_market_pubkey: Pubkey,
    user_transfer_authority_pubkey: Pubkey,
) -> Instruction {
    let (lending_market_authority_pubkey, _bump_seed) = Pubkey::find_program_address(
        &[&lending_market_pubkey.to_bytes()[..PUBKEY_BYTES]],
        &program_id,
    );
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(source_collateral_pubkey, false),
            AccountMeta::new(destination_liquidity_pubkey, false),
            AccountMeta::new(reserve_pubkey, false),
            AccountMeta::new(reserve_collateral_mint_pubkey, false),
            AccountMeta::new(reserve_liquidity_supply_pubkey, false),
            AccountMeta::new_readonly(lending_market_pubkey, false),
            AccountMeta::new_readonly(lending_market_authority_pubkey, false),
            AccountMeta::new_readonly(user_transfer_authority_pubkey, true),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: LendingInstruction::RedeemReserveCollateral { collateral_amount }.pack(),
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::state::ReserveFees};
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
    #[test]
    fn test_refresh_reserve() {
        let program_id = Pubkey::new_unique();
        let reserve_pubkey = Pubkey::new_unique();
        let reserve_liquidity_oracle_pubkey = Pubkey::new_unique();
        let instruction =
            refresh_reserve(program_id, reserve_pubkey, reserve_liquidity_oracle_pubkey);
        assert_eq!(instruction.program_id, program_id);
        assert_eq!(instruction.accounts.len(), 2);
        assert_eq!(instruction.data, LendingInstruction::RefreshReserve.pack());
    }

    #[test]
    fn test_init_reserve() {
        let program_id = Pubkey::new_unique();
        let liquidity_amount = u64::MAX;
        let config = ReserveConfig {
            optimal_utilization_rate: 50,
            loan_to_value_ratio: 1,
            liquidation_bonus: 10,
            liquidation_threshold: 5,
            min_borrow_rate: 2,
            optimal_borrow_rate: 4,
            max_borrow_rate: 10,
            fees: ReserveFees {
                borrow_fee_wad: 1,
                flash_loan_fee_wad: 3,
                host_fee_percentage: 1,
            },
        };
        let source_liquidity_pubkey = Pubkey::new_unique();
        let destination_collateral_pubkey = Pubkey::new_unique();
        let reserve_pubkey = Pubkey::new_unique();
        let reserve_liquidity_mint_pubkey = Pubkey::new_unique();
        let reserve_liquidity_supply_pubkey = Pubkey::new_unique();
        let reserve_liquidity_fee_receiver_pubkey = Pubkey::new_unique();
        let reserve_collateral_mint_pubkey = Pubkey::new_unique();
        let reserve_collateral_supply_pubkey = Pubkey::new_unique();
        let pyth_product_pubkey = Pubkey::new_unique();
        let pyth_price_pubkey = Pubkey::new_unique();
        let lending_market_pubkey = Pubkey::new_unique();
        let lending_market_owner_pubkey = Pubkey::new_unique();
        let user_transfer_authority_pubkey = Pubkey::new_unique();
        let instruction = init_reserve(
            program_id,
            liquidity_amount,
            config,
            source_liquidity_pubkey,
            destination_collateral_pubkey,
            reserve_pubkey,
            reserve_liquidity_mint_pubkey,
            reserve_liquidity_supply_pubkey,
            reserve_liquidity_fee_receiver_pubkey,
            reserve_collateral_mint_pubkey,
            reserve_collateral_supply_pubkey,
            pyth_product_pubkey,
            pyth_price_pubkey,
            lending_market_pubkey,
            lending_market_owner_pubkey,
            user_transfer_authority_pubkey,
        );
        assert_eq!(instruction.program_id, program_id);
        assert_eq!(instruction.accounts.len(), 15);
        assert_eq!(
            instruction.data,
            LendingInstruction::InitReserve {
                liquidity_amount,
                config,
            }
            .pack()
        );
    }
    #[test]
    fn test_deposit_reserve_liquidity() {
        let program_id = Pubkey::new_unique();
        let liquidity_amount = u64::MAX;
        let source_liquidity_pubkey = Pubkey::new_unique();
        let destination_collateral_pubkey = Pubkey::new_unique();
        let reserve_pubkey = Pubkey::new_unique();
        let reserve_liquidity_supply_pubkey = Pubkey::new_unique();
        let reserve_collateral_mint_pubkey = Pubkey::new_unique();
        let lending_market_pubkey = Pubkey::new_unique();
        let user_transfer_authority_pubkey = Pubkey::new_unique();
        let instruction = deposit_reserve_liquidity(
            program_id,
            liquidity_amount,
            source_liquidity_pubkey,
            destination_collateral_pubkey,
            reserve_pubkey,
            reserve_liquidity_supply_pubkey,
            reserve_collateral_mint_pubkey,
            lending_market_pubkey,
            user_transfer_authority_pubkey,
        );
        assert_eq!(instruction.program_id, program_id);
        assert_eq!(instruction.accounts.len(), 9);
        assert_eq!(
            instruction.data,
            LendingInstruction::DepositReserveLiquidity { liquidity_amount }.pack()
        );
    }
    #[test]
    fn test_redeem_reserve_collateral() {
        let program_id = Pubkey::new_unique();
        let collateral_amount = u64::MAX;
        let source_collateral_pubkey = Pubkey::new_unique();
        let destination_liquidity_pubkey = Pubkey::new_unique();
        let reserve_pubkey = Pubkey::new_unique();
        let reserve_collateral_mint_pubkey = Pubkey::new_unique();
        let reserve_liquidity_supply_pubkey = Pubkey::new_unique();
        let lending_market_pubkey = Pubkey::new_unique();
        let user_transfer_authority_pubkey = Pubkey::new_unique();
        let instruction = redeem_reserve_collateral(
            program_id,
            collateral_amount,
            source_collateral_pubkey,
            destination_liquidity_pubkey,
            reserve_pubkey,
            reserve_collateral_mint_pubkey,
            reserve_liquidity_supply_pubkey,
            lending_market_pubkey,
            user_transfer_authority_pubkey,
        );
        assert_eq!(instruction.program_id, program_id);
        assert_eq!(instruction.accounts.len(), 9);
        assert_eq!(
            instruction.data,
            LendingInstruction::RedeemReserveCollateral { collateral_amount }.pack()
        );
    }
    #[test]
    fn test_init_obligation() {
        let program_id = Pubkey::new_unique();
        let obligation_pubkey = Pubkey::new_unique();
        let lending_market_pubkey = Pubkey::new_unique();
        let obligation_owner_pubkey = Pubkey::new_unique();
        let instruction = init_obligation(
            program_id,
            obligation_pubkey,
            lending_market_pubkey,
            obligation_owner_pubkey,
        );
        assert_eq!(instruction.program_id, program_id);
        assert_eq!(instruction.accounts.len(), 4);
        assert_eq!(instruction.data, LendingInstruction::InitObligation.pack());
    }
    #[test]
    fn test_refresh_obligation() {
        let program_id = Pubkey::new_unique();
        let obligation_pubkey = Pubkey::new_unique();
        let reserve_pubkeys = vec![Pubkey::new_unique()];
        let instruction = refresh_obligation(program_id, obligation_pubkey, reserve_pubkeys);
        assert_eq!(instruction.program_id, program_id);
        assert_eq!(instruction.accounts.len(), 2);
        assert_eq!(
            instruction.data,
            LendingInstruction::RefreshObligation.pack()
        );
    }
    #[test]
    fn test_deposit_obligation_collateral() {
        let program_id = Pubkey::new_unique();
        let collateral_amount = u64::MAX;
        let source_collateral_pubkey = Pubkey::new_unique();
        let destination_collateral_pubkey = Pubkey::new_unique();
        let deposit_reserve_pubkey = Pubkey::new_unique();
        let obligation_pubkey = Pubkey::new_unique();
        let lending_market_pubkey = Pubkey::new_unique();
        let obligation_owner_pubkey = Pubkey::new_unique();
        let user_transfer_authority_pubkey = Pubkey::new_unique();
        let instruction = deposit_obligation_collateral(
            program_id,
            collateral_amount,
            source_collateral_pubkey,
            destination_collateral_pubkey,
            deposit_reserve_pubkey,
            obligation_pubkey,
            lending_market_pubkey,
            obligation_owner_pubkey,
            user_transfer_authority_pubkey,
        );
        assert_eq!(instruction.program_id, program_id);
        assert_eq!(instruction.accounts.len(), 8);
        assert_eq!(
            instruction.data,
            LendingInstruction::DepositObligationCollateral { collateral_amount }.pack()
        );
    }
    #[test]
    fn test_withdraw_obligation_collateral() {
        let program_id = Pubkey::new_unique();
        let collateral_amount = u64::MAX;
        let source_collateral_pubkey = Pubkey::new_unique();
        let destination_collateral_pubkey = Pubkey::new_unique();
        let withdraw_reserve_pubkey = Pubkey::new_unique();
        let obligation_pubkey = Pubkey::new_unique();
        let lending_market_pubkey = Pubkey::new_unique();
        let obligation_owner_pubkey = Pubkey::new_unique();
        let instruction = withdraw_obligation_collateral(
            program_id,
            collateral_amount,
            source_collateral_pubkey,
            destination_collateral_pubkey,
            withdraw_reserve_pubkey,
            obligation_pubkey,
            lending_market_pubkey,
            obligation_owner_pubkey,
        );
        assert_eq!(instruction.program_id, program_id);
        assert_eq!(instruction.accounts.len(), 8);
        assert_eq!(
            instruction.data,
            LendingInstruction::WithdrawObligationCollateral { collateral_amount }.pack()
        );
    }
    #[test]
    fn test_borrow_obligation_liquidity() {
        let program_id = Pubkey::new_unique();
        let liquidity_amount = u64::MAX;
        let source_liquidity_pubkey = Pubkey::new_unique();
        let destination_liquidity_pubkey = Pubkey::new_unique();
        let borrow_reserve_pubkey = Pubkey::new_unique();
        let borrow_reserve_liquidity_fee_receiver_pubkey = Pubkey::new_unique();
        let obligation_pubkey = Pubkey::new_unique();
        let lending_market_pubkey = Pubkey::new_unique();
        let obligation_owner_pubkey = Pubkey::new_unique();
        let host_fee_receiver_pubkey = Some(Pubkey::new_unique());
        let instruction = borrow_obligation_liquidity(
            program_id,
            liquidity_amount,
            None,
            source_liquidity_pubkey,
            destination_liquidity_pubkey,
            borrow_reserve_pubkey,
            borrow_reserve_liquidity_fee_receiver_pubkey,
            obligation_pubkey,
            lending_market_pubkey,
            obligation_owner_pubkey,
            host_fee_receiver_pubkey,
        );
        assert_eq!(instruction.program_id, program_id);
        assert_eq!(instruction.accounts.len(), 10);
        assert_eq!(
            instruction.data,
            LendingInstruction::BorrowObligationLiquidity {
                liquidity_amount,
                slippage_limit: 0
            }
            .pack()
        );
    }
    #[test]
    fn test_repay_obligation_liquidity() {
        let program_id = Pubkey::new_unique();
        let liquidity_amount = u64::MAX;
        let source_liquidity_pubkey = Pubkey::new_unique();
        let destination_liquidity_pubkey = Pubkey::new_unique();
        let repay_reserve_pubkey = Pubkey::new_unique();
        let obligation_pubkey = Pubkey::new_unique();
        let lending_market_pubkey = Pubkey::new_unique();
        let user_transfer_authority_pubkey = Pubkey::new_unique();
        let instruction = repay_obligation_liquidity(
            program_id,
            liquidity_amount,
            source_liquidity_pubkey,
            destination_liquidity_pubkey,
            repay_reserve_pubkey,
            obligation_pubkey,
            lending_market_pubkey,
            user_transfer_authority_pubkey,
        );
        assert_eq!(instruction.program_id, program_id);
        assert_eq!(instruction.accounts.len(), 7);
        assert_eq!(
            instruction.data,
            LendingInstruction::RepayObligationLiquidity { liquidity_amount }.pack()
        );
    }

    #[test]
    fn test_liquidate_obligation() {
        let program_id = Pubkey::new_unique();
        let liquidity_amount = u64::MAX;
        let source_liquidity_pubkey = Pubkey::new_unique();
        let destination_collateral_pubkey = Pubkey::new_unique();
        let repay_reserve_pubkey = Pubkey::new_unique();
        let repay_reserve_liquidity_supply_pubkey = Pubkey::new_unique();
        let withdraw_reserve_pubkey = Pubkey::new_unique();
        let withdraw_reserve_collateral_supply_pubkey = Pubkey::new_unique();
        let obligation_pubkey = Pubkey::new_unique();
        let lending_market_pubkey = Pubkey::new_unique();
        let user_transfer_authority_pubkey = Pubkey::new_unique();
        let instruction = liquidate_obligation(
            program_id,
            liquidity_amount,
            source_liquidity_pubkey,
            destination_collateral_pubkey,
            repay_reserve_pubkey,
            repay_reserve_liquidity_supply_pubkey,
            withdraw_reserve_pubkey,
            withdraw_reserve_collateral_supply_pubkey,
            obligation_pubkey,
            lending_market_pubkey,
            user_transfer_authority_pubkey,
        );
        assert_eq!(instruction.program_id, program_id);
        assert_eq!(instruction.accounts.len(), 11);
        assert_eq!(
            instruction.data,
            LendingInstruction::LiquidateObligation { liquidity_amount }.pack()
        );
    }

    // #[test]
    // fn test_flash_loan() {
    //     let program_id = Pubkey::new_unique();
    //     let amount = u64::MAX;
    //     let source_liquidity_pubkey = Pubkey::new_unique();
    //     let destination_liquidity_pubkey = Pubkey::new_unique();
    //     let repay_reserve_pubkey = Pubkey::new_unique();
    //     let reserve_liquidity_fee_receiver_pubkey = Pubkey::new_unique();
    //     let host_fee_receiver_pubkey = Pubkey::new_unique();
    //     let lending_market_pubkey = Pubkey::new_unique();
    //     let flash_loan_receiver_program_id = Pubkey::new_unique();
    //     let account_meta = AccountMeta {
    //         pubkey: Pubkey::new_unique(),
    //         is_signer: true,
    //         is_writable: false,
    //     };
    //     let flash_loan_receiver_program_accounts = vec![account_meta];
    //     let instruction = flash_loan(
    //         program_id,
    //         amount,
    //         source_liquidity_pubkey,
    //         destination_liquidity_pubkey,
    //         repay_reserve_pubkey,
    //         reserve_liquidity_fee_receiver_pubkey,
    //         host_fee_receiver_pubkey,
    //         lending_market_pubkey,
    //         flash_loan_receiver_program_id,
    //         flash_loan_receiver_program_accounts,
    //     );
    //     assert_eq!(instruction.program_id, program_id);
    //     assert_eq!(instruction.accounts.len(), 10);
    //     assert_eq!(
    //         instruction.data,
    //         LendingInstruction::FlashLoan { amount }.pack()
    //     );
    // }

    #[test]
    fn test_modify_reserve_config() {
        let program_id = Pubkey::new_unique();
        let config = ReserveConfig {
            optimal_utilization_rate: 60,
            loan_to_value_ratio: 1,
            liquidation_bonus: 10,
            liquidation_threshold: 5,
            min_borrow_rate: 2,
            optimal_borrow_rate: 4,
            max_borrow_rate: 10,
            fees: ReserveFees {
                borrow_fee_wad: 1,
                flash_loan_fee_wad: 3,
                host_fee_percentage: 1,
            },
        };
        let reserve_pubkey = Pubkey::new_unique();
        let lending_market_pubkey = Pubkey::new_unique();
        let lending_market_owner_pubkey = Pubkey::new_unique();
        let instruction = modify_reserve_config(
            program_id,
            config,
            reserve_pubkey,
            lending_market_pubkey,
            lending_market_owner_pubkey,
        );
        assert_eq!(instruction.program_id, program_id);
        assert_eq!(instruction.accounts.len(), 3);
        assert_eq!(
            instruction.data,
            LendingInstruction::ModifyReserveConfig { new_config: config }.pack()
        );
    }
}
