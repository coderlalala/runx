use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use runx_contracts::EffectSettlementPhase;
use runx_contracts::{JsonObject, JsonValue, Receipt};
use runx_pay::PAYMENT_EFFECT_FAMILY;
use runx_pay::state::{
    EffectCapabilityConsumption, EffectIdempotencyEntry, EffectIdempotencyKey, EffectMutation,
    EffectMutationStatus, EffectRecoveryState, EffectRunSpendReservation,
    EffectSettlementEventRecord, EffectSettlementFinalityRecord, EffectSettlementIntent,
    EffectSettlementIntentStatus, EffectStepStateInput, FileBackedEffectStateStore,
    RUNX_EFFECT_STATE_PATH_ENV, consumed_spend_capability_recorded, escalate_effect_mutation,
    lookup_effect_idempotency_entry, lookup_effect_mutation, persist_effect_step_state,
    record_effect_settlement_intent,
};
use runx_pay::supervisor::{PAYMENT_RAIL_SUPERVISOR_VERIFIER_ID, PaymentSupervisorProof};
use runx_runtime::RUNX_RECEIPT_DIR_ENV;
use runx_runtime::receipts::step_receipt;
use runx_runtime::{InvocationStatus, SkillOutput};
use serde_json::json;

const MESSAGE_EFFECT_FAMILY: &str = "message";

#[test]
fn records_settlement_intent_before_rail_mutation() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let env = BTreeMap::from([(
        RUNX_EFFECT_STATE_PATH_ENV.to_owned(),
        temp.path().join("effect-state.json").display().to_string(),
    )]);
    let idempotency_key = EffectIdempotencyKey::new("mock", "merchant:paid-echo", "money-move-001");
    let input = EffectStepStateInput {
        idempotency_key: idempotency_key.clone(),
        act_id: "act_pay".to_owned(),
        ..payment_step_input()
    };

    record_effect_settlement_intent(&env, temp.path(), &input)?;
    // Admission retries are safe: the same intent is idempotent, not a second
    // mutation and not a conflict.
    record_effect_settlement_intent(&env, temp.path(), &input)?;

    let store = FileBackedEffectStateStore::open(temp.path().join("effect-state.json"))?;
    assert_eq!(
        store.lookup_settlement_intent(PAYMENT_EFFECT_FAMILY, &idempotency_key),
        Some(&EffectSettlementIntent {
            idempotency_key,
            rail: "mock".to_owned(),
            amount_minor: 125,
            currency: "USD".to_owned(),
            counterparty: "merchant:paid-echo".to_owned(),
            spend_capability_ref: "runx:payment-capability:paid-echo-spend-1".to_owned(),
            act_id: "act_pay".to_owned(),
            status: EffectSettlementIntentStatus::Open,
        })
    );
    Ok(())
}

#[test]
fn reserves_run_spend_and_refuses_over_aggregate_cap() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let state_path = temp.path().join("effect-state.json");
    let env = BTreeMap::from([(
        RUNX_EFFECT_STATE_PATH_ENV.to_owned(),
        state_path.display().to_string(),
    )]);

    let first = run_spend_input("payment:run-cap-001", 125, 250);
    let second = run_spend_input("payment:run-cap-002", 125, 250);
    let third = run_spend_input("payment:run-cap-003", 1, 250);

    record_effect_settlement_intent(&env, temp.path(), &first)?;
    record_effect_settlement_intent(&env, temp.path(), &second)?;
    let error = record_effect_settlement_intent(&env, temp.path(), &third)
        .expect_err("third under-call act must be refused at aggregate run cap");

    assert!(
        error.to_string().contains("would exceed max_per_run_minor"),
        "unexpected error: {error}"
    );
    let state = std::fs::read_to_string(state_path)?;
    assert!(state.contains("\"reserved_minor\": 250"));
    assert!(!state.contains("payment:run-cap-003"));

    Ok(())
}

#[test]
fn run_spend_reservation_is_idempotent_for_same_key() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let state_path = temp.path().join("effect-state.json");
    let env = BTreeMap::from([(
        RUNX_EFFECT_STATE_PATH_ENV.to_owned(),
        state_path.display().to_string(),
    )]);
    let input = run_spend_input("payment:run-cap-replay", 125, 125);

    record_effect_settlement_intent(&env, temp.path(), &input)?;
    record_effect_settlement_intent(&env, temp.path(), &input)?;

    let state = std::fs::read_to_string(state_path)?;
    assert!(state.contains("\"reserved_minor\": 125"));

    Ok(())
}

#[test]
fn old_effect_state_without_run_spend_ledger_loads_and_writes_back()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let state_path = temp.path().join("effect-state.json");
    std::fs::write(
        &state_path,
        r#"{
  "schema_version": "runx.effect_state.v1",
  "families": {
    "payment": {
      "settlement_intents": {},
      "idempotency_entries": {},
      "consumed_spend_capabilities": {},
      "rail_mutations": {}
    }
  }
}
"#,
    )?;

    let mut store = FileBackedEffectStateStore::open(&state_path)?;
    store.record_settlement_intent(
        PAYMENT_EFFECT_FAMILY,
        EffectSettlementIntent {
            idempotency_key: payment_step_input().idempotency_key,
            rail: "mock".to_owned(),
            amount_minor: 125,
            currency: "USD".to_owned(),
            counterparty: "merchant:paid-echo".to_owned(),
            spend_capability_ref: "runx:payment-capability:paid-echo-spend-1".to_owned(),
            act_id: "act_fulfill".to_owned(),
            status: EffectSettlementIntentStatus::Open,
        },
        None,
    )?;

    let written = std::fs::read_to_string(state_path)?;
    assert!(
        written.contains("\"run_spend_ledger\": {}"),
        "old state documents must round-trip with the additive run spend ledger field"
    );

    Ok(())
}

#[test]
fn old_effect_state_without_finality_maps_loads_and_writes_back()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let state_path = temp.path().join("effect-state.json");
    std::fs::write(
        &state_path,
        r#"{
  "schema_version": "runx.effect_state.v1",
  "families": {
    "payment": {
      "settlement_intents": {
        "intent": {
          "idempotency_key": { "rail": "mock", "counterparty": "merchant", "key": "key" },
          "rail": "mock",
          "amount_minor": 125,
          "currency": "USD",
          "counterparty": "merchant",
          "spend_capability_ref": "capability",
          "act_id": "act_pay",
          "status": "open"
        }
      },
      "run_spend_ledger": {},
      "idempotency_entries": {},
      "consumed_spend_capabilities": {},
      "rail_mutations": {}
    }
  }
}
"#,
    )?;

    let mut store = FileBackedEffectStateStore::open(&state_path)?;
    let record = finality_record(
        "money-movement-001",
        EffectSettlementPhase::InFlight,
        Some(1),
        "receipt:settlement:in-flight",
    );
    store.record_settlement_finality(PAYMENT_EFFECT_FAMILY, record.clone())?;
    store.record_settlement_event(
        PAYMENT_EFFECT_FAMILY,
        EffectSettlementEventRecord {
            provider_event_id: "evt_depth_1".to_owned(),
            rail: "mpp-tempo".to_owned(),
            event_kind: "confirmation_depth".to_owned(),
            received_at: "2026-06-01T00:00:10Z".to_owned(),
            signature_digest: "sha256:event-depth-1".to_owned(),
            settlement_key: record.money_movement_id.clone(),
            result_phase: EffectSettlementPhase::InFlight,
        },
    )?;

    let reopened = FileBackedEffectStateStore::open(&state_path)?;
    assert_eq!(
        reopened.lookup_settlement_finality(PAYMENT_EFFECT_FAMILY, "money-movement-001"),
        Some(&record)
    );
    assert!(
        reopened
            .lookup_settlement_event(PAYMENT_EFFECT_FAMILY, "mpp-tempo", "evt_depth_1")
            .is_some()
    );
    let written: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(state_path)?)?;
    let family = written
        .get("families")
        .and_then(|families| families.get("payment"))
        .ok_or("missing payment family")?;
    assert!(family.get("settlement_finality").is_some());
    assert!(family.get("settlement_events").is_some());
    assert!(family.get("settlement_intents").is_some());
    assert!(family.get("idempotency_entries").is_some());
    assert!(family.get("consumed_spend_capabilities").is_some());
    assert!(family.get("run_spend_ledger").is_some());
    assert!(family.get("rail_mutations").is_some());

    Ok(())
}

#[test]
fn finality_records_update_depth_and_reject_immutable_conflicts()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("effect-state.json");
    let mut store = FileBackedEffectStateStore::open(&path)?;
    let first = finality_record(
        "money-movement-001",
        EffectSettlementPhase::InFlight,
        Some(1),
        "receipt:settlement:depth-1",
    );
    let sealed = finality_record(
        "money-movement-001",
        EffectSettlementPhase::Sealed,
        Some(3),
        "receipt:settlement:sealed",
    );

    store.record_settlement_finality(PAYMENT_EFFECT_FAMILY, first)?;
    store.record_settlement_finality(PAYMENT_EFFECT_FAMILY, sealed.clone())?;
    assert_eq!(
        store
            .lookup_settlement_finality(PAYMENT_EFFECT_FAMILY, "money-movement-001")
            .map(|record| (&record.phase, record.confirmation_depth)),
        Some((&EffectSettlementPhase::Sealed, Some(3)))
    );

    let mut conflict = sealed;
    conflict.rail = "stripe-spt".to_owned();
    let error = store
        .record_settlement_finality(PAYMENT_EFFECT_FAMILY, conflict)
        .expect_err("same money movement on a different rail must conflict");
    assert!(
        error
            .to_string()
            .contains("conflicts with existing finality state")
    );

    Ok(())
}

#[test]
fn finality_events_are_idempotent_and_conflict_on_drift() -> Result<(), Box<dyn std::error::Error>>
{
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("effect-state.json");
    let mut store = FileBackedEffectStateStore::open(&path)?;
    let event = EffectSettlementEventRecord {
        provider_event_id: "evt_depth_1".to_owned(),
        rail: "mpp-tempo".to_owned(),
        event_kind: "confirmation_depth".to_owned(),
        received_at: "2026-06-01T00:00:10Z".to_owned(),
        signature_digest: "sha256:event-depth-1".to_owned(),
        settlement_key: "money-movement-001".to_owned(),
        result_phase: EffectSettlementPhase::InFlight,
    };

    store.record_settlement_event(PAYMENT_EFFECT_FAMILY, event.clone())?;
    store.record_settlement_event(PAYMENT_EFFECT_FAMILY, event.clone())?;
    let mut drifted = event;
    drifted.result_phase = EffectSettlementPhase::Reversed;
    let error = store
        .record_settlement_event(PAYMENT_EFFECT_FAMILY, drifted)
        .expect_err("same provider event id cannot change result");
    assert!(
        error
            .to_string()
            .contains("conflicts with existing event state")
    );

    Ok(())
}

#[test]
fn repair_script_strips_only_run_spend_ledger() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let state_path = temp.path().join("effect-state.json");
    let original = json!({
        "schema_version": "runx.effect_state.v1",
        "families": {
            "payment": {
                "settlement_intents": {
                    "intent": { "kept": "settlement_intents" }
                },
                "run_spend_ledger": {
                    "ledger": { "removed": true }
                },
                "idempotency_entries": {
                    "idempotency": { "kept": "idempotency_entries" }
                },
                "consumed_spend_capabilities": {
                    "capability": { "kept": "consumed_spend_capabilities" }
                },
                "rail_mutations": {
                    "mutation": { "kept": "rail_mutations" }
                }
            }
        }
    });
    std::fs::write(&state_path, serde_json::to_string_pretty(&original)?)?;
    let script_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/repair-effect-state.mjs")
        .canonicalize()?;

    let output = Command::new("node")
        .arg(script_path)
        .arg("--strip-run-spend-ledger")
        .arg("--path")
        .arg(&state_path)
        .output()?;
    assert!(
        output.status.success(),
        "repair script failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let repaired: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(state_path)?)?;
    let payment = repaired
        .get("families")
        .and_then(|families| families.get("payment"))
        .and_then(|family| family.as_object())
        .ok_or("missing payment family after repair")?;
    assert!(!payment.contains_key("run_spend_ledger"));
    for key in [
        "settlement_intents",
        "idempotency_entries",
        "consumed_spend_capabilities",
        "rail_mutations",
    ] {
        assert_eq!(
            payment.get(key),
            original
                .get("families")
                .and_then(|families| families.get("payment"))
                .and_then(|family| family.get(key)),
            "repair script must preserve {key}"
        );
    }

    Ok(())
}

#[test]
fn repair_script_strips_only_finality_ledger() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let state_path = temp.path().join("effect-state.json");
    let original = json!({
        "schema_version": "runx.effect_state.v1",
        "families": {
            "payment": {
                "settlement_intents": {
                    "intent": { "kept": "settlement_intents" }
                },
                "settlement_finality": {
                    "finality": { "removed": true }
                },
                "settlement_events": {
                    "event": { "removed": true }
                },
                "run_spend_ledger": {
                    "ledger": { "kept": "run_spend_ledger" }
                },
                "idempotency_entries": {
                    "idempotency": { "kept": "idempotency_entries" }
                },
                "consumed_spend_capabilities": {
                    "capability": { "kept": "consumed_spend_capabilities" }
                },
                "rail_mutations": {
                    "mutation": { "kept": "rail_mutations" }
                }
            }
        }
    });
    std::fs::write(&state_path, serde_json::to_string_pretty(&original)?)?;
    let script_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/repair-effect-state.mjs")
        .canonicalize()?;

    let output = Command::new("node")
        .arg(script_path)
        .arg("--strip-finality-ledger")
        .arg("--path")
        .arg(&state_path)
        .output()?;
    assert!(
        output.status.success(),
        "repair script failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let repaired: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(state_path)?)?;
    let payment = repaired
        .get("families")
        .and_then(|families| families.get("payment"))
        .and_then(|family| family.as_object())
        .ok_or("missing payment family after repair")?;
    assert!(!payment.contains_key("settlement_finality"));
    assert!(!payment.contains_key("settlement_events"));
    for key in [
        "settlement_intents",
        "run_spend_ledger",
        "idempotency_entries",
        "consumed_spend_capabilities",
        "rail_mutations",
    ] {
        assert_eq!(
            payment.get(key),
            original
                .get("families")
                .and_then(|families| families.get("payment"))
                .and_then(|family| family.get(key)),
            "repair script must preserve {key}"
        );
    }

    Ok(())
}

#[test]
fn persists_effect_state_across_fresh_store() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("effect-state.json");
    let idempotency_key =
        EffectIdempotencyKey::new("mock", "merchant:paid-echo", "payment:paid-echo-001");

    {
        let mut store = FileBackedEffectStateStore::open(&path)?;
        store.record_idempotency(
            PAYMENT_EFFECT_FAMILY,
            EffectIdempotencyEntry {
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
            },
        )?;
        store.record_mutation(
            PAYMENT_EFFECT_FAMILY,
            EffectMutation {
                idempotency_key: idempotency_key.clone(),
                rail: "mock".to_owned(),
                amount_minor: 125,
                currency: "USD".to_owned(),
                counterparty: "merchant:paid-echo".to_owned(),
                status: EffectMutationStatus::Fulfilled,
                proof_ref: Some("receipt-proof:mock:paid-echo-001".to_owned()),
                recovery_state: EffectRecoveryState::Sealed,
            },
        )?;
    }

    let store = FileBackedEffectStateStore::open(&path)?;
    let entry = store
        .lookup_idempotency(PAYMENT_EFFECT_FAMILY, &idempotency_key)
        .ok_or("idempotency entry should survive fresh store open")?;
    assert_eq!(entry.receipt_ref, "receipt:paid-echo:first");
    assert_eq!(entry.rail_proof_ref, "receipt-proof:mock:paid-echo-001");

    let mutation = store
        .lookup_mutation(PAYMENT_EFFECT_FAMILY, &idempotency_key)
        .ok_or("rail mutation should survive fresh store open")?;
    assert_eq!(mutation.status, EffectMutationStatus::Fulfilled);
    assert_eq!(mutation.recovery_state, EffectRecoveryState::Sealed);

    Ok(())
}

#[test]
fn effect_state_namespaces_idempotency_by_family() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("effect-state.json");
    let idempotency_key =
        EffectIdempotencyKey::new("mock", "merchant:paid-echo", "payment:paid-echo-001");
    let mut store = FileBackedEffectStateStore::open(&path)?;

    store.record_idempotency(
        PAYMENT_EFFECT_FAMILY,
        EffectIdempotencyEntry {
            idempotency_key: idempotency_key.clone(),
            receipt_ref: "receipt:payment".to_owned(),
            receipt_created_at: "2026-05-18T00:00:00Z".to_owned(),
            receipt_digest: "sha256:receipt-payment".to_owned(),
            rail_proof_ref: "proof:payment".to_owned(),
            supervisor_proof: supervisor_proof_for_fields(
                "proof:payment",
                "receipt:payment",
                "sha256:receipt-payment",
            ),
            amount_minor: 125,
            currency: "USD".to_owned(),
            outputs: JsonObject::new(),
        },
    )?;
    store.record_idempotency(
        MESSAGE_EFFECT_FAMILY,
        EffectIdempotencyEntry {
            idempotency_key: idempotency_key.clone(),
            receipt_ref: "receipt:message".to_owned(),
            receipt_created_at: "2026-05-18T00:00:00Z".to_owned(),
            receipt_digest: "sha256:receipt-message".to_owned(),
            rail_proof_ref: "proof:message".to_owned(),
            supervisor_proof: supervisor_proof_for_fields(
                "proof:message",
                "receipt:message",
                "sha256:receipt-message",
            ),
            amount_minor: 125,
            currency: "USD".to_owned(),
            outputs: JsonObject::new(),
        },
    )?;

    let store = FileBackedEffectStateStore::open(&path)?;
    assert_eq!(
        store
            .lookup_idempotency(PAYMENT_EFFECT_FAMILY, &idempotency_key)
            .map(|entry| entry.receipt_ref.as_str()),
        Some("receipt:payment")
    );
    assert_eq!(
        store
            .lookup_idempotency(MESSAGE_EFFECT_FAMILY, &idempotency_key)
            .map(|entry| entry.receipt_ref.as_str()),
        Some("receipt:message")
    );

    Ok(())
}

#[test]
fn records_consumed_spend_capability_for_reuse_lookup() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("nested").join("effect-state.json");
    let idempotency_key =
        EffectIdempotencyKey::new("mock", "merchant:paid-echo", "payment:paid-echo-001");
    let capability_ref = "runx:payment-capability:paid-echo-spend-1";

    let mut store = FileBackedEffectStateStore::open(&path)?;
    store.consume_spend_capability(
        PAYMENT_EFFECT_FAMILY,
        EffectCapabilityConsumption {
            capability_ref: capability_ref.to_owned(),
            idempotency_key: idempotency_key.clone(),
            receipt_ref: Some("receipt:paid-echo:first".to_owned()),
            recovery_state: Some(EffectRecoveryState::Sealed),
        },
    )?;

    let store = FileBackedEffectStateStore::open(&path)?;
    let consumed = store
        .lookup_consumed_spend_capability(PAYMENT_EFFECT_FAMILY, capability_ref)
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
    let path = temp.path().join("effect-state.json");
    let idempotency_key =
        EffectIdempotencyKey::new("mock", "merchant:paid-echo", "payment:paid-echo-001");
    let capability_ref = "runx:payment-capability:paid-echo-spend-1";
    let consumption = EffectCapabilityConsumption {
        capability_ref: capability_ref.to_owned(),
        idempotency_key,
        receipt_ref: Some("receipt:paid-echo:first".to_owned()),
        recovery_state: Some(EffectRecoveryState::Sealed),
    };

    let mut store = FileBackedEffectStateStore::open(&path)?;
    store.consume_spend_capability(PAYMENT_EFFECT_FAMILY, consumption.clone())?;

    let error = store
        .consume_spend_capability(PAYMENT_EFFECT_FAMILY, consumption)
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
    let path = temp.path().join("effect-state.json");
    let idempotency_key =
        EffectIdempotencyKey::new("mock", "merchant:paid-echo", "payment:paid-echo-001");
    let mut store = FileBackedEffectStateStore::open(&path)?;

    store.record_idempotency(
        PAYMENT_EFFECT_FAMILY,
        EffectIdempotencyEntry {
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
        },
    )?;
    let idempotency_error = store
        .record_idempotency(
            PAYMENT_EFFECT_FAMILY,
            EffectIdempotencyEntry {
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
            },
        )
        .err()
        .ok_or("duplicate idempotency record should be rejected")?;
    assert_eq!(
        idempotency_error.to_string(),
        "idempotency key mock\u{1f}merchant:paid-echo\u{1f}payment:paid-echo-001 was already recorded"
    );

    let stored = store
        .lookup_idempotency(PAYMENT_EFFECT_FAMILY, &idempotency_key)
        .ok_or("original idempotency entry should remain stored")?;
    assert_eq!(stored.receipt_ref, "receipt:first");

    store.record_mutation(
        PAYMENT_EFFECT_FAMILY,
        EffectMutation {
            idempotency_key: idempotency_key.clone(),
            rail: "mock".to_owned(),
            amount_minor: 125,
            currency: "USD".to_owned(),
            counterparty: "merchant:paid-echo".to_owned(),
            status: EffectMutationStatus::Partial,
            proof_ref: None,
            recovery_state: EffectRecoveryState::InFlight,
        },
    )?;
    let mutation_error = store
        .record_mutation(
            PAYMENT_EFFECT_FAMILY,
            EffectMutation {
                idempotency_key: idempotency_key.clone(),
                rail: "mock".to_owned(),
                amount_minor: 250,
                currency: "USD".to_owned(),
                counterparty: "merchant:paid-echo".to_owned(),
                status: EffectMutationStatus::Fulfilled,
                proof_ref: Some("proof:second".to_owned()),
                recovery_state: EffectRecoveryState::Sealed,
            },
        )
        .err()
        .ok_or("duplicate rail mutation should be rejected")?;
    assert_eq!(
        mutation_error.to_string(),
        "rail mutation for idempotency key mock\u{1f}merchant:paid-echo\u{1f}payment:paid-echo-001 was already recorded"
    );

    let mutation = store
        .lookup_mutation(PAYMENT_EFFECT_FAMILY, &idempotency_key)
        .ok_or("original rail mutation should remain stored")?;
    assert_eq!(mutation.status, EffectMutationStatus::Partial);
    assert_eq!(mutation.recovery_state, EffectRecoveryState::InFlight);

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
    persist_effect_step_state(
        &env,
        &graph_dir,
        &input,
        &outputs,
        &receipt,
        Some(&supervisor_proof),
    )?;

    let entry = lookup_effect_idempotency_entry(
        &env,
        &graph_dir,
        PAYMENT_EFFECT_FAMILY,
        &input.idempotency_key,
    )?
    .ok_or("sealed idempotency entry should be available through public lookup")?;
    assert_eq!(entry.receipt_ref, receipt.id);
    assert_eq!(entry.rail_proof_ref, "receipt-proof:mock:paid-echo-001");
    assert_eq!(entry.receipt_created_at, receipt.created_at.as_str());
    assert_eq!(entry.receipt_digest, receipt.digest);
    let entry_text = serde_json::to_string(&entry)?;
    assert!(
        !entry_text.contains("rail_session_material_ref"),
        "replay state must not persist rail session material"
    );
    assert!(consumed_spend_capability_recorded(
        &env,
        &graph_dir,
        PAYMENT_EFFECT_FAMILY,
        &input.spend_capability_ref
    )?);

    let store =
        FileBackedEffectStateStore::open(graph_dir.join("receipts").join("effect-state.json"))?;
    let mutation = store
        .lookup_mutation(PAYMENT_EFFECT_FAMILY, &input.idempotency_key)
        .ok_or("sealed rail mutation should be persisted")?;
    assert_eq!(mutation.status, EffectMutationStatus::Fulfilled);
    assert_eq!(mutation.recovery_state, EffectRecoveryState::Sealed);
    assert_eq!(
        mutation.proof_ref.as_deref(),
        Some("receipt-proof:mock:paid-echo-001")
    );

    Ok(())
}

#[test]
fn effect_step_state_persistence_keeps_first_sealed_record()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let mut env = BTreeMap::new();
    env.insert(
        RUNX_EFFECT_STATE_PATH_ENV.to_owned(),
        "effect-state.json".to_owned(),
    );
    let input = payment_step_input();

    let first_outputs = sealed_payment_outputs("receipt-proof:mock:first", 125)?;
    let first_receipt = receipt_for_outputs("first", "fulfill", &first_outputs)?;
    let first_supervisor_proof =
        supervisor_proof_for_receipt(&input, "receipt-proof:mock:first", &first_receipt);
    persist_effect_step_state(
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
    persist_effect_step_state(
        &env,
        temp.path(),
        &input,
        &second_outputs,
        &second_receipt,
        Some(&second_supervisor_proof),
    )?;

    let store = FileBackedEffectStateStore::open(temp.path().join("effect-state.json"))?;
    let entry = store
        .lookup_idempotency(PAYMENT_EFFECT_FAMILY, &input.idempotency_key)
        .ok_or("first idempotency entry should remain stored")?;
    assert_eq!(entry.receipt_ref, first_receipt.id);
    assert_eq!(entry.rail_proof_ref, "receipt-proof:mock:first");
    assert_eq!(entry.amount_minor, 125);

    let mutation = store
        .lookup_mutation(PAYMENT_EFFECT_FAMILY, &input.idempotency_key)
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
    let path = temp.path().join("effect-state.json");
    let idempotency_key =
        EffectIdempotencyKey::new("mock", "merchant:paid-echo", "payment:paid-echo-001");
    let mut first = FileBackedEffectStateStore::open(&path)?;
    let mut stale_second = FileBackedEffectStateStore::open(&path)?;

    first.record_idempotency(
        PAYMENT_EFFECT_FAMILY,
        EffectIdempotencyEntry {
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
        },
    )?;

    let error = stale_second
        .record_idempotency(
            PAYMENT_EFFECT_FAMILY,
            EffectIdempotencyEntry {
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
            },
        )
        .err()
        .ok_or("stale store must not overwrite locked effect state")?;
    assert_eq!(
        error.to_string(),
        "idempotency key mock\u{1f}merchant:paid-echo\u{1f}payment:paid-echo-001 was already recorded"
    );

    let fresh = FileBackedEffectStateStore::open(&path)?;
    let entry = fresh
        .lookup_idempotency(PAYMENT_EFFECT_FAMILY, &idempotency_key)
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
        RUNX_EFFECT_STATE_PATH_ENV.to_owned(),
        "effect-state.json".to_owned(),
    );
    let input = payment_step_input();

    let outputs = partial_payment_outputs()?;
    let receipt = receipt_for_outputs("partial", "fulfill", &outputs)?;
    persist_effect_step_state(&env, temp.path(), &input, &outputs, &receipt, None)?;

    assert!(
        lookup_effect_idempotency_entry(
            &env,
            temp.path(),
            PAYMENT_EFFECT_FAMILY,
            &input.idempotency_key
        )?
        .is_none(),
        "partial rail mutation without proof must not be exposed as sealed replay"
    );
    assert!(consumed_spend_capability_recorded(
        &env,
        temp.path(),
        PAYMENT_EFFECT_FAMILY,
        &input.spend_capability_ref
    )?);

    let store = FileBackedEffectStateStore::open(temp.path().join("effect-state.json"))?;
    let mutation = store
        .lookup_mutation(PAYMENT_EFFECT_FAMILY, &input.idempotency_key)
        .ok_or("partial rail mutation should be persisted for recovery")?;
    assert_eq!(mutation.status, EffectMutationStatus::Partial);
    assert_eq!(mutation.recovery_state, EffectRecoveryState::InFlight);
    assert_eq!(mutation.proof_ref, None);

    let escalated = escalate_effect_mutation(
        &env,
        temp.path(),
        PAYMENT_EFFECT_FAMILY,
        &input.idempotency_key,
    )?
    .ok_or("partial rail mutation should be escalated by public recovery helper")?;
    assert_eq!(escalated.status, EffectMutationStatus::Escalated);
    assert_eq!(escalated.recovery_state, EffectRecoveryState::Escalated);
    let looked_up = lookup_effect_mutation(
        &env,
        temp.path(),
        PAYMENT_EFFECT_FAMILY,
        &input.idempotency_key,
    )?
    .ok_or("escalated rail mutation should remain queryable")?;
    assert_eq!(looked_up.status, EffectMutationStatus::Escalated);
    assert_eq!(looked_up.recovery_state, EffectRecoveryState::Escalated);

    Ok(())
}

fn payment_step_input() -> EffectStepStateInput {
    EffectStepStateInput {
        family: PAYMENT_EFFECT_FAMILY,
        idempotency_key: EffectIdempotencyKey::new(
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
        run_spend: None,
    }
}

fn run_spend_input(
    idempotency_key: &str,
    amount_minor: u64,
    max_per_run_minor: u64,
) -> EffectStepStateInput {
    EffectStepStateInput {
        idempotency_key: EffectIdempotencyKey::new("mock", "merchant:paid-echo", idempotency_key),
        amount_minor,
        run_spend: Some(EffectRunSpendReservation {
            run_id: "run:demo-cap".to_owned(),
            authority_ref: "runx:payment-grant:paid-echo".to_owned(),
            max_per_run_minor,
        }),
        ..payment_step_input()
    }
}

fn finality_record(
    money_movement_id: &str,
    phase: EffectSettlementPhase,
    confirmation_depth: Option<u64>,
    latest_receipt_ref: &str,
) -> EffectSettlementFinalityRecord {
    EffectSettlementFinalityRecord {
        money_movement_id: money_movement_id.to_owned(),
        rail: "mpp-tempo".to_owned(),
        phase,
        confirmation_depth,
        finality_threshold: Some(3),
        original_receipt_ref: "receipt:payment:original".to_owned(),
        latest_receipt_ref: latest_receipt_ref.to_owned(),
        terminal_reason: None,
        updated_at: "2026-06-01T00:00:10Z".to_owned(),
    }
}

fn supervisor_proof_for_receipt(
    input: &EffectStepStateInput,
    proof_ref: &str,
    receipt: &Receipt,
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
        receipt_ref: receipt.id.to_string(),
        receipt_digest: receipt.digest.to_string(),
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
) -> Result<Receipt, Box<dyn std::error::Error>> {
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
