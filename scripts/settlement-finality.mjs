#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

const flags = new Set(process.argv.slice(2));

function apply(store, event) {
  const eventKey = `${event.rail}\u001f${event.providerEventId}`;
  if (store.events.has(eventKey)) {
    return { status: "duplicate", record: store.records.get(event.moneyMovementId) };
  }
  const existing = store.records.get(event.moneyMovementId);
  let phase = "in_flight";
  if (event.kind === "dispute_created" || event.kind === "refund_reversed" || event.kind === "reorg") {
    phase = "reversed";
  } else if (event.kind === "provider_succeeded" || event.depth >= event.threshold) {
    phase = "sealed";
  }
  const next = existing && existing.phase === "reversed"
    ? existing
    : existing && phase === "in_flight" && existing.confirmation_depth >= event.depth
      ? existing
      : {
          money_movement_id: event.moneyMovementId,
          rail: event.rail,
          phase,
          confirmation_depth: event.depth,
          finality_threshold: event.threshold,
          original_receipt_ref: "receipt:original",
          latest_receipt_ref: event.latestReceiptRef,
          terminal_reason: phase === "reversed" ? event.kind : undefined,
        };
  store.records.set(event.moneyMovementId, next);
  store.events.set(eventKey, { ...event, result_phase: next.phase });
  return { status: "recorded", record: next };
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

function runReorg() {
  const store = { records: new Map(), events: new Map() };
  apply(store, event("evt_depth_3", "confirmation_depth", { depth: 3, latestReceiptRef: "receipt:sealed" }));
  const reversed = apply(store, event("evt_reorg_1", "reorg", { depth: 1, latestReceiptRef: "receipt:reversed" }));
  assert(reversed.record.phase === "reversed", "reorg must reverse finality below threshold");
  return reversed.record;
}

function runDispute() {
  const store = { records: new Map(), events: new Map() };
  const reversed = apply(store, {
    ...event("evt_dispute_1", "dispute_created", {
      rail: "mpp-fiat",
      depth: undefined,
      threshold: undefined,
      latestReceiptRef: "pi_123",
    }),
  });
  assert(reversed.record.phase === "reversed", "dispute webhook must reverse finality");
  return reversed.record;
}

function runOutOfOrder() {
  const store = { records: new Map(), events: new Map() };
  const depth2 = apply(store, event("evt_depth_2", "confirmation_depth", { depth: 2, latestReceiptRef: "receipt:depth-2" }));
  const stale = apply(store, event("evt_depth_1", "confirmation_depth", { depth: 1, latestReceiptRef: "receipt:depth-1" }));
  const replay = apply(store, event("evt_depth_2", "confirmation_depth", { depth: 2, latestReceiptRef: "receipt:depth-2" }));
  assert(stale.record === depth2.record, "out-of-order lower depth must not regress finality");
  assert(replay.status === "duplicate", "replayed provider event id must dedupe");
  return replay.record;
}

function event(providerEventId, kind, overrides = {}) {
  return {
    moneyMovementId: "money-movement-001",
    rail: overrides.rail ?? "mpp-tempo",
    providerEventId,
    kind,
    depth: overrides.depth ?? 0,
    threshold: overrides.threshold ?? 3,
    latestReceiptRef: overrides.latestReceiptRef ?? `receipt:${providerEventId}`,
  };
}

const results = {};
if (flags.has("--reorg")) {
  results.reorg = runReorg();
}
if (flags.has("--dispute")) {
  results.dispute = runDispute();
}
if (flags.has("--out-of-order")) {
  results.out_of_order = runOutOfOrder();
}
if (flags.has("--refund-race")) {
  results.refund_race = runRefundRace();
}
console.log(JSON.stringify({ status: "passed", results }, null, 2));

function runRefundRace() {
  const fixtureDir = path.resolve("fixtures/payment-finality/refund-admission");
  const fixtures = fs.readdirSync(fixtureDir)
    .filter((file) => file.endsWith(".json"))
    .sort()
    .map((file) => JSON.parse(fs.readFileSync(path.join(fixtureDir, file), "utf8")));
  for (const fixture of fixtures) {
    const actual = admitRefund(fixture.input);
    assert(
      JSON.stringify(actual) === JSON.stringify(fixture.expected),
      `refund fixture ${fixture.name} mismatch`,
    );
  }
  const race = fixtures.find((fixture) => fixture.name === "reversed_race_refused");
  assert(race?.expected?.code === "charge_reversed", "refund-vs-Reversed race must refuse with charge_reversed");
  return { fixtures: fixtures.length, race: race.expected };
}

function admitRefund(input) {
  if (input.charge.phase === "reversed") {
    return refused("charge_reversed", "refund refused because the linked charge is already reversed");
  }
  if (input.charge.phase !== "sealed") {
    return refused("charge_not_sealed", "refund refused because the linked charge is not sealed");
  }
  if (input.refund.amount_minor <= 0) {
    return refused("empty_refund", "refund amount must be positive");
  }
  if (input.refund.amount_minor > input.charge.amount_minor) {
    return refused("refund_exceeds_charge", "refund amount exceeds the linked charge");
  }
  if (input.refund.requested_counterparty && input.refund.requested_counterparty !== input.charge.payer_ref) {
    return refused("counterparty_mismatch", "refund reversal must target the recorded payer");
  }
  return {
    status: "admitted",
    reversal: {
      rail: input.charge.rail,
      amount_minor: input.refund.amount_minor,
      currency: input.charge.currency,
      counterparty: input.charge.payer_ref,
      original_money_movement_id: input.charge.money_movement_id,
      original_proof_ref: input.charge.proof_ref,
    },
  };
}

function refused(code, reason) {
  return { status: "refused", code, reason };
}
