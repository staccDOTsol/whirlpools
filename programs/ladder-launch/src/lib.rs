//! Ladder Launch: a launchpad built on Orca Whirlpools.
//!
//! Buyers do not buy. They deposit SOL, and the program pairs it with tokens from a
//! fixed 1B reserve into a concentrated position that is ~90% token / ~10% SOL by
//! value at the current pool price (lower bound ~21% under spot, open top). The
//! program's launch PDA owns the Whirlpool position (in a position bundle); the
//! depositor receives a plain SPL NFT that is the only key to that seat.
//!
//! Exits follow the seed-clawback rule: the holder takes the SOL side and all fees,
//! seeded tokens return to the reserve, only tokens beyond the seed are theirs.
//! Exits are refused for positions younger than `min_age_s` and are rate-limited to
//! `exit_cap_bps` of the launch's liquidity per minute (first exit in a window always
//! passes, so no seat is ever stuck).
#![no_std]

use pinocchio::{account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey, ProgramResult};

pub mod constants;
pub mod cpi;
pub mod error;
pub mod instructions;
pub mod math;
pub mod state;

pinocchio_pubkey::declare_id!("GJViDKnTV3pZMwgMCqjwj1hC8JozuJR69QntyVGeGrj8");

#[cfg(not(feature = "no-entrypoint"))]
pinocchio::program_entrypoint!(process_instruction, { pinocchio::MAX_TX_ACCOUNTS });
#[cfg(not(feature = "no-entrypoint"))]
pinocchio::default_allocator!();
#[cfg(not(feature = "no-entrypoint"))]
pinocchio::nostd_panic_handler!();

pub fn process_instruction(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    if program_id != &ID {
        return Err(ProgramError::IncorrectProgramId);
    }
    let (tag, args) = data.split_first().ok_or(ProgramError::InvalidInstructionData)?;
    match *tag {
        0 => instructions::create_launch::process(accounts, args),
        1 => instructions::init_pool::process(accounts, args),
        2 => instructions::new_bundle::process(accounts, args),
        3 => instructions::seed_floor::process(accounts, args),
        4 => instructions::deposit::process(accounts, args),
        5 => instructions::exit::process(accounts, args),
        6 => instructions::collect_fees::process(accounts, args),
        7 => instructions::collect_floor_fees::process(accounts, args),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}
