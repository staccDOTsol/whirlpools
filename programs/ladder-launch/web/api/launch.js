import { PublicKey } from "@solana/web3.js";
import { launch } from "./_data.js";
export default async function handler(req, res) {
  let mint;
  try { mint = new PublicKey(String(req.query?.mint || "")); } catch { return res.status(400).json({ error: "bad mint" }); }
  try {
    const data = await launch(mint);
    if (!data) return res.status(404).json({ error: "no launch for this mint" });
    res.setHeader("cache-control", "public, s-maxage=3, stale-while-revalidate=30");
    res.status(200).json(data);
  } catch (e) { res.status(500).json({ error: e.message || String(e) }); }
}
