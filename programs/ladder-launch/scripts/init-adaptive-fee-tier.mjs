// Initialize the adaptive fee tier (tick spacing 128, 1% base) on the launchpad's WhirlpoolsConfig.
// Created on mainnet 2026-09-25 as 6assHYd5438D91RXfMUrETNqGHmcbfzFCJsBjHmbkeu9 (sig YF6kYX...bw5N).
// Run on the machine that holds the config's fee authority (12Nqk...):
//   npm i @orca-so/whirlpools-client@^7 @solana/kit@^5
//   RPC_URL=https://... node init-adaptive-fee-tier.mjs            (dry run: prints the instruction, no send)
//   RPC_URL=https://... SEND=1 node init-adaptive-fee-tier.mjs     (signs with ~/.config/solana/id.json and sends)
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import {
  address, createSolanaRpc, createSolanaRpcSubscriptions, createKeyPairSignerFromBytes,
  pipe, createTransactionMessage, setTransactionMessageFeePayerSigner, setTransactionMessageLifetimeUsingBlockhash,
  appendTransactionMessageInstructions, signTransactionMessageWithSigners, sendAndConfirmTransactionFactory,
  getSignatureFromTransaction,
} from "@solana/kit";
import { getInitializeAdaptiveFeeTierInstructionAsync } from "@orca-so/whirlpools-client";

const CONFIG = address("12yTE48QR6bGK4EMcyY8XsARbX1TRTEbwHYSuuxR1Hp8");   // 25% protocol fee, tiers ts32 + ts128
const EXPECTED_FEE_AUTHORITY = "12Nqk2jyA3XNe3rPxAaLFixytmohnMrBfCsdwrCfWNm2";
const ANYONE = address("11111111111111111111111111111111");                // Pubkey::default(): anyone may open pools / no delegate

// Orca's canonical adaptive tier for tick spacing 128 (mainnet fee_tier_index 1032).
const PARAMS = {
  feeTierIndex: 1032,            // any u16 != 128; mirrors Orca's index for this spacing
  tickSpacing: 128,
  initializePoolAuthority: ANYONE,
  delegatedFeeAuthority: ANYONE,
  defaultBaseFeeRate: 10_000,    // 1.00% (units of 1e-6)
  filterPeriod: 30,              // s
  decayPeriod: 600,              // s
  reductionFactor: 5_000,        // 50% (units of 1e-4)
  adaptiveFeeControlFactor: 20_000,
  maxVolatilityAccumulator: 100_000,
  tickGroupSize: 128,
  majorSwapThresholdTicks: 128,
};

const rpcUrl = process.env.RPC_URL;
if (!rpcUrl) throw new Error("set RPC_URL");
const keyPath = process.env.KEYPAIR ?? `${homedir()}/.config/solana/id.json`;
const signer = await createKeyPairSignerFromBytes(new Uint8Array(JSON.parse(readFileSync(keyPath, "utf8"))));
if (signer.address !== EXPECTED_FEE_AUTHORITY) {
  throw new Error(`keypair ${signer.address} is not the config's fee authority ${EXPECTED_FEE_AUTHORITY}`);
}

const ix = await getInitializeAdaptiveFeeTierInstructionAsync({
  whirlpoolsConfig: CONFIG, funder: signer, feeAuthority: signer, ...PARAMS,
});
console.log("adaptive_fee_tier PDA:", ix.accounts[1].address);
console.log("params:", PARAMS);

if (!process.env.SEND) { console.log("dry run: set SEND=1 to broadcast"); process.exit(0); }

const rpc = createSolanaRpc(rpcUrl);
const rpcSubscriptions = createSolanaRpcSubscriptions(rpcUrl.replace(/^http/, "ws"));
const { value: blockhash } = await rpc.getLatestBlockhash().send();
const tx = await pipe(
  createTransactionMessage({ version: 0 }),
  (m) => setTransactionMessageFeePayerSigner(signer, m),
  (m) => setTransactionMessageLifetimeUsingBlockhash(blockhash, m),
  (m) => appendTransactionMessageInstructions([ix], m),
  (m) => signTransactionMessageWithSigners(m),
);
await sendAndConfirmTransactionFactory({ rpc, rpcSubscriptions })(tx, { commitment: "confirmed" });
console.log("sent:", getSignatureFromTransaction(tx));
