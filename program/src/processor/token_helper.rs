use {
    crate::{
        debug_msg,
        error::LendingError,
        math::{Decimal, TryDiv, TryMul},
        pyth,
        utils::get_pow,
    },
    solana_program::{
        account_info::AccountInfo,
        clock::Clock,
        entrypoint::ProgramResult,
        instruction::Instruction,
        msg,
        program::{invoke, invoke_signed},
        program_error::ProgramError,
        program_pack::Pack,
        pubkey::Pubkey,
    },
    spl_token::state::Mint,
};

/// Issue a spl_token `Transfer` instruction.
#[inline(always)]
pub(super) fn spl_token_transfer(params: TokenTransferParams<'_, '_>) -> ProgramResult {
    let TokenTransferParams {
        source,
        destination,
        authority,
        token_program,
        amount,
        authority_signer_seeds,
    } = params;
    let result = invoke_optionally_signed(
        &spl_token::instruction::transfer(
            token_program.key,
            source.key,
            destination.key,
            authority.key,
            &[],
            amount,
        )?,
        &[source, destination, authority, token_program],
        authority_signer_seeds,
    );
    result.map_err(|e| {
        debug_msg!("Error in spl_token_transfer: {}", e);
        LendingError::TokenTransferFailed.into()
    })
}

#[inline(always)]
fn invoke_optionally_signed(
    ix: &Instruction,
    accounts: &[AccountInfo],
    authority_signer_seeds: &[&[u8]],
) -> ProgramResult {
    if authority_signer_seeds.is_empty() {
        invoke(ix, accounts)
    } else {
        invoke_signed(ix, accounts, &[authority_signer_seeds])
    }
}

pub(super) fn get_pyth_product_quote_currency(
    pyth_product: &pyth::Product,
) -> Result<[u8; 32], ProgramError> {
    // 1 byte for key length
    // N bytes for key content
    // 1 byte for value length
    // M bytes for value content
    const LEN: usize = 14;
    const KEY: &[u8; LEN] = b"quote_currency";

    let mut start = 0;
    while start < pyth::PROD_ATTR_SIZE {
        let mut length = pyth_product.attr[start] as usize;
        start += 1;

        if length == LEN {
            let mut end = start + length;
            if end > pyth::PROD_ATTR_SIZE {
                msg!("Pyth product attribute key length too long");
                return Err(LendingError::InvalidOracleConfig.into());
            }

            let key = &pyth_product.attr[start..end];
            if key == KEY {
                start += length;
                length = pyth_product.attr[start] as usize;
                start += 1;

                end = start + length;
                if length > 32 || end > pyth::PROD_ATTR_SIZE {
                    msg!("Pyth product quote currency value too long");
                    return Err(LendingError::InvalidOracleConfig.into());
                }

                let mut value = [0u8; 32];
                value[0..length].copy_from_slice(&pyth_product.attr[start..end]);
                return Ok(value);
            }
        }

        start += length;
        start += 1 + pyth_product.attr[start] as usize;
    }

    msg!("Pyth product quote currency not found");
    Err(LendingError::InvalidOracleConfig.into())
}

pub(super) fn get_pyth_price(
    pyth_price_info: &AccountInfo,
    clock: &Clock,
) -> Result<Decimal, ProgramError> {
    #[cfg(feature = "test-sbf")]
    const STALE_AFTER_SLOTS_ELAPSED: u64 = 100 * crate::state::SLOTS_PER_YEAR;
    #[cfg(not(feature = "test-sbf"))]
    const STALE_AFTER_SLOTS_ELAPSED: u64 = 3;
    let pyth_price_data = pyth_price_info.try_borrow_data()?;
    let pyth_price = pyth::load::<pyth::Price>(&pyth_price_data)
        .map_err(|_| ProgramError::InvalidAccountData)?;

    if pyth_price.ptype != pyth::PriceType::Price {
        msg!("Oracle price type is invalid");
        return Err(LendingError::InvalidOracleConfig.into());
    }

    if pyth_price.agg.status != pyth::PriceStatus::Trading {
        msg!("Oracle price status is invalid");
        return Err(LendingError::InvalidOracleConfig.into());
    }

    let slots_elapsed = clock
        .slot
        .checked_sub(pyth_price.valid_slot)
        .ok_or(LendingError::MathOverflow)?;
    if slots_elapsed >= STALE_AFTER_SLOTS_ELAPSED {
        msg!("Oracle price is stale");
        return Err(LendingError::InvalidOracleConfig.into());
    }

    let price: u64 = pyth_price.agg.price.try_into().map_err(|_| {
        msg!("Oracle price cannot be negative");
        LendingError::InvalidOracleConfig
    })?;

    let market_price = if pyth_price.expo >= 0 {
        let exponent = pyth_price
            .expo
            .try_into()
            .map_err(|_| LendingError::MathOverflow)?;
        let zeros = get_pow(exponent)?;
        Decimal::from(price).try_mul(zeros)?
    } else {
        let exponent = pyth_price
            .expo
            .checked_abs()
            .ok_or(LendingError::MathOverflow)?
            .try_into()
            .map_err(|_| LendingError::MathOverflow)?;
        let decimals = get_pow(exponent)?;
        Decimal::from(price).try_div(decimals)?
    };
    #[cfg(feature = "test-sbf")]
    pub static USDC_PYTH_PRICE: &Pubkey =
        &Pubkey::from_str_const("992moaMQKs32GKZ9dxi8keyM2bUmbrwBZpK4p2K6X5Vs");
    //bellow hack is used for test liquidation, as when cannot change
    #[cfg(feature = "test-sbf")]
    if pyth_price_info.key == USDC_PYTH_PRICE && slots_elapsed >= crate::state::SLOTS_PER_YEAR * 2 {
        debug_msg!(
            "if time elapsed more than 2 years, drop the USDC price by 1/1000 to test the liquidation"
        );
        return Ok(market_price.try_div(4000000)?);
    }
    Ok(market_price)
}

/// Unpacks a spl_token `Mint`.
pub(super) fn unpack_mint(data: &[u8]) -> Result<Mint, LendingError> {
    Mint::unpack(data).map_err(|_| LendingError::InvalidTokenMint)
}

#[inline(always)]
pub(super) fn spl_token_init_mint(params: TokenInitializeMintParams<'_, '_>) -> ProgramResult {
    let TokenInitializeMintParams {
        mint,
        authority,
        token_program,
        decimals,
    } = params;
    let ix = spl_token::instruction::initialize_mint2(
        token_program.key,
        mint.key,
        authority,
        None,
        decimals,
    )?;
    let result = invoke(&ix, &[mint, token_program]);
    result.map_err(|_| LendingError::TokenInitializeMintFailed.into())
}

#[inline(always)]
pub(super) fn spl_token_init_account(params: TokenInitializeAccountParams<'_>) -> ProgramResult {
    let TokenInitializeAccountParams {
        account,
        mint,
        owner,
        token_program,
    } = params;
    let ix = spl_token::instruction::initialize_account3(
        token_program.key,
        account.key,
        mint.key,
        owner.key,
    )?;
    invoke(&ix, &[account, mint, owner, token_program])
        .map_err(|_| LendingError::TokenInitializeAccountFailed.into())
}

/// Issue a spl_token `MintTo` instruction.
pub(super) fn spl_token_mint_to(params: TokenMintToParams<'_, '_>) -> ProgramResult {
    let TokenMintToParams {
        mint,
        destination,
        authority,
        token_program,
        amount,
        authority_signer_seeds,
    } = params;
    let result = invoke_optionally_signed(
        &spl_token::instruction::mint_to(
            token_program.key,
            mint.key,
            destination.key,
            authority.key,
            &[],
            amount,
        )?,
        &[mint, destination, authority, token_program],
        authority_signer_seeds,
    );
    result.map_err(|_| LendingError::TokenMintToFailed.into())
}

/// Issue a spl_token `Burn` instruction.
#[inline(always)]
pub(super) fn spl_token_burn(params: TokenBurnParams<'_, '_>) -> ProgramResult {
    let TokenBurnParams {
        mint,
        source,
        authority,
        token_program,
        amount,
        authority_signer_seeds,
    } = params;
    let result = invoke_optionally_signed(
        &spl_token::instruction::burn(
            token_program.key,
            source.key,
            mint.key,
            authority.key,
            &[],
            amount,
        )?,
        &[source, mint, authority, token_program],
        authority_signer_seeds,
    );
    result.map_err(|_| LendingError::TokenBurnFailed.into())
}

pub(super) struct TokenTransferParams<'a: 'b, 'b> {
    pub source: AccountInfo<'a>,
    pub destination: AccountInfo<'a>,
    pub amount: u64,
    pub authority: AccountInfo<'a>,
    pub authority_signer_seeds: &'b [&'b [u8]],
    pub token_program: AccountInfo<'a>,
}

pub(super) struct TokenInitializeAccountParams<'a> {
    pub account: AccountInfo<'a>,
    pub mint: AccountInfo<'a>,
    pub owner: AccountInfo<'a>,
    pub token_program: AccountInfo<'a>,
}

pub(super) struct TokenInitializeMintParams<'a: 'b, 'b> {
    pub mint: AccountInfo<'a>,
    pub authority: &'b Pubkey,
    pub decimals: u8,
    pub token_program: AccountInfo<'a>,
}

pub(super) struct TokenMintToParams<'a: 'b, 'b> {
    pub mint: AccountInfo<'a>,
    pub destination: AccountInfo<'a>,
    pub amount: u64,
    pub authority: AccountInfo<'a>,
    pub authority_signer_seeds: &'b [&'b [u8]],
    pub token_program: AccountInfo<'a>,
}

pub(super) struct TokenBurnParams<'a: 'b, 'b> {
    pub mint: AccountInfo<'a>,
    pub source: AccountInfo<'a>,
    pub amount: u64,
    pub authority: AccountInfo<'a>,
    pub authority_signer_seeds: &'b [&'b [u8]],
    pub token_program: AccountInfo<'a>,
}
