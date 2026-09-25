// JSON-RPC proxy with a short in-memory cache ("proxycache"). The browser never talks to
// the upstream RPC directly, so the key stays server side and repeated reads are absorbed.
// Set RPC_URL in the project's environment; the public mainnet endpoint is the fallback.
const UPSTREAM = process.env.RPC_URL || "https://api.mainnet-beta.solana.com";
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
    // Forward each call as a single JSON-RPC object: the public mainnet endpoint rejects
    // batched sendTransaction ("cannot unmarshal array"), so batching upstream is not safe.
    await Promise.all(forward.map(async (f) => {
      let r;
      try {
        const up = await fetch(UPSTREAM, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(f.call) });
        const text = await up.text();
        try { r = JSON.parse(text); } catch { r = { jsonrpc: "2.0", id: f.call.id, error: { code: -32000, message: `upstream ${up.status}: ${text.slice(0, 300)}` } }; }
        if (Array.isArray(r)) r = r[0];
        if (!r || typeof r !== "object") r = { jsonrpc: "2.0", id: f.call.id, error: { code: -32000, message: `upstream ${up.status}: ${text.slice(0, 300)}` } };
      } catch (e) {
        r = { jsonrpc: "2.0", id: f.call.id, error: { code: -32000, message: `upstream unreachable: ${e.message}` } };
      }
      r.id = f.call.id;
      results[f.i] = r;
      if (!r.error) store(f.key, TTL[f.call.method], r);
    }));
  }
  res.setHeader("content-type", "application/json");
  res.status(200).send(JSON.stringify(batch ? results : results[0]));
}
