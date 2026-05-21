// rust-style-allow: large-file because the x402 payment ledger projection,
// idempotent local event append, and receipt artifact assembly remain one
// audited boundary until the payment state modules are split.
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use runx_contracts::{
    ClosureDisposition, HarnessReceipt, JsonNumber, JsonValue, ProofKind, Reference, ReferenceType,
    sha256_prefixed,
};
use serde::{Deserialize, Serialize};
use serde_json::{self, Value as JsonWireValue, json};
use thiserror::Error;

use crate::execution::runner::StepRun;

pub const PAYMENT_LEDGER_PROJECTION_SCHEMA_VERSION: &str = "runx.payment_ledger_projection.v1";
pub const X402_PAY_PAYMENT_PROFILE: &str = "x402-pay";
pub const PAYMENT_LEDGER_PROJECTED_EVENT_KIND: &str = "payment_ledger_projected";
pub const PAYMENT_LEDGER_EVENT_LEDGER_DIR: &str = "ledgers";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentLedgerProjection {
    pub schema_version: String,
    pub payment_profile: String,
    pub scenario_id: String,
    pub source_receipt_id: String,
    pub disposition: PaymentLedgerDisposition,
    pub accrual: PaymentLedgerAccrual,
    pub refusal: Option<PaymentLedgerRefusal>,
    pub evidence_refs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentLedgerProjectionArtifact {
    pub artifact_id: String,
    pub artifact_type: String,
    pub path: PathBuf,
    pub event_payload: PaymentLedgerProjectedEventPayload,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentLedgerRuntimeEvent {
    pub ledger_path: PathBuf,
    pub artifact: PaymentLedgerProjectionArtifact,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentLedgerProjectedEventPayload {
    pub kind: String,
    pub payment_profile: String,
    pub projection_artifact_id: String,
    pub projection_artifact_path: String,
    pub source_receipt_id: String,
    pub scenario_id: String,
    pub disposition: PaymentLedgerDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentLedgerDisposition {
    Settled,
    Refused,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentLedgerAccrual {
    pub amount_minor: u64,
    pub currency: String,
    pub rail: String,
    pub counterparty: String,
    pub operation: String,
    pub idempotency_key: String,
    pub rail_proof_refs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentLedgerRefusal {
    pub reason_code: String,
    pub refused_stage: String,
    pub rail_call_performed: bool,
    pub ledger_spend_recorded: bool,
}

#[derive(Clone, Debug)]
pub struct PaymentLedgerProjectionInput<'a> {
    pub graph_receipt: &'a HarnessReceipt,
    pub scenario_id: &'a str,
    pub evidence: Vec<PaymentLedgerEvidence<'a>>,
}

#[derive(Clone, Debug)]
pub struct PaymentLedgerEvidence<'a> {
    pub receipt: &'a HarnessReceipt,
    pub packet: PaymentLedgerEvidencePacket,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaymentLedgerEvidencePacket {
    Reservation(PaymentReservationEvidence),
    RailSettlement(PaymentRailSettlementEvidence),
    Refusal(PaymentRefusalEvidence),
    PaidTool(PaidToolEvidence),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaymentReservationEvidence {
    pub amount_minor: u64,
    pub currency: String,
    pub rail: String,
    pub counterparty: String,
    pub operation: String,
    pub idempotency_key: String,
    pub spend_capability_ref: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaymentRailSettlementEvidence {
    pub amount_minor: u64,
    pub currency: String,
    pub rail: String,
    pub proof_ref: String,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaymentRefusalEvidence {
    pub reason_code: String,
    pub refused_stage: String,
    pub rail_call_performed: bool,
    pub ledger_spend_recorded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaidToolEvidence {
    pub payment_proof_ref: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PaymentLedgerProjectionError {
    #[error("payment ledger projection requires at least one reservation evidence packet")]
    MissingReservation,
    #[error("payment ledger projection requires a rail settlement or refusal evidence packet")]
    MissingDispositionEvidence,
    #[error(
        "settlement evidence proof ref {proof_ref} is not present as a typed payment rail proof on receipt {receipt_id}"
    )]
    MissingReceiptRailProof {
        receipt_id: String,
        proof_ref: String,
    },
    #[error("paid tool evidence proof ref {proof_ref} has no matching settlement proof")]
    PaidToolProofMismatch { proof_ref: String },
    #[error(
        "child receipt {child_receipt_id} is not referenced by graph receipt {graph_receipt_id}"
    )]
    ChildReceiptNotReferenced {
        graph_receipt_id: String,
        child_receipt_id: String,
    },
    #[error("settlement evidence does not match reservation evidence")]
    SettlementReservationMismatch,
    #[error(
        "payment ledger projection source receipt id {source_receipt_id} is not a harness receipt ref"
    )]
    InvalidSourceReceiptId { source_receipt_id: String },
    #[error("payment ledger projection artifact already exists with different contents at {path}")]
    ArtifactConflict { path: PathBuf },
    #[error("payment ledger projection artifact I/O failed at {path}: {message}")]
    ArtifactIo { path: PathBuf, message: String },
    #[error("payment ledger projection artifact JSON failed at {path}: {message}")]
    ArtifactJson { path: PathBuf, message: String },
    #[error("payment ledger projection evidence is missing {field}")]
    MissingEvidenceField { field: &'static str },
    #[error("payment ledger projection evidence field {field} has an invalid value")]
    InvalidEvidenceField { field: &'static str },
    #[error("payment ledger projection run id {run_id} is not safe for a local ledger file")]
    InvalidRunLedgerId { run_id: String },
    #[error("payment ledger projection event already exists with different contents at {path}")]
    LedgerEventConflict { path: PathBuf },
    #[error("payment ledger projection event I/O failed at {path}: {message}")]
    LedgerEventIo { path: PathBuf, message: String },
    #[error("payment ledger projection event JSON failed at {path}: {message}")]
    LedgerEventJson { path: PathBuf, message: String },
}

// rust-style-allow: long-function because the projection validates reservation,
// settlement/refusal evidence, child receipts, and accrual in one audited pass.
pub fn build_payment_ledger_projection(
    input: PaymentLedgerProjectionInput<'_>,
) -> Result<PaymentLedgerProjection, PaymentLedgerProjectionError> {
    validate_child_receipts(input.graph_receipt, &input.evidence)?;

    let reservation = input
        .evidence
        .iter()
        .find_map(|evidence| match &evidence.packet {
            PaymentLedgerEvidencePacket::Reservation(reservation) => Some(reservation),
            _ => None,
        })
        .ok_or(PaymentLedgerProjectionError::MissingReservation)?;

    let refusal = input
        .evidence
        .iter()
        .find_map(|evidence| match &evidence.packet {
            PaymentLedgerEvidencePacket::Refusal(refusal) => Some(refusal),
            _ => None,
        });

    let settlement = input
        .evidence
        .iter()
        .find_map(|evidence| match &evidence.packet {
            PaymentLedgerEvidencePacket::RailSettlement(settlement) => Some((evidence, settlement)),
            _ => None,
        });

    let (disposition, accrual, refusal) = if let Some(refusal) = refusal {
        (
            PaymentLedgerDisposition::Refused,
            refused_accrual(reservation),
            Some(PaymentLedgerRefusal {
                reason_code: refusal.reason_code.clone(),
                refused_stage: refusal.refused_stage.clone(),
                rail_call_performed: refusal.rail_call_performed,
                ledger_spend_recorded: refusal.ledger_spend_recorded,
            }),
        )
    } else if let Some((evidence, settlement)) = settlement {
        validate_settlement_matches_reservation(reservation, settlement)?;
        validate_receipt_rail_proof(evidence.receipt, settlement)?;
        validate_paid_tool_refs(&input.evidence, &settlement.proof_ref)?;
        (
            PaymentLedgerDisposition::Settled,
            PaymentLedgerAccrual {
                amount_minor: settlement.amount_minor,
                currency: settlement.currency.clone(),
                rail: settlement.rail.clone(),
                counterparty: reservation.counterparty.clone(),
                operation: reservation.operation.clone(),
                idempotency_key: settlement.idempotency_key.clone(),
                rail_proof_refs: vec![settlement.proof_ref.clone()],
            },
            None,
        )
    } else {
        return Err(PaymentLedgerProjectionError::MissingDispositionEvidence);
    };

    Ok(PaymentLedgerProjection {
        schema_version: PAYMENT_LEDGER_PROJECTION_SCHEMA_VERSION.to_owned(),
        payment_profile: X402_PAY_PAYMENT_PROFILE.to_owned(),
        scenario_id: input.scenario_id.to_owned(),
        source_receipt_id: receipt_ref(input.graph_receipt),
        disposition,
        accrual,
        refusal,
        evidence_refs: evidence_refs(&input.evidence),
    })
}

// rust-style-allow: long-function because artifact path derivation, JSON
// serialization, hashing, and reference construction must stay byte-aligned.
pub fn write_payment_ledger_projection_artifact(
    receipt_dir: impl AsRef<Path>,
    projection: &PaymentLedgerProjection,
) -> Result<PaymentLedgerProjectionArtifact, PaymentLedgerProjectionError> {
    let receipt_id = source_receipt_file_stem(&projection.source_receipt_id)?;
    let artifact_id = format!(
        "{}:{}",
        projection.payment_profile, projection.source_receipt_id
    );
    let artifact_dir = receipt_dir
        .as_ref()
        .join("artifacts")
        .join("payment-ledger")
        .join(&projection.payment_profile);
    let artifact_path = artifact_dir.join(format!("{receipt_id}.json"));
    let mut contents = serde_json::to_vec_pretty(projection).map_err(|source| {
        PaymentLedgerProjectionError::ArtifactJson {
            path: artifact_path.clone(),
            message: source.to_string(),
        }
    })?;
    contents.push(b'\n');

    fs::create_dir_all(&artifact_dir).map_err(|source| {
        PaymentLedgerProjectionError::ArtifactIo {
            path: artifact_dir.clone(),
            message: source.to_string(),
        }
    })?;

    if artifact_path.exists() {
        let existing = fs::read(&artifact_path).map_err(|source| {
            PaymentLedgerProjectionError::ArtifactIo {
                path: artifact_path.clone(),
                message: source.to_string(),
            }
        })?;
        if existing != contents {
            return Err(PaymentLedgerProjectionError::ArtifactConflict {
                path: artifact_path,
            });
        }
    } else {
        fs::write(&artifact_path, &contents).map_err(|source| {
            PaymentLedgerProjectionError::ArtifactIo {
                path: artifact_path.clone(),
                message: source.to_string(),
            }
        })?;
    }

    let projection_artifact_path = artifact_path.to_string_lossy().into_owned();
    Ok(PaymentLedgerProjectionArtifact {
        artifact_id: artifact_id.clone(),
        artifact_type: PAYMENT_LEDGER_PROJECTION_SCHEMA_VERSION.to_owned(),
        path: artifact_path,
        event_payload: PaymentLedgerProjectedEventPayload {
            kind: PAYMENT_LEDGER_PROJECTED_EVENT_KIND.to_owned(),
            payment_profile: projection.payment_profile.clone(),
            projection_artifact_id: artifact_id,
            projection_artifact_path,
            source_receipt_id: projection.source_receipt_id.clone(),
            scenario_id: projection.scenario_id.clone(),
            disposition: projection.disposition.clone(),
        },
    })
}

pub fn persist_x402_payment_ledger_projection_event(
    receipt_dir: impl AsRef<Path>,
    run_id: &str,
    created_at: &str,
    graph_receipt: &HarnessReceipt,
    steps: &[StepRun],
    scenario_id: &str,
) -> Result<Option<PaymentLedgerRuntimeEvent>, PaymentLedgerProjectionError> {
    if graph_receipt.seal.disposition != ClosureDisposition::Closed
        || !is_x402_payment_receipt(graph_receipt)
        || !steps.iter().any(has_payment_reservation_packet)
    {
        return Ok(None);
    }
    let projection =
        build_x402_payment_ledger_projection_from_steps(graph_receipt, steps, scenario_id)?;
    let artifact = write_payment_ledger_projection_artifact(&receipt_dir, &projection)?;
    let ledger_path = append_payment_ledger_projected_event(
        receipt_dir,
        run_id,
        created_at,
        &artifact.event_payload,
    )?;
    Ok(Some(PaymentLedgerRuntimeEvent {
        ledger_path,
        artifact,
    }))
}

pub fn build_x402_payment_ledger_projection_from_steps(
    graph_receipt: &HarnessReceipt,
    steps: &[StepRun],
    scenario_id: &str,
) -> Result<PaymentLedgerProjection, PaymentLedgerProjectionError> {
    let mut evidence = Vec::new();
    for step in steps {
        if let Some(reservation) = reservation_evidence(step)? {
            evidence.push(PaymentLedgerEvidence {
                receipt: &step.receipt,
                packet: PaymentLedgerEvidencePacket::Reservation(reservation),
            });
        }
        if let Some(settlement) = settlement_evidence(step)? {
            evidence.push(PaymentLedgerEvidence {
                receipt: &step.receipt,
                packet: PaymentLedgerEvidencePacket::RailSettlement(settlement),
            });
        }
        if let Some(refusal) = refusal_evidence(step)? {
            evidence.push(PaymentLedgerEvidence {
                receipt: &step.receipt,
                packet: PaymentLedgerEvidencePacket::Refusal(refusal),
            });
        }
        if let Some(paid_tool) = paid_tool_evidence(step)? {
            evidence.push(PaymentLedgerEvidence {
                receipt: &step.receipt,
                packet: PaymentLedgerEvidencePacket::PaidTool(paid_tool),
            });
        }
    }
    build_payment_ledger_projection(PaymentLedgerProjectionInput {
        graph_receipt,
        scenario_id,
        evidence,
    })
}

// rust-style-allow: long-function because append is the idempotency boundary:
// read existing events, compare semantic identity, reject conflicts, then write.
pub fn append_payment_ledger_projected_event(
    receipt_dir: impl AsRef<Path>,
    run_id: &str,
    created_at: &str,
    payload: &PaymentLedgerProjectedEventPayload,
) -> Result<PathBuf, PaymentLedgerProjectionError> {
    validate_run_ledger_id(run_id)?;
    let ledger_dir = receipt_dir.as_ref().join(PAYMENT_LEDGER_EVENT_LEDGER_DIR);
    let ledger_path = ledger_dir.join(format!("{run_id}.jsonl"));
    let payload_bytes = serde_json::to_vec(payload).map_err(|source| {
        PaymentLedgerProjectionError::LedgerEventJson {
            path: ledger_path.clone(),
            message: source.to_string(),
        }
    })?;
    let record = payment_ledger_projected_record(run_id, created_at, payload, &payload_bytes);
    let line = serde_json::to_vec(&record).map_err(|source| {
        PaymentLedgerProjectionError::LedgerEventJson {
            path: ledger_path.clone(),
            message: source.to_string(),
        }
    })?;

    if ledger_path.exists() {
        let contents = fs::read_to_string(&ledger_path).map_err(|source| {
            PaymentLedgerProjectionError::LedgerEventIo {
                path: ledger_path.clone(),
                message: source.to_string(),
            }
        })?;
        for line in contents
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            let existing = serde_json::from_str::<JsonWireValue>(line).map_err(|source| {
                PaymentLedgerProjectionError::LedgerEventJson {
                    path: ledger_path.clone(),
                    message: source.to_string(),
                }
            })?;
            if is_same_payment_ledger_event(&existing, payload) {
                if existing == record {
                    return Ok(ledger_path);
                }
                return Err(PaymentLedgerProjectionError::LedgerEventConflict {
                    path: ledger_path,
                });
            }
        }
    } else {
        fs::create_dir_all(&ledger_dir).map_err(|source| {
            PaymentLedgerProjectionError::LedgerEventIo {
                path: ledger_dir.clone(),
                message: source.to_string(),
            }
        })?;
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&ledger_path)
        .map_err(|source| PaymentLedgerProjectionError::LedgerEventIo {
            path: ledger_path.clone(),
            message: source.to_string(),
        })?;
    file.write_all(&line)
        .and_then(|()| file.write_all(b"\n"))
        .map_err(|source| PaymentLedgerProjectionError::LedgerEventIo {
            path: ledger_path.clone(),
            message: source.to_string(),
        })?;
    Ok(ledger_path)
}

fn refused_accrual(reservation: &PaymentReservationEvidence) -> PaymentLedgerAccrual {
    PaymentLedgerAccrual {
        amount_minor: 0,
        currency: reservation.currency.clone(),
        rail: reservation.rail.clone(),
        counterparty: reservation.counterparty.clone(),
        operation: reservation.operation.clone(),
        idempotency_key: reservation.idempotency_key.clone(),
        rail_proof_refs: Vec::new(),
    }
}

fn validate_child_receipts(
    graph_receipt: &HarnessReceipt,
    evidence: &[PaymentLedgerEvidence<'_>],
) -> Result<(), PaymentLedgerProjectionError> {
    let graph_child_receipts = graph_receipt
        .harness
        .child_harness_receipt_refs
        .iter()
        .map(|reference| reference.uri.as_str())
        .collect::<HashSet<_>>();
    for evidence in evidence {
        let child_ref = receipt_ref(evidence.receipt);
        if !graph_child_receipts.contains(child_ref.as_str()) {
            return Err(PaymentLedgerProjectionError::ChildReceiptNotReferenced {
                graph_receipt_id: graph_receipt.id.clone(),
                child_receipt_id: evidence.receipt.id.clone(),
            });
        }
    }
    Ok(())
}

fn validate_settlement_matches_reservation(
    reservation: &PaymentReservationEvidence,
    settlement: &PaymentRailSettlementEvidence,
) -> Result<(), PaymentLedgerProjectionError> {
    if reservation.amount_minor == settlement.amount_minor
        && reservation.currency == settlement.currency
        && reservation.rail == settlement.rail
        && reservation.idempotency_key == settlement.idempotency_key
    {
        Ok(())
    } else {
        Err(PaymentLedgerProjectionError::SettlementReservationMismatch)
    }
}

fn validate_receipt_rail_proof(
    receipt: &HarnessReceipt,
    settlement: &PaymentRailSettlementEvidence,
) -> Result<(), PaymentLedgerProjectionError> {
    let has_proof = receipt
        .harness
        .acts
        .iter()
        .flat_map(|act| act.verification_refs.iter())
        .any(|reference| is_matching_payment_rail_proof(reference, settlement));
    if has_proof {
        Ok(())
    } else {
        Err(PaymentLedgerProjectionError::MissingReceiptRailProof {
            receipt_id: receipt.id.clone(),
            proof_ref: settlement.proof_ref.clone(),
        })
    }
}

fn is_matching_payment_rail_proof(
    reference: &Reference,
    settlement: &PaymentRailSettlementEvidence,
) -> bool {
    reference.reference_type == ReferenceType::Verification
        && reference.proof_kind.as_ref() == Some(&ProofKind::PaymentRail)
        && reference.uri == settlement.proof_ref
        && reference.locator.as_deref() == Some(settlement.idempotency_key.as_str())
}

fn validate_paid_tool_refs(
    evidence: &[PaymentLedgerEvidence<'_>],
    proof_ref: &str,
) -> Result<(), PaymentLedgerProjectionError> {
    for evidence in evidence {
        if let PaymentLedgerEvidencePacket::PaidTool(paid_tool) = &evidence.packet
            && paid_tool.payment_proof_ref != proof_ref
        {
            return Err(PaymentLedgerProjectionError::PaidToolProofMismatch {
                proof_ref: paid_tool.payment_proof_ref.clone(),
            });
        }
    }
    Ok(())
}

fn evidence_refs(evidence: &[PaymentLedgerEvidence<'_>]) -> Vec<String> {
    let mut refs = Vec::new();
    for evidence in evidence {
        if matches!(
            evidence.packet,
            PaymentLedgerEvidencePacket::RailSettlement(_)
                | PaymentLedgerEvidencePacket::Refusal(_)
        ) {
            push_unique(&mut refs, evidence.receipt.harness.harness_ref.uri.clone());
            push_unique(&mut refs, receipt_ref(evidence.receipt));
        }
    }
    for evidence in evidence {
        if let PaymentLedgerEvidencePacket::Reservation(reservation) = &evidence.packet {
            push_unique(&mut refs, reservation.spend_capability_ref.clone());
        }
    }
    refs
}

fn receipt_ref(receipt: &HarnessReceipt) -> String {
    format!("runx:harness_receipt:{}", receipt.id)
}

fn source_receipt_file_stem(source_receipt_id: &str) -> Result<&str, PaymentLedgerProjectionError> {
    const PREFIX: &str = "runx:harness_receipt:";
    let Some(receipt_id) = source_receipt_id.strip_prefix(PREFIX) else {
        return Err(PaymentLedgerProjectionError::InvalidSourceReceiptId {
            source_receipt_id: source_receipt_id.to_owned(),
        });
    };
    if receipt_id.is_empty()
        || !receipt_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        return Err(PaymentLedgerProjectionError::InvalidSourceReceiptId {
            source_receipt_id: source_receipt_id.to_owned(),
        });
    }
    Ok(receipt_id)
}

fn push_unique(refs: &mut Vec<String>, reference: String) {
    if !refs.contains(&reference) {
        refs.push(reference);
    }
}

fn is_x402_payment_receipt(receipt: &HarnessReceipt) -> bool {
    receipt.id.contains("x402-pay") || receipt.harness.harness_ref.uri.contains("x402-pay")
}

fn has_payment_reservation_packet(step: &StepRun) -> bool {
    with_step_outputs(step, |outputs| {
        Ok(packet_data(outputs, "payment_reservation_packet").map(|_| ()))
    })
    .ok()
    .flatten()
    .is_some()
}

// rust-style-allow: long-function because reservation packet projection accepts
// both bound authority and legacy fixture-adjacent packet fields in one reader.
fn reservation_evidence(
    step: &StepRun,
) -> Result<Option<PaymentReservationEvidence>, PaymentLedgerProjectionError> {
    with_step_outputs(step, |outputs| {
        let Some(data) = packet_data(outputs, "payment_reservation_packet") else {
            return Ok(None);
        };
        let Some(binding) = object_path(
            data,
            &["reserved_payment_authority", "spend_capability_binding"],
        )
        .or_else(|| object_path(data, &["spend_capability_binding"])) else {
            return Ok(None);
        };
        let payment_bounds = object_path(
            data,
            &[
                "reserved_payment_authority",
                "child_authority",
                "bounds",
                "payment",
            ],
        )
        .or_else(|| {
            object_path(
                data,
                &[
                    "reserved_payment_authority",
                    "parent_authority",
                    "bounds",
                    "payment",
                ],
            )
        });
        Ok(Some(PaymentReservationEvidence {
            amount_minor: required_u64(
                binding,
                "amount_minor",
                "spend_capability_binding.amount_minor",
            )?,
            currency: required_string(binding, "currency", "spend_capability_binding.currency")?
                .to_owned(),
            rail: required_string(binding, "rail", "spend_capability_binding.rail")?.to_owned(),
            counterparty: required_string(
                binding,
                "counterparty",
                "spend_capability_binding.counterparty",
            )?
            .to_owned(),
            operation: payment_bounds
                .and_then(|bounds| string_field(bounds, "operation"))
                .ok_or(PaymentLedgerProjectionError::MissingEvidenceField {
                    field: "reserved_payment_authority.*.bounds.payment.operation",
                })?
                .to_owned(),
            idempotency_key: required_string(
                binding,
                "idempotency_key",
                "spend_capability_binding.idempotency_key",
            )?
            .to_owned(),
            spend_capability_ref: object_path(data, &["spend_capability_ref"])
                .and_then(|reference| string_field(reference, "uri"))
                .ok_or(PaymentLedgerProjectionError::MissingEvidenceField {
                    field: "spend_capability_ref.uri",
                })?
                .to_owned(),
        }))
    })
}

fn settlement_evidence(
    step: &StepRun,
) -> Result<Option<PaymentRailSettlementEvidence>, PaymentLedgerProjectionError> {
    with_step_outputs(step, |outputs| {
        let Some(data) = packet_data(outputs, "payment_rail_packet") else {
            return Ok(None);
        };
        let Some(proof) = object_path(data, &["rail_proof"]) else {
            return Ok(None);
        };
        let result = object_path(data, &["rail_result"]).ok_or(
            PaymentLedgerProjectionError::MissingEvidenceField {
                field: "payment_rail_packet.data.rail_result",
            },
        )?;
        Ok(Some(PaymentRailSettlementEvidence {
            amount_minor: required_u64(result, "amount_minor", "rail_result.amount_minor")?,
            currency: required_string(result, "currency", "rail_result.currency")?.to_owned(),
            rail: required_string(result, "rail", "rail_result.rail")?.to_owned(),
            proof_ref: required_string(proof, "proof_ref", "rail_proof.proof_ref")?.to_owned(),
            idempotency_key: required_string(
                proof,
                "idempotency_key",
                "rail_proof.idempotency_key",
            )?
            .to_owned(),
        }))
    })
}

fn refusal_evidence(
    step: &StepRun,
) -> Result<Option<PaymentRefusalEvidence>, PaymentLedgerProjectionError> {
    with_step_outputs(step, |outputs| {
        let refusal = packet_data(outputs, "payment_refusal_packet").or_else(|| {
            packet_data(outputs, "payment_reservation_packet")
                .and_then(|data| object_path(data, &["payment_refusal_packet"]))
        });
        let Some(refusal) = refusal else {
            return Ok(None);
        };
        Ok(Some(PaymentRefusalEvidence {
            reason_code: required_string(
                refusal,
                "reason_code",
                "payment_refusal_packet.reason_code",
            )?
            .to_owned(),
            refused_stage: step.step_id.clone(),
            rail_call_performed: bool_field(refusal, "rail_call_performed").unwrap_or(false),
            ledger_spend_recorded: bool_field(refusal, "ledger_spend_recorded").unwrap_or(false),
        }))
    })
}

fn paid_tool_evidence(
    step: &StepRun,
) -> Result<Option<PaidToolEvidence>, PaymentLedgerProjectionError> {
    with_step_outputs(step, |outputs| {
        let Some(result) = object_path(outputs, &["paid_echo_result"]) else {
            return Ok(None);
        };
        Ok(Some(PaidToolEvidence {
            payment_proof_ref: required_string(
                result,
                "payment_proof_ref",
                "paid_echo_result.payment_proof_ref",
            )?
            .to_owned(),
        }))
    })
}

fn with_step_outputs<T>(
    step: &StepRun,
    extract: impl Fn(&runx_contracts::JsonObject) -> Result<Option<T>, PaymentLedgerProjectionError>,
) -> Result<Option<T>, PaymentLedgerProjectionError> {
    if let Some(value) = extract(&step.outputs)? {
        return Ok(Some(value));
    }
    let Ok(JsonValue::Object(parsed)) = serde_json::from_str::<JsonValue>(&step.output.stdout)
    else {
        return Ok(None);
    };
    extract(&parsed)
}

fn packet_data<'a>(
    outputs: &'a runx_contracts::JsonObject,
    packet: &str,
) -> Option<&'a runx_contracts::JsonObject> {
    object_path(outputs, &[packet, "data"])
}

fn object_path<'a>(
    object: &'a runx_contracts::JsonObject,
    path: &[&str],
) -> Option<&'a runx_contracts::JsonObject> {
    let mut current = object;
    for (index, segment) in path.iter().enumerate() {
        let value = current.get(*segment)?;
        if index + 1 == path.len() {
            return match value {
                JsonValue::Object(object) => Some(object),
                _ => None,
            };
        }
        let JsonValue::Object(next) = value else {
            return None;
        };
        current = next;
    }
    Some(current)
}

fn required_string<'a>(
    object: &'a runx_contracts::JsonObject,
    key: &'static str,
    field: &'static str,
) -> Result<&'a str, PaymentLedgerProjectionError> {
    string_field(object, key).ok_or(PaymentLedgerProjectionError::MissingEvidenceField { field })
}

fn string_field<'a>(object: &'a runx_contracts::JsonObject, key: &str) -> Option<&'a str> {
    match object.get(key)? {
        JsonValue::String(value) if !value.is_empty() => Some(value),
        _ => None,
    }
}

fn bool_field(object: &runx_contracts::JsonObject, key: &str) -> Option<bool> {
    match object.get(key)? {
        JsonValue::Bool(value) => Some(*value),
        _ => None,
    }
}

fn required_u64(
    object: &runx_contracts::JsonObject,
    key: &'static str,
    field: &'static str,
) -> Result<u64, PaymentLedgerProjectionError> {
    match object.get(key) {
        Some(JsonValue::Number(JsonNumber::U64(value))) => Ok(*value),
        Some(JsonValue::Number(JsonNumber::I64(value))) => u64::try_from(*value)
            .map_err(|_| PaymentLedgerProjectionError::InvalidEvidenceField { field }),
        Some(JsonValue::Number(JsonNumber::F64(value)))
            if value.is_finite() && value.fract() == 0.0 && *value >= 0.0 =>
        {
            Ok(*value as u64)
        }
        Some(JsonValue::Number(_)) => {
            Err(PaymentLedgerProjectionError::InvalidEvidenceField { field })
        }
        Some(_) => Err(PaymentLedgerProjectionError::InvalidEvidenceField { field }),
        None => Err(PaymentLedgerProjectionError::MissingEvidenceField { field }),
    }
}

fn validate_run_ledger_id(run_id: &str) -> Result<(), PaymentLedgerProjectionError> {
    if !run_id.is_empty()
        && run_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        Ok(())
    } else {
        Err(PaymentLedgerProjectionError::InvalidRunLedgerId {
            run_id: run_id.to_owned(),
        })
    }
}

fn payment_ledger_projected_record(
    run_id: &str,
    created_at: &str,
    payload: &PaymentLedgerProjectedEventPayload,
    payload_bytes: &[u8],
) -> JsonWireValue {
    json!({
        "entry": {
            "type": "run_event",
            "version": "1",
            "data": {
                "kind": PAYMENT_LEDGER_PROJECTED_EVENT_KIND,
                "status": "completed",
                "step_id": null,
                "detail": payload
            },
            "meta": {
                "artifact_id": format!("ax_payment_ledger_projected_{}", sha256_prefixed(payload.source_receipt_id.as_bytes()).trim_start_matches("sha256:")),
                "run_id": run_id,
                "step_id": null,
                "producer": {
                    "skill": X402_PAY_PAYMENT_PROFILE,
                    "runner": "graph"
                },
                "created_at": created_at,
                "hash": sha256_prefixed(payload_bytes),
                "size_bytes": payload_bytes.len(),
                "parent_artifact_id": payload.projection_artifact_id,
                "receipt_id": payload.source_receipt_id,
                "redacted": false
            }
        }
    })
}

fn is_same_payment_ledger_event(
    record: &JsonWireValue,
    payload: &PaymentLedgerProjectedEventPayload,
) -> bool {
    let entry = &record["entry"];
    entry["type"].as_str() == Some("run_event")
        && entry["data"]["kind"].as_str() == Some(PAYMENT_LEDGER_PROJECTED_EVENT_KIND)
        && entry["data"]["detail"]["source_receipt_id"].as_str()
            == Some(payload.source_receipt_id.as_str())
        && entry["data"]["detail"]["projection_artifact_id"].as_str()
            == Some(payload.projection_artifact_id.as_str())
}
