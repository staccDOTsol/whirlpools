//! exit: burn the seat NFT, pull the position, apply seed-clawback, enforce
//! the minimum age and the per-minute exit cap.
//!
//! Accounts:
//!  0 user                          signer, writable (receives rent)
//!  1 launch                        writable
//!  2 seat                          writable
//!  3 nft_mint                      writable
//!  4 user_nft_account              writable
//!  5 whirlpool                     writable
//!  6 position_bundle               writable
//!  7 position_bundle_token_account
//!  8 bundled_position              writable
//!  9 reserve_vault                 writable
//! 10 quote_vault                   writable
//! 11 user_token_account            writable, user's ATA for the launch token
//! 12 user_quote_account            writable, user's quote token account
//! 13 token_vault_a                 writable
//! 14 token_vault_b                 writable
//! 15 tick_array_lower              writable
//! 16 tick_array_upper              writable
//! 17 token_mint
//! 18 quote_mint
//! 19 token_program                 SPL Token
//! 20 token_2022_program            Token-2022 (launch token, and the quote when it is Token-2022)
//! 21 memo_program
//! 22 whirlpool_program
use super::*;
use crate::{cpi, state::*};
use pinocchio::{account_info::AccountInfo, sysvars::{clock::Clock, Sysvar}, ProgramResult};

pub fn process(accounts: &[AccountInfo], _args: &[u8]) -> ProgramResult {
    let [user, launch_info, seat_info, nft_mint, user_nft_account, whirlpool, position_bundle, bundle_token_account, bundled_position, reserve_vault, quote_vault, user_token_account, user_quote, token_vault_a, token_vault_b, tick_array_lower, tick_array_upper, token_mint, quote_mint, token_program, token_2022_program, memo_program, whirlpool_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer(user)?;
    let mut launch = *Launch::load(launch_info)?; // copy: window/cap updates are written back at the end
    let seat = Seat::load(seat_info, launch_info.key())?;
    same(token_mint, &launch.token_mint, LaunchError::InvalidMint)?;
    same(quote_mint, &launch.quote_mint, LaunchError::InvalidMint)?;
    let quote_token_program = resolve_quote_program(&launch.quote_token_program, token_program, token_2022_program)?;
    cpi::check_program(token_2022_program, &crate::constants::TOKEN_2022_PROGRAM)?;
    same(whirlpool, &launch.whirlpool, LaunchError::InvalidWhirlpool)?;
    same(reserve_vault, &launch.reserve_vault, LaunchError::InvalidVault)?;
    same(quote_vault, &launch.quote_vault, LaunchError::InvalidVault)?;
    same(nft_mint, &seat.nft_mint, LaunchError::NotSeatHolder)?;
    if state::whirlpool::read_bundle_mint(position_bundle)? != seat.bundle_mint {
        return Err(LaunchError::InvalidBundle.into());
    }
    let pos = state::whirlpool::read_position(bundled_position)?;
    if pos.whirlpool != launch.whirlpool || pos.position_mint != seat.bundle_mint || pos.liquidity != seat.liquidity() {
        return Err(LaunchError::InvalidSeat.into());
    }
    // holder proof: exactly one NFT, burned here
    if token_account_of(user_nft_account, nft_mint.key(), user.key())? != 1 {
        return Err(LaunchError::NotSeatHolder.into());
    }
    cpi::burn(user_nft_account, nft_mint, user, 1)?;

    // ---- timing rules
    let now = Clock::get()?.unix_timestamp;
    if now.saturating_sub(seat.entry_ts()) < launch.min_age_s() as i64 {
        return Err(LaunchError::SeatTooYoung.into());
    }
    let window = now.div_euclid(60);
    if launch.window_start() != window {
        launch.set_window_start(window);
        launch.set_window_liquidity(0);
    }
    let liquidity = seat.liquidity();
    let budget = launch.total_liquidity() / 10_000 * launch.exit_cap_bps() as u128;
    let used = launch.window_liquidity();
    if used > 0 && used.saturating_add(liquidity) > budget {
        return Err(LaunchError::ExitCapReached.into());
    }
    launch.set_window_liquidity(used.saturating_add(liquidity));

    // ---- pull principal, then fees, measuring each by vault deltas
    let bump_bytes = [launch.bump];
    let ls = cpi::LaunchSigner::new(token_mint.key(), &bump_bytes);
    let s = sides(launch.has(FLAG_TOKEN_IS_A), token_mint, quote_mint, reserve_vault, quote_vault, token_2022_program, quote_token_program);
    let t0 = token_account_of(reserve_vault, &launch.token_mint, launch_info.key())?;
    let q0 = token_account_of(quote_vault, &launch.quote_mint, launch_info.key())?;
    let liq = s.liquidity_accounts(whirlpool, launch_info, bundled_position, bundle_token_account, token_vault_a, token_vault_b, tick_array_lower, tick_array_upper, memo_program, whirlpool_program);
    if liquidity > 0 {
        cpi::decrease_liquidity_v2(&liq, liquidity, 0, 0, &[ls.signer()])?;
    }
    let t1 = cpi::token_amount(reserve_vault)?;
    let q1 = cpi::token_amount(quote_vault)?;
    cpi::collect_fees_v2(&liq, &[ls.signer()])?;
    let t2 = cpi::token_amount(reserve_vault)?;
    let q2 = cpi::token_amount(quote_vault)?;
    let principal_token = t1.saturating_sub(t0);
    let fees_token = t2.saturating_sub(t1);
    let principal_quote = q1.saturating_sub(q0);
    let fees_quote = q2.saturating_sub(q1);

    // seed clawback: seeded tokens stay in the reserve; the rest and all quote go to the holder
    let user_token = principal_token.saturating_sub(seat.seeded_tokens()).saturating_add(fees_token);
    let user_quote_out = principal_quote.saturating_add(fees_quote);
    token_account_of(user_token_account, &launch.token_mint, user.key())?;
    cpi::transfer_checked(token_2022_program, reserve_vault, token_mint, user_token_account, launch_info, user_token, cpi::mint_decimals(token_mint)?, &[ls.signer()])?;
    cpi::transfer_checked(quote_token_program, quote_vault, quote_mint, user_quote, launch_info, user_quote_out, launch.quote_decimals, &[ls.signer()])?;

    cpi::close_bundled_position(bundled_position, position_bundle, bundle_token_account, launch_info, user, whirlpool_program, seat.bundle_index(), &[ls.signer()])?;

    {
        let mut l = Launch::load(launch_info)?;
        l.set_window_start(launch.window_start());
        l.set_window_liquidity(launch.window_liquidity());
        l.set_total_liquidity(launch.total_liquidity().saturating_sub(liquidity));
        l.set_seats_open(launch.seats_open().saturating_sub(1));
    }
    drop(seat);
    cpi::close_program_account(seat_info, user)
}

use crate::state;
