use std::collections::BTreeMap;

use runx_contracts::{HarnessReceipt, JsonObject, JsonValue};
use runx_runtime::RUNX_RECEIPT_DIR_ENV;
use runx_runtime::payment_state::{
    FileBackedPaymentStateStore, PaymentIdempotencyEntry, PaymentIdempotencyKey,
    PaymentRecoveryState, PaymentStepStateInput, RUNX_PAYMENT_STATE_PATH_ENV, RailMutation,
    RailMutationStatus, SpendCapabilityConsumption, consumed_spend_capability_recorded,
    escalate_payment_rail_mutation, lookup_payment_idempotency_entry, lookup_payment_rail_mutation,
    persist_payment_step_state,
};
use runx_runtime::payment_supervisor::{
    PAYMENT_RAIL_SUPERVISOR_VERIFIER_ID, PaymentSupervisorProof,
};
use runx_runtime::receipts::step_receipt;
use runx_runtime::{InvocationStatus, SkillOutput};
use serde_json::json;

#[test]
fn persists_payment_state_across_fresh_store() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("payment-state.json");
    let idempotency_key =
        PaymentIdempotencyKey::new("mock", "merchant:paid-echo", "payment:paid-echo-001");

    {
        let mut store = FileBackedPaymentStateStore::open(&path)?;
        store.record_idempotency(PaymentIdempotencyEntry {
            idempotency_key: idempotency_key.clone(),
            receipt_ref: "receipt:paid-echo:first".to_owned(),
            receipt_created_at: "2026-05-18T00:00:00Z".to_owned(),
            receipt_digest: "sha256:receipt-paid-echo-first".to_owned(),
            rail_proof_ref: "receipt-proof:mock:paid-echo-001".to_owned(),
            supervisor_proof: supervisor_proof_for_fields(
                "receipt-proof:mock:paid-echo-001",
                "receipt:paid-echo:first",
                "sha256:receipt-paid-echo-first",
            ),
            amount_minor: 125,
            currency: "USD".to_owned(),
            outputs: JsonObject::new(),
        })?;
        store.record_rail_mutation(RailMutation {
            idempotency_key: idempotency_key.clone(),
            rail: "mock".to_owned(),
            amount_minor: 125,
            currency: "USD".to_owned(),
            counterparty: "merchant:paid-echo".to_owned(),
            status: RailMutationStatus::Fulfilled,
            proof_ref: Some("receipt-proof:mock:paid-echo-001".to_owned()),
            recovery_state: PaymentRecoveryState::Sealed,
        })?;
    }

    let store = FileBackedPaymentStateStore::open(&path)?;
    let entry = store
        .lookup_idempotency(&idempotency_key)
        .ok_or("idempotency entry should survive fresh store open")?;
    assert_eq!(entry.receipt_ref, "receipt:paid-echo:first");
    assert_eq!(entry.rail_proof_ref, "receipt-proof:mock:paid-echo-001");

    let mutation = store
        .lookup_rail_mutation(&idempotency_key)
        .ok_or("rail mutation should survive fresh store open")?;
    assert_eq!(mutation.status, RailMutationStatus::Fulfilled);
    assert_eq!(mutation.recovery_state, PaymentRecoveryState::Sealed);

    Ok(())
}

#[test]
fn opens_v2_payment_state_fail_closed_without_replay_entries()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("payment-state.json");
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&json!({
            "schema_version": "runx.payment_state.v2",
            "idempotency_entries": {
                "mock\u{1f}merchant:paid-echo\u{1f}payment:paid-echo-001": {
                    "idempotency_key": {
                        "rail": "mock",
                        "counterparty": "merchant:paid-echo",
                        "key": "payment:paid-echo-001"
                    },
                    "receipt_ref": "receipt:legacy",
                    "rail_proof_ref": "proof:legacy",
                    "amount_minor": 125,
                    "currency": "USD"
                }
            },
            "consumed_spend_capabilities": {
                "runx:payment-capability:paid-echo-spend-1": {
                    "capability_ref": "runx:payment-capability:paid-echo-spend-1",
                    "idempotency_key": {
                        "rail": "mock",
                        "counterparty": "merchant:paid-echo",
                        "key": "payment:paid-echo-001"
                    },
                    "receipt_ref": "receipt:legacy",
                    "recovery_state": "sealed"
                }
            },
            "rail_mutations": {}
        }))?,
    )?;

    let idempotency_key =
        PaymentIdempotencyKey::new("mock", "merchant:paid-echo", "payment:paid-echo-001");
    let store = FileBackedPaymentStateStore::open(&path)?;
    assert!(
        store.lookup_idempotency(&idempotency_key).is_none(),
        "v2 sealed idempotency entries lack replay-safe outputs and must not replay"
    );
    assert!(
        store
            .lookup_consumed_spend_capability("runx:payment-capability:paid-echo-spend-1")
            .is_some(),
        "v2 consumed capability state must remain fail-closed"
    );
    Ok(())
}

#[test]
fn records_consumed_spend_capability_for_reuse_lookup() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("nested").join("payment-state.json");
    let idempotency_key =
        PaymentIdempotencyKey::new("mock", "merchant:paid-echo", "payment:paid-echo-001");
    let capability_ref = "runx:payment-capability:paid-echo-spend-1";

    let mut store = FileBackedPaymentStateStore::open(&path)?;
    store.consume_spend_capability(SpendCapabilityConsumption {
        capability_ref: capability_ref.to_owned(),
        idempotency_key: idempotency_key.clone(),
        receipt_ref: Some("receipt:paid-echo:first".to_owned()),
        recovery_state: Some(PaymentRecoveryState::Sealed),
    })?;

    let store = FileBackedPaymentStateStore::open(&path)?;
    let consumed = store
        .lookup_consumed_spend_capability(capability_ref)
        .ok_or("consumed spend capability should be persisted")?;
    assert_eq!(consumed.idempotency_key, idempotency_key);
    assert_eq!(
        consumed.receipt_ref.as_deref(),
        Some("receipt:paid-echo:first")
    );

    Ok(())
}

#[test]
fn rejects_duplicate_spend_capability_consumption() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("payment-state.json");
    let idempotency_key =
        PaymentIdempotencyKey::new("mock", "merchant:paid-echo", "payment:paid-echo-001");
    let capability_ref = "runx:payment-capability:paid-echo-spend-1";
    let consumption = SpendCapabilityConsumption {
        capability_ref: capability_ref.to_owned(),
        idempotency_key,
        receipt_ref: Some("receipt:paid-echo:first".to_owned()),
        recovery_state: Some(PaymentRecoveryState::Sealed),
    };

    let mut store = FileBackedPaymentStateStore::open(&path)?;
    store.consume_spend_capability(consumption.clone())?;

    let error = store
        .consume_spend_capability(consumption)
        .err()
        .ok_or("duplicate spend capability should be rejected")?;
    assert_eq!(
        error.to_string(),
        "spend capability runx:payment-capability:paid-echo-spend-1 was already consumed"
    );

    Ok(())
}

#[test]
fn rejects_duplicate_idempotency_and_rail_mutation_without_overwrite()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("payment-state.json");
    let idempotency_key =
        PaymentIdempotencyKey::new("mock", "merchant:paid-echo", "payment:paid-echo-001");
    let mut store = FileBackedPaymentStateStore::open(&path)?;

    store.record_idempotency(PaymentIdempotencyEntry {
        idempotency_key: idempotency_key.clone(),
        receipt_ref: "receipt:first".to_owned(),
        receipt_created_at: "2026-05-18T00:00:00Z".to_owned(),
        receipt_digest: "sha256:receipt-first".to_owned(),
        rail_proof_ref: "proof:first".to_owned(),
        supervisor_proof: supervisor_proof_for_fields(
            "proof:first",
            "receipt:first",
            "sha256:receipt-first",
        ),
        amount_minor: 125,
        currency: "USD".to_owned(),
        outputs: JsonObject::new(),
    })?;
    let idempotency_error = store
        .record_idempotency(PaymentIdempotencyEntry {
            idempotency_key: idempotency_key.clone(),
            receipt_ref: "receipt:second".to_owned(),
            receipt_created_at: "2026-05-18T00:00:00Z".to_owned(),
            receipt_digest: "sha256:receipt-second".to_owned(),
            rail_proof_ref: "proof:second".to_owned(),
            supervisor_proof: supervisor_proof_for_fields(
                "proof:second",
                "receipt:second",
                "sha256:receipt-second",
            ),
            amount_minor: 250,
            currency: "USD".to_owned(),
            outputs: JsonObject::new(),
        })
        .err()
        .ok_or("duplicate idempotency record should be rejected")?;
    assert_eq!(
        idempotency_error.to_string(),
        "idempotency key mock\u{1f}merchant:paid-echo\u{1f}payment:paid-echo-001 was already recorded"
    );

    let stored = store
        .lookup_idempotency(&idempotency_key)
        .ok_or("original idempotency entry should remain stored")?;
    assert_eq!(stored.receipt_ref, "receipt:first");

    store.record_rail_mutation(RailMutation {
        idempotency_key: idempotency_key.clone(),
        rail: "mock".to_owned(),
        amount_minor: 125,
        currency: "USD".to_owned(),
        counterparty: "merchant:paid-echo".to_owned(),
        status: RailMutationStatus::Partial,
        proof_ref: None,
        recovery_state: PaymentRecoveryState::InFlight,
    })?;
    let mutation_error = store
        .record_rail_mutation(RailMutation {
            idempotency_key: idempotency_key.clone(),
            rail: "mock".to_owned(),
            amount_minor: 250,
            currency: "USD".to_owned(),
            counterparty: "merchant:paid-echo".to_owned(),
            status: RailMutationStatus::Fulfilled,
            proof_ref: Some("proof:second".to_owned()),
            recovery_state: PaymentRecoveryState::Sealed,
        })
        .err()
        .ok_or("duplicate rail mutation should be rejected")?;
    assert_eq!(
        mutation_error.to_string(),
        "rail mutation for idempotency key mock\u{1f}merchant:paid-echo\u{1f}payment:paid-echo-001 was already recorded"
    );

    let mutation = store
        .lookup_rail_mutation(&idempotency_key)
        .ok_or("original rail mutation should remain stored")?;
    assert_eq!(mutation.status, RailMutationStatus::Partial);
    assert_eq!(mutation.recovery_state, PaymentRecoveryState::InFlight);

    Ok(())
}

#[test]
fn persists_sealed_payment_step_state_for_replay_and_reuse_lookups()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let graph_dir = temp.path().join("graph");
    std::fs::create_dir(&graph_dir)?;
    let mut env = BTreeMap::new();
    env.insert(RUNX_RECEIPT_DIR_ENV.to_owned(), "receipts".to_owned());
    let input = payment_step_input();

    let outputs = sealed_payment_outputs("receipt-proof:mock:paid-echo-001", 125)?;
    let receipt = receipt_for_outputs("x402-pay-idempotency-replay", "fulfill", &outputs)?;
    let supervisor_proof =
        supervisor_proof_for_receipt(&input, "receipt-proof:mock:paid-echo-001", &receipt);
    persist_payment_step_state(
        &env,
        &graph_dir,
        &input,
        &outputs,
        &receipt,
        Some(&supervisor_proof),
    )?;

    let entry = lookup_payment_idempotency_entry(&env, &graph_dir, &input.idempotency_key)?
        .ok_or("sealed idempotency entry should be available through public lookup")?;
    assert_eq!(
        entry.receipt_ref,
        "hrn_rcpt_x402-pay-idempotency-replay_fulfill"
    );
    assert_eq!(entry.rail_proof_ref, "receipt-proof:mock:paid-echo-001");
    assert_eq!(entry.receipt_created_at, receipt.created_at);
    assert_eq!(entry.receipt_digest, receipt.seal.digest);
    let entry_text = serde_json::to_string(&entry)?;
    assert!(
        !entry_text.contains("rail_session_material_ref"),
        "replay state must not persist rail session material"
    );
    assert!(consumed_spend_capability_recorded(
        &env,
        &graph_dir,
        &input.spend_capability_ref
    )?);

    let store =
        FileBackedPaymentStateStore::open(graph_dir.join("receipts").join("payment-state.json"))?;
    let mutation = store
        .lookup_rail_mutation(&input.idempotency_key)
        .ok_or("sealed rail mutation should be persisted")?;
    assert_eq!(mutation.status, RailMutationStatus::Fulfilled);
    assert_eq!(mutation.recovery_state, PaymentRecoveryState::Sealed);
    assert_eq!(
        mutation.proof_ref.as_deref(),
        Some("receipt-proof:mock:paid-echo-001")
    );

    Ok(())
}

#[test]
fn payment_step_state_persistence_keeps_first_sealed_record()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let mut env = BTreeMap::new();
    env.insert(
        RUNX_PAYMENT_STATE_PATH_ENV.to_owned(),
        "payment-state.json".to_owned(),
    );
    let input = payment_step_input();

    let first_outputs = sealed_payment_outputs("receipt-proof:mock:first", 125)?;
    let first_receipt = receipt_for_outputs("first", "fulfill", &first_outputs)?;
    let first_supervisor_proof =
        supervisor_proof_for_receipt(&input, "receipt-proof:mock:first", &first_receipt);
    persist_payment_step_state(
        &env,
        temp.path(),
        &input,
        &first_outputs,
        &first_receipt,
        Some(&first_supervisor_proof),
    )?;
    let second_outputs = sealed_payment_outputs("receipt-proof:mock:second", 250)?;
    let second_receipt = receipt_for_outputs("second", "fulfill", &second_outputs)?;
    let mut second_supervisor_proof =
        supervisor_proof_for_receipt(&input, "receipt-proof:mock:second", &second_receipt);
    second_supervisor_proof.amount_minor = 250;
    persist_payment_step_state(
        &env,
        temp.path(),
        &input,
        &second_outputs,
        &second_receipt,
        Some(&second_supervisor_proof),
    )?;

    let store = FileBackedPaymentStateStore::open(temp.path().join("payment-state.json"))?;
    let entry = store
        .lookup_idempotency(&input.idempotency_key)
        .ok_or("first idempotency entry should remain stored")?;
    assert_eq!(entry.receipt_ref, first_receipt.id);
    assert_eq!(entry.rail_proof_ref, "receipt-proof:mock:first");
    assert_eq!(entry.amount_minor, 125);

    let mutation = store
        .lookup_rail_mutation(&input.idempotency_key)
        .ok_or("first rail mutation should remain stored")?;
    assert_eq!(mutation.amount_minor, 125);
    assert_eq!(
        mutation.proof_ref.as_deref(),
        Some("receipt-proof:mock:first")
    );

    Ok(())
}

#[test]
fn stale_store_mutation_reloads_locked_state_before_writing()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("payment-state.json");
    let idempotency_key =
        PaymentIdempotencyKey::new("mock", "merchant:paid-echo", "payment:paid-echo-001");
    let mut first = FileBackedPaymentStateStore::open(&path)?;
    let mut stale_second = FileBackedPaymentStateStore::open(&path)?;

    first.record_idempotency(PaymentIdempotencyEntry {
        idempotency_key: idempotency_key.clone(),
        receipt_ref: "receipt:first".to_owned(),
        receipt_created_at: "2026-05-18T00:00:00Z".to_owned(),
        receipt_digest: "sha256:receipt-first".to_owned(),
        rail_proof_ref: "proof:first".to_owned(),
        supervisor_proof: supervisor_proof_for_fields(
            "proof:first",
            "receipt:first",
            "sha256:receipt-first",
        ),
        amount_minor: 125,
        currency: "USD".to_owned(),
        outputs: JsonObject::new(),
    })?;

    let error = stale_second
        .record_idempotency(PaymentIdempotencyEntry {
            idempotency_key: idempotency_key.clone(),
            receipt_ref: "receipt:second".to_owned(),
            receipt_created_at: "2026-05-18T00:00:00Z".to_owned(),
            receipt_digest: "sha256:receipt-second".to_owned(),
            rail_proof_ref: "proof:second".to_owned(),
            supervisor_proof: supervisor_proof_for_fields(
                "proof:second",
                "receipt:second",
                "sha256:receipt-second",
            ),
            amount_minor: 250,
            currency: "USD".to_owned(),
            outputs: JsonObject::new(),
        })
        .err()
        .ok_or("stale store must not overwrite locked payment state")?;
    assert_eq!(
        error.to_string(),
        "idempotency key mock\u{1f}merchant:paid-echo\u{1f}payment:paid-echo-001 was already recorded"
    );

    let fresh = FileBackedPaymentStateStore::open(&path)?;
    let entry = fresh
        .lookup_idempotency(&idempotency_key)
        .ok_or("first idempotency entry should remain persisted")?;
    assert_eq!(entry.receipt_ref, "receipt:first");

    Ok(())
}

#[test]
fn persists_partial_rail_mutation_for_recovery_lookup_without_sealed_idempotency()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let mut env = BTreeMap::new();
    env.insert(
        RUNX_PAYMENT_STATE_PATH_ENV.to_owned(),
        "payment-state.json".to_owned(),
    );
    let input = payment_step_input();

    let outputs = partial_payment_outputs()?;
    let receipt = receipt_for_outputs("partial", "fulfill", &outputs)?;
    persist_payment_step_state(&env, temp.path(), &input, &outputs, &receipt, None)?;

    assert!(
        lookup_payment_idempotency_entry(&env, temp.path(), &input.idempotency_key)?.is_none(),
        "partial rail mutation without proof must not be exposed as sealed replay"
    );
    assert!(consumed_spend_capability_recorded(
        &env,
        temp.path(),
        &input.spend_capability_ref
    )?);

    let store = FileBackedPaymentStateStore::open(temp.path().join("payment-state.json"))?;
    let mutation = store
        .lookup_rail_mutation(&input.idempotency_key)
        .ok_or("partial rail mutation should be persisted for recovery")?;
    assert_eq!(mutation.status, RailMutationStatus::Partial);
    assert_eq!(mutation.recovery_state, PaymentRecoveryState::InFlight);
    assert_eq!(mutation.proof_ref, None);

    let escalated = escalate_payment_rail_mutation(&env, temp.path(), &input.idempotency_key)?
        .ok_or("partial rail mutation should be escalated by public recovery helper")?;
    assert_eq!(escalated.status, RailMutationStatus::Escalated);
    assert_eq!(escalated.recovery_state, PaymentRecoveryState::Escalated);
    let looked_up = lookup_payment_rail_mutation(&env, temp.path(), &input.idempotency_key)?
        .ok_or("escalated rail mutation should remain queryable")?;
    assert_eq!(looked_up.status, RailMutationStatus::Escalated);
    assert_eq!(looked_up.recovery_state, PaymentRecoveryState::Escalated);

    Ok(())
}

fn payment_step_input() -> PaymentStepStateInput {
    PaymentStepStateInput {
        idempotency_key: PaymentIdempotencyKey::new(
            "mock",
            "merchant:paid-echo",
            "payment:paid-echo-001",
        ),
        spend_capability_ref: "runx:payment-capability:paid-echo-spend-1".to_owned(),
        rail: "mock".to_owned(),
        counterparty: "merchant:paid-echo".to_owned(),
        amount_minor: 125,
        currency: "USD".to_owned(),
        act_id: "act_fulfill".to_owned(),
    }
}

fn supervisor_proof_for_receipt(
    input: &PaymentStepStateInput,
    proof_ref: &str,
    receipt: &HarnessReceipt,
) -> PaymentSupervisorProof {
    PaymentSupervisorProof {
        verifier_id: PAYMENT_RAIL_SUPERVISOR_VERIFIER_ID.to_owned(),
        proof_ref: proof_ref.to_owned(),
        rail: input.rail.clone(),
        counterparty: input.counterparty.clone(),
        amount_minor: input.amount_minor,
        currency: input.currency.clone(),
        idempotency_key: input.idempotency_key.key.clone(),
        spend_capability_ref: input.spend_capability_ref.clone(),
        act_id: input.act_id.clone(),
        receipt_ref: receipt.id.clone(),
        receipt_digest: receipt.seal.digest.clone(),
        evidence_digest: "sha256:test-supervisor-evidence".to_owned(),
    }
}

fn supervisor_proof_for_fields(
    proof_ref: &str,
    receipt_ref: &str,
    receipt_digest: &str,
) -> PaymentSupervisorProof {
    PaymentSupervisorProof {
        verifier_id: PAYMENT_RAIL_SUPERVISOR_VERIFIER_ID.to_owned(),
        proof_ref: proof_ref.to_owned(),
        rail: "mock".to_owned(),
        counterparty: "merchant:paid-echo".to_owned(),
        amount_minor: 125,
        currency: "USD".to_owned(),
        idempotency_key: "payment:paid-echo-001".to_owned(),
        spend_capability_ref: "runx:payment-capability:paid-echo-spend-1".to_owned(),
        act_id: "act_fulfill".to_owned(),
        receipt_ref: receipt_ref.to_owned(),
        receipt_digest: receipt_digest.to_owned(),
        evidence_digest: "sha256:test-supervisor-evidence".to_owned(),
    }
}

fn sealed_payment_outputs(
    proof_ref: &str,
    amount_minor: u64,
) -> Result<JsonObject, serde_json::Error> {
    serde_json::from_value(json!({
        "payment_rail_packet": {
            "data": {
                "rail_result": {
                    "status": "fulfilled",
                    "rail": "mock",
                    "amount_minor": amount_minor,
                    "currency": "USD",
                    "counterparty": "merchant:paid-echo"
                },
                "rail_proof": {
                    "proof_ref": proof_ref,
                    "idempotency_key": "payment:paid-echo-001",
                    "rail_session_material_ref": "rail-session-material:mock:paid-echo-001"
                },
                "recovery_hint": { "status": "sealed" }
            }
        }
    }))
}

fn partial_payment_outputs() -> Result<JsonObject, serde_json::Error> {
    serde_json::from_value(json!({
        "payment_rail_packet": {
            "data": {
                "rail_result": {
                    "status": "partial",
                    "rail": "mock",
                    "amount_minor": 125,
                    "currency": "USD",
                    "counterparty": "merchant:paid-echo"
                },
                "recovery_hint": { "status": "partial" }
            }
        }
    }))
}

fn receipt_for_outputs(
    graph_name: &str,
    step_id: &str,
    outputs: &JsonObject,
) -> Result<HarnessReceipt, Box<dyn std::error::Error>> {
    let output = SkillOutput {
        status: InvocationStatus::Success,
        stdout: serde_json::to_string(&JsonValue::Object(outputs.clone()))?,
        stderr: String::new(),
        exit_code: Some(0),
        duration_ms: 1,
        metadata: JsonObject::new(),
    };
    Ok(step_receipt(
        graph_name,
        step_id,
        1,
        &output,
        "2026-05-18T00:00:00Z",
    )?)
}
