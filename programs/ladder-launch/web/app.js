// Ladder Launch front end. Every figure on screen is read from mainnet through /api/rpc.
import { Connection, PublicKey, TransactionMessage, VersionedTransaction, Keypair, SystemProgram, ComputeBudgetProgram } from "@solana/web3.js";
import * as C from "./chain.js?v=1790356955";

// Live state and sends go straight to the RPC (RPC_URL, served by /api/config so the key is
// not in the repo). History (signatures, transactions) goes through /api/rpc, which caches.
const CFG = await fetch("/api/config").then((r) => r.json()).catch(() => ({}));
const LIVE_RPC = CFG.rpc || location.origin + "/api/rpc";
const conn = new Connection(LIVE_RPC, { commitment: "confirmed", disableRetryOnRateLimit: true, wsEndpoint: CFG.ws || undefined });
const histConn = new Connection(location.origin + "/api/rpc", { commitment: "confirmed", disableRetryOnRateLimit: true });
const app = document.getElementById("app");
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
const short = (k) => { const s = k.toString(); return s.slice(0, 4) + "…" + s.slice(-4); };
const ago = (ts) => { const d = Math.max(0, Date.now() / 1000 - ts); return d < 60 ? `${d | 0}s` : d < 3600 ? `${(d / 60) | 0}m` : d < 86400 ? `${(d / 3600) | 0}h` : `${(d / 86400) | 0}d`; };
const fmt = (n, dp = 2) => Number(n).toLocaleString(undefined, { maximumFractionDigits: dp });
const fmtTok = (n) => (n >= 1e9 ? (n / 1e9).toFixed(2) + "B" : n >= 1e6 ? (n / 1e6).toFixed(1) + "M" : n >= 1e3 ? (n / 1e3).toFixed(1) + "k" : n.toFixed(0));
const quoteName = (mint) => (mint.equals(C.WSOL) ? "SOL" : mint.equals(C.USDC) ? "USDC" : short(mint));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
document.getElementById("program-link").href = `https://solscan.io/account/${C.PROGRAM}`;

// ---------- wallet ----------
const wallet = {
  provider: null, pubkey: null, nfts: null,
  list() {
    const out = [], seen = new Set();
    const add = (name, p) => { if (p && !seen.has(p)) { seen.add(p); out.push({ name, p }); } };
    add("Phantom", window.phantom?.solana); add("Solflare", window.solflare); add("Backpack", window.backpack); add("Wallet", window.solana);
    return out;
  },
  async connect(entry) {
    const res = await entry.p.connect();
    this.provider = entry.p;
    this.pubkey = new PublicKey((res?.publicKey ?? entry.p.publicKey).toString());
    this.nfts = null;
    try { localStorage.setItem("ladder.wallet", entry.name); } catch {}
    entry.p.on?.("disconnect", () => this.disconnect());
    entry.p.on?.("accountChanged", (pk) => { if (pk) { this.pubkey = new PublicKey(pk.toString()); this.nfts = null; route(); } else this.disconnect(); });
    renderWalletButton(); route();
  },
  disconnect() { try { this.provider?.disconnect?.(); } catch {} this.provider = null; this.pubkey = null; this.nfts = null; try { localStorage.removeItem("ladder.wallet"); } catch {} renderWalletButton(); route(); },
  async signAll(txs) {
    if (this.provider.signAllTransactions) return this.provider.signAllTransactions(txs);
    const out = []; for (const t of txs) out.push(await this.provider.signTransaction(t)); return out;
  },
  async nftMints() { if (!this.pubkey) return new Set(); if (!this.nfts) this.nfts = await C.fetchNftMints(conn, this.pubkey); return this.nfts; },
};
function renderWalletButton() {
  const b = document.getElementById("wallet-btn");
  b.textContent = wallet.pubkey ? short(wallet.pubkey) : "Connect";
  b.onclick = () => (wallet.pubkey ? wallet.disconnect() : connectFlow());
}
async function connectFlow() {
  const list = wallet.list();
  if (list.length === 1) return wallet.connect(list[0]).catch(showError);
  const here = encodeURIComponent(location.href), origin = encodeURIComponent(location.origin);
  sheet(`
    <div class="display" style="font-size:18px;margin-bottom:12px">Connect a wallet</div>
    ${list.length ? list.map((w, i) => `<button class="btn solid" style="width:100%;margin-bottom:8px" data-w="${i}">${esc(w.name)}</button>`).join("") :
      `<div class="muted" style="margin-bottom:12px">No wallet found in this browser. Open this page inside a wallet app:</div>
       <a class="btn solid" style="width:100%;margin-bottom:8px" href="https://phantom.app/ul/browse/${here}?ref=${origin}">Open in Phantom</a>
       <a class="btn solid" style="width:100%;margin-bottom:8px" href="https://solflare.com/ul/v1/browse/${here}?ref=${origin}">Open in Solflare</a>`}
  `, (el) => el.querySelectorAll("[data-w]").forEach((b) => (b.onclick = () => { closeSheet(); wallet.connect(list[+b.dataset.w]).catch(showError); })));
}
function sheet(html, wire) {
  closeSheet();
  const s = document.createElement("div"); s.className = "sheet"; s.id = "sheet";
  s.innerHTML = `<div>${html}</div>`;
  s.onclick = (e) => { if (e.target === s) closeSheet(); };
  document.body.appendChild(s); wire?.(s);
}
const closeSheet = () => document.getElementById("sheet")?.remove();
function showError(e) {
  console.error("[ladder]", e);
  const logs = e?.logs || e?.transactionLogs || null;
  let msg = e?.message || String(e);
  if (e?.code != null) msg = `[${e.code}] ${msg}`;
  if (e?.transactionMessage) msg += `\n${e.transactionMessage}`;
  if (logs?.length) msg += `\n\nprogram logs:\n${logs.slice(-12).join("\n")}`;
  // Lives outside <main>, so in-place redraws never wipe it; cleared on route change or ×.
  const box = document.getElementById("errbox");
  box.hidden = false;
  box.innerHTML = `<div><button aria-label="dismiss">×</button><span></span></div>`;
  box.querySelector("span").textContent = msg;
  box.querySelector("button").onclick = () => { box.hidden = true; box.innerHTML = ""; };
}
const clearError = () => { const box = document.getElementById("errbox"); box.hidden = true; box.innerHTML = ""; };

// ---------- transactions ----------
const cu = (units, microLamports = 50_000) => [ComputeBudgetProgram.setComputeUnitLimit({ units }), ComputeBudgetProgram.setComputeUnitPrice({ microLamports })];
async function sendAll(list, onStep) {
  // v0 transactions. One wallet prompt for the whole set (signAllTransactions), then each
  // one is submitted in order and resubmitted with a small backoff until it lands.
  if (!wallet.pubkey) throw new Error("connect a wallet first");
  const { blockhash, lastValidBlockHeight } = await conn.getLatestBlockhash("confirmed");
  // The wallet signs untouched transactions first; the fresh keypairs (mints, vaults) add
  // their signatures afterwards. Pre-signed transactions made Phantom fail after approval.
  const txs = list.map(({ ixs }) => new VersionedTransaction(new TransactionMessage({ payerKey: wallet.pubkey, recentBlockhash: blockhash, instructions: ixs }).compileToV0Message()));
  // Simulate the first one so a program error shows real logs instead of the wallet's
  // generic failure. Later ones depend on it landing, so they can't be simulated yet.
  console.log("[ladder] simulating", txs.length, "tx");
  const sim = await conn.simulateTransaction(txs[0], { sigVerify: false, commitment: "confirmed" });
  if (sim.value.err) throw Object.assign(new Error(`simulation failed: ${JSON.stringify(sim.value.err)}`), { logs: sim.value.logs || [] });
  console.log("[ladder] requesting wallet signature via", wallet.provider?.isPhantom ? "Phantom" : wallet.provider?.constructor?.name || "provider");
  let signed;
  try { signed = await wallet.signAll(txs); }
  catch (e) {
    if (/reject|denied|cancel/i.test(e?.message || "")) throw e;
    console.warn("batch sign failed, retrying once", e);
    try { signed = await wallet.signAll(txs); }
    catch (e2) {
      const hint = e2?.code === -32603 || /unexpected error/i.test(e2?.message || "") ? " Usually the wallet is locked or its window was closed: unlock it and try again." : "";
      throw new Error(`wallet could not sign (${e2?.name || "error"}${e2?.code != null ? " " + e2.code : ""}: ${e2?.message || e2}).${hint} ${txs.length} v0 transaction${txs.length === 1 ? "" : "s"}, ${txs.map((t) => t.serialize().length).join("/")} bytes.`);
    }
  }
  signed = signed.map((t, i) => {
    const tx = VersionedTransaction.deserialize(t.serialize());
    const signers = list[i].signers || [];
    if (signers.length) tx.sign(signers);
    return tx;
  });
  const sigs = [];
  for (let i = 0; i < signed.length; i++) {
    onStep?.(i, signed.length);
    sigs.push(await land(signed[i], lastValidBlockHeight, i));
  }
  return sigs;
}
async function land(tx, lastValidBlockHeight, step) {
  const raw = tx.serialize();
  const sig = C.bs58.encode(tx.signatures[0]);
  let delay = 1200, first = true;
  for (;;) {
    try { await conn.sendRawTransaction(raw, { skipPreflight: !first, preflightCommitment: "confirmed", maxRetries: 0 }); }
    catch (e) {
      if (first) {
        // Preflight failed: attach the logs so the page shows what the program said.
        let logs = e?.logs || e?.transactionLogs;
        if (!logs?.length) { try { const sim = await conn.simulateTransaction(tx, { sigVerify: false, commitment: "confirmed" }); logs = sim.value.logs; } catch {} }
        throw Object.assign(new Error(`transaction ${step + 1} rejected: ${e?.transactionMessage || e?.message || e}`), { logs, code: e?.code });
      }
      // resubmits may race the landing; the status check decides
    }
    first = false;
    await sleep(delay);
    delay = Math.min(delay * 1.5, 4000);
    const { value: [st] } = await conn.getSignatureStatuses([sig]);
    if (st?.err) throw new Error(`transaction ${step + 1} failed: ${JSON.stringify(st.err)}\nhttps://solscan.io/tx/${sig}`);
    if (st && (st.confirmationStatus === "confirmed" || st.confirmationStatus === "finalized")) return sig;
    if ((await conn.getBlockHeight("confirmed")) > lastValidBlockHeight) throw new Error(`transaction ${step + 1} did not land before its blockhash expired. Steps before it landed; retry to continue.`);
  }
}

// ---------- data ----------
// Pages read /api/launches and /api/launch, which the server assembles from chain and the
// CDN caches for a few seconds, so loads are near-instant. `fresh` bypasses the cache after
// the user's own action so their seat shows up immediately.
const hydrateRow = (j) => ({ ...j, l: C.hydrateLaunch(j.launch), pool: C.hydratePool(j.pool), mint: C.hydrateMint(j.mint) });
let launchCache = { at: 0, rows: [] };
async function loadLaunches(fresh) {
  if (!fresh && Date.now() - launchCache.at < 5000) return launchCache.rows;
  const r = await fetch(`/api/launches${fresh ? `?_=${Date.now()}` : ""}`);
  const j = await r.json();
  if (!r.ok) throw new Error(j.error || "could not load launches");
  const rows = j.rows.map(hydrateRow);
  launchCache = { at: Date.now(), rows };
  const seats = rows.reduce((n, x) => n + x.l.seatsOpen, 0);
  document.getElementById("live").innerHTML = `<i></i>LIVE<b> · ${seats} seats open · ${rows.length} launch${rows.length === 1 ? "" : "es"}</b>`;
  return rows;
}
async function loadLaunch(mintStr, fresh) {
  const r = await fetch(`/api/launch?mint=${encodeURIComponent(mintStr)}${fresh ? `&_=${Date.now()}` : ""}`);
  if (r.status === 404) return null;
  const j = await r.json();
  if (!r.ok) throw new Error(j.error || "could not load launch");
  const row = hydrateRow(j);
  row.seats = j.seats.map((x) => ({ ...C.hydrateSeat(x), position: x.position ? { ...x.position, feeOwedA: BigInt(x.position.feeOwedA), feeOwedB: BigInt(x.position.feeOwedB) } : null }));
  row.history = j.history.map((h) => ({ ...h, sqrtPrice: BigInt(h.sqrtPrice) }));
  return row;
}
async function allSeats() {
  const res = await C.rpc(conn, "getProgramAccounts", [C.PROGRAM.toBase58(), { encoding: "base64", commitment: "confirmed", filters: [{ dataSize: C.SEAT_LEN }, { memcmp: { offset: 0, bytes: "3" } }] }]);
  return res.map((a) => C.decodeSeat(new PublicKey(a.pubkey), Uint8Array.from(atob(a.account.data[0]), (c) => c.charCodeAt(0)))).filter(Boolean);
}
const avatar = (r) => r.meta.image ? `<img class="avatar" data-img="${r.l.tokenMint}" src="${esc(r.meta.image)}" alt="">` : `<div class="avatar" data-img="${r.l.tokenMint}">${esc((r.meta.symbol || "?").slice(0, 3))}</div>`;

// ---------- explore ----------
let exploreTab = "hot", routeSeq = 0;
async function renderExplore() {
  const token = ++routeSeq;
  app.innerHTML = `<div class="empty">Reading launches from chain…</div>`;
  let rows;
  try { rows = await loadLaunches(); } catch (e) { return showError(e); }
  if (token !== routeSeq) return;
  let mine = new Set();
  if (exploreTab === "mine" && wallet.pubkey) {
    const nfts = await wallet.nftMints();
    for (const s of await allSeats()) if (nfts.has(s.nftMint.toBase58())) mine.add(s.launch.toBase58());
  }
  if (token !== routeSeq) return;
  const q = (document.getElementById("q")?.value || "").trim().toLowerCase();
  let list = rows.filter((r) => !q || r.meta.name.toLowerCase().includes(q) || r.meta.symbol.toLowerCase().includes(q) || r.l.tokenMint.toBase58().toLowerCase().startsWith(q));
  if (exploreTab === "hot") list.sort((a, b) => b.quoteIn - a.quoteIn);
  if (exploreTab === "mine") list = list.filter((r) => mine.has(r.l.pubkey.toBase58()));
  const maxIn = Math.max(1e-9, ...rows.map((r) => r.quoteIn));
  app.innerHTML = `
    <div style="display:flex;flex-wrap:wrap;gap:12px;align-items:center;justify-content:space-between">
      <div>
        <div class="display" style="font-size:28px">Explore</div>
        <div class="muted">Every launch is a Whirlpool. Deposit, get a seat, cash out any time.</div>
      </div>
      <div style="display:flex;gap:8px;align-items:center">
        <div class="tabs"><button data-tab="hot" class="${exploreTab === "hot" ? "on" : ""}">Hot</button><button data-tab="mine" class="${exploreTab === "mine" ? "on" : ""}">Mine</button></div>
        <input id="q" class="input" style="width:180px;height:40px" placeholder="name, symbol, mint" value="${esc(q)}">
      </div>
    </div>
    <div class="list">
      ${list.length ? list.map((r) => `
        <a class="row ${r.quoteIn >= maxIn * 0.5 && r.quoteIn > 0 ? "hot" : ""}" href="#/launch/${r.l.tokenMint}">
          ${avatar(r)}
          <div style="flex:1;min-width:0">
            <div style="display:flex;gap:8px;align-items:baseline"><b class="display" style="font-size:16px">${esc(r.meta.symbol || short(r.l.tokenMint))}</b><span class="muted" style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${esc(r.meta.name)}</span>${r.l.ready ? "" : `<span class="heat" style="font-size:11px">setting up</span>`}</div>
            <div class="muted" style="font-size:12px">${r.l.seatsOpen} seats · ${(r.reserveLeft * 100).toFixed(0)}% reserve left · ${esc(r.quote)}</div>
            <div class="bar heat" style="margin-top:6px"><i style="width:${(100 * r.quoteIn / maxIn).toFixed(0)}%"></i></div>
          </div>
          <div style="text-align:right">
            <div class="mono" style="font-size:18px">${r.pool ? fmt(r.mcap, r.mcap < 10 ? 3 : 1) : "—"} <span class="muted" style="font-size:12px">${esc(r.quote)}</span></div>
            <div class="muted" style="font-size:12px">mcap · ${fmt(r.quoteIn, 2)} ${esc(r.quote)} in</div>
          </div>
        </a>`).join("") :
        `<div class="empty">${exploreTab === "mine" ? (wallet.pubkey ? "No seats in this wallet yet." : "Connect a wallet to see your seats.") : rows.length ? "Nothing matches." : "No launches on chain yet. Be the first: <a href='#/create'>launch a token</a>."}</div>`}
    </div>`;
  app.querySelectorAll("[data-tab]").forEach((b) => (b.onclick = () => { exploreTab = b.dataset.tab; renderExplore(); }));
  const qEl = document.getElementById("q");
  if (qEl) qEl.oninput = () => renderExplore();
}

// ---------- in-place rendering ----------
// Periodic refreshes patch the existing DOM instead of replacing it, so nothing flashes and
// a focused input is never clobbered. Handlers are re-attached by the caller after each pass.
function morph(from, to) {
  if (from.nodeType === Node.TEXT_NODE && to.nodeType === Node.TEXT_NODE) { if (from.nodeValue !== to.nodeValue) from.nodeValue = to.nodeValue; return; }
  if (from.nodeType !== to.nodeType || from.nodeName !== to.nodeName) { from.replaceWith(to); return; }
  if (from.nodeType !== Node.ELEMENT_NODE) return;
  for (const a of Array.from(from.attributes)) if (!to.hasAttribute(a.name)) from.removeAttribute(a.name);
  for (const a of Array.from(to.attributes)) if (from.getAttribute(a.name) !== a.value) from.setAttribute(a.name, a.value);
  if ((from.tagName === "INPUT" || from.tagName === "TEXTAREA") && document.activeElement === from) return;
  const fc = Array.from(from.childNodes), tc = Array.from(to.childNodes);
  for (let i = 0; i < Math.max(fc.length, tc.length); i++) {
    if (!tc[i]) fc[i].remove();
    else if (!fc[i]) from.appendChild(tc[i]);
    else morph(fc[i], tc[i]);
  }
}
function patch(root, html) {
  const tmp = document.createElement("div");
  tmp.innerHTML = html;
  const fc = Array.from(root.childNodes), tc = Array.from(tmp.childNodes);
  for (let i = 0; i < Math.max(fc.length, tc.length); i++) {
    if (!tc[i]) fc[i].remove();
    else if (!fc[i]) root.appendChild(tc[i]);
    else morph(fc[i], tc[i]);
  }
}

// ---------- chart ----------
// Market cap over time from the pool's own Traded events, plus the live point. No embed.
function priceChart(series, quote, trades) {
  const W = 800, H = 300, L = 8, R = 8, T = 18, B = 26;
  const xs = series.map((p) => p[0]), ys = series.map((p) => p[1]);
  const x0 = Math.min(...xs), x1 = Math.max(...xs), span = Math.max(1, x1 - x0);
  let y0 = Math.min(...ys), y1 = Math.max(...ys);
  if (y1 - y0 < y1 * 0.02) { y0 = y0 * 0.98; y1 = y1 * 1.02; }
  const X = (t) => L + ((t - x0) / span) * (W - L - R), Y = (v) => H - B - ((v - y0) / (y1 - y0 || 1)) * (H - T - B);
  const pts = series.length === 1 ? [[L, Y(ys[0])], [W - R, Y(ys[0])]] : series.map((p) => [X(p[0]), Y(p[1])]);
  const d = pts.map((p, i) => (i ? "L" : "M") + p[0].toFixed(1) + " " + p[1].toFixed(1)).join(" ");
  const area = `${d} L${pts[pts.length - 1][0].toFixed(1)} ${H - B} L${pts[0][0].toFixed(1)} ${H - B}Z`;
  const up = ys[ys.length - 1] >= ys[0];
  const col = up ? "var(--accent)" : "var(--exit)";
  const lab = (v) => fmt(v, v < 10 ? 3 : 1);
  return `<svg viewBox="0 0 ${W} ${H}" preserveAspectRatio="none" style="position:absolute;inset:0;width:100%;height:100%">
      <defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${col}" stop-opacity=".25"/><stop offset="1" stop-color="${col}" stop-opacity="0"/></linearGradient></defs>
      <path d="${area}" fill="url(#g)"/><path d="${d}" fill="none" stroke="${col}" stroke-width="2" vector-effect="non-scaling-stroke"/>
      <circle cx="${pts[pts.length - 1][0].toFixed(1)}" cy="${pts[pts.length - 1][1].toFixed(1)}" r="4" fill="${col}" vector-effect="non-scaling-stroke"/>
    </svg>
    <div class="mono muted" style="position:absolute;top:8px;left:12px;font-size:12px">${lab(y1)} ${esc(quote)}</div>
    <div class="mono muted" style="position:absolute;bottom:8px;left:12px;font-size:12px">${lab(y0)} ${esc(quote)}</div>
    <div class="mono muted" style="position:absolute;bottom:8px;right:12px;font-size:12px">${trades ? `${trades} trade${trades === 1 ? "" : "s"} · since ${ago(series[0][0])} ago` : "no trades yet · price set by the opening tick"}</div>`;
}

// ---------- launch page ----------
let pageTimer = null, subs = [];
function unsubscribeAll() { for (const id of subs) conn.removeAccountChangeListener(id).catch(() => {}); subs = []; }
async function renderLaunch(mintStr) {
  const token = ++routeSeq;
  clearInterval(pageTimer); unsubscribeAll();
  let tokenMint;
  try { tokenMint = new PublicKey(mintStr); } catch { app.innerHTML = `<div class="empty">Bad mint address.</div>`; return; }
  app.innerHTML = `<div class="empty">Reading launch from chain…</div>`;
  // livePool / livePoints come from the websocket and override the cached snapshot until it catches up.
  const state = { armed: null, flash: null, busy: false, livePool: null, livePoints: [], fresh: false };
  let drawing = false, again = false;
  const draw = async () => {
    if (drawing) { again = true; return; }
    drawing = true;
    try { await drawOnce(); } finally { drawing = false; if (again) { again = false; draw(); } }
  };
  const drawOnce = async () => {
    let r;
    try { r = await loadLaunch(mintStr, state.fresh); } catch (e) { return showError(e); }
    if (token !== routeSeq) return;
    state.fresh = false;
    if (!r) { app.innerHTML = `<div class="empty">No launch for this mint.</div>`; return; }
    const l = r.l;
    if (state.livePool) {
      // The socket saw a newer pool state than the cached snapshot: use it for price and cap.
      r.pool = state.livePool;
      r.price = C.tokenPrice(r.pool, l, r.dec);
      r.mcap = r.price * r.supplyWhole;
    }
    const pool = r.pool;
    const now = Date.now() / 1000;
    const inWindow = now - Number(l.windowStart) < 60;
    const cap = Number(l.totalLiquidity) * l.exitCapBps / 10_000;
    const room = cap > 0 ? Math.max(0, 1 - (inWindow ? Number(l.windowLiquidity) : 0) / cap) : 1;
    const qdec = l.quoteDecimals, quote = r.quote;
    const isSol = l.quoteMint.equals(C.WSOL);
    const tiles = isSol ? [0.5, 1, 5] : [10, 50, 250];
    const preview = (amt) => pool ? C.seatPreview(l, pool, amt * 10 ** qdec).tokens / 10 ** r.dec : 0;
    const tape = r.tape, history = r.history;
    const mcapAt = (sqrtPrice) => C.tokenPrice({ sqrtPrice }, l, r.dec) * r.supplyWhole;
    const series = history.map((h) => [h.time, mcapAt(h.sqrtPrice)]);
    const lastHist = series.length ? series[series.length - 1][0] : 0;
    for (const [t, sp] of state.livePoints) if (t > lastHist) series.push([t, mcapAt(sp)]);
    series.push([now, r.mcap]);
    let allSeats = [], mySeats = [];
    const poolQuote = r.poolQuote, poolTokens = r.poolTokens;
    if (pool) {
      const nfts = wallet.pubkey ? await wallet.nftMints() : new Set();
      if (token !== routeSeq) return;
      allSeats = r.seats.map((s) => {
        const p = s.position;
        const { a, b } = C.positionAmounts(s.liquidity, s.tickLower, s.tickUpper, pool.sqrtPrice);
        const tokens = l.tokenIsA ? a : b, quoteSide = l.tokenIsA ? b : a;
        const extraTokens = Math.max(0, tokens - Number(s.seededTokens));
        const nowQuote = (quoteSide + extraTokens * r.price * 10 ** (qdec - r.dec)) / 10 ** qdec;
        const inQuote = Number(s.quoteIn) / 10 ** qdec;
        const feesQuote = p ? Number(l.tokenIsA ? p.feeOwedB : p.feeOwedA) / 10 ** qdec : 0;
        return { s, p, nowQuote, inQuote, pct: inQuote > 0 ? (nowQuote / inQuote - 1) * 100 : 0, feesQuote, age: now - s.entryTs, young: now - s.entryTs < l.minAgeS, mine: nfts.has(s.nftMint.toBase58()), tokens: tokens / 10 ** r.dec };
      });
      mySeats = allSeats.filter((m) => m.mine).sort((x, y) => y.s.entryTs - x.s.entryTs);
    }
    const bySize = allSeats.slice().sort((x, y) => y.inQuote - x.inQuote);
    const seatedQuote = allSeats.reduce((n, m) => n + m.inQuote, 0), seatedNow = allSeats.reduce((n, m) => n + m.nowQuote, 0);
    const dispensedPct = r.mint && r.mint.supply > 0n ? Number(l.tokensDispensed) / Number(r.mint.supply) * 100 : 0;
    const f = state.flash;
    patch(app, `
      <div style="display:flex;gap:12px;align-items:center;margin-bottom:14px">
        ${avatar(r)}
        <div style="min-width:0">
          <div style="display:flex;gap:10px;align-items:baseline;flex-wrap:wrap"><span class="display" style="font-size:28px">${esc(r.meta.symbol || short(l.tokenMint))}</span><span class="muted">${esc(r.meta.name)}</span>
            ${r.quoteIn > 0 ? `<span class="heat display" style="font-size:12px;padding:4px 8px;border-radius:6px;background:#2a1a10">${fmt(r.quoteIn)} ${esc(quote)} IN</span>` : ""}</div>
          <div class="muted" style="font-size:13px">${esc(quote)} quote · ${l.seatsOpen} seats · ${(r.reserveLeft * 100).toFixed(0)}% reserve left · mint <a class="mono" href="https://solscan.io/token/${l.tokenMint}" target="_blank" rel="noopener">${short(l.tokenMint)}</a>${pool ? ` · pool <a class="mono" href="https://solscan.io/account/${l.whirlpool}" target="_blank" rel="noopener">${short(l.whirlpool)}</a>` : ""}</div>
        </div>
      </div>
      ${l.ready ? "" : `<div class="panel heat" style="margin-bottom:14px">This launch is still being set up (pool ${l.flags & C.FLAG.POOL ? "ready" : "missing"}, bundle ${l.flags & C.FLAG.BUNDLE ? "ready" : "missing"}). Seats open once both exist.</div>`}
      <div class="grid">
        <div style="display:flex;flex-direction:column;gap:14px;min-width:0">
          <div style="display:flex;align-items:flex-end;justify-content:space-between;gap:16px;flex-wrap:wrap">
            <div><div class="muted" style="font-size:12px">market cap</div><div class="big">${pool ? fmt(r.mcap, r.mcap < 10 ? 3 : 1) : "—"} <span class="muted" style="font-size:16px">${esc(quote)}</span></div></div>
            <div style="text-align:right"><div class="muted" style="font-size:12px">price</div><div class="mono" style="font-size:18px">${pool ? r.price.toPrecision(4) : "—"} <span class="muted" style="font-size:12px">${esc(quote)}</span></div></div>
          </div>
          <div class="chart">${pool ? priceChart(series, quote, history.length) : `<div class="empty">No pool yet.</div>`}</div>
          <div class="tape">${tape.length ? tape.map((t) => `<span><b class="${t.kind === "deposit" ? "accent" : t.kind === "exit" ? "exit" : "muted"}">${t.kind === "deposit" ? "+" : t.kind === "exit" ? "out " : "fees "}${fmt(t.amount / 10 ** qdec, 3)}</b><span class="muted">${short(t.who)} · ${ago(t.time)}</span></span>`).join("") : `<span class="muted">No seats yet. First one sets the tape.</span>`}</div>
          <div class="stats">
            <div class="panel"><div class="muted" style="font-size:12px">seats open</div><div class="mono" style="font-size:20px">${l.seatsOpen}</div><div class="muted" style="font-size:12px">${l.nextIndex} taken all time</div></div>
            <div class="panel"><div class="muted" style="font-size:12px">seated ${esc(quote)}</div><div class="mono" style="font-size:20px">${fmt(seatedQuote, 2)}</div><div class="muted" style="font-size:12px">worth ${fmt(seatedNow, 2)} now · ${fmt(r.quoteIn, 2)} in all time</div></div>
            <div class="panel"><div class="muted" style="font-size:12px">in the pool</div><div class="mono" style="font-size:20px">${fmt(poolQuote, 2)} <span class="muted" style="font-size:12px">${esc(quote)}</span></div><div class="muted" style="font-size:12px">${fmtTok(poolTokens)} ${esc(r.meta.symbol)} on the ask</div></div>
            <div class="panel"><div class="muted" style="font-size:12px">reserve left</div><div class="mono" style="font-size:20px">${(r.reserveLeft * 100).toFixed(1)}%</div><div class="bar" style="margin-top:6px"><i style="width:${(r.reserveLeft * 100).toFixed(1)}%"></i></div><div class="muted" style="font-size:12px;margin-top:4px">${dispensedPct.toFixed(1)}% dispensed · floor ${(l.floorBps / 100).toFixed(1)}%${l.flags & C.FLAG.FLOOR ? " locked" : " not seeded"}</div></div>
            <div class="panel"><div class="muted" style="font-size:12px">exit room this minute</div><div class="mono" style="font-size:20px">${(room * 100).toFixed(0)}%</div><div class="bar" style="margin-top:6px"><i style="width:${(room * 100).toFixed(0)}%;background:var(--exit)"></i></div><div class="muted" style="font-size:12px;margin-top:4px">${inWindow ? `reset in ${Math.max(0, 60 - (now - Number(l.windowStart))) | 0}s` : `${l.exitCapBps / 100}% of the book per minute`}</div></div>
          </div>
          <div class="panel">
            <div style="display:flex;justify-content:space-between;align-items:baseline"><span class="display" style="font-size:16px">All seats</span><span class="muted" style="font-size:12px">${allSeats.length} open · by size</span></div>
            ${bySize.length ? `<div class="seatlist">${bySize.slice(0, 30).map((m) => `
              <div class="seatrow ${m.mine ? "mine" : ""}">
                <span class="mono muted">#${m.s.bundleIndex}${m.mine ? " · you" : ""}</span>
                <span class="mono">${fmt(m.inQuote, 2)} → ${fmt(m.nowQuote, 2)}</span>
                <span class="mono ${m.pct >= 0 ? "accent" : "exit"}">${m.pct >= 0 ? "+" : ""}${m.pct.toFixed(0)}%</span>
                <span class="mono muted">${fmtTok(m.tokens)} ${esc(r.meta.symbol)}</span>
                <span class="mono muted">${ago(m.s.entryTs)}</span>
              </div>`).join("")}${bySize.length > 30 ? `<div class="muted" style="font-size:12px;padding-top:6px">and ${bySize.length - 30} more</div>` : ""}</div>` : `<div class="muted" style="font-size:13px;margin-top:8px">No open seats.</div>`}
          </div>
        </div>
        <div style="display:flex;flex-direction:column;gap:14px">
          <div class="panel">
            <div style="display:flex;justify-content:space-between;align-items:baseline"><span class="display" style="font-size:16px">Take a seat</span><span class="muted" style="font-size:12px">one tap, no confirm</span></div>
            ${f ? `<div class="flash" style="margin-top:12px"><div class="display accent" style="font-size:22px">SEATED · #${f.index}</div><div class="muted" style="font-size:12px">${fmt(f.amount, 3)} ${esc(quote)} in · <a href="https://solscan.io/tx/${f.sig}" target="_blank" rel="noopener">tx</a></div></div>` : ""}
            <div class="tiles" style="margin-top:12px">
              ${tiles.map((amt, i) => `<button class="tile ${i === 1 ? "main" : ""}" data-amt="${amt}" ${!l.ready || state.busy ? "disabled" : ""}><b>${amt}</b><small>${esc(quote)}</small><small>${pool ? fmtTok(preview(amt)) + " " + esc(r.meta.symbol) : ""}</small></button>`).join("")}
            </div>
            <div style="display:flex;gap:8px;margin-top:10px"><input id="custom" class="input mono" placeholder="custom ${esc(quote)}" inputmode="decimal"><button class="btn solid" id="custom-go" ${!l.ready || state.busy ? "disabled" : ""}>Seat</button></div>
            <div class="muted" style="font-size:12px;margin-top:10px">Your ${esc(quote)} is paired with tokens from the reserve into a ~90/10 position (lower bound ${pool ? (C.seatPreview(l, pool, 1).lowerPct * 100).toFixed(0) : "—"}% under spot, open top). Cash out any time after ${l.minAgeS}s; fees are yours, unsold seed goes back.</div>
            ${state.busy ? `<div class="accent" style="margin-top:8px">${esc(state.busy)}</div>` : ""}
          </div>
          <div class="panel">
            <div style="display:flex;justify-content:space-between;align-items:baseline"><span class="display" style="font-size:16px">Your seats</span><span class="muted" style="font-size:12px">${wallet.pubkey ? `${mySeats.length} here` : "connect to see"}</span></div>
            <div style="display:flex;flex-direction:column;gap:10px;margin-top:12px">
              ${mySeats.map((m) => `
                <div class="seat">
                  <div style="display:flex;justify-content:space-between;align-items:baseline">
                    <span class="gain ${m.pct >= 0 ? "accent" : "exit"}">${m.pct >= 0 ? "+" : ""}${m.pct.toFixed(0)}%</span>
                    <span class="mono muted" style="font-size:12px">#${m.s.bundleIndex} · ${fmt(m.inQuote, 3)} → ${fmt(m.nowQuote, 3)} ${esc(quote)} · ${ago(m.s.entryTs)}</span>
                  </div>
                  <div class="actions">
                    <button class="btn solid" data-collect="${m.s.pubkey}" ${state.busy ? "disabled" : ""}>Collect${m.feesQuote > 0 ? ` +${fmt(m.feesQuote, 4)}` : ""}</button>
                    <button class="btn danger" data-exit="${m.s.pubkey}" ${state.busy || m.young ? "disabled" : ""}>${m.young ? `wait ${Math.max(0, l.minAgeS - m.age) | 0}s` : state.armed === m.s.pubkey.toBase58() ? `Tap again · ${fmt(m.nowQuote, 3)}` : `Cash out ${fmt(m.nowQuote, 3)}`}</button>
                  </div>
                  ${state.armed === m.s.pubkey.toBase58() ? `<div class="muted" style="font-size:12px">Second tap is final. Burns the seat.</div>` : ""}
                </div>`).join("") || `<div class="muted" style="font-size:13px">${wallet.pubkey ? "No seats in this launch yet." : ""}</div>`}
            </div>
          </div>
        </div>
      </div>`);
    const seatsById = new Map(mySeats.map((m) => [m.s.pubkey.toBase58(), m.s]));
    const run = async (label, fn) => {
      console.log("[ladder]", label);
      clearError();
      state.busy = label; state.armed = null;
      try { await draw(); } catch (e) { console.warn("[ladder] pre-action redraw failed", e); }
      try { await fn(); } catch (e) { showError(e); }
      state.busy = false; state.fresh = true; launchCache.at = 0; wallet.nfts = null;
      try { await draw(); } catch (e) { console.warn("[ladder] post-action redraw failed", e); }
    };
    app.querySelectorAll("[data-amt]").forEach((b) => (b.onclick = () => seat(+b.dataset.amt)));
    document.getElementById("custom-go").onclick = () => { const v = parseFloat(document.getElementById("custom").value); if (v > 0) seat(v); };
    const seat = (amt) => { if (!wallet.pubkey) return connectFlow(); run(`Seating ${amt} ${quote}…`, async () => { const res = await deposit(l, pool, amt); state.flash = { ...res, amount: amt }; }); };
    app.querySelectorAll("[data-exit]").forEach((b) => (b.onclick = () => { const id = b.dataset.exit; if (state.armed !== id) { state.armed = id; draw(); return; } run("Cashing out…", () => exitSeat(l, pool, seatsById.get(id))); }));
    app.querySelectorAll("[data-collect]").forEach((b) => (b.onclick = () => run("Collecting fees…", () => collect(l, pool, seatsById.get(b.dataset.collect)))));
  };
  await draw();
  // Realtime: pool account changes (swaps, liquidity) and launch account changes (seats,
  // exits) push a redraw; the interval is only a fallback if the socket is unavailable.
  try {
    const l0 = await loadLaunch(mintStr, false);
    if (l0?.l) {
      let lastSqrt = l0.pool?.sqrtPrice ?? null;
      subs.push(conn.onAccountChange(l0.l.whirlpool, (acc) => {
        const p = C.decodePool(new Uint8Array(acc.data));
        if (!p) return;
        state.livePool = p;
        if (lastSqrt == null || p.sqrtPrice !== lastSqrt) { state.livePoints.push([Date.now() / 1000, p.sqrtPrice]); lastSqrt = p.sqrtPrice; }
        state.fresh = true;
        if (!state.busy && !state.armed) draw();
      }, "confirmed"));
      subs.push(conn.onAccountChange(l0.l.pubkey, () => { state.fresh = true; if (!state.busy && !state.armed) draw(); }, "confirmed"));
    }
  } catch (e) { console.warn("live subscription unavailable, polling instead", e); }
  pageTimer = setInterval(() => { if (!state.busy && !state.armed && !document.hidden && location.hash.startsWith(`#/launch/${mintStr}`)) draw(); }, subs.length ? 30_000 : 10_000);
}
async function deposit(l, pool, amt) {
  const user = wallet.pubkey, nft = Keypair.generate();
  const amountBase = BigInt(Math.round(amt * 10 ** l.quoteDecimals));
  const freshPool = C.decodePool(new Uint8Array((await conn.getAccountInfo(l.whirlpool, "confirmed")).data));
  const { ix, index } = C.depositIx({ user, launch: l, pool: freshPool || pool, nftMint: nft.publicKey, amount: amountBase });
  const isSol = l.quoteMint.equals(C.WSOL), w = C.ata(user, C.WSOL);
  const ixs = [...cu(1_000_000)];
  if (isSol) ixs.push(C.createAtaIdempotent(user, user, C.WSOL), SystemProgram.transfer({ fromPubkey: user, toPubkey: w, lamports: Number(amountBase) }), C.syncNative(w));
  ixs.push(ix);
  if (isSol) ixs.push(C.closeAccount(w, user, user));
  const [sig] = await sendAll([{ ixs, signers: [nft] }]);
  return { sig, index };
}
function seatIxs(l, seat, kind) {
  const user = wallet.pubkey, isSol = l.quoteMint.equals(C.WSOL);
  const ixs = [...cu(1_000_000), C.createAtaIdempotent(user, user, l.tokenMint, C.TOKEN_2022), C.createAtaIdempotent(user, user, l.quoteMint, l.quoteTokenProgram)];
  return { ixs, isSol, user };
}
async function exitSeat(l, pool, seat) {
  const { ixs, isSol, user } = seatIxs(l, seat);
  ixs.push(C.exitIx({ user, launch: l, pool, seat }));
  if (isSol) ixs.push(C.closeAccount(C.ata(user, C.WSOL), user, user));
  await sendAll([{ ixs }]);
}
async function collect(l, pool, seat) {
  const { ixs, isSol, user } = seatIxs(l, seat);
  ixs.push(C.collectFeesIx({ user, launch: l, pool, seat }));
  if (isSol) ixs.push(C.closeAccount(C.ata(user, C.WSOL), user, user));
  await sendAll([{ ixs }]);
}

// ---------- create ----------
const SUPPLY_WHOLE = 1_000_000_000, DECIMALS = 6; // every launch: 1B tokens, 6 decimals
async function uploadImage(name, file) {
  // Up to 4.5 MB: post the bytes to the function, which writes through OIDC. Bigger files
  // go direct-to-Blob as a client upload, which needs a read-write token on the project.
  if (file.size <= 4.5 * 1024 * 1024) {
    const r = await fetch(`/api/upload?name=${encodeURIComponent(name)}&type=${encodeURIComponent(file.type)}`, { method: "POST", headers: { "content-type": "application/octet-stream" }, body: file });
    const j = await r.json();
    if (!r.ok) throw new Error(j.error || "image upload failed");
    return j;
  }
  try {
    const { upload } = await import("https://esm.sh/@vercel/blob@2.8.0/client");
    return await upload(`images/${name}`, file, { access: "public", handleUploadUrl: "/api/upload" });
  } catch (e) {
    throw new Error("images over 4.5 MB need the project's Blob read-write token; use a smaller image for now");
  }
}
function renderCreate() {
  ++routeSeq;
  app.innerHTML = `
    <div class="display" style="font-size:28px">Launch a token</div>
    <div class="muted" style="margin-bottom:16px">Token-2022 mint with the metadata inside it, fixed supply, mint and freeze authority gone. Four transactions, one signature prompt.</div>
    <form id="create" class="grid">
      <div style="display:flex;flex-direction:column;gap:14px">
        <div class="panel two">
          <div class="field"><label>Name</label><input class="input" name="name" maxlength="32" required placeholder="Breaking"></div>
          <div class="field"><label>Symbol</label><input class="input mono" name="symbol" maxlength="10" required placeholder="CNN" style="text-transform:uppercase"></div>
          <div class="field" style="grid-column:1/-1"><label>Description</label><textarea class="input" name="description" rows="2" maxlength="1000" placeholder="One line is plenty."></textarea></div>
          <div class="field" style="grid-column:1/-1"><label>Image (png, jpg, gif, webp · up to 5 MB)</label><input class="input" type="file" name="image" accept="image/png,image/jpeg,image/gif,image/webp" required style="padding-top:10px"></div>
        </div>
        <div class="panel two">
          <div class="field" style="grid-column:1/-1"><label>Quote token</label>
            <div style="display:flex;gap:8px"><select class="input" name="quoteSel" style="width:auto"><option value="sol">SOL</option><option value="usdc">USDC</option><option value="custom">Other mint…</option></select><input class="input mono" name="quoteMint" placeholder="quote mint address" style="display:none"></div>
            <div class="muted" style="font-size:12px">Any SPL or Token-2022 mint without transfer hooks, permanent delegate, close authority, default-frozen, confidential, non-transferable or pausable extensions.</div></div>
          <div class="field"><label>Opening market cap (quote units)</label><input class="input mono" name="mcap" inputmode="decimal" value="411" required></div>
          <div class="field"><label>Floor (% of reserve, locked forever)</label><input class="input mono" name="floor" inputmode="decimal" value="5" required></div>
        </div>
      </div>
      <div style="display:flex;flex-direction:column;gap:14px">
        <div class="panel" style="display:flex;flex-direction:column;gap:8px">
          <div class="display" style="font-size:16px">What gets created</div>
          <div class="kv"><span class="muted">Token-2022 mint</span><b>metadata in mint</b></div>
          <div class="kv"><span class="muted">Supply</span><b>1B · 6 decimals · fixed</b></div>
          <div class="kv"><span class="muted">Whirlpool</span><b>ts 128 · 1% adaptive</b></div>
          <div class="kv"><span class="muted">Floor position</span><b>permanent lock</b></div>
          <div class="kv"><span class="muted">Seat rules</span><b>60s min age · 10%/min exit cap</b></div>
          <div class="kv"><span class="muted">Floor fees</span><b>50% creator · 50% treasury</b></div>
          <div class="kv"><span class="muted">Tokens per quote at open</span><b id="tpq">—</b></div>
        </div>
        <button class="btn primary" style="height:56px;font-size:18px" type="submit" id="go">Launch</button>
        <div id="progress" class="muted" style="font-size:13px"></div>
      </div>
    </form>`;
  const form = document.getElementById("create");
  const sel = form.quoteSel, custom = form.quoteMint;
  sel.onchange = () => (custom.style.display = sel.value === "custom" ? "" : "none");
  const tpq = () => { const m = parseFloat(form.mcap.value); document.getElementById("tpq").textContent = m > 0 ? `${fmtTok(9 * SUPPLY_WHOLE / m)} per quote (90/10 seat)` : "—"; };
  form.mcap.oninput = tpq; tpq();
  form.onsubmit = async (e) => {
    e.preventDefault();
    if (!wallet.pubkey) return connectFlow();
    const go = document.getElementById("go"), prog = document.getElementById("progress");
    go.disabled = true;
    const step = (t) => (prog.textContent = t);
    try {
      const name = form.name.value.trim(), symbol = form.symbol.value.trim().toUpperCase(), description = form.description.value.trim();
      const file = form.image.files[0];
      if (!file) throw new Error("pick an image");
      if (file.size > 5 * 1024 * 1024) throw new Error("image must be under 5 MB");
      const quoteMint = sel.value === "sol" ? C.WSOL : sel.value === "usdc" ? C.USDC : new PublicKey(custom.value.trim());
      const decimals = DECIMALS, supplyWhole = SUPPLY_WHOLE, mcap = parseFloat(form.mcap.value), floorPct = parseFloat(form.floor.value);
      if (!(mcap > 0) || !(floorPct >= 0 && floorPct < 50)) throw new Error("check market cap and floor");
      step("Uploading image…");
      const img = await uploadImage(`${symbol.toLowerCase()}-${file.name}`, file);
      step("Writing metadata…");
      const mr = await fetch("/api/upload", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ type: "metadata", name, symbol, description, image: img.url }) });
      const mj = await mr.json();
      if (!mr.ok) throw new Error(mj.error || "metadata upload failed");
      step("Reading quote mint…");
      const qa = await conn.getAccountInfo(quoteMint, "confirmed");
      if (!qa) throw new Error("quote mint not found");
      const quoteProgram = new PublicKey(qa.owner.toString());
      const quoteDecimals = C.decodeMint(new Uint8Array(qa.data)).decimals;
      const creator = wallet.pubkey;
      const tokenMint = Keypair.generate(), vaultToken = Keypair.generate(), vaultQuote = Keypair.generate(), bundleMint = Keypair.generate(), positionMint = Keypair.generate();
      const tokenIsA = C.isTokenA(tokenMint.publicKey, quoteMint);
      const launch = C.launchPda(tokenMint.publicKey);
      const supply = BigInt(Math.round(supplyWhole)) * 10n ** BigInt(decimals);
      const initialTick = C.initialTickFor(mcap, supplyWhole, decimals, quoteDecimals, tokenIsA);
      if (initialTick <= C.BOTTOM_TICK + 4000 || initialTick >= C.TOP_TICK - 4000) throw new Error("opening market cap is out of range for this supply");
      const ix0 = C.createLaunchIx({ creator, launch, tokenMint: tokenMint.publicKey, quoteMint, quoteProgram, decimals, supply, minAgeS: 60, exitCapBps: 1000, lowerDeltaTicks: -2432, floorBps: Math.round(floorPct * 100), creatorFeeBps: 5000, name, symbol, uri: mj.url });
      const { pool, ix: ix1 } = C.initPoolIx({ creator, launch, tokenMint: tokenMint.publicKey, quoteMint, tokenIsA, vaultToken: vaultToken.publicKey, vaultQuote: vaultQuote.publicKey, initialTick });
      const ix2 = C.newBundleIx({ payer: creator, launch, bundleMint: bundleMint.publicKey });
      const poolState = { tick: initialTick, vaultA: tokenIsA ? vaultToken.publicKey : vaultQuote.publicKey, vaultB: tokenIsA ? vaultQuote.publicKey : vaultToken.publicKey, quoteProgram };
      const ix3 = C.seedFloorIx({ creator, launch, pool, poolState, positionMint: positionMint.publicKey, tokenMint: tokenMint.publicKey, quoteMint, tokenIsA });
      step("Sign 4 transactions in your wallet…");
      await sendAll([
        { ixs: [...cu(400_000), ix0], signers: [tokenMint] },
        { ixs: [...cu(400_000), ix1], signers: [vaultToken, vaultQuote] },
        { ixs: [...cu(300_000), ix2], signers: [bundleMint] },
        { ixs: [...cu(1_000_000), ix3], signers: [positionMint] },
      ], (i, n) => step(`Sending ${i + 1}/${n}: ${["create launch", "open pool", "position bundle", "seed floor"][i]}…`));
      launchCache.at = 0;
      location.hash = `#/launch/${tokenMint.publicKey}`;
    } catch (err) { showError(err); go.disabled = false; step(""); }
  };
}

// ---------- router ----------
function route() {
  clearError();
  const h = location.hash || "#/";
  const m = h.match(/^#\/launch\/([1-9A-HJ-NP-Za-km-z]+)/);
  if (m) return renderLaunch(m[1]);
  if (h.startsWith("#/create")) { clearInterval(pageTimer); unsubscribeAll(); return renderCreate(); }
  clearInterval(pageTimer); unsubscribeAll();
  return renderExplore();
}
window.ladder = { wallet, conn, C, route }; // debugging handle
window.addEventListener("hashchange", route);
renderWalletButton();
route();
(async () => { // silent reconnect
  let saved = null; try { saved = localStorage.getItem("ladder.wallet"); } catch {}
  const entry = wallet.list().find((w) => w.name === saved);
  if (entry) { try { await entry.p.connect({ onlyIfTrusted: true }); wallet.provider = entry.p; wallet.pubkey = new PublicKey(entry.p.publicKey.toString()); renderWalletButton(); route(); } catch {} }
})();
