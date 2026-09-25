# Ladder Launch web

Static front end plus two Vercel functions. Nothing on the page is sample data: launches,
prices, seats and the tape are read from mainnet through `api/rpc.js`.

- `index.html`, `app.js`, `chain.js`: explore list, launch page (chart embed, tape,
  one-tap seats, two-tap cash-out, fee collection) and the create flow. No bundler;
  `@solana/web3.js` comes from esm.sh.
- `api/rpc.js`: JSON-RPC proxy with a method allowlist and a short in-memory cache.
  Set `RPC_URL` in the Vercel project; the public mainnet endpoint is the fallback.
- `api/upload.js`: Vercel Blob. Token images go up as client uploads (5 MB cap, random
  suffix); the fungible metadata JSON is written server-side and its URL becomes the
  mint's on-chain `uri`. Needs `BLOB_READ_WRITE_TOKEN`.

Local: `vercel dev --listen 3000` in this directory (after `vercel link`). Deploy:
`vercel deploy --prod`.
