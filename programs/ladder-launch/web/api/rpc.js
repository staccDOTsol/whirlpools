// JSON-RPC proxy with a short in-memory cache ("proxycache"). The browser never talks to
// the upstream RPC directly, so the key stays server side and repeated reads are absorbed.
// Set RPC_URL in the project's environment; the public mainnet endpoint is the fallback.
const UPSTREAM = process.env.RPC_URL || "https://api.mainnet-beta.solana.com";
// Signature history and transactions come from a separate upstream: FluxRPC's address index
// lags (0 signatures where the public RPC has 6), and these calls are cached hard anyway.
const HISTORY = process.env.HISTORY_RPC_URL || "https://api.mainnet-beta.solana.com";
// getTokenAccountsByOwner and getProgramAccounts also go there: the live endpoint returns
// malformed JSON for large token-account lists and takes 4 s per getProgramAccounts.
const HISTORY_METHODS = new Set(["getSignaturesForAddress", "getTransaction", "getTokenAccountsByOwner", "getProgramAccounts"]);
// Method allowlist with cache TTL in ms (0 = never cached).
const TTL = {
  getProgramAccounts: 4000, getAccountInfo: 2000, getMultipleAccounts: 2000, getTokenAccountsByOwner: 3000,
  getSignaturesForAddress: 4000, getTransaction: 3_600_000, getBalance: 2000, getMinimumBalanceForRentExemption: 3_600_000,
  getLatestBlockhash: 0, getBlockHeight: 0, sendTransaction: 0, getSignatureStatuses: 0, simulateTransaction: 0, getSlot: 0, getVersion: 60_000,
  getFeeForMessage: 0, getRecentPrioritizationFees: 5000, getTokenAccountBalance: 2000,
};
const cache = new Map();
const MAX_CACHE = 2000;

function cached(key, ttl) {
  if (!ttl) return null;
  const hit = cache.get(key);
  if (hit && hit.exp > Date.now()) return hit.value;
  if (hit) cache.delete(key);
  return null;
}
function store(key, ttl, value) {
  if (!ttl) return;
  if (cache.size >= MAX_CACHE) cache.delete(cache.keys().next().value);
  cache.set(key, { exp: Date.now() + ttl, value });
}

export default async function handler(req, res) {
  res.setHeader("Access-Control-Allow-Origin", "*");
  res.setHeader("Access-Control-Allow-Headers", "content-type");
  if (req.method === "OPTIONS") return res.status(204).end();
  if (req.method !== "POST") return res.status(405).json({ error: "POST only" });
  const body = typeof req.body === "string" ? JSON.parse(req.body) : req.body;
  const batch = Array.isArray(body);
  const calls = batch ? body : [body];
  const results = new Array(calls.length);
  const forward = [];
  calls.forEach((c, i) => {
    if (!c || !(c.method in TTL)) { results[i] = { jsonrpc: "2.0", id: c?.id ?? null, error: { code: -32601, message: `method not allowed: ${c?.method}` } }; return; }
    const key = JSON.stringify([c.method, c.params ?? []]);
    const hit = cached(key, TTL[c.method]);
    if (hit) { results[i] = { ...hit, id: c.id }; return; }
    forward.push({ i, key, call: c });
  });
  if (forward.length) {
    const errFor = (f, message) => ({ jsonrpc: "2.0", id: f.call.id, error: { code: -32000, message } });
    const settle = (f, r) => { r.id = f.call.id; results[f.i] = r; if (!r.error) store(f.key, TTL[f.call.method], r); };
    const post = async (url, body) => {
      for (let attempt = 0; attempt < 3; attempt++) {
        const up = await fetch(url, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) });
        const text = await up.text();
        let parsed = null;
        try { parsed = JSON.parse(text); } catch {}
        const limited = up.status === 429 || /rate limit/i.test(text);
        if (limited && attempt < 2) { await new Promise((r) => setTimeout(r, 400 * (attempt + 1))); continue; }
        return { status: up.status, text, parsed };
      }
    };
    // History (signatures, transactions) goes to HISTORY as ONE batch: that upstream indexes
    // history properly and counts connections, so one request beats many parallel ones.
    const hist = forward.filter((f) => HISTORY_METHODS.has(f.call.method));
    const live = forward.filter((f) => !HISTORY_METHODS.has(f.call.method));
    const jobs = [];
    if (hist.length) jobs.push((async () => {
      try {
        const { status, text, parsed } = await post(HISTORY, hist.map((f) => f.call));
        const arr = Array.isArray(parsed) ? parsed : parsed && typeof parsed === "object" ? [parsed] : [];
        const byId = new Map(arr.map((r) => [String(r.id), r]));
        hist.forEach((f, i) => settle(f, byId.get(String(f.call.id)) ?? (arr.length === hist.length ? arr[i] : null) ?? errFor(f, `upstream ${status}: ${text.slice(0, 200)}`)));
      } catch (e) { hist.forEach((f) => settle(f, errFor(f, `history upstream unreachable: ${e.message}`))); }
    })());
    // Live calls go to UPSTREAM one at a time: the public endpoint rejects batched sendTransaction.
    for (const f of live) jobs.push((async () => {
      try {
        const { status, text, parsed } = await post(UPSTREAM, f.call);
        let r = Array.isArray(parsed) ? parsed[0] : parsed;
        if (!r || typeof r !== "object") r = errFor(f, `upstream ${status}: ${text.slice(0, 200)}`);
        settle(f, r);
      } catch (e) { settle(f, errFor(f, `upstream unreachable: ${e.message}`)); }
    })());
    await Promise.all(jobs);
  }
  res.setHeader("content-type", "application/json");
  res.status(200).send(JSON.stringify(batch ? results : results[0]));
}
