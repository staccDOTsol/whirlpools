// Sweep protocol fees from every Whirlpool on the launchpad config to the collect authority.
//   node scripts/collect-protocol-fees.mjs            dry run: simulates every batch
//   SEND=1 node scripts/collect-protocol-fees.mjs     signs with ~/.config/solana/id.json and sends
// Env: RPC (defaults to the public endpoint), KEYPAIR (path).
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { Connection, Keypair, PublicKey, Transaction, TransactionInstruction, ComputeBudgetProgram, SystemProgram } from "@solana/web3.js";

const RPC = process.env.RPC || "https://api.mainnet-beta.solana.com";
const CONFIG = new PublicKey("12yTE48QR6bGK4EMcyY8XsARbX1TRTEbwHYSuuxR1Hp8");
const WHIRLPOOL = new PublicKey("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");
const TOKEN = new PublicKey("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const ATA_PROGRAM = new PublicKey("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
const MEMO = new PublicKey("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
const WSOL = new PublicKey("So11111111111111111111111111111111111111112");
const DISC = Buffer.from([103, 128, 222, 134, 114, 200, 22, 200]); // sha256("global:collect_protocol_fees_v2")[..8]

const conn = new Connection(RPC, "confirmed");
const kp = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(process.env.KEYPAIR ?? `${homedir()}/.config/solana/id.json`, "utf8"))));
const me = kp.publicKey;

const cfg = await conn.getAccountInfo(CONFIG);
const authority = new PublicKey(cfg.data.subarray(40, 72));
if (!authority.equals(me)) throw new Error(`keypair ${me} is not the collect authority ${authority}`);

const ata = (owner, mint, program) => PublicKey.findProgramAddressSync([owner.toBytes(), program.toBytes(), mint.toBytes()], ATA_PROGRAM)[0];
const createAtaIdempotent = (owner, mint, program) => new TransactionInstruction({ programId: ATA_PROGRAM, keys: [{ pubkey: me, isSigner: true, isWritable: true }, { pubkey: ata(owner, mint, program), isSigner: false, isWritable: true }, { pubkey: owner, isSigner: false, isWritable: false }, { pubkey: mint, isSigner: false, isWritable: false }, { pubkey: SystemProgram.programId, isSigner: false, isWritable: false }, { pubkey: program, isSigner: false, isWritable: false }], data: Buffer.from([1]) });
const closeAccount = (account) => new TransactionInstruction({ programId: TOKEN, keys: [{ pubkey: account, isSigner: false, isWritable: true }, { pubkey: me, isSigner: false, isWritable: true }, { pubkey: me, isSigner: true, isWritable: false }], data: Buffer.from([9]) });

const pools = await conn.getProgramAccounts(WHIRLPOOL, { filters: [{ dataSize: 653 }, { memcmp: { offset: 8, bytes: CONFIG.toBase58() } }] });
const owed = pools.map((p) => { const b = p.account.data; return { pool: p.pubkey, mintA: new PublicKey(b.subarray(101, 133)), vaultA: new PublicKey(b.subarray(133, 165)), mintB: new PublicKey(b.subarray(181, 213)), vaultB: new PublicKey(b.subarray(213, 245)), feeA: b.readBigUInt64LE(85), feeB: b.readBigUInt64LE(93) }; }).filter((x) => x.feeA > 0n || x.feeB > 0n);
console.log(`${pools.length} pools on config, ${owed.length} with protocol fees owed`);
const mints = [...new Set(owed.flatMap((x) => [x.mintA.toBase58(), x.mintB.toBase58()]))].map((m) => new PublicKey(m));
const mintInfos = await conn.getMultipleAccountsInfo(mints);
const program = new Map(mints.map((m, i) => [m.toBase58(), mintInfos[i].owner]));
const decimals = new Map(mints.map((m, i) => [m.toBase58(), mintInfos[i].data[44]]));

const collectIx = (x) => new TransactionInstruction({
  programId: WHIRLPOOL,
  keys: [
    { pubkey: CONFIG, isSigner: false, isWritable: false },
    { pubkey: x.pool, isSigner: false, isWritable: true },
    { pubkey: me, isSigner: true, isWritable: false },
    { pubkey: x.mintA, isSigner: false, isWritable: false },
    { pubkey: x.mintB, isSigner: false, isWritable: false },
    { pubkey: x.vaultA, isSigner: false, isWritable: true },
    { pubkey: x.vaultB, isSigner: false, isWritable: true },
    { pubkey: ata(me, x.mintA, program.get(x.mintA.toBase58())), isSigner: false, isWritable: true },
    { pubkey: ata(me, x.mintB, program.get(x.mintB.toBase58())), isSigner: false, isWritable: true },
    { pubkey: program.get(x.mintA.toBase58()), isSigner: false, isWritable: false },
    { pubkey: program.get(x.mintB.toBase58()), isSigner: false, isWritable: false },
    { pubkey: MEMO, isSigner: false, isWritable: false },
  ],
  data: Buffer.concat([DISC, Buffer.from([0])]), // remaining_accounts_info: None
});

// Batches of 3 pools per transaction; each pool needs its two destination accounts.
const batches = [];
for (let i = 0; i < owed.length; i += 3) {
  const group = owed.slice(i, i + 3);
  const ixs = [ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 }), ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 20_000 })];
  const seen = new Set();
  for (const x of group) for (const m of [x.mintA, x.mintB]) { const k = m.toBase58(); if (!seen.has(k)) { seen.add(k); ixs.push(createAtaIdempotent(me, m, program.get(k))); } }
  for (const x of group) ixs.push(collectIx(x));
  batches.push({ group, ixs });
}
// Last: unwrap WSOL (closes the WSOL account, all wrapped SOL to the wallet).
if (mints.some((m) => m.equals(WSOL))) batches.push({ group: [], ixs: [ComputeBudgetProgram.setComputeUnitLimit({ units: 50_000 }), closeAccount(ata(me, WSOL, TOKEN))], unwrap: true });

const fmt = (x) => `${x.pool.toBase58().slice(0, 8)}: ${Number(x.feeA) / 10 ** decimals.get(x.mintA.toBase58())} ${x.mintA.toBase58().slice(0, 5)} + ${Number(x.feeB) / 10 ** decimals.get(x.mintB.toBase58())} ${x.mintB.toBase58().slice(0, 5)}`;
for (const [i, b] of batches.entries()) {
  const { blockhash, lastValidBlockHeight } = await conn.getLatestBlockhash("confirmed");
  const tx = new Transaction({ feePayer: me, blockhash, lastValidBlockHeight }).add(...b.ixs);
  tx.sign(kp);
  const label = b.unwrap ? "unwrap WSOL" : b.group.map(fmt).join(" | ");
  const sim = await conn.simulateTransaction(tx);
  if (sim.value.err) { console.log(`batch ${i + 1} SIM FAILED`, JSON.stringify(sim.value.err), (sim.value.logs || []).slice(-5).join("\n"), "\n  ", label); continue; }
  console.log(`batch ${i + 1} ok (${sim.value.unitsConsumed} cu, ${tx.serialize().length} bytes): ${label}`);
  if (!process.env.SEND) continue;
  const sig = await conn.sendRawTransaction(tx.serialize(), { skipPreflight: false, maxRetries: 5 });
  for (let t = 0; t < 40; t++) {
    const { value: [st] } = await conn.getSignatureStatuses([sig]);
    if (st?.err) throw new Error(`batch ${i + 1} failed on chain: ${JSON.stringify(st.err)} ${sig}`);
    if (st && (st.confirmationStatus === "confirmed" || st.confirmationStatus === "finalized")) break;
    await new Promise((r) => setTimeout(r, 1500));
  }
  console.log(`  sent https://solscan.io/tx/${sig}`);
}
if (!process.env.SEND) console.log("dry run only: set SEND=1 to broadcast");
