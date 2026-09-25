// On-chain reads and instruction builders for ladder-launch. No mock data anywhere:
// every number on the page comes from an account or a transaction on mainnet.
import { PublicKey, TransactionInstruction, SystemProgram, SYSVAR_RENT_PUBKEY } from "@solana/web3.js";
import bs58 from "bs58";
import { Buffer } from "buffer";
export { bs58 };
// web3.js bundles its own Buffer; instruction data and key compares here need the global.
if (!globalThis.Buffer) globalThis.Buffer = Buffer;

export const PROGRAM = new PublicKey("6drxnwCC6coNFcB9vNAyCC7wZWLqJrSfoMGZ78G8eEkG");
export const WHIRLPOOL = new PublicKey("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");
export const CONFIG = new PublicKey("12yTE48QR6bGK4EMcyY8XsARbX1TRTEbwHYSuuxR1Hp8");
export const FEE_TIER = new PublicKey("6assHYd5438D91RXfMUrETNqGHmcbfzFCJsBjHmbkeu9");
export const FEE_TIER_INDEX = 1032;
export const TOKEN = new PublicKey("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
export const TOKEN_2022 = new PublicKey("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
export const ATA_PROGRAM = new PublicKey("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
export const MEMO = new PublicKey("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
export const WSOL = new PublicKey("So11111111111111111111111111111111111111112");
export const USDC = new PublicKey("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
export const METADATA_UPDATE_AUTH = new PublicKey("3axbTs2z5GBy6usVbNVoqEgZMng3vZvMnAoX29BFfwhr");
/// Protocol treasury: the launchpad config's fee authority.
export const TREASURY = new PublicKey("12Nqk2jyA3XNe3rPxAaLFixytmohnMrBfCsdwrCfWNm2");

export const TICK_SPACING = 128;
export const TICKS_PER_ARRAY = 88 * TICK_SPACING;
export const TOP_TICK = 443520;
export const BOTTOM_TICK = -443520;
export const LAUNCH_LEN = 408;
export const SEAT_LEN = 148;
export const FLAG = { POOL: 1, FLOOR: 2, BUNDLE: 4, TOKEN_IS_A: 8 };

// ---------- byte helpers ----------
const u16 = (d, o) => d[o] | (d[o + 1] << 8);
const i32 = (d, o) => new DataView(d.buffer, d.byteOffset).getInt32(o, true);
const u32 = (d, o) => new DataView(d.buffer, d.byteOffset).getUint32(o, true);
const u64 = (d, o) => new DataView(d.buffer, d.byteOffset).getBigUint64(o, true);
const i64 = (d, o) => new DataView(d.buffer, d.byteOffset).getBigInt64(o, true);
const u128 = (d, o) => u64(d, o) | (u64(d, o + 8) << 64n);
const key = (d, o) => new PublicKey(d.subarray(o, o + 32));
const leU16 = (n) => new Uint8Array([n & 0xff, (n >> 8) & 0xff]);
const leU32 = (n) => { const b = new Uint8Array(4); new DataView(b.buffer).setUint32(0, n >>> 0, true); return b; };
const leI32 = (n) => { const b = new Uint8Array(4); new DataView(b.buffer).setInt32(0, n, true); return b; };
const leU64 = (n) => { const b = new Uint8Array(8); new DataView(b.buffer).setBigUint64(0, BigInt(n), true); return b; };
const cat = (...parts) => { const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0)); let o = 0; for (const p of parts) { out.set(p, o); o += p.length; } return out; };

// ---------- decoders ----------
export function decodeLaunch(pubkey, d) {
  if (d.length < LAUNCH_LEN || d[0] !== 1) return null;
  return {
    pubkey, bump: d[1],
    tokenMint: key(d, 2), quoteMint: key(d, 34), whirlpool: key(d, 66), reserveVault: key(d, 98), quoteVault: key(d, 130),
    creator: key(d, 162), treasury: key(d, 194), floorPosition: key(d, 226), bundleMint: key(d, 258), quoteTokenProgram: key(d, 290),
    nextIndex: u16(d, 322), bundleCount: u16(d, 324), minAgeS: u32(d, 326), exitCapBps: u16(d, 330), lowerDeltaTicks: i32(d, 332),
    floorBps: u16(d, 336), creatorFeeBps: u16(d, 338), windowStart: i64(d, 340), windowLiquidity: u128(d, 348), totalLiquidity: u128(d, 364),
    tokensDispensed: u64(d, 380), quoteIn: u64(d, 388), seatsOpen: u32(d, 396), flags: d[400], quoteDecimals: d[401],
    get tokenIsA() { return (this.flags & FLAG.TOKEN_IS_A) !== 0; },
    get ready() { return (this.flags & (FLAG.POOL | FLAG.BUNDLE)) === (FLAG.POOL | FLAG.BUNDLE); },
  };
}
export function decodeSeat(pubkey, d) {
  if (d.length < SEAT_LEN || d[0] !== 2) return null;
  return {
    pubkey, launch: key(d, 2), bundleMint: key(d, 34), bundleIndex: u16(d, 66), nftMint: key(d, 68), entryTs: Number(i64(d, 100)),
    seededTokens: u64(d, 108), quoteIn: u64(d, 116), liquidity: u128(d, 124), tickLower: i32(d, 140), tickUpper: i32(d, 144),
  };
}
export function decodePool(d) {
  if (!d || d.length < 653) return null;
  return { tickSpacing: u16(d, 41), liquidity: u128(d, 49), sqrtPrice: u128(d, 65), tick: i32(d, 81), mintA: key(d, 101), vaultA: key(d, 133), mintB: key(d, 181), vaultB: key(d, 213) };
}
export function decodePosition(d) {
  if (!d || d.length < 216) return null;
  return { whirlpool: key(d, 8), mint: key(d, 40), liquidity: u128(d, 72), tickLower: i32(d, 88), tickUpper: i32(d, 92), feeOwedA: u64(d, 112), feeOwedB: u64(d, 136) };
}
export function decodeMint(d) {
  if (!d || d.length < 82) return null;
  return { supply: u64(d, 36), decimals: d[44], metadata: decodeMintMetadata(d) };
}
/// TokenMetadata extension (type 19) held in a Token-2022 mint: name, symbol, uri.
export function decodeMintMetadata(d) {
  if (d.length <= 166) return null;
  let o = 166;
  while (o + 4 <= d.length) {
    const type = u16(d, o), len = u16(d, o + 2);
    o += 4;
    if (type === 19) {
      const dec = new TextDecoder();
      let p = o + 64; // update_authority + mint
      const str = () => { const n = u32(d, p); p += 4; const v = dec.decode(d.subarray(p, p + n)); p += n; return v; };
      return { name: str(), symbol: str(), uri: str() };
    }
    if (type === 0) break;
    o += len;
  }
  return null;
}
export const tokenAccountAmount = (d) => (d && d.length >= 72 ? u64(d, 64) : 0n);

// ---------- math ----------
export const floorTs = (t, s = TICK_SPACING) => Math.floor(t / s) * s;
export const tickArrayStart = (t) => floorTs(t, TICKS_PER_ARRAY);
export const sqrtPriceX64ToFloat = (x) => Number(x) / 2 ** 64;
export const tickToSqrt = (t) => Math.sqrt(1.0001 ** t);
export const priceToTick = (p) => Math.log(p) / Math.log(1.0001);
/// Quote units per whole token at the pool's current price.
export function tokenPrice(pool, launch, tokenDecimals) {
  const s = sqrtPriceX64ToFloat(pool.sqrtPrice);
  const bPerA = s * s; // base units of B per base unit of A
  const quotePerTokenBase = launch.tokenIsA ? bPerA : 1 / bPerA;
  return quotePerTokenBase * 10 ** (tokenDecimals - launch.quoteDecimals);
}
/// Token amounts held by a concentrated position at sqrt price s (floats, display only).
export function positionAmounts(liquidity, tickLower, tickUpper, sqrtPrice) {
  const L = Number(liquidity), s = sqrtPriceX64ToFloat(sqrtPrice), sl = tickToSqrt(tickLower), su = tickToSqrt(tickUpper);
  let a = 0, b = 0;
  if (s <= sl) a = L * (su - sl) / (sl * su);
  else if (s >= su) b = L * (su - sl);
  else { a = L * (su - s) / (s * su); b = L * (s - sl); }
  return { a, b };
}
/// Whirlpool initial tick for an opening market cap (quote units) given supply and decimals.
export function initialTickFor(mcapQuote, supplyWhole, tokenDecimals, quoteDecimals, tokenIsA) {
  const quotePerTokenBase = (mcapQuote / supplyWhole) * 10 ** (quoteDecimals - tokenDecimals);
  const bPerA = tokenIsA ? quotePerTokenBase : 1 / quotePerTokenBase;
  return floorTs(Math.round(priceToTick(bPerA)));
}

// ---------- PDAs ----------
const pda = (seeds, program) => PublicKey.findProgramAddressSync(seeds, program)[0];
const enc = (s) => new TextEncoder().encode(s);
export const ata = (owner, mint, program = TOKEN) => pda([owner.toBytes(), program.toBytes(), mint.toBytes()], ATA_PROGRAM);
export const launchPda = (tokenMint) => pda([enc("launch"), tokenMint.toBytes()], PROGRAM);
export const seatPda = (launch, bundleMint, index) => pda([enc("seat"), launch.toBytes(), bundleMint.toBytes(), leU16(index)], PROGRAM);
export const whirlpoolPda = (mintA, mintB) => pda([enc("whirlpool"), CONFIG.toBytes(), mintA.toBytes(), mintB.toBytes(), leU16(FEE_TIER_INDEX)], WHIRLPOOL);
export const oraclePda = (pool) => pda([enc("oracle"), pool.toBytes()], WHIRLPOOL);
export const tokenBadgePda = (mint) => pda([enc("token_badge"), CONFIG.toBytes(), mint.toBytes()], WHIRLPOOL);
export const positionBundlePda = (bundleMint) => pda([enc("position_bundle"), bundleMint.toBytes()], WHIRLPOOL);
export const bundledPositionPda = (bundleMint, index) => pda([enc("bundled_position"), bundleMint.toBytes(), enc(String(index))], WHIRLPOOL);
export const positionPda = (mint) => pda([enc("position"), mint.toBytes()], WHIRLPOOL);
export const lockConfigPda = (position) => pda([enc("lock_config"), position.toBytes()], WHIRLPOOL);
export const tickArrayPda = (pool, start) => pda([enc("tick_array"), pool.toBytes(), enc(String(start))], WHIRLPOOL);
export const isTokenA = (tokenMint, quoteMint) => Buffer.from(tokenMint.toBytes()).compare(Buffer.from(quoteMint.toBytes())) < 0;

// ---------- account metas ----------
const w = (k) => ({ pubkey: k, isSigner: false, isWritable: true });
const r = (k) => ({ pubkey: k, isSigner: false, isWritable: false });
const ws = (k) => ({ pubkey: k, isSigner: true, isWritable: true });
const rs = (k) => ({ pubkey: k, isSigner: true, isWritable: false });
const ix = (keys, data) => new TransactionInstruction({ programId: PROGRAM, keys, data: Buffer.from(data) });
const vaultsAB = (launch, pool) => [pool.vaultA, pool.vaultB];

// ---------- token helpers ----------
export const createAtaIdempotent = (payer, owner, mint, program = TOKEN) =>
  new TransactionInstruction({ programId: ATA_PROGRAM, keys: [ws(payer), w(ata(owner, mint, program)), r(owner), r(mint), r(SystemProgram.programId), r(program)], data: Buffer.from([1]) });
export const syncNative = (account) => new TransactionInstruction({ programId: TOKEN, keys: [w(account)], data: Buffer.from([17]) });
export const closeAccount = (account, dest, owner) => new TransactionInstruction({ programId: TOKEN, keys: [w(account), w(dest), rs(owner)], data: Buffer.from([9]) });

// ---------- program instructions ----------
const strArg = (s) => { const b = new TextEncoder().encode(s); return cat(new Uint8Array([b.length]), b); };
export function createLaunchIx({ creator, launch, tokenMint, quoteMint, quoteProgram, decimals, supply, minAgeS, exitCapBps, lowerDeltaTicks, floorBps, creatorFeeBps, name, symbol, uri }) {
  const data = cat(new Uint8Array([0, decimals]), leU64(supply), leU32(minAgeS), leU16(exitCapBps), leI32(lowerDeltaTicks), leU16(floorBps), leU16(creatorFeeBps), strArg(name), strArg(symbol), strArg(uri));
  return ix([ws(creator), w(launch), ws(tokenMint), w(ata(launch, tokenMint, TOKEN_2022)), r(quoteMint), w(ata(launch, quoteMint, quoteProgram)), r(TREASURY),
    r(TOKEN), r(TOKEN_2022), r(ATA_PROGRAM), r(SystemProgram.programId), r(SYSVAR_RENT_PUBKEY)], data);
}
export function initPoolIx({ creator, launch, tokenMint, quoteMint, tokenIsA, vaultToken, vaultQuote, initialTick }) {
  const [mintA, mintB] = tokenIsA ? [tokenMint, quoteMint] : [quoteMint, tokenMint];
  const pool = whirlpoolPda(mintA, mintB);
  return { pool, ix: ix([ws(creator), w(launch), r(CONFIG), r(tokenMint), r(quoteMint), r(tokenBadgePda(tokenMint)), r(tokenBadgePda(quoteMint)), w(pool), w(oraclePda(pool)),
    ws(vaultToken), ws(vaultQuote), r(FEE_TIER), r(TOKEN), r(TOKEN_2022), r(SystemProgram.programId), r(SYSVAR_RENT_PUBKEY), r(WHIRLPOOL)], cat(new Uint8Array([1]), leI32(initialTick))) };
}
export function newBundleIx({ payer, launch, bundleMint }) {
  return ix([ws(payer), w(launch), w(positionBundlePda(bundleMint)), ws(bundleMint), w(ata(launch, bundleMint)), r(TOKEN), r(SystemProgram.programId), r(SYSVAR_RENT_PUBKEY), r(ATA_PROGRAM), r(WHIRLPOOL)], new Uint8Array([2]));
}
export function seedFloorIx({ creator, launch, pool, poolState, positionMint, tokenMint, quoteMint, tokenIsA }) {
  const position = positionPda(positionMint);
  const t = poolState.tick;
  const [lower, upper] = tokenIsA ? [floorTs(t) + TICK_SPACING, TOP_TICK] : [BOTTOM_TICK, floorTs(t)];
  return ix([ws(creator), w(launch), w(pool), w(position), ws(positionMint), w(ata(launch, positionMint, TOKEN_2022)), w(ata(launch, tokenMint, TOKEN_2022)), w(ata(launch, quoteMint, poolState.quoteProgram)),
    w(poolState.vaultA), w(poolState.vaultB), w(tickArrayPda(pool, tickArrayStart(lower))), w(tickArrayPda(pool, tickArrayStart(upper))), r(tokenMint), r(quoteMint), w(lockConfigPda(position)),
    r(METADATA_UPDATE_AUTH), r(TOKEN), r(TOKEN_2022), r(SystemProgram.programId), r(ATA_PROGRAM), r(MEMO), r(WHIRLPOOL)], new Uint8Array([3]));
}
export function depositIx({ user, launch, pool, nftMint, amount }) {
  const index = launch.nextIndex;
  const t = pool.tick;
  const [lower, upper] = launch.tokenIsA ? [floorTs(t + launch.lowerDeltaTicks), TOP_TICK] : [BOTTOM_TICK, floorTs(t - launch.lowerDeltaTicks)];
  const taLower = tickArrayPda(launch.whirlpool, tickArrayStart(lower)), taUpper = tickArrayPda(launch.whirlpool, tickArrayStart(upper));
  const seat = seatPda(launch.pubkey, launch.bundleMint, index);
  return { index, seat, ix: ix([ws(user), w(launch.pubkey), r(launch.tokenMint), w(launch.whirlpool), w(launch.reserveVault), w(launch.quoteVault), w(ata(user, launch.quoteMint, launch.quoteTokenProgram)),
    w(positionBundlePda(launch.bundleMint)), r(ata(launch.pubkey, launch.bundleMint)), w(bundledPositionPda(launch.bundleMint, index)), w(seat), ws(nftMint), w(ata(user, nftMint)),
    w(taLower), w(taUpper), w(pool.vaultA), w(pool.vaultB), r(launch.quoteMint), r(TOKEN), r(TOKEN_2022), r(MEMO), r(SystemProgram.programId), r(SYSVAR_RENT_PUBKEY), r(ATA_PROGRAM), r(WHIRLPOOL)],
    cat(new Uint8Array([4]), leU64(amount))) };
}
function seatRefs(user, launch, pool, seat) {
  return {
    va: pool.vaultA, vb: pool.vaultB,
    bundle: positionBundlePda(seat.bundleMint), bundleAta: ata(launch.pubkey, seat.bundleMint),
    bundled: bundledPositionPda(seat.bundleMint, seat.bundleIndex),
    taLower: tickArrayPda(launch.whirlpool, tickArrayStart(seat.tickLower)), taUpper: tickArrayPda(launch.whirlpool, tickArrayStart(seat.tickUpper)),
    userToken: ata(user, launch.tokenMint, TOKEN_2022), userQuote: ata(user, launch.quoteMint, launch.quoteTokenProgram), nftAta: ata(user, seat.nftMint),
  };
}
export function exitIx({ user, launch, pool, seat }) {
  const s = seatRefs(user, launch, pool, seat);
  return ix([ws(user), w(launch.pubkey), w(seat.pubkey), w(seat.nftMint), w(s.nftAta), w(launch.whirlpool), w(s.bundle), r(s.bundleAta), w(s.bundled), w(launch.reserveVault), w(launch.quoteVault),
    w(s.userToken), w(s.userQuote), w(s.va), w(s.vb), w(s.taLower), w(s.taUpper), r(launch.tokenMint), r(launch.quoteMint), r(TOKEN), r(launch.quoteTokenProgram), r(MEMO), r(WHIRLPOOL)], new Uint8Array([5]));
}
export function collectFeesIx({ user, launch, pool, seat }) {
  const s = seatRefs(user, launch, pool, seat);
  return ix([rs(user), w(launch.pubkey), r(seat.pubkey), r(seat.nftMint), r(s.nftAta), w(launch.whirlpool), r(s.bundleAta), w(s.bundled), w(launch.reserveVault), w(launch.quoteVault),
    w(s.userToken), w(s.userQuote), w(s.va), w(s.vb), r(s.taLower), r(s.taUpper), r(launch.tokenMint), r(launch.quoteMint), r(TOKEN), r(TOKEN_2022), r(MEMO), r(WHIRLPOOL)], new Uint8Array([6]));
}

// ---------- reads ----------
const b64 = (s) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
/// Seat preview for a quote amount (base units): tokens seeded from the reserve and the range.
export function seatPreview(launch, pool, quoteBase) {
  const s = sqrtPriceX64ToFloat(pool.sqrtPrice), t = pool.tick;
  if (launch.tokenIsA) {
    const sl = tickToSqrt(floorTs(t + launch.lowerDeltaTicks)), su = tickToSqrt(TOP_TICK);
    const L = quoteBase / (s - sl);
    return { tokens: L * (su - s) / (s * su), lowerPct: (sl * sl) / (s * s) - 1 };
  }
  const sl = tickToSqrt(BOTTOM_TICK), su = tickToSqrt(floorTs(t - launch.lowerDeltaTicks));
  const L = quoteBase * s * su / (su - s);
  return { tokens: L * (s - sl), lowerPct: (s * s) / (su * su) - 1 };
}
export async function rpc(conn, method, params) {
  // Raw JSON-RPC through the same proxy the Connection uses.
  return conn._rpcRequest(method, params).then((r) => { if (r.error) throw new Error(r.error.message); return r.result; });
}
export async function fetchLaunches(conn) {
  const res = await rpc(conn, "getProgramAccounts", [PROGRAM.toBase58(), { encoding: "base64", commitment: "confirmed", filters: [{ dataSize: LAUNCH_LEN }, { memcmp: { offset: 0, bytes: bs58.encode([1]) } }] }]);
  return res.map((a) => decodeLaunch(new PublicKey(a.pubkey), b64(a.account.data[0]))).filter(Boolean);
}
export async function fetchLaunch(conn, tokenMint) {
  const l = launchPda(tokenMint);
  const a = await conn.getAccountInfo(l, "confirmed");
  return a ? decodeLaunch(l, new Uint8Array(a.data)) : null;
}
export async function fetchSeatsForLaunch(conn, launch) {
  const res = await rpc(conn, "getProgramAccounts", [PROGRAM.toBase58(), { encoding: "base64", commitment: "confirmed", filters: [{ dataSize: SEAT_LEN }, { memcmp: { offset: 0, bytes: bs58.encode([2]) } }, { memcmp: { offset: 2, bytes: launch.toBase58() } }] }]);
  return res.map((a) => decodeSeat(new PublicKey(a.pubkey), b64(a.account.data[0]))).filter(Boolean);
}
export async function fetchMany(conn, keys) {
  if (!keys.length) return [];
  const out = [];
  for (let i = 0; i < keys.length; i += 100) {
    const infos = await conn.getMultipleAccountsInfo(keys.slice(i, i + 100), "confirmed");
    out.push(...infos.map((a) => (a ? new Uint8Array(a.data) : null)));
  }
  return out;
}
/// Mints of the user's SPL token accounts holding exactly one unit (seat NFT candidates).
export async function fetchNftMints(conn, owner) {
  const res = await rpc(conn, "getTokenAccountsByOwner", [owner.toBase58(), { programId: TOKEN.toBase58() }, { encoding: "jsonParsed", commitment: "confirmed" }]);
  const set = new Set();
  for (const a of res.value) { const info = a.account.data.parsed.info; if (info.tokenAmount.amount === "1" && info.tokenAmount.decimals === 0) set.add(info.mint); }
  return set;
}
/// Recent deposits and exits on a launch, parsed from its transaction history.
export async function fetchTape(conn, launch, limit = 25) {
  const sigs = await rpc(conn, "getSignaturesForAddress", [launch.pubkey.toBase58(), { limit, commitment: "confirmed" }]);
  if (!sigs.length) return [];
  const txs = (await conn._rpcBatchRequest(sigs.map((s) => ({ methodName: "getTransaction", args: [s.signature, { encoding: "json", commitment: "confirmed", maxSupportedTransactionVersion: 0 }] })))).map((r) => r.result);
  const out = [];
  txs.forEach((tx, i) => {
    if (!tx || tx.meta?.err) return;
    const keys = tx.transaction.message.accountKeys.map(String);
    const signer = keys[0];
    for (const ins of tx.transaction.message.instructions) {
      if (keys[ins.programIdIndex] !== PROGRAM.toBase58()) continue;
      const data = bs58.decode(ins.data);
      const tag = data[0];
      const time = sigs[i].blockTime;
      if (tag === 4) out.push({ kind: "deposit", who: signer, amount: Number(u64(data, 1)), time, sig: sigs[i].signature });
      else if (tag === 5) {
        // quote received = post - pre on the signer's quote account
        const delta = quoteDelta(tx.meta, keys, signer, launch.quoteMint.toBase58());
        out.push({ kind: "exit", who: signer, amount: delta, time, sig: sigs[i].signature });
      } else if (tag === 6) out.push({ kind: "collect", who: signer, amount: quoteDelta(tx.meta, keys, signer, launch.quoteMint.toBase58()), time, sig: sigs[i].signature });
    }
  });
  return out;
}
function quoteDelta(meta, keys, owner, mint) {
  const find = (arr) => arr.find((b) => b.mint === mint && b.owner === owner);
  const pre = find(meta.preTokenBalances || []), post = find(meta.postTokenBalances || []);
  return Number((post?.uiTokenAmount.amount ?? 0)) - Number((pre?.uiTokenAmount.amount ?? 0));
}

/// Whirlpool `Traded` events for the launch pool, parsed from its transaction history:
/// [{ time, sqrtPrice, aToB, amountIn, amountOut }] oldest first. Deposits don't trade, so
/// a launch with no swaps yet has an empty history.
const TRADED_DISC = [225, 202, 73, 175, 147, 43, 160, 150]; // sha256("event:Traded")[..8]
export async function fetchPriceHistory(conn, launch, limit = 100) {
  const sigs = await rpc(conn, "getSignaturesForAddress", [launch.whirlpool.toBase58(), { limit, commitment: "confirmed" }]);
  const ok = sigs.filter((s) => !s.err);
  if (!ok.length) return [];
  const txs = (await conn._rpcBatchRequest(ok.map((s) => ({ methodName: "getTransaction", args: [s.signature, { encoding: "json", commitment: "confirmed", maxSupportedTransactionVersion: 0 }] })))).map((r) => r.result);
  const out = [];
  txs.forEach((tx, i) => {
    if (!tx || tx.meta?.err) return;
    for (const l of tx.meta.logMessages || []) {
      if (!l.startsWith("Program data: ")) continue;
      const b = b64(l.slice(14));
      if (b.length < 121 || TRADED_DISC.some((x, k) => b[k] !== x)) continue;
      if (!new PublicKey(b.subarray(8, 40)).equals(launch.whirlpool)) continue;
      out.push({ time: ok[i].blockTime, sig: ok[i].signature, aToB: b[40] === 1, sqrtPrice: u128(b, 57), amountIn: Number(u64(b, 73)), amountOut: Number(u64(b, 81)) });
    }
  });
  return out.sort((a, b) => a.time - b.time);
}

// ---------- JSON transport (server endpoints -> browser) ----------
export function ser(v) {
  if (v == null) return v;
  if (typeof v === "bigint") return v.toString();
  if (v instanceof PublicKey) return v.toBase58();
  if (v instanceof Uint8Array) return Array.from(v);
  if (Array.isArray(v)) return v.map(ser);
  if (typeof v === "object") { const o = {}; for (const k of Object.keys(v)) o[k] = ser(v[k]); if ("tokenIsA" in v) o.tokenIsA = v.tokenIsA; if ("ready" in v) o.ready = v.ready; return o; }
  return v;
}
const PK = (x) => (x == null ? x : new PublicKey(x));
const BI = (x) => (x == null ? x : BigInt(x));
export function hydrateLaunch(j) {
  if (!j) return null;
  const l = { ...j };
  for (const k of ["pubkey", "tokenMint", "quoteMint", "whirlpool", "reserveVault", "quoteVault", "creator", "treasury", "floorPosition", "bundleMint", "quoteTokenProgram"]) l[k] = PK(j[k]);
  for (const k of ["windowStart", "windowLiquidity", "totalLiquidity", "tokensDispensed", "quoteIn"]) l[k] = BI(j[k]);
  return l;
}
export function hydratePool(j) {
  if (!j) return null;
  return { ...j, liquidity: BI(j.liquidity), sqrtPrice: BI(j.sqrtPrice), mintA: PK(j.mintA), vaultA: PK(j.vaultA), mintB: PK(j.mintB), vaultB: PK(j.vaultB) };
}
export function hydrateSeat(j) {
  return { ...j, pubkey: PK(j.pubkey), launch: PK(j.launch), bundleMint: PK(j.bundleMint), nftMint: PK(j.nftMint), seededTokens: BI(j.seededTokens), quoteIn: BI(j.quoteIn), liquidity: BI(j.liquidity) };
}
export function hydrateMint(j) { return j ? { ...j, supply: BI(j.supply) } : null; }
