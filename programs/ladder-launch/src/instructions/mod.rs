pub mod collect_fees;
pub mod collect_floor_fees;
pub mod create_launch;
pub mod deposit;
pub mod exit;
pub mod init_pool;
pub mod new_bundle;
pub mod seed_floor;

use crate::error::LaunchError;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey};

#[inline(always)]
pub fn signer(info: &AccountInfo) -> Result<(), ProgramError> {
    if !info.is_signer() {
        return Err(LaunchError::MissingSigner.into());
    }
    Ok(())
}

#[inline(always)]
pub fn same(info: &AccountInfo, key: &Pubkey, err: LaunchError) -> Result<(), ProgramError> {
    if info.key() != key {
        return Err(err.into());
    }
    Ok(())
}

/// Verify `ata` is a token account (either program) for `mint` owned by `owner`; returns its balance.
pub fn token_account_of(ata: &AccountInfo, mint: &Pubkey, owner: &Pubkey) -> Result<u64, ProgramError> {
    let acc = crate::cpi::read_token_account(ata)?;
    if &acc.mint != mint || &acc.owner != owner {
        return Err(LaunchError::InvalidTokenAccount.into());
    }
    Ok(acc.amount)
}

/// Pick the account to use as the quote's token program: the SPL Token slot when the
/// quote is SPL, the Token-2022 slot otherwise. Instructions therefore never need the
/// same program account passed twice.
pub fn resolve_quote_program<'a>(quote_token_program: &Pubkey, token_program: &'a AccountInfo, token_2022_program: &'a AccountInfo) -> Result<&'a AccountInfo, ProgramError> {
    crate::cpi::check_program(token_program, &crate::constants::TOKEN_PROGRAM)?;
    if quote_token_program == &crate::constants::TOKEN_PROGRAM {
        Ok(token_program)
    } else {
        crate::cpi::check_program(token_2022_program, &crate::constants::TOKEN_2022_PROGRAM)?;
        Ok(token_2022_program)
    }
}

/// Whirlpool side assignment: the launch token is A when its mint sorts below the quote mint.
pub struct Sides<'a> {
    pub token_is_a: bool,
    pub mint_a: &'a AccountInfo,
    pub mint_b: &'a AccountInfo,
    pub owner_a: &'a AccountInfo,
    pub owner_b: &'a AccountInfo,
    pub prog_a: &'a AccountInfo,
    pub prog_b: &'a AccountInfo,
}

#[allow(clippy::too_many_arguments)]
pub fn sides<'a>(
    token_is_a: bool,
    token_mint: &'a AccountInfo,
    quote_mint: &'a AccountInfo,
    reserve_vault: &'a AccountInfo,
    quote_vault: &'a AccountInfo,
    token_program: &'a AccountInfo,
    quote_program: &'a AccountInfo,
) -> Sides<'a> {
    if token_is_a {
        Sides { token_is_a, mint_a: token_mint, mint_b: quote_mint, owner_a: reserve_vault, owner_b: quote_vault, prog_a: token_program, prog_b: quote_program }
    } else {
        Sides { token_is_a, mint_a: quote_mint, mint_b: token_mint, owner_a: quote_vault, owner_b: reserve_vault, prog_a: quote_program, prog_b: token_program }
    }
}

impl<'a> Sides<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn liquidity_accounts(
        &self,
        whirlpool: &'a AccountInfo,
        position_authority: &'a AccountInfo,
        position: &'a AccountInfo,
        position_token_account: &'a AccountInfo,
        token_vault_a: &'a AccountInfo,
        token_vault_b: &'a AccountInfo,
        tick_array_lower: &'a AccountInfo,
        tick_array_upper: &'a AccountInfo,
        memo_program: &'a AccountInfo,
        whirlpool_program: &'a AccountInfo,
    ) -> crate::cpi::LiquidityAccounts<'a> {
        crate::cpi::LiquidityAccounts {
            whirlpool,
            position_authority,
            position,
            position_token_account,
            token_mint_a: self.mint_a,
            token_mint_b: self.mint_b,
            token_owner_account_a: self.owner_a,
            token_owner_account_b: self.owner_b,
            token_vault_a,
            token_vault_b,
            tick_array_lower,
            tick_array_upper,
            token_program_a: self.prog_a,
            token_program_b: self.prog_b,
            memo_program,
            whirlpool_program,
        }
    }
}

pub fn read_u64(args: &[u8], at: usize) -> Result<u64, ProgramError> {
    Ok(u64::from_le_bytes(args.get(at..at + 8).ok_or(LaunchError::InvalidArgs)?.try_into().unwrap()))
}
pub fn read_u32(args: &[u8], at: usize) -> Result<u32, ProgramError> {
    Ok(u32::from_le_bytes(args.get(at..at + 4).ok_or(LaunchError::InvalidArgs)?.try_into().unwrap()))
}
pub fn read_i32(args: &[u8], at: usize) -> Result<i32, ProgramError> {
    Ok(i32::from_le_bytes(args.get(at..at + 4).ok_or(LaunchError::InvalidArgs)?.try_into().unwrap()))
}
pub fn read_u16(args: &[u8], at: usize) -> Result<u16, ProgramError> {
    Ok(u16::from_le_bytes(args.get(at..at + 2).ok_or(LaunchError::InvalidArgs)?.try_into().unwrap()))
}
