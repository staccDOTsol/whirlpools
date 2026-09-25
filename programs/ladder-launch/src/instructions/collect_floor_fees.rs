//! collect_floor_fees: anyone may crank. Collects the locked floor position's fees
//! and splits both tokens between the creator and the treasury by creator_fee_bps.
//!
//! Accounts:
//!  0 cranker                signer
//!  1 launch                 writable
//!  2 whirlpool              writable
//!  3 position               writable (launch.floor_position)
//!  4 position_token_account Token-2022 ATA(launch, position_mint)
//!  5 reserve_vault          writable
//!  6 quote_vault            writable
//!  7 creator_token_account  writable
//!  8 creator_quote_account  writable
//!  9 treasury_token_account writable
//! 10 treasury_quote_account writable
//! 11 token_vault_a          writable
//! 12 token_vault_b          writable
//! 13 tick_array_lower
//! 14 tick_array_upper
//! 15 token_mint
//! 16 quote_mint
//! 17 token_program          SPL Token
//! 18 token_2022_program     (used only when the quote is Token-2022)
//! 19 memo_program
//! 20 whirlpool_program
use super::*;
use crate::{cpi, state::*};
use pinocchio::{account_info::AccountInfo, ProgramResult};

pub fn process(accounts: &[AccountInfo], _args: &[u8]) -> ProgramResult {
    let [cranker, launch_info, whirlpool, position, position_token_account, reserve_vault, quote_vault, creator_token, creator_quote, treasury_token, treasury_quote, token_vault_a, token_vault_b, tick_array_lower, tick_array_upper, token_mint, quote_mint, token_program, token_2022_program, memo_program, whirlpool_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer(cranker)?;
    let launch = *Launch::load(launch_info)?;
    if !launch.has(FLAG_FLOOR_SEEDED) {
        return Err(LaunchError::NotInitialized.into());
    }
    same(position, &launch.floor_position, LaunchError::InvalidSeat)?;
    same(token_mint, &launch.token_mint, LaunchError::InvalidMint)?;
    same(quote_mint, &launch.quote_mint, LaunchError::InvalidMint)?;
    let quote_token_program = resolve_quote_program(&launch.quote_token_program, token_program, token_2022_program)?;
    same(whirlpool, &launch.whirlpool, LaunchError::InvalidWhirlpool)?;
    same(reserve_vault, &launch.reserve_vault, LaunchError::InvalidVault)?;
    same(quote_vault, &launch.quote_vault, LaunchError::InvalidVault)?;
    token_account_of(creator_token, &launch.token_mint, &launch.creator)?;
    token_account_of(creator_quote, &launch.quote_mint, &launch.creator)?;
    token_account_of(treasury_token, &launch.token_mint, &launch.treasury)?;
    token_account_of(treasury_quote, &launch.quote_mint, &launch.treasury)?;

    let bump_bytes = [launch.bump];
    let ls = cpi::LaunchSigner::new(token_mint.key(), &bump_bytes);
    let s = sides(launch.has(FLAG_TOKEN_IS_A), token_mint, quote_mint, reserve_vault, quote_vault, token_program, quote_token_program);
    let t0 = token_account_of(reserve_vault, &launch.token_mint, launch_info.key())?;
    let q0 = token_account_of(quote_vault, &launch.quote_mint, launch_info.key())?;
    cpi::update_fees_and_rewards(whirlpool, position, tick_array_lower, tick_array_upper, whirlpool_program)?;
    let liq = s.liquidity_accounts(whirlpool, launch_info, position, position_token_account, token_vault_a, token_vault_b, tick_array_lower, tick_array_upper, memo_program, whirlpool_program);
    cpi::collect_fees_v2(&liq, &[ls.signer()])?;
    let fees_token = cpi::token_amount(reserve_vault)?.saturating_sub(t0);
    let fees_quote = cpi::token_amount(quote_vault)?.saturating_sub(q0);
    let bps = launch.creator_fee_bps() as u128;
    let creator_t = (fees_token as u128 * bps / 10_000) as u64;
    let creator_q = (fees_quote as u128 * bps / 10_000) as u64;
    let dec = launch.quote_decimals;
    cpi::token_transfer(reserve_vault, creator_token, launch_info, creator_t, &[ls.signer()])?;
    cpi::token_transfer(reserve_vault, treasury_token, launch_info, fees_token - creator_t, &[ls.signer()])?;
    cpi::transfer_checked(quote_token_program, quote_vault, quote_mint, creator_quote, launch_info, creator_q, dec, &[ls.signer()])?;
    cpi::transfer_checked(quote_token_program, quote_vault, quote_mint, treasury_quote, launch_info, fees_quote - creator_q, dec, &[ls.signer()])
}
