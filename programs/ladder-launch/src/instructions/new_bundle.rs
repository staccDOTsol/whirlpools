//! new_bundle: open a fresh Whirlpool position bundle owned by the launch PDA.
//! Needed once at setup and again every 256 seats.
//!
//! Accounts:
//!  0 payer                        signer, writable
//!  1 launch                       writable
//!  2 position_bundle              writable, PDA ["position_bundle", bundle_mint] (whirlpool program)
//!  3 position_bundle_mint         signer, writable (fresh keypair)
//!  4 position_bundle_token_account writable, ATA(launch, bundle_mint)
//!  5 token_program
//!  6 system_program
//!  7 rent sysvar
//!  8 ata_program
//!  9 whirlpool_program
use super::*;
use crate::{cpi, state::*};
use pinocchio::{account_info::AccountInfo, ProgramResult};

pub fn process(accounts: &[AccountInfo], _args: &[u8]) -> ProgramResult {
    let [payer, launch_info, position_bundle, bundle_mint, bundle_token_account, token_program, system_program, rent, ata_program, whirlpool_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer(payer)?;
    signer(bundle_mint)?;
    let launch = *Launch::load(launch_info)?;
    if !launch.has(FLAG_POOL_INITIALIZED) {
        return Err(LaunchError::PoolNotInitialized.into());
    }
    // Only when there is no bundle yet or the current one is full.
    if launch.has(FLAG_BUNDLE_READY) && launch.next_index() < crate::constants::BUNDLE_CAPACITY {
        return Err(LaunchError::InvalidBundle.into());
    }
    cpi::initialize_position_bundle(&cpi::InitBundleAccounts {
        position_bundle,
        position_bundle_mint: bundle_mint,
        position_bundle_token_account: bundle_token_account,
        position_bundle_owner: launch_info,
        funder: payer,
        token_program,
        system_program,
        rent,
        ata_program,
        whirlpool_program,
    })?;
    if state::whirlpool::read_bundle_mint(position_bundle)? != *bundle_mint.key() {
        return Err(LaunchError::InvalidBundle.into());
    }
    let mut l = Launch::load(launch_info)?;
    l.bundle_mint = *bundle_mint.key();
    l.set_next_index(0);
    { let v = l.bundle_count().saturating_add(1); l.set_bundle_count(v); }
    l.flags |= FLAG_BUNDLE_READY;
    Ok(())
}

use crate::state;
