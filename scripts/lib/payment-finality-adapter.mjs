import { runAdapter } from "./external-adapter.mjs";

const DEFAULT_VERIFIER_ID = "runx.payment_rail_supervisor.local.v1";

export function runPaymentFinalityAdapter(config) {
  const rail = requiredConfigString(config, "rail");
  const label = config.label || `${rail} finality adapter`;
  const acceptedStatuses = new Set(config.acceptedStatuses || ["fulfilled"]);
  const defaultStatus = config.defaultStatus || "fulfilled";
  const proofLocatorFields = config.proofLocatorFields || [];

  runAdapter(({ inputs }) => {
    const family = optionalString(inputs, "effect_family") || "payment";
    if (family !== "payment") {
      throw new Error(`${label} expected effect_family payment, got ${family}`);
    }
    const actualRail = requiredString(inputs, "rail");
    if (actualRail !== rail) {
      throw new Error(`${label} expected rail ${rail}, got ${actualRail}`);
    }
    const status = optionalString(inputs, "skill_settlement_status") || defaultStatus;
    if (!acceptedStatuses.has(status)) {
      throw new Error(
        `${label} requires ${statusList(acceptedStatuses)} rail result, got ${status}`,
      );
    }

    const proofRef = requiredString(inputs, "proof_ref");
    const providerEventRef = firstPresentString(inputs, [
      "provider_event_ref",
      ...proofLocatorFields,
    ]) || config.proofRefProviderLocator?.(proofRef);

    return {
      payment_finality_evidence: pruneUndefined({
        verifier_id: config.verifierId || DEFAULT_VERIFIER_ID,
        proof_ref: proofRef,
        rail,
        counterparty: requiredString(inputs, "counterparty"),
        amount_minor: requiredNonNegativeInteger(inputs, "amount_minor"),
        currency: requiredString(inputs, "currency"),
        idempotency_key: requiredString(inputs, "idempotency_key"),
        payment_admission_id: requiredString(inputs, "payment_admission_id"),
        money_movement_id: requiredString(inputs, "money_movement_id"),
        kernel_token_digest: requiredString(inputs, "kernel_token_digest"),
        proof_locator: providerEventRef || proofRef,
        proof_status: status,
        settlement_status: status,
        provider_event_ref: providerEventRef,
      }),
    };
  });
}

export function requiredString(inputs, field) {
  const value = optionalString(inputs, field);
  if (!value) {
    throw new Error(`${field} is required`);
  }
  return value;
}

export function optionalString(inputs, field) {
  const value = inputs[field];
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

export function requiredNonNegativeInteger(inputs, field) {
  const value = inputs[field];
  if (typeof value === "number" && Number.isSafeInteger(value) && value >= 0) {
    return value;
  }
  throw new Error(`${field} must be a non-negative integer`);
}

function firstPresentString(inputs, fields) {
  for (const field of fields) {
    const value = optionalString(inputs, field);
    if (value) {
      return value;
    }
  }
  return undefined;
}

function pruneUndefined(value) {
  return Object.fromEntries(
    Object.entries(value).filter(([, entry]) => entry !== undefined && entry !== ""),
  );
}

function requiredConfigString(config, field) {
  const value = config[field];
  if (typeof value !== "string" || !value.trim()) {
    throw new Error(`payment finality adapter config.${field} is required`);
  }
  return value.trim();
}

function statusList(values) {
  return [...values].join(" or ");
}
