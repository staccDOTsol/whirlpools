use pinocchio::pubkey::Pubkey;
use pinocchio_pubkey::pubkey;

pub const WHIRLPOOL_PROGRAM: Pubkey = pubkey!("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");
pub const TOKEN_PROGRAM: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
pub const TOKEN_2022_PROGRAM: Pubkey = pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
pub const ATA_PROGRAM: Pubkey = pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
pub const MEMO_PROGRAM: Pubkey = pubkey!("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
pub const SYSTEM_PROGRAM: Pubkey = pubkey!("11111111111111111111111111111111");
pub const RENT_SYSVAR: Pubkey = pubkey!("SysvarRent111111111111111111111111111111111");
pub const WSOL_MINT: Pubkey = pubkey!("So11111111111111111111111111111111111111112");

/// WhirlpoolsConfig with the 25% protocol fee that hosts every launch pool.
pub const WHIRLPOOLS_CONFIG: Pubkey = pubkey!("12yTE48QR6bGK4EMcyY8XsARbX1TRTEbwHYSuuxR1Hp8");
/// Adaptive fee tier on that config: tick spacing 128, 1% base, index 1032.
pub const ADAPTIVE_FEE_TIER: Pubkey = pubkey!("6assHYd5438D91RXfMUrETNqGHmcbfzFCJsBjHmbkeu9");
pub const FEE_TIER_INDEX: u16 = 1032;
pub const TICK_SPACING: i32 = 128;
pub const TICK_ARRAY_SIZE: i32 = 88;
pub const MAX_TICK_INDEX: i32 = 443636;
pub const MIN_TICK_INDEX: i32 = -443636;
/// Highest tick usable with tick spacing 128 (floor(MAX_TICK / 128) * 128).
pub const TOP_TICK: i32 = 443520;
/// Lowest tick usable with tick spacing 128.
pub const BOTTOM_TICK: i32 = -443520;
/// Lower bound offset for a 90% token / 10% SOL position with an open top:
/// ln(1 / (9/8)^2) / ln(1.0001) = -2355.9, floored to the tick spacing.
pub const DEFAULT_LOWER_DELTA_TICKS: i32 = -2432;
/// A reserve at or below this many base units counts as empty (liquidity rounding dust).
pub const RESERVE_DUST: u64 = 1_000;
/// Whirlpool position bundle capacity.
pub const BUNDLE_CAPACITY: u16 = 256;

pub const LAUNCH_SEED: &[u8] = b"launch";
pub const SEAT_SEED: &[u8] = b"seat";
