use runx_contracts::post_merge_observer::{
    PostMergeObserverCommandRequest, normalize_post_merge_observer_command,
};
use runx_contracts::{
    ActForm, ClosureDisposition, CriterionStatus, HarnessReceipt, HarnessState, OperationalPolicy,
    OperationalPolicyAction, OperationalPolicySourceProvider, PostMergeObserverClosureState,
    PostMergeObserverCriterionPlan, PostMergeObserverPlan, PostMergeObserverPlanError,
    PostMergeObserverPlanRequest, PostMergeObserverRuntimeDecision, PostMergeObserverSignalSource,
    PostMergeProvider, PostMergePullRequestObservation, PostMergePullRequestState,
    PostMergeSourceIssueDisposition, PostMergeVerificationObservation, PostMergeVerificationStatus,
    Reference, ReferenceType, TargetRepoRunnerExistingPullRequest, TargetRepoRunnerPlanRequest,
    TargetRepoRunnerSourceContext, plan_post_merge_observer_closure,
    plan_post_merge_observer_runtime_dedupe, plan_target_repo_runner,
    plan_target_repo_runner_source_publication_receipt,
    project_post_merge_observer_publication_from_receipt,
};

const NITROSEND_LIKE: &str =
    include_str!("../../../fixtures/operational-policy/nitrosend-like.json");
const POST_MERGE_OBSERVER_FIXTURE: &str = include_str!(
    "../../../fixtures/contracts/harness-spine/post-merge-observer-merged-verified.json"
);

#[test]
fn closed_unmerged_plans_distinct_observation_without_shipped_claim()
-> Result<(), Box<dyn std::error::Error>> {
    let policy = nitrosend_policy()?;
    let plan = plan_post_merge_observer_closure(
        &policy,
        &observer_request(false, None, PostMergeVerificationStatus::NotRequired, true),
    )?;

    assert_eq!(
        plan.final_state,
        PostMergeObserverClosureState::ClosedUnmerged
    );
    assert_eq!(plan.reason_code, "closed_unmerged");
    assert_eq!(plan.seal_disposition, ClosureDisposition::Closed);
    assert_eq!(
        plan.source_issue.disposition,
        PostMergeSourceIssueDisposition::KeepOpen
    );
    assert_eq!(plan.act_forms, vec![ActForm::Observation, ActForm::Reply]);
    assert_eq!(
        criterion_ids(&plan),
        vec![
            "post_merge.provider_state",
            "post_merge.human_gate",
            "post_merge.source_thread_target_present",
        ]
    );
    assert!(!plan.summary.contains("shipped"));
    assert!(!plan.summary.contains("verified"));
    assert!(!plan.summary.contains("merged PR"));
    assert!(plan.closure_key.contains("@closed-unmerged:"));
    Ok(())
}

#[test]
fn failed_verification_keeps_source_issue_open_when_policy_closes_when_verified()
-> Result<(), Box<dyn std::error::Error>> {
    let policy = nitrosend_policy()?;
    let plan = plan_post_merge_observer_closure(
        &policy,
        &observer_request(
            true,
            Some("9f14c0ffee1234567890abcdef1234567890abcd"),
            PostMergeVerificationStatus::Failed,
            true,
        ),
    )?;

    assert_eq!(
        plan.final_state,
        PostMergeObserverClosureState::FailedVerification
    );
    assert_eq!(plan.reason_code, "failed_verification");
    assert_eq!(plan.seal_disposition, ClosureDisposition::Failed);
    assert_eq!(
        plan.source_issue.disposition,
        PostMergeSourceIssueDisposition::KeepOpen
    );
    assert_eq!(
        plan.act_forms,
        vec![ActForm::Observation, ActForm::Verification, ActForm::Reply]
    );
    assert_eq!(
        criterion(&plan, "post_merge.verification_failed")?.status,
        CriterionStatus::Failed
    );
    assert!(
        !plan
            .seal_criteria
            .iter()
            .any(|criterion| criterion.criterion_id == "post_merge.close_policy_authorized")
    );
    assert_eq!(
        plan.idempotency.intent_key,
        "post-merge:github://nitrosend/nitrosend/issues/482:github://nitrosend/api/pulls/144"
    );
    assert!(plan.idempotency.trigger_fingerprint.starts_with("sha256:"));
    assert!(plan.idempotency.content_hash.starts_with("sha256:"));

    let json = serde_json::to_string(&plan)?;
    assert!(!json.contains("/Users/"));
    assert!(!json.contains("/tmp/"));
    Ok(())
}

#[test]
fn merged_verified_authorizes_source_issue_close_and_revision()
-> Result<(), Box<dyn std::error::Error>> {
    let policy = nitrosend_policy()?;
    let plan = plan_post_merge_observer_closure(
        &policy,
        &observer_request(
            true,
            Some("9f14c0ffee1234567890abcdef1234567890abcd"),
            PostMergeVerificationStatus::Passed,
            true,
        ),
    )?;

    assert_eq!(
        plan.final_state,
        PostMergeObserverClosureState::MergedVerified
    );
    assert_eq!(plan.reason_code, "merged_verified");
    assert_eq!(
        plan.source_issue.disposition,
        PostMergeSourceIssueDisposition::Close
    );
    assert_eq!(
        plan.act_forms,
        vec![
            ActForm::Observation,
            ActForm::Verification,
            ActForm::Reply,
            ActForm::Revision,
        ]
    );
    assert_eq!(
        criterion(&plan, "post_merge.close_policy_authorized")?.status,
        CriterionStatus::Verified
    );
    assert!(
        plan.closure_key
            .ends_with("@merge:9f14c0ffee1234567890abcdef1234567890abcd")
    );
    Ok(())
}

#[test]
fn repeated_merged_verified_signal_keeps_same_idempotency_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let policy = nitrosend_policy()?;
    let first = plan_post_merge_observer_closure(
        &policy,
        &observer_request(
            true,
            Some("9f14c0ffee1234567890abcdef1234567890abcd"),
            PostMergeVerificationStatus::Passed,
            true,
        ),
    )?;
    let mut repeated_request = observer_request(
        true,
        Some("9f14c0ffee1234567890abcdef1234567890abcd"),
        PostMergeVerificationStatus::Passed,
        true,
    );
    repeated_request.pull_request.observed_at = "2026-05-20T05:21:00Z".to_owned();
    repeated_request.pull_request.actor = Some("github:user:webhook-redelivery".to_owned());
    let repeated = plan_post_merge_observer_closure(&policy, &repeated_request)?;

    assert_ne!(first.observed_at, repeated.observed_at);
    assert_eq!(first.closure_key, repeated.closure_key);
    assert_eq!(first.act_forms, repeated.act_forms);
    assert_eq!(first.idempotency, repeated.idempotency);
    assert_eq!(first.idempotency.closure_key, first.closure_key);
    assert_eq!(first.idempotency.act_forms, first.act_forms);
    Ok(())
}

#[test]
fn changed_verification_state_separates_idempotency_content()
-> Result<(), Box<dyn std::error::Error>> {
    let policy = nitrosend_policy()?;
    let verified = plan_post_merge_observer_closure(
        &policy,
        &observer_request(
            true,
            Some("9f14c0ffee1234567890abcdef1234567890abcd"),
            PostMergeVerificationStatus::Passed,
            true,
        ),
    )?;
    let failed = plan_post_merge_observer_closure(
        &policy,
        &observer_request(
            true,
            Some("9f14c0ffee1234567890abcdef1234567890abcd"),
            PostMergeVerificationStatus::Failed,
            true,
        ),
    )?;

    assert_eq!(
        verified.idempotency.intent_key,
        failed.idempotency.intent_key
    );
    assert_eq!(
        verified.idempotency.closure_key,
        failed.idempotency.closure_key
    );
    assert_ne!(
        verified.idempotency.content_hash,
        failed.idempotency.content_hash
    );
    assert_ne!(verified.idempotency.act_forms, failed.idempotency.act_forms);
    Ok(())
}

#[test]
fn webhook_and_scheduler_signals_share_runtime_dedupe_receipt_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let policy = nitrosend_policy()?;
    let plan = plan_post_merge_observer_closure(
        &policy,
        &observer_request(
            true,
            Some("9f14c0ffee1234567890abcdef1234567890abcd"),
            PostMergeVerificationStatus::Passed,
            true,
        ),
    )?;

    let webhook = plan_post_merge_observer_runtime_dedupe(
        &plan,
        PostMergeObserverSignalSource::Webhook,
        None,
    );
    let scheduler = plan_post_merge_observer_runtime_dedupe(
        &plan,
        PostMergeObserverSignalSource::Scheduler,
        None,
    );
    let repeated_scheduler = plan_post_merge_observer_runtime_dedupe(
        &plan,
        PostMergeObserverSignalSource::Scheduler,
        Some(webhook.receipt_ref.clone()),
    );

    assert_eq!(
        webhook.decision,
        PostMergeObserverRuntimeDecision::SealAndPublish
    );
    assert_eq!(
        scheduler.decision,
        PostMergeObserverRuntimeDecision::SealAndPublish
    );
    assert_eq!(
        repeated_scheduler.decision,
        PostMergeObserverRuntimeDecision::AlreadyPublished
    );
    assert_eq!(webhook.lock_key, scheduler.lock_key);
    assert_eq!(webhook.receipt_id, scheduler.receipt_id);
    assert_eq!(webhook.publication_key, scheduler.publication_key);
    assert_eq!(webhook.content_hash, plan.idempotency.content_hash);
    Ok(())
}

#[test]
fn webhook_and_scheduler_commands_normalize_to_same_observer_key()
-> Result<(), Box<dyn std::error::Error>> {
    let policy = nitrosend_policy()?;
    let webhook = normalize_post_merge_observer_command(
        &policy,
        &observer_command_request(
            PostMergeObserverSignalSource::Webhook,
            Some(webhook_delivery_ref()),
            true,
        ),
    )?;
    let scheduler = normalize_post_merge_observer_command(
        &policy,
        &observer_command_request(PostMergeObserverSignalSource::Scheduler, None, true),
    )?;

    assert_eq!(webhook.command_key, scheduler.command_key);
    assert_eq!(
        webhook.command_key,
        "post-merge-observer:github://nitrosend/nitrosend/issues/482:github://nitrosend/api/pulls/144"
    );
    assert_eq!(webhook.source_id, "bugs-fixes");
    assert_eq!(scheduler.source_id, "bugs-fixes");
    assert_eq!(
        webhook.signal_ref.as_ref().map(|reference| {
            (
                reference.reference_type.clone(),
                reference.provider.as_deref(),
                reference.locator.as_deref(),
            )
        }),
        Some((
            ReferenceType::WebhookDelivery,
            Some("github"),
            Some("nitrosend/api/delivery/evt_01HX")
        ))
    );
    assert_eq!(scheduler.signal_ref, None);
    assert_eq!(
        webhook
            .source_thread_ref
            .as_ref()
            .and_then(|reference| reference.provider.as_deref()),
        Some("slack")
    );
    assert_eq!(
        webhook.pull_request_ref.reference_type,
        ReferenceType::GithubPullRequest
    );
    Ok(())
}

#[test]
fn target_runner_publication_refs_normalize_to_observer_command()
-> Result<(), Box<dyn std::error::Error>> {
    let policy = nitrosend_policy()?;
    let target_plan = plan_target_repo_runner(
        &policy,
        &TargetRepoRunnerPlanRequest {
            source_id: Some("bugs-fixes".to_owned()),
            target_repo: "nitrosend/api".to_owned(),
            action: OperationalPolicyAction::IssueToPr,
            runner_id: Some("aster-production".to_owned()),
            source: TargetRepoRunnerSourceContext {
                provider: OperationalPolicySourceProvider::Slack,
                locator: "slack://nitrosend/C0APFMY0V8Q".to_owned(),
                thread_locator: Some("slack://nitrosend/C0APFMY0V8Q/1778834840.485629".to_owned()),
                thread_ts: Some("1778834840.485629".to_owned()),
                issue_url: Some("https://github.com/nitrosend/nitrosend/issues/482".to_owned()),
            },
            signal_fingerprint: Some("sha256:nitrosend-source-482".to_owned()),
            existing_pull_request: Some(TargetRepoRunnerExistingPullRequest {
                url: "https://github.com/nitrosend/api/pull/144".to_owned(),
                number: Some(144),
                branch: Some("runx/source-482".to_owned()),
            }),
        },
    )?;
    let publication = plan_target_repo_runner_source_publication_receipt(
        &target_plan,
        &TargetRepoRunnerExistingPullRequest {
            url: "https://github.com/nitrosend/api/pull/144".to_owned(),
            number: Some(144),
            branch: Some("runx/source-482".to_owned()),
        },
    );
    let source_issue_ref = publication
        .source_issue_ref
        .clone()
        .ok_or("target-runner publication should carry a source issue ref")?;

    let command = normalize_post_merge_observer_command(
        &policy,
        &PostMergeObserverCommandRequest {
            source_id: Some("bugs-fixes".to_owned()),
            source_issue_ref,
            source_thread_ref: Some(publication.source_thread_ref),
            pull_request_ref: publication.pull_request_ref,
            signal_source: PostMergeObserverSignalSource::Webhook,
            signal_ref: Some(webhook_delivery_ref()),
        },
    )?;

    assert_eq!(
        command.command_key,
        "post-merge-observer:https://github.com/nitrosend/nitrosend/issues/482:https://github.com/nitrosend/api/pull/144"
    );
    assert_eq!(command.source_id, "bugs-fixes");
    assert_eq!(command.source_issue_ref.provider.as_deref(), Some("github"));
    assert_eq!(
        command.source_issue_ref.locator.as_deref(),
        Some("nitrosend/nitrosend#482")
    );
    assert_eq!(command.pull_request_ref.provider.as_deref(), Some("github"));
    assert_eq!(
        command.source_thread_ref.as_ref().map(|reference| {
            (
                reference.reference_type.clone(),
                reference.provider.as_deref(),
                reference.locator.as_deref(),
            )
        }),
        Some((
            ReferenceType::SlackThread,
            Some("slack"),
            Some("slack://nitrosend/C0APFMY0V8Q/1778834840.485629")
        ))
    );
    Ok(())
}

#[test]
fn observer_command_rejects_missing_target_metadata_before_provider_readback()
-> Result<(), Box<dyn std::error::Error>> {
    let policy = nitrosend_policy()?;
    let mut request = observer_command_request(
        PostMergeObserverSignalSource::Webhook,
        Some(webhook_delivery_ref()),
        true,
    );
    request.pull_request_ref.provider = None;

    let error = normalize_post_merge_observer_command(&policy, &request)
        .err()
        .ok_or("expected missing target metadata error")?;

    assert!(matches!(
        error,
        PostMergeObserverPlanError::MissingObserverCommandReferenceMetadata {
            field: "pull_request_ref"
        }
    ));
    Ok(())
}

#[test]
fn webhook_command_requires_delivery_ref_before_provider_readback()
-> Result<(), Box<dyn std::error::Error>> {
    let policy = nitrosend_policy()?;
    let request = observer_command_request(PostMergeObserverSignalSource::Webhook, None, true);

    let error = normalize_post_merge_observer_command(&policy, &request)
        .err()
        .ok_or("expected missing webhook delivery ref error")?;

    assert!(matches!(
        error,
        PostMergeObserverPlanError::MissingObserverSignal {
            signal_source: PostMergeObserverSignalSource::Webhook
        }
    ));
    Ok(())
}

#[test]
fn webhook_command_rejects_non_github_delivery_provider_before_provider_readback()
-> Result<(), Box<dyn std::error::Error>> {
    let policy = nitrosend_policy()?;
    let mut signal_ref = webhook_delivery_ref();
    signal_ref.provider = Some("slack".to_owned());
    let request = observer_command_request(
        PostMergeObserverSignalSource::Webhook,
        Some(signal_ref),
        true,
    );

    let error = normalize_post_merge_observer_command(&policy, &request)
        .err()
        .ok_or("expected unsupported webhook provider error")?;

    assert!(matches!(
        error,
        PostMergeObserverPlanError::UnsupportedObserverCommandProvider {
            field: "signal_ref",
            provider
        } if provider == "slack"
    ));
    Ok(())
}

#[test]
fn sealed_harness_receipt_projects_publication_and_close_authority()
-> Result<(), Box<dyn std::error::Error>> {
    let receipt = post_merge_observer_receipt()?;
    let projection = project_post_merge_observer_publication_from_receipt(&receipt)?;

    assert_eq!(projection.reason_code, "merged_verified");
    assert_eq!(
        projection.verification_criterion_id,
        Some("post_merge.verification_passed".to_owned())
    );
    assert_eq!(
        projection.proof_criterion_id,
        "post_merge.verification_passed"
    );
    assert_eq!(
        projection.source_issue_disposition,
        PostMergeSourceIssueDisposition::Close
    );
    assert!(projection.close_authorized);
    assert_eq!(
        projection.source_issue_ref.reference_type,
        ReferenceType::GithubIssue
    );
    assert_eq!(
        projection.pull_request_ref.reference_type,
        ReferenceType::GithubPullRequest
    );
    assert_eq!(
        projection.pull_request_ref.uri,
        "github://runxhq/nitrosend/pulls/188"
    );
    assert_eq!(
        projection.merge_sha.as_deref(),
        Some("9f14c0ffee1234567890abcdef1234567890abcd")
    );
    assert_eq!(
        projection.verification_summary.as_deref(),
        Some("Nitrosend dogfood verification passed.")
    );
    assert_eq!(
        projection
            .source_thread_ref
            .as_ref()
            .map(|reference| reference.reference_type.clone()),
        Some(ReferenceType::SlackThread)
    );
    assert_eq!(
        projection.harness_receipt_ref.uri,
        "runx:harness_receipt:hrn_rcpt_post_merge_nitrosend_77_188"
    );
    Ok(())
}

#[test]
fn sealed_closed_unmerged_receipt_projects_without_verification_or_close()
-> Result<(), Box<dyn std::error::Error>> {
    let receipt = closed_unmerged_receipt()?;
    let projection = project_post_merge_observer_publication_from_receipt(&receipt)?;

    assert_eq!(projection.reason_code, "closed_unmerged");
    assert_eq!(projection.proof_criterion_id, "post_merge.provider_state");
    assert_eq!(projection.verification_criterion_id, None);
    assert_eq!(
        projection.source_issue_disposition,
        PostMergeSourceIssueDisposition::KeepOpen
    );
    assert!(!projection.close_authorized);
    assert_eq!(
        projection.pull_request_ref.reference_type,
        ReferenceType::GithubPullRequest
    );
    assert_eq!(projection.merge_sha, None);
    assert_eq!(
        projection
            .source_thread_ref
            .as_ref()
            .map(|reference| reference.reference_type.clone()),
        Some(ReferenceType::SlackThread)
    );
    assert!(!projection.summary.contains("shipped"));
    Ok(())
}

#[test]
fn publication_projection_rejects_unsealed_or_under_proven_receipts()
-> Result<(), Box<dyn std::error::Error>> {
    let mut unsealed = post_merge_observer_receipt()?;
    unsealed.harness.state = HarnessState::Running;
    assert!(matches!(
        project_post_merge_observer_publication_from_receipt(&unsealed),
        Err(PostMergeObserverPlanError::ReceiptNotSealed)
    ));

    let mut missing_verification = post_merge_observer_receipt()?;
    for criterion in &mut missing_verification.seal.criteria {
        if criterion.criterion_id == "post_merge.verification_passed" {
            criterion.verification_refs.clear();
        }
    }
    missing_verification.harness.seal = Some(missing_verification.seal.clone());
    assert!(matches!(
        project_post_merge_observer_publication_from_receipt(&missing_verification),
        Err(PostMergeObserverPlanError::ReceiptPublicationNotAuthorized(
            _
        ))
    ));

    let mut missing_merge_sha = post_merge_observer_receipt()?;
    missing_merge_sha.metadata = None;
    assert!(matches!(
        project_post_merge_observer_publication_from_receipt(&missing_merge_sha),
        Err(PostMergeObserverPlanError::MissingReceiptMetadata(
            "merge_sha"
        ))
    ));
    Ok(())
}

#[test]
fn missing_source_thread_fails_closed_before_planning() -> Result<(), Box<dyn std::error::Error>> {
    let policy = nitrosend_policy()?;
    let error = plan_post_merge_observer_closure(
        &policy,
        &observer_request(
            true,
            Some("9f14c0ffee1234567890abcdef1234567890abcd"),
            PostMergeVerificationStatus::Failed,
            false,
        ),
    )
    .err()
    .ok_or("expected missing source thread planning error")?;

    match error {
        PostMergeObserverPlanError::MissingSourceThread { source_id } => {
            assert_eq!(source_id, "bugs-fixes");
        }
        other => return Err(format!("unexpected error: {other}").into()),
    }
    Ok(())
}

#[test]
fn missing_source_thread_fails_before_provider_state_planning()
-> Result<(), Box<dyn std::error::Error>> {
    let policy = nitrosend_policy()?;
    let mut request = observer_request(
        true,
        Some("9f14c0ffee1234567890abcdef1234567890abcd"),
        PostMergeVerificationStatus::Passed,
        false,
    );
    request.pull_request.state = PostMergePullRequestState::Open;

    let error = plan_post_merge_observer_closure(&policy, &request)
        .err()
        .ok_or("expected missing source thread planning error")?;

    match error {
        PostMergeObserverPlanError::MissingSourceThread { source_id } => {
            assert_eq!(source_id, "bugs-fixes");
        }
        PostMergeObserverPlanError::ProviderStateNotTerminal => {
            return Err("source-thread routing must fail before provider-state planning".into());
        }
        other => return Err(format!("unexpected error: {other}").into()),
    }
    Ok(())
}

fn nitrosend_policy() -> Result<OperationalPolicy, serde_json::Error> {
    serde_json::from_str(NITROSEND_LIKE)
}

fn post_merge_observer_receipt() -> Result<HarnessReceipt, serde_json::Error> {
    #[derive(serde::Deserialize)]
    struct Fixture {
        expected: serde_json::Value,
    }

    let fixture: Fixture = serde_json::from_str(POST_MERGE_OBSERVER_FIXTURE)?;
    serde_json::from_value(fixture.expected)
}

fn closed_unmerged_receipt() -> Result<HarnessReceipt, serde_json::Error> {
    let mut receipt = post_merge_observer_receipt()?;
    receipt.seal.reason_code = "closed_unmerged".to_owned();
    receipt.seal.summary =
        "Target PR was closed without merge; source issue remains unresolved.".to_owned();
    receipt.seal.disposition = ClosureDisposition::Closed;
    receipt.seal.criteria.retain(|criterion| {
        matches!(
            criterion.criterion_id.as_str(),
            "post_merge.provider_state"
                | "post_merge.human_gate"
                | "post_merge.source_thread_target_present"
        )
    });
    for criterion in &mut receipt.seal.criteria {
        if criterion.criterion_id == "post_merge.provider_state" {
            criterion.summary = Some("Provider reported closed without merge.".to_owned());
        }
    }
    receipt
        .harness
        .acts
        .retain(|act| act.form == ActForm::Observation || act.form == ActForm::Reply);
    receipt.harness.idempotency.content_hash =
        "sha256:post-merge-closure-closed-unmerged-nitrosend".to_owned();
    receipt.harness.seal = Some(receipt.seal.clone());
    Ok(receipt)
}

fn observer_request(
    merged: bool,
    merge_sha: Option<&str>,
    verification_status: PostMergeVerificationStatus,
    include_source_thread: bool,
) -> PostMergeObserverPlanRequest {
    PostMergeObserverPlanRequest {
        source_id: Some("bugs-fixes".to_owned()),
        source_issue_ref: source_issue_ref(),
        source_thread_ref: include_source_thread.then(source_thread_ref),
        pull_request: PostMergePullRequestObservation {
            provider: PostMergeProvider::Github,
            repo: "nitrosend/api".to_owned(),
            number: 144,
            uri: "github://nitrosend/api/pulls/144".to_owned(),
            state: PostMergePullRequestState::Closed,
            merged,
            merge_sha: merge_sha.map(str::to_owned),
            observed_at: "2026-05-20T05:20:00Z".to_owned(),
            closed_at: Some("2026-05-20T05:19:30Z".to_owned()),
            actor: Some("github:user:human-reviewer".to_owned()),
        },
        verification: PostMergeVerificationObservation {
            status: verification_status,
            summary: Some(
                match verification_status {
                    PostMergeVerificationStatus::Passed => "dogfood smoke passed",
                    PostMergeVerificationStatus::Failed => "dogfood smoke failed",
                    PostMergeVerificationStatus::Pending => "dogfood smoke pending",
                    PostMergeVerificationStatus::NotRequired => "verification not applicable",
                }
                .to_owned(),
            ),
            verification_ref: Some(reference(
                ReferenceType::Verification,
                "runx:verification:nitrosend-dogfood-smoke",
                "nitrosend dogfood smoke",
            )),
            evidence_refs: vec![reference(
                ReferenceType::Deployment,
                "deploy://nitrosend/dogfood/2026-05-20T05-12Z",
                "nitrosend dogfood deploy",
            )],
            verified_at: Some("2026-05-20T05:20:30Z".to_owned()),
        },
    }
}

fn observer_command_request(
    signal_source: PostMergeObserverSignalSource,
    signal_ref: Option<Reference>,
    include_source_thread: bool,
) -> PostMergeObserverCommandRequest {
    PostMergeObserverCommandRequest {
        source_id: Some("bugs-fixes".to_owned()),
        source_issue_ref: source_issue_ref(),
        source_thread_ref: include_source_thread.then(source_thread_ref),
        pull_request_ref: pull_request_ref(),
        signal_source,
        signal_ref,
    }
}

fn source_issue_ref() -> Reference {
    Reference {
        reference_type: ReferenceType::GithubIssue,
        uri: "github://nitrosend/nitrosend/issues/482".to_owned(),
        provider: Some("github".to_owned()),
        locator: Some("nitrosend/nitrosend#482".to_owned()),
        label: Some("Nitrosend source issue".to_owned()),
        observed_at: None,
        proof_kind: None,
    }
}

fn pull_request_ref() -> Reference {
    Reference {
        reference_type: ReferenceType::GithubPullRequest,
        uri: "github://nitrosend/api/pulls/144".to_owned(),
        provider: Some("github".to_owned()),
        locator: Some("nitrosend/api#144".to_owned()),
        label: Some("Nitrosend target PR".to_owned()),
        observed_at: None,
        proof_kind: None,
    }
}

fn source_thread_ref() -> Reference {
    Reference {
        reference_type: ReferenceType::SlackThread,
        uri: "slack://nitrosend/C0APFMY0V8Q/p1778834840.485629".to_owned(),
        provider: Some("slack".to_owned()),
        locator: Some("nitrosend/C0APFMY0V8Q/1778834840.485629".to_owned()),
        label: Some("Nitrosend source thread".to_owned()),
        observed_at: None,
        proof_kind: None,
    }
}

fn webhook_delivery_ref() -> Reference {
    Reference {
        reference_type: ReferenceType::WebhookDelivery,
        uri: "github://webhook-deliveries/evt_01HX".to_owned(),
        provider: Some("github".to_owned()),
        locator: Some("nitrosend/api/delivery/evt_01HX".to_owned()),
        label: Some("GitHub pull_request webhook delivery".to_owned()),
        observed_at: Some("2026-05-20T05:20:00Z".to_owned()),
        proof_kind: None,
    }
}

fn reference(reference_type: ReferenceType, uri: &str, label: &str) -> Reference {
    Reference {
        reference_type,
        uri: uri.to_owned(),
        provider: None,
        locator: None,
        label: Some(label.to_owned()),
        observed_at: None,
        proof_kind: None,
    }
}

fn criterion_ids(plan: &PostMergeObserverPlan) -> Vec<&str> {
    plan.seal_criteria
        .iter()
        .map(|criterion| criterion.criterion_id.as_str())
        .collect()
}

fn criterion<'a>(
    plan: &'a PostMergeObserverPlan,
    criterion_id: &str,
) -> Result<&'a PostMergeObserverCriterionPlan, Box<dyn std::error::Error>> {
    plan.seal_criteria
        .iter()
        .find(|criterion| criterion.criterion_id == criterion_id)
        .ok_or_else(|| format!("missing criterion {criterion_id}").into())
}
