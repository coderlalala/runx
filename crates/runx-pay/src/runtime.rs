// rust-style-allow: large-file - the payment effect lifecycle is kept together
// so replay, admission, evidence binding, and persistence invariants can be
// reviewed as one adapter; authority algebra and durable state live separately.
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use runx_contracts::{
    AuthorityEffectLimit, AuthoritySubsetProof, AuthorityTerm, AuthorityVerb, Decision, JsonNumber,
    JsonObject, JsonValue, Reference,
};
use runx_core::policy::authority_term_has_verb;
use runx_core::state_machine::AuthorityAdmissionWitness;
use runx_parser::GraphStep;
use runx_runtime::{
    EffectAdmission, EffectOutputRequest, EffectReceiptRequest, EffectReplay,
    EffectReplayOutputRequest, EffectReplayReceiptRequest, EffectStepRequest, RuntimeEffect,
    RuntimeEffectError, insert_effect_verification_ref,
};
use thiserror::Error;

use crate::authority::{
    PaymentSpendCapabilityBinding, StepAuthorityAdmission, admit_step_authority,
};
use crate::packets::{PaymentRailProof, read_effect_evidence_packet};
use crate::state::{
    EffectIdempotencyEntry, EffectIdempotencyKey, EffectMutation, EffectMutationStatus,
    EffectPeriodSpendReservation, EffectRecoveryState, EffectRunSpendReservation, EffectStateError,
    EffectStepStateInput, consumed_spend_capability_recorded, escalate_effect_mutation,
    lookup_effect_idempotency_entry, lookup_effect_mutation, period_window_start,
    persist_effect_step_state, record_effect_finality_intent,
};
use crate::supervisor::{
    PAYMENT_RAIL_SUPERVISOR_EVIDENCE_METADATA, PaymentSupervisorProof, PaymentSupervisorProofMatch,
    PaymentSupervisorVerificationInput, insert_payment_supervisor_proof_metadata,
    payment_supervisor_evidence_from_payload, payment_supervisor_evidence_metadata_value,
    payment_supervisor_evidence_reference, payment_supervisor_proof_reference,
    rebind_supervisor_proof_to_receipt, validate_payment_supervisor_proof,
    verify_payment_rail_supervisor_proof,
};

pub const PAYMENT_EFFECT_FAMILY: &str = "payment";

pub trait PaymentFinalitySupervisor: Send + Sync {
    fn supervise(
        &self,
        request: PaymentFinalitySupervisorRequest<'_>,
    ) -> Result<PaymentFinalitySupervisorEvidence, PaymentFinalitySupervisorError>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PaymentFinalitySupervisorError {
    #[error("payment finality supervisor is not configured")]
    SupervisorUnavailable,
    #[error("payment finality supervisor evidence is invalid: {message}")]
    InvalidEvidence { message: String },
    #[error("payment finality supervisor denied request: {message}")]
    Denied { message: String },
    #[error(
        "payment finality supervisor field {field} mismatch: expected {expected}, got {actual}"
    )]
    FieldMismatch {
        field: &'static str,
        expected: String,
        actual: String,
    },
}

#[derive(Clone, Debug)]
pub struct PaymentFinalitySupervisorRequest<'a> {
    pub family: &'a str,
    pub payload: JsonObject,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PaymentFinalitySupervisorEvidence {
    pub family: String,
    pub payload: JsonObject,
}

impl PaymentFinalitySupervisorEvidence {
    #[must_use]
    pub fn new(family: impl Into<String>, payload: JsonObject) -> Self {
        Self {
            family: family.into(),
            payload,
        }
    }
}

#[derive(Clone)]
pub struct PaymentRuntimeEffect {
    supervisor: Arc<dyn PaymentFinalitySupervisor>,
}

impl PaymentRuntimeEffect {
    pub fn new<T>(supervisor: T) -> Self
    where
        T: PaymentFinalitySupervisor + 'static,
    {
        Self {
            supervisor: Arc::new(supervisor),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DeterministicPaymentFinalitySupervisor;

impl PaymentFinalitySupervisor for DeterministicPaymentFinalitySupervisor {
    fn supervise(
        &self,
        request: PaymentFinalitySupervisorRequest<'_>,
    ) -> Result<PaymentFinalitySupervisorEvidence, PaymentFinalitySupervisorError> {
        let status =
            supervisor_payload_optional_string(&request.payload, "skill_settlement_status")?;
        if status != Some("fulfilled") {
            return Err(PaymentFinalitySupervisorError::Denied {
                message: format!("payment rail result status {status:?} is not fulfilled"),
            });
        }
        let proof_ref = supervisor_payload_string(&request.payload, "proof_ref")?;
        let rail = supervisor_payload_string(&request.payload, "rail")?;
        let counterparty = supervisor_payload_string(&request.payload, "counterparty")?;
        let amount_minor = supervisor_payload_u64(&request.payload, "amount_minor")?;
        let currency = supervisor_payload_string(&request.payload, "currency")?;
        let idempotency_key = supervisor_payload_string(&request.payload, "idempotency_key")?;
        let mut payload = JsonObject::new();
        payload.insert(
            "verifier_id".to_owned(),
            JsonValue::String(crate::supervisor::PAYMENT_RAIL_SUPERVISOR_VERIFIER_ID.to_owned()),
        );
        payload.insert(
            "proof_ref".to_owned(),
            JsonValue::String(proof_ref.to_owned()),
        );
        payload.insert("rail".to_owned(), JsonValue::String(rail.to_owned()));
        payload.insert(
            "counterparty".to_owned(),
            JsonValue::String(counterparty.to_owned()),
        );
        payload.insert(
            "amount_minor".to_owned(),
            JsonValue::Number(JsonNumber::U64(amount_minor)),
        );
        payload.insert(
            "currency".to_owned(),
            JsonValue::String(currency.to_owned()),
        );
        payload.insert(
            "idempotency_key".to_owned(),
            JsonValue::String(idempotency_key.to_owned()),
        );
        if let Some(status) = status {
            payload.insert(
                "settlement_status".to_owned(),
                JsonValue::String(status.to_owned()),
            );
        }
        payload.insert(
            "provider_event_ref".to_owned(),
            JsonValue::String(format!("runx-pay:test:{proof_ref}")),
        );
        Ok(PaymentFinalitySupervisorEvidence::new(
            request.family,
            payload,
        ))
    }
}

fn supervisor_payload_string<'a>(
    payload: &'a JsonObject,
    field: &'static str,
) -> Result<&'a str, PaymentFinalitySupervisorError> {
    match payload.get(field) {
        Some(JsonValue::String(value)) => Ok(value),
        Some(value) => Err(invalid_supervisor_payload(field, value, "string")),
        None => Err(missing_supervisor_payload(field)),
    }
}

fn supervisor_payload_optional_string<'a>(
    payload: &'a JsonObject,
    field: &'static str,
) -> Result<Option<&'a str>, PaymentFinalitySupervisorError> {
    match payload.get(field) {
        Some(JsonValue::String(value)) => Ok(Some(value)),
        Some(JsonValue::Null) | None => Ok(None),
        Some(value) => Err(invalid_supervisor_payload(field, value, "string")),
    }
}

fn supervisor_payload_u64(
    payload: &JsonObject,
    field: &'static str,
) -> Result<u64, PaymentFinalitySupervisorError> {
    match payload.get(field) {
        Some(JsonValue::Number(JsonNumber::U64(value))) => Ok(*value),
        Some(value @ JsonValue::Number(JsonNumber::I64(number))) => u64::try_from(*number)
            .map_err(|_| invalid_supervisor_payload(field, value, "unsigned integer")),
        Some(value) => Err(invalid_supervisor_payload(field, value, "unsigned integer")),
        None => Err(missing_supervisor_payload(field)),
    }
}

fn missing_supervisor_payload(field: &'static str) -> PaymentFinalitySupervisorError {
    PaymentFinalitySupervisorError::InvalidEvidence {
        message: format!("payment finality supervisor payload is missing {field}"),
    }
}

fn invalid_supervisor_payload(
    field: &'static str,
    value: &JsonValue,
    expected: &'static str,
) -> PaymentFinalitySupervisorError {
    PaymentFinalitySupervisorError::InvalidEvidence {
        message: format!(
            "payment finality supervisor payload field {field} must be {expected}, got {}",
            json_value_kind(value)
        ),
    }
}

fn json_value_kind(value: &JsonValue) -> &'static str {
    match value {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "bool",
        JsonValue::Number(_) => "number",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "array",
        JsonValue::Object(_) => "object",
    }
}

impl RuntimeEffect for PaymentRuntimeEffect {
    fn family(&self) -> &'static str {
        PAYMENT_EFFECT_FAMILY
    }

    fn can_run_parallel(&self, step: &GraphStep) -> bool {
        !payment_admission_field_present(&step.inputs)
            && !step
                .context_edges
                .iter()
                .any(|edge| is_payment_admission_key(&edge.input))
    }

    fn find_replay(
        &self,
        request: EffectStepRequest<'_>,
    ) -> Result<Option<EffectReplay>, RuntimeEffectError> {
        let Some(input) = step_authority_submission(request.step, request.inputs)? else {
            return Ok(None);
        };
        let Some(payment) = payment_context(&input, request.inputs, request.env)? else {
            return Ok(None);
        };
        let Some(entry) = lookup_effect_idempotency_entry(
            request.env,
            request.graph_dir,
            PAYMENT_EFFECT_FAMILY,
            &payment.idempotency_key,
        )
        .map_err(|source| failed("state replay lookup", source))?
        else {
            return Ok(None);
        };

        let act_id = format!("act_{}", request.step.id);
        let decision = admit_step_authority(StepAuthorityAdmission {
            parent_authority: &input.parent_authority,
            child_authority: &input.child_authority,
            reservation_decision: input.reservation_decision.as_ref(),
            subset_proof: input.subset_proof.as_ref(),
            child_harness_ref: &input.child_harness_ref,
            act_id: &act_id,
            idempotency_key: input.idempotency_key.as_deref(),
            spend_capability_binding: input.spend_capability_binding.clone(),
            consumed_spend_capability_refs: &input.consumed_spend_capability_refs,
            spend_capability_ref: input.spend_capability_ref.as_ref(),
        })
        .map_err(|source| denied(source.to_string()))?;
        if decision.verb != Some(AuthorityVerb::Commit) {
            return Ok(None);
        }
        validate_entry_matches_payment(&entry, &payment)?;

        Ok(Some(EffectReplay::new(
            PAYMENT_EFFECT_FAMILY,
            entry.receipt_ref.clone(),
            entry.receipt_created_at.clone(),
            entry.receipt_digest.clone(),
            entry.outputs.clone(),
            PaymentReplayContext {
                rail_proof_ref: entry.rail_proof_ref.clone(),
                idempotency_key: entry.idempotency_key.clone(),
                spend_capability_ref: entry.supervisor_proof.spend_capability_ref.clone(),
                rail: entry.supervisor_proof.rail.clone(),
                counterparty: entry.supervisor_proof.counterparty.clone(),
                amount_minor: entry.supervisor_proof.amount_minor,
                currency: entry.supervisor_proof.currency.clone(),
                act_id,
                supervisor_proof: entry.supervisor_proof.clone(),
            },
        )))
    }

    fn recover_pending(&self, request: EffectStepRequest<'_>) -> Result<(), RuntimeEffectError> {
        let Some(input) = step_authority_submission(request.step, request.inputs)? else {
            return Ok(());
        };
        let Some(payment) = payment_context(&input, request.inputs, request.env)? else {
            return Ok(());
        };
        let Some(mutation) = pending_mutation_for_recovery(request, &payment)? else {
            return Ok(());
        };

        let act_id = format!("act_{}", request.step.id);
        admit_step_authority(StepAuthorityAdmission {
            parent_authority: &input.parent_authority,
            child_authority: &input.child_authority,
            reservation_decision: input.reservation_decision.as_ref(),
            subset_proof: input.subset_proof.as_ref(),
            child_harness_ref: &input.child_harness_ref,
            act_id: &act_id,
            idempotency_key: input.idempotency_key.as_deref(),
            spend_capability_binding: input.spend_capability_binding.clone(),
            consumed_spend_capability_refs: &input.consumed_spend_capability_refs,
            spend_capability_ref: input.spend_capability_ref.as_ref(),
        })
        .map_err(|source| denied(source.to_string()))?;
        validate_pending_mutation_matches_payment(&mutation, &payment)?;

        let _ = escalate_effect_mutation(
            request.env,
            request.graph_dir,
            PAYMENT_EFFECT_FAMILY,
            &payment.idempotency_key,
        )
        .map_err(|source| failed("state recovery escalation", source))?;
        Err(denied(format!(
            "payment idempotency key {} has an in-flight rail mutation; recovery escalated without issuing a second rail mutation",
            payment.idempotency_key.key
        )))
    }

    // rust-style-allow: long-function because admission is one fail-closed
    // decision path (parse submission, check idempotency, reserve, build the
    // admission record) that must read top to bottom to stay auditable.
    fn admit(
        &self,
        request: EffectStepRequest<'_>,
    ) -> Result<Option<EffectAdmission>, RuntimeEffectError> {
        let Some(input) = step_authority_submission(request.step, request.inputs)? else {
            return Ok(None);
        };
        let consumed_spend_capability_refs =
            consumed_spend_capability_refs_for_admission(&input, request.env, request.graph_dir)?;
        let act_id = format!("act_{}", request.step.id);
        let admission_error_verb =
            if authority_term_has_verb(&input.child_authority, AuthorityVerb::Commit) {
                AuthorityVerb::Commit
            } else {
                input
                    .child_authority
                    .verbs
                    .first()
                    .cloned()
                    .unwrap_or(AuthorityVerb::Commit)
            };
        let decision = admit_step_authority(StepAuthorityAdmission {
            parent_authority: &input.parent_authority,
            child_authority: &input.child_authority,
            reservation_decision: input.reservation_decision.as_ref(),
            subset_proof: input.subset_proof.as_ref(),
            child_harness_ref: &input.child_harness_ref,
            act_id: &act_id,
            idempotency_key: input.idempotency_key.as_deref(),
            spend_capability_binding: input.spend_capability_binding.clone(),
            consumed_spend_capability_refs: &consumed_spend_capability_refs,
            spend_capability_ref: input.spend_capability_ref.as_ref(),
        })
        .map_err(|source| RuntimeEffectError::Denied {
            family: PAYMENT_EFFECT_FAMILY.to_owned(),
            verb: admission_error_verb,
            message: source.to_string(),
        })?;
        let Some(verb) = decision.verb else {
            return Ok(None);
        };
        let payment = if verb == AuthorityVerb::Commit {
            payment_context(&input, request.inputs, request.env)?
        } else {
            None
        };
        if let Some(payment) = payment.as_ref() {
            record_effect_finality_intent(
                request.env,
                request.graph_dir,
                &EffectStepStateInput {
                    family: PAYMENT_EFFECT_FAMILY,
                    idempotency_key: payment.idempotency_key.clone(),
                    spend_capability_ref: payment.spend_capability_ref.uri.clone().into_string(),
                    rail: payment.rail.clone(),
                    counterparty: payment.counterparty.clone(),
                    amount_minor: payment.amount_minor,
                    currency: payment.currency.clone(),
                    act_id: format!("act_{}", request.step.id),
                    run_spend: payment.run_spend.clone(),
                    period_spend: payment.period_spend.clone(),
                },
            )
            .map_err(finality_intent_error)?;
        }
        Ok(Some(EffectAdmission::new(
            PAYMENT_EFFECT_FAMILY,
            verb.clone(),
            AuthorityAdmissionWitness {
                verb,
                parent_term_id: decision.parent_term_id.to_owned(),
                child_term_id: decision.child_term_id.to_owned(),
                idempotency_key: decision.idempotency_key.map(str::to_owned),
                capability_ref: decision.spend_capability_ref.cloned(),
            },
            PaymentAdmissionContext { payment },
        )))
    }

    fn prepare_output(&self, request: EffectOutputRequest<'_>) -> Result<(), RuntimeEffectError> {
        let Some(payment) = payment_admission_context(request.admission)?
            .payment
            .as_ref()
        else {
            return Ok(());
        };
        if !request.output.succeeded() {
            return Ok(());
        }
        let Some(packet) = read_effect_evidence_packet(request.claim)
            .map_err(|source| failed("reading rail packet", source))?
        else {
            return Ok(());
        };
        let Some(claim) = packet.proof.as_ref() else {
            return Ok(());
        };
        let status = packet
            .result
            .as_ref()
            .and_then(|result| result.status.as_deref());
        let supervisor_evidence = self
            .supervisor
            .supervise(supervisor_request(payment, claim, status))
            .map_err(|source| {
                denied(format!(
                    "supervisor-verified rail proof is required: {source}"
                ))
            })?;
        if supervisor_evidence.family != PAYMENT_EFFECT_FAMILY {
            return Err(denied(format!(
                "supervisor returned evidence family {}, expected {}",
                supervisor_evidence.family, PAYMENT_EFFECT_FAMILY
            )));
        }
        let evidence = payment_supervisor_evidence_from_payload(&supervisor_evidence.payload)
            .map_err(|source| {
                denied(format!(
                    "supervisor-verified rail proof is required: {source}"
                ))
            })?;
        let value = payment_supervisor_evidence_metadata_value(&evidence)
            .map_err(|source| failed("encoding supervisor evidence", source))?;
        request
            .output
            .metadata
            .insert(PAYMENT_RAIL_SUPERVISOR_EVIDENCE_METADATA.to_owned(), value);
        insert_effect_verification_ref(
            &mut request.output.metadata,
            payment_supervisor_evidence_reference(&evidence),
        )?;
        Ok(())
    }

    fn finalize_output(&self, request: EffectReceiptRequest<'_>) -> Result<(), RuntimeEffectError> {
        let Some(payment) = payment_admission_context(request.admission)?
            .payment
            .as_ref()
        else {
            return Ok(());
        };
        if !request.output.succeeded() {
            return Ok(());
        }
        let act_id = format!("act_{}", request.step.id);
        let proof = verify_payment_rail_supervisor_proof(PaymentSupervisorVerificationInput {
            outputs: request.claim,
            metadata: &request.output.metadata,
            receipt: request.receipt,
            rail: &payment.rail,
            counterparty: &payment.counterparty,
            amount_minor: payment.amount_minor,
            currency: &payment.currency,
            idempotency_key: &payment.idempotency_key.key,
            spend_capability_ref: &payment.spend_capability_ref.uri,
            act_id: &act_id,
        })
        .map_err(|source| {
            denied(format!(
                "spend success requires supervisor-verified rail proof: {source}"
            ))
        })?;
        insert_payment_supervisor_proof_metadata(&mut request.output.metadata, &proof)
            .map_err(|source| failed("recording supervisor proof metadata", source))?;
        Ok(())
    }

    fn persist(&self, request: EffectReceiptRequest<'_>) -> Result<(), RuntimeEffectError> {
        let Some(payment) = payment_admission_context(request.admission)?
            .payment
            .as_ref()
        else {
            return Ok(());
        };
        let proof =
            crate::supervisor::payment_supervisor_proof_from_metadata(&request.output.metadata)
                .map_err(|source| failed("reading supervisor proof metadata", source))?;
        persist_effect_step_state(
            request.env,
            request.graph_dir,
            &EffectStepStateInput {
                family: PAYMENT_EFFECT_FAMILY,
                idempotency_key: payment.idempotency_key.clone(),
                spend_capability_ref: payment.spend_capability_ref.uri.clone().into_string(),
                rail: payment.rail.clone(),
                counterparty: payment.counterparty.clone(),
                amount_minor: payment.amount_minor,
                currency: payment.currency.clone(),
                act_id: format!("act_{}", request.step.id),
                run_spend: payment.run_spend.clone(),
                period_spend: payment.period_spend.clone(),
            },
            request.claim,
            request.receipt,
            proof.as_ref(),
        )
        .map_err(|source| failed("persisting state", source))
    }

    fn prepare_replay_output(
        &self,
        request: EffectReplayOutputRequest<'_>,
    ) -> Result<(), RuntimeEffectError> {
        let context = payment_replay_context(request.replay)?;
        insert_payment_supervisor_proof_metadata(
            &mut request.output.metadata,
            &context.supervisor_proof,
        )
        .map_err(|source| failed("recording replayed supervisor proof metadata", source))?;
        insert_effect_verification_ref(
            &mut request.output.metadata,
            payment_supervisor_proof_reference(&context.supervisor_proof),
        )
    }

    fn validate_replay(
        &self,
        request: EffectReplayReceiptRequest<'_>,
    ) -> Result<(), RuntimeEffectError> {
        let context = payment_replay_context(request.replay)?;
        if !receipt_has_payment_rail_proof(request.receipt, &context.rail_proof_ref) {
            return Err(denied(format!(
                "sealed payment replay rebuilt receipt without rail proof {}",
                context.rail_proof_ref
            )));
        }
        validate_payment_supervisor_proof(
            &context.supervisor_proof,
            PaymentSupervisorProofMatch {
                proof_ref: &context.rail_proof_ref,
                rail: &context.rail,
                counterparty: &context.counterparty,
                amount_minor: context.amount_minor,
                currency: &context.currency,
                idempotency_key: &context.idempotency_key.key,
                spend_capability_ref: &context.spend_capability_ref,
                act_id: &context.act_id,
                receipt_ref: &request.receipt.id,
                receipt_digest: &request.receipt.digest,
            },
        )
        .map_err(|source| {
            denied(format!(
                "sealed payment replay supervisor proof mismatch: {source}"
            ))
        })
    }

    fn refresh_output_metadata(
        &self,
        request: runx_runtime::EffectMetadataRefreshRequest<'_>,
    ) -> Result<(), RuntimeEffectError> {
        rebind_supervisor_proof_to_receipt(&mut request.output.metadata, request.receipt)
            .map_err(|source| failed("refreshing supervisor proof metadata", source))
    }
}

fn pending_mutation_for_recovery(
    request: EffectStepRequest<'_>,
    payment: &StepPaymentAuthorityContext,
) -> Result<Option<EffectMutation>, RuntimeEffectError> {
    let mutation = lookup_effect_mutation(
        request.env,
        request.graph_dir,
        PAYMENT_EFFECT_FAMILY,
        &payment.idempotency_key,
    )
    .map_err(|source| failed("state recovery lookup", source))?;
    Ok(mutation.filter(|mutation| {
        mutation.recovery_state == EffectRecoveryState::InFlight
            || mutation.status == EffectMutationStatus::Partial
    }))
}

fn validate_pending_mutation_matches_payment(
    mutation: &EffectMutation,
    payment: &StepPaymentAuthorityContext,
) -> Result<(), RuntimeEffectError> {
    if mutation.amount_minor == payment.amount_minor
        && mutation.currency == payment.currency
        && mutation.rail == payment.rail
        && mutation.counterparty == payment.counterparty
    {
        return Ok(());
    }
    Err(denied(format!(
        "payment idempotency key {} has in-flight rail mutation for {} {} on {} {}, but this spend requested {} {} on {} {}",
        payment.idempotency_key.key,
        mutation.amount_minor,
        mutation.currency,
        mutation.rail,
        mutation.counterparty,
        payment.amount_minor,
        payment.currency,
        payment.rail,
        payment.counterparty
    )))
}

fn supervisor_request<'a>(
    payment: &'a StepPaymentAuthorityContext,
    claim: &'a PaymentRailProof,
    skill_settlement_status: Option<&'a str>,
) -> PaymentFinalitySupervisorRequest<'a> {
    let mut payload = JsonObject::new();
    payload.insert("rail".to_owned(), JsonValue::String(payment.rail.clone()));
    payload.insert(
        "counterparty".to_owned(),
        JsonValue::String(payment.counterparty.clone()),
    );
    payload.insert(
        "amount_minor".to_owned(),
        JsonValue::Number(JsonNumber::U64(payment.amount_minor)),
    );
    payload.insert(
        "currency".to_owned(),
        JsonValue::String(payment.currency.clone()),
    );
    payload.insert(
        "idempotency_key".to_owned(),
        JsonValue::String(payment.idempotency_key.key.clone()),
    );
    payload.insert(
        "proof_ref".to_owned(),
        JsonValue::String(claim.proof_ref.clone()),
    );
    if let Some(status) = skill_settlement_status {
        payload.insert(
            "skill_settlement_status".to_owned(),
            JsonValue::String(status.to_owned()),
        );
    }
    PaymentFinalitySupervisorRequest {
        family: PAYMENT_EFFECT_FAMILY,
        payload,
    }
}

fn consumed_spend_capability_refs_for_admission(
    input: &OwnedStepAuthoritySubmission,
    env: &BTreeMap<String, String>,
    graph_dir: &Path,
) -> Result<Vec<Reference>, RuntimeEffectError> {
    let mut refs = input.consumed_spend_capability_refs.clone();
    let Some(spend_capability_ref) = input.spend_capability_ref.as_ref() else {
        return Ok(refs);
    };
    if consumed_spend_capability_recorded(
        env,
        graph_dir,
        PAYMENT_EFFECT_FAMILY,
        &spend_capability_ref.uri,
    )
    .map_err(|source| failed("state admission lookup", source))?
        && !refs
            .iter()
            .any(|reference| same_reference(reference, spend_capability_ref))
    {
        refs.push(spend_capability_ref.clone());
    }
    Ok(refs)
}

fn validate_entry_matches_payment(
    entry: &EffectIdempotencyEntry,
    payment: &StepPaymentAuthorityContext,
) -> Result<(), RuntimeEffectError> {
    if entry.amount_minor != payment.amount_minor || entry.currency != payment.currency {
        return Err(denied(format!(
            "payment idempotency key {} was sealed for {} {}, but this spend requested {} {}",
            payment.idempotency_key.key,
            entry.amount_minor,
            entry.currency,
            payment.amount_minor,
            payment.currency
        )));
    }
    if entry.supervisor_proof.rail == payment.rail
        && entry.supervisor_proof.counterparty == payment.counterparty
        && entry.supervisor_proof.spend_capability_ref == payment.spend_capability_ref.uri
    {
        return Ok(());
    }
    Err(denied(format!(
        "payment idempotency key {} supervisor proof was sealed for {} {}, capability {}, but this spend requested {} {}, capability {}",
        payment.idempotency_key.key,
        entry.supervisor_proof.rail,
        entry.supervisor_proof.counterparty,
        entry.supervisor_proof.spend_capability_ref,
        payment.rail,
        payment.counterparty,
        payment.spend_capability_ref.uri
    )))
}

fn payment_context(
    input: &OwnedStepAuthoritySubmission,
    inputs: &JsonObject,
    env: &BTreeMap<String, String>,
) -> Result<Option<StepPaymentAuthorityContext>, RuntimeEffectError> {
    let Some(binding) = input.spend_capability_binding.as_ref() else {
        return Ok(None);
    };
    let Some(idempotency_key) = input.idempotency_key.as_ref() else {
        return Ok(None);
    };
    let Some(spend_capability_ref) = input.spend_capability_ref.as_ref() else {
        return Ok(None);
    };
    let run_spend = run_spend_reservation(input, inputs, env)?;
    let period_spend = period_spend_reservation(input)?;
    Ok(Some(StepPaymentAuthorityContext {
        idempotency_key: EffectIdempotencyKey::new(
            binding.rail.clone(),
            binding.counterparty.clone(),
            idempotency_key.clone(),
        ),
        spend_capability_ref: spend_capability_ref.clone(),
        rail: binding.rail.clone(),
        counterparty: binding.counterparty.clone(),
        amount_minor: binding.amount_minor,
        currency: binding.currency.clone(),
        run_spend,
        period_spend,
    }))
}

fn run_spend_reservation(
    input: &OwnedStepAuthoritySubmission,
    inputs: &JsonObject,
    env: &BTreeMap<String, String>,
) -> Result<Option<EffectRunSpendReservation>, RuntimeEffectError> {
    let payment = payment_effect_limit(&input.child_authority);
    let max_per_run_units = payment.and_then(|payment| payment.max_per_run_units);
    let max_per_period_units = payment.and_then(|payment| payment.max_per_period_units);
    // A run never spans more than one period, so the period cap also bounds
    // each run. Until a durable cross-run period ledger lands, the period cap
    // is enforced as a run-level clamp instead of being parsed and ignored.
    let Some(max_per_run_units) = (match (max_per_run_units, max_per_period_units) {
        (Some(run_cap), Some(period_cap)) => Some(run_cap.min(period_cap)),
        (Some(run_cap), None) => Some(run_cap),
        (None, Some(period_cap)) => Some(period_cap),
        (None, None) => None,
    }) else {
        return Ok(None);
    };
    let Some(run_id) = payment_run_id(inputs, env)? else {
        return Err(denied(
            "payment authority with an aggregate spend cap requires a run_id before rail execution"
                .to_owned(),
        ));
    };
    Ok(Some(EffectRunSpendReservation {
        run_id,
        authority_ref: input.child_authority.resource_ref.uri.clone().into_string(),
        max_per_run_units,
    }))
}

/// Durable cross-run enforcement for `max_per_period_units`: when the
/// authority declares a recognized `period`, the spend is reserved against a
/// calendar-window ledger in the effect state file in addition to the
/// run-level clamp above. A declared period the runtime cannot interpret
/// fails closed rather than becoming an unenforced annotation.
fn period_spend_reservation(
    input: &OwnedStepAuthoritySubmission,
) -> Result<Option<EffectPeriodSpendReservation>, RuntimeEffectError> {
    let Some(payment) = payment_effect_limit(&input.child_authority) else {
        return Ok(None);
    };
    let Some(max_per_period_units) = payment.max_per_period_units else {
        return Ok(None);
    };
    let Some(period) = payment.period.as_ref() else {
        // Period cap without a declared window: the run-level clamp is the
        // enforceable meaning, so there is no durable window to reserve.
        return Ok(None);
    };
    let unix_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|source| failed("reading wall clock for period window", source))?
        .as_secs();
    let window_start = period_window_start(period.as_str(), unix_seconds)
        .map_err(|source| denied(source.to_string()))?;
    Ok(Some(EffectPeriodSpendReservation {
        authority_ref: input.child_authority.resource_ref.uri.clone().into_string(),
        max_per_period_units,
        period: period.as_str().to_owned(),
        window_start,
    }))
}

fn payment_effect_limit(term: &AuthorityTerm) -> Option<&AuthorityEffectLimit> {
    term.bounds
        .effect_limits
        .iter()
        .find(|limit| limit.family == PAYMENT_EFFECT_FAMILY)
}

fn payment_run_id(
    inputs: &JsonObject,
    env: &BTreeMap<String, String>,
) -> Result<Option<String>, RuntimeEffectError> {
    if let Some(run_id) = env
        .get(runx_runtime::RUNX_RUN_ID_ENV)
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(Some(run_id.clone()));
    }
    if let Some(run_id) = optional_string_input(inputs, "run_id")? {
        return Ok(Some(run_id));
    }
    let Some(JsonValue::Object(admission)) = inputs.get("payment_admission") else {
        return Ok(None);
    };
    if let Some(JsonValue::Object(token)) = admission.get("token") {
        return optional_string_input(token, "run_id");
    }
    optional_string_input(admission, "run_id")
}

fn step_authority_submission(
    step: &GraphStep,
    inputs: &JsonObject,
) -> Result<Option<OwnedStepAuthoritySubmission>, RuntimeEffectError> {
    let Some(reserved) = optional_payment_authority_object(inputs)? else {
        return Ok(None);
    };
    let reserved = parse_reserved_payment_authority(reserved)?;
    let spends = authority_term_has_verb(&reserved.child_authority, AuthorityVerb::Commit);
    let (spend_capability_ref, idempotency_key) = if spends {
        let idempotency = require_object_input(inputs, "idempotency")?;
        (
            Some(require_reference_input(inputs, "spend_capability_ref")?),
            Some(require_non_empty_string_field(
                idempotency,
                "idempotency.key",
            )?),
        )
    } else {
        (None, None)
    };
    let _ = step;
    Ok(Some(OwnedStepAuthoritySubmission {
        spend_capability_ref,
        idempotency_key,
        parent_authority: reserved.parent_authority,
        child_authority: reserved.child_authority,
        reservation_decision: reserved.reservation_decision,
        subset_proof: reserved.subset_proof,
        child_harness_ref: reserved.child_harness_ref,
        spend_capability_binding: reserved.spend_capability_binding,
        consumed_spend_capability_refs: reserved.consumed_spend_capability_refs,
    }))
}

fn optional_payment_authority_object(
    inputs: &JsonObject,
) -> Result<Option<&JsonObject>, RuntimeEffectError> {
    let has_execution_field =
        inputs.contains_key("payment_challenge") || inputs.contains_key("spend_capability_ref");
    if inputs.contains_key("reserved_payment_authority") {
        if !has_execution_field && !inputs.contains_key("idempotency") {
            return Ok(None);
        }
        return require_object_input(inputs, "reserved_payment_authority").map(Some);
    }
    if has_execution_field {
        return Err(denied(
            "reserved_payment_authority is required before payment rail execution".to_owned(),
        ));
    }
    Ok(None)
}

fn payment_admission_field_present(inputs: &JsonObject) -> bool {
    inputs.keys().any(|key| is_payment_admission_key(key))
}

fn is_payment_admission_key(key: &str) -> bool {
    matches!(key, "spend_capability_ref" | "payment_challenge")
}

fn parse_reserved_payment_authority(
    object: &JsonObject,
) -> Result<ReservedAuthorityInput, RuntimeEffectError> {
    Ok(ReservedAuthorityInput {
        parent_authority: required_typed_input(
            object,
            "reserved_payment_authority.parent_authority",
            "parent_authority",
        )?,
        child_authority: required_typed_input(
            object,
            "reserved_payment_authority.child_authority",
            "child_authority",
        )?,
        reservation_decision: optional_typed_input(
            object,
            "reserved_payment_authority.reservation_decision",
            "reservation_decision",
        )?,
        subset_proof: optional_typed_input(
            object,
            "reserved_payment_authority.subset_proof",
            "subset_proof",
        )?,
        child_harness_ref: required_typed_input(
            object,
            "reserved_payment_authority.child_harness_ref",
            "child_harness_ref",
        )?,
        spend_capability_binding: optional_typed_input(
            object,
            "reserved_payment_authority.spend_capability_binding",
            "spend_capability_binding",
        )?,
        consumed_spend_capability_refs: optional_typed_input(
            object,
            "reserved_payment_authority.consumed_spend_capability_refs",
            "consumed_spend_capability_refs",
        )?
        .unwrap_or_default(),
    })
}

fn require_object_input<'a>(
    inputs: &'a JsonObject,
    field: &str,
) -> Result<&'a JsonObject, RuntimeEffectError> {
    match inputs.get(field) {
        Some(JsonValue::Object(object)) => Ok(object),
        Some(_) => Err(denied(format!(
            "{field} must be an object before payment rail execution"
        ))),
        None => Err(denied(format!(
            "{field} is required before payment rail execution"
        ))),
    }
}

fn optional_string_input(
    inputs: &JsonObject,
    field: &str,
) -> Result<Option<String>, RuntimeEffectError> {
    match inputs.get(field) {
        Some(JsonValue::String(value)) if !value.trim().is_empty() => Ok(Some(value.clone())),
        Some(JsonValue::String(_)) => Err(denied(format!(
            "{field} must not be empty before payment rail execution"
        ))),
        Some(_) => Err(denied(format!(
            "{field} must be a string before payment rail execution"
        ))),
        None => Ok(None),
    }
}

fn require_non_empty_string_field(
    object: &JsonObject,
    field_path: &str,
) -> Result<String, RuntimeEffectError> {
    let Some((_, field)) = field_path.rsplit_once('.') else {
        return Err(denied(format!(
            "{field_path} is not a valid payment admission field"
        )));
    };
    let Some(value) = object.get(field) else {
        return Err(denied(format!(
            "{field_path} is required before payment rail execution"
        )));
    };
    let JsonValue::String(value) = value else {
        return Err(denied(format!(
            "{field_path} must be a string before payment rail execution"
        )));
    };
    if value.trim().is_empty() {
        return Err(denied(format!(
            "{field_path} must not be empty before payment rail execution"
        )));
    }
    Ok(value.to_owned())
}

fn require_reference_input(
    inputs: &JsonObject,
    field: &str,
) -> Result<Reference, RuntimeEffectError> {
    match inputs.get(field) {
        Some(JsonValue::Object(_)) => required_typed_value(inputs.get(field), field),
        Some(_) => Err(denied(format!(
            "{field} must be a Reference before payment rail execution"
        ))),
        None => Err(denied(format!(
            "{field} is required before payment rail execution"
        ))),
    }
}

fn optional_typed_input<T: serde::de::DeserializeOwned>(
    object: &JsonObject,
    field_path: &str,
    field: &str,
) -> Result<Option<T>, RuntimeEffectError> {
    let Some(value) = object.get(field) else {
        return Ok(None);
    };
    required_typed_value(Some(value), field_path).map(Some)
}

fn required_typed_input<T: serde::de::DeserializeOwned>(
    object: &JsonObject,
    field_path: &str,
    field: &str,
) -> Result<T, RuntimeEffectError> {
    required_typed_value(object.get(field), field_path)
}

fn required_typed_value<T: serde::de::DeserializeOwned>(
    value: Option<&JsonValue>,
    field_path: &str,
) -> Result<T, RuntimeEffectError> {
    let Some(value) = value else {
        return Err(denied(format!(
            "{field_path} is required before payment rail execution"
        )));
    };
    serde_json::from_value::<T>(
        serde_json::to_value(value).map_err(|source| failed("serializing input", source))?,
    )
    .map_err(|source| {
        denied(format!(
            "{field_path} is not valid typed payment authority: {source}"
        ))
    })
}

fn payment_admission_context(
    admission: &EffectAdmission,
) -> Result<&PaymentAdmissionContext, RuntimeEffectError> {
    admission
        .context::<PaymentAdmissionContext>()
        .ok_or_else(|| RuntimeEffectError::Failed {
            family: PAYMENT_EFFECT_FAMILY.to_owned(),
            operation: "effect context",
            message: "payment admission context is missing".to_owned(),
        })
}

fn payment_replay_context(
    replay: &EffectReplay,
) -> Result<&PaymentReplayContext, RuntimeEffectError> {
    replay
        .context::<PaymentReplayContext>()
        .ok_or_else(|| RuntimeEffectError::Failed {
            family: PAYMENT_EFFECT_FAMILY.to_owned(),
            operation: "effect replay context",
            message: "payment replay context is missing".to_owned(),
        })
}

fn receipt_has_payment_rail_proof(receipt: &runx_contracts::Receipt, rail_proof_ref: &str) -> bool {
    receipt.acts.iter().any(|act| {
        act.criterion_bindings
            .iter()
            .flat_map(|criterion| criterion.verification_refs.iter())
            .any(|reference| {
                reference.uri == rail_proof_ref
                    && reference.proof_kind.as_ref()
                        == Some(&runx_contracts::ProofKind::EffectEvidence)
            })
    })
}

fn same_reference(left: &Reference, right: &Reference) -> bool {
    left.uri == right.uri
        && left.reference_type == right.reference_type
        && left.provider == right.provider
        && left.locator == right.locator
        && left.proof_kind == right.proof_kind
}

fn denied(message: impl Into<String>) -> RuntimeEffectError {
    RuntimeEffectError::Denied {
        family: PAYMENT_EFFECT_FAMILY.to_owned(),
        verb: AuthorityVerb::Commit,
        message: message.into(),
    }
}

fn finality_intent_error(source: EffectStateError) -> RuntimeEffectError {
    if matches!(&source, EffectStateError::RunSpendCapExceeded { .. }) {
        denied(source.to_string())
    } else {
        failed("recording state settlement intent", source)
    }
}

fn failed(operation: &'static str, source: impl std::fmt::Display) -> RuntimeEffectError {
    RuntimeEffectError::Failed {
        family: PAYMENT_EFFECT_FAMILY.to_owned(),
        operation,
        message: source.to_string(),
    }
}

#[derive(Clone, Debug)]
struct PaymentAdmissionContext {
    payment: Option<StepPaymentAuthorityContext>,
}

#[derive(Clone, Debug)]
struct StepPaymentAuthorityContext {
    idempotency_key: EffectIdempotencyKey,
    spend_capability_ref: Reference,
    rail: String,
    counterparty: String,
    amount_minor: u64,
    currency: String,
    run_spend: Option<EffectRunSpendReservation>,
    period_spend: Option<EffectPeriodSpendReservation>,
}

#[derive(Clone, Debug)]
struct PaymentReplayContext {
    rail_proof_ref: String,
    idempotency_key: EffectIdempotencyKey,
    spend_capability_ref: String,
    rail: String,
    counterparty: String,
    amount_minor: u64,
    currency: String,
    act_id: String,
    supervisor_proof: PaymentSupervisorProof,
}

#[derive(Clone, Debug)]
struct OwnedStepAuthoritySubmission {
    parent_authority: AuthorityTerm,
    child_authority: AuthorityTerm,
    reservation_decision: Option<Decision>,
    subset_proof: Option<AuthoritySubsetProof>,
    child_harness_ref: Reference,
    spend_capability_binding: Option<PaymentSpendCapabilityBinding>,
    consumed_spend_capability_refs: Vec<Reference>,
    spend_capability_ref: Option<Reference>,
    idempotency_key: Option<String>,
}

#[derive(Clone, Debug)]
struct ReservedAuthorityInput {
    parent_authority: AuthorityTerm,
    child_authority: AuthorityTerm,
    reservation_decision: Option<Decision>,
    subset_proof: Option<AuthoritySubsetProof>,
    child_harness_ref: Reference,
    spend_capability_binding: Option<PaymentSpendCapabilityBinding>,
    consumed_spend_capability_refs: Vec<Reference>,
}
