//! Account state. Every multi-byte field is a little-endian byte array so the structs
//! are alignment-free and can be viewed directly over account data.
use crate::{constants::*, error::LaunchError};
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey};

pub const LAUNCH_DISC: u8 = 1;
pub const SEAT_DISC: u8 = 2;

pub const FLAG_POOL_INITIALIZED: u8 = 1 << 0;
pub const FLAG_FLOOR_SEEDED: u8 = 1 << 1;
pub const FLAG_BUNDLE_READY: u8 = 1 << 2;
/// The launch token is Whirlpool token A (its pubkey sorts below the quote mint's).
pub const FLAG_TOKEN_IS_A: u8 = 1 << 3;

macro_rules! le_field {
    ($get:ident, $set:ident, $field:ident, $t:ty) => {
        #[inline(always)]
        pub fn $get(&self) -> $t {
            <$t>::from_le_bytes(self.$field)
        }
        #[inline(always)]
        pub fn $set(&mut self, v: $t) {
            self.$field = v.to_le_bytes();
        }
    };
}

/// One per launched token. PDA: ["launch", token_mint].
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Launch {
    pub disc: u8,
    pub bump: u8,
    pub token_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub whirlpool: Pubkey,
    pub reserve_vault: Pubkey,
    pub quote_vault: Pubkey,
    pub creator: Pubkey,
    pub treasury: Pubkey,
    pub floor_position: Pubkey,
    pub bundle_mint: Pubkey,
    pub quote_token_program: Pubkey,
    next_index: [u8; 2],
    bundle_count: [u8; 2],
    min_age_s: [u8; 4],
    exit_cap_bps: [u8; 2],
    lower_delta_ticks: [u8; 4],
    floor_bps: [u8; 2],
    creator_fee_bps: [u8; 2],
    window_start: [u8; 8],
    window_liquidity: [u8; 16],
    total_liquidity: [u8; 16],
    tokens_dispensed: [u8; 8],
    quote_in: [u8; 8],
    seats_open: [u8; 4],
    pub flags: u8,
    pub quote_decimals: u8,
    _pad: [u8; 6],
}

impl Launch {
    pub const LEN: usize = core::mem::size_of::<Launch>();

    le_field!(next_index, set_next_index, next_index, u16);
    le_field!(bundle_count, set_bundle_count, bundle_count, u16);
    le_field!(min_age_s, set_min_age_s, min_age_s, u32);
    le_field!(exit_cap_bps, set_exit_cap_bps, exit_cap_bps, u16);
    le_field!(lower_delta_ticks, set_lower_delta_ticks, lower_delta_ticks, i32);
    le_field!(floor_bps, set_floor_bps, floor_bps, u16);
    le_field!(creator_fee_bps, set_creator_fee_bps, creator_fee_bps, u16);
    le_field!(window_start, set_window_start, window_start, i64);
    le_field!(window_liquidity, set_window_liquidity, window_liquidity, u128);
    le_field!(total_liquidity, set_total_liquidity, total_liquidity, u128);
    le_field!(tokens_dispensed, set_tokens_dispensed, tokens_dispensed, u64);
    le_field!(quote_in, set_quote_in, quote_in, u64);
    le_field!(seats_open, set_seats_open, seats_open, u32);

    pub fn seeds<'a>(token_mint: &'a Pubkey) -> [&'a [u8]; 2] {
        [LAUNCH_SEED, token_mint.as_ref()]
    }

    /// Borrow an initialized Launch owned by this program, verifying the PDA.
    pub fn load<'a>(info: &'a AccountInfo) -> Result<pinocchio::account_info::RefMut<'a, Launch>, ProgramError> {
        if info.owner() != &crate::ID {
            return Err(LaunchError::InvalidAccountOwner.into());
        }
        if info.data_len() < Self::LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        let data = info.try_borrow_mut_data()?;
        let launch = pinocchio::account_info::RefMut::map(data, |d| unsafe { &mut *(d.as_mut_ptr() as *mut Launch) });
        if launch.disc != LAUNCH_DISC {
            return Err(LaunchError::NotInitialized.into());
        }
        let expected = pinocchio::pubkey::create_program_address(
            &[LAUNCH_SEED, launch.token_mint.as_ref(), &[launch.bump]],
            &crate::ID,
        )?;
        if &expected != info.key() {
            return Err(LaunchError::InvalidPda.into());
        }
        Ok(launch)
    }

    #[inline(always)]
    pub fn has(&self, flag: u8) -> bool {
        self.flags & flag != 0
    }
}

/// One per deposit. PDA: ["seat", launch, bundle_mint, bundle_index_le].
#[repr(C)]
pub struct Seat {
    pub disc: u8,
    pub bump: u8,
    pub launch: Pubkey,
    pub bundle_mint: Pubkey,
    bundle_index: [u8; 2],
    pub nft_mint: Pubkey,
    entry_ts: [u8; 8],
    seeded_tokens: [u8; 8],
    quote_in: [u8; 8],
    liquidity: [u8; 16],
    tick_lower: [u8; 4],
    tick_upper: [u8; 4],
}

impl Seat {
    pub const LEN: usize = core::mem::size_of::<Seat>();

    le_field!(bundle_index, set_bundle_index, bundle_index, u16);
    le_field!(entry_ts, set_entry_ts, entry_ts, i64);
    le_field!(seeded_tokens, set_seeded_tokens, seeded_tokens, u64);
    le_field!(quote_in, set_quote_in, quote_in, u64);
    le_field!(liquidity, set_liquidity, liquidity, u128);
    le_field!(tick_lower, set_tick_lower, tick_lower, i32);
    le_field!(tick_upper, set_tick_upper, tick_upper, i32);

    pub fn load<'a>(info: &'a AccountInfo, launch: &Pubkey) -> Result<pinocchio::account_info::RefMut<'a, Seat>, ProgramError> {
        if info.owner() != &crate::ID {
            return Err(LaunchError::InvalidAccountOwner.into());
        }
        if info.data_len() < Self::LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        let data = info.try_borrow_mut_data()?;
        let seat = pinocchio::account_info::RefMut::map(data, |d| unsafe { &mut *(d.as_mut_ptr() as *mut Seat) });
        if seat.disc != SEAT_DISC || &seat.launch != launch {
            return Err(LaunchError::InvalidSeat.into());
        }
        Ok(seat)
    }
}

/// Zero-copy reads of Whirlpool program accounts (Anchor/borsh field order).
pub mod whirlpool {
    use super::*;

    pub const WHIRLPOOL_LEN: usize = 653;
    pub const POSITION_LEN: usize = 216;
    pub const POSITION_BUNDLE_LEN: usize = 136;

    fn check(info: &AccountInfo, len: usize) -> Result<(), ProgramError> {
        if info.owner() != &WHIRLPOOL_PROGRAM {
            return Err(LaunchError::InvalidAccountOwner.into());
        }
        if info.data_len() < len {
            return Err(ProgramError::InvalidAccountData);
        }
        Ok(())
    }
    fn u128_at(d: &[u8], o: usize) -> u128 {
        u128::from_le_bytes(d[o..o + 16].try_into().unwrap())
    }
    fn u64_at(d: &[u8], o: usize) -> u64 {
        u64::from_le_bytes(d[o..o + 8].try_into().unwrap())
    }
    fn i32_at(d: &[u8], o: usize) -> i32 {
        i32::from_le_bytes(d[o..o + 4].try_into().unwrap())
    }
    fn key_at(d: &[u8], o: usize) -> Pubkey {
        d[o..o + 32].try_into().unwrap()
    }

    pub struct PoolView {
        pub tick_spacing: u16,
        pub liquidity: u128,
        pub sqrt_price: u128,
        pub tick_current_index: i32,
        pub token_mint_a: Pubkey,
        pub token_vault_a: Pubkey,
        pub token_mint_b: Pubkey,
        pub token_vault_b: Pubkey,
    }

    pub fn read_pool(info: &AccountInfo) -> Result<PoolView, ProgramError> {
        check(info, WHIRLPOOL_LEN)?;
        let d = info.try_borrow_data()?;
        Ok(PoolView {
            tick_spacing: u16::from_le_bytes([d[41], d[42]]),
            liquidity: u128_at(&d, 49),
            sqrt_price: u128_at(&d, 65),
            tick_current_index: i32_at(&d, 81),
            token_mint_a: key_at(&d, 101),
            token_vault_a: key_at(&d, 133),
            token_mint_b: key_at(&d, 181),
            token_vault_b: key_at(&d, 213),
        })
    }

    pub struct PositionView {
        pub whirlpool: Pubkey,
        pub position_mint: Pubkey,
        pub liquidity: u128,
        pub tick_lower_index: i32,
        pub tick_upper_index: i32,
        pub fee_owed_a: u64,
        pub fee_owed_b: u64,
    }

    pub fn read_position(info: &AccountInfo) -> Result<PositionView, ProgramError> {
        check(info, POSITION_LEN)?;
        let d = info.try_borrow_data()?;
        Ok(PositionView {
            whirlpool: key_at(&d, 8),
            position_mint: key_at(&d, 40),
            liquidity: u128_at(&d, 72),
            tick_lower_index: i32_at(&d, 88),
            tick_upper_index: i32_at(&d, 92),
            fee_owed_a: u64_at(&d, 112),
            fee_owed_b: u64_at(&d, 136),
        })
    }

    pub fn read_bundle_mint(info: &AccountInfo) -> Result<Pubkey, ProgramError> {
        check(info, POSITION_BUNDLE_LEN)?;
        let d = info.try_borrow_data()?;
        Ok(key_at(&d, 8))
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    #[test]
    fn launch_layout() {
        assert_eq!(Launch::LEN, 408, "Launch::LEN = {}", Launch::LEN);
        assert_eq!(core::mem::offset_of!(Launch, flags), 400);
        assert_eq!(Seat::LEN, 148);
    }
}
