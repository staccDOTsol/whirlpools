// Vercel Blob for token images and the fungible metadata JSON the mint's `uri` points at.
// Three paths:
//   type "blob.*"      handleUpload: mints a client-upload token (5 MB cap, random suffix).
//                      Needs BLOB_READ_WRITE_TOKEN; the SDK has no OIDC path for this.
//   octet-stream body  put() through the project's OIDC token (or the read-write token).
//                      Vercel caps function payloads at 4.5 MB, so the browser only uses
//                      this when the client upload is unavailable.
//   type "metadata"    put() of the metadata JSON; its URL becomes the on-chain uri.
import { put } from "@vercel/blob";
import { handleUpload } from "@vercel/blob/client";

const MAX_IMAGE = 5 * 1024 * 1024;
const IMAGE_TYPES = ["image/png", "image/jpeg", "image/gif", "image/webp", "image/svg+xml"];
export const config = { api: { bodyParser: { sizeLimit: "5mb" } } };

async function rawBody(req) {
  if (Buffer.isBuffer(req.body)) return req.body;
  if (typeof req.body === "string") return Buffer.from(req.body, "binary");
  const chunks = [];
  for await (const c of req) chunks.push(Buffer.isBuffer(c) ? c : Buffer.from(c));
  return Buffer.concat(chunks);
}

export default async function handler(req, res) {
  if (req.method !== "POST") return res.status(405).json({ error: "POST only" });
  const ct = String(req.headers["content-type"] || "");
  try {
    if (ct.startsWith("application/octet-stream")) {
      // Image bytes; the real MIME type rides in ?type= because the runtime only hands
      // over a Buffer for octet-stream bodies.
      const mime = String(req.query?.type || "");
      if (!IMAGE_TYPES.includes(mime)) return res.status(400).json({ error: `unsupported image type ${mime}` });
      const name = String(req.query?.name || "image").replace(/[^a-zA-Z0-9._-]/g, "_").slice(0, 80);
      const body = await rawBody(req);
      if (!body.length) return res.status(400).json({ error: "empty image" });
      if (body.length > MAX_IMAGE) return res.status(413).json({ error: "image over 5 MB" });
      const blob = await put(`images/${name}`, body, { access: "public", addRandomSuffix: true, contentType: mime });
      return res.status(200).json({ url: blob.url });
    }
    const body = typeof req.body === "string" ? JSON.parse(req.body) : req.body;
    if (body?.type?.startsWith("blob.")) {
      const json = await handleUpload({
        body, request: req,
        onBeforeGenerateToken: async (pathname) => ({ allowedContentTypes: IMAGE_TYPES, maximumSizeInBytes: MAX_IMAGE, addRandomSuffix: true, tokenPayload: JSON.stringify({ pathname }) }),
        onUploadCompleted: async () => {},
      });
      return res.status(200).json(json);
    }
    if (body?.type === "metadata") {
      const { name, symbol, description, image } = body;
      if (!name || !symbol || !image) return res.status(400).json({ error: "name, symbol and image are required" });
      const meta = { name: String(name).slice(0, 32), symbol: String(symbol).slice(0, 10), description: String(description || "").slice(0, 1000), image, showName: true, createdOn: "https://ladderlaunch.fun" };
      const blob = await put(`metadata/${meta.symbol.toLowerCase()}.json`, JSON.stringify(meta), { access: "public", addRandomSuffix: true, contentType: "application/json" });
      return res.status(200).json({ url: blob.url });
    }
    return res.status(400).json({ error: "unknown request" });
  } catch (e) {
    return res.status(400).json({ error: e.message || String(e) });
  }
}
