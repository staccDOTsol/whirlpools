//! deposit: pair the user's quote with reserve tokens into a 90/10 bundled position.
//!
//! Accounts:
//!  0 user                          signer, writable
//!  1 launch                        writable
//!  2 token_mint                    launch.token_mint
//!  3 whirlpool                     writable
//!  4 reserve_vault                 writable
//!  5 quote_vault                   writable
//!  6 user_quote_account            writable, user's quote token account
//!  7 position_bundle               writable
//!  8 position_bundle_token_account launch's ATA for the bundle mint
//!  9 bundled_position              writable, PDA ["bundled_position", bundle_mint, index.to_string()]
//! 10 seat                          writable, PDA ["seat", launch, bundle_mint, index_le]
//! 11 nft_mint                      signer, writable (fresh keypair)
//! 12 user_nft_account              writable, ATA(user, nft_mint)
//! 13 tick_array_lower              writable (created here if missing, dynamic)
//! 14 tick_array_upper              writable (created here if missing, dynamic)
//! 15 token_vault_a                 writable (Whirlpool vault A)
//! 16 token_vault_b                 writable (Whirlpool vault B)
//! 17 quote_mint                    launch.quote_mint
//! 18 token_program                 SPL Token
//! 19 token_2022_program    Token-2022 program (used only when the quote is Token-2022)
//! 20 memo_program
//! 21 system_program
//! 22 rent sysvar
//! 23 ata_program
//! 24 whirlpool_program
//! Args: quote_amount u64
use super::*;
use crate::{constants::*, cpi, math, state::*};
use pinocchio::{account_info::AccountInfo, instruction::Seed, sysvars::{clock::Clock, Sysvar}, ProgramResult};

pub fn process(accounts: &[AccountInfo], args: &[u8]) -> ProgramResult {
    let [user, launch_info, token_mint, whirlpool, reserve_vault, quote_vault, user_quote, position_bundle, bundle_token_account, bundled_position, seat, nft_mint, user_nft_account, tick_array_lower, tick_array_upper, token_vault_a, token_vault_b, quote_mint, token_program, token_2022_program, memo_program, system_program, rent, ata_program, whirlpool_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer(user)?;
    signer(nft_mint)?;
    let quote_amount = read_u64(args, 0)?;
    if quote_amount == 0 {
        return Err(LaunchError::ZeroAmount.into());
    }

    let launch = *Launch::load(launch_info)?; // copy: no state borrow is held across CPIs
    if !launch.has(FLAG_POOL_INITIALIZED) || !launch.has(FLAG_BUNDLE_READY) {
        return Err(LaunchError::PoolNotInitialized.into());
    }
    same(token_mint, &launch.token_mint, LaunchError::InvalidMint)?;
    same(quote_mint, &launch.quote_mint, LaunchError::InvalidMint)?;
    let quote_token_program = resolve_quote_program(&launch.quote_token_program, token_program, token_2022_program)?;
    cpi::check_program(token_2022_program, &TOKEN_2022_PROGRAM)?;
    same(whirlpool, &launch.whirlpool, LaunchError::InvalidWhirlpool)?;
    same(reserve_vault, &launch.reserve_vault, LaunchError::InvalidVault)?;
    same(quote_vault, &launch.quote_vault, LaunchError::InvalidVault)?;
    if state::whirlpool::read_bundle_mint(position_bundle)? != launch.bundle_mint {
        return Err(LaunchError::InvalidBundle.into());
    }
    let index = launch.next_index();
    if index >= BUNDLE_CAPACITY {
        return Err(LaunchError::BundleFull.into());
    }
    let reserve_before = token_account_of(reserve_vault, &launch.token_mint, launch_info.key())?;
    if reserve_before <= RESERVE_DUST {
        return Err(LaunchError::ReserveEmpty.into()); // liquidity rounding can strand a few base units
    }
    let quote_before = token_account_of(quote_vault, &launch.quote_mint, launch_info.key())?;
    let token_is_a = launch.has(FLAG_TOKEN_IS_A);
    let s = sides(token_is_a, token_mint, quote_mint, reserve_vault, quote_vault, token_2022_program, quote_token_program);

    // ---- range and liquidity from the current pool price. The token side is the open
    // ---- ladder (up to the top when the token is A, down to the bottom when it is B);
    // ---- the quote side covers ~21% on the other side of spot.
    let pool = state::whirlpool::read_pool(whirlpool)?;
    same(token_vault_a, &pool.token_vault_a, LaunchError::InvalidVault)?;
    same(token_vault_b, &pool.token_vault_b, LaunchError::InvalidVault)?;
    let delta = launch.lower_delta_ticks(); // negative
    let (tick_lower, tick_upper) = if token_is_a {
        (math::floor_to_spacing(pool.tick_current_index.saturating_add(delta), TICK_SPACING).max(BOTTOM_TICK), TOP_TICK)
    } else {
        (BOTTOM_TICK, math::floor_to_spacing(pool.tick_current_index.saturating_sub(delta), TICK_SPACING).min(TOP_TICK)) // |delta| > spacing keeps this above spot
    };
    let sqrt_lower = math::sqrt_price_from_tick_index(tick_lower);
    let sqrt_upper = math::sqrt_price_from_tick_index(tick_upper);
    let sqrt_cur = pool.sqrt_price.clamp(sqrt_lower, sqrt_upper);
    let (mut liquidity, mut need_token) = if token_is_a {
        let l = math::liquidity_from_token_b(quote_amount, sqrt_lower, sqrt_cur)?;
        (l, math::amount_delta_a(sqrt_cur, sqrt_upper, l, true)?)
    } else {
        let l = math::liquidity_from_token_a(quote_amount, sqrt_cur, sqrt_upper)?;
        (l, math::amount_delta_b(sqrt_lower, sqrt_cur, l, true)?)
    };
    if need_token > reserve_before {
        // last deposit: take whatever the reserve has left, refund the rest of the quote
        liquidity = math::scale_liquidity(liquidity, need_token, reserve_before)?;
        need_token = if token_is_a { math::amount_delta_a(sqrt_cur, sqrt_upper, liquidity, true)? } else { math::amount_delta_b(sqrt_lower, sqrt_cur, liquidity, true)? }.min(reserve_before);
    }
    if liquidity == 0 {
        return Err(LaunchError::ZeroAmount.into());
    }
    let need_quote = if token_is_a { math::amount_delta_b(sqrt_lower, sqrt_cur, liquidity, true)? } else { math::amount_delta_a(sqrt_cur, sqrt_upper, liquidity, true)? };
    if need_quote > quote_amount {
        return Err(LaunchError::MathOverflow.into());
    }
    let (max_a, max_b) = if token_is_a { (need_token, need_quote) } else { (need_quote, need_token) };

    // ---- quote from the user into the launch's quote vault (only what the position needs)
    cpi::transfer_checked(quote_token_program, user_quote, quote_mint, quote_vault, user, need_quote, launch.quote_decimals, &[])?;
    let quote_funded = cpi::token_amount(quote_vault)?.saturating_sub(quote_before); // net of any transfer fee

    // ---- tick arrays (dynamic, idempotent) and the position
    let bump_bytes = [launch.bump];
    let ls = cpi::LaunchSigner::new(token_mint.key(), &bump_bytes);
    cpi::initialize_dynamic_tick_array(whirlpool, user, tick_array_lower, system_program, whirlpool_program, math::tick_array_start(tick_lower, TICK_SPACING))?;
    cpi::initialize_dynamic_tick_array(whirlpool, user, tick_array_upper, system_program, whirlpool_program, math::tick_array_start(tick_upper, TICK_SPACING))?;
    cpi::open_bundled_position(
        &cpi::OpenBundledAccounts {
            bundled_position,
            position_bundle,
            position_bundle_token_account: bundle_token_account,
            position_bundle_authority: launch_info,
            whirlpool,
            funder: user,
            system_program,
            rent,
            whirlpool_program,
        },
        index,
        tick_lower,
        tick_upper,
        &[ls.signer()],
    )?;
    let liq = s.liquidity_accounts(whirlpool, launch_info, bundled_position, bundle_token_account, token_vault_a, token_vault_b, tick_array_lower, tick_array_upper, memo_program, whirlpool_program);
    cpi::increase_liquidity_v2(&liq, liquidity, max_a.min(if token_is_a { reserve_before } else { quote_funded }), max_b.min(if token_is_a { quote_funded } else { reserve_before }), &[ls.signer()])?;
    let seeded = reserve_before.checked_sub(cpi::token_amount(reserve_vault)?).ok_or(LaunchError::MathOverflow)?;
    let leftover = cpi::token_amount(quote_vault)?.saturating_sub(quote_before);
    let quote_used = quote_funded.saturating_sub(leftover);
    // anything the position did not take goes back
    cpi::transfer_checked(quote_token_program, quote_vault, quote_mint, user_quote, launch_info, leftover, launch.quote_decimals, &[ls.signer()])?;

    // ---- seat NFT: plain SPL mint, supply 1, authority revoked
    cpi::create_mint(user, nft_mint, 0, launch_info.key(), token_program)?;
    cpi::create_ata_idempotent(user, user_nft_account, user, nft_mint, system_program, token_program, ata_program)?;
    cpi::mint_to(nft_mint, user_nft_account, launch_info, 1, &[ls.signer()])?;
    cpi::revoke_mint_authority(nft_mint, launch_info, &[ls.signer()])?;

    // ---- seat record
    let index_le = index.to_le_bytes();
    let (seat_key, seat_bump) = pinocchio::pubkey::find_program_address(
        &[SEAT_SEED, launch_info.key().as_ref(), launch.bundle_mint.as_ref(), &index_le],
        &crate::ID,
    );
    same(seat, &seat_key, LaunchError::InvalidPda)?;
    let seat_bump_bytes = [seat_bump];
    let seat_seeds = [
        Seed::from(SEAT_SEED),
        Seed::from(launch_info.key().as_ref()),
        Seed::from(launch.bundle_mint.as_ref()),
        Seed::from(&index_le[..]),
        Seed::from(&seat_bump_bytes[..]),
    ];
    cpi::create_pda_account(user, seat, Seat::LEN, &crate::ID, &seat_seeds)?;
    {
        let mut data = seat.try_borrow_mut_data()?;
        let st = unsafe { &mut *(data.as_mut_ptr() as *mut Seat) };
        st.disc = SEAT_DISC;
        st.bump = seat_bump;
        st.launch = *launch_info.key();
        st.bundle_mint = launch.bundle_mint;
        st.set_bundle_index(index);
        st.nft_mint = *nft_mint.key();
        st.set_entry_ts(Clock::get()?.unix_timestamp);
        st.set_seeded_tokens(seeded);
        st.set_quote_in(quote_used);
        st.set_liquidity(liquidity);
        st.set_tick_lower(tick_lower);
        st.set_tick_upper(tick_upper);
    }

    let mut launch = Launch::load(launch_info)?;
    launch.set_next_index(index + 1);
    { let v = launch.total_liquidity().checked_add(liquidity).ok_or(LaunchError::MathOverflow)?; launch.set_total_liquidity(v); }
    { let v = launch.tokens_dispensed().checked_add(seeded).ok_or(LaunchError::MathOverflow)?; launch.set_tokens_dispensed(v); }
    { let v = launch.quote_in().checked_add(quote_used).ok_or(LaunchError::MathOverflow)?; launch.set_quote_in(v); }
    { let v = launch.seats_open().checked_add(1).ok_or(LaunchError::MathOverflow)?; launch.set_seats_open(v); }
    Ok(())
}

use crate::state;
