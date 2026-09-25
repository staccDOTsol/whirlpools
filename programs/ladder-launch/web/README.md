# Ladder Launch web

Static front end plus two Vercel functions. Nothing on the page is sample data: launches,
prices, seats and the tape are read from mainnet through `api/rpc.js`.

- `index.html`, `app.js`, `chain.js`: explore list, launch page (chart embed, tape,
  one-tap seats, two-tap cash-out, fee collection) and the create flow. No bundler;
  `@solana/web3.js` comes from esm.sh.
- `api/launches.js`, `api/launch.js`: server-assembled JSON for the explore list and a
  launch page (launch, pool, seats, tape, price history from Whirlpool `Traded` events),
  cached at the CDN edge for a few seconds with stale-while-revalidate. Reads use
  `HISTORY_RPC_URL` (Triton: fast getProgramAccounts and a complete address index).
- `api/rpc.js`: JSON-RPC proxy with a method allowlist and a short in-memory cache;
  history methods go to `HISTORY_RPC_URL`, the rest to `RPC_URL`.
- `api/config.js`: hands the browser `RPC_URL` (live reads, simulation, sends) and
  `WS_URL` (account subscriptions for realtime chart and seat updates).
- `api/upload.js`: Vercel Blob. Token images go up as client uploads (5 MB cap, random
  suffix); the fungible metadata JSON is written server-side and its URL becomes the
  mint's on-chain `uri`. Needs `BLOB_READ_WRITE_TOKEN`.

Local: `vercel dev --listen 3000` in this directory (after `vercel link`). Deploy with
`sh deploy.sh`: it stamps the script URLs with a version so no browser keeps running a
stale `app.js`, then runs `vercel deploy --prod`.

Env (Vercel, sensitive): `RPC_URL`, `HISTORY_RPC_URL`, `WS_URL`. Blob uses the project's
OIDC token and `BLOB_STORE_ID`; add `BLOB_READ_WRITE_TOKEN` to enable 5 MB client uploads.
