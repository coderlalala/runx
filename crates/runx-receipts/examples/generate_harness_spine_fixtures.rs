//! Regenerates the flat `runx.receipt.v1` harness-spine fixtures and the
//! canonical-json oracle. Run with:
//!   cargo run --manifest-path crates/Cargo.toml -p runx-receipts \
//!     --example generate_harness_spine_fixtures

// Fixture/oracle generator tool: failing loud on a construction error and
// printing progress is intended, so the workspace unwrap/print bans are lifted.
#![allow(clippy::unwrap_used, clippy::print_stdout)]

use std::fs;
use std::path::Path;

use runx_contracts::schema::NonEmptyString;
use runx_contracts::{
    ActForm, AuthorityAttenuation, ChangePlan, ChangeRequest, Closure, ClosureDisposition,
    CriterionBinding, CriterionStatus, Decision, DecisionChoice, DecisionInputs,
    DecisionJustification, HashAlgorithm, Intent, Lineage, RECEIPT_CANONICALIZATION, Receipt,
    ReceiptAct, ReceiptAuthority, ReceiptCommitment, ReceiptCommitmentScope, ReceiptEnforcement,
    ReceiptIdempotency, ReceiptInputContext, ReceiptIssuer, ReceiptIssuerType, ReceiptSchema,
    ReceiptSignature, Reference, ReferenceType, RevisionDetails, Seal, SignatureAlgorithm, Subject,
    SuccessCriterion, Verification, VerificationCheck, VerificationDetails, VerificationStatus,
    receipt_subject_kind,
};
use runx_receipts::{
    canonical_receipt_body_digest, canonical_receipt_digest, canonical_receipt_json,
    content_addressed_receipt_id,
};
use serde_json::{Value, json};

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let spine = root.join("fixtures/contracts/harness-spine");
    let canonical = root.join("fixtures/contracts/canonical-json");

    let success = sealed(success_receipt());
    let abnormal = sealed(abnormal_receipt());
    let post_merge = sealed(post_merge_receipt());

    write_fixture(
        &spine.join("receipt-success.json"),
        "receipt_success",
        "Sealed runx.receipt.v1 for a successful skill run.",
        "receipt",
        &success,
    );
    write_fixture(
        &spine.join("receipt-abnormal.json"),
        "receipt_abnormal",
        "Sealed runx.receipt.v1 for a failed skill run.",
        "receipt",
        &abnormal,
    );
    write_fixture(
        &spine.join("post-merge-observer-merged-verified.json"),
        "post_merge_observer_merged_verified",
        "Sealed post-merge observer runx.receipt.v1.",
        "receipt",
        &post_merge,
    );

    let oracle = json!({
        "schema": "runx.canonical_json_oracle.v1",
        "canonicalization": RECEIPT_CANONICALIZATION,
        "cases": [
            oracle_case("receipt-success", "harness-spine/receipt-success.json", &success),
            oracle_case("receipt-abnormal", "harness-spine/receipt-abnormal.json", &abnormal),
            oracle_case(
                "post-merge-observer-merged-verified",
                "harness-spine/post-merge-observer-merged-verified.json",
                &post_merge,
            ),
        ],
    });
    fs::write(
        canonical.join("runx-receipt-c14n-v1.oracles.json"),
        format!("{}\n", serde_json::to_string_pretty(&oracle).unwrap()),
    )
    .unwrap();

    // Remove the retired old-shape oracle.

    println!("regenerated harness-spine fixtures + receipt c14n oracle");
}

fn write_fixture(path: &Path, name: &str, description: &str, kind: &str, receipt: &Receipt) {
    let wrapper = json!({
        "fixture_kind": kind,
        "name": name,
        "description": description,
        "scope": "harness-spine",
        "expected": serde_json::to_value(receipt).unwrap(),
    });
    fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(&wrapper).unwrap()),
    )
    .unwrap();
}

fn oracle_case(name: &str, fixture: &str, receipt: &Receipt) -> Value {
    json!({
        "name": name,
        "fixture": fixture,
        "full_canonical_json": canonical_receipt_json(receipt).unwrap(),
        "full_sha256": canonical_receipt_digest(receipt).unwrap(),
        "body_canonical_json": runx_receipts::canonical_receipt_body_json(receipt).unwrap(),
        "body_sha256": canonical_receipt_body_digest(receipt).unwrap(),
    })
}

fn sealed(mut receipt: Receipt) -> Receipt {
    receipt.id = content_addressed_receipt_id(&receipt).unwrap().into();
    let digest = canonical_receipt_body_digest(&receipt).unwrap();
    receipt.digest = digest.clone().into();
    receipt.signature.value = format!("sig:{digest}").into();
    receipt
}

fn base(id: &str, kind: NonEmptyString, subject_id: &str) -> Receipt {
    Receipt {
        schema: ReceiptSchema::V1,
        id: id.into(),
        created_at: "2026-05-22T00:00:00Z".into(),
        canonicalization: RECEIPT_CANONICALIZATION.into(),
        issuer: ReceiptIssuer {
            issuer_type: ReceiptIssuerType::Local,
            kid: "fixture-key".into(),
            public_key_sha256: format!("sha256:{}", "0".repeat(64)).into(),
        },
        signature: ReceiptSignature {
            alg: SignatureAlgorithm::Ed25519,
            value: "sig:pending".into(),
        },
        digest: "sha256:pending".into(),
        idempotency: ReceiptIdempotency {
            intent_key: format!("sha256:{}", "1".repeat(64)).into(),
            trigger_fingerprint: format!("sha256:{}", "2".repeat(64)).into(),
            content_hash: format!("sha256:{}", "3".repeat(64)).into(),
        },
        subject: Subject {
            kind,
            reference: Reference::runx(ReferenceType::Harness, subject_id),
            input_context: Some(ReceiptInputContext {
                source: format!("runx:signal:{subject_id}").into(),
                preview: format!("Run {subject_id}"),
                value_hash: format!("sha256:{}", "6".repeat(64)).into(),
            }),
            commitments: vec![ReceiptCommitment {
                scope: ReceiptCommitmentScope::Output,
                algorithm: HashAlgorithm::Sha256,
                value: format!("sha256:{}", "4".repeat(64)).into(),
                canonicalization: "runx.stable-json.v1".into(),
            }],
        },
        authority: ReceiptAuthority {
            actor_ref: Reference::runx(ReferenceType::Principal, "local_runtime"),
            authority_proof_refs: Vec::new(),
            grant_refs: Vec::new(),
            scope_refs: Vec::new(),
            terms: Vec::new(),
            attenuation: AuthorityAttenuation {
                parent_authority_ref: None,
                subset_proof: None,
            },
            mandate_ref: None,
            enforcement: ReceiptEnforcement {
                profile_hash: format!("sha256:{}", "5".repeat(64)).into(),
                redaction_refs: Vec::new(),
                setup_refs: Vec::new(),
                teardown_refs: Vec::new(),
            },
        },
        signals: Vec::new(),
        decisions: Vec::new(),
        acts: Vec::new(),
        seal: Seal {
            disposition: ClosureDisposition::Closed,
            reason_code: "process_closed".into(),
            summary: "closed".into(),
            closed_at: "2026-05-22T00:00:00Z".into(),
            last_observed_at: "2026-05-22T00:00:00Z".into(),
            criteria: Vec::new(),
        },
        lineage: Some(Lineage::default()),
        metadata: None,
    }
}

const CREATED_AT: &str = "2026-05-22T00:00:00Z";

fn observation_intent(criterion_id: &str, statement: &str) -> Intent {
    Intent {
        purpose: "Execute the requested skill step".into(),
        legitimacy: "Local harness admitted this run".into(),
        success_criteria: vec![SuccessCriterion {
            criterion_id: criterion_id.into(),
            statement: statement.into(),
            required: true,
        }],
        constraints: Vec::new(),
        derived_from: Vec::new(),
    }
}

fn observation_act(
    id: &str,
    summary: &str,
    status: CriterionStatus,
    disposition: ClosureDisposition,
    binding_summary: &str,
) -> ReceiptAct {
    ReceiptAct {
        id: id.into(),
        form: ActForm::Observation,
        intent: observation_intent("process_exit", "cli-tool exits successfully"),
        summary: summary.into(),
        criterion_bindings: vec![CriterionBinding {
            criterion_id: "process_exit".into(),
            status,
            evidence_refs: Vec::new(),
            verification_refs: Vec::new(),
            summary: Some(binding_summary.into()),
        }],
        by: None,
        source_refs: Vec::new(),
        target_refs: Vec::new(),
        artifact_refs: Vec::new(),
        context_ref: Some(Reference::runx(
            ReferenceType::Act,
            &format!("{id}_context"),
        )),
        closure: Closure {
            disposition,
            reason_code: "process_exit".into(),
            summary: binding_summary.into(),
            closed_at: CREATED_AT.into(),
        },
        revision: None,
        verification: None,
    }
}

fn open_decision(act_id: &str) -> Decision {
    Decision {
        decision_id: format!("dec_{act_id}").into(),
        choice: DecisionChoice::Open,
        inputs: DecisionInputs::default(),
        proposed_intent: Intent {
            purpose: format!("Open node for {act_id}").into(),
            legitimacy: "Local graph execution requested this node".into(),
            success_criteria: Vec::new(),
            constraints: Vec::new(),
            derived_from: Vec::new(),
        },
        selected_act_id: Some(act_id.into()),
        selected_harness_ref: None,
        justification: DecisionJustification {
            summary: "runtime graph planner selected this node".into(),
            evidence_refs: Vec::new(),
        },
        closure: None,
        artifact_refs: Vec::new(),
    }
}

fn success_receipt() -> Receipt {
    let mut receipt = base(
        "hrn_rcpt_echo_success",
        receipt_subject_kind::SKILL.into(),
        "echo_success",
    );
    receipt.acts = vec![observation_act(
        "act_echo",
        "Executed graph step echo",
        CriterionStatus::Verified,
        ClosureDisposition::Closed,
        "cli-tool exited successfully",
    )];
    receipt.decisions = vec![open_decision("act_echo")];
    receipt.seal.summary = "cli-tool exited successfully".into();
    receipt.seal.criteria = vec![CriterionBinding {
        criterion_id: "process_exit".into(),
        status: CriterionStatus::Verified,
        evidence_refs: Vec::new(),
        verification_refs: Vec::new(),
        summary: Some("cli-tool exited successfully".into()),
    }];
    receipt.signals = vec![Reference::runx(ReferenceType::Signal, "echo_success")];
    receipt
}

fn abnormal_receipt() -> Receipt {
    let mut receipt = base(
        "hrn_rcpt_echo_abnormal",
        receipt_subject_kind::SKILL.into(),
        "echo_abnormal",
    );
    receipt.acts = vec![observation_act(
        "act_echo",
        "Executed graph step echo",
        CriterionStatus::Failed,
        ClosureDisposition::Failed,
        "cli-tool failed",
    )];
    receipt.decisions = vec![open_decision("act_echo")];
    receipt.seal.disposition = ClosureDisposition::Failed;
    receipt.seal.reason_code = "process_failed".into();
    receipt.seal.summary = "cli-tool failed".into();
    receipt.seal.criteria = vec![CriterionBinding {
        criterion_id: "process_exit".into(),
        status: CriterionStatus::Failed,
        evidence_refs: Vec::new(),
        verification_refs: Vec::new(),
        summary: Some("cli-tool failed".into()),
    }];
    receipt
}

fn post_merge_receipt() -> Receipt {
    let mut receipt = base(
        "hrn_rcpt_post_merge_nitrosend_77_188",
        receipt_subject_kind::SKILL.into(),
        "post_merge_observer",
    );
    receipt.idempotency.intent_key =
        "post-merge:github://runxhq/nitrosend/issues/77:github://runxhq/nitrosend/pulls/188".into();
    let issue_ref = Reference {
        provider: Some("github".to_owned().into()),
        locator: Some("runxhq/nitrosend#77".to_owned().into()),
        ..Reference::with_uri(
            ReferenceType::GithubIssue,
            "github://runxhq/nitrosend/issues/77",
        )
    };
    let pr_ref = Reference {
        provider: Some("github".to_owned().into()),
        locator: Some("runxhq/nitrosend!188".to_owned().into()),
        ..Reference::with_uri(
            ReferenceType::GithubPullRequest,
            "github://runxhq/nitrosend/pulls/188",
        )
    };
    let slack_ref = Reference {
        provider: Some("slack".to_owned().into()),
        locator: Some("team/channel/1700000000.0001".to_owned().into()),
        ..Reference::with_uri(
            ReferenceType::SlackThread,
            "slack://team/channel/1700000000.0001",
        )
    };
    let verification_ref = Reference::runx(ReferenceType::Verification, "ver_post_merge_verified");
    let post_merge_criteria = [
        "post_merge.provider_state",
        "post_merge.human_gate",
        "post_merge.verification_passed",
        "post_merge.source_thread_target_present",
        "post_merge.close_policy_authorized",
    ];
    let act_artifacts = vec![issue_ref.clone(), pr_ref.clone(), slack_ref.clone()];
    // The observation act declares and binds the post-merge criteria the seal
    // rolls up; the other forms carry their form-specific bodies inline.
    let observe_intent = Intent {
        purpose: "Observe the post-merge state of the target pull request".into(),
        legitimacy: "Post-merge observer is authorized to inspect provider state".into(),
        success_criteria: post_merge_criteria
            .iter()
            .map(|id| SuccessCriterion {
                criterion_id: (*id).into(),
                statement: format!("{id} holds").into(),
                required: true,
            })
            .collect(),
        constraints: Vec::new(),
        derived_from: vec![pr_ref.clone(), slack_ref.clone()],
    };
    let observe_bindings = post_merge_criteria
        .iter()
        .map(|id| CriterionBinding {
            criterion_id: (*id).into(),
            status: CriterionStatus::Verified,
            evidence_refs: vec![pr_ref.clone()],
            verification_refs: vec![verification_ref.clone()],
            summary: Some(format!("{id} verified").into()),
        })
        .collect::<Vec<_>>();
    let verification = Verification {
        schema: None,
        verification_id: Some("ver_post_merge_verified".into()),
        status: VerificationStatus::Passed,
        checks: vec![VerificationCheck {
            check_id: "post_merge.verification_passed".into(),
            criterion_ids: vec!["post_merge.verification_passed".into()],
            status: VerificationStatus::Passed,
            summary: Some("Nitrosend dogfood verification passed.".into()),
            checked_refs: vec![pr_ref.clone()],
            evidence_refs: vec![verification_ref.clone()],
            verified_at: Some(CREATED_AT.into()),
        }],
        verified_at: Some(CREATED_AT.into()),
        evidence_refs: vec![verification_ref.clone()],
    };
    let revision = RevisionDetails {
        change_request: ChangeRequest {
            request_id: "act_revise_request".into(),
            summary: "Ship the target pull request".into(),
            target_surfaces: Vec::new(),
            success_criteria: Vec::new(),
        },
        change_plan: ChangePlan {
            plan_id: "act_revise_plan".into(),
            summary: "Open and merge the target pull request".into(),
            steps: vec!["Open PR".into(), "Merge PR".into()],
            risks: Vec::new(),
        },
        target_surfaces: Vec::new(),
        invariants: Vec::new(),
        verification: None,
        handoff_refs: Vec::new(),
        revision_refs: Vec::new(),
    };
    let post_merge_act = |id: &str, form: ActForm| ReceiptAct {
        id: id.into(),
        form: form.clone(),
        intent: match form {
            ActForm::Observation => observe_intent.clone(),
            _ => Intent {
                purpose: format!("post-merge {id}").into(),
                legitimacy: "Post-merge observer is authorized for this act".into(),
                success_criteria: Vec::new(),
                constraints: Vec::new(),
                derived_from: Vec::new(),
            },
        },
        summary: format!("post-merge {id}").into(),
        criterion_bindings: match form {
            ActForm::Observation => observe_bindings.clone(),
            _ => Vec::new(),
        },
        by: None,
        source_refs: vec![slack_ref.clone(), issue_ref.clone()],
        target_refs: vec![pr_ref.clone()],
        artifact_refs: act_artifacts.clone(),
        context_ref: Some(Reference::runx(
            ReferenceType::Act,
            &format!("{id}_context"),
        )),
        closure: Closure {
            disposition: ClosureDisposition::Closed,
            reason_code: "merged_verified".into(),
            summary: format!("post-merge {id} closed").into(),
            closed_at: CREATED_AT.into(),
        },
        revision: if matches!(form, ActForm::Revision) {
            Some(revision.clone())
        } else {
            None
        },
        verification: if matches!(form, ActForm::Verification) {
            Some(VerificationDetails {
                criterion_ids: vec!["post_merge.verification_passed".into()],
                verification: verification.clone(),
                deployment_ref: None,
            })
        } else {
            None
        },
    };
    receipt.acts = vec![
        post_merge_act("act_observe", ActForm::Observation),
        post_merge_act("act_verify", ActForm::Verification),
        post_merge_act("act_reply", ActForm::Reply),
        post_merge_act("act_revise", ActForm::Revision),
    ];
    receipt.decisions = vec![open_decision("act_observe")];
    receipt.seal.reason_code = "merged_verified".into();
    receipt.seal.summary = "Target PR shipped and verified.".into();
    receipt.seal.criteria = vec![
        criterion(
            "post_merge.provider_state",
            Vec::new(),
            vec![pr_ref.clone()],
            None,
        ),
        criterion("post_merge.human_gate", Vec::new(), Vec::new(), None),
        criterion(
            "post_merge.verification_passed",
            vec![verification_ref.clone()],
            vec![pr_ref.clone()],
            Some("Nitrosend dogfood verification passed."),
        ),
        criterion(
            "post_merge.source_thread_target_present",
            vec![verification_ref],
            vec![slack_ref, issue_ref],
            None,
        ),
        criterion(
            "post_merge.close_policy_authorized",
            Vec::new(),
            Vec::new(),
            None,
        ),
    ];
    use runx_contracts::{JsonObject, JsonValue};
    let mut pr = JsonObject::new();
    pr.insert(
        "merge_sha".to_owned(),
        JsonValue::String("9f14c0ffee1234567890abcdef1234567890abcd".to_owned()),
    );
    let mut observer = JsonObject::new();
    observer.insert("pr".to_owned(), JsonValue::Object(pr));
    let mut metadata = JsonObject::new();
    metadata.insert("observer_contract".to_owned(), JsonValue::Object(observer));
    receipt.metadata = Some(metadata);
    receipt
}

fn criterion(
    id: &str,
    verification_refs: Vec<Reference>,
    evidence_refs: Vec<Reference>,
    summary: Option<&str>,
) -> CriterionBinding {
    CriterionBinding {
        criterion_id: id.into(),
        status: CriterionStatus::Verified,
        evidence_refs,
        verification_refs,
        summary: summary.map(Into::into),
    }
}
