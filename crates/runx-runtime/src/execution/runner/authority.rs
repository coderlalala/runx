// rust-style-allow: large-file - the runner authority gate keeps step admission, payment-authority
// derivation, and in-flight recovery escalation in one module so the admit/recover decision surface
// is reviewed as a single trust boundary.
use std::collections::BTreeMap;
use std::path::Path;

use runx_contracts::{AuthoritySubsetProof, AuthorityTerm, AuthorityVerb, Decision, JsonObject};
use runx_core::policy::authority_term_has_verb;
use runx_core::state_machine::AuthorityAdmissionWitness;
use runx_parser::GraphStep;
use runx_payments::authority::{
    PaymentSpendCapabilityBinding, StepAuthorityAdmission, admit_step_authority,
};

use super::inputs::{
    optional_typed_input, optional_typed_vec_input, require_non_empty_string_field,
    require_object_input, require_reference_input, required_typed_input,
};
use crate::RuntimeError;
use crate::adapter::SkillOutput;
use crate::effects::{
    EffectIdempotencyKey, EffectMutationStatus, EffectRecoveryState, PAYMENT_EFFECT_FAMILY,
    PAYMENT_RAIL_SUPERVISOR_EVIDENCE_METADATA, PaymentRailProof, PaymentSupervisorProof,
    PaymentSupervisorProofMatch, PaymentSupervisorSettlementRequest,
    PaymentSupervisorVerificationInput, RuntimeEffectRegistry, consumed_spend_capability_recorded,
    effect_evidence_metadata_value, escalate_effect_mutation, lookup_effect_idempotency_entry,
    lookup_effect_mutation, read_payment_rail_packet, validate_effect_supervisor_proof,
    verify_effect_supervisor_proof,
};
use crate::reference_match::same_reference;

/// Trusted supervisor producer: attach rail settlement evidence from the
/// runtime-owned supervisor before the receipt-before-success gate verifies it.
/// The skill supplies only the rail proof claim; settlement evidence must come
/// from the configured supervisor.
pub(super) fn attach_effect_evidence_before_gate(
    step: &GraphStep,
    authority: Option<&StepAuthorityContext>,
    outputs: &JsonObject,
    output: &mut SkillOutput,
    effects: &RuntimeEffectRegistry,
) -> Result<(), RuntimeError> {
    let Some(authority) = authority else {
        return Ok(());
    };
    if !output.succeeded() || authority.verb != AuthorityVerb::Spend {
        return Ok(());
    }
    let Some(payment) = authority.payment.as_ref() else {
        return Ok(());
    };
    let Some(packet) = read_payment_rail_packet(outputs).map_err(|source| {
        authority_denied(
            step,
            AuthorityVerb::Spend,
            format!("reading payment rail packet for supervisor evidence failed: {source}"),
        )
    })?
    else {
        return Ok(());
    };
    let Some(claim) = packet.proof.as_ref() else {
        return Ok(());
    };
    let settlement_status = packet
        .result
        .as_ref()
        .and_then(|result| result.status.as_deref());
    let request = supervisor_settlement_request(payment, claim, settlement_status);
    let evidence = effects
        .payment_rail_settlement_evidence(request)
        .map_err(|source| {
            authority_denied(
                step,
                AuthorityVerb::Spend,
                format!("supervisor-verified rail settlement proof is required: {source}"),
            )
        })?;
    let value = effect_evidence_metadata_value(&evidence).map_err(|source| {
        authority_denied(
            step,
            AuthorityVerb::Spend,
            format!("encoding supervisor evidence failed: {source}"),
        )
    })?;
    output
        .metadata
        .insert(PAYMENT_RAIL_SUPERVISOR_EVIDENCE_METADATA.to_owned(), value);
    Ok(())
}

fn supervisor_settlement_request<'a>(
    payment: &'a StepPaymentAuthorityContext,
    claim: &'a PaymentRailProof,
    skill_settlement_status: Option<&'a str>,
) -> PaymentSupervisorSettlementRequest<'a> {
    PaymentSupervisorSettlementRequest {
        rail: &payment.rail,
        counterparty: &payment.counterparty,
        amount_minor: payment.amount_minor,
        currency: &payment.currency,
        idempotency_key: &payment.idempotency_key.key,
        proof_ref: &claim.proof_ref,
        skill_settlement_status,
    }
}

pub(super) fn enforce_step_authority_receipt_before_success(
    step: &GraphStep,
    authority: Option<&StepAuthorityContext>,
    output: &SkillOutput,
    outputs: &JsonObject,
    receipt: &runx_contracts::Receipt,
) -> Result<Option<PaymentSupervisorProof>, RuntimeError> {
    let Some(authority) = authority else {
        return Ok(None);
    };
    if !output.succeeded() || authority.verb != AuthorityVerb::Spend {
        return Ok(None);
    }
    let Some(payment) = authority.payment.as_ref() else {
        return Ok(None);
    };
    let act_id = format!("act_{}", step.id);
    let proof = verify_effect_supervisor_proof(PaymentSupervisorVerificationInput {
        outputs,
        metadata: &output.metadata,
        receipt,
        rail: &payment.rail,
        counterparty: &payment.counterparty,
        amount_minor: payment.amount_minor,
        currency: &payment.currency,
        idempotency_key: &payment.idempotency_key.key,
        spend_capability_ref: &payment.spend_capability_ref.uri,
        act_id: &act_id,
    })
    .map_err(|source| {
        authority_denied(
            step,
            AuthorityVerb::Spend,
            format!("spend success requires supervisor-verified rail proof: {source}"),
        )
    })?;
    Ok(Some(proof))
}

pub(super) fn validate_replayed_effect_supervisor_proof(
    step: &GraphStep,
    replay: &StepPaymentReplay,
) -> Result<(), RuntimeError> {
    validate_effect_supervisor_proof(
        &replay.supervisor_proof,
        PaymentSupervisorProofMatch {
            proof_ref: &replay.rail_proof_ref,
            rail: &replay.rail,
            counterparty: &replay.counterparty,
            amount_minor: replay.amount_minor,
            currency: &replay.currency,
            idempotency_key: &replay.idempotency_key.key,
            spend_capability_ref: &replay.spend_capability_ref,
            act_id: &replay.act_id,
            receipt_ref: &replay.receipt_ref,
            receipt_digest: &replay.receipt_digest,
        },
    )
    .map_err(|source| {
        authority_denied(
            step,
            AuthorityVerb::Spend,
            format!("sealed payment replay supervisor proof mismatch: {source}"),
        )
    })
}

fn validate_entry_matches_payment(
    step: &GraphStep,
    entry: &crate::effects::EffectIdempotencyEntry,
    payment: &StepPaymentAuthorityContext,
) -> Result<(), RuntimeError> {
    if entry.amount_minor != payment.amount_minor || entry.currency != payment.currency {
        return Err(authority_denied(
            step,
            AuthorityVerb::Spend,
            format!(
                "payment idempotency key {} was sealed for {} {}, but this spend requested {} {}",
                payment.idempotency_key.key,
                entry.amount_minor,
                entry.currency,
                payment.amount_minor,
                payment.currency
            ),
        ));
    }
    if entry.supervisor_proof.rail == payment.rail
        && entry.supervisor_proof.counterparty == payment.counterparty
        && entry.supervisor_proof.spend_capability_ref == payment.spend_capability_ref.uri
    {
        return Ok(());
    }
    Err(authority_denied(
        step,
        AuthorityVerb::Spend,
        format!(
            "payment idempotency key {} supervisor proof was sealed for {} {}, capability {}, but this spend requested {} {}, capability {}",
            payment.idempotency_key.key,
            entry.supervisor_proof.rail,
            entry.supervisor_proof.counterparty,
            entry.supervisor_proof.spend_capability_ref,
            payment.rail,
            payment.counterparty,
            payment.spend_capability_ref.uri
        ),
    ))
}

pub(super) fn enforce_step_authority_admission(
    step: &GraphStep,
    inputs: &JsonObject,
    env: &BTreeMap<String, String>,
    graph_dir: &Path,
) -> Result<Option<StepAuthorityContext>, RuntimeError> {
    let Some(input) = step_authority_submission(step, inputs)? else {
        return Ok(None);
    };
    let consumed_spend_capability_refs =
        consumed_spend_capability_refs_for_admission(&input, env, graph_dir)?;
    let act_id = format!("act_{}", step.id);
    let admission_error_verb =
        if authority_term_has_verb(&input.child_authority, AuthorityVerb::Spend) {
            AuthorityVerb::Spend
        } else {
            input
                .child_authority
                .verbs
                .first()
                .cloned()
                .unwrap_or(AuthorityVerb::Spend)
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
    .map_err(|source| authority_denied(step, admission_error_verb, source.to_string()))?;
    let payment = payment_context(&input);
    Ok(decision.verb.map(|verb| {
        let admission_witness = AuthorityAdmissionWitness {
            verb: verb.clone(),
            parent_term_id: decision.parent_term_id.to_owned(),
            child_term_id: decision.child_term_id.to_owned(),
            idempotency_key: decision.idempotency_key.map(str::to_owned),
            capability_ref: decision.spend_capability_ref.cloned(),
        };
        let payment = if verb == AuthorityVerb::Spend {
            payment
        } else {
            None
        };
        StepAuthorityContext {
            verb,
            payment,
            admission_witness,
        }
    }))
}

pub(super) fn sealed_payment_replay(
    step: &GraphStep,
    inputs: &JsonObject,
    env: &BTreeMap<String, String>,
    graph_dir: &Path,
) -> Result<Option<StepPaymentReplay>, RuntimeError> {
    let Some(input) = step_authority_submission(step, inputs)? else {
        return Ok(None);
    };
    let Some(payment) = payment_context(&input) else {
        return Ok(None);
    };
    let Some(entry) = lookup_effect_idempotency_entry(
        env,
        graph_dir,
        PAYMENT_EFFECT_FAMILY,
        &payment.idempotency_key,
    )
    .map_err(|source| {
        RuntimeError::effect_state("reading effect state for replay lookup", source)
    })?
    else {
        return Ok(None);
    };

    let act_id = format!("act_{}", step.id);
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
    .map_err(|source| authority_denied(step, AuthorityVerb::Spend, source.to_string()))?;
    if decision.verb != Some(AuthorityVerb::Spend) {
        return Ok(None);
    }
    validate_entry_matches_payment(step, &entry, &payment)?;

    Ok(Some(StepPaymentReplay {
        receipt_ref: entry.receipt_ref.clone(),
        receipt_created_at: entry.receipt_created_at.clone(),
        receipt_digest: entry.receipt_digest.clone(),
        rail_proof_ref: entry.rail_proof_ref.clone(),
        idempotency_key: entry.idempotency_key.clone(),
        spend_capability_ref: entry.supervisor_proof.spend_capability_ref.clone(),
        rail: entry.supervisor_proof.rail.clone(),
        counterparty: entry.supervisor_proof.counterparty.clone(),
        amount_minor: entry.supervisor_proof.amount_minor,
        currency: entry.supervisor_proof.currency.clone(),
        act_id,
        supervisor_proof: entry.supervisor_proof.clone(),
        outputs: entry.outputs.clone(),
    }))
}

// rust-style-allow: long-function - in-flight payment recovery escalation is one decision sequence
// over the recovered authority context; keeping it linear preserves the ordering of its guards.
pub(super) fn escalate_in_flight_payment_recovery(
    step: &GraphStep,
    inputs: &JsonObject,
    env: &BTreeMap<String, String>,
    graph_dir: &Path,
) -> Result<(), RuntimeError> {
    let Some(input) = step_authority_submission(step, inputs)? else {
        return Ok(());
    };
    let Some(payment) = payment_context(&input) else {
        return Ok(());
    };
    let Some(mutation) = lookup_effect_mutation(
        env,
        graph_dir,
        PAYMENT_EFFECT_FAMILY,
        &payment.idempotency_key,
    )
    .map_err(|source| {
        RuntimeError::effect_state("reading effect state for rail recovery", source)
    })?
    else {
        return Ok(());
    };
    if mutation.recovery_state != EffectRecoveryState::InFlight
        && mutation.status != EffectMutationStatus::Partial
    {
        return Ok(());
    }

    let act_id = format!("act_{}", step.id);
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
    .map_err(|source| authority_denied(step, AuthorityVerb::Spend, source.to_string()))?;
    if mutation.amount_minor != payment.amount_minor
        || mutation.currency != payment.currency
        || mutation.rail != payment.rail
        || mutation.counterparty != payment.counterparty
    {
        return Err(authority_denied(
            step,
            AuthorityVerb::Spend,
            format!(
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
            ),
        ));
    }

    let _ = escalate_effect_mutation(
        env,
        graph_dir,
        PAYMENT_EFFECT_FAMILY,
        &payment.idempotency_key,
    )
    .map_err(|source| RuntimeError::effect_state("escalating effect recovery", source))?;
    Err(authority_denied(
        step,
        AuthorityVerb::Spend,
        format!(
            "payment idempotency key {} has an in-flight rail mutation; recovery escalated without issuing a second rail mutation",
            payment.idempotency_key.key
        ),
    ))
}

fn consumed_spend_capability_refs_for_admission(
    input: &OwnedStepAuthoritySubmission,
    env: &BTreeMap<String, String>,
    graph_dir: &Path,
) -> Result<Vec<runx_contracts::Reference>, RuntimeError> {
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
    .map_err(|source| RuntimeError::effect_state("reading effect state for admission", source))?
        && !refs
            .iter()
            .any(|reference| same_reference(reference, spend_capability_ref))
    {
        refs.push(spend_capability_ref.clone());
    }
    Ok(refs)
}

fn payment_context(input: &OwnedStepAuthoritySubmission) -> Option<StepPaymentAuthorityContext> {
    let binding = input.spend_capability_binding.as_ref()?;
    let idempotency_key = input.idempotency_key.as_ref()?;
    let spend_capability_ref = input.spend_capability_ref.as_ref()?;
    Some(StepPaymentAuthorityContext {
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
    })
}

fn step_authority_submission(
    step: &GraphStep,
    inputs: &JsonObject,
) -> Result<Option<OwnedStepAuthoritySubmission>, RuntimeError> {
    let Some(reserved) = optional_payment_authority_object(step, inputs)? else {
        return Ok(None);
    };
    let reserved = parse_reserved_payment_authority(step, reserved)?;
    let spends = authority_term_has_verb(&reserved.child_authority, AuthorityVerb::Spend);
    let (spend_capability_ref, idempotency_key) = if spends {
        let idempotency = require_object_input(step, inputs, "idempotency")?;
        (
            Some(require_reference_input(
                step,
                inputs,
                "spend_capability_ref",
            )?),
            Some(require_non_empty_string_field(
                step,
                idempotency,
                "idempotency.key",
            )?),
        )
    } else {
        (None, None)
    };
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

fn optional_payment_authority_object<'a>(
    step: &GraphStep,
    inputs: &'a JsonObject,
) -> Result<Option<&'a JsonObject>, RuntimeError> {
    if inputs.contains_key("reserved_payment_authority") {
        return require_object_input(step, inputs, "reserved_payment_authority").map(Some);
    }
    if payment_admission_field_present(inputs) {
        return Err(authority_denied(
            step,
            AuthorityVerb::Spend,
            "reserved_payment_authority is required before payment rail execution".to_owned(),
        ));
    }
    Ok(None)
}

fn payment_admission_field_present(inputs: &JsonObject) -> bool {
    inputs.contains_key("spend_capability_ref") || inputs.contains_key("payment_challenge")
}

fn parse_reserved_payment_authority(
    step: &GraphStep,
    object: &JsonObject,
) -> Result<ReservedAuthorityInput, RuntimeError> {
    Ok(ReservedAuthorityInput {
        parent_authority: required_typed_input(
            step,
            object,
            "reserved_payment_authority.parent_authority",
            "parent_authority",
        )?,
        child_authority: required_typed_input(
            step,
            object,
            "reserved_payment_authority.child_authority",
            "child_authority",
        )?,
        reservation_decision: optional_typed_input(
            step,
            object,
            "reserved_payment_authority.reservation_decision",
            "reservation_decision",
        )?,
        subset_proof: optional_typed_input(
            step,
            object,
            "reserved_payment_authority.subset_proof",
            "subset_proof",
        )?,
        child_harness_ref: required_typed_input(
            step,
            object,
            "reserved_payment_authority.child_harness_ref",
            "child_harness_ref",
        )?,
        spend_capability_binding: optional_typed_input(
            step,
            object,
            "reserved_payment_authority.spend_capability_binding",
            "spend_capability_binding",
        )?,
        consumed_spend_capability_refs: optional_typed_vec_input(
            step,
            object,
            "reserved_payment_authority.consumed_spend_capability_refs",
            "consumed_spend_capability_refs",
        )?
        .unwrap_or_default(),
    })
}

pub(super) fn authority_denied(
    step: &GraphStep,
    verb: AuthorityVerb,
    reason: String,
) -> RuntimeError {
    RuntimeError::AuthorityDenied {
        verb,
        step_id: step.id.clone(),
        reason,
    }
}

#[derive(Clone, Debug)]
pub(super) struct StepAuthorityContext {
    pub(super) verb: AuthorityVerb,
    pub(super) payment: Option<StepPaymentAuthorityContext>,
    pub(super) admission_witness: AuthorityAdmissionWitness,
}

#[derive(Clone, Debug)]
pub(super) struct StepPaymentAuthorityContext {
    pub(super) idempotency_key: EffectIdempotencyKey,
    pub(super) spend_capability_ref: runx_contracts::Reference,
    pub(super) rail: String,
    pub(super) counterparty: String,
    pub(super) amount_minor: u64,
    pub(super) currency: String,
}

#[derive(Clone, Debug)]
pub(super) struct StepPaymentReplay {
    pub(super) receipt_ref: String,
    pub(super) receipt_created_at: String,
    pub(super) receipt_digest: String,
    pub(super) rail_proof_ref: String,
    pub(super) idempotency_key: EffectIdempotencyKey,
    pub(super) spend_capability_ref: String,
    pub(super) rail: String,
    pub(super) counterparty: String,
    pub(super) amount_minor: u64,
    pub(super) currency: String,
    pub(super) act_id: String,
    pub(super) supervisor_proof: PaymentSupervisorProof,
    pub(super) outputs: JsonObject,
}

#[derive(Clone, Debug)]
struct OwnedStepAuthoritySubmission {
    parent_authority: AuthorityTerm,
    child_authority: AuthorityTerm,
    reservation_decision: Option<Decision>,
    subset_proof: Option<AuthoritySubsetProof>,
    child_harness_ref: runx_contracts::Reference,
    spend_capability_binding: Option<PaymentSpendCapabilityBinding>,
    consumed_spend_capability_refs: Vec<runx_contracts::Reference>,
    spend_capability_ref: Option<runx_contracts::Reference>,
    idempotency_key: Option<String>,
}

#[derive(Clone, Debug)]
struct ReservedAuthorityInput {
    parent_authority: AuthorityTerm,
    child_authority: AuthorityTerm,
    reservation_decision: Option<Decision>,
    subset_proof: Option<AuthoritySubsetProof>,
    child_harness_ref: runx_contracts::Reference,
    spend_capability_binding: Option<PaymentSpendCapabilityBinding>,
    consumed_spend_capability_refs: Vec<runx_contracts::Reference>,
}
