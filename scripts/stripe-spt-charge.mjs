#!/usr/bin/env node

import crypto from "node:crypto";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";

const PREVIEW_VERSION = "2026-04-22.preview";
const STRIPE_API = "https://api.stripe.com";
const SPT_FIELD = "payment_method_data[shared_payment_granted_token]";

const args = process.argv.slice(2);

if (args.includes("--help") || args.includes("-h")) {
  usage(0);
}
if (!args.includes("--demo")) {
  usage(1);
}

const receiptDir = option("--receipt-dir") || mkdtempSync(path.join(os.tmpdir(), "runx-stripe-spt-demo-"));
mkdirSync(receiptDir, { recursive: true });

const mode = process.env.RUNX_STRIPE_DEMO_MODE === "mock" || !process.env.STRIPE_TEST_KEY ? "mock" : "live";
const settlement = mode === "mock" ? mockSettlement() : await liveSettlement();
const refusal = governedRefusal();
const receipts = writeDemoReceipts(receiptDir, settlement, refusal);
const report = {
  schema: "runx.stripe_spt.demo.v1",
  mode,
  operator_keyed: mode === "live",
  receipt_dir: receiptDir,
  settlement,
  refusal,
  receipts,
};
writeFileSync(path.join(receiptDir, "stripe-spt-demo-report.json"), `${JSON.stringify(report, null, 2)}\n`);
console.log(JSON.stringify(report, null, 2));

async function liveSettlement() {
  const restrictedKey = requiredEnv("STRIPE_TEST_KEY");
  if (!restrictedKey.startsWith("rk_test_")) {
    fail("STRIPE_TEST_KEY must be a restricted rk_test_ key");
  }
  const admission = admissionFromEnv();
  const paymentMethod = process.env.RUNX_STRIPE_TEST_PAYMENT_METHOD || "pm_card_visa";
  const token = await stripePost(restrictedKey, "/v1/test_helpers/shared_payment/granted_tokens", {
    idempotencyKey: `${admission.idempotency_key}:test-granted-token`,
    form: sharedPaymentTokenForm(admission, paymentMethod),
  });
  const spt = stringField(token, "id");
  if (!spt.startsWith("spt_")) {
    fail("Stripe test helper did not return an spt_ token id");
  }
  const paymentIntent = await stripePost(restrictedKey, "/v1/payment_intents", {
    idempotencyKey: `${admission.idempotency_key}:payment-intent`,
    form: paymentIntentForm(admission, spt),
  });
  const paymentIntentId = stringField(paymentIntent, "id");
  const chargeId = stringField(paymentIntent, "latest_charge", false) || `ch_from_${paymentIntentId}`;
  if (!paymentIntentId.startsWith("pi_") || !chargeId.startsWith("ch_")) {
    fail("Stripe PaymentIntent response must include pi_ and ch_ identifiers");
  }
  return settlementReport({ admission, paymentIntentId, chargeId, eventId: `evt_from_${paymentIntentId}`, spt });
}

function mockSettlement() {
  const admission = admissionFromEnv({
    moneyMovementId: process.env.RUNX_STRIPE_MONEY_MOVEMENT_ID || "mmid_stripe_mock_demo",
  });
  return settlementReport({
    admission,
    paymentIntentId: process.env.RUNX_STRIPE_PAYMENT_INTENT_ID || "pi_test_mock_demo",
    chargeId: process.env.RUNX_STRIPE_CHARGE_ID || "ch_test_mock_demo",
    eventId: process.env.RUNX_STRIPE_EVENT_ID || "evt_test_mock_demo",
    spt: process.env.RUNX_STRIPE_SPT_ID || "spt_test_mock_demo",
  });
}

function settlementReport({ admission, paymentIntentId, chargeId, eventId, spt }) {
  return {
    status: "settled",
    rail: "stripe-spt",
    money_movement_id: admission.money_movement_id,
    rail_reference: chargeId,
    payment_intent_id: paymentIntentId,
    charge_id: chargeId,
    event_id: eventId,
    shared_payment_token_ref: spt,
    admission_token_digest: admission.admission_token_digest,
    amount_minor: admission.amount_minor,
    currency: admission.currency,
    settlement_proof: {
      payment_admission_id: admission.payment_admission_id,
      money_movement_id: admission.money_movement_id,
      kernel_token_digest: admission.kernel_token_digest,
      proof_locator: chargeId,
      proof_status: "settled",
    },
  };
}

function governedRefusal() {
  const maxPerCall = numberEnv("RUNX_STRIPE_MAX_PER_CALL_MINOR", 100);
  const attempted = numberEnv("RUNX_STRIPE_DEMO_REFUSAL_AMOUNT_MINOR", maxPerCall + 25);
  const refused = attempted > maxPerCall;
  return {
    status: refused ? "refused" : "allowed",
    reason_code: refused ? "cap_exceeded" : "within_cap",
    attempted_amount_minor: attempted,
    max_per_call_minor: maxPerCall,
    spt_minted: false,
    stripe_call_performed: false,
  };
}

function writeDemoReceipts(directory, settlement, refusal) {
  const settlementReceipt = signedDemoReceipt({
    idSeed: `${settlement.money_movement_id}:settled:${settlement.charge_id}`,
    disposition: "sealed",
    reasonCode: "stripe_spt_settled",
    subject: settlement,
  });
  const refusalReceipt = signedDemoReceipt({
    idSeed: `stripe-demo-refusal:${refusal.reason_code}:${refusal.attempted_amount_minor}`,
    disposition: "refused",
    reasonCode: refusal.reason_code,
    subject: {
      rail: "stripe-spt",
      ...refusal,
    },
  });
  const settlementPath = path.join(directory, "stripe-spt-settlement.receipt.json");
  const refusalPath = path.join(directory, "stripe-spt-refusal.receipt.json");
  writeFileSync(settlementPath, `${JSON.stringify(settlementReceipt, null, 2)}\n`);
  writeFileSync(refusalPath, `${JSON.stringify(refusalReceipt, null, 2)}\n`);
  return {
    settlement: settlementPath,
    refusal: refusalPath,
    verify_settlement: `node examples/governed-spend/verify.mjs ${settlementPath}`,
    verify_refusal: `node examples/governed-spend/verify.mjs ${refusalPath}`,
  };
}

function sharedPaymentTokenForm(admission, paymentMethod) {
  const form = new URLSearchParams();
  form.set("payment_method", paymentMethod);
  form.set("usage_limits[max_amount]", String(admission.amount_minor));
  form.set("usage_limits[currency]", admission.currency.toLowerCase());
  appendMetadata(form, admission);
  return form;
}

function paymentIntentForm(admission, spt) {
  const form = new URLSearchParams();
  form.set("amount", String(admission.amount_minor));
  form.set("currency", admission.currency.toLowerCase());
  form.set("confirm", "true");
  form.set(SPT_FIELD, spt);
  appendMetadata(form, admission);
  return form;
}

function appendMetadata(form, admission) {
  form.set("metadata[money_movement_id]", admission.money_movement_id);
  form.set("metadata[admission_token_digest]", admission.admission_token_digest);
  form.set("metadata[counterparty]", admission.counterparty);
  form.set("metadata[rail]", "stripe-spt");
}

async function stripePost(restrictedKey, route, { idempotencyKey, form }) {
  const response = await fetch(`${STRIPE_API}${route}`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${restrictedKey}`,
      "Content-Type": "application/x-www-form-urlencoded",
      "Idempotency-Key": idempotencyKey,
      "Stripe-Version": PREVIEW_VERSION,
    },
    body: form,
  });
  const payload = await response.json();
  if (!response.ok) {
    fail(payload?.error?.message || `Stripe request failed with HTTP ${response.status}`);
  }
  return payload;
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
    schema: "runx.stripe_spt.demo_receipt.v1",
    id: `stripe_spt_demo_${sha256Hex(input.idSeed).slice(0, 24)}`,
    created_at: new Date().toISOString(),
    name: "stripe-spt-charge",
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

function admissionFromEnv(defaults = {}) {
  const amount = numberEnv("RUNX_STRIPE_AMOUNT_MINOR", 125);
  return {
    payment_admission_id: envOr("RUNX_STRIPE_PAYMENT_ADMISSION_ID", "pa_stripe_demo"),
    money_movement_id: envOr("RUNX_STRIPE_MONEY_MOVEMENT_ID", defaults.moneyMovementId || "mmid_stripe_demo"),
    kernel_token_digest: envOr("RUNX_STRIPE_KERNEL_TOKEN_DIGEST", "sha256:kernel-token-demo"),
    admission_token_digest: envOr("RUNX_STRIPE_ADMISSION_TOKEN_DIGEST", "sha256:admission-token-demo"),
    amount_minor: amount,
    currency: envOr("RUNX_STRIPE_CURRENCY", "USD"),
    counterparty: envOr("RUNX_STRIPE_COUNTERPARTY", "acct_demo_counterparty"),
    idempotency_key: envOr("RUNX_STRIPE_IDEMPOTENCY_KEY", "stripe-spt-demo"),
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

function stringField(object, field, required = true) {
  const value = object?.[field];
  if (typeof value === "string") return value;
  if (required) fail(`Stripe response missing ${field}`);
  return undefined;
}

function numberEnv(name, fallback) {
  const raw = process.env[name];
  if (raw === undefined || raw === "") return fallback;
  const value = Number(raw);
  if (!Number.isSafeInteger(value) || value < 0) fail(`${name} must be a non-negative integer`);
  return value;
}

function requiredEnv(name) {
  const value = process.env[name]?.trim();
  if (!value) fail(`${name} is required`);
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
  console.error("usage: node scripts/stripe-spt-charge.mjs --demo [--receipt-dir DIR]");
  process.exit(code);
}

function fail(message) {
  console.error(`stripe-spt-charge: ${message}`);
  process.exit(1);
}
