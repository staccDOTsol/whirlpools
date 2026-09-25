//! CPI builders: Whirlpool (Anchor discriminators + borsh args), SPL Token, ATA, System.
use crate::{constants::*, error::LaunchError};
use pinocchio::{
    account_info::AccountInfo,
    cpi::invoke_signed_unchecked,
    instruction::{Account, AccountMeta, Instruction, Seed, Signer},
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvars::Sysvar,
    ProgramResult,
};

// sha256("global:<name>")[..8]
pub const D_OPEN_BUNDLED_POSITION: [u8; 8] = [169, 113, 126, 171, 213, 172, 212, 49];
pub const D_INCREASE_LIQUIDITY_V2: [u8; 8] = [133, 29, 89, 223, 69, 238, 176, 10];
pub const D_DECREASE_LIQUIDITY_V2: [u8; 8] = [58, 127, 188, 62, 79, 82, 196, 96];
pub const D_COLLECT_FEES_V2: [u8; 8] = [207, 117, 95, 191, 229, 180, 226, 15];
pub const D_CLOSE_BUNDLED_POSITION: [u8; 8] = [41, 36, 216, 245, 27, 85, 103, 67];
pub const D_INITIALIZE_POSITION_BUNDLE: [u8; 8] = [117, 45, 241, 149, 24, 18, 194, 65];
pub const D_INITIALIZE_POOL_WITH_ADAPTIVE_FEE: [u8; 8] = [143, 94, 96, 76, 172, 124, 119, 199];
pub const D_INITIALIZE_DYNAMIC_TICK_ARRAY: [u8; 8] = [41, 33, 165, 200, 120, 231, 142, 50];
pub const D_UPDATE_FEES_AND_REWARDS: [u8; 8] = [154, 230, 250, 13, 236, 209, 75, 223];
pub const D_LOCK_POSITION: [u8; 8] = [227, 62, 2, 252, 247, 10, 171, 185];
pub const D_OPEN_POSITION_WITH_TOKEN_EXTENSIONS: [u8; 8] = [212, 47, 95, 92, 114, 102, 131, 250];

/// Signer seeds for the launch PDA: ["launch", token_mint, bump].
pub struct LaunchSigner<'a> {
    seeds: [Seed<'a>; 3],
}
impl<'a> LaunchSigner<'a> {
    pub fn new(token_mint: &'a Pubkey, bump: &'a [u8; 1]) -> Self {
        Self { seeds: [Seed::from(LAUNCH_SEED), Seed::from(token_mint.as_ref()), Seed::from(&bump[..])] }
    }
    pub fn signer(&self) -> Signer<'_, '_> {
        Signer::from(&self.seeds)
    }
}

/// Invoke with the account metas as given (Whirlpool legitimately lists the same
/// account twice, e.g. the SPL Token program as both token programs) but with each
/// account info passed to the runtime exactly once. The runtime rejects an
/// `account_infos` array that repeats an account, which pinocchio's checked
/// invoke would produce from a 1:1 meta/info list.
///
/// Safety: callers must not hold a data or lamports borrow on any account that
/// appears in `infos`; the handlers in this program release state borrows before
/// invoking, and the launch state account is never passed to a callee.
fn invoke(program: &Pubkey, metas: &[AccountMeta], infos: &[&AccountInfo], data: &[u8], signers: &[Signer]) -> ProgramResult {
    const MAX: usize = 24;
    if metas.len() != infos.len() || infos.len() > MAX {
        return Err(ProgramError::InvalidArgument);
    }
    let ix = Instruction { program_id: program, accounts: metas, data };
    let mut accounts: [core::mem::MaybeUninit<Account>; MAX] = [const { core::mem::MaybeUninit::uninit() }; MAX];
    let mut keys: [&Pubkey; MAX] = [&[0u8; 32]; MAX];
    let mut n = 0usize;
    'outer: for (meta, info) in metas.iter().zip(infos.iter()) {
        if info.key() != meta.pubkey {
            return Err(ProgramError::InvalidArgument);
        }
        for k in keys.iter().take(n) {
            if *k == info.key() {
                continue 'outer;
            }
        }
        keys[n] = info.key();
        accounts[n].write(Account::from(*info));
        n += 1;
    }
    unsafe {
        invoke_signed_unchecked(&ix, core::slice::from_raw_parts(accounts.as_ptr() as *const Account, n), signers);
    }
    Ok(())
}

#[inline(always)]
fn w(a: &AccountInfo) -> AccountMeta<'_> {
    AccountMeta::new(a.key(), true, false)
}
#[inline(always)]
fn r(a: &AccountInfo) -> AccountMeta<'_> {
    AccountMeta::new(a.key(), false, false)
}
#[inline(always)]
fn ws(a: &AccountInfo) -> AccountMeta<'_> {
    AccountMeta::new(a.key(), true, true)
}
#[inline(always)]
fn rs(a: &AccountInfo) -> AccountMeta<'_> {
    AccountMeta::new(a.key(), false, true)
}

pub fn check_program(info: &AccountInfo, expected: &Pubkey) -> Result<(), ProgramError> {
    if info.key() != expected {
        return Err(LaunchError::InvalidProgram.into());
    }
    Ok(())
}

// ------------------------------------------------------------------ Whirlpool

pub struct LiquidityAccounts<'a> {
    pub whirlpool: &'a AccountInfo,
    pub position_authority: &'a AccountInfo,
    pub position: &'a AccountInfo,
    pub position_token_account: &'a AccountInfo,
    pub token_mint_a: &'a AccountInfo,
    pub token_mint_b: &'a AccountInfo,
    pub token_owner_account_a: &'a AccountInfo,
    pub token_owner_account_b: &'a AccountInfo,
    pub token_vault_a: &'a AccountInfo,
    pub token_vault_b: &'a AccountInfo,
    pub tick_array_lower: &'a AccountInfo,
    pub tick_array_upper: &'a AccountInfo,
    pub token_program_a: &'a AccountInfo,
    pub token_program_b: &'a AccountInfo,
    pub memo_program: &'a AccountInfo,
    pub whirlpool_program: &'a AccountInfo,
}

fn liquidity_v2(a: &LiquidityAccounts, disc: [u8; 8], liquidity: u128, lim_a: u64, lim_b: u64, signers: &[Signer]) -> ProgramResult {
    check_program(a.whirlpool_program, &WHIRLPOOL_PROGRAM)?;
    let mut data = [0u8; 8 + 16 + 8 + 8 + 1];
    data[..8].copy_from_slice(&disc);
    data[8..24].copy_from_slice(&liquidity.to_le_bytes());
    data[24..32].copy_from_slice(&lim_a.to_le_bytes());
    data[32..40].copy_from_slice(&lim_b.to_le_bytes());
    // remaining_accounts_info: Option::None
    let metas = [
        w(a.whirlpool),
        r(a.token_program_a),
        r(a.token_program_b),
        r(a.memo_program),
        rs(a.position_authority),
        w(a.position),
        r(a.position_token_account),
        r(a.token_mint_a),
        r(a.token_mint_b),
        w(a.token_owner_account_a),
        w(a.token_owner_account_b),
        w(a.token_vault_a),
        w(a.token_vault_b),
        w(a.tick_array_lower),
        w(a.tick_array_upper),
    ];
    let infos = [
        a.whirlpool,
        a.token_program_a,
        a.token_program_b,
        a.memo_program,
        a.position_authority,
        a.position,
        a.position_token_account,
        a.token_mint_a,
        a.token_mint_b,
        a.token_owner_account_a,
        a.token_owner_account_b,
        a.token_vault_a,
        a.token_vault_b,
        a.tick_array_lower,
        a.tick_array_upper,
    ];
    invoke(&WHIRLPOOL_PROGRAM, &metas, &infos, &data, signers)
}

pub fn increase_liquidity_v2(a: &LiquidityAccounts, liquidity: u128, max_a: u64, max_b: u64, signers: &[Signer]) -> ProgramResult {
    liquidity_v2(a, D_INCREASE_LIQUIDITY_V2, liquidity, max_a, max_b, signers)
}

pub fn decrease_liquidity_v2(a: &LiquidityAccounts, liquidity: u128, min_a: u64, min_b: u64, signers: &[Signer]) -> ProgramResult {
    liquidity_v2(a, D_DECREASE_LIQUIDITY_V2, liquidity, min_a, min_b, signers)
}

pub fn collect_fees_v2(a: &LiquidityAccounts, signers: &[Signer]) -> ProgramResult {
    check_program(a.whirlpool_program, &WHIRLPOOL_PROGRAM)?;
    let mut data = [0u8; 9];
    data[..8].copy_from_slice(&D_COLLECT_FEES_V2);
    let metas = [
        w(a.whirlpool),
        rs(a.position_authority),
        w(a.position),
        r(a.position_token_account),
        r(a.token_mint_a),
        r(a.token_mint_b),
        w(a.token_owner_account_a),
        w(a.token_vault_a),
        w(a.token_owner_account_b),
        w(a.token_vault_b),
        r(a.token_program_a),
        r(a.token_program_b),
        r(a.memo_program),
    ];
    let infos = [
        a.whirlpool,
        a.position_authority,
        a.position,
        a.position_token_account,
        a.token_mint_a,
        a.token_mint_b,
        a.token_owner_account_a,
        a.token_vault_a,
        a.token_owner_account_b,
        a.token_vault_b,
        a.token_program_a,
        a.token_program_b,
        a.memo_program,
    ];
    invoke(&WHIRLPOOL_PROGRAM, &metas, &infos, &data, signers)
}

/// Refresh a position's owed fees from the pool's fee growth (required before collect_fees).
pub fn update_fees_and_rewards(whirlpool: &AccountInfo, position: &AccountInfo, tick_array_lower: &AccountInfo, tick_array_upper: &AccountInfo, whirlpool_program: &AccountInfo) -> ProgramResult {
    check_program(whirlpool_program, &WHIRLPOOL_PROGRAM)?;
    let metas = [w(whirlpool), w(position), r(tick_array_lower), r(tick_array_upper)];
    let infos = [whirlpool, position, tick_array_lower, tick_array_upper];
    invoke(&WHIRLPOOL_PROGRAM, &metas, &infos, &D_UPDATE_FEES_AND_REWARDS, &[])
}

pub struct OpenBundledAccounts<'a> {
    pub bundled_position: &'a AccountInfo,
    pub position_bundle: &'a AccountInfo,
    pub position_bundle_token_account: &'a AccountInfo,
    pub position_bundle_authority: &'a AccountInfo,
    pub whirlpool: &'a AccountInfo,
    pub funder: &'a AccountInfo,
    pub system_program: &'a AccountInfo,
    pub rent: &'a AccountInfo,
    pub whirlpool_program: &'a AccountInfo,
}

pub fn open_bundled_position(a: &OpenBundledAccounts, bundle_index: u16, tick_lower: i32, tick_upper: i32, signers: &[Signer]) -> ProgramResult {
    check_program(a.whirlpool_program, &WHIRLPOOL_PROGRAM)?;
    let mut data = [0u8; 8 + 2 + 4 + 4];
    data[..8].copy_from_slice(&D_OPEN_BUNDLED_POSITION);
    data[8..10].copy_from_slice(&bundle_index.to_le_bytes());
    data[10..14].copy_from_slice(&tick_lower.to_le_bytes());
    data[14..18].copy_from_slice(&tick_upper.to_le_bytes());
    let metas = [
        w(a.bundled_position),
        w(a.position_bundle),
        r(a.position_bundle_token_account),
        rs(a.position_bundle_authority),
        r(a.whirlpool),
        ws(a.funder),
        r(a.system_program),
        r(a.rent),
    ];
    let infos = [
        a.bundled_position,
        a.position_bundle,
        a.position_bundle_token_account,
        a.position_bundle_authority,
        a.whirlpool,
        a.funder,
        a.system_program,
        a.rent,
    ];
    invoke(&WHIRLPOOL_PROGRAM, &metas, &infos, &data, signers)
}

pub fn close_bundled_position(
    bundled_position: &AccountInfo,
    position_bundle: &AccountInfo,
    position_bundle_token_account: &AccountInfo,
    position_bundle_authority: &AccountInfo,
    receiver: &AccountInfo,
    whirlpool_program: &AccountInfo,
    bundle_index: u16,
    signers: &[Signer],
) -> ProgramResult {
    check_program(whirlpool_program, &WHIRLPOOL_PROGRAM)?;
    let mut data = [0u8; 10];
    data[..8].copy_from_slice(&D_CLOSE_BUNDLED_POSITION);
    data[8..10].copy_from_slice(&bundle_index.to_le_bytes());
    let metas = [w(bundled_position), w(position_bundle), r(position_bundle_token_account), rs(position_bundle_authority), w(receiver)];
    let infos = [bundled_position, position_bundle, position_bundle_token_account, position_bundle_authority, receiver];
    invoke(&WHIRLPOOL_PROGRAM, &metas, &infos, &data, signers)
}

pub struct InitBundleAccounts<'a> {
    pub position_bundle: &'a AccountInfo,
    pub position_bundle_mint: &'a AccountInfo,
    pub position_bundle_token_account: &'a AccountInfo,
    pub position_bundle_owner: &'a AccountInfo,
    pub funder: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
    pub system_program: &'a AccountInfo,
    pub rent: &'a AccountInfo,
    pub ata_program: &'a AccountInfo,
    pub whirlpool_program: &'a AccountInfo,
}

pub fn initialize_position_bundle(a: &InitBundleAccounts) -> ProgramResult {
    check_program(a.whirlpool_program, &WHIRLPOOL_PROGRAM)?;
    let metas = [
        w(a.position_bundle),
        ws(a.position_bundle_mint),
        w(a.position_bundle_token_account),
        r(a.position_bundle_owner),
        ws(a.funder),
        r(a.token_program),
        r(a.system_program),
        r(a.rent),
        r(a.ata_program),
    ];
    let infos = [
        a.position_bundle,
        a.position_bundle_mint,
        a.position_bundle_token_account,
        a.position_bundle_owner,
        a.funder,
        a.token_program,
        a.system_program,
        a.rent,
        a.ata_program,
    ];
    invoke(&WHIRLPOOL_PROGRAM, &metas, &infos, &D_INITIALIZE_POSITION_BUNDLE, &[])
}

pub struct InitPoolAccounts<'a> {
    pub whirlpools_config: &'a AccountInfo,
    pub token_mint_a: &'a AccountInfo,
    pub token_mint_b: &'a AccountInfo,
    pub token_badge_a: &'a AccountInfo,
    pub token_badge_b: &'a AccountInfo,
    pub funder: &'a AccountInfo,
    pub initialize_pool_authority: &'a AccountInfo,
    pub whirlpool: &'a AccountInfo,
    pub oracle: &'a AccountInfo,
    pub token_vault_a: &'a AccountInfo,
    pub token_vault_b: &'a AccountInfo,
    pub adaptive_fee_tier: &'a AccountInfo,
    pub token_program_a: &'a AccountInfo,
    pub token_program_b: &'a AccountInfo,
    pub system_program: &'a AccountInfo,
    pub rent: &'a AccountInfo,
    pub whirlpool_program: &'a AccountInfo,
}

pub fn initialize_pool_with_adaptive_fee(a: &InitPoolAccounts, initial_sqrt_price: u128) -> ProgramResult {
    check_program(a.whirlpool_program, &WHIRLPOOL_PROGRAM)?;
    let mut data = [0u8; 8 + 16 + 1];
    data[..8].copy_from_slice(&D_INITIALIZE_POOL_WITH_ADAPTIVE_FEE);
    data[8..24].copy_from_slice(&initial_sqrt_price.to_le_bytes());
    // trade_enable_timestamp: Option::None
    let metas = [
        r(a.whirlpools_config),
        r(a.token_mint_a),
        r(a.token_mint_b),
        r(a.token_badge_a),
        r(a.token_badge_b),
        ws(a.funder),
        rs(a.initialize_pool_authority),
        w(a.whirlpool),
        w(a.oracle),
        ws(a.token_vault_a),
        ws(a.token_vault_b),
        r(a.adaptive_fee_tier),
        r(a.token_program_a),
        r(a.token_program_b),
        r(a.system_program),
        r(a.rent),
    ];
    let infos = [
        a.whirlpools_config,
        a.token_mint_a,
        a.token_mint_b,
        a.token_badge_a,
        a.token_badge_b,
        a.funder,
        a.initialize_pool_authority,
        a.whirlpool,
        a.oracle,
        a.token_vault_a,
        a.token_vault_b,
        a.adaptive_fee_tier,
        a.token_program_a,
        a.token_program_b,
        a.system_program,
        a.rent,
    ];
    invoke(&WHIRLPOOL_PROGRAM, &metas, &infos, &data, &[])
}

/// Idempotent: a no-op if the array already exists.
pub fn initialize_dynamic_tick_array(
    whirlpool: &AccountInfo,
    funder: &AccountInfo,
    tick_array: &AccountInfo,
    system_program: &AccountInfo,
    whirlpool_program: &AccountInfo,
    start_tick_index: i32,
) -> ProgramResult {
    check_program(whirlpool_program, &WHIRLPOOL_PROGRAM)?;
    let mut data = [0u8; 8 + 4 + 1];
    data[..8].copy_from_slice(&D_INITIALIZE_DYNAMIC_TICK_ARRAY);
    data[8..12].copy_from_slice(&start_tick_index.to_le_bytes());
    data[12] = 1;
    let metas = [r(whirlpool), ws(funder), w(tick_array), r(system_program)];
    let infos = [whirlpool, funder, tick_array, system_program];
    invoke(&WHIRLPOOL_PROGRAM, &metas, &infos, &data, &[])
}

pub struct OpenPositionTeAccounts<'a> {
    pub funder: &'a AccountInfo,
    pub owner: &'a AccountInfo,
    pub position: &'a AccountInfo,
    pub position_mint: &'a AccountInfo,
    pub position_token_account: &'a AccountInfo,
    pub whirlpool: &'a AccountInfo,
    pub token_2022_program: &'a AccountInfo,
    pub system_program: &'a AccountInfo,
    pub ata_program: &'a AccountInfo,
    pub metadata_update_auth: &'a AccountInfo,
    pub whirlpool_program: &'a AccountInfo,
}

pub fn open_position_with_token_extensions(a: &OpenPositionTeAccounts, tick_lower: i32, tick_upper: i32) -> ProgramResult {
    check_program(a.whirlpool_program, &WHIRLPOOL_PROGRAM)?;
    let mut data = [0u8; 8 + 4 + 4 + 1];
    data[..8].copy_from_slice(&D_OPEN_POSITION_WITH_TOKEN_EXTENSIONS);
    data[8..12].copy_from_slice(&tick_lower.to_le_bytes());
    data[12..16].copy_from_slice(&tick_upper.to_le_bytes());
    data[16] = 0; // with_token_metadata_extension = false (cheap)
    let metas = [
        ws(a.funder),
        r(a.owner),
        w(a.position),
        ws(a.position_mint),
        w(a.position_token_account),
        r(a.whirlpool),
        r(a.token_2022_program),
        r(a.system_program),
        r(a.ata_program),
        r(a.metadata_update_auth),
    ];
    let infos = [
        a.funder,
        a.owner,
        a.position,
        a.position_mint,
        a.position_token_account,
        a.whirlpool,
        a.token_2022_program,
        a.system_program,
        a.ata_program,
        a.metadata_update_auth,
    ];
    invoke(&WHIRLPOOL_PROGRAM, &metas, &infos, &data, &[])
}

pub struct LockAccounts<'a> {
    pub funder: &'a AccountInfo,
    pub position_authority: &'a AccountInfo,
    pub position: &'a AccountInfo,
    pub position_mint: &'a AccountInfo,
    pub position_token_account: &'a AccountInfo,
    pub lock_config: &'a AccountInfo,
    pub whirlpool: &'a AccountInfo,
    pub token_2022_program: &'a AccountInfo,
    pub system_program: &'a AccountInfo,
    pub whirlpool_program: &'a AccountInfo,
}

pub fn lock_position_permanent(a: &LockAccounts, signers: &[Signer]) -> ProgramResult {
    check_program(a.whirlpool_program, &WHIRLPOOL_PROGRAM)?;
    let mut data = [0u8; 9];
    data[..8].copy_from_slice(&D_LOCK_POSITION);
    data[8] = 0; // LockType::Permanent
    let metas = [
        ws(a.funder),
        rs(a.position_authority),
        w(a.position),
        r(a.position_mint),
        w(a.position_token_account),
        w(a.lock_config),
        r(a.whirlpool),
        r(a.token_2022_program),
        r(a.system_program),
    ];
    let infos = [
        a.funder,
        a.position_authority,
        a.position,
        a.position_mint,
        a.position_token_account,
        a.lock_config,
        a.whirlpool,
        a.token_2022_program,
        a.system_program,
    ];
    invoke(&WHIRLPOOL_PROGRAM, &metas, &infos, &data, signers)
}

// ------------------------------------------------------------------ Token / ATA / System

/// SPL Token transfer (launch token side, always SPL Token).
pub fn token_transfer(from: &AccountInfo, to: &AccountInfo, authority: &AccountInfo, amount: u64, signers: &[Signer]) -> ProgramResult {
    if amount == 0 {
        return Ok(());
    }
    pinocchio_token::instructions::Transfer { from, to, authority, amount }.invoke_signed(signers)
}

/// TransferChecked through whichever token program owns the mint (SPL Token or Token-2022).
pub fn transfer_checked(
    token_program: &AccountInfo,
    from: &AccountInfo,
    mint: &AccountInfo,
    to: &AccountInfo,
    authority: &AccountInfo,
    amount: u64,
    decimals: u8,
    signers: &[Signer],
) -> ProgramResult {
    if amount == 0 {
        return Ok(());
    }
    if token_program.key() != &TOKEN_PROGRAM && token_program.key() != &TOKEN_2022_PROGRAM {
        return Err(LaunchError::InvalidProgram.into());
    }
    let mut data = [0u8; 10];
    data[0] = 12; // TransferChecked
    data[1..9].copy_from_slice(&amount.to_le_bytes());
    data[9] = decimals;
    let metas = [w(from), r(mint), w(to), rs(authority)];
    let infos = [from, mint, to, authority];
    invoke(token_program.key(), &metas, &infos, &data, signers)
}

/// Parsed view of an SPL Token or Token-2022 token account (base layout is identical).
pub struct TokenView {
    pub mint: Pubkey,
    pub owner: Pubkey,
    pub amount: u64,
}

pub fn read_token_account(info: &AccountInfo) -> Result<TokenView, ProgramError> {
    let owner = info.owner();
    if owner != &TOKEN_PROGRAM && owner != &TOKEN_2022_PROGRAM {
        return Err(LaunchError::InvalidTokenAccount.into());
    }
    let d = info.try_borrow_data()?;
    if d.len() < 165 || d[108] != 1 {
        return Err(LaunchError::InvalidTokenAccount.into()); // state must be Initialized
    }
    Ok(TokenView { mint: d[0..32].try_into().unwrap(), owner: d[32..64].try_into().unwrap(), amount: u64::from_le_bytes(d[64..72].try_into().unwrap()) })
}

pub fn token_amount(info: &AccountInfo) -> Result<u64, ProgramError> {
    Ok(read_token_account(info)?.amount)
}

/// Mint decimals for SPL Token or Token-2022 mints.
pub fn mint_decimals(info: &AccountInfo) -> Result<u8, ProgramError> {
    let owner = info.owner();
    if owner != &TOKEN_PROGRAM && owner != &TOKEN_2022_PROGRAM {
        return Err(LaunchError::InvalidMint.into());
    }
    let d = info.try_borrow_data()?;
    if d.len() < 82 || d[45] != 1 {
        return Err(LaunchError::InvalidMint.into());
    }
    Ok(d[44])
}

/// Token-2022 mint extensions this launchpad refuses as a quote token.
const REJECTED_EXTENSIONS: [u16; 9] = [
    3,  // MintCloseAuthority
    4,  // ConfidentialTransferMint
    6,  // DefaultAccountState
    9,  // NonTransferable
    12, // PermanentDelegate
    14, // TransferHook
    16, // ConfidentialTransferFeeConfig
    24, // ConfidentialMintBurn
    26, // Pausable
];

/// Accept any SPL Token mint, or a Token-2022 mint whose extensions are all benign.
pub fn check_quote_mint(info: &AccountInfo) -> Result<(), ProgramError> {
    let owner = info.owner();
    if owner == &TOKEN_PROGRAM {
        return Ok(());
    }
    if owner != &TOKEN_2022_PROGRAM {
        return Err(LaunchError::InvalidMint.into());
    }
    let d = info.try_borrow_data()?;
    if d.len() <= 165 {
        return Ok(()); // no extensions
    }
    if d[165] != 1 {
        return Err(LaunchError::InvalidMint.into()); // account type must be Mint
    }
    let mut i = 166;
    while i + 4 <= d.len() {
        let ext = u16::from_le_bytes([d[i], d[i + 1]]);
        let len = u16::from_le_bytes([d[i + 2], d[i + 3]]) as usize;
        if ext == 0 {
            break;
        }
        if REJECTED_EXTENSIONS.contains(&ext) {
            return Err(LaunchError::UnsupportedQuoteMint.into());
        }
        i += 4 + len;
    }
    Ok(())
}

/// Create an associated token account (idempotent). `owner` may be any pubkey.
pub fn create_ata_idempotent(
    payer: &AccountInfo,
    ata: &AccountInfo,
    owner: &AccountInfo,
    mint: &AccountInfo,
    system_program: &AccountInfo,
    token_program: &AccountInfo,
    ata_program: &AccountInfo,
) -> ProgramResult {
    check_program(ata_program, &ATA_PROGRAM)?;
    let metas = [ws(payer), w(ata), r(owner), r(mint), r(system_program), r(token_program)];
    let infos = [payer, ata, owner, mint, system_program, token_program];
    invoke(&ATA_PROGRAM, &metas, &infos, &[1u8], &[])
}

/// Create a program-owned PDA account of `space` bytes, rent exempt, signed with `seeds`.
pub fn create_pda_account(payer: &AccountInfo, new: &AccountInfo, space: usize, owner: &Pubkey, seeds: &[Seed]) -> ProgramResult {
    let rent = pinocchio::sysvars::rent::Rent::get()?;
    let lamports = rent.minimum_balance(space);
    let signer = Signer::from(seeds);
    pinocchio_system::instructions::CreateAccount { from: payer, to: new, lamports, space: space as u64, owner }.invoke_signed(&[signer])
}

/// Create a plain SPL mint account (payer + mint keypair sign) and initialize it.
pub fn create_mint(payer: &AccountInfo, mint: &AccountInfo, decimals: u8, mint_authority: &Pubkey, token_program: &AccountInfo) -> ProgramResult {
    check_program(token_program, &TOKEN_PROGRAM)?;
    let rent = pinocchio::sysvars::rent::Rent::get()?;
    let space = pinocchio_token::state::Mint::LEN;
    pinocchio_system::instructions::CreateAccount {
        from: payer,
        to: mint,
        lamports: rent.minimum_balance(space),
        space: space as u64,
        owner: &TOKEN_PROGRAM,
    }
    .invoke()?;
    pinocchio_token::instructions::InitializeMint2 { mint, decimals, mint_authority, freeze_authority: None }.invoke()
}

pub fn mint_to(mint: &AccountInfo, account: &AccountInfo, mint_authority: &AccountInfo, amount: u64, signers: &[Signer]) -> ProgramResult {
    pinocchio_token::instructions::MintTo { mint, account, mint_authority, amount }.invoke_signed(signers)
}

pub fn revoke_mint_authority(mint: &AccountInfo, authority: &AccountInfo, signers: &[Signer]) -> ProgramResult {
    pinocchio_token::instructions::SetAuthority {
        account: mint,
        authority,
        authority_type: pinocchio_token::instructions::AuthorityType::MintTokens,
        new_authority: None,
    }
    .invoke_signed(signers)
}

pub fn burn(account: &AccountInfo, mint: &AccountInfo, authority: &AccountInfo, amount: u64) -> ProgramResult {
    pinocchio_token::instructions::Burn { account, mint, authority, amount }.invoke()
}

/// Move all lamports out of `account` to `to` and zero its data so the runtime reaps it.
pub fn close_program_account(account: &AccountInfo, to: &AccountInfo) -> ProgramResult {
    let amount = account.lamports();
    {
        let mut dst = to.try_borrow_mut_lamports()?;
        *dst = dst.checked_add(amount).ok_or(LaunchError::MathOverflow)?;
    }
    {
        let mut src = account.try_borrow_mut_lamports()?;
        *src = 0;
    }
    account.close()
}
