// Server-side data assembly for the page. Everything here is read from chain and returned
// as plain JSON so the endpoints can be cached at the CDN edge (near-instant loads).
import { Connection } from "@solana/web3.js";
import * as C from "../chain.js";

const LIVE = process.env.RPC_URL || "https://api.mainnet-beta.solana.com";
const HISTORY = process.env.HISTORY_RPC_URL || LIVE;
export const live = new Connection(LIVE, "confirmed");
export const hist = new Connection(HISTORY, "confirmed");

const meta = new Map(); // uri -> { image, description, at }
async function metaJson(uri) {
  if (!uri) return {};
  const hit = meta.get(uri);
  if (hit && Date.now() - hit.at < 600_000) return hit;
  try {
    const j = await (await fetch(uri, { signal: AbortSignal.timeout(4000) })).json();
    const v = { image: j.image || null, description: j.description || "", at: Date.now() };
    meta.set(uri, v);
    return v;
  } catch { return { image: null, description: "", at: Date.now() }; }
}
const quoteName = (l) => (l.quoteMint.equals(C.WSOL) ? "SOL" : l.quoteMint.equals(C.USDC) ? "USDC" : l.quoteMint.toBase58().slice(0, 4) + "…" + l.quoteMint.toBase58().slice(-4));

export async function enrich(launches) {
  const infos = await C.fetchMany(live, launches.flatMap((l) => [l.whirlpool, l.tokenMint, l.reserveVault]));
  return Promise.all(launches.map(async (l, i) => {
    const pool = l.flags & C.FLAG.POOL ? C.decodePool(infos[3 * i]) : null;
    const mint = C.decodeMint(infos[3 * i + 1]);
    const reserve = C.tokenAccountAmount(infos[3 * i + 2]);
    const dec = mint?.decimals ?? 6, supplyWhole = mint ? Number(mint.supply) / 10 ** dec : 0;
    const price = pool && mint ? C.tokenPrice(pool, l, dec) : 0;
    const m = mint?.metadata || {};
    const extra = await metaJson(m.uri);
    return {
      launch: C.ser(l), pool: C.ser(pool), mint: C.ser(mint), dec, supplyWhole, price, mcap: price * supplyWhole,
      reserveLeft: mint && mint.supply > 0n ? Number(reserve) / Number(mint.supply) : 0,
      meta: { name: m.name || l.tokenMint.toBase58().slice(0, 4) + "…" + l.tokenMint.toBase58().slice(-4), symbol: m.symbol || "", uri: m.uri || "", image: extra.image, description: extra.description },
      quote: quoteName(l), quoteIn: Number(l.quoteIn) / 10 ** l.quoteDecimals,
    };
  }));
}

export async function launches() {
  const rows = await enrich(await C.fetchLaunches(live));
  return { at: Date.now(), rows };
}

export async function launch(mint) {
  const l = await C.fetchLaunch(live, mint);
  if (!l) return null;
  const [row] = await enrich([l]);
  const pool = row.pool ? C.hydratePool(row.pool) : null;
  let seats = [], poolQuote = 0, poolTokens = 0, tape = [], history = [];
  const jobs = [
    C.fetchTape(hist, l, 25).then((t) => (tape = t)).catch(() => {}),
    pool ? C.fetchPriceHistory(hist, l, 100).then((h) => (history = h)).catch(() => {}) : null,
    pool ? (async () => {
      const raw = await C.fetchSeatsForLaunch(live, l.pubkey);
      const quoteVault = l.tokenIsA ? pool.vaultB : pool.vaultA, tokenVault = l.tokenIsA ? pool.vaultA : pool.vaultB;
      const infos = await C.fetchMany(live, [quoteVault, tokenVault, ...raw.map((s) => C.bundledPositionPda(s.bundleMint, s.bundleIndex))]);
      poolQuote = Number(C.tokenAccountAmount(infos[0])) / 10 ** l.quoteDecimals;
      poolTokens = Number(C.tokenAccountAmount(infos[1])) / 10 ** row.dec;
      seats = raw.map((s, i) => ({ ...C.ser(s), position: C.ser(C.decodePosition(infos[i + 2])) }));
    })() : null,
  ].filter(Boolean);
  await Promise.all(jobs);
  return { at: Date.now(), ...row, seats, poolQuote, poolTokens, tape, history: history.map((h) => ({ time: h.time, sqrtPrice: h.sqrtPrice.toString(), aToB: h.aToB, amountIn: h.amountIn, amountOut: h.amountOut })) };
}
