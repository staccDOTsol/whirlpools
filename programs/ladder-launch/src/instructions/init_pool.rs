//! init_pool: open the Whirlpool for the launch on the adaptive fee tier.
//! The program orders mints into Whirlpool's A/B by pubkey.
//!
//! Accounts:
//!  0 creator              signer, writable (funder and initialize_pool_authority)
//!  1 launch               writable
//!  2 whirlpools_config    must be WHIRLPOOLS_CONFIG
//!  3 token_mint           launch.token_mint
//!  4 quote_mint           launch.quote_mint
//!  5 token_badge          PDA ["token_badge", config, token_mint] (may be uninitialized)
//!  6 quote_badge          PDA ["token_badge", config, quote_mint] (may be uninitialized)
//!  7 whirlpool            writable, PDA ["whirlpool", config, mint_a, mint_b, 1032u16]
//!  8 oracle               writable, PDA ["oracle", whirlpool]
//!  9 token_vault          signer, writable (fresh keypair; becomes vault A or B)
//! 10 quote_vault_kp       signer, writable (fresh keypair; becomes vault B or A)
//! 11 adaptive_fee_tier    must be ADAPTIVE_FEE_TIER
//! 12 token_program        SPL Token
//! 13 token_2022_program    Token-2022 program (used only when the quote is Token-2022)
//! 14 system_program
//! 15 rent sysvar
//! 16 whirlpool_program
//! Args: initial_tick i32 (Whirlpool price = 1.0001^tick, token B per token A in base units)
use super::*;
use crate::{constants::*, cpi, math, state::*};
use pinocchio::{account_info::AccountInfo, ProgramResult};

pub fn process(accounts: &[AccountInfo], args: &[u8]) -> ProgramResult {
    let [creator, launch_info, config, token_mint, quote_mint, token_badge, quote_badge, whirlpool, oracle, token_vault_kp, quote_vault_kp, fee_tier, token_program, token_2022_program, system_program, rent, whirlpool_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer(creator)?;
    let (token_is_a, launch_token_mint, launch_quote_mint) = {
        let launch = Launch::load(launch_info)?;
        same(creator, &launch.creator, LaunchError::MissingSigner)?;
        if launch.has(FLAG_POOL_INITIALIZED) {
            return Err(LaunchError::AlreadyInitialized.into());
        }
        same(token_mint, &launch.token_mint, LaunchError::InvalidMint)?;
        same(quote_mint, &launch.quote_mint, LaunchError::InvalidMint)?;
        (launch.has(FLAG_TOKEN_IS_A), launch.token_mint, launch.quote_mint)
    }; // state borrow released before the CPI
    let quote_token_program = {
        let launch = Launch::load(launch_info)?;
        let p = launch.quote_token_program;
        drop(launch);
        resolve_quote_program(&p, token_program, token_2022_program)?
    };
    cpi::check_program(token_2022_program, &TOKEN_2022_PROGRAM)?;
    same(config, &WHIRLPOOLS_CONFIG, LaunchError::InvalidProgram)?;
    same(fee_tier, &ADAPTIVE_FEE_TIER, LaunchError::InvalidProgram)?;

    let initial_tick = read_i32(args, 0)?;
    if initial_tick <= BOTTOM_TICK || initial_tick >= TOP_TICK {
        return Err(LaunchError::InvalidArgs.into());
    }
    let sqrt_price = math::sqrt_price_from_tick_index(initial_tick);
    let s = sides(token_is_a, token_mint, quote_mint, token_vault_kp, quote_vault_kp, token_2022_program, quote_token_program);
    let (badge_a, badge_b) = if token_is_a { (token_badge, quote_badge) } else { (quote_badge, token_badge) };

    cpi::initialize_pool_with_adaptive_fee(
        &cpi::InitPoolAccounts {
            whirlpools_config: config,
            token_mint_a: s.mint_a,
            token_mint_b: s.mint_b,
            token_badge_a: badge_a,
            token_badge_b: badge_b,
            funder: creator,
            initialize_pool_authority: creator,
            whirlpool,
            oracle,
            token_vault_a: s.owner_a,
            token_vault_b: s.owner_b,
            adaptive_fee_tier: fee_tier,
            token_program_a: s.prog_a,
            token_program_b: s.prog_b,
            system_program,
            rent,
            whirlpool_program,
        },
        sqrt_price,
    )?;

    let pool = state::whirlpool::read_pool(whirlpool)?;
    let ok = if token_is_a { pool.token_mint_a == launch_token_mint && pool.token_mint_b == launch_quote_mint } else { pool.token_mint_b == launch_token_mint && pool.token_mint_a == launch_quote_mint };
    if !ok || pool.tick_spacing as i32 != TICK_SPACING {
        return Err(LaunchError::InvalidWhirlpool.into());
    }
    let mut launch = Launch::load(launch_info)?;
    launch.whirlpool = *whirlpool.key();
    launch.flags |= FLAG_POOL_INITIALIZED;
    Ok(())
}

use crate::state;
