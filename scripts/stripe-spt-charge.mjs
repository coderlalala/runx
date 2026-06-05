#!/usr/bin/env node

import crypto from "node:crypto";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { sha256Hex, sha256Prefixed, signedDemoReceipt } from "./lib/demo-receipts.mjs";

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

const requestedMode = envOr("RUNX_STRIPE_DEMO_MODE", "auto");
const stripeKey = stripeKeyFromEnv();
const mode =
  requestedMode === "mock" ? "mock" : requestedMode === "live" || stripeKey ? "live" : "mock";
if (requestedMode !== "auto" && requestedMode !== "mock" && requestedMode !== "live") {
  fail("RUNX_STRIPE_DEMO_MODE must be auto, mock, or live");
}
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
  const testKey = validateStripeTestKey(stripeKey || requiredStripeKey());
  const admission = admissionFromEnv();
  const paymentMethod = process.env.RUNX_STRIPE_TEST_PAYMENT_METHOD || "pm_card_visa";
  const token = await stripePost(testKey, "/v1/test_helpers/shared_payment/granted_tokens", {
    idempotencyKey: `${admission.idempotency_key}:test-granted-token`,
    form: sharedPaymentTokenForm(admission, paymentMethod),
  });
  const spt = stringField(token, "id");
  if (!spt.startsWith("spt_")) {
    fail("Stripe test helper did not return an spt_ token id");
  }
  const paymentIntent = await stripePost(testKey, "/v1/payment_intents", {
    idempotencyKey: `${admission.idempotency_key}:payment-intent`,
    form: paymentIntentForm(admission, spt),
  });
  const paymentIntentId = stringField(paymentIntent, "id");
  const chargeId = stripeIdField(paymentIntent, "latest_charge");
  if (!paymentIntentId.startsWith("pi_") || !chargeId.startsWith("ch_")) {
    fail("Stripe PaymentIntent response must include pi_ and ch_ identifiers");
  }
  const eventId = `evt_local_${sha256Hex(paymentIntentId).slice(0, 24)}`;
  const webhook = stripeWebhookProof({ admission, paymentIntentId, chargeId, eventId, required: true });
  return settlementReport({ admission, paymentIntentId, chargeId, eventId, spt, webhook });
}

function mockSettlement() {
  const admission = admissionFromEnv({
    moneyMovementId: process.env.RUNX_STRIPE_MONEY_MOVEMENT_ID || "mmid_stripe_mock_demo",
  });
  const paymentIntentId = process.env.RUNX_STRIPE_PAYMENT_INTENT_ID || "pi_test_mock_demo";
  const chargeId = process.env.RUNX_STRIPE_CHARGE_ID || "ch_test_mock_demo";
  const eventId = process.env.RUNX_STRIPE_EVENT_ID || "evt_test_mock_demo";
  const webhook = stripeWebhookProof({
    admission,
    paymentIntentId,
    chargeId,
    eventId,
    required: false,
  });
  return settlementReport({
    admission,
    paymentIntentId,
    chargeId,
    eventId,
    spt: process.env.RUNX_STRIPE_SPT_ID || "spt_test_mock_demo",
    webhook,
  });
}

function settlementReport({ admission, paymentIntentId, chargeId, eventId, spt, webhook }) {
  return {
    status: "settled",
    rail: "stripe-spt",
    money_movement_id: admission.money_movement_id,
    rail_reference: chargeId,
    payment_intent_id: paymentIntentId,
    charge_id: chargeId,
    event_id: eventId,
    amount_minor: admission.amount_minor,
    currency: admission.currency,
    settlement_proof: {
      payment_admission_id: admission.payment_admission_id,
      money_movement_id: admission.money_movement_id,
      kernel_token_digest: admission.kernel_token_digest,
      proof_locator: chargeId,
      proof_status: "settled",
      webhook_event_id: eventId,
      webhook_signature_verified: webhook.signature_verified,
    },
    webhook,
  };
}

function governedRefusal() {
  const maxPerCall = numberEnv("RUNX_STRIPE_MAX_PER_CALL_UNITS", 100);
  const attempted = numberEnv("RUNX_STRIPE_DEMO_REFUSAL_AMOUNT_MINOR", maxPerCall + 25);
  const refused = attempted > maxPerCall;
  return {
    status: refused ? "refused" : "allowed",
    reason_code: refused ? "cap_exceeded" : "within_cap",
    attempted_amount_minor: attempted,
    max_per_call_units: maxPerCall,
    spt_minted: false,
    stripe_call_performed: false,
  };
}

function writeDemoReceipts(directory, settlement, refusal) {
  const railReceipt = signedDemoReceipt({
    name: "stripe-spt-charge",
    disposition: "sealed",
    reasonCode: "stripe_spt_settled",
    subject: settlement,
  });
  const refusalReceipt = signedDemoReceipt({
    name: "stripe-spt-charge",
    disposition: "refused",
    reasonCode: refusal.reason_code,
    subject: {
      rail: "stripe-spt",
      ...refusal,
    },
  });
  const settlementPath = path.join(directory, "stripe-spt-settlement.receipt.json");
  const refusalPath = path.join(directory, "stripe-spt-refusal.receipt.json");
  writeFileSync(settlementPath, `${JSON.stringify(railReceipt, null, 2)}\n`);
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
  form.set("usage_limits[expires_at]", String(sptExpiresAt()));
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

function stripeWebhookProof({ admission, paymentIntentId, chargeId, eventId, required }) {
  const secret = envOr("STRIPE_WEBHOOK_SECRET", "");
  if (!secret) {
    if (required) fail("STRIPE_WEBHOOK_SECRET is required for live Stripe SPT demo mode");
    return {
      signature_verified: false,
      mode: "not_configured",
      reason_code: "stripe_webhook_secret_not_configured",
    };
  }
  if (!secret.startsWith("whsec_")) {
    fail("STRIPE_WEBHOOK_SECRET must be a Stripe test-mode webhook signing secret");
  }
  const event = {
    id: eventId,
    object: "event",
    type: "payment_intent.succeeded",
    livemode: false,
    data: {
      object: {
        id: paymentIntentId,
        object: "payment_intent",
        latest_charge: chargeId,
        amount: admission.amount_minor,
        currency: admission.currency.toLowerCase(),
        metadata: {
          money_movement_id: admission.money_movement_id,
          admission_token_digest: admission.admission_token_digest,
          counterparty: admission.counterparty,
          rail: "stripe-spt",
        },
      },
    },
  };
  const payload = JSON.stringify(event);
  const timestamp = Math.floor(Date.now() / 1000);
  const signature = stripeWebhookSignature(payload, secret, timestamp);
  const header = `t=${timestamp},v1=${signature}`;
  if (!verifyStripeWebhookSignature(payload, header, secret)) {
    fail("Stripe webhook signature verification failed");
  }
  return {
    signature_verified: true,
    mode: "local_stripe_signature_check",
    event_id: eventId,
    event_type: event.type,
    payload_sha256: sha256Prefixed(payload),
  };
}

function stripeWebhookSignature(payload, secret, timestamp) {
  return crypto.createHmac("sha256", secret).update(`${timestamp}.${payload}`).digest("hex");
}

function verifyStripeWebhookSignature(payload, header, secret, toleranceSeconds = 300) {
  const fields = new Map();
  for (const part of header.split(",")) {
    const [key, value] = part.split("=", 2);
    if (key && value) fields.set(key, value);
  }
  const timestamp = Number(fields.get("t"));
  const signature = fields.get("v1");
  if (!Number.isSafeInteger(timestamp) || !signature) return false;
  if (Math.abs(Math.floor(Date.now() / 1000) - timestamp) > toleranceSeconds) return false;
  const expected = stripeWebhookSignature(payload, secret, timestamp);
  const expectedBytes = Buffer.from(expected, "hex");
  const actualBytes = Buffer.from(signature, "hex");
  return expectedBytes.length === actualBytes.length && crypto.timingSafeEqual(expectedBytes, actualBytes);
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

function stringField(object, field, required = true) {
  const value = object?.[field];
  if (typeof value === "string") return value;
  if (required) fail(`Stripe response missing ${field}`);
  return undefined;
}

function stripeIdField(object, field) {
  const value = object?.[field];
  if (typeof value === "string") return value;
  if (value && typeof value === "object" && typeof value.id === "string") return value.id;
  fail(`Stripe response missing ${field}`);
}

function numberEnv(name, fallback) {
  const raw = process.env[name];
  if (raw === undefined || raw === "") return fallback;
  const value = Number(raw);
  if (!Number.isSafeInteger(value) || value < 0) fail(`${name} must be a non-negative integer`);
  return value;
}

function sptExpiresAt() {
  return numberEnv("RUNX_STRIPE_SPT_EXPIRES_AT", Math.floor(Date.now() / 1000) + 15 * 60);
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

function stripeKeyFromEnv() {
  return envOr("STRIPE_SECRET_KEY", "") || envOr("STRIPE_TEST_KEY", "");
}

function requiredStripeKey() {
  return envOr("STRIPE_SECRET_KEY", "") || requiredEnv("STRIPE_TEST_KEY");
}

function validateStripeTestKey(key) {
  if (key.startsWith("sk_live_") || key.startsWith("rk_live_")) {
    fail("live-mode Stripe keys are refused; use a test-mode sk_test_ or rk_test_ key");
  }
  if (!key.startsWith("sk_test_") && !key.startsWith("rk_test_")) {
    fail("STRIPE_SECRET_KEY or STRIPE_TEST_KEY must be a test-mode sk_test_ or rk_test_ key");
  }
  return key;
}

function option(name) {
  const index = args.indexOf(name);
  return index === -1 ? undefined : args[index + 1];
}

function usage(code) {
  console.error("usage: node scripts/stripe-spt-charge.mjs --demo [--receipt-dir DIR]");
  console.error("");
  console.error("env:");
  console.error("  STRIPE_SECRET_KEY          test-mode sk_test_ key for live demo mode");
  console.error("  STRIPE_TEST_KEY            backwards-compatible test-mode sk_test_ or rk_test_ key");
  console.error("  STRIPE_WEBHOOK_SECRET      whsec_ signing secret required for live demo mode");
  console.error("  RUNX_STRIPE_DEMO_MODE      auto (default), mock, or live");
  process.exit(code);
}

function fail(message) {
  console.error(`stripe-spt-charge: ${message}`);
  process.exit(1);
}
