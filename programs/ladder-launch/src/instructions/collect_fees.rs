//! collect_fees: the seat holder collects the position's accrued fees, both tokens.
//!
//! Accounts:
//!  0 user                          signer
//!  1 launch                        writable
//!  2 seat
//!  3 nft_mint
//!  4 user_nft_account              must hold 1
//!  5 whirlpool                     writable
//!  6 position_bundle_token_account
//!  7 bundled_position              writable
//!  8 reserve_vault                 writable
//!  9 quote_vault                   writable
//! 10 user_token_account            writable
//! 11 user_quote_account            writable
//! 12 token_vault_a                 writable
//! 13 token_vault_b                 writable
//! 14 tick_array_lower
//! 15 tick_array_upper
//! 16 token_mint
//! 17 quote_mint
//! 18 token_program                 SPL Token
//! 19 token_2022_program            (used only when the quote is Token-2022)
//! 20 memo_program
//! 21 whirlpool_program
use super::*;
use crate::{cpi, state::*};
use pinocchio::{account_info::AccountInfo, ProgramResult};

pub fn process(accounts: &[AccountInfo], _args: &[u8]) -> ProgramResult {
    let [user, launch_info, seat_info, nft_mint, user_nft_account, whirlpool, bundle_token_account, bundled_position, reserve_vault, quote_vault, user_token_account, user_quote, token_vault_a, token_vault_b, tick_array_lower, tick_array_upper, token_mint, quote_mint, token_program, token_2022_program, memo_program, whirlpool_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer(user)?;
    let launch = *Launch::load(launch_info)?;
    let seat = Seat::load(seat_info, launch_info.key())?;
    same(token_mint, &launch.token_mint, LaunchError::InvalidMint)?;
    same(quote_mint, &launch.quote_mint, LaunchError::InvalidMint)?;
    let quote_token_program = resolve_quote_program(&launch.quote_token_program, token_program, token_2022_program)?;
    cpi::check_program(token_2022_program, &crate::constants::TOKEN_2022_PROGRAM)?;
    same(whirlpool, &launch.whirlpool, LaunchError::InvalidWhirlpool)?;
    same(reserve_vault, &launch.reserve_vault, LaunchError::InvalidVault)?;
    same(quote_vault, &launch.quote_vault, LaunchError::InvalidVault)?;
    same(nft_mint, &seat.nft_mint, LaunchError::NotSeatHolder)?;
    if token_account_of(user_nft_account, nft_mint.key(), user.key())? != 1 {
        return Err(LaunchError::NotSeatHolder.into());
    }
    let pos = state::whirlpool::read_position(bundled_position)?;
    if pos.whirlpool != launch.whirlpool || pos.position_mint != seat.bundle_mint {
        return Err(LaunchError::InvalidSeat.into());
    }

    let bump_bytes = [launch.bump];
    let ls = cpi::LaunchSigner::new(token_mint.key(), &bump_bytes);
    let s = sides(launch.has(FLAG_TOKEN_IS_A), token_mint, quote_mint, reserve_vault, quote_vault, token_2022_program, quote_token_program);
    let t0 = token_account_of(reserve_vault, &launch.token_mint, launch_info.key())?;
    let q0 = token_account_of(quote_vault, &launch.quote_mint, launch_info.key())?;
    cpi::update_fees_and_rewards(whirlpool, bundled_position, tick_array_lower, tick_array_upper, whirlpool_program)?;
    let liq = s.liquidity_accounts(whirlpool, launch_info, bundled_position, bundle_token_account, token_vault_a, token_vault_b, tick_array_lower, tick_array_upper, memo_program, whirlpool_program);
    cpi::collect_fees_v2(&liq, &[ls.signer()])?;
    let fees_token = cpi::token_amount(reserve_vault)?.saturating_sub(t0);
    let fees_quote = cpi::token_amount(quote_vault)?.saturating_sub(q0);
    token_account_of(user_token_account, &launch.token_mint, user.key())?;
    cpi::transfer_checked(token_2022_program, reserve_vault, token_mint, user_token_account, launch_info, fees_token, cpi::mint_decimals(token_mint)?, &[ls.signer()])?;
    cpi::transfer_checked(quote_token_program, quote_vault, quote_mint, user_quote, launch_info, fees_quote, launch.quote_decimals, &[ls.signer()])
}

use crate::state;
