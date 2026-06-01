#!/usr/bin/env node

import crypto from "node:crypto";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";

const args = process.argv.slice(2);
if (args.includes("--help") || args.includes("-h")) usage(0);
if (!args.includes("--record")) usage(1);

const receiptDir = option("--receipt-dir") || mkdtempSync(path.join(os.tmpdir(), "runx-payments-demo-"));
mkdirSync(receiptDir, { recursive: true });

const liveRequested = process.env.RUNX_PAYMENTS_DEMO_MODE === "live";
const liveReady = Boolean(process.env.ANTHROPIC_API_KEY && process.env.RUNX_X402_SIGNER);
if (liveRequested && !liveReady) {
  fail("live mode requires ANTHROPIC_API_KEY and RUNX_X402_SIGNER");
}

const mode = liveReady ? "operator-keyed-testnet" : "recorded-mock";
const runId = envOr("RUNX_PAYMENTS_DEMO_RUN_ID", "run_payments_demo_001");
const paid = paidSpend(runId);
const refusal = governedRefusal(runId);
const receipts = writeDemoReceipts(receiptDir, paid, refusal);
const report = {
  schema: "runx.payments_demo.v1",
  mode,
  operator_keyed: liveReady,
  honesty: liveReady
    ? "Operator keys were present; this transcript is suitable for a recorded testnet run."
    : "No operator keys were present; this is a deterministic mock transcript. The receipts and refusal verifier are real.",
  ab: {
    without_runx: {
      result: "unscoped_spend_possible",
      wallet_key_exposed_to_agent: true,
      refusal_before_money_moves: false,
    },
    with_runx: {
      result: "scoped_spend_then_refusal",
      wallet_key_exposed_to_agent: false,
      refusal_before_money_moves: true,
    },
  },
  paid,
  refusal,
  receipts,
};

writeFileSync(path.join(receiptDir, "payments-demo-report.json"), `${JSON.stringify(report, null, 2)}\n`);
console.log(JSON.stringify(report, null, 2));

function paidSpend(runId) {
  const amount = numberEnv("RUNX_PAYMENTS_DEMO_PAID_AMOUNT_MINOR", 125);
  return {
    status: "settled",
    rail: "x402",
    run_id: runId,
    money_movement_id: envOr("RUNX_PAYMENTS_DEMO_MONEY_MOVEMENT_ID", "mmid_x402_demo_paid_001"),
    tx_hash: envOr("RUNX_X402_TX_HASH", "0xmock_x402_base_sepolia_paid_001"),
    facilitator: envOr("RUNX_X402_FACILITATOR", "base-sepolia-demo-facilitator"),
    amount_minor: amount,
    currency: envOr("RUNX_PAYMENTS_DEMO_CURRENCY", "USD"),
    counterparty: envOr("RUNX_PAYMENTS_DEMO_COUNTERPARTY", "merchant:x402-demo"),
    authority: {
      max_per_call_minor: numberEnv("RUNX_PAYMENTS_DEMO_MAX_PER_CALL_MINOR", 150),
      max_per_run_minor: numberEnv("RUNX_PAYMENTS_DEMO_MAX_PER_RUN_MINOR", 150),
      rails: ["x402"],
    },
    settlement_proof: {
      payment_admission_id: envOr("RUNX_PAYMENTS_DEMO_PAYMENT_ADMISSION_ID", "pa_x402_demo"),
      money_movement_id: envOr("RUNX_PAYMENTS_DEMO_MONEY_MOVEMENT_ID", "mmid_x402_demo_paid_001"),
      kernel_token_digest: envOr("RUNX_PAYMENTS_DEMO_KERNEL_TOKEN_DIGEST", "sha256:kernel-token-demo"),
      proof_locator: envOr("RUNX_X402_TX_HASH", "0xmock_x402_base_sepolia_paid_001"),
      proof_status: "settled",
    },
  };
}

function governedRefusal(runId) {
  const maxPerRun = numberEnv("RUNX_PAYMENTS_DEMO_MAX_PER_RUN_MINOR", 150);
  const attempted = numberEnv("RUNX_PAYMENTS_DEMO_REFUSAL_AMOUNT_MINOR", 75);
  const alreadySpent = numberEnv("RUNX_PAYMENTS_DEMO_ALREADY_SPENT_MINOR", 125);
  const refused = alreadySpent + attempted > maxPerRun;
  return {
    status: refused ? "refused" : "allowed",
    run_id: runId,
    reason_code: refused ? "run_cap_exceeded" : "within_cap",
    attempted_amount_minor: attempted,
    already_reserved_minor: alreadySpent,
    max_per_run_minor: maxPerRun,
    rail_call_performed: false,
    money_movement_id: null,
  };
}

function writeDemoReceipts(directory, paid, refusal) {
  const paidReceipt = signedDemoReceipt({
    idSeed: `${paid.run_id}:${paid.money_movement_id}:paid`,
    name: "payments-demo-paid",
    disposition: "sealed",
    reasonCode: "x402_testnet_settled",
    subject: paid,
  });
  const refusalReceipt = signedDemoReceipt({
    idSeed: `${refusal.run_id}:${refusal.reason_code}:${refusal.attempted_amount_minor}`,
    name: "payments-demo-refusal",
    disposition: "refused",
    reasonCode: refusal.reason_code,
    subject: {
      rail: "x402",
      ...refusal,
    },
  });
  const paidPath = path.join(directory, "payments-demo-paid.receipt.json");
  const refusalPath = path.join(directory, "payments-demo-refusal.receipt.json");
  writeFileSync(paidPath, `${JSON.stringify(paidReceipt, null, 2)}\n`);
  writeFileSync(refusalPath, `${JSON.stringify(refusalReceipt, null, 2)}\n`);
  return {
    paid: paidPath,
    refusal: refusalPath,
    verify_paid: `node examples/governed-spend/verify.mjs ${paidPath}`,
    verify_refusal: `node examples/governed-spend/verify.mjs ${refusalPath}`,
  };
}

function signedDemoReceipt(input) {
  const seed = Buffer.from(
    process.env.RUNX_RECEIPT_SIGN_ED25519_SEED_BASE64 ||
      "QkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkI=",
    "base64",
  );
  if (seed.length !== 32) {
    fail("RUNX_RECEIPT_SIGN_ED25519_SEED_BASE64 must decode to a 32-byte Ed25519 seed");
  }
  const privateKey = privateKeyFromSeed(seed);
  const publicKey = crypto.createPublicKey(privateKey);
  const publicKeyRaw = publicKey.export({ format: "der", type: "spki" }).subarray(-32);
  const body = {
    schema: "runx.payments_demo.receipt.v1",
    id: `payments_demo_${sha256Hex(input.idSeed).slice(0, 24)}`,
    created_at: new Date().toISOString(),
    name: input.name,
    seal: {
      disposition: input.disposition,
      reason_code: input.reasonCode,
    },
    issuer: {
      type: process.env.RUNX_RECEIPT_SIGN_ISSUER_TYPE || "hosted",
      kid: process.env.RUNX_RECEIPT_SIGN_KID || "runx-demo-key",
      public_key_sha256: sha256Prefixed(publicKeyRaw),
    },
    subject: input.subject,
  };
  const digest = sha256Prefixed(canon(body));
  const signature = crypto.sign(null, Buffer.from(digest), privateKey).toString("base64url");
  return {
    ...body,
    digest,
    signature: {
      alg: "Ed25519",
      kid: body.issuer.kid,
      value: `base64:${signature}`,
    },
  };
}

function privateKeyFromSeed(seed) {
  const pkcs8 = Buffer.concat([Buffer.from("302e020100300506032b657004220420", "hex"), seed]);
  return crypto.createPrivateKey({ key: pkcs8, format: "der", type: "pkcs8" });
}

function canon(value) {
  if (value === null) return "null";
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "number" || typeof value === "string") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canon).join(",")}]`;
  return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canon(value[key])}`).join(",")}}`;
}

function sha256Prefixed(value) {
  return `sha256:${crypto.createHash("sha256").update(value).digest("hex")}`;
}

function sha256Hex(value) {
  return crypto.createHash("sha256").update(value).digest("hex");
}

function numberEnv(name, fallback) {
  const raw = process.env[name];
  if (raw === undefined || raw === "") return fallback;
  const value = Number(raw);
  if (!Number.isSafeInteger(value) || value < 0) fail(`${name} must be a non-negative integer`);
  return value;
}

function envOr(name, fallback) {
  const value = process.env[name]?.trim();
  return value || fallback;
}

function option(name) {
  const index = args.indexOf(name);
  return index === -1 ? undefined : args[index + 1];
}

function usage(code) {
  console.error("usage: node scripts/payments-demo.mjs --record [--receipt-dir DIR]");
  process.exit(code);
}

function fail(message) {
  console.error(`payments-demo: ${message}`);
  process.exit(1);
}
