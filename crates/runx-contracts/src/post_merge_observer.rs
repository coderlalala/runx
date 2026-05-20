//! Post-merge observer closure planning contracts.
// rust-style-allow: large-file - closure plan schemas and planner helpers stay together to preserve the contract surface.

use std::fmt;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::operational_policy::OperationalPolicySourceRule;
use crate::{
    ActForm, ClosureDisposition, CriterionStatus, HarnessReceipt, HarnessState, OperationalPolicy,
    OperationalPolicyError, OperationalPolicyPublishMode, OperationalPolicySourceIssueClosureMode,
    Reference, ReferenceType, validate_operational_policy_semantics,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostMergeProvider {
    Github,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostMergePullRequestState {
    Open,
    Closed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostMergePullRequestObservation {
    pub provider: PostMergeProvider,
    pub repo: String,
    pub number: u64,
    pub uri: String,
    pub state: PostMergePullRequestState,
    pub merged: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_sha: Option<String>,
    pub observed_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostMergeVerificationStatus {
    Passed,
    Failed,
    Pending,
    NotRequired,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostMergeVerificationObservation {
    pub status: PostMergeVerificationStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_ref: Option<Reference>,
    #[serde(default)]
    pub evidence_refs: Vec<Reference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostMergeObserverPlanRequest {
    pub source_id: Option<String>,
    pub source_issue_ref: Reference,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_thread_ref: Option<Reference>,
    pub pull_request: PostMergePullRequestObservation,
    pub verification: PostMergeVerificationObservation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostMergeObserverClosureState {
    MergedVerified,
    FailedVerification,
    MergedPendingVerification,
    ClosedUnmerged,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostMergeSourceIssueDisposition {
    KeepOpen,
    Close,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PostMergeObserverPlan {
    pub policy_id: String,
    pub source_id: String,
    pub final_state: PostMergeObserverClosureState,
    pub reason_code: String,
    pub seal_disposition: ClosureDisposition,
    pub summary: String,
    pub closure_key: String,
    pub observed_at: String,
    pub provider: PostMergeObserverProviderPlan,
    pub verification: PostMergeObserverVerificationPlan,
    pub publication: PostMergeObserverPublicationPlan,
    pub source_issue: PostMergeObserverSourceIssuePlan,
    pub act_forms: Vec<ActForm>,
    pub seal_criteria: Vec<PostMergeObserverCriterionPlan>,
    pub idempotency: PostMergeObserverIdempotencyPlan,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PostMergeObserverProviderPlan {
    pub provider: PostMergeProvider,
    pub pull_request_ref: Reference,
    pub merged: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_sha: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PostMergeObserverVerificationPlan {
    pub required: bool,
    pub status: PostMergeVerificationStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub criterion_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_ref: Option<Reference>,
    #[serde(default)]
    pub evidence_refs: Vec<Reference>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PostMergeObserverPublicationPlan {
    pub final_source_thread_update: bool,
    pub source_issue_comment_required: bool,
    pub publish_mode: OperationalPolicyPublishMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_thread_ref: Option<Reference>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PostMergeObserverSourceIssuePlan {
    pub disposition: PostMergeSourceIssueDisposition,
    pub reason: String,
    pub target_ref: Reference,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PostMergeObserverCriterionPlan {
    pub criterion_id: String,
    pub status: CriterionStatus,
    pub required: bool,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub act_form: Option<ActForm>,
    #[serde(default)]
    pub evidence_refs: Vec<Reference>,
    #[serde(default)]
    pub verification_refs: Vec<Reference>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PostMergeObserverIdempotencyPlan {
    pub closure_key: String,
    pub act_forms: Vec<ActForm>,
    pub intent_key: String,
    pub trigger_fingerprint: String,
    pub content_hash: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostMergeObserverSignalSource {
    Webhook,
    Scheduler,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PostMergeObserverRuntimeDecision {
    SealAndPublish,
    AlreadyPublished,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PostMergeObserverRuntimeDedupePlan {
    pub decision: PostMergeObserverRuntimeDecision,
    pub signal_source: PostMergeObserverSignalSource,
    pub lock_key: String,
    pub receipt_id: String,
    pub receipt_ref: Reference,
    pub publication_key: String,
    pub content_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PostMergeObserverPublicationProjection {
    pub harness_receipt_ref: Reference,
    pub source_issue_ref: Reference,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_thread_ref: Option<Reference>,
    pub reason_code: String,
    pub summary: String,
    pub proof_criterion_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_criterion_id: Option<String>,
    pub source_issue_disposition: PostMergeSourceIssueDisposition,
    pub close_authorized: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PostMergeObserverPlanError {
    Policy(OperationalPolicyError),
    ProviderObservationDisabled,
    SourceRequired,
    UnknownSource(String),
    ProviderStateNotTerminal,
    MissingSourceThread { source_id: String },
    VerificationRequired,
    InconsistentObservation(String),
    ReceiptNotSealed,
    ReceiptNotPostMergeObserver,
    MissingReceiptCriterion(String),
    MissingReceiptReference(&'static str),
    ReceiptPublicationNotAuthorized(String),
}

impl fmt::Display for PostMergeObserverPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Policy(error) => write!(formatter, "operational policy error: {error}"),
            Self::ProviderObservationDisabled => {
                formatter.write_str("post-merge observer planning requires observe_provider=true")
            }
            Self::SourceRequired => {
                formatter.write_str("post-merge observer planning requires a source_id")
            }
            Self::UnknownSource(source_id) => {
                write!(
                    formatter,
                    "post-merge observer planning references unknown source '{source_id}'"
                )
            }
            Self::ProviderStateNotTerminal => {
                formatter.write_str("post-merge observer planning requires terminal PR state")
            }
            Self::MissingSourceThread { source_id } => {
                write!(
                    formatter,
                    "source '{source_id}' requires a source-thread target before final publication"
                )
            }
            Self::VerificationRequired => {
                formatter.write_str("merged post-merge observer planning requires verification")
            }
            Self::InconsistentObservation(message) => formatter.write_str(message),
            Self::ReceiptNotSealed => {
                formatter.write_str("post-merge publication requires a sealed harness receipt")
            }
            Self::ReceiptNotPostMergeObserver => {
                formatter.write_str("sealed harness receipt is not a post-merge observer closure")
            }
            Self::MissingReceiptCriterion(criterion_id) => {
                write!(
                    formatter,
                    "sealed post-merge receipt is missing required criterion '{criterion_id}'"
                )
            }
            Self::MissingReceiptReference(kind) => {
                write!(
                    formatter,
                    "sealed post-merge receipt is missing required {kind} reference"
                )
            }
            Self::ReceiptPublicationNotAuthorized(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for PostMergeObserverPlanError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Policy(error) => Some(error),
            Self::ProviderObservationDisabled
            | Self::SourceRequired
            | Self::UnknownSource(_)
            | Self::ProviderStateNotTerminal
            | Self::MissingSourceThread { .. }
            | Self::VerificationRequired
            | Self::InconsistentObservation(_)
            | Self::ReceiptNotSealed
            | Self::ReceiptNotPostMergeObserver
            | Self::MissingReceiptCriterion(_)
            | Self::MissingReceiptReference(_)
            | Self::ReceiptPublicationNotAuthorized(_) => None,
        }
    }
}

pub fn plan_post_merge_observer_closure(
    policy: &OperationalPolicy,
    request: &PostMergeObserverPlanRequest,
) -> Result<PostMergeObserverPlan, PostMergeObserverPlanError> {
    let source = validated_observer_source(policy, request)?;
    let publication = publication_plan(policy, source, request)?;
    let final_state = classify_closure(policy, request)?;
    let pull_request_ref = pull_request_ref(&request.pull_request);
    let source_issue = source_issue_plan(
        policy.post_merge.source_issue_closure_mode,
        final_state,
        request,
    );
    let verification = verification_plan(policy, final_state, &request.verification);
    let act_forms = act_forms(final_state, &publication, source_issue.disposition);
    let seal_criteria = seal_criteria(
        policy,
        final_state,
        request,
        &pull_request_ref,
        &publication,
        &source_issue,
        &verification,
    );
    let reason_code = closure_reason(final_state).to_owned();
    let seal_disposition = seal_disposition(final_state);
    let closure_key = closure_key(&request.pull_request, final_state);
    let summary = closure_summary(final_state).to_owned();
    let idempotency = idempotency_plan(
        request,
        final_state,
        &reason_code,
        &closure_key,
        &act_forms,
        &seal_criteria,
    );

    Ok(PostMergeObserverPlan {
        policy_id: policy.policy_id.clone(),
        source_id: source.source_id.clone(),
        final_state,
        reason_code,
        seal_disposition,
        summary,
        closure_key,
        observed_at: request.pull_request.observed_at.clone(),
        provider: PostMergeObserverProviderPlan {
            provider: request.pull_request.provider,
            pull_request_ref,
            merged: request.pull_request.merged,
            merge_sha: request.pull_request.merge_sha.clone(),
        },
        verification,
        publication,
        source_issue,
        act_forms,
        seal_criteria,
        idempotency,
    })
}

pub fn plan_post_merge_observer_runtime_dedupe(
    plan: &PostMergeObserverPlan,
    signal_source: PostMergeObserverSignalSource,
    existing_receipt_ref: Option<Reference>,
) -> PostMergeObserverRuntimeDedupePlan {
    let receipt_id = post_merge_observer_receipt_id(plan);
    let already_published = existing_receipt_ref
        .as_ref()
        .is_some_and(|reference| reference.uri == format!("runx:harness_receipt:{receipt_id}"));
    let receipt_ref = existing_receipt_ref.unwrap_or_else(|| Reference {
        reference_type: ReferenceType::HarnessReceipt,
        uri: format!("runx:harness_receipt:{receipt_id}"),
        provider: None,
        locator: Some(plan.idempotency.content_hash.clone()),
        label: Some("post-merge observer harness receipt".to_owned()),
        observed_at: None,
    });
    PostMergeObserverRuntimeDedupePlan {
        decision: if already_published {
            PostMergeObserverRuntimeDecision::AlreadyPublished
        } else {
            PostMergeObserverRuntimeDecision::SealAndPublish
        },
        signal_source,
        lock_key: format!("post-merge-observer:{}", plan.idempotency.content_hash),
        receipt_id,
        receipt_ref,
        publication_key: format!(
            "post-merge-publication:{}:{}",
            plan.idempotency.intent_key, plan.idempotency.content_hash
        ),
        content_hash: plan.idempotency.content_hash.clone(),
    }
}

pub fn project_post_merge_observer_publication_from_receipt(
    receipt: &HarnessReceipt,
) -> Result<PostMergeObserverPublicationProjection, PostMergeObserverPlanError> {
    let final_state = require_projectable_post_merge_receipt(receipt)?;
    let publication_criteria = require_publication_criteria(receipt, final_state)?;
    let source_issue_ref =
        required_receipt_reference(receipt, ReferenceType::GithubIssue, "source issue")?;
    let source_thread_ref =
        required_receipt_reference(receipt, ReferenceType::SlackThread, "source thread")?;
    let close_authorized = receipt_close_authorized(receipt)?;

    Ok(PostMergeObserverPublicationProjection {
        harness_receipt_ref: harness_receipt_ref(receipt),
        source_issue_ref,
        source_thread_ref: Some(source_thread_ref),
        reason_code: receipt.seal.reason_code.clone(),
        summary: receipt.seal.summary.clone(),
        proof_criterion_id: publication_criteria.proof_criterion_id.to_owned(),
        verification_criterion_id: publication_criteria
            .verification_criterion_id
            .map(str::to_owned),
        source_issue_disposition: source_issue_disposition(close_authorized),
        close_authorized,
    })
}

struct PostMergeObserverPublicationCriteria {
    proof_criterion_id: &'static str,
    verification_criterion_id: Option<&'static str>,
}

fn require_projectable_post_merge_receipt(
    receipt: &HarnessReceipt,
) -> Result<PostMergeObserverClosureState, PostMergeObserverPlanError> {
    if receipt.harness.state != HarnessState::Sealed
        || receipt.harness.seal.as_ref() != Some(&receipt.seal)
    {
        return Err(PostMergeObserverPlanError::ReceiptNotSealed);
    }
    closure_state_from_reason(&receipt.seal.reason_code)
        .ok_or(PostMergeObserverPlanError::ReceiptNotPostMergeObserver)
}

fn require_publication_criteria(
    receipt: &HarnessReceipt,
    final_state: PostMergeObserverClosureState,
) -> Result<PostMergeObserverPublicationCriteria, PostMergeObserverPlanError> {
    require_receipt_criterion(receipt, "post_merge.provider_state")?;
    require_receipt_criterion(receipt, "post_merge.human_gate")?;
    let verification_criterion_id = verification_criterion_id(final_state);
    if let Some(verification_criterion_id) = verification_criterion_id {
        let verification_criterion = require_receipt_criterion(receipt, verification_criterion_id)?;
        if verification_criterion.verification_refs.is_empty() {
            return Err(PostMergeObserverPlanError::ReceiptPublicationNotAuthorized(
                "final publication requires proof-bound verification refs".to_owned(),
            ));
        }
    }
    require_receipt_criterion(receipt, "post_merge.source_thread_target_present")?;
    Ok(PostMergeObserverPublicationCriteria {
        proof_criterion_id: verification_criterion_id.unwrap_or("post_merge.provider_state"),
        verification_criterion_id,
    })
}

fn receipt_close_authorized(receipt: &HarnessReceipt) -> Result<bool, PostMergeObserverPlanError> {
    let close_authorized = receipt
        .seal
        .criteria
        .iter()
        .any(|criterion| criterion.criterion_id == "post_merge.close_policy_authorized");
    if close_authorized {
        require_receipt_criterion(receipt, "post_merge.close_policy_authorized")?;
    }
    Ok(close_authorized)
}

fn required_receipt_reference(
    receipt: &HarnessReceipt,
    reference_type: ReferenceType,
    label: &'static str,
) -> Result<Reference, PostMergeObserverPlanError> {
    receipt_reference(receipt, reference_type)
        .ok_or(PostMergeObserverPlanError::MissingReceiptReference(label))
}

fn harness_receipt_ref(receipt: &HarnessReceipt) -> Reference {
    Reference {
        reference_type: ReferenceType::HarnessReceipt,
        uri: format!("runx:harness_receipt:{}", receipt.id),
        provider: None,
        locator: Some(receipt.seal.digest.clone()),
        label: Some("sealed post-merge observer receipt".to_owned()),
        observed_at: Some(receipt.seal.closed_at.clone()),
    }
}

fn source_issue_disposition(close_authorized: bool) -> PostMergeSourceIssueDisposition {
    if close_authorized {
        PostMergeSourceIssueDisposition::Close
    } else {
        PostMergeSourceIssueDisposition::KeepOpen
    }
}

fn validated_observer_source<'a>(
    policy: &'a OperationalPolicy,
    request: &PostMergeObserverPlanRequest,
) -> Result<&'a OperationalPolicySourceRule, PostMergeObserverPlanError> {
    validate_operational_policy_semantics(policy).map_err(PostMergeObserverPlanError::Policy)?;
    if !policy.post_merge.observe_provider {
        return Err(PostMergeObserverPlanError::ProviderObservationDisabled);
    }
    select_source(policy, request.source_id.as_deref())
}

fn publication_plan(
    policy: &OperationalPolicy,
    source: &OperationalPolicySourceRule,
    request: &PostMergeObserverPlanRequest,
) -> Result<PostMergeObserverPublicationPlan, PostMergeObserverPlanError> {
    let final_source_thread_update = source_thread_publication_required(policy, source);
    if final_source_thread_update && request.source_thread_ref.is_none() {
        return Err(PostMergeObserverPlanError::MissingSourceThread {
            source_id: source.source_id.clone(),
        });
    }
    Ok(PostMergeObserverPublicationPlan {
        final_source_thread_update,
        source_issue_comment_required: policy.post_merge.publish_source_thread_closure_update,
        publish_mode: source.source_thread.publish_mode,
        source_thread_ref: request.source_thread_ref.clone(),
    })
}

fn select_source<'a>(
    policy: &'a OperationalPolicy,
    source_id: Option<&str>,
) -> Result<&'a OperationalPolicySourceRule, PostMergeObserverPlanError> {
    if let Some(source_id) = source_id.and_then(non_empty_string) {
        return policy
            .sources
            .iter()
            .find(|source| source.source_id == source_id)
            .ok_or_else(|| PostMergeObserverPlanError::UnknownSource(source_id.to_owned()));
    }
    if policy.sources.len() == 1 {
        return policy
            .sources
            .first()
            .ok_or(PostMergeObserverPlanError::SourceRequired);
    }
    Err(PostMergeObserverPlanError::SourceRequired)
}

fn classify_closure(
    policy: &OperationalPolicy,
    request: &PostMergeObserverPlanRequest,
) -> Result<PostMergeObserverClosureState, PostMergeObserverPlanError> {
    if request.pull_request.state != PostMergePullRequestState::Closed {
        return Err(PostMergeObserverPlanError::ProviderStateNotTerminal);
    }
    if !request.pull_request.merged {
        return Ok(PostMergeObserverClosureState::ClosedUnmerged);
    }
    if request
        .pull_request
        .merge_sha
        .as_deref()
        .is_some_and(str::is_empty)
    {
        return Err(PostMergeObserverPlanError::InconsistentObservation(
            "merged pull request observation has an empty merge_sha".to_owned(),
        ));
    }

    match request.verification.status {
        PostMergeVerificationStatus::Passed => Ok(PostMergeObserverClosureState::MergedVerified),
        PostMergeVerificationStatus::Failed => {
            Ok(PostMergeObserverClosureState::FailedVerification)
        }
        PostMergeVerificationStatus::Pending => {
            Ok(PostMergeObserverClosureState::MergedPendingVerification)
        }
        PostMergeVerificationStatus::NotRequired if policy.post_merge.verification_required => {
            Err(PostMergeObserverPlanError::VerificationRequired)
        }
        PostMergeVerificationStatus::NotRequired => {
            Ok(PostMergeObserverClosureState::MergedVerified)
        }
    }
}

fn source_thread_publication_required(
    policy: &OperationalPolicy,
    source: &OperationalPolicySourceRule,
) -> bool {
    policy.post_merge.publish_source_thread_closure_update
        && source.source_thread.publish_mode != OperationalPolicyPublishMode::None
}

fn source_issue_plan(
    close_mode: OperationalPolicySourceIssueClosureMode,
    final_state: PostMergeObserverClosureState,
    request: &PostMergeObserverPlanRequest,
) -> PostMergeObserverSourceIssuePlan {
    let disposition = match (close_mode, final_state) {
        (
            OperationalPolicySourceIssueClosureMode::WhenVerified
            | OperationalPolicySourceIssueClosureMode::WhenTerminal,
            PostMergeObserverClosureState::MergedVerified,
        ) => PostMergeSourceIssueDisposition::Close,
        (
            OperationalPolicySourceIssueClosureMode::WhenTerminal,
            PostMergeObserverClosureState::FailedVerification,
        ) => PostMergeSourceIssueDisposition::Close,
        _ => PostMergeSourceIssueDisposition::KeepOpen,
    };

    let reason = match disposition {
        PostMergeSourceIssueDisposition::Close => {
            "source issue closure is authorized by post-merge policy".to_owned()
        }
        PostMergeSourceIssueDisposition::KeepOpen => keep_open_reason(final_state).to_owned(),
    };

    PostMergeObserverSourceIssuePlan {
        disposition,
        reason,
        target_ref: request.source_issue_ref.clone(),
    }
}

fn keep_open_reason(final_state: PostMergeObserverClosureState) -> &'static str {
    match final_state {
        PostMergeObserverClosureState::MergedVerified => {
            "source issue remains open because policy does not close verified post-merge states"
        }
        PostMergeObserverClosureState::FailedVerification => {
            "source issue remains open because merge verification failed"
        }
        PostMergeObserverClosureState::MergedPendingVerification => {
            "source issue remains open until post-merge verification completes"
        }
        PostMergeObserverClosureState::ClosedUnmerged => {
            "source issue remains open because the target PR closed without merge"
        }
    }
}

fn verification_plan(
    policy: &OperationalPolicy,
    final_state: PostMergeObserverClosureState,
    verification: &PostMergeVerificationObservation,
) -> PostMergeObserverVerificationPlan {
    PostMergeObserverVerificationPlan {
        required: policy.post_merge.verification_required
            && final_state != PostMergeObserverClosureState::ClosedUnmerged,
        status: if final_state == PostMergeObserverClosureState::ClosedUnmerged {
            PostMergeVerificationStatus::NotRequired
        } else {
            verification.status
        },
        criterion_id: verification_criterion_id(final_state).map(str::to_owned),
        verification_ref: verification.verification_ref.clone(),
        evidence_refs: verification.evidence_refs.clone(),
    }
}

fn act_forms(
    final_state: PostMergeObserverClosureState,
    publication: &PostMergeObserverPublicationPlan,
    source_issue_disposition: PostMergeSourceIssueDisposition,
) -> Vec<ActForm> {
    let mut forms = vec![ActForm::Observation];
    if final_state != PostMergeObserverClosureState::ClosedUnmerged {
        forms.push(ActForm::Verification);
    }
    if publication.final_source_thread_update || publication.source_issue_comment_required {
        forms.push(ActForm::Reply);
    }
    if source_issue_disposition == PostMergeSourceIssueDisposition::Close {
        forms.push(ActForm::Revision);
    }
    forms
}

fn seal_criteria(
    policy: &OperationalPolicy,
    final_state: PostMergeObserverClosureState,
    request: &PostMergeObserverPlanRequest,
    pull_request_ref: &Reference,
    publication: &PostMergeObserverPublicationPlan,
    source_issue: &PostMergeObserverSourceIssuePlan,
    verification: &PostMergeObserverVerificationPlan,
) -> Vec<PostMergeObserverCriterionPlan> {
    let mut criteria = Vec::new();
    criteria.push(provider_criterion(final_state, pull_request_ref));
    if policy.permissions.require_human_merge_gate {
        criteria.push(human_gate_criterion(final_state, pull_request_ref));
    }
    if let Some(criterion_id) = &verification.criterion_id {
        criteria.push(verification_criterion(
            final_state,
            request,
            verification,
            criterion_id,
        ));
    }
    if publication.final_source_thread_update {
        criteria.push(source_thread_criterion(request, verification));
    }
    if source_issue.disposition == PostMergeSourceIssueDisposition::Close {
        criteria.push(close_policy_criterion(source_issue, verification));
    }
    criteria
}

fn provider_criterion(
    final_state: PostMergeObserverClosureState,
    pull_request_ref: &Reference,
) -> PostMergeObserverCriterionPlan {
    PostMergeObserverCriterionPlan {
        criterion_id: "post_merge.provider_state".to_owned(),
        status: CriterionStatus::Verified,
        required: true,
        summary: provider_criterion_summary(final_state).to_owned(),
        act_form: Some(ActForm::Observation),
        evidence_refs: vec![pull_request_ref.clone()],
        verification_refs: Vec::new(),
    }
}

fn human_gate_criterion(
    final_state: PostMergeObserverClosureState,
    pull_request_ref: &Reference,
) -> PostMergeObserverCriterionPlan {
    PostMergeObserverCriterionPlan {
        criterion_id: "post_merge.human_gate".to_owned(),
        status: CriterionStatus::Verified,
        required: true,
        summary: human_gate_summary(final_state).to_owned(),
        act_form: Some(ActForm::Observation),
        evidence_refs: vec![pull_request_ref.clone()],
        verification_refs: Vec::new(),
    }
}

fn verification_criterion(
    final_state: PostMergeObserverClosureState,
    request: &PostMergeObserverPlanRequest,
    verification: &PostMergeObserverVerificationPlan,
    criterion_id: &str,
) -> PostMergeObserverCriterionPlan {
    PostMergeObserverCriterionPlan {
        criterion_id: criterion_id.to_owned(),
        status: verification_criterion_status(final_state),
        required: verification.required,
        summary: verification_summary(final_state, &request.verification).to_owned(),
        act_form: Some(ActForm::Verification),
        evidence_refs: verification.evidence_refs.clone(),
        verification_refs: verification_refs(verification),
    }
}

fn source_thread_criterion(
    request: &PostMergeObserverPlanRequest,
    verification: &PostMergeObserverVerificationPlan,
) -> PostMergeObserverCriterionPlan {
    PostMergeObserverCriterionPlan {
        criterion_id: "post_merge.source_thread_target_present".to_owned(),
        status: CriterionStatus::Verified,
        required: true,
        summary: "final source-thread publication is bound to a thread target".to_owned(),
        act_form: Some(ActForm::Reply),
        evidence_refs: [
            request.source_thread_ref.clone(),
            Some(request.source_issue_ref.clone()),
        ]
        .into_iter()
        .flatten()
        .collect(),
        verification_refs: verification_refs(verification),
    }
}

fn close_policy_criterion(
    source_issue: &PostMergeObserverSourceIssuePlan,
    verification: &PostMergeObserverVerificationPlan,
) -> PostMergeObserverCriterionPlan {
    PostMergeObserverCriterionPlan {
        criterion_id: "post_merge.close_policy_authorized".to_owned(),
        status: CriterionStatus::Verified,
        required: true,
        summary: "source issue closure is authorized by post-merge policy".to_owned(),
        act_form: Some(ActForm::Revision),
        evidence_refs: vec![source_issue.target_ref.clone()],
        verification_refs: verification_refs(verification),
    }
}

fn verification_refs(verification: &PostMergeObserverVerificationPlan) -> Vec<Reference> {
    verification.verification_ref.iter().cloned().collect()
}

fn idempotency_plan(
    request: &PostMergeObserverPlanRequest,
    final_state: PostMergeObserverClosureState,
    reason_code: &str,
    closure_key: &str,
    act_forms: &[ActForm],
    criteria: &[PostMergeObserverCriterionPlan],
) -> PostMergeObserverIdempotencyPlan {
    let intent_key = format!(
        "post-merge:{}:{}",
        request.source_issue_ref.uri, request.pull_request.uri
    );
    let trigger_material = trigger_fingerprint_material(request);
    let content_material =
        content_hash_material(final_state, reason_code, closure_key, act_forms, criteria);

    PostMergeObserverIdempotencyPlan {
        closure_key: closure_key.to_owned(),
        act_forms: act_forms.to_vec(),
        intent_key,
        trigger_fingerprint: sha256_prefixed(&trigger_material),
        content_hash: sha256_prefixed(&content_material),
    }
}

fn trigger_fingerprint_material(request: &PostMergeObserverPlanRequest) -> String {
    let mut trigger_material = String::new();
    push_kv(
        &mut trigger_material,
        "source_issue",
        &request.source_issue_ref.uri,
    );
    push_kv(
        &mut trigger_material,
        "pull_request",
        &request.pull_request.uri,
    );
    push_kv(
        &mut trigger_material,
        "provider_state",
        pull_request_state_name(request.pull_request.state),
    );
    push_kv(
        &mut trigger_material,
        "merged",
        if request.pull_request.merged {
            "true"
        } else {
            "false"
        },
    );
    push_kv(
        &mut trigger_material,
        "merge_sha",
        request.pull_request.merge_sha.as_deref().unwrap_or(""),
    );
    push_kv(
        &mut trigger_material,
        "verification",
        verification_status_name(request.verification.status),
    );
    trigger_material
}

fn content_hash_material(
    final_state: PostMergeObserverClosureState,
    reason_code: &str,
    closure_key: &str,
    act_forms: &[ActForm],
    criteria: &[PostMergeObserverCriterionPlan],
) -> String {
    let mut content_material = String::new();
    push_kv(&mut content_material, "reason_code", reason_code);
    push_kv(&mut content_material, "closure_key", closure_key);
    push_kv(
        &mut content_material,
        "final_state",
        closure_state_name(final_state),
    );
    for act_form in act_forms {
        push_kv(&mut content_material, "act_form", act_form_name(act_form));
    }
    for criterion in criteria {
        push_kv(&mut content_material, "criterion", &criterion.criterion_id);
        push_kv(
            &mut content_material,
            "criterion_status",
            criterion_status_name(&criterion.status),
        );
    }
    content_material
}

fn pull_request_ref(observation: &PostMergePullRequestObservation) -> Reference {
    Reference {
        reference_type: ReferenceType::GithubPullRequest,
        uri: observation.uri.clone(),
        provider: Some(provider_name(observation.provider).to_owned()),
        locator: Some(format!("{}#{}", observation.repo, observation.number)),
        label: Some("observed pull request".to_owned()),
        observed_at: Some(observation.observed_at.clone()),
    }
}

fn closure_key(
    observation: &PostMergePullRequestObservation,
    final_state: PostMergeObserverClosureState,
) -> String {
    match final_state {
        PostMergeObserverClosureState::MergedVerified
        | PostMergeObserverClosureState::FailedVerification
        | PostMergeObserverClosureState::MergedPendingVerification => format!(
            "{}@merge:{}",
            observation.uri,
            observation.merge_sha.as_deref().unwrap_or("missing")
        ),
        PostMergeObserverClosureState::ClosedUnmerged => format!(
            "{}@closed-unmerged:{}",
            observation.uri,
            observation
                .closed_at
                .as_deref()
                .unwrap_or(observation.observed_at.as_str())
        ),
    }
}

fn verification_criterion_id(final_state: PostMergeObserverClosureState) -> Option<&'static str> {
    match final_state {
        PostMergeObserverClosureState::MergedVerified => Some("post_merge.verification_passed"),
        PostMergeObserverClosureState::FailedVerification => Some("post_merge.verification_failed"),
        PostMergeObserverClosureState::MergedPendingVerification => {
            Some("post_merge.verification_pending")
        }
        PostMergeObserverClosureState::ClosedUnmerged => None,
    }
}

fn verification_criterion_status(final_state: PostMergeObserverClosureState) -> CriterionStatus {
    match final_state {
        PostMergeObserverClosureState::MergedVerified => CriterionStatus::Verified,
        PostMergeObserverClosureState::FailedVerification => CriterionStatus::Failed,
        PostMergeObserverClosureState::MergedPendingVerification => CriterionStatus::Pending,
        PostMergeObserverClosureState::ClosedUnmerged => CriterionStatus::NotApplicable,
    }
}

fn verification_summary(
    final_state: PostMergeObserverClosureState,
    verification: &PostMergeVerificationObservation,
) -> &str {
    if let Some(summary) = verification.summary.as_deref() {
        return summary;
    }
    match final_state {
        PostMergeObserverClosureState::MergedVerified => "post-merge verification passed",
        PostMergeObserverClosureState::FailedVerification => "post-merge verification failed",
        PostMergeObserverClosureState::MergedPendingVerification => {
            "post-merge verification is pending"
        }
        PostMergeObserverClosureState::ClosedUnmerged => "verification is not applicable",
    }
}

fn provider_criterion_summary(final_state: PostMergeObserverClosureState) -> &'static str {
    match final_state {
        PostMergeObserverClosureState::MergedVerified
        | PostMergeObserverClosureState::FailedVerification
        | PostMergeObserverClosureState::MergedPendingVerification => {
            "provider reported the pull request merged"
        }
        PostMergeObserverClosureState::ClosedUnmerged => {
            "provider reported the pull request closed without merge"
        }
    }
}

fn human_gate_summary(final_state: PostMergeObserverClosureState) -> &'static str {
    match final_state {
        PostMergeObserverClosureState::ClosedUnmerged => {
            "the observer records external human closure without merging"
        }
        PostMergeObserverClosureState::MergedVerified
        | PostMergeObserverClosureState::FailedVerification
        | PostMergeObserverClosureState::MergedPendingVerification => {
            "the observer records an external human merge gate"
        }
    }
}

fn seal_disposition(final_state: PostMergeObserverClosureState) -> ClosureDisposition {
    match final_state {
        PostMergeObserverClosureState::MergedVerified
        | PostMergeObserverClosureState::ClosedUnmerged => ClosureDisposition::Closed,
        PostMergeObserverClosureState::FailedVerification => ClosureDisposition::Failed,
        PostMergeObserverClosureState::MergedPendingVerification => ClosureDisposition::Deferred,
    }
}

fn closure_reason(final_state: PostMergeObserverClosureState) -> &'static str {
    match final_state {
        PostMergeObserverClosureState::MergedVerified => "merged_verified",
        PostMergeObserverClosureState::FailedVerification => "failed_verification",
        PostMergeObserverClosureState::MergedPendingVerification => "merged_pending_verification",
        PostMergeObserverClosureState::ClosedUnmerged => "closed_unmerged",
    }
}

fn closure_summary(final_state: PostMergeObserverClosureState) -> &'static str {
    match final_state {
        PostMergeObserverClosureState::MergedVerified => {
            "merged PR was observed and post-merge verification passed"
        }
        PostMergeObserverClosureState::FailedVerification => {
            "merged PR was observed but post-merge verification failed"
        }
        PostMergeObserverClosureState::MergedPendingVerification => {
            "merged PR was observed and post-merge verification is pending"
        }
        PostMergeObserverClosureState::ClosedUnmerged => {
            "target PR was closed without merge; source issue remains unresolved"
        }
    }
}

fn provider_name(provider: PostMergeProvider) -> &'static str {
    match provider {
        PostMergeProvider::Github => "github",
    }
}

fn pull_request_state_name(state: PostMergePullRequestState) -> &'static str {
    match state {
        PostMergePullRequestState::Open => "open",
        PostMergePullRequestState::Closed => "closed",
    }
}

fn verification_status_name(status: PostMergeVerificationStatus) -> &'static str {
    match status {
        PostMergeVerificationStatus::Passed => "passed",
        PostMergeVerificationStatus::Failed => "failed",
        PostMergeVerificationStatus::Pending => "pending",
        PostMergeVerificationStatus::NotRequired => "not_required",
    }
}

fn closure_state_name(state: PostMergeObserverClosureState) -> &'static str {
    match state {
        PostMergeObserverClosureState::MergedVerified => "merged_verified",
        PostMergeObserverClosureState::FailedVerification => "failed_verification",
        PostMergeObserverClosureState::MergedPendingVerification => "merged_pending_verification",
        PostMergeObserverClosureState::ClosedUnmerged => "closed_unmerged",
    }
}

fn closure_state_from_reason(reason_code: &str) -> Option<PostMergeObserverClosureState> {
    match reason_code {
        "merged_verified" => Some(PostMergeObserverClosureState::MergedVerified),
        "failed_verification" => Some(PostMergeObserverClosureState::FailedVerification),
        "merged_pending_verification" => {
            Some(PostMergeObserverClosureState::MergedPendingVerification)
        }
        "closed_unmerged" => Some(PostMergeObserverClosureState::ClosedUnmerged),
        _ => None,
    }
}

fn require_receipt_criterion<'a>(
    receipt: &'a HarnessReceipt,
    criterion_id: &str,
) -> Result<&'a crate::SealCriterion, PostMergeObserverPlanError> {
    receipt
        .seal
        .criteria
        .iter()
        .find(|criterion| criterion.criterion_id == criterion_id)
        .ok_or_else(|| PostMergeObserverPlanError::MissingReceiptCriterion(criterion_id.to_owned()))
}

fn receipt_reference(receipt: &HarnessReceipt, reference_type: ReferenceType) -> Option<Reference> {
    receipt
        .seal
        .criteria
        .iter()
        .flat_map(|criterion| criterion.evidence_refs.iter())
        .chain(
            receipt
                .harness
                .acts
                .iter()
                .flat_map(|act| act.surface_refs.iter()),
        )
        .find(|reference| reference.reference_type == reference_type)
        .cloned()
}

fn post_merge_observer_receipt_id(plan: &PostMergeObserverPlan) -> String {
    let material = format!(
        "{}:{}",
        plan.idempotency.intent_key, plan.idempotency.content_hash
    );
    format!("hrn_rcpt_post_merge_{}", sha256_hex(&material))
}

fn act_form_name(form: &ActForm) -> &'static str {
    match form {
        ActForm::Revision => "revision",
        ActForm::Reply => "reply",
        ActForm::Review => "review",
        ActForm::Observation => "observation",
        ActForm::Verification => "verification",
    }
}

fn criterion_status_name(status: &CriterionStatus) -> &'static str {
    match *status {
        CriterionStatus::Verified => "verified",
        CriterionStatus::Failed => "failed",
        CriterionStatus::Pending => "pending",
        CriterionStatus::NotApplicable => "not_applicable",
        CriterionStatus::Unknown => "unknown",
    }
}

fn push_kv(material: &mut String, key: &str, value: &str) {
    material.push_str(key);
    material.push('=');
    material.push_str(value);
    material.push('\n');
}

fn sha256_prefixed(value: &str) -> String {
    format!("sha256:{}", sha256_hex(value))
}

fn sha256_hex(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

fn non_empty_string(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}
