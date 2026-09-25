// Public runtime config for the page. The browser talks to RPC_URL directly for live state
// and sends; keeping it here (not in app.js) keeps the key out of the repository.
export default function handler(req, res) {
  res.setHeader("cache-control", "public, max-age=60");
  res.status(200).json({ rpc: process.env.RPC_URL || null, ws: process.env.WS_URL || null });
}
