import { readFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";

import { describe, expect, it } from "vitest";

import { validateExternalAdapterManifestContract } from "../packages/contracts/src/index.js";

describe("payment finality adapters", () => {
  it("validates x402 and stripe-spt manifests", async () => {
    for (const manifestPath of [
      "scripts/x402-finality-adapter.manifest.json",
      "scripts/stripe-spt-finality-adapter.manifest.json",
      "scripts/mpp-tempo-finality-adapter.manifest.json",
      "scripts/mpp-fiat-finality-adapter.manifest.json",
    ]) {
      const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
      expect(validateExternalAdapterManifestContract(manifest).schema).toBe(
        "runx.external_adapter.manifest.v1",
      );
    }
  });

  it("emits x402 finality evidence from supervisor invocation inputs", () => {
    const proofRef = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const response = invokeAdapter("scripts/x402-finality-adapter.mjs", {
      rail: "x402",
      proof_ref: proofRef,
      tx_hash: proofRef,
      ...paymentInputs(),
    });

    expect(response.status).toBe("completed");
    expect(paymentFinalityEvidence(response)).toMatchObject({
      rail: "x402",
      proof_ref: proofRef,
      proof_locator: proofRef,
      payment_admission_id: "pa_test_1",
      money_movement_id: "mmid_test_1",
      kernel_token_digest: "sha256:kernel-token",
    });
  });

  it("emits stripe-spt finality evidence from charge and event refs", () => {
    const response = invokeAdapter("scripts/stripe-spt-finality-adapter.mjs", {
      rail: "stripe-spt",
      proof_ref: "ch_test_demo_1",
      provider_event_ref: "evt_test_demo_1",
      charge_id: "ch_test_demo_1",
      payment_intent_id: "pi_test_demo_1",
      ...paymentInputs(),
    });

    expect(response.status).toBe("completed");
    expect(paymentFinalityEvidence(response)).toMatchObject({
      rail: "stripe-spt",
      proof_ref: "ch_test_demo_1",
      provider_event_ref: "evt_test_demo_1",
      proof_locator: "evt_test_demo_1",
      amount_minor: 125,
      currency: "USD",
      idempotency_key: "payment:test-1",
      payment_admission_id: "pa_test_1",
      money_movement_id: "mmid_test_1",
      kernel_token_digest: "sha256:kernel-token",
    });
  });

  it("emits mpp-tempo finality evidence from the x402-style transaction locator", () => {
    const proofRef = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const response = invokeAdapter("scripts/mpp-tempo-finality-adapter.mjs", {
      rail: "mpp-tempo",
      proof_ref: proofRef,
      tx_hash: proofRef,
      ...paymentInputs(),
    });

    expect(response.status).toBe("completed");
    expect(paymentFinalityEvidence(response)).toMatchObject({
      rail: "mpp-tempo",
      proof_ref: proofRef,
      proof_locator: proofRef,
      payment_admission_id: "pa_test_1",
      money_movement_id: "mmid_test_1",
      kernel_token_digest: "sha256:kernel-token",
    });
  });

  it("emits mpp-fiat finality evidence from the Stripe-scoped provider event", () => {
    const response = invokeAdapter("scripts/mpp-fiat-finality-adapter.mjs", {
      rail: "mpp-fiat",
      proof_ref: "pi_test_mpp_1",
      provider_event_ref: "evt_test_mpp_1",
      payment_intent_id: "pi_test_mpp_1",
      ...paymentInputs(),
    });

    expect(response.status).toBe("completed");
    expect(paymentFinalityEvidence(response)).toMatchObject({
      rail: "mpp-fiat",
      proof_ref: "pi_test_mpp_1",
      provider_event_ref: "evt_test_mpp_1",
      proof_locator: "evt_test_mpp_1",
      amount_minor: 125,
      currency: "USD",
      idempotency_key: "payment:test-1",
      payment_admission_id: "pa_test_1",
      money_movement_id: "mmid_test_1",
      kernel_token_digest: "sha256:kernel-token",
    });
  });

  it("fails closed on rail mismatch", () => {
    const response = invokeAdapter("scripts/stripe-spt-finality-adapter.mjs", {
      rail: "x402",
      proof_ref: "ch_test_demo_1",
      ...paymentInputs(),
    });

    expect(response.status).toBe("failed");
    expect(response.stderr).toContain("expected rail stripe-spt");
  });
});

function paymentInputs(): Record<string, unknown> {
  return {
    effect_family: "payment",
    skill_settlement_status: "fulfilled",
    counterparty: "merchant:demo",
    amount_minor: 125,
    currency: "USD",
    idempotency_key: "payment:test-1",
    payment_admission_id: "pa_test_1",
    money_movement_id: "mmid_test_1",
    kernel_token_digest: "sha256:kernel-token",
  };
}

function invokeAdapter(script: string, inputs: Record<string, unknown>): Record<string, unknown> {
  const result = spawnSync(process.execPath, [script], {
    cwd: process.cwd(),
    input: JSON.stringify({
      schema: "runx.external_adapter.invocation.v1",
      protocol_version: "runx.external_adapter.v1",
      invocation_id: "payment_finality_test.invoke",
      adapter_id: adapterId(script),
      run_id: "payment_finality_test",
      step_id: "payment_finality",
      source_type: "external-adapter",
      skill_ref: "runx/payment-finality-supervisor",
      harness_ref: { type: "harness", uri: "runx:harness:payment_finality_test" },
      host_ref: { type: "host", uri: "runx:host:test" },
      inputs,
    }),
    encoding: "utf8",
  });
  expect(result.status, result.stderr).toBe(0);
  return JSON.parse(result.stdout) as Record<string, unknown>;
}

function adapterId(script: string): string {
  if (script.includes("stripe-spt")) {
    return "runx.payment_finality.stripe_spt";
  }
  if (script.includes("mpp-fiat")) {
    return "runx.payment_finality.mpp_fiat";
  }
  if (script.includes("mpp-tempo")) {
    return "runx.payment_finality.mpp_tempo";
  }
  return "runx.payment_finality.x402";
}

function paymentFinalityEvidence(response: Record<string, unknown>): Record<string, unknown> {
  return requireRecord(
    requireRecord(response.output, "response.output").payment_finality_evidence,
    "payment_finality_evidence",
  );
}

function requireRecord(value: unknown, label: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error(`${label} must be an object.`);
  }
  return value as Record<string, unknown>;
}
