//! End-to-end lifecycle on litesvm against the mainnet Whirlpool program binary.
//! Runs with WSOL (SPL Token quote, launch token as Whirlpool token A) and with a
//! Token-2022 quote where the launch token lands on the B side.
use base64::Engine;
use litesvm::LiteSVM;
use solana_sdk::{
    account::Account,
    instruction::{AccountMeta, Instruction},
    program_pack::Pack,
    pubkey,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction, system_program,
    sysvar::{clock::Clock, rent},
    transaction::Transaction,
};

const WHIRLPOOL: Pubkey = pubkey!("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");
const CONFIG: Pubkey = pubkey!("12yTE48QR6bGK4EMcyY8XsARbX1TRTEbwHYSuuxR1Hp8");
const FEE_TIER: Pubkey = pubkey!("6assHYd5438D91RXfMUrETNqGHmcbfzFCJsBjHmbkeu9");
const WSOL: Pubkey = pubkey!("So11111111111111111111111111111111111111112");
const MEMO: Pubkey = pubkey!("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
const TOKEN_2022: Pubkey = pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
const METADATA_UPDATE_AUTH: Pubkey = pubkey!("3axbTs2z5GBy6usVbNVoqEgZMng3vZvMnAoX29BFfwhr");
const MAX_SQRT_PRICE: u128 = 79226673515401279992447579055;
const MIN_SQRT_PRICE: u128 = 4295048016;
const TICK_SPACING: i32 = 128;
const TICKS_PER_ARRAY: i32 = 88 * 128;
const TOP_TICK: i32 = 443520;
const BOTTOM_TICK: i32 = -443520;
const SUPPLY: u64 = 1_000_000_000_000_000; // 1B tokens, 6 decimals
/// ~411 SOL of the supply when the token is A: 411e9 / 1e15 = 4.11e-4 -> tick ≈ -77930
const INITIAL_TICK_A: i32 = -77952;
const QUOTE_UNIT: u64 = 1_000_000_000; // 1 SOL / 1 quote token (9 decimals for the Token-2022 quote too)

fn program_id() -> Pubkey {
    Pubkey::new_from_array(ladder_launch::ID)
}
fn ata_for(owner: &Pubkey, mint: &Pubkey, program: &Pubkey) -> Pubkey {
    spl_associated_token_account::get_associated_token_address_with_program_id(owner, mint, program)
}
fn pda(seeds: &[&[u8]], program: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(seeds, program).0
}
fn floor_ts(t: i32, s: i32) -> i32 {
    t.div_euclid(s) * s
}
fn tick_array_start(t: i32) -> i32 {
    floor_ts(t, TICKS_PER_ARRAY)
}
fn tick_array(whirlpool: &Pubkey, start: i32) -> Pubkey {
    pda(&[b"tick_array", whirlpool.as_ref(), start.to_string().as_bytes()], &WHIRLPOOL)
}

fn load_fixture(svm: &mut LiteSVM, name: &str) {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(format!("tests/fixtures/{name}.json")).unwrap()).unwrap();
    let pk: Pubkey = v["pubkey"].as_str().unwrap().parse().unwrap();
    let a = &v["account"];
    let data = base64::engine::general_purpose::STANDARD.decode(a["data"][0].as_str().unwrap()).unwrap();
    svm.set_account(
        pk,
        Account { lamports: a["lamports"].as_u64().unwrap(), data, owner: a["owner"].as_str().unwrap().parse().unwrap(), executable: false, rent_epoch: 0 },
    )
    .unwrap();
}

fn packed_mint(decimals: u8, authority: Option<Pubkey>) -> Vec<u8> {
    let mut data = vec![0u8; spl_token::state::Mint::LEN];
    spl_token::state::Mint { mint_authority: authority.into(), supply: 0, decimals, is_initialized: true, freeze_authority: None.into() }.pack_into_slice(&mut data);
    data
}

#[derive(Clone, Copy, PartialEq)]
enum Quote {
    Wsol,
    Token2022,
}

struct Env {
    svm: LiteSVM,
    quote: Quote,
    quote_mint: Pubkey,
    quote_program: Pubkey,
    quote_authority: Keypair,
    creator: Keypair,
    token_mint: Keypair,
    token_is_a: bool,
    launch: Pubkey,
    reserve_vault: Pubkey,
    quote_vault: Pubkey,
    whirlpool: Pubkey,
    oracle: Pubkey,
    vault_token: Pubkey,
    vault_quote: Pubkey,
    bundle_mint: Pubkey,
    position_bundle: Pubkey,
    bundle_ata: Pubkey,
}

impl Env {
    fn send(&mut self, ixs: &[Instruction], signers: &[&Keypair]) -> Result<litesvm::types::TransactionMetadata, String> {
        let payer = signers[0];
        let bh = self.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(ixs, Some(&payer.pubkey()), signers, bh);
        self.svm.send_transaction(tx).map_err(|e| format!("{:?}\n{}", e.err, e.meta.logs.join("\n")))
    }
    fn must(&mut self, what: &str, ixs: &[Instruction], signers: &[&Keypair]) -> litesvm::types::TransactionMetadata {
        match self.send(ixs, signers) {
            Ok(m) => m,
            Err(e) => panic!("{what} failed:\n{e}"),
        }
    }
    fn token_balance(&self, acc: &Pubkey) -> u64 {
        let a = self.svm.get_account(acc).expect("token account");
        u64::from_le_bytes(a.data[64..72].try_into().unwrap())
    }
    fn pool_tick(&self) -> i32 {
        let d = self.svm.get_account(&self.whirlpool).unwrap().data;
        i32::from_le_bytes(d[81..85].try_into().unwrap())
    }
    fn pool_sqrt_price(&self) -> u128 {
        let d = self.svm.get_account(&self.whirlpool).unwrap().data;
        u128::from_le_bytes(d[65..81].try_into().unwrap())
    }
    /// quote per token base unit at the pool price
    fn token_price(&self) -> f64 {
        let s = self.pool_sqrt_price() as f64 / 2f64.powi(64);
        if self.token_is_a {
            s * s
        } else {
            1.0 / (s * s)
        }
    }
    fn next_index(&self) -> u16 {
        let d = self.svm.get_account(&self.launch).unwrap().data;
        u16::from_le_bytes([d[322], d[323]]) // after disc, bump and ten pubkeys
    }
    fn vaults_ab(&self) -> (Pubkey, Pubkey) {
        if self.token_is_a {
            (self.vault_token, self.vault_quote)
        } else {
            (self.vault_quote, self.vault_token)
        }
    }
    fn advance_time(&mut self, secs: i64) {
        let mut clock: Clock = self.svm.get_sysvar();
        clock.unix_timestamp += secs;
        clock.slot += (secs as u64) * 2 + 1;
        self.svm.set_sysvar(&clock);
        self.svm.expire_blockhash();
    }
    fn user_quote(&self, user: &Pubkey) -> Pubkey {
        ata_for(user, &self.quote_mint, &self.quote_program)
    }
    fn user_token(&self, user: &Pubkey) -> Pubkey {
        ata_for(user, &self.token_mint.pubkey(), &TOKEN_2022)
    }
    /// Fund `user` with lamports and `amount` of the quote in their ATA (wrapped SOL or minted Token-2022).
    fn new_user(&mut self, amount: u64) -> Keypair {
        let user = Keypair::new();
        self.svm.airdrop(&user.pubkey(), amount + 5_000_000_000).unwrap();
        let q = self.user_quote(&user.pubkey());
        let mut ixs = vec![
            spl_associated_token_account::instruction::create_associated_token_account(&user.pubkey(), &user.pubkey(), &self.quote_mint, &self.quote_program),
            spl_associated_token_account::instruction::create_associated_token_account_idempotent(&user.pubkey(), &user.pubkey(), &self.token_mint.pubkey(), &TOKEN_2022),
        ];
        match self.quote {
            Quote::Wsol => {
                ixs.push(system_instruction::transfer(&user.pubkey(), &q, amount));
                ixs.push(spl_token::instruction::sync_native(&spl_token::id(), &q).unwrap());
                self.must("fund user", &ixs, &[&user]);
            }
            Quote::Token2022 => {
                let mut data = vec![7u8]; // MintTo
                data.extend(amount.to_le_bytes());
                ixs.push(Instruction {
                    program_id: TOKEN_2022,
                    accounts: vec![AccountMeta::new(self.quote_mint, false), AccountMeta::new(q, false), AccountMeta::new_readonly(self.quote_authority.pubkey(), true)],
                    data,
                });
                let auth = Keypair::from_bytes(&self.quote_authority.to_bytes()).unwrap();
                self.must("fund user", &ixs, &[&user, &auth]);
            }
        }
        user
    }
}

fn ix(accounts: Vec<AccountMeta>, data: Vec<u8>) -> Instruction {
    Instruction { program_id: program_id(), accounts, data }
}
fn w(k: Pubkey) -> AccountMeta {
    AccountMeta::new(k, false)
}
fn r(k: Pubkey) -> AccountMeta {
    AccountMeta::new_readonly(k, false)
}
fn ws(k: Pubkey) -> AccountMeta {
    AccountMeta::new(k, true)
}
fn rs(k: Pubkey) -> AccountMeta {
    AccountMeta::new_readonly(k, true)
}

fn create_launch_args() -> Vec<u8> {
    // decimals 6, supply 1B, min age 60s, exit cap 10%/min, lower delta -2432, floor 5%, creator 50%
    let mut data = vec![0u8, 6];
    data.extend(SUPPLY.to_le_bytes());
    data.extend(60u32.to_le_bytes());
    data.extend(1000u16.to_le_bytes());
    data.extend((-2432i32).to_le_bytes());
    data.extend(500u16.to_le_bytes());
    data.extend(5000u16.to_le_bytes());
    for s in ["Test Token", "TEST", "https://example.com/meta.json"] {
        data.push(s.len() as u8);
        data.extend(s.as_bytes());
    }
    data
}

fn base_svm() -> LiteSVM {
    let mut svm = LiteSVM::new();
    svm.add_program(WHIRLPOOL, &std::fs::read("tests/fixtures/whirlpool.so").expect("run from programs/ladder-launch after RPC_URL=... python3 tests/fixtures/fetch_whirlpool.py"));
    svm.add_program(program_id(), &std::fs::read("../../target/deploy/ladder_launch.so").expect("cargo build-sbf first"));
    load_fixture(&mut svm, "whirlpools_config");
    load_fixture(&mut svm, "adaptive_fee_tier");
    svm.set_account(WSOL, Account { lamports: 1_000_000_000, data: packed_mint(9, None), owner: spl_token::id(), executable: false, rent_epoch: 0 }).unwrap();
    svm
}

/// Build a launch. `token_is_a` picks the mint ordering by grinding the token mint keypair.
fn setup(quote: Quote, token_is_a: bool) -> Env {
    let mut svm = base_svm();
    let creator = Keypair::new();
    svm.airdrop(&creator.pubkey(), 100_000_000_000).unwrap();
    let treasury = Keypair::new().pubkey();
    let quote_authority = Keypair::new();
    let (quote_mint, quote_program) = match quote {
        Quote::Wsol => (WSOL, spl_token::id()),
        Quote::Token2022 => {
            let m = Keypair::new().pubkey();
            svm.set_account(m, Account { lamports: 1_000_000_000, data: packed_mint(9, Some(quote_authority.pubkey())), owner: TOKEN_2022, executable: false, rent_epoch: 0 }).unwrap();
            (m, TOKEN_2022)
        }
    };
    let token_mint = loop {
        let k = Keypair::new();
        if (k.pubkey().to_bytes() < quote_mint.to_bytes()) == token_is_a {
            break k;
        }
    };
    let launch = pda(&[b"launch", token_mint.pubkey().as_ref()], &program_id());
    let reserve_vault = ata_for(&launch, &token_mint.pubkey(), &TOKEN_2022);
    let quote_vault = ata_for(&launch, &quote_mint, &quote_program);
    let (mint_a, mint_b) = if token_is_a { (token_mint.pubkey(), quote_mint) } else { (quote_mint, token_mint.pubkey()) };
    let whirlpool = pda(&[b"whirlpool", CONFIG.as_ref(), mint_a.as_ref(), mint_b.as_ref(), &1032u16.to_le_bytes()], &WHIRLPOOL);
    let oracle = pda(&[b"oracle", whirlpool.as_ref()], &WHIRLPOOL);
    let vault_token_kp = Keypair::new();
    let vault_quote_kp = Keypair::new();
    let bundle_kp = Keypair::new();
    let position_bundle = pda(&[b"position_bundle", bundle_kp.pubkey().as_ref()], &WHIRLPOOL);
    let bundle_ata = ata_for(&launch, &bundle_kp.pubkey(), &spl_token::id());
    let mut env = Env {
        svm,
        quote,
        quote_mint,
        quote_program,
        quote_authority,
        creator,
        token_mint,
        token_is_a,
        launch,
        reserve_vault,
        quote_vault,
        whirlpool,
        oracle,
        vault_token: vault_token_kp.pubkey(),
        vault_quote: vault_quote_kp.pubkey(),
        bundle_mint: bundle_kp.pubkey(),
        position_bundle,
        bundle_ata,
    };
    let creator_kp = Keypair::from_bytes(&env.creator.to_bytes()).unwrap();
    let mint_kp = Keypair::from_bytes(&env.token_mint.to_bytes()).unwrap();
    let creator_pk = creator_kp.pubkey();
    let mint_pk = mint_kp.pubkey();

    // 0 create_launch
    let create = ix(
        vec![
            ws(creator_pk),
            w(launch),
            ws(mint_pk),
            w(reserve_vault),
            r(quote_mint),
            w(quote_vault),
            r(treasury),
            r(spl_token::id()),
            r(TOKEN_2022),
            r(spl_associated_token_account::id()),
            r(system_program::id()),
            r(rent::id()),
        ],
        create_launch_args(),
    );
    env.must("create_launch", &[create], &[&creator_kp, &mint_kp]);
    assert_eq!(env.token_balance(&reserve_vault), SUPPLY);
    let mint_acc = env.svm.get_account(&mint_pk).unwrap();
    assert_eq!(mint_acc.owner, TOKEN_2022, "launch token is Token-2022");
    let mint = spl_token::state::Mint::unpack_from_slice(&mint_acc.data[..82]).unwrap();
    assert!(mint.mint_authority.is_none(), "mint authority must be revoked");
    assert!(mint.freeze_authority.is_none(), "no freeze authority, ever");
    assert_eq!(mint.supply, SUPPLY);
    // TokenMetadata TLV (type 19) starts with update_authority: must be the zero key (none).
    let tlv = &mint_acc.data[166..];
    let mut o = 0;
    let mut found = false;
    while o + 4 <= tlv.len() {
        let (ty, len) = (u16::from_le_bytes([tlv[o], tlv[o + 1]]) as usize, u16::from_le_bytes([tlv[o + 2], tlv[o + 3]]) as usize);
        if ty == 19 {
            assert_eq!(&tlv[o + 4..o + 36], &[0u8; 32], "metadata update authority must be none");
            found = true;
            break;
        }
        if ty == 0 { break; }
        o += 4 + len;
    }
    assert!(found, "TokenMetadata extension present");
    assert!(mint_acc.data.windows(10).any(|w| w == b"Test Token"), "metadata lives in the mint");
    assert!(mint_acc.data.windows(29).any(|w| w == b"https://example.com/meta.json"), "uri lives in the mint");

    // 1 init_pool: same token price either way (price is B per A, so flip the tick when the token is B)
    let initial_tick = if token_is_a { INITIAL_TICK_A } else { -INITIAL_TICK_A };
    let badge_token = pda(&[b"token_badge", CONFIG.as_ref(), mint_pk.as_ref()], &WHIRLPOOL);
    let badge_quote = pda(&[b"token_badge", CONFIG.as_ref(), quote_mint.as_ref()], &WHIRLPOOL);
    let mut data = vec![1u8];
    data.extend(initial_tick.to_le_bytes());
    let init = ix(
        vec![
            ws(creator_pk),
            w(launch),
            r(CONFIG),
            r(mint_pk),
            r(quote_mint),
            r(badge_token),
            r(badge_quote),
            w(whirlpool),
            w(oracle),
            ws(vault_token_kp.pubkey()),
            ws(vault_quote_kp.pubkey()),
            r(FEE_TIER),
            r(spl_token::id()),
            r(TOKEN_2022),
            r(system_program::id()),
            r(rent::id()),
            r(WHIRLPOOL),
        ],
        data,
    );
    env.must("init_pool", &[init], &[&creator_kp, &vault_token_kp, &vault_quote_kp]);
    assert_eq!(env.pool_tick(), initial_tick);

    // 2 new_bundle
    let nb = ix(
        vec![
            ws(creator_pk),
            w(launch),
            w(position_bundle),
            ws(bundle_kp.pubkey()),
            w(bundle_ata),
            r(spl_token::id()),
            r(system_program::id()),
            r(rent::id()),
            r(spl_associated_token_account::id()),
            r(WHIRLPOOL),
        ],
        vec![2u8],
    );
    env.must("new_bundle", &[nb], &[&creator_kp, &bundle_kp]);
    env
}

fn seed_floor(env: &mut Env) -> Pubkey {
    let creator_kp = Keypair::from_bytes(&env.creator.to_bytes()).unwrap();
    let pos_mint = Keypair::new();
    let position = pda(&[b"position", pos_mint.pubkey().as_ref()], &WHIRLPOOL);
    let pos_ata = ata_for(&env.launch, &pos_mint.pubkey(), &TOKEN_2022);
    let lock_config = pda(&[b"lock_config", position.as_ref()], &WHIRLPOOL);
    let tick = env.pool_tick();
    let (lower, upper) = if env.token_is_a { (floor_ts(tick, TICK_SPACING) + TICK_SPACING, TOP_TICK) } else { (BOTTOM_TICK, floor_ts(tick, TICK_SPACING)) };
    let ta_lower = tick_array(&env.whirlpool, tick_array_start(lower));
    let ta_upper = tick_array(&env.whirlpool, tick_array_start(upper));
    let (va, vb) = env.vaults_ab();
    let sf = ix(
        vec![
            ws(env.creator.pubkey()),
            w(env.launch),
            w(env.whirlpool),
            w(position),
            ws(pos_mint.pubkey()),
            w(pos_ata),
            w(env.reserve_vault),
            w(env.quote_vault),
            w(va),
            w(vb),
            w(ta_lower),
            w(ta_upper),
            r(env.token_mint.pubkey()),
            r(env.quote_mint),
            w(lock_config),
            r(METADATA_UPDATE_AUTH),
            r(spl_token::id()),
            r(TOKEN_2022),
            r(system_program::id()),
            r(spl_associated_token_account::id()),
            r(MEMO),
            r(WHIRLPOOL),
        ],
        vec![3u8],
    );
    env.must("seed_floor", &[sf], &[&creator_kp, &pos_mint]);
    position
}

#[derive(Debug)]
struct SeatRefs {
    index: u16,
    nft_mint: Pubkey,
    nft_ata: Pubkey,
    seat: Pubkey,
    bundled_position: Pubkey,
    ta_lower: Pubkey,
    ta_upper: Pubkey,
}

fn deposit(env: &mut Env, user: &Keypair, amount: u64) -> Result<SeatRefs, String> {
    let index = env.next_index();
    let nft = Keypair::new();
    let nft_ata = ata_for(&user.pubkey(), &nft.pubkey(), &spl_token::id());
    let seat = pda(&[b"seat", env.launch.as_ref(), env.bundle_mint.as_ref(), &index.to_le_bytes()], &program_id());
    let bundled_position = pda(&[b"bundled_position", env.bundle_mint.as_ref(), index.to_string().as_bytes()], &WHIRLPOOL);
    let tick = env.pool_tick();
    let (lower, upper) = if env.token_is_a { (floor_ts(tick - 2432, TICK_SPACING), TOP_TICK) } else { (BOTTOM_TICK, floor_ts(tick + 2432, TICK_SPACING)) };
    let ta_lower = tick_array(&env.whirlpool, tick_array_start(lower));
    let ta_upper = tick_array(&env.whirlpool, tick_array_start(upper));
    let (va, vb) = env.vaults_ab();
    let mut data = vec![4u8];
    data.extend(amount.to_le_bytes());
    let dep = ix(
        vec![
            ws(user.pubkey()),
            w(env.launch),
            r(env.token_mint.pubkey()),
            w(env.whirlpool),
            w(env.reserve_vault),
            w(env.quote_vault),
            w(env.user_quote(&user.pubkey())),
            w(env.position_bundle),
            r(env.bundle_ata),
            w(bundled_position),
            w(seat),
            ws(nft.pubkey()),
            w(nft_ata),
            w(ta_lower),
            w(ta_upper),
            w(va),
            w(vb),
            r(env.quote_mint),
            r(spl_token::id()),
            r(TOKEN_2022),
            r(MEMO),
            r(system_program::id()),
            r(rent::id()),
            r(spl_associated_token_account::id()),
            r(WHIRLPOOL),
        ],
        data,
    );
    env.send(&[dep], &[user, &nft])?;
    Ok(SeatRefs { index, nft_mint: nft.pubkey(), nft_ata, seat, bundled_position, ta_lower, ta_upper })
}

fn exit(env: &mut Env, user: &Keypair, s: &SeatRefs) -> Result<litesvm::types::TransactionMetadata, String> {
    let (va, vb) = env.vaults_ab();
    let ex = ix(
        vec![
            ws(user.pubkey()),
            w(env.launch),
            w(s.seat),
            w(s.nft_mint),
            w(s.nft_ata),
            w(env.whirlpool),
            w(env.position_bundle),
            r(env.bundle_ata),
            w(s.bundled_position),
            w(env.reserve_vault),
            w(env.quote_vault),
            w(env.user_token(&user.pubkey())),
            w(env.user_quote(&user.pubkey())),
            w(va),
            w(vb),
            w(s.ta_lower),
            w(s.ta_upper),
            r(env.token_mint.pubkey()),
            r(env.quote_mint),
            r(spl_token::id()),
            r(TOKEN_2022),
            r(MEMO),
            r(WHIRLPOOL),
        ],
        vec![5u8],
    );
    env.send(&[ex], &[user])
}

fn collect_fees(env: &mut Env, user: &Keypair, s: &SeatRefs) -> Result<litesvm::types::TransactionMetadata, String> {
    let (va, vb) = env.vaults_ab();
    let cf = ix(
        vec![
            rs(user.pubkey()),
            w(env.launch),
            r(s.seat),
            r(s.nft_mint),
            r(s.nft_ata),
            w(env.whirlpool),
            r(env.bundle_ata),
            w(s.bundled_position),
            w(env.reserve_vault),
            w(env.quote_vault),
            w(env.user_token(&user.pubkey())),
            w(env.user_quote(&user.pubkey())),
            w(va),
            w(vb),
            r(s.ta_lower),
            r(s.ta_upper),
            r(env.token_mint.pubkey()),
            r(env.quote_mint),
            r(spl_token::id()),
            r(TOKEN_2022),
            r(MEMO),
            r(WHIRLPOOL),
        ],
        vec![6u8],
    );
    env.send(&[cf], &[user])
}

/// External buyer: quote in, tokens out.
fn buy(env: &mut Env, buyer: &Keypair, amount: u64) -> litesvm::types::TransactionMetadata {
    let tick = env.pool_tick();
    let s0 = tick_array_start(tick);
    let a_to_b = !env.token_is_a; // paying quote: if quote is A the swap is a->b
    let (dir, limit) = if a_to_b { (-1, MIN_SQRT_PRICE) } else { (1, MAX_SQRT_PRICE) };
    let mut data = vec![43u8, 4, 237, 11, 26, 201, 30, 98];
    data.extend(amount.to_le_bytes());
    data.extend(0u64.to_le_bytes());
    data.extend(limit.to_le_bytes());
    data.push(1); // amount_specified_is_input
    data.push(a_to_b as u8);
    data.push(0); // remaining_accounts_info None
    let (mint_a, mint_b, owner_a, owner_b, prog_a, prog_b) = if env.token_is_a {
        (env.token_mint.pubkey(), env.quote_mint, env.user_token(&buyer.pubkey()), env.user_quote(&buyer.pubkey()), TOKEN_2022, env.quote_program)
    } else {
        (env.quote_mint, env.token_mint.pubkey(), env.user_quote(&buyer.pubkey()), env.user_token(&buyer.pubkey()), env.quote_program, TOKEN_2022)
    };
    let (va, vb) = env.vaults_ab();
    let swap = Instruction {
        program_id: WHIRLPOOL,
        accounts: vec![
            r(prog_a),
            r(prog_b),
            r(MEMO),
            rs(buyer.pubkey()),
            w(env.whirlpool),
            r(mint_a),
            r(mint_b),
            w(owner_a),
            w(va),
            w(owner_b),
            w(vb),
            w(tick_array(&env.whirlpool, s0)),
            w(tick_array(&env.whirlpool, s0 + dir * TICKS_PER_ARRAY)),
            w(tick_array(&env.whirlpool, s0 + 2 * dir * TICKS_PER_ARRAY)),
            w(env.oracle),
        ],
        data,
    };
    env.must("swap", &[swap], &[buyer])
}

fn custom_err(e: &str, code: u32) -> bool {
    e.contains(&format!("Custom({code})"))
}

fn lifecycle(env: &mut Env) {
    let floor = seed_floor(env);
    assert!(env.svm.get_account(&floor).is_some());
    let reserve_after_floor = env.token_balance(&env.reserve_vault);
    let floor_take = SUPPLY - reserve_after_floor;
    assert!(floor_take <= SUPPLY / 20 && floor_take >= SUPPLY / 20 - 1_000, "floor takes 5% of the reserve, took {floor_take}");

    // ---- first presaler: 1 quote unit
    let alice = env.new_user(QUOTE_UNIT);
    let alice_quote = env.user_quote(&alice.pubkey());
    let seat_a = deposit(env, &alice, QUOTE_UNIT).expect("deposit");
    let seeded_a = reserve_after_floor - env.token_balance(&env.reserve_vault);
    let spent_a = QUOTE_UNIT - env.token_balance(&alice_quote);
    assert!(spent_a >= QUOTE_UNIT - 1_000_000, "position takes ~all the quote, spent {spent_a}");
    // 90/10 by value: seeded tokens are worth ~8.7x the quote at the pool price
    let value_ratio = seeded_a as f64 * env.token_price() / spent_a as f64;
    assert!((8.4..9.2).contains(&value_ratio), "seeded value / quote = {value_ratio}");
    assert_eq!(env.token_balance(&seat_a.nft_ata), 1);
    let nft = spl_token::state::Mint::unpack(&env.svm.get_account(&seat_a.nft_mint).unwrap().data).unwrap();
    assert!(nft.mint_authority.is_none() && nft.supply == 1);

    // ---- second presaler: 2 units at the same price
    let bob = env.new_user(2 * QUOTE_UNIT);
    let bob_quote = env.user_quote(&bob.pubkey());
    let seat_b = deposit(env, &bob, 2 * QUOTE_UNIT).expect("deposit 2");
    assert_eq!(seat_b.index, 1);
    let tick0 = env.pool_tick();

    // ---- exit too early
    let err = exit(env, &alice, &seat_a).unwrap_err();
    assert!(custom_err(&err, ladder_launch::error::LaunchError::SeatTooYoung as u32), "{err}");

    // ---- an outside buyer pushes the token price up
    let carol = env.new_user(3 * QUOTE_UNIT);
    let price0 = env.token_price();
    buy(env, &carol, QUOTE_UNIT * 3 / 2);
    assert!(env.token_price() > price0, "buy must raise the token price");
    assert_ne!(env.pool_tick(), tick0);
    assert!(env.token_balance(&env.user_token(&carol.pubkey())) > 0);

    // ---- fees accrued to alice's seat
    env.advance_time(61);
    let before = env.token_balance(&alice_quote);
    collect_fees(env, &alice, &seat_a).expect("collect");
    assert!(env.token_balance(&alice_quote) > before, "alice earned swap fees in quote");

    // ---- exit cap: alice (smaller) first, bob refused in the same minute, allowed next minute
    let reserve_before_exit = env.token_balance(&env.reserve_vault);
    exit(env, &alice, &seat_a).expect("alice exit");
    let alice_out = env.token_balance(&alice_quote);
    assert!(alice_out > QUOTE_UNIT, "quote side grew with the price: {alice_out}");
    assert_eq!(env.token_balance(&env.user_token(&alice.pubkey())), 0, "no tokens beyond the seed");
    assert!(env.token_balance(&env.reserve_vault) > reserve_before_exit, "unsold seed tokens return to the reserve");
    assert!(env.svm.get_account(&seat_a.seat).map(|a| a.data.is_empty()).unwrap_or(true), "seat closed");
    assert!(env.svm.get_account(&seat_a.bundled_position).map(|a| a.data.is_empty()).unwrap_or(true), "position closed");
    let err = exit(env, &bob, &seat_b).unwrap_err();
    assert!(custom_err(&err, ladder_launch::error::LaunchError::ExitCapReached as u32), "{err}");
    env.advance_time(60);
    exit(env, &bob, &seat_b).expect("bob exit next minute");
    assert!(env.token_balance(&bob_quote) > 0);

    // ---- reserve exhaustion: a whale deposit is capped at what is left and refunded the rest
    assert!(env.token_balance(&env.reserve_vault) > 0);
    let whale = env.new_user(2_000 * QUOTE_UNIT);
    deposit(env, &whale, 2_000 * QUOTE_UNIT).expect("whale");
    assert!(env.token_balance(&env.reserve_vault) <= 1_000, "reserve fully dispensed (dust only): {}", env.token_balance(&env.reserve_vault));
    assert!(env.token_balance(&env.user_quote(&whale.pubkey())) > 0, "unused quote refunded");
    let late = env.new_user(QUOTE_UNIT);
    let err = deposit(env, &late, QUOTE_UNIT).unwrap_err();
    assert!(custom_err(&err, ladder_launch::error::LaunchError::ReserveEmpty as u32), "{err}");
}

#[test]
fn lifecycle_wsol_token_is_a() {
    let mut env = setup(Quote::Wsol, true);
    lifecycle(&mut env);
}

#[test]
fn lifecycle_token2022_quote_token_is_b() {
    let mut env = setup(Quote::Token2022, false);
    lifecycle(&mut env);
}

#[test]
fn rejects_transfer_hook_quote() {
    let mut svm = base_svm();
    let creator = Keypair::new();
    svm.airdrop(&creator.pubkey(), 10_000_000_000).unwrap();
    // Token-2022 mint with a TransferHook extension (type 14) in its TLV area
    let mut data = packed_mint(6, None);
    data.resize(165, 0);
    data.push(1); // account type: Mint
    data.extend(14u16.to_le_bytes());
    data.extend(64u16.to_le_bytes());
    data.extend([0u8; 64]);
    let hooked = Keypair::new().pubkey();
    svm.set_account(hooked, Account { lamports: 1_000_000_000, data, owner: TOKEN_2022, executable: false, rent_epoch: 0 }).unwrap();
    let token_mint = Keypair::new();
    let launch = pda(&[b"launch", token_mint.pubkey().as_ref()], &program_id());
    let create = ix(
        vec![
            ws(creator.pubkey()),
            w(launch),
            ws(token_mint.pubkey()),
            w(ata_for(&launch, &token_mint.pubkey(), &TOKEN_2022)),
            r(hooked),
            w(ata_for(&launch, &hooked, &TOKEN_2022)),
            r(Keypair::new().pubkey()),
            r(spl_token::id()),
            r(TOKEN_2022),
            r(spl_associated_token_account::id()),
            r(system_program::id()),
            r(rent::id()),
        ],
        create_launch_args(),
    );
    let bh = svm.latest_blockhash();
    let tx = Transaction::new_signed_with_payer(&[create], Some(&creator.pubkey()), &[&creator, &token_mint], bh);
    let err = svm.send_transaction(tx).err().expect("must be rejected");
    assert!(format!("{:?}", err.err).contains(&format!("Custom({})", ladder_launch::error::LaunchError::UnsupportedQuoteMint as u32)), "{:?}", err.err);
}
