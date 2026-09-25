import { launches } from "./_data.js";
export default async function handler(req, res) {
  try {
    const data = await launches();
    res.setHeader("cache-control", "public, s-maxage=5, stale-while-revalidate=60");
    res.status(200).json(data);
  } catch (e) { res.status(500).json({ error: e.message || String(e) }); }
}
