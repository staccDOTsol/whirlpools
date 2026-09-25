//! seed_floor: open a permanently locked, token-only position from a slice of the
//! reserve on the far side of spot (above when the token is A, below when it is B).
//! It guarantees an ask at every price the launch ever trades at; its fees are split
//! creator/treasury by collect_floor_fees.
//!
//! Accounts:
//!  0 creator                signer, writable
//!  1 launch                 writable
//!  2 whirlpool              writable
//!  3 position               writable, PDA ["position", position_mint] (whirlpool program)
//!  4 position_mint          signer, writable (fresh keypair, Token-2022)
//!  5 position_token_account writable, Token-2022 ATA(launch, position_mint)
//!  6 reserve_vault          writable
//!  7 quote_vault            writable
//!  8 token_vault_a          writable
//!  9 token_vault_b          writable
//! 10 tick_array_lower       writable (dynamic, created if missing)
//! 11 tick_array_upper       writable (dynamic, created if missing)
//! 12 token_mint
//! 13 quote_mint
//! 14 lock_config            writable, PDA ["lock_config", position]
//! 15 metadata_update_auth   Whirlpool's fixed metadata authority account
//! 16 token_program          SPL Token
//! 17 token_2022_program     (positions, and the quote when it is Token-2022)
//! 18 system_program
//! 19 ata_program
//! 20 memo_program
//! 21 whirlpool_program
use super::*;
use crate::{constants::*, cpi, math, state::*};
use pinocchio::{account_info::AccountInfo, ProgramResult};

pub fn process(accounts: &[AccountInfo], _args: &[u8]) -> ProgramResult {
    let [creator, launch_info, whirlpool, position, position_mint, position_token_account, reserve_vault, quote_vault, token_vault_a, token_vault_b, tick_array_lower, tick_array_upper, token_mint, quote_mint, lock_config, metadata_update_auth, token_program, token_2022_program, system_program, ata_program, memo_program, whirlpool_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer(creator)?;
    signer(position_mint)?;
    let launch = *Launch::load(launch_info)?;
    same(creator, &launch.creator, LaunchError::MissingSigner)?;
    if !launch.has(FLAG_POOL_INITIALIZED) {
        return Err(LaunchError::PoolNotInitialized.into());
    }
    if launch.has(FLAG_FLOOR_SEEDED) {
        return Err(LaunchError::FloorAlreadySeeded.into());
    }
    same(token_mint, &launch.token_mint, LaunchError::InvalidMint)?;
    same(quote_mint, &launch.quote_mint, LaunchError::InvalidMint)?;
    let quote_token_program = resolve_quote_program(&launch.quote_token_program, token_program, token_2022_program)?;
    same(whirlpool, &launch.whirlpool, LaunchError::InvalidWhirlpool)?;
    same(reserve_vault, &launch.reserve_vault, LaunchError::InvalidVault)?;
    same(quote_vault, &launch.quote_vault, LaunchError::InvalidVault)?;
    cpi::check_program(token_2022_program, &TOKEN_2022_PROGRAM)?;

    let reserve = token_account_of(reserve_vault, &launch.token_mint, launch_info.key())?;
    let amount = (reserve as u128 * launch.floor_bps() as u128 / 10_000) as u64;
    if amount == 0 {
        return Err(LaunchError::ZeroAmount.into());
    }
    let pool = state::whirlpool::read_pool(whirlpool)?;
    same(token_vault_a, &pool.token_vault_a, LaunchError::InvalidVault)?;
    same(token_vault_b, &pool.token_vault_b, LaunchError::InvalidVault)?;
    let token_is_a = launch.has(FLAG_TOKEN_IS_A);
    // entirely on the token side of spot so the position needs no quote
    let (tick_lower, tick_upper) = if token_is_a {
        ((math::floor_to_spacing(pool.tick_current_index, TICK_SPACING) + TICK_SPACING).min(TOP_TICK - TICK_SPACING), TOP_TICK)
    } else {
        (BOTTOM_TICK, math::floor_to_spacing(pool.tick_current_index, TICK_SPACING).max(BOTTOM_TICK + TICK_SPACING))
    };
    let sqrt_lower = math::sqrt_price_from_tick_index(tick_lower);
    let sqrt_upper = math::sqrt_price_from_tick_index(tick_upper);
    let liquidity = if token_is_a { math::liquidity_from_token_a(amount, sqrt_lower, sqrt_upper)? } else { math::liquidity_from_token_b(amount, sqrt_lower, sqrt_upper)? };
    if liquidity == 0 {
        return Err(LaunchError::ZeroAmount.into());
    }
    let (max_a, max_b) = if token_is_a { (amount, 0) } else { (0, amount) };

    let bump_bytes = [launch.bump];
    let ls = cpi::LaunchSigner::new(token_mint.key(), &bump_bytes);
    let s = sides(token_is_a, token_mint, quote_mint, reserve_vault, quote_vault, token_program, quote_token_program);
    cpi::initialize_dynamic_tick_array(whirlpool, creator, tick_array_lower, system_program, whirlpool_program, math::tick_array_start(tick_lower, TICK_SPACING))?;
    cpi::initialize_dynamic_tick_array(whirlpool, creator, tick_array_upper, system_program, whirlpool_program, math::tick_array_start(tick_upper, TICK_SPACING))?;
    cpi::open_position_with_token_extensions(
        &cpi::OpenPositionTeAccounts {
            funder: creator,
            owner: launch_info,
            position,
            position_mint,
            position_token_account,
            whirlpool,
            token_2022_program,
            system_program,
            ata_program,
            metadata_update_auth,
            whirlpool_program,
        },
        tick_lower,
        tick_upper,
    )?;
    let liq = s.liquidity_accounts(whirlpool, launch_info, position, position_token_account, token_vault_a, token_vault_b, tick_array_lower, tick_array_upper, memo_program, whirlpool_program);
    cpi::increase_liquidity_v2(&liq, liquidity, max_a, max_b, &[ls.signer()])?;
    cpi::lock_position_permanent(
        &cpi::LockAccounts {
            funder: creator,
            position_authority: launch_info,
            position,
            position_mint,
            position_token_account,
            lock_config,
            whirlpool,
            token_2022_program,
            system_program,
            whirlpool_program,
        },
        &[ls.signer()],
    )?;
    let seeded = reserve.saturating_sub(cpi::token_amount(reserve_vault)?);
    let mut l = Launch::load(launch_info)?;
    l.floor_position = *position.key();
    { let v = l.tokens_dispensed().saturating_add(seeded); l.set_tokens_dispensed(v); }
    l.flags |= FLAG_FLOOR_SEEDED;
    Ok(())
}

use crate::state;
