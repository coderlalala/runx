#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import {
  createHash,
  createPublicKey,
  verify as verifyEd25519Signature,
} from "node:crypto";
import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const expectedMoneyMovementId = "sha256:b1f910b08abe1053af9343df6b0467dbea9018a9052e4601d7a4616f1f73ff33";
const expectedTokenDigest = "sha256:ea9a5c55346a95eceb8daa949bb6564465d6fdac31fdf4f2ab111e1722fb372c";
const zeroSeedPublicKey = "O2onvM62pC1io6jQKm8Nc2UyFXcd4kOmOsBIoYtZ2ik=";
const args = new Set(process.argv.slice(2));
const requireRecovery = args.has("--require-recovery");
const requireDigestParity = args.has("--require-digest-parity");
const ossRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = path.resolve(ossRoot, "..");

const request = {
  principal: "principal_1",
  act: "act_pay_quote",
  rail: "x402",
  amount_minor: 1250,
  currency: "USD",
  counterparty: "merchant_1",
  run_id: "run_1",
  authority_digest: "sha256:authority",
  expires_at: "2026-06-01T00:05:00Z",
};

const admission = issueAdmissionToken(request);
verifyAdmission(admission, request);
runCloudBridgeGates();

process.stdout.write(`${JSON.stringify({
  status: "passed",
  money_movement_id: admission.result.money_movement_id,
  token_digest: admission.result.token_digest,
  recovery_required: requireRecovery,
  digest_parity_required: requireDigestParity,
}, null, 2)}\n`);

function issueAdmissionToken(input) {
  const runxBin = process.env.RUNX_BIN;
  const command = runxBin && existsSync(runxBin) ? runxBin : "cargo";
  const commandArgs = runxBin && existsSync(runxBin)
    ? ["payment", "admission", "issue", "--input", "-", "--json"]
    : ["run", "-q", "--manifest-path", "crates/Cargo.toml", "-p", "runx-cli", "--", "payment", "admission", "issue", "--input", "-", "--json"];
  const result = spawnSync(command, commandArgs, {
    cwd: ossRoot,
    input: `${JSON.stringify(input)}\n`,
    encoding: "utf8",
    env: {
      ...process.env,
      RUNX_PAYMENT_ADMISSION_KID: "kid-admission-1",
      RUNX_PAYMENT_ADMISSION_SIGNING_KEY: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
    },
  });
  if (result.status !== 0) {
    throw new Error(`admission token command failed:\n${result.stderr || result.stdout}`);
  }
  const parsed = JSON.parse(result.stdout);
  if (parsed.status !== "success") {
    throw new Error(`admission token command returned ${parsed.status}`);
  }
  return parsed;
}

function verifyAdmission(envelope, input) {
  const { token, token_digest, money_movement_id } = envelope.result;
  const moneyMovementId = deriveMoneyMovementId(input);
  assertEqual(money_movement_id, moneyMovementId, "money_movement_id matches stable TS derivation");
  assertEqual(token.money_movement_id, moneyMovementId, "token money_movement_id matches stable TS derivation");
  if (requireDigestParity) {
    assertEqual(moneyMovementId, expectedMoneyMovementId, "money_movement_id matches pinned fixture");
    assertEqual(token_digest, expectedTokenDigest, "token_digest matches pinned fixture");
  }
  const unsigned = { ...token };
  delete unsigned.sig;
  const signature = Buffer.from(stripPrefix(token.sig, "base64:"), "base64url");
  const verified = verifyEd25519Signature(
    null,
    Buffer.from(canonicalJson(unsigned), "utf8"),
    ed25519PublicKeyFromRaw(Buffer.from(zeroSeedPublicKey, "base64")),
    signature,
  );
  if (!verified) {
    throw new Error("admission token signature failed standalone verification");
  }
}

function runCloudBridgeGates() {
  const tests = [
    "packages/api/src/payment-admission.test.ts",
    "packages/api/src/trust-root.test.ts",
    "packages/billing/src/index.test.ts",
    "packages/worker/src/metering.test.ts",
  ];
  const nameFilter = requireRecovery
    ? "payment admission|hosted trust root|topup is receipt-before-credit|hosted-run metering"
    : "payment admission|hosted trust root|hosted-run metering";
  const result = spawnSync("pnpm", [
    "--dir",
    "cloud",
    "exec",
    "vitest",
    "run",
    ...tests,
    "-t",
    nameFilter,
    "--maxWorkers=4",
    "--testTimeout=30000",
    "--hookTimeout=30000",
    "--teardownTimeout=30000",
  ], {
    cwd: repoRoot,
    stdio: "inherit",
  });
  if (result.status !== 0) {
    throw new Error(`cloud bridge spike tests failed with exit ${result.status}`);
  }
}

function deriveMoneyMovementId(input) {
  const preimage = {
    act: input.act,
    amount_minor: input.amount_minor,
    authority_digest: input.authority_digest,
    counterparty: input.counterparty,
    currency: input.currency,
    principal: input.principal,
    rail: input.rail,
    run_id: input.run_id,
  };
  return `sha256:${createHash("sha256").update(`runx.money_movement.v1\n${canonicalJson(preimage)}`).digest("hex")}`;
}

function canonicalJson(value) {
  if (value === null || typeof value !== "object") {
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) {
    return `[${value.map((item) => canonicalJson(item)).join(",")}]`;
  }
  return `{${Object.keys(value)
    .filter((key) => value[key] !== undefined)
    .sort()
    .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`)
    .join(",")}}`;
}

function ed25519PublicKeyFromRaw(raw) {
  if (raw.length !== 32) {
    throw new Error("expected raw Ed25519 public key");
  }
  return createPublicKey({
    key: Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), raw]),
    format: "der",
    type: "spki",
  });
}

function stripPrefix(value, prefix) {
  return value.startsWith(prefix) ? value.slice(prefix.length) : value;
}

function assertEqual(actual, expected, label) {
  if (actual !== expected) {
    throw new Error(`${label}: expected ${expected}, got ${actual}`);
  }
}
