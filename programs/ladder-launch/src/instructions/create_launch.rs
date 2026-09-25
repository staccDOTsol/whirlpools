//! create_launch: create the Token-2022 launch mint with its metadata in the mint
//! (MetadataPointer + TokenMetadata: name, symbol, uri; immutable), mint the fixed supply
//! into the reserve, explicitly revoke the mint and freeze authorities, create the
//! launch's token and quote vaults, write Launch state.
//!
//! The quote may be any SPL Token mint (WSOL included) or a Token-2022 mint without
//! transfer hooks, permanent delegates, close authorities, default-frozen state,
//! confidential transfers, non-transferability or pausing.
//!
//! Accounts:
//!  0 creator             signer, writable (pays rent)
//!  1 launch              writable, PDA ["launch", token_mint]
//!  2 token_mint          signer, writable (fresh keypair)
//!  3 reserve_vault       writable, Token-2022 ATA(launch, token_mint)
//!  4 quote_mint          any accepted mint
//!  5 quote_vault         writable, ATA(launch, quote_mint) under the quote's token program
//!  6 treasury            protocol treasury pubkey (receives the treasury share of floor fees)
//!  7 token_program       SPL Token
//!  8 token_2022_program  Token-2022 program (launch token, and the quote when it is Token-2022)
//!  9 ata_program
//! 10 system_program
//! 11 rent sysvar
//! Args: decimals u8 | supply u64 | min_age_s u32 | exit_cap_bps u16 | lower_delta_ticks i32 | floor_bps u16 | creator_fee_bps u16
//!       | name_len u8 | name | symbol_len u8 | symbol | uri_len u8 | uri   (limits: 32 / 10 / 200 bytes)
use super::*;
use crate::{constants::*, cpi, state::*};
use pinocchio::{account_info::AccountInfo, instruction::Seed, ProgramResult};

pub fn process(accounts: &[AccountInfo], args: &[u8]) -> ProgramResult {
    let [creator, launch, token_mint, reserve_vault, quote_mint, quote_vault, treasury, token_program, token_2022_program, ata_program, system_program, _rent] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer(creator)?;
    signer(token_mint)?;
    cpi::check_program(token_program, &TOKEN_PROGRAM)?;
    cpi::check_program(token_2022_program, &TOKEN_2022_PROGRAM)?;
    cpi::check_program(system_program, &SYSTEM_PROGRAM)?;
    cpi::check_quote_mint(quote_mint)?;
    let quote_token_program = resolve_quote_program(quote_mint.owner(), token_program, token_2022_program)?;
    let quote_decimals = cpi::mint_decimals(quote_mint)?;

    let decimals = *args.first().ok_or(LaunchError::InvalidArgs)?;
    let supply = read_u64(args, 1)?;
    let min_age_s = read_u32(args, 9)?;
    let exit_cap_bps = read_u16(args, 13)?;
    let lower_delta_ticks = read_i32(args, 15)?;
    let floor_bps = read_u16(args, 19)?;
    let creator_fee_bps = read_u16(args, 21)?;
    let (name, rest) = read_bytes(args.get(23..).ok_or(LaunchError::InvalidArgs)?)?;
    let (symbol, rest) = read_bytes(rest)?;
    let (uri, _) = read_bytes(rest)?;
    if supply == 0 || exit_cap_bps == 0 || exit_cap_bps > 10_000 || floor_bps >= 5_000 || creator_fee_bps > 10_000 || lower_delta_ticks >= 0 {
        return Err(LaunchError::InvalidArgs.into());
    }
    if token_mint.key() == quote_mint.key() {
        return Err(LaunchError::InvalidMint.into());
    }
    let token_is_a = token_mint.key() < quote_mint.key();

    // Launch PDA
    let (launch_key, bump) = pinocchio::pubkey::find_program_address(&Launch::seeds(token_mint.key()), &crate::ID);
    same(launch, &launch_key, LaunchError::InvalidPda)?;
    if launch.data_len() != 0 {
        return Err(LaunchError::AlreadyInitialized.into());
    }
    let bump_bytes = [bump];
    let seeds = [Seed::from(LAUNCH_SEED), Seed::from(token_mint.key().as_ref()), Seed::from(&bump_bytes[..])];
    cpi::create_pda_account(creator, launch, Launch::LEN, &crate::ID, &seeds)?;

    // Token-2022 mint with the metadata inside it; launch PDA is the temporary authority, revoked below.
    let ls = cpi::LaunchSigner::new(token_mint.key(), &bump_bytes);
    cpi::create_mint_2022_with_metadata(creator, token_mint, launch, decimals, name, symbol, uri, system_program, token_2022_program, &[ls.signer()])?;
    cpi::create_ata_idempotent(creator, reserve_vault, launch, token_mint, system_program, token_2022_program, ata_program)?;
    cpi::create_ata_idempotent(creator, quote_vault, launch, quote_mint, system_program, quote_token_program, ata_program)?;
    cpi::mint_to_2022(token_mint, reserve_vault, launch, supply, &[ls.signer()])?;
    cpi::revoke_mint_authority_2022(token_mint, launch, &[ls.signer()])?;
    cpi::revoke_freeze_authority_2022(token_mint, launch, &[ls.signer()])?;
    token_account_of(reserve_vault, token_mint.key(), &launch_key)?;
    token_account_of(quote_vault, quote_mint.key(), &launch_key)?;

    // State
    let mut data = launch.try_borrow_mut_data()?;
    let l = unsafe { &mut *(data.as_mut_ptr() as *mut Launch) };
    l.disc = LAUNCH_DISC;
    l.bump = bump;
    l.token_mint = *token_mint.key();
    l.quote_mint = *quote_mint.key();
    l.quote_token_program = *quote_token_program.key();
    l.quote_decimals = quote_decimals;
    l.reserve_vault = *reserve_vault.key();
    l.quote_vault = *quote_vault.key();
    l.creator = *creator.key();
    l.treasury = *treasury.key();
    l.set_min_age_s(min_age_s);
    l.set_exit_cap_bps(exit_cap_bps);
    l.set_lower_delta_ticks(lower_delta_ticks);
    l.set_floor_bps(floor_bps);
    l.set_creator_fee_bps(creator_fee_bps);
    l.set_next_index(0);
    l.set_bundle_count(0);
    l.flags = if token_is_a { FLAG_TOKEN_IS_A } else { 0 };
    Ok(())
}

/// Length-prefixed (u8) byte string; returns it and the remaining args.
fn read_bytes(a: &[u8]) -> Result<(&[u8], &[u8]), ProgramError> {
    let n = *a.first().ok_or(LaunchError::InvalidArgs)? as usize;
    if a.len() < 1 + n {
        return Err(LaunchError::InvalidArgs.into());
    }
    Ok((&a[1..1 + n], &a[1 + n..]))
}
