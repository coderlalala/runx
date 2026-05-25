//! Non-authoritative wire-compatibility gate for the type-driven JSON Schema
//! emitter (Phase 1 of `rust-contract-pipeline-inversion`).
//!
//! For each covered contract: the Rust-emitted schema must preserve schema
//! identity (`$id`, `x-runx-schema`) and agree with the committed
//! `oss/schemas/*.json` on accept/reject for every corpus value. The schema
//! *document* shape may differ from the committed one; only the validated value
//! domain must match (dod1). The committed TypeBox-generated schemas remain the
//! source of truth until the pipeline inversion flips.

use std::path::PathBuf;

use runx_contracts::act::Act;
use runx_contracts::act_assignment::ActAssignment;
use runx_contracts::agent_context::AgentContextEnvelope;
use runx_contracts::artifact::Artifact;
use runx_contracts::aster::{
    FeedEntry, Opportunity, ReflectionEntry, Selection, SelectionCycle, SkillBinding, Target,
    TargetTransitionEntry, ThesisAssessment,
};
use runx_contracts::authority::{Authority, AuthoritySubsetProof};
use runx_contracts::credential_delivery::{
    CredentialDeliveryBrokerResponse, CredentialDeliveryObservation, CredentialDeliveryProfile,
    CredentialDeliveryRequest,
};
use runx_contracts::decision::Decision;
use runx_contracts::doctor::DoctorReport;
use runx_contracts::external_adapter::{
    ExternalAdapterCancellationFrame, ExternalAdapterCredentialRequest,
    ExternalAdapterHostResolutionFrame, ExternalAdapterInvocation, ExternalAdapterManifest,
    ExternalAdapterResponse,
};
use runx_contracts::handoff::{HandoffSignal, HandoffState};
use runx_contracts::host_protocol::{
    ApprovalGate, Question, ResolutionRequest, ResolutionResponse,
};
use runx_contracts::operational_policy::OperationalPolicy;
use runx_contracts::output::Output;
use runx_contracts::packet_index::PacketIndex;
use runx_contracts::policy_proof::{AuthorityProof, CredentialEnvelope, ScopeAdmission};
use runx_contracts::receipt::Receipt;
use runx_contracts::redaction::Redaction;
use runx_contracts::reference::Reference;
use runx_contracts::registry_binding::RegistryBinding;
use runx_contracts::review::ReviewReceiptOutput;
use runx_contracts::schema::RunxSchema;
use runx_contracts::signal::Signal;
use runx_contracts::suppression::SuppressionRecord;
use runx_contracts::thread_outbox_provider::{
    ThreadOutboxProviderFetch, ThreadOutboxProviderManifest, ThreadOutboxProviderObservation,
    ThreadOutboxProviderPush,
};
use runx_contracts::verification::Verification;
use serde_json::{Value, json};

struct Covered {
    file_name: &'static str,
    emitted: Value,
    corpus: Vec<(&'static str, Value)>,
}

fn covered() -> Vec<Covered> {
    vec![
        Covered {
            file_name: "reference.schema.json",
            emitted: Reference::json_schema(),
            corpus: reference_corpus(),
        },
        Covered {
            file_name: "doctor.schema.json",
            emitted: DoctorReport::json_schema(),
            corpus: doctor_corpus(),
        },
        Covered {
            file_name: "redaction.schema.json",
            emitted: Redaction::json_schema(),
            corpus: redaction_corpus(),
        },
        Covered {
            file_name: "artifact.schema.json",
            emitted: Artifact::json_schema(),
            corpus: artifact_corpus(),
        },
        Covered {
            file_name: "verification.schema.json",
            emitted: Verification::json_schema(),
            corpus: verification_corpus(),
        },
        Covered {
            file_name: "signal.schema.json",
            emitted: Signal::json_schema(),
            corpus: signal_corpus(),
        },
        Covered {
            file_name: "external-adapter-response.schema.json",
            emitted: ExternalAdapterResponse::json_schema(),
            corpus: external_adapter_response_corpus(),
        },
        Covered {
            file_name: "decision.schema.json",
            emitted: Decision::json_schema(),
            corpus: decision_corpus(),
        },
        Covered {
            file_name: "target.schema.json",
            emitted: Target::json_schema(),
            corpus: target_corpus(),
        },
        Covered {
            file_name: "opportunity.schema.json",
            emitted: Opportunity::json_schema(),
            corpus: opportunity_corpus(),
        },
        Covered {
            file_name: "thesis-assessment.schema.json",
            emitted: ThesisAssessment::json_schema(),
            corpus: thesis_assessment_corpus(),
        },
        Covered {
            file_name: "selection.schema.json",
            emitted: Selection::json_schema(),
            corpus: selection_corpus(),
        },
        Covered {
            file_name: "skill-binding.schema.json",
            emitted: SkillBinding::json_schema(),
            corpus: skill_binding_corpus(),
        },
        Covered {
            file_name: "target-transition-entry.schema.json",
            emitted: TargetTransitionEntry::json_schema(),
            corpus: target_transition_entry_corpus(),
        },
        Covered {
            file_name: "selection-cycle.schema.json",
            emitted: SelectionCycle::json_schema(),
            corpus: selection_cycle_corpus(),
        },
        Covered {
            file_name: "reflection-entry.schema.json",
            emitted: ReflectionEntry::json_schema(),
            corpus: reflection_entry_corpus(),
        },
        Covered {
            file_name: "feed-entry.schema.json",
            emitted: FeedEntry::json_schema(),
            corpus: feed_entry_corpus(),
        },
        Covered {
            file_name: "credential-delivery-profile.schema.json",
            emitted: CredentialDeliveryProfile::json_schema(),
            corpus: credential_delivery_profile_corpus(),
        },
        Covered {
            file_name: "credential-delivery-request.schema.json",
            emitted: CredentialDeliveryRequest::json_schema(),
            corpus: credential_delivery_request_corpus(),
        },
        Covered {
            file_name: "credential-delivery-broker-response.schema.json",
            emitted: CredentialDeliveryBrokerResponse::json_schema(),
            corpus: credential_delivery_broker_response_corpus(),
        },
        Covered {
            file_name: "credential-delivery-observation.schema.json",
            emitted: CredentialDeliveryObservation::json_schema(),
            corpus: credential_delivery_observation_corpus(),
        },
        Covered {
            file_name: "external-adapter-manifest.schema.json",
            emitted: ExternalAdapterManifest::json_schema(),
            corpus: external_adapter_manifest_corpus(),
        },
        Covered {
            file_name: "external-adapter-invocation.schema.json",
            emitted: ExternalAdapterInvocation::json_schema(),
            corpus: external_adapter_invocation_corpus(),
        },
        Covered {
            file_name: "external-adapter-credential-request.schema.json",
            emitted: ExternalAdapterCredentialRequest::json_schema(),
            corpus: external_adapter_credential_request_corpus(),
        },
        Covered {
            file_name: "external-adapter-host-resolution.schema.json",
            emitted: ExternalAdapterHostResolutionFrame::json_schema(),
            corpus: external_adapter_host_resolution_corpus(),
        },
        Covered {
            file_name: "external-adapter-cancellation.schema.json",
            emitted: ExternalAdapterCancellationFrame::json_schema(),
            corpus: external_adapter_cancellation_corpus(),
        },
        Covered {
            file_name: "question.schema.json",
            emitted: Question::json_schema(),
            corpus: question_corpus(),
        },
        Covered {
            file_name: "approval-gate.schema.json",
            emitted: ApprovalGate::json_schema(),
            corpus: approval_gate_corpus(),
        },
        Covered {
            file_name: "resolution-response.schema.json",
            emitted: ResolutionResponse::json_schema(),
            corpus: resolution_response_corpus(),
        },
        Covered {
            file_name: "resolution-request.schema.json",
            emitted: ResolutionRequest::json_schema(),
            corpus: resolution_request_corpus(),
        },
        Covered {
            file_name: "thread-outbox-provider-manifest.schema.json",
            emitted: ThreadOutboxProviderManifest::json_schema(),
            corpus: thread_outbox_manifest_corpus(),
        },
        Covered {
            file_name: "thread-outbox-provider-push.schema.json",
            emitted: ThreadOutboxProviderPush::json_schema(),
            corpus: thread_outbox_push_corpus(),
        },
        Covered {
            file_name: "thread-outbox-provider-fetch.schema.json",
            emitted: ThreadOutboxProviderFetch::json_schema(),
            corpus: thread_outbox_fetch_corpus(),
        },
        Covered {
            file_name: "thread-outbox-provider-observation.schema.json",
            emitted: ThreadOutboxProviderObservation::json_schema(),
            corpus: thread_outbox_observation_corpus(),
        },
        Covered {
            file_name: "act-assignment.schema.json",
            emitted: ActAssignment::json_schema(),
            corpus: act_assignment_corpus(),
        },
        Covered {
            file_name: "authority-subset-proof.schema.json",
            emitted: AuthoritySubsetProof::json_schema(),
            corpus: authority_subset_proof_corpus(),
        },
        Covered {
            file_name: "authority.schema.json",
            emitted: Authority::json_schema(),
            corpus: authority_corpus(),
        },
        Covered {
            file_name: "operational-policy.schema.json",
            emitted: OperationalPolicy::json_schema(),
            corpus: operational_policy_corpus(),
        },
        Covered {
            file_name: "act.schema.json",
            emitted: Act::json_schema(),
            corpus: act_corpus(),
        },
        Covered {
            file_name: "receipt.schema.json",
            emitted: Receipt::json_schema(),
            corpus: receipt_corpus(),
        },
        Covered {
            file_name: "handoff-signal.schema.json",
            emitted: HandoffSignal::json_schema(),
            corpus: handoff_signal_corpus(),
        },
        Covered {
            file_name: "handoff-state.schema.json",
            emitted: HandoffState::json_schema(),
            corpus: handoff_state_corpus(),
        },
        Covered {
            file_name: "suppression-record.schema.json",
            emitted: SuppressionRecord::json_schema(),
            corpus: suppression_record_corpus(),
        },
        Covered {
            file_name: "packet-index.schema.json",
            emitted: PacketIndex::json_schema(),
            corpus: packet_index_corpus(),
        },
        Covered {
            file_name: "registry-binding.schema.json",
            emitted: RegistryBinding::json_schema(),
            corpus: registry_binding_corpus(),
        },
        Covered {
            file_name: "review-receipt-output.schema.json",
            emitted: ReviewReceiptOutput::json_schema(),
            corpus: review_receipt_output_corpus(),
        },
        Covered {
            file_name: "agent-context-envelope.schema.json",
            emitted: AgentContextEnvelope::json_schema(),
            corpus: agent_context_envelope_corpus(),
        },
        Covered {
            file_name: "scope-admission.schema.json",
            emitted: ScopeAdmission::json_schema(),
            corpus: scope_admission_corpus(),
        },
        Covered {
            file_name: "credential-envelope.schema.json",
            emitted: CredentialEnvelope::json_schema(),
            corpus: credential_envelope_corpus(),
        },
        Covered {
            file_name: "authority-proof.schema.json",
            emitted: AuthorityProof::json_schema(),
            corpus: authority_proof_corpus(),
        },
        Covered {
            file_name: "output.schema.json",
            emitted: Output::json_schema(),
            corpus: output_corpus(),
        },
    ]
}

fn scope_admission_corpus() -> Vec<(&'static str, Value)> {
    // `status` is a closed enum the emitter models; the string fields commit
    // `minLength: 1` (a constraint the plain-`String` Rust type does not carry),
    // so corpus values keep those fields non-empty so both validators agree.
    let valid = json!({
        "status": "allow",
        "requested_scopes": ["issues:write"],
        "granted_scopes": ["issues:write"],
        "decision_summary": "granted",
    });
    vec![
        ("minimal valid", valid.clone()),
        ("full valid", {
            let mut v = valid.clone();
            v["grant_id"] = json!("grant_1");
            v["reasons"] = json!(["policy allowed"]);
            v
        }),
        ("missing status", drop_field(valid.clone(), "status")),
        (
            "missing requested_scopes",
            drop_field(valid.clone(), "requested_scopes"),
        ),
        // `decision_summary` is required by the Rust type but optional in the
        // committed schema; keep it present so both validators agree.
        (
            "unknown status",
            set_field(valid.clone(), "status", json!("maybe")),
        ),
        (
            "status as object",
            set_field(valid.clone(), "status", json!({})),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn credential_envelope_corpus() -> Vec<(&'static str, Value)> {
    // The committed schema requires `connection_id` and pins `kind` to a const,
    // and marks the string fields `minLength: 1`; the Rust type carries `kind`
    // as a plain `String` and `connection_id` as optional. Corpus values keep
    // those fields present, correctly-valued, and non-empty so both validators
    // agree on accept/reject.
    let valid = json!({
        "kind": "runx.credential-envelope.v1",
        "grant_id": "grant_1",
        "provider": "github",
        "auth_mode": "oauth",
        "material_kind": "token",
        "connection_id": "conn_1",
        "scopes": ["issues:write"],
        "material_ref": "ref:abc",
    });
    vec![
        ("minimal valid", valid.clone()),
        ("full valid (grant_reference)", {
            let mut v = valid.clone();
            v["grant_reference"] = json!({
                "grant_id": "grant_1",
                "scope_family": "github",
                "authority_kind": "constructive",
                "target_repo": "acme/widgets",
                "target_locator": "issue/1",
            });
            v
        }),
        ("missing grant_id", drop_field(valid.clone(), "grant_id")),
        ("missing provider", drop_field(valid.clone(), "provider")),
        (
            "missing material_ref",
            drop_field(valid.clone(), "material_ref"),
        ),
        (
            "grant_reference unknown authority_kind",
            set_field(
                valid.clone(),
                "grant_reference",
                json!({ "grant_id": "g", "authority_kind": "godmode" }),
            ),
        ),
        (
            "grant_reference missing grant_id",
            set_field(
                valid.clone(),
                "grant_reference",
                json!({ "scope_family": "github" }),
            ),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn authority_proof_requested() -> Value {
    json!({
        "connected_auth": true,
        "scopes": ["issues:write"],
        "mutating": false,
    })
}

fn authority_proof_credential_material() -> Value {
    json!({ "status": "resolved" })
}

fn authority_proof_redaction() -> Value {
    json!({
        "status": "applied",
        "secret_material": "omitted",
        "stdout": "hashed",
        "stderr": "hashed",
        "metadata_secret_keys": [],
    })
}

fn authority_proof_corpus() -> Vec<(&'static str, Value)> {
    // The committed schema pins several const strings (`schema_version`, the
    // redaction attestation fields) and marks string fields `minLength: 1`; the
    // Rust type carries those as plain `String`. Corpus values keep those fields
    // present and correctly-valued so both validators agree; reject cases target
    // the enum-typed nested fields (`requested.authority_kind`,
    // `scope_admission.status`) and required-field presence, which the emitter
    // models.
    let valid = json!({
        "schema_version": "runx.authority-proof.v1",
        "skill_name": "demo",
        "source_type": "github_issue",
        "requested": authority_proof_requested(),
        "scope_admission": {
            "status": "allow",
            "requested_scopes": ["issues:write"],
            "granted_scopes": ["issues:write"],
            "decision_summary": "granted",
        },
        "credential_material": authority_proof_credential_material(),
        "redaction": authority_proof_redaction(),
    });
    vec![
        ("minimal valid", valid.clone()),
        ("full valid (sandbox + approval + targeting)", {
            let mut v = valid.clone();
            v["run_id"] = json!("run_1");
            v["requested"] = json!({
                "connected_auth": true,
                "scopes": ["issues:write"],
                "mutating": true,
                "scope_family": "github",
                "authority_kind": "constructive",
                "target_repo": "acme/widgets",
                "target_locator": "issue/1",
                "sandbox_profile": "workspace-write",
            });
            v["credential_material"] = json!({
                "status": "resolved",
                "grant_id": "grant_1",
                "provider": "github",
                "connection_id": "conn_1",
                "scopes": ["issues:write"],
                "authority_kind": "constructive",
                "grant_reference": {
                    "grant_id": "grant_1",
                    "scope_family": "github",
                    "authority_kind": "constructive",
                },
            });
            v["sandbox"] = json!({
                "profile": "workspace-write",
                "cwd_policy": "skill-directory",
                "require_enforcement": true,
                "network": { "declared": false },
                "filesystem": { "readonly_paths": true, "private_tmp": true },
                "runtime": { "enforcer": "seatbelt" },
                "approval_required": false,
            });
            v["approval_gate"] = json!({
                "gate_id": "gate_1",
                "gate_type": "human",
                "decision": "approved",
            });
            v
        }),
        (
            "missing schema_version",
            drop_field(valid.clone(), "schema_version"),
        ),
        ("missing requested", drop_field(valid.clone(), "requested")),
        (
            "missing scope_admission",
            drop_field(valid.clone(), "scope_admission"),
        ),
        ("missing redaction", drop_field(valid.clone(), "redaction")),
        (
            "requested unknown authority_kind",
            set_field(
                valid.clone(),
                "requested",
                set_field(
                    authority_proof_requested(),
                    "authority_kind",
                    json!("godmode"),
                ),
            ),
        ),
        (
            "scope_admission unknown status",
            set_field(
                valid.clone(),
                "scope_admission",
                json!({
                    "status": "maybe",
                    "requested_scopes": [],
                    "granted_scopes": [],
                    "decision_summary": "x",
                }),
            ),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn output_corpus() -> Vec<(&'static str, Value)> {
    // The per-value `OutputFieldSpec` commits `minProperties: 1` (a numeric bound
    // the emitter does not model); corpus spec values stay non-empty so both
    // validators agree.
    vec![
        ("empty map", json!({})),
        (
            "bare type values",
            json!({ "result": "string", "count": "integer" }),
        ),
        (
            "typed spec value",
            json!({ "report": { "type": "object", "required": true } }),
        ),
        (
            "spec with enum + wrap_as",
            json!({ "status": { "type": "string", "enum": ["ok", "fail"], "wrap_as": "value" } }),
        ),
        ("unknown bare type rejected", json!({ "result": "blob" })),
        (
            "spec unknown type rejected",
            json!({ "report": { "type": "blob" } }),
        ),
        (
            "spec additional property rejected",
            json!({ "report": { "type": "object", "bogus": true } }),
        ),
        ("not an object", json!("nope")),
    ]
}

fn handoff_signal_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.handoff_signal.v1",
        "signal_id": "sig_1",
        "handoff_id": "ho_1",
        "source": "issue_comment",
        "disposition": "acknowledged",
        "recorded_at": "2026-01-01T00:00:00Z",
    });
    vec![
        ("minimal valid", valid.clone()),
        ("full valid", {
            let mut v = valid.clone();
            v["boundary_kind"] = json!("pull_request");
            v["target_repo"] = json!("acme/widgets");
            v["thread_locator"] = json!("acme/widgets#1");
            v["actor"] = json!({ "actor_id": "u1", "display_name": "User", "role": "maintainer" });
            v["notes"] = json!("looks good");
            v["labels"] = json!(["bug"]);
            v["source_ref"] = json!({ "type": "pull_request", "uri": "runx:pr:1", "label": "PR" });
            v["metadata"] = json!({ "k": 1 });
            v
        }),
        ("missing schema", drop_field(valid.clone(), "schema")),
        ("missing signal_id", drop_field(valid.clone(), "signal_id")),
        (
            "missing disposition",
            drop_field(valid.clone(), "disposition"),
        ),
        (
            "empty signal_id",
            set_field(valid.clone(), "signal_id", json!("")),
        ),
        (
            "unknown source",
            set_field(valid.clone(), "source", json!("smoke_signal")),
        ),
        (
            "unknown disposition",
            set_field(valid.clone(), "disposition", json!("ghosted")),
        ),
        (
            "malformed recorded_at",
            set_field(valid.clone(), "recorded_at", json!("nope")),
        ),
        (
            "source_ref empty uri rejected",
            set_field(
                valid.clone(),
                "source_ref",
                json!({ "type": "pull_request", "uri": "" }),
            ),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn handoff_state_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.handoff_state.v1",
        "handoff_id": "ho_1",
        "status": "awaiting_response",
        "signal_count": 0,
    });
    vec![
        ("minimal valid", valid.clone()),
        ("full valid", {
            let mut v = valid.clone();
            v["boundary_kind"] = json!("pull_request");
            v["target_repo"] = json!("acme/widgets");
            v["last_signal_id"] = json!("sig_9");
            v["last_signal_at"] = json!("2026-01-02T00:00:00Z");
            v["last_signal_disposition"] = json!("merged");
            v["suppression_record_id"] = json!("sup_1");
            v["suppression_reason"] = json!("operator_block");
            v["summary"] = json!("ongoing");
            v
        }),
        ("missing schema", drop_field(valid.clone(), "schema")),
        (
            "missing handoff_id",
            drop_field(valid.clone(), "handoff_id"),
        ),
        (
            "missing signal_count",
            drop_field(valid.clone(), "signal_count"),
        ),
        (
            "empty handoff_id",
            set_field(valid.clone(), "handoff_id", json!("")),
        ),
        (
            "unknown status",
            set_field(valid.clone(), "status", json!("ghosted")),
        ),
        // `signal_count` commits a `minimum: 0` bound the type-driven emitter
        // does not model (same gap as `minItems`/`minProperties`); keep the
        // corpus on the non-negative side so both validators agree.
        (
            "signal_count as string",
            set_field(valid.clone(), "signal_count", json!("two")),
        ),
        (
            "unknown suppression_reason",
            set_field(valid.clone(), "suppression_reason", json!("mood")),
        ),
        (
            "malformed last_signal_at",
            set_field(valid.clone(), "last_signal_at", json!("nope")),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn suppression_record_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.suppression_record.v1",
        "record_id": "sup_1",
        "scope": "contact",
        "key": "user@example.com",
        "reason": "requested_no_contact",
        "recorded_at": "2026-01-01T00:00:00Z",
    });
    vec![
        ("minimal valid", valid.clone()),
        ("full valid", {
            let mut v = valid.clone();
            v["expires_at"] = json!("2026-02-01T00:00:00Z");
            v["source_signal_id"] = json!("sig_1");
            v["notes"] = json!("per request");
            v
        }),
        ("missing schema", drop_field(valid.clone(), "schema")),
        ("missing record_id", drop_field(valid.clone(), "record_id")),
        ("missing reason", drop_field(valid.clone(), "reason")),
        (
            "empty record_id",
            set_field(valid.clone(), "record_id", json!("")),
        ),
        ("empty key", set_field(valid.clone(), "key", json!(""))),
        (
            "unknown scope",
            set_field(valid.clone(), "scope", json!("galaxy")),
        ),
        (
            "unknown reason",
            set_field(valid.clone(), "reason", json!("mood")),
        ),
        (
            "malformed expires_at",
            set_field(valid.clone(), "expires_at", json!("nope")),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn packet_index_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.packet.index.v1",
        "packets": [{
            "id": "p1",
            "package": "@runxhq/skill",
            "version": "1.0.0",
            "path": "skills/p1",
            "sha256": "abc",
        }],
    });
    vec![
        ("valid with packet", valid.clone()),
        (
            "valid empty packets",
            set_field(valid.clone(), "packets", json!([])),
        ),
        ("missing schema", drop_field(valid.clone(), "schema")),
        ("missing packets", drop_field(valid.clone(), "packets")),
        (
            "wrong schema const",
            set_field(valid.clone(), "schema", json!("runx.other.v1")),
        ),
        (
            "packet missing id",
            set_field(
                valid.clone(),
                "packets",
                json!([{ "package": "p", "version": "1", "path": "x", "sha256": "y" }]),
            ),
        ),
        (
            "packet additional property",
            set_field(
                valid.clone(),
                "packets",
                json!([{ "id": "p1", "package": "p", "version": "1", "path": "x", "sha256": "y", "bogus": true }]),
            ),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn registry_binding_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.registry_binding.v1",
        "state": "registry_bound",
        "skill": { "id": "s1", "name": "Skill", "description": "does a thing" },
        "upstream": {
            "host": "github.com",
            "owner": "acme",
            "repo": "skills",
            "path": "s1/SKILL.md",
            "commit": "deadbeef",
            "blob_sha": "cafef00d",
            "source_of_truth": true,
        },
        "registry": {
            "owner": "acme",
            "trust_tier": "first_party",
            "version": "1.0.0",
            "profile_path": "profiles/s1",
            "materialized_package_is_registry_artifact": true,
        },
        "harness": { "status": "harness_verified", "case_count": 3 },
    });
    vec![
        ("minimal valid", valid.clone()),
        ("full valid (open extras + optionals)", {
            let mut v = valid.clone();
            v["extra_top"] = json!("ok");
            v["upstream"]["branch"] = json!("main");
            v["upstream"]["pr_url"] = json!("https://github.com/acme/skills/pull/1");
            v["registry"]["install_command"] = json!("runx install s1");
            v["harness"]["assertion_count"] = json!(9);
            v["harness"]["case_names"] = json!(["c1", "c2"]);
            v
        }),
        ("missing schema", drop_field(valid.clone(), "schema")),
        ("missing state", drop_field(valid.clone(), "state")),
        ("missing harness", drop_field(valid.clone(), "harness")),
        (
            "wrong schema const",
            set_field(valid.clone(), "schema", json!("runx.other.v1")),
        ),
        (
            "unknown state",
            set_field(valid.clone(), "state", json!("limbo")),
        ),
        (
            "unknown trust_tier",
            set_field(
                valid.clone(),
                "registry",
                json!({
                    "owner": "acme",
                    "trust_tier": "platinum",
                    "version": "1.0.0",
                    "profile_path": "profiles/s1",
                    "materialized_package_is_registry_artifact": true,
                }),
            ),
        ),
        (
            "unknown harness status",
            set_field(
                valid.clone(),
                "harness",
                json!({ "status": "exploded", "case_count": 1 }),
            ),
        ),
        (
            "skill missing description",
            set_field(
                valid.clone(),
                "skill",
                json!({ "id": "s1", "name": "Skill" }),
            ),
        ),
    ]
}

fn review_receipt_output_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "verdict": "needs_update",
        "failure_summary": "the harness step failed because the prompt drifted",
        "improvement_proposals": [{
            "target": "SKILL.md",
            "change": "tighten the output contract",
            "rationale": "prevents drift",
            "risk": "none",
        }],
        "next_harness_checks": ["output parses", "verdict present"],
    });
    vec![
        ("full valid", valid.clone()),
        ("valid pass (empty proposals)", {
            let mut v = valid.clone();
            v["verdict"] = json!("pass");
            v["improvement_proposals"] = json!([]);
            v
        }),
        ("valid with open extras", {
            let mut v = valid.clone();
            v["extra"] = json!("ok");
            v["improvement_proposals"] = json!([{ "target": "t", "change": "c", "extra": 1 }]);
            v
        }),
        ("missing verdict", drop_field(valid.clone(), "verdict")),
        (
            "missing failure_summary",
            drop_field(valid.clone(), "failure_summary"),
        ),
        (
            "missing next_harness_checks",
            drop_field(valid.clone(), "next_harness_checks"),
        ),
        (
            "unknown verdict",
            set_field(valid.clone(), "verdict", json!("maybe")),
        ),
        (
            "proposal missing change",
            set_field(
                valid.clone(),
                "improvement_proposals",
                json!([{ "target": "SKILL.md" }]),
            ),
        ),
        (
            "next_harness_checks as object",
            set_field(valid.clone(), "next_harness_checks", json!({})),
        ),
    ]
}

fn agent_context_meta() -> Value {
    json!({
        "artifact_id": "art_1",
        "run_id": "run_1",
        "step_id": null,
        "producer": { "skill": "demo", "runner": "local" },
        "created_at": "2026-01-01T00:00:00Z",
        "hash": "sha256:abc",
        "size_bytes": 12,
        "parent_artifact_id": null,
        "receipt_id": null,
        "redacted": false,
    })
}

fn agent_context_envelope_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "run_id": "run_1",
        "skill": "demo",
        "instructions": "do the thing",
        "inputs": {},
        "allowed_tools": ["fs.read"],
        "current_context": [],
        "historical_context": [],
        "provenance": [],
        "trust_boundary": "trusted",
    });
    vec![
        ("minimal valid", valid.clone()),
        ("full valid (nested context + output spec)", {
            let mut v = valid.clone();
            v["step_id"] = json!("step_1");
            v["current_context"] = json!([{
                "type": "artifact",
                "version": "1",
                "data": { "k": 1 },
                "meta": agent_context_meta(),
            }]);
            v["historical_context"] = json!([{
                "type": null,
                "version": "1",
                "data": {},
                "meta": agent_context_meta(),
            }]);
            v["provenance"] = json!([{ "input": "a", "output": "b", "from_step": "s0" }]);
            v["context"] = json!({
                "memory": { "root_path": "/r", "path": "MEMORY.md", "sha256": "abc", "content": "x" },
            });
            v["voice_profile"] =
                json!({ "root_path": "/r", "path": "voice.md", "sha256": "abc", "content": "" });
            v["quality_profile"] =
                json!({ "source": "SKILL.md#quality-profile", "sha256": "abc", "content": "" });
            v["execution_location"] =
                json!({ "skill_directory": "/skills/demo", "tool_roots": ["/tools"] });
            v["output"] =
                json!({ "result": "string", "report": { "type": "object", "required": true } });
            v
        }),
        ("missing run_id", drop_field(valid.clone(), "run_id")),
        (
            "missing trust_boundary",
            drop_field(valid.clone(), "trust_boundary"),
        ),
        (
            "empty run_id",
            set_field(valid.clone(), "run_id", json!("")),
        ),
        (
            "empty instructions",
            set_field(valid.clone(), "instructions", json!("")),
        ),
        (
            "allowed_tools empty item",
            set_field(valid.clone(), "allowed_tools", json!([""])),
        ),
        (
            "context entry bad version const",
            set_field(
                valid.clone(),
                "current_context",
                json!([{ "type": "x", "version": "2", "data": {}, "meta": agent_context_meta() }]),
            ),
        ),
        (
            "context entry missing meta",
            set_field(
                valid.clone(),
                "current_context",
                json!([{ "type": "x", "version": "1", "data": {} }]),
            ),
        ),
        (
            "output bad type name",
            set_field(valid.clone(), "output", json!({ "result": "blob" })),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn receipt_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.receipt.v1",
        "id": "hrn_rcpt_1",
        "created_at": "2026-01-01T00:00:00Z",
        "canonicalization": "runx.receipt.c14n.v1",
        "issuer": {
            "type": "local",
            "kid": "fixture-key",
            "public_key_sha256": "sha256:abc",
        },
        "signature": { "alg": "Ed25519", "value": "sig:abc" },
        "digest": "sha256:abc",
        "idempotency": {
            "intent_key": "sha256:intent",
            "trigger_fingerprint": "sha256:trigger",
            "content_hash": "sha256:content",
        },
        "subject": {
            "kind": "skill",
            "ref": a_ref(),
            "commitments": [],
        },
        "authority": {
            "actor_ref": a_ref(),
            "grant_refs": [],
            "scope_refs": [],
            "authority_proof_refs": [],
            "attenuation": { "parent_authority_ref": null, "subset_proof": null },
            "terms": [],
            "enforcement": {
                "profile_hash": "sha256:profile",
                "redaction_refs": [],
                "setup_refs": [],
                "teardown_refs": [],
            },
        },
        "signals": [],
        "decisions": [],
        "acts": [],
        "seal": {
            "disposition": "closed",
            "reason_code": "process_closed",
            "summary": "closed",
            "closed_at": "2026-01-01T00:00:00Z",
            "last_observed_at": "2026-01-01T00:00:00Z",
            "criteria": [],
        },
    });
    vec![
        ("minimal valid", valid.clone()),
        ("full valid (act + seal criteria + lineage)", {
            let mut v = valid.clone();
            v["acts"] = json!([{
                "id": "act_1",
                "form": "observation",
                "intent": an_intent(),
                "summary": "did the thing",
                "criterion_bindings": [{
                    "criterion_id": "c1",
                    "status": "verified",
                    "evidence_refs": [],
                    "verification_refs": [],
                    "summary": "ok",
                }],
                "source_refs": [],
                "target_refs": [],
                "artifact_refs": [],
                "closure": act_closure(),
            }]);
            v["seal"]["criteria"] = json!([{
                "criterion_id": "c1",
                "status": "verified",
                "evidence_refs": [],
                "verification_refs": [],
            }]);
            v["lineage"] = json!({
                "children": [],
                "sync": [],
            });
            v
        }),
        ("missing schema", drop_field(valid.clone(), "schema")),
        ("missing id", drop_field(valid.clone(), "id")),
        ("missing seal", drop_field(valid.clone(), "seal")),
        ("missing digest", drop_field(valid.clone(), "digest")),
        (
            "empty id rejected",
            set_field(valid.clone(), "id", json!("")),
        ),
        (
            "empty digest rejected",
            set_field(valid.clone(), "digest", json!("")),
        ),
        (
            "wrong schema const",
            set_field(valid.clone(), "schema", json!("runx.act.v1")),
        ),
        (
            "malformed created_at",
            set_field(valid.clone(), "created_at", json!("nope")),
        ),
        (
            "unknown issuer type",
            set_field(
                valid.clone(),
                "issuer",
                json!({ "type": "alien", "kid": "k", "public_key_sha256": "sha256:x" }),
            ),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn act_closure() -> Value {
    json!({
        "disposition": "closed",
        "reason_code": "done",
        "summary": "completed",
        "closed_at": "2026-01-01T00:00:00Z",
    })
}

fn act_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.act.v1",
        "act_id": "act_1",
        "form": "observation",
        "intent": an_intent(),
        "summary": "did the thing",
        "closure": act_closure(),
        "criterion_bindings": [],
        "source_refs": [],
        "target_refs": [],
        "surface_refs": [],
        "artifact_refs": [],
        "verification_refs": [],
        "harness_refs": [],
        "performed_at": "2026-01-01T00:00:00Z",
    });
    vec![
        ("minimal valid", valid.clone()),
        ("full valid (revision + bindings)", {
            let mut v = valid.clone();
            v["form"] = json!("revision");
            v["criterion_bindings"] = json!([{
                "criterion_id": "c1",
                "status": "verified",
                "evidence_refs": [],
                "verification_refs": [],
                "summary": "looks good",
            }]);
            v["revision"] = json!({
                "change_request": {
                    "request_id": "req_1",
                    "summary": "ship it",
                    "target_surfaces": [
                        { "surface_ref": a_ref(), "mutating": true, "rationale": "open pr" },
                    ],
                    "success_criteria": [],
                },
                "change_plan": {
                    "plan_id": "plan_1",
                    "summary": "open and merge",
                    "steps": ["open pr"],
                    "risks": [],
                },
                "target_surfaces": [],
                "invariants": ["keep tests green"],
                "handoff_refs": [],
                "revision_refs": [],
            });
            v
        }),
        (
            "missing schema (optional)",
            drop_field(valid.clone(), "schema"),
        ),
        ("missing act_id", drop_field(valid.clone(), "act_id")),
        ("missing closure", drop_field(valid.clone(), "closure")),
        (
            "missing performed_at",
            drop_field(valid.clone(), "performed_at"),
        ),
        (
            "empty act_id",
            set_field(valid.clone(), "act_id", json!("")),
        ),
        (
            "empty summary",
            set_field(valid.clone(), "summary", json!("")),
        ),
        (
            "unknown form",
            set_field(valid.clone(), "form", json!("teleport")),
        ),
        (
            "malformed performed_at",
            set_field(valid.clone(), "performed_at", json!("nope")),
        ),
        (
            "empty criterion binding criterion_id",
            set_field(
                valid.clone(),
                "criterion_bindings",
                json!([{
                    "criterion_id": "",
                    "status": "verified",
                    "evidence_refs": [],
                    "verification_refs": [],
                }]),
            ),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn operational_policy_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.operational_policy.v1",
        "schema_version": "runx.operational_policy.v1",
        "policy_id": "nitrosend.intake",
        "sources": [{
            "source_id": "slack.intake",
            "provider": "slack",
            "allowed_locators": ["C123"],
            "allowed_actions": ["issue-intake"],
            "source_thread": {
                "required": true,
                "publish_mode": "reply",
                "missing_behavior": "fail_closed",
            },
        }],
        "runners": [{
            "runner_id": "local.default",
            "kind": "local",
            "state": "available",
            "allowed_actions": ["issue-intake"],
            "target_repos": ["acme/widgets"],
            "scafld_required": true,
        }],
        "owner_routes": [{
            "route_id": "default.route",
            "owners": ["alice"],
            "target_repos": ["acme/widgets"],
        }],
        "targets": [{
            "repo": "acme/widgets",
            "runner_ids": ["local.default"],
            "allowed_actions": ["issue-intake"],
            "default_owner_route": "default.route",
            "scafld_required": true,
        }],
        "dedupe": {
            "strategy": "source_fingerprint",
            "key_fields": ["source_id"],
            "on_duplicate": "reuse",
        },
        "outcomes": {
            "observe_provider": true,
            "verification_required": true,
            "close_source_issue": "when_verified",
            "publish_final_source_thread_update": true,
        },
        "permissions": {
            "auto_merge": false,
            "mutate_target_repo": true,
            "require_human_merge_gate": true,
        },
    });
    vec![
        ("minimal valid", valid.clone()),
        ("full valid (created_at + optionals)", {
            let mut v = valid.clone();
            v["created_at"] = json!("2026-01-01T00:00:00Z");
            v["sources"][0]["minimum_confidence"] = json!(0.5);
            v["sources"][0]["sentry"] = json!({ "production_only": true, "unresolved_only": true });
            v["owner_routes"][0]["labels"] = json!(["bug"]);
            v["owner_routes"][0]["project"] = json!("Roadmap");
            v["targets"][0]["base_branch"] = json!("main");
            v
        }),
        ("missing schema", drop_field(valid.clone(), "schema")),
        (
            "missing schema_version",
            drop_field(valid.clone(), "schema_version"),
        ),
        ("missing policy_id", drop_field(valid.clone(), "policy_id")),
        ("missing dedupe", drop_field(valid.clone(), "dedupe")),
        (
            "empty policy_id rejected",
            set_field(valid.clone(), "policy_id", json!("")),
        ),
        (
            "wrong schema const",
            set_field(valid.clone(), "schema", json!("runx.other.v1")),
        ),
        (
            "unknown dedupe strategy",
            set_field(
                valid.clone(),
                "dedupe",
                json!({
                    "strategy": "magic",
                    "key_fields": ["source_id"],
                    "on_duplicate": "reuse",
                }),
            ),
        ),
        (
            "malformed created_at",
            set_field(valid.clone(), "created_at", json!("nope")),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn authority_term() -> Value {
    json!({
        "term_id": "term_1",
        "principal_ref": a_ref(),
        "resource_ref": a_ref(),
        "resource_family": "github_repo",
        "verbs": ["read", "write"],
        "bounds": {},
        "conditions": [],
        "approvals": [],
        "capabilities": [],
        "issued_by_ref": a_ref(),
    })
}

fn authority_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.authority.v1",
        "actor_ref": a_ref(),
        "authority_proof_refs": [],
        "grant_refs": [],
        "scope_refs": [],
        "policy_refs": [],
        "terms": [authority_term()],
        "attenuation": {
            "parent_authority_ref": null,
            "subset_proof": null,
        },
    });
    vec![
        ("minimal valid (nullable attenuation)", valid.clone()),
        ("full valid", {
            let mut v = valid.clone();
            v["attenuation"] = json!({
                "parent_authority_ref": a_ref(),
                "subset_proof": {
                    "parent_authority_ref": a_ref(),
                    "comparison_algorithm": "runx.subset.v1",
                    "result": "subset",
                    "compared_terms": [
                        { "child_term_id": "c1", "parent_term_id": "p1", "relation": "subset" },
                    ],
                    "checked_at": "2026-01-01T00:00:00Z",
                },
            });
            v["mandate_ref"] = a_ref();
            v["terms"] = json!([{
                "term_id": "term_1",
                "principal_ref": a_ref(),
                "resource_ref": a_ref(),
                "resource_family": "payment",
                "verbs": ["spend"],
                "bounds": {
                    "payment": {
                        "currency": "USD",
                        "rails": ["card"],
                        "max_per_call_minor": 2500,
                    },
                },
                "conditions": [
                    { "condition_id": "cond_1", "predicate": "within_budget" },
                ],
                "approvals": [
                    { "approval_ref": a_ref(), "approved_at": "2026-01-01T00:00:00Z" },
                ],
                "capabilities": ["payment_single_use_spend"],
                "expires_at": "2026-02-01T00:00:00Z",
                "issued_by_ref": a_ref(),
            }]);
            v
        }),
        ("missing actor_ref", drop_field(valid.clone(), "actor_ref")),
        (
            "missing attenuation",
            drop_field(valid.clone(), "attenuation"),
        ),
        (
            "empty term_id rejected",
            set_field(
                valid.clone(),
                "terms",
                json!([set_field(authority_term(), "term_id", json!(""))]),
            ),
        ),
        (
            "unknown resource_family",
            set_field(
                valid.clone(),
                "terms",
                json!([set_field(
                    authority_term(),
                    "resource_family",
                    json!("nope")
                )]),
            ),
        ),
        (
            "unknown verb",
            set_field(
                valid.clone(),
                "terms",
                json!([set_field(authority_term(), "verbs", json!(["fly"]))]),
            ),
        ),
        (
            "empty payment currency rejected",
            set_field(
                valid.clone(),
                "terms",
                json!([set_field(
                    authority_term(),
                    "bounds",
                    json!({ "payment": { "currency": "", "rails": ["card"] } }),
                )]),
            ),
        ),
        (
            "malformed approval approved_at",
            set_field(
                valid.clone(),
                "terms",
                json!([set_field(
                    authority_term(),
                    "approvals",
                    json!([{ "approval_ref": a_ref(), "approved_at": "nope" }]),
                )]),
            ),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn authority_subset_proof_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "parent_authority_ref": a_ref(),
        "comparison_algorithm": "runx.subset.v1",
        "result": "subset",
        "compared_terms": [
            { "child_term_id": "c1", "parent_term_id": "p1", "relation": "subset" },
        ],
        "checked_at": "2026-01-01T00:00:00Z",
    });
    vec![
        ("minimal valid", valid.clone()),
        (
            "with proof_ref",
            set_field(valid.clone(), "proof_ref", a_ref()),
        ),
        (
            "missing comparison_algorithm",
            drop_field(valid.clone(), "comparison_algorithm"),
        ),
        ("missing result", drop_field(valid.clone(), "result")),
        (
            "missing checked_at",
            drop_field(valid.clone(), "checked_at"),
        ),
        (
            "empty comparison_algorithm",
            set_field(valid.clone(), "comparison_algorithm", json!("")),
        ),
        (
            "unknown result value",
            set_field(valid.clone(), "result", json!("superset")),
        ),
        (
            "comparison empty child_term_id",
            set_field(
                valid.clone(),
                "compared_terms",
                json!([{ "child_term_id": "", "parent_term_id": "p1", "relation": "subset" }]),
            ),
        ),
        (
            "comparison unknown relation",
            set_field(
                valid.clone(),
                "compared_terms",
                json!([{ "child_term_id": "c1", "parent_term_id": "p1", "relation": "disjoint" }]),
            ),
        ),
        (
            "malformed checked_at",
            set_field(valid.clone(), "checked_at", json!("nope")),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn act_assignment_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.act_assignment.v1",
        "skill_ref": "skill:1",
        "runner": "local",
        "requested_at": "2026-01-01T00:00:00Z",
        "host": { "kind": "cli" },
        "idempotency": {
            "algorithm": "sha256",
            "intent_key": "sha256:intent",
            "content_hash": "sha256:content",
        },
    });
    vec![
        ("minimal valid", valid.clone()),
        ("full valid", {
            let mut v = valid.clone();
            v["source_ref"] = json!("runx:signal:1");
            v["input_overrides"] = json!({ "k": 1 });
            v["host"] = json!({
                "kind": "github_issue_comment",
                "trigger_ref": "owner/repo#1",
                "scope_set": ["issues:write"],
                "actor": { "actor_id": "u1", "display_name": "User" },
            });
            v["idempotency"] = json!({
                "algorithm": "sha256",
                "intent_key": "sha256:intent",
                "trigger_key": "sha256:trigger",
                "content_hash": "sha256:content",
            });
            v
        }),
        ("missing skill_ref", drop_field(valid.clone(), "skill_ref")),
        ("missing schema", drop_field(valid.clone(), "schema")),
        (
            "missing idempotency",
            drop_field(valid.clone(), "idempotency"),
        ),
        (
            "empty skill_ref",
            set_field(valid.clone(), "skill_ref", json!("")),
        ),
        (
            "empty runner",
            set_field(valid.clone(), "runner", json!("")),
        ),
        (
            "unknown host kind",
            set_field(valid.clone(), "host", json!({ "kind": "carrier-pigeon" })),
        ),
        (
            "malformed requested_at",
            set_field(valid.clone(), "requested_at", json!("nope")),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn fingerprint() -> Value {
    // The committed schemas require `derived_from` to be non-empty (`minItems:
    // 1`), a numeric/array bound the type-driven emitter does not model; keep the
    // corpus outside that gap so both validators agree.
    json!({
        "algorithm": "sha256",
        "canonicalization": "json-c14n",
        "value": "abc",
        "derived_from": [a_ref()],
    })
}

fn drop_field(mut value: Value, field: &str) -> Value {
    value.as_object_mut().unwrap().remove(field);
    value
}

fn set_field(mut value: Value, field: &str, replacement: Value) -> Value {
    value[field] = replacement;
    value
}

fn target_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.target.v1",
        "target_id": "tgt_1",
        "target_ref": a_ref(),
        "title": "a target",
        "lifecycle_state": "candidate",
        "authority_refs": [],
        "fingerprint": fingerprint(),
        "cooldown": { "state": "none" },
        "verification_recipe_refs": [],
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z",
    });
    vec![
        ("minimal valid", valid.clone()),
        ("full valid", {
            let mut v = valid.clone();
            v["summary"] = json!("a summary");
            v["cooldown"] = json!({ "state": "cooling_down", "until": "2026-02-01T00:00:00Z", "reason_code": "rl" });
            v["owner_refs"] = json!([a_ref()]);
            v
        }),
        ("missing target_id", drop_field(valid.clone(), "target_id")),
        ("missing schema", drop_field(valid.clone(), "schema")),
        (
            "missing created_at",
            drop_field(valid.clone(), "created_at"),
        ),
        (
            "empty target_id",
            set_field(valid.clone(), "target_id", json!("")),
        ),
        ("empty title", set_field(valid.clone(), "title", json!(""))),
        (
            "unknown lifecycle_state",
            set_field(valid.clone(), "lifecycle_state", json!("frozen")),
        ),
        (
            "malformed created_at",
            set_field(valid.clone(), "created_at", json!("nope")),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn opportunity_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.opportunity.v1",
        "opportunity_id": "opp_1",
        "target_ref": a_ref(),
        "summary": "an opportunity",
        "proposed_form": "revision",
        "value_score": 5,
        "risk_score": 2,
        "freshness_expires_at": "2026-01-02T00:00:00Z",
        "fingerprint": fingerprint(),
        "source_refs": [],
        "evidence_refs": [],
        "discovered_at": "2026-01-01T00:00:00Z",
    });
    vec![
        ("minimal valid", valid.clone()),
        (
            "missing opportunity_id",
            drop_field(valid.clone(), "opportunity_id"),
        ),
        ("missing schema", drop_field(valid.clone(), "schema")),
        (
            "missing value_score",
            drop_field(valid.clone(), "value_score"),
        ),
        (
            "empty opportunity_id",
            set_field(valid.clone(), "opportunity_id", json!("")),
        ),
        (
            "empty summary",
            set_field(valid.clone(), "summary", json!("")),
        ),
        (
            "unknown proposed_form",
            set_field(valid.clone(), "proposed_form", json!("nope")),
        ),
        (
            "value_score as string",
            set_field(valid.clone(), "value_score", json!("five")),
        ),
        (
            "malformed discovered_at",
            set_field(valid.clone(), "discovered_at", json!("nope")),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn thesis_assessment_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.thesis_assessment.v1",
        "assessment_id": "as_1",
        "target_ref": a_ref(),
        "opportunity_ref": a_ref(),
        "thesis_ref": a_ref(),
        "score": 80,
        "rubric_refs": [],
        "proof_strength": "strong",
        "authority_cost": "low",
        "rationale": "because",
        "evidence_refs": [],
        "assessed_at": "2026-01-01T00:00:00Z",
    });
    vec![
        ("minimal valid", valid.clone()),
        (
            "missing assessment_id",
            drop_field(valid.clone(), "assessment_id"),
        ),
        ("missing score", drop_field(valid.clone(), "score")),
        ("missing schema", drop_field(valid.clone(), "schema")),
        (
            "empty assessment_id",
            set_field(valid.clone(), "assessment_id", json!("")),
        ),
        (
            "empty rationale",
            set_field(valid.clone(), "rationale", json!("")),
        ),
        (
            "unknown proof_strength",
            set_field(valid.clone(), "proof_strength", json!("epic")),
        ),
        (
            "unknown authority_cost",
            set_field(valid.clone(), "authority_cost", json!("infinite")),
        ),
        (
            "malformed assessed_at",
            set_field(valid.clone(), "assessed_at", json!("nope")),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn selection_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.selection.v1",
        "selection_id": "sel_1",
        "cycle_ref": a_ref(),
        "opportunity_ref": a_ref(),
        "candidate_refs": [a_ref()],
        "rank": 1,
        "score": 90,
        "selected": true,
        "reason": "top ranked",
        "decision_ref": null,
        "evidence_refs": [],
        "selected_at": "2026-01-01T00:00:00Z",
    });
    vec![
        ("minimal valid (decision_ref null)", valid.clone()),
        (
            "decision_ref populated",
            set_field(valid.clone(), "decision_ref", a_ref()),
        ),
        (
            "missing required-but-nullable decision_ref",
            drop_field(valid.clone(), "decision_ref"),
        ),
        (
            "missing selection_id",
            drop_field(valid.clone(), "selection_id"),
        ),
        (
            "empty selection_id",
            set_field(valid.clone(), "selection_id", json!("")),
        ),
        (
            "empty reason",
            set_field(valid.clone(), "reason", json!("")),
        ),
        (
            "selected as string",
            set_field(valid.clone(), "selected", json!("yes")),
        ),
        (
            "malformed selected_at",
            set_field(valid.clone(), "selected_at", json!("nope")),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn skill_binding_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.skill_binding.v1",
        "binding_id": "sb_1",
        "skill_ref": a_ref(),
        "scope_family": "github_repo",
        "allowed_act_forms": ["revision"],
        "authority_refs": [],
        "policy_refs": [],
        "harness_template_ref": null,
        "active": true,
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z",
    });
    vec![
        ("minimal valid", valid.clone()),
        (
            "harness_template_ref populated",
            set_field(valid.clone(), "harness_template_ref", a_ref()),
        ),
        (
            "missing required-but-nullable harness_template_ref",
            drop_field(valid.clone(), "harness_template_ref"),
        ),
        (
            "missing binding_id",
            drop_field(valid.clone(), "binding_id"),
        ),
        (
            "empty binding_id",
            set_field(valid.clone(), "binding_id", json!("")),
        ),
        (
            "unknown scope_family",
            set_field(valid.clone(), "scope_family", json!("nope")),
        ),
        (
            "active as string",
            set_field(valid.clone(), "active", json!("yes")),
        ),
        (
            "malformed updated_at",
            set_field(valid.clone(), "updated_at", json!("nope")),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn target_transition_entry_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.target_transition_entry.v1",
        "entry_id": "tte_1",
        "target_ref": a_ref(),
        "from_state": null,
        "to_state": "eligible",
        "reason_code": "promoted",
        "summary": "moved up",
        "source_refs": [],
        "decision_ref": null,
        "receipt_ref": null,
        "recorded_at": "2026-01-01T00:00:00Z",
    });
    vec![
        ("minimal valid (nullables null)", valid.clone()),
        (
            "from_state populated",
            set_field(valid.clone(), "from_state", json!("candidate")),
        ),
        (
            "missing required-but-nullable from_state",
            drop_field(valid.clone(), "from_state"),
        ),
        ("missing entry_id", drop_field(valid.clone(), "entry_id")),
        (
            "empty entry_id",
            set_field(valid.clone(), "entry_id", json!("")),
        ),
        (
            "empty reason_code",
            set_field(valid.clone(), "reason_code", json!("")),
        ),
        (
            "unknown to_state",
            set_field(valid.clone(), "to_state", json!("nope")),
        ),
        (
            "malformed recorded_at",
            set_field(valid.clone(), "recorded_at", json!("nope")),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn selection_cycle_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.selection_cycle.v1",
        "cycle_id": "cyc_1",
        "state": "open",
        "started_at": "2026-01-01T00:00:00Z",
        "closed_at": null,
        "input_refs": [],
        "target_refs": [],
        "opportunity_refs": [],
        "ranked_selection_refs": [],
        "chosen_selection_ref": null,
        "decision_ref": null,
        "receipt_ref": null,
        "no_action_closure": null,
        "fingerprint": fingerprint(),
    });
    vec![
        ("minimal valid (nullables null)", valid.clone()),
        (
            "closed_at populated",
            set_field(valid.clone(), "closed_at", json!("2026-02-01T00:00:00Z")),
        ),
        (
            "missing required-but-nullable closed_at",
            drop_field(valid.clone(), "closed_at"),
        ),
        ("missing cycle_id", drop_field(valid.clone(), "cycle_id")),
        (
            "empty cycle_id",
            set_field(valid.clone(), "cycle_id", json!("")),
        ),
        (
            "unknown state",
            set_field(valid.clone(), "state", json!("nope")),
        ),
        (
            "malformed started_at",
            set_field(valid.clone(), "started_at", json!("nope")),
        ),
        (
            "malformed closed_at when populated",
            set_field(valid.clone(), "closed_at", json!("nope")),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn reflection_entry_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.reflection_entry.v1",
        "reflection_id": "ref_1",
        "target_ref": null,
        "opportunity_ref": null,
        "selection_ref": null,
        "decision_ref": null,
        "receipt_refs": [],
        "act_refs": [],
        "summary": "learned something",
        "lessons": [],
        "follow_up_refs": [],
        "evidence_refs": [],
        "recorded_at": "2026-01-01T00:00:00Z",
    });
    vec![
        ("minimal valid (nullables null)", valid.clone()),
        (
            "target_ref populated",
            set_field(valid.clone(), "target_ref", a_ref()),
        ),
        (
            "missing required-but-nullable target_ref",
            drop_field(valid.clone(), "target_ref"),
        ),
        (
            "missing reflection_id",
            drop_field(valid.clone(), "reflection_id"),
        ),
        (
            "empty reflection_id",
            set_field(valid.clone(), "reflection_id", json!("")),
        ),
        (
            "empty summary",
            set_field(valid.clone(), "summary", json!("")),
        ),
        (
            "empty lessons item",
            set_field(valid.clone(), "lessons", json!([""])),
        ),
        (
            "malformed recorded_at",
            set_field(valid.clone(), "recorded_at", json!("nope")),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn feed_entry_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.feed_entry.v1",
        "feed_entry_id": "fe_1",
        "public_at": "2026-01-01T00:00:00Z",
        "title": "shipped a thing",
        "summary": "details here",
        "target_ref": null,
        "opportunity_ref": null,
        "selection_ref": null,
        "decision_refs": [a_ref()],
        "receipt_refs": [a_ref()],
        "act_refs": [{ "receipt_ref": a_ref(), "act_id": "act_1" }],
        "verification_refs": [a_ref()],
        "evidence_refs": [a_ref()],
        "artifact_refs": [],
        "redaction_policy_ref": a_ref(),
        "redaction_refs": [],
    });
    vec![
        ("minimal valid (nullables null)", valid.clone()),
        (
            "target_ref populated",
            set_field(valid.clone(), "target_ref", a_ref()),
        ),
        (
            "missing required-but-nullable selection_ref",
            drop_field(valid.clone(), "selection_ref"),
        ),
        (
            "missing feed_entry_id",
            drop_field(valid.clone(), "feed_entry_id"),
        ),
        (
            "empty feed_entry_id",
            set_field(valid.clone(), "feed_entry_id", json!("")),
        ),
        ("empty title", set_field(valid.clone(), "title", json!(""))),
        (
            "malformed public_at",
            set_field(valid.clone(), "public_at", json!("nope")),
        ),
        (
            "missing redaction_policy_ref",
            drop_field(valid.clone(), "redaction_policy_ref"),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn credential_delivery_profile_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.credential_delivery.profile.v1",
        "profile_id": "github-env",
        "provider": "github",
        "auth_mode": "oauth_bearer",
        "purpose": "provider_api",
        "delivery_mode": "process_env",
        "material_roles": ["access_token"],
        "env_bindings": [{ "role": "access_token", "env_var": "GITHUB_TOKEN", "required": true }],
        "redaction_policy_ref": a_ref(),
    });
    vec![
        ("valid", valid.clone()),
        (
            "missing profile_id",
            drop_field(valid.clone(), "profile_id"),
        ),
        ("missing schema", drop_field(valid.clone(), "schema")),
        (
            "empty profile_id",
            set_field(valid.clone(), "profile_id", json!("")),
        ),
        (
            "empty provider",
            set_field(valid.clone(), "provider", json!("")),
        ),
        (
            "unknown purpose",
            set_field(valid.clone(), "purpose", json!("nope")),
        ),
        (
            "unknown delivery_mode",
            set_field(valid.clone(), "delivery_mode", json!("nope")),
        ),
        (
            "unknown material role",
            set_field(valid.clone(), "material_roles", json!(["nope"])),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn credential_delivery_request_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.credential_delivery.request.v1",
        "request_id": "req_1",
        "harness_ref": a_ref(),
        "host_ref": a_ref(),
        "grant_ref": a_ref(),
        "credential_ref": a_ref(),
        "profile_id": "github-env",
        "provider": "github",
        "purpose": "provider_api",
        "requested_roles": ["access_token"],
        "requested_at": "2026-01-01T00:00:00Z",
    });
    vec![
        ("valid", valid.clone()),
        (
            "missing request_id",
            drop_field(valid.clone(), "request_id"),
        ),
        ("missing schema", drop_field(valid.clone(), "schema")),
        (
            "empty request_id",
            set_field(valid.clone(), "request_id", json!("")),
        ),
        (
            "empty profile_id",
            set_field(valid.clone(), "profile_id", json!("")),
        ),
        (
            "unknown purpose",
            set_field(valid.clone(), "purpose", json!("nope")),
        ),
        (
            "malformed requested_at",
            set_field(valid.clone(), "requested_at", json!("nope")),
        ),
        (
            "missing requested_at",
            drop_field(valid.clone(), "requested_at"),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn credential_delivery_broker_response_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.credential_delivery.broker_response.v1",
        "response_id": "resp_1",
        "request_id": "req_1",
        "status": "delivered",
        "credential_refs": [a_ref()],
        "issued_at": "2026-01-01T00:00:00Z",
    });
    vec![
        ("minimal valid", valid.clone()),
        ("full valid", {
            let mut v = valid.clone();
            v["delivery_mode"] = json!("process_env");
            v["handles"] = json!([{ "role": "access_token", "delivery_handle_ref": a_ref(), "env_var": "GITHUB_TOKEN" }]);
            v["material_ref_hash"] = json!("sha256:abc");
            v["denied_reasons"] = json!([]);
            v["expires_at"] = json!("2026-02-01T00:00:00Z");
            v
        }),
        (
            "missing response_id",
            drop_field(valid.clone(), "response_id"),
        ),
        ("missing schema", drop_field(valid.clone(), "schema")),
        (
            "empty response_id",
            set_field(valid.clone(), "response_id", json!("")),
        ),
        (
            "unknown status",
            set_field(valid.clone(), "status", json!("nope")),
        ),
        (
            "empty denied_reasons item",
            set_field(valid.clone(), "denied_reasons", json!([""])),
        ),
        (
            "malformed issued_at",
            set_field(valid.clone(), "issued_at", json!("nope")),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn credential_delivery_observation_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.credential_delivery.observation.v1",
        "observation_id": "obs_1",
        "request_id": "req_1",
        "status": "delivered",
        "harness_ref": a_ref(),
        "profile_id": "github-env",
        "provider": "github",
        "purpose": "provider_api",
        "credential_refs": [a_ref()],
        "delivered_roles": ["access_token"],
        "observed_at": "2026-01-01T00:00:00Z",
    });
    vec![
        ("minimal valid", valid.clone()),
        (
            "missing observation_id",
            drop_field(valid.clone(), "observation_id"),
        ),
        ("missing schema", drop_field(valid.clone(), "schema")),
        (
            "empty observation_id",
            set_field(valid.clone(), "observation_id", json!("")),
        ),
        (
            "empty profile_id",
            set_field(valid.clone(), "profile_id", json!("")),
        ),
        (
            "unknown status",
            set_field(valid.clone(), "status", json!("nope")),
        ),
        (
            "malformed observed_at",
            set_field(valid.clone(), "observed_at", json!("nope")),
        ),
        (
            "missing harness_ref",
            drop_field(valid.clone(), "harness_ref"),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn external_adapter_manifest_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.external_adapter.manifest.v1",
        "protocol_version": "runx.external_adapter.v1",
        "adapter_id": "ad_1",
        "name": "Adapter",
        "version": "1.0.0",
        "supported_source_types": ["github_issue"],
        "transport": { "kind": "process", "command": "node" },
        "timeouts": { "startup_ms": 1000, "invocation_ms": 5000 },
        "sandbox_intent": { "profile": "readonly", "network": false, "cwd_policy": "workspace" },
    });
    vec![
        ("minimal valid", valid.clone()),
        (
            "missing adapter_id",
            drop_field(valid.clone(), "adapter_id"),
        ),
        ("missing schema", drop_field(valid.clone(), "schema")),
        (
            "wrong protocol_version",
            set_field(valid.clone(), "protocol_version", json!("nope")),
        ),
        (
            "empty adapter_id",
            set_field(valid.clone(), "adapter_id", json!("")),
        ),
        (
            "empty version",
            set_field(valid.clone(), "version", json!("")),
        ),
        (
            "empty supported_source_types item",
            set_field(valid.clone(), "supported_source_types", json!([""])),
        ),
        (
            "transport unknown kind",
            set_field(
                valid.clone(),
                "transport",
                json!({ "kind": "carrier-pigeon" }),
            ),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn external_adapter_invocation_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.external_adapter.invocation.v1",
        "protocol_version": "runx.external_adapter.v1",
        "invocation_id": "inv_1",
        "adapter_id": "ad_1",
        "run_id": "run_1",
        "step_id": "step_1",
        "source_type": "github_issue",
        "skill_ref": "skill:1",
        "harness_ref": a_ref(),
        "host_ref": a_ref(),
        "inputs": {},
    });
    vec![
        ("minimal valid", valid.clone()),
        (
            "missing invocation_id",
            drop_field(valid.clone(), "invocation_id"),
        ),
        ("missing schema", drop_field(valid.clone(), "schema")),
        (
            "wrong schema const",
            set_field(valid.clone(), "schema", json!("runx.x.v1")),
        ),
        (
            "empty run_id",
            set_field(valid.clone(), "run_id", json!("")),
        ),
        (
            "empty skill_ref",
            set_field(valid.clone(), "skill_ref", json!("")),
        ),
        (
            "missing harness_ref",
            drop_field(valid.clone(), "harness_ref"),
        ),
        (
            "inputs as array",
            set_field(valid.clone(), "inputs", json!([])),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn external_adapter_credential_request_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.external_adapter.credential_request.v1",
        "protocol_version": "runx.external_adapter.v1",
        "request_id": "req_1",
        "adapter_id": "ad_1",
        "invocation_id": "inv_1",
        "credential_refs": [{ "credential_ref": a_ref(), "provider": "github", "purpose": "provider_api" }],
        "requested_at": "2026-01-01T00:00:00Z",
    });
    vec![
        ("valid", valid.clone()),
        (
            "missing request_id",
            drop_field(valid.clone(), "request_id"),
        ),
        ("missing schema", drop_field(valid.clone(), "schema")),
        (
            "empty request_id",
            set_field(valid.clone(), "request_id", json!("")),
        ),
        (
            "credential ref unknown purpose",
            set_field(
                valid.clone(),
                "credential_refs",
                json!([{ "credential_ref": a_ref(), "provider": "github", "purpose": "nope" }]),
            ),
        ),
        (
            "credential ref empty provider",
            set_field(
                valid.clone(),
                "credential_refs",
                json!([{ "credential_ref": a_ref(), "provider": "", "purpose": "provider_api" }]),
            ),
        ),
        (
            "malformed requested_at",
            set_field(valid.clone(), "requested_at", json!("nope")),
        ),
        (
            "missing requested_at",
            drop_field(valid.clone(), "requested_at"),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn external_adapter_host_resolution_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.external_adapter.host_resolution.v1",
        "protocol_version": "runx.external_adapter.v1",
        "frame_id": "frame_1",
        "invocation_id": "inv_1",
        "adapter_id": "ad_1",
        "request": {
            "kind": "input",
            "id": "q_1",
            "questions": [{ "id": "name", "prompt": "Name?", "required": true, "type": "string" }],
        },
        "requested_at": "2026-01-01T00:00:00Z",
    });
    vec![
        ("valid input request", valid.clone()),
        ("valid approval request", {
            let mut v = valid.clone();
            v["request"] = json!({
                "kind": "approval",
                "id": "ap_1",
                "gate": { "id": "g1", "reason": "needs approval" },
            });
            v
        }),
        ("missing frame_id", drop_field(valid.clone(), "frame_id")),
        ("missing schema", drop_field(valid.clone(), "schema")),
        (
            "empty frame_id",
            set_field(valid.clone(), "frame_id", json!("")),
        ),
        (
            "request unknown kind",
            set_field(
                valid.clone(),
                "request",
                json!({ "kind": "nope", "id": "x" }),
            ),
        ),
        (
            "request missing id",
            set_field(
                valid.clone(),
                "request",
                json!({ "kind": "input", "questions": [] }),
            ),
        ),
        (
            "malformed requested_at",
            set_field(valid.clone(), "requested_at", json!("nope")),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn external_adapter_cancellation_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.external_adapter.cancellation.v1",
        "protocol_version": "runx.external_adapter.v1",
        "frame_id": "frame_1",
        "invocation_id": "inv_1",
        "adapter_id": "ad_1",
        "reason": "user cancelled",
        "requested_at": "2026-01-01T00:00:00Z",
    });
    vec![
        ("valid", valid.clone()),
        ("missing frame_id", drop_field(valid.clone(), "frame_id")),
        ("missing schema", drop_field(valid.clone(), "schema")),
        (
            "wrong protocol_version",
            set_field(valid.clone(), "protocol_version", json!("x")),
        ),
        (
            "empty frame_id",
            set_field(valid.clone(), "frame_id", json!("")),
        ),
        (
            "empty reason",
            set_field(valid.clone(), "reason", json!("")),
        ),
        (
            "malformed requested_at",
            set_field(valid.clone(), "requested_at", json!("nope")),
        ),
        ("missing reason", drop_field(valid.clone(), "reason")),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn question_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({ "id": "q1", "prompt": "What?", "required": true, "type": "string" });
    vec![
        ("minimal valid", valid.clone()),
        (
            "full valid",
            set_field(valid.clone(), "description", json!("a hint")),
        ),
        ("missing id", drop_field(valid.clone(), "id")),
        ("missing prompt", drop_field(valid.clone(), "prompt")),
        ("missing type", drop_field(valid.clone(), "type")),
        ("empty id", set_field(valid.clone(), "id", json!(""))),
        (
            "empty prompt",
            set_field(valid.clone(), "prompt", json!("")),
        ),
        ("empty type", set_field(valid.clone(), "type", json!(""))),
        (
            "required as string",
            set_field(valid.clone(), "required", json!("yes")),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn approval_gate_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({ "id": "g1", "reason": "needs approval" });
    vec![
        ("minimal valid", valid.clone()),
        ("full valid", {
            let mut v = valid.clone();
            v["type"] = json!("sandbox");
            v["summary"] = json!({ "k": 1 });
            v
        }),
        ("missing id", drop_field(valid.clone(), "id")),
        ("missing reason", drop_field(valid.clone(), "reason")),
        ("empty id", set_field(valid.clone(), "id", json!(""))),
        (
            "empty reason",
            set_field(valid.clone(), "reason", json!("")),
        ),
        (
            "summary as array",
            set_field(valid.clone(), "summary", json!([1, 2])),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn resolution_response_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({ "actor": "human", "payload": { "answer": "yes" } });
    vec![
        ("valid human", valid.clone()),
        (
            "valid agent",
            set_field(valid.clone(), "actor", json!("agent")),
        ),
        (
            "payload as string accepted",
            set_field(valid.clone(), "payload", json!("text")),
        ),
        (
            "payload as null accepted",
            set_field(valid.clone(), "payload", json!(null)),
        ),
        ("missing actor", drop_field(valid.clone(), "actor")),
        ("missing payload", drop_field(valid.clone(), "payload")),
        (
            "unknown actor",
            set_field(valid.clone(), "actor", json!("robot")),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn agent_context_envelope() -> Value {
    // A fully-valid agent-context envelope: the committed schema models this
    // strictly while the Rust `AgentActInvocation.envelope` is an opaque
    // `JsonValue` (accepts anything). Keep agent_act corpus values to envelopes
    // both validators accept, so the corpus stays outside that modeling gap.
    json!({
        "run_id": "run_1",
        "skill": "demo",
        "instructions": "do the thing",
        "inputs": {},
        "allowed_tools": ["fs.read"],
        "current_context": [],
        "historical_context": [],
        "provenance": [],
        "trust_boundary": "trusted",
    })
}

fn resolution_request_corpus() -> Vec<(&'static str, Value)> {
    let input = json!({
        "kind": "input",
        "id": "q_1",
        "questions": [{ "id": "name", "prompt": "Name?", "required": true, "type": "string" }],
    });
    let approval = json!({
        "kind": "approval",
        "id": "ap_1",
        "gate": { "id": "g1", "reason": "needs approval" },
    });
    let agent_act = json!({
        "kind": "agent_act",
        "id": "aa_1",
        "invocation": {
            "id": "inv_1",
            "source_type": "agent",
            "envelope": agent_context_envelope(),
        },
    });
    vec![
        ("valid input request", input.clone()),
        ("valid approval request", approval.clone()),
        ("valid agent_act request", agent_act.clone()),
        (
            "input missing questions",
            drop_field(input.clone(), "questions"),
        ),
        (
            "input empty id rejected",
            set_field(input.clone(), "id", json!("")),
        ),
        (
            "unknown kind rejected",
            set_field(input.clone(), "kind", json!("teleport")),
        ),
        (
            "approval empty gate reason rejected",
            set_field(
                approval.clone(),
                "gate",
                json!({ "id": "g1", "reason": "" }),
            ),
        ),
        (
            "approval missing gate",
            drop_field(approval.clone(), "gate"),
        ),
        (
            "agent_act missing invocation",
            drop_field(agent_act.clone(), "invocation"),
        ),
        (
            "input additional property rejected",
            set_field(input.clone(), "bogus", json!(true)),
        ),
        (
            "question additional property rejected",
            set_field(
                input.clone(),
                "questions",
                json!([{ "id": "name", "prompt": "Name?", "required": true, "type": "string", "bogus": 1 }]),
            ),
        ),
    ]
}

fn thread_outbox_manifest_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.thread_outbox_provider.manifest.v1",
        "protocol_version": "runx.thread_outbox_provider.v1",
        "adapter_id": "ad_1",
        "provider": "github",
        "name": "Provider",
        "version": "1.0.0",
        "supported_operations": ["push"],
        "transport": { "kind": "process", "command": "node" },
        "receipt_capabilities": { "idempotent_push": true, "readback": true, "stable_provider_event_hash": true },
        "redaction_capabilities": { "redacts_credentials": true, "redacts_provider_payloads": true, "supports_redaction_refs": true },
    });
    vec![
        ("minimal valid", valid.clone()),
        (
            "missing adapter_id",
            drop_field(valid.clone(), "adapter_id"),
        ),
        ("missing schema", drop_field(valid.clone(), "schema")),
        (
            "wrong protocol_version",
            set_field(valid.clone(), "protocol_version", json!("x")),
        ),
        (
            "empty provider",
            set_field(valid.clone(), "provider", json!("")),
        ),
        (
            "empty version",
            set_field(valid.clone(), "version", json!("")),
        ),
        (
            "unknown operation",
            set_field(valid.clone(), "supported_operations", json!(["fly"])),
        ),
        (
            "transport unknown kind",
            set_field(valid.clone(), "transport", json!({ "kind": "http" })),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn thread_outbox_push_corpus() -> Vec<(&'static str, Value)> {
    let thread_locator =
        json!({ "provider": "github", "thread_ref": a_ref(), "locator": "owner/repo#1" });
    let profile = json!({ "provider": "github", "purpose": "provider_api", "profile_id": "github-env", "delivery_mode": "process_env", "credential_refs": [] });
    let receipt_context = json!({ "harness_ref": a_ref(), "host_ref": a_ref() });
    let valid = json!({
        "schema": "runx.thread_outbox_provider.push.v1",
        "protocol_version": "runx.thread_outbox_provider.v1",
        "push_id": "push_1",
        "adapter_id": "ad_1",
        "provider": "github",
        "outbox_entry_id": "oe_1",
        "thread_locator": thread_locator,
        "idempotency": { "key": "k1" },
        "payload": { "format": "markdown", "body": "hello" },
        "provider_profile": profile,
        "credential_delivery_refs": [],
        "receipt_context": receipt_context,
        "requested_at": "2026-01-01T00:00:00Z",
    });
    vec![
        ("minimal valid", valid.clone()),
        ("missing push_id", drop_field(valid.clone(), "push_id")),
        ("missing schema", drop_field(valid.clone(), "schema")),
        (
            "empty push_id",
            set_field(valid.clone(), "push_id", json!("")),
        ),
        (
            "empty idempotency key",
            set_field(valid.clone(), "idempotency", json!({ "key": "" })),
        ),
        (
            "unknown payload format",
            set_field(
                valid.clone(),
                "payload",
                json!({ "format": "rtf", "body": "x" }),
            ),
        ),
        (
            "empty payload body",
            set_field(
                valid.clone(),
                "payload",
                json!({ "format": "markdown", "body": "" }),
            ),
        ),
        (
            "malformed requested_at",
            set_field(valid.clone(), "requested_at", json!("nope")),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn thread_outbox_fetch_corpus() -> Vec<(&'static str, Value)> {
    let profile = json!({ "provider": "github", "purpose": "provider_api", "profile_id": "github-env", "delivery_mode": "process_env", "credential_refs": [] });
    let receipt_context = json!({ "harness_ref": a_ref(), "host_ref": a_ref() });
    let valid = json!({
        "schema": "runx.thread_outbox_provider.fetch.v1",
        "protocol_version": "runx.thread_outbox_provider.v1",
        "fetch_id": "fetch_1",
        "adapter_id": "ad_1",
        "provider": "github",
        "target": { "thread_locator": { "provider": "github", "thread_ref": a_ref(), "locator": "owner/repo#1" } },
        "idempotency": { "key": "k1" },
        "provider_profile": profile,
        "credential_delivery_refs": [],
        "receipt_context": receipt_context,
        "requested_at": "2026-01-01T00:00:00Z",
    });
    vec![
        ("valid thread target", valid.clone()),
        ("valid provider target", {
            let mut v = valid.clone();
            v["target"] =
                json!({ "provider_locator": { "provider": "github", "locator": "owner/repo" } });
            v
        }),
        ("missing fetch_id", drop_field(valid.clone(), "fetch_id")),
        ("missing schema", drop_field(valid.clone(), "schema")),
        (
            "empty fetch_id",
            set_field(valid.clone(), "fetch_id", json!("")),
        ),
        (
            "target empty (matches neither variant)",
            set_field(valid.clone(), "target", json!({})),
        ),
        (
            "empty readback_cursor",
            set_field(valid.clone(), "readback_cursor", json!("")),
        ),
        (
            "malformed requested_at",
            set_field(valid.clone(), "requested_at", json!("nope")),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn thread_outbox_observation_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.thread_outbox_provider.observation.v1",
        "protocol_version": "runx.thread_outbox_provider.v1",
        "observation_id": "obs_1",
        "adapter_id": "ad_1",
        "provider": "github",
        "operation": "push",
        "request_id": "push_1",
        "status": "accepted",
        "idempotency": { "key": "k1", "status": "created" },
        "observed_at": "2026-01-01T00:00:00Z",
    });
    vec![
        ("minimal valid", valid.clone()),
        (
            "missing observation_id",
            drop_field(valid.clone(), "observation_id"),
        ),
        ("missing schema", drop_field(valid.clone(), "schema")),
        (
            "empty observation_id",
            set_field(valid.clone(), "observation_id", json!("")),
        ),
        (
            "unknown operation",
            set_field(valid.clone(), "operation", json!("fly")),
        ),
        (
            "unknown status",
            set_field(valid.clone(), "status", json!("nope")),
        ),
        (
            "unknown idempotency status",
            set_field(
                valid.clone(),
                "idempotency",
                json!({ "key": "k1", "status": "nope" }),
            ),
        ),
        (
            "malformed observed_at",
            set_field(valid.clone(), "observed_at", json!("nope")),
        ),
        (
            "additional property",
            set_field(valid.clone(), "bogus", json!(true)),
        ),
    ]
}

fn an_intent() -> Value {
    json!({
        "purpose": "ship the change",
        "legitimacy": "operator approved",
        "success_criteria": [],
        "constraints": [],
        "derived_from": [],
    })
}

fn decision_corpus() -> Vec<(&'static str, Value)> {
    let inputs = json!({
        "signal_refs": [],
        "target_ref": null,
        "opportunity_refs": [],
        "selection_ref": null,
    });
    let valid = json!({
        "schema": "runx.decision.v1",
        "decision_id": "dec_1",
        "choice": "open",
        "inputs": inputs.clone(),
        "proposed_intent": an_intent(),
        "selected_act_id": null,
        "selected_harness_ref": null,
        "justification": { "summary": "because", "evidence_refs": [] },
        "closure": null,
        "artifact_refs": [],
    });
    vec![
        ("valid with all nullables null", valid.clone()),
        ("valid with nullables populated", {
            let mut v = valid.clone();
            v["selected_act_id"] = json!("act_1");
            v["selected_harness_ref"] = a_ref();
            v["closure"] = json!({
                "disposition": "closed",
                "reason_code": "done",
                "summary": "completed",
                "closed_at": "2026-01-01T00:00:00Z",
            });
            v["inputs"] = json!({
                "signal_refs": [a_ref()],
                "target_ref": a_ref(),
                "opportunity_refs": [],
                "selection_ref": a_ref(),
            });
            v
        }),
        ("missing required-but-nullable selected_act_id", {
            let mut v = valid.clone();
            v.as_object_mut().unwrap().remove("selected_act_id");
            v
        }),
        ("missing required-but-nullable closure", {
            let mut v = valid.clone();
            v.as_object_mut().unwrap().remove("closure");
            v
        }),
        ("missing required inputs.target_ref", {
            let mut v = valid.clone();
            v["inputs"] = json!({
                "signal_refs": [],
                "opportunity_refs": [],
                "selection_ref": null,
            });
            v
        }),
        ("empty decision_id rejected", {
            let mut v = valid.clone();
            v["decision_id"] = json!("");
            v
        }),
        ("empty selected_act_id rejected by minLength", {
            let mut v = valid.clone();
            v["selected_act_id"] = json!("");
            v
        }),
        ("unknown choice variant", {
            let mut v = valid.clone();
            v["choice"] = json!("ponder");
            v
        }),
        ("malformed closure closed_at", {
            let mut v = valid.clone();
            v["closure"] = json!({
                "disposition": "closed",
                "reason_code": "done",
                "summary": "completed",
                "closed_at": "nope",
            });
            v
        }),
        ("additional property", {
            let mut v = valid.clone();
            v["bogus"] = json!(true);
            v
        }),
    ]
}

fn verification_corpus() -> Vec<(&'static str, Value)> {
    let check = json!({
        "check_id": "c1",
        "criterion_ids": ["crit_1"],
        "status": "passed",
        "summary": "looks good",
        "checked_refs": [a_ref()],
        "evidence_refs": [a_ref()],
        "verified_at": "2026-01-01T00:00:00Z",
    });
    let valid = json!({
        "schema": "runx.verification.v1",
        "verification_id": "ver_1",
        "status": "passed",
        "checks": [check],
        "verified_at": "2026-01-01T00:00:00Z",
        "evidence_refs": [a_ref()],
    });
    vec![
        ("full valid", valid.clone()),
        (
            "minimal valid",
            json!({ "status": "pending", "checks": [], "evidence_refs": [] }),
        ),
        (
            "valid without optional schema marker and id",
            json!({
                "status": "failed",
                "checks": [{
                    "check_id": "c1",
                    "criterion_ids": ["crit_1"],
                    "status": "failed",
                    "evidence_refs": [],
                }],
                "evidence_refs": [],
            }),
        ),
        ("missing status", {
            let mut v = valid.clone();
            v.as_object_mut().unwrap().remove("status");
            v
        }),
        ("missing checks", {
            let mut v = valid.clone();
            v.as_object_mut().unwrap().remove("checks");
            v
        }),
        ("missing evidence_refs", {
            let mut v = valid.clone();
            v.as_object_mut().unwrap().remove("evidence_refs");
            v
        }),
        ("unknown status variant", {
            let mut v = valid.clone();
            v["status"] = json!("maybe");
            v
        }),
        ("empty verification_id", {
            let mut v = valid.clone();
            v["verification_id"] = json!("");
            v
        }),
        ("malformed verified_at", {
            let mut v = valid.clone();
            v["verified_at"] = json!("not-a-timestamp");
            v
        }),
        ("check missing required field", {
            let mut v = valid.clone();
            v["checks"] = json!([{ "criterion_ids": ["crit_1"], "status": "passed" }]);
            v
        }),
        ("additional property", {
            let mut v = valid.clone();
            v["bogus"] = json!(true);
            v
        }),
    ]
}

fn signal_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.signal.v1",
        "signal_id": "sig_1",
        "source_ref": a_ref(),
        "signal_type": "issue_opened",
        "title": "an issue opened",
        "observed_at": "2026-01-01T00:00:00Z",
    });
    let full = json!({
        "schema": "runx.signal.v1",
        "signal_id": "sig_1",
        "source_ref": a_ref(),
        "authenticity": {
            "host_ref": a_ref(),
            "principal_ref": a_ref(),
            "verified_by_ref": a_ref(),
            "trust_level": "verified_signature",
            "verified_at": "2026-01-01T00:00:00Z",
            "signature_refs": [a_ref()],
            "evidence_refs": [a_ref()],
        },
        "signal_type": "alert",
        "title": "an alert",
        "body_preview": "some body",
        "observed_at": "2026-01-01T00:00:00Z",
        "evidence_refs": [a_ref()],
        "fingerprint": {
            "algorithm": "sha256",
            "canonicalization": "json-c14n",
            "value": "abc",
            "derived_from": [a_ref()],
        },
        "extensions": { "k": 1 },
    });
    vec![
        ("minimal valid", valid.clone()),
        ("full valid", full),
        ("missing schema", {
            let mut v = valid.clone();
            v.as_object_mut().unwrap().remove("schema");
            v
        }),
        ("missing signal_id", {
            let mut v = valid.clone();
            v.as_object_mut().unwrap().remove("signal_id");
            v
        }),
        ("missing signal_type", {
            let mut v = valid.clone();
            v.as_object_mut().unwrap().remove("signal_type");
            v
        }),
        ("missing title", {
            let mut v = valid.clone();
            v.as_object_mut().unwrap().remove("title");
            v
        }),
        ("empty signal_id", {
            let mut v = valid.clone();
            v["signal_id"] = json!("");
            v
        }),
        ("empty title", {
            let mut v = valid.clone();
            v["title"] = json!("");
            v
        }),
        ("unknown signal_type variant", {
            let mut v = valid.clone();
            v["signal_type"] = json!("not_a_type");
            v
        }),
        ("malformed observed_at", {
            let mut v = valid.clone();
            v["observed_at"] = json!("not-a-timestamp");
            v
        }),
        ("additional property", {
            let mut v = valid.clone();
            v["bogus"] = json!(true);
            v
        }),
    ]
}

fn external_adapter_response_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.external_adapter.response.v1",
        "protocol_version": "runx.external_adapter.v1",
        "invocation_id": "inv_1",
        "adapter_id": "ad_1",
        "status": "completed",
        "observed_at": "2026-01-01T00:00:00Z",
    });
    let full = json!({
        "schema": "runx.external_adapter.response.v1",
        "protocol_version": "runx.external_adapter.v1",
        "invocation_id": "inv_1",
        "adapter_id": "ad_1",
        "status": "completed",
        "stdout": "out",
        "exit_code": 0,
        "telemetry": [
            { "name": "latency", "value": 12.5 },
            { "name": "label", "value": "ok" },
            { "name": "flag", "value": true },
        ],
        "errors": [{ "code": "e1", "message": "m", "retryable": false }],
        "observed_at": "2026-01-01T00:00:00Z",
    });
    vec![
        ("minimal valid", valid.clone()),
        ("full valid", full),
        ("missing status", {
            let mut v = valid.clone();
            v.as_object_mut().unwrap().remove("status");
            v
        }),
        ("missing invocation_id", {
            let mut v = valid.clone();
            v.as_object_mut().unwrap().remove("invocation_id");
            v
        }),
        ("unknown status variant", {
            let mut v = valid.clone();
            v["status"] = json!("frozen");
            v
        }),
        ("telemetry value as object rejected by untagged union", {
            let mut v = valid.clone();
            v["telemetry"] = json!([{ "name": "x", "value": { "nested": 1 } }]);
            v
        }),
        ("telemetry value as null rejected by untagged union", {
            let mut v = valid.clone();
            v["telemetry"] = json!([{ "name": "x", "value": null }]);
            v
        }),
        ("telemetry value string accepted", {
            let mut v = valid.clone();
            v["telemetry"] = json!([{ "name": "x", "value": "text" }]);
            v
        }),
        ("telemetry missing required value", {
            let mut v = valid.clone();
            v["telemetry"] = json!([{ "name": "x" }]);
            v
        }),
        ("additional property", {
            let mut v = valid.clone();
            v["bogus"] = json!(true);
            v
        }),
    ]
}

fn a_ref() -> Value {
    json!({ "type": "act", "uri": "runx:act:1" })
}

fn hash_commitment() -> Value {
    json!({ "algorithm": "sha256", "value": "abc", "canonicalization": "json-c14n" })
}

fn doctor_corpus() -> Vec<(&'static str, Value)> {
    let summary = json!({ "errors": 0, "warnings": 0, "infos": 0 });
    vec![
        (
            "minimal valid",
            json!({
                "schema": "runx.doctor.v1",
                "status": "success",
                "summary": summary,
                "diagnostics": [],
            }),
        ),
        (
            "full valid",
            json!({
                "schema": "runx.doctor.v1",
                "status": "failure",
                "summary": summary,
                "diagnostics": [{
                    "id": "d1",
                    "instance_id": "i1",
                    "severity": "warning",
                    "title": "t",
                    "message": "m",
                    "target": {},
                    "location": { "path": "p", "json_pointer": "/a" },
                    "evidence": { "e": 1 },
                    "repairs": [{
                        "id": "r1",
                        "kind": "edit_json",
                        "confidence": "high",
                        "risk": "low",
                        "path": "p",
                        "requires_human_review": false,
                    }],
                }],
            }),
        ),
        (
            "missing status",
            json!({ "schema": "runx.doctor.v1", "summary": summary, "diagnostics": [] }),
        ),
        (
            "missing summary",
            json!({ "schema": "runx.doctor.v1", "status": "success", "diagnostics": [] }),
        ),
        (
            "missing schema",
            json!({ "status": "success", "summary": summary, "diagnostics": [] }),
        ),
        (
            "unknown status variant",
            json!({
                "schema": "runx.doctor.v1",
                "status": "maybe",
                "summary": summary,
                "diagnostics": [],
            }),
        ),
        (
            "additional property",
            json!({
                "schema": "runx.doctor.v1",
                "status": "success",
                "summary": summary,
                "diagnostics": [],
                "bogus": true,
            }),
        ),
        (
            "diagnostic missing required field",
            json!({
                "schema": "runx.doctor.v1",
                "status": "failure",
                "summary": summary,
                "diagnostics": [{
                    "id": "d1",
                    "severity": "error",
                    "title": "t",
                    "message": "m",
                    "target": {},
                    "location": { "path": "p" },
                    "repairs": [],
                }],
            }),
        ),
        ("not an object", json!("nope")),
    ]
}

fn redaction_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.redaction.v1",
        "redaction_id": "red_1",
        "policy_ref": a_ref(),
        "redacted_fields": ["a", "b"],
        "hash_commitments": [hash_commitment()],
        "canonicalization": "json-c14n",
        "performed_by_ref": a_ref(),
        "performed_at": "2026-01-01T00:00:00Z",
    });
    vec![
        ("full valid", valid.clone()),
        (
            "minimal valid",
            json!({
                "schema": "runx.redaction.v1",
                "redaction_id": "red_1",
                "policy_ref": a_ref(),
                "redacted_fields": [],
                "hash_commitments": [],
                "canonicalization": "json-c14n",
                "performed_by_ref": a_ref(),
                "performed_at": "2026-01-01T00:00:00Z",
            }),
        ),
        ("missing schema", {
            let mut v = valid.clone();
            v.as_object_mut().unwrap().remove("schema");
            v
        }),
        ("missing redaction_id", {
            let mut v = valid.clone();
            v.as_object_mut().unwrap().remove("redaction_id");
            v
        }),
        ("empty redaction_id", {
            let mut v = valid.clone();
            v["redaction_id"] = json!("");
            v
        }),
        ("empty canonicalization", {
            let mut v = valid.clone();
            v["canonicalization"] = json!("");
            v
        }),
        ("empty redacted_fields item", {
            let mut v = valid.clone();
            v["redacted_fields"] = json!([""]);
            v
        }),
        ("malformed performed_at", {
            let mut v = valid.clone();
            v["performed_at"] = json!("not-a-timestamp");
            v
        }),
        ("additional property", {
            let mut v = valid.clone();
            v["bogus"] = json!(true);
            v
        }),
    ]
}

fn artifact_corpus() -> Vec<(&'static str, Value)> {
    let valid = json!({
        "schema": "runx.artifact.v1",
        "artifact_id": "art_1",
        "artifact_ref": a_ref(),
        "produced_by": { "receipt_ref": a_ref() },
        "media_type": "text/plain",
        "created_at": "2026-01-01T00:00:00Z",
        "size_bytes": 12,
        "hash": hash_commitment(),
        "redaction_refs": [],
        "source_refs": [],
    });
    vec![
        ("minimal valid", valid.clone()),
        ("full valid", {
            let mut v = valid.clone();
            v["produced_by"] = json!({
                "receipt_ref": a_ref(),
                "act_ref": { "receipt_ref": a_ref(), "act_id": "act_1" },
            });
            v["data_ref"] = a_ref();
            v["summary"] = json!("a summary");
            v["extensions"] = json!({ "k": 1 });
            v
        }),
        ("missing schema", {
            let mut v = valid.clone();
            v.as_object_mut().unwrap().remove("schema");
            v
        }),
        ("missing artifact_id", {
            let mut v = valid.clone();
            v.as_object_mut().unwrap().remove("artifact_id");
            v
        }),
        ("missing hash", {
            let mut v = valid.clone();
            v.as_object_mut().unwrap().remove("hash");
            v
        }),
        ("empty artifact_id", {
            let mut v = valid.clone();
            v["artifact_id"] = json!("");
            v
        }),
        ("empty media_type", {
            let mut v = valid.clone();
            v["media_type"] = json!("");
            v
        }),
        ("malformed created_at", {
            let mut v = valid.clone();
            v["created_at"] = json!("nope");
            v
        }),
        ("empty hash value", {
            let mut v = valid.clone();
            v["hash"] = json!({ "algorithm": "sha256", "value": "", "canonicalization": "c" });
            v
        }),
        ("additional property", {
            let mut v = valid.clone();
            v["bogus"] = json!(true);
            v
        }),
    ]
}

fn reference_corpus() -> Vec<(&'static str, Value)> {
    vec![
        (
            "minimal valid",
            json!({ "type": "github_issue", "uri": "runx:github_issue:1" }),
        ),
        (
            "full valid",
            json!({
                "type": "act",
                "uri": "runx:act:1",
                "provider": "github",
                "locator": "owner/repo#1",
                "label": "an act",
                "observed_at": "2026-01-01T00:00:00.000Z",
                "proof_kind": "payment_rail",
            }),
        ),
        (
            "optional schema marker",
            json!({ "schema": "runx.reference.v1", "type": "act", "uri": "x" }),
        ),
        ("missing uri", json!({ "type": "act" })),
        ("missing type", json!({ "uri": "x" })),
        (
            "unknown type variant",
            json!({ "type": "not_a_type", "uri": "x" }),
        ),
        ("empty uri", json!({ "type": "act", "uri": "" })),
        (
            "malformed observed_at",
            json!({ "type": "act", "uri": "x", "observed_at": "not-a-timestamp" }),
        ),
        (
            "additional property",
            json!({ "type": "act", "uri": "x", "bogus": true }),
        ),
        (
            "bad proof_kind",
            json!({ "type": "act", "uri": "x", "proof_kind": "wire" }),
        ),
    ]
}

fn committed_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas")
}

#[test]
fn emitted_schemas_are_wire_compatible_with_committed() {
    let dir = committed_dir();
    let mut failures: Vec<String> = Vec::new();

    for contract in covered() {
        let name = contract.file_name;
        let raw = match std::fs::read_to_string(dir.join(name)) {
            Ok(raw) => raw,
            Err(error) => {
                failures.push(format!("{name}: cannot read committed schema: {error}"));
                continue;
            }
        };
        let committed: Value = match serde_json::from_str(&raw) {
            Ok(value) => value,
            Err(error) => {
                failures.push(format!(
                    "{name}: committed schema is not valid JSON: {error}"
                ));
                continue;
            }
        };

        if contract.emitted.get("$id") != committed.get("$id")
            || contract.emitted.get("x-runx-schema") != committed.get("x-runx-schema")
        {
            failures.push(format!(
                "{name}: schema identity ($id / x-runx-schema) diverged"
            ));
            continue;
        }

        let Ok(committed_validator) = jsonschema::validator_for(&committed) else {
            failures.push(format!(
                "{name}: committed schema is not a usable validator"
            ));
            continue;
        };
        let Ok(emitted_validator) = jsonschema::validator_for(&contract.emitted) else {
            failures.push(format!("{name}: emitted schema is not a usable validator"));
            continue;
        };

        for (label, value) in &contract.corpus {
            let committed_accepts = committed_validator.is_valid(value);
            let emitted_accepts = emitted_validator.is_valid(value);
            if committed_accepts != emitted_accepts {
                failures.push(format!(
                    "{name} / {label}: committed accepts={committed_accepts}, emitted accepts={emitted_accepts}"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "schema wire-compat drift:\n{}",
        failures.join("\n")
    );
}
