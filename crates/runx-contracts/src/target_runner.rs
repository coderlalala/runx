//! Target repo runner planning contracts.
// rust-style-allow: large-file - target runner plan schemas and dedupe lookup helpers share one contract module.

use std::fmt;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    JsonNumber, JsonObject, JsonValue, OperationalPolicy, OperationalPolicyAction,
    OperationalPolicyAdmission, OperationalPolicyAdmissionRequest,
    OperationalPolicyAdmissionStatus, OperationalPolicyDedupeStrategy,
    OperationalPolicyDuplicateBehavior, OperationalPolicyError, OperationalPolicyPublishMode,
    OperationalPolicyRunnerKind, OperationalPolicyRunnerRule,
    OperationalPolicySourceIssueClosureMode, OperationalPolicySourceProvider,
    OperationalPolicySourceRule, OperationalPolicyTargetRule, Reference, ReferenceType,
    admit_operational_policy_request,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetRepoRunnerPlanRequest {
    pub source_id: Option<String>,
    pub target_repo: String,
    pub action: OperationalPolicyAction,
    pub runner_id: Option<String>,
    pub source: TargetRepoRunnerSourceContext,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_pull_request: Option<TargetRepoRunnerExistingPullRequest>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetRepoRunnerSourceContext {
    pub provider: OperationalPolicySourceProvider,
    pub locator: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_locator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_ts: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_url: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetRepoRunnerExistingPullRequest {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetRepoRunnerPlan {
    pub policy_id: String,
    pub action: OperationalPolicyAction,
    pub source: TargetRepoRunnerSourcePlan,
    pub source_thread: TargetRepoRunnerSourceThreadPlan,
    pub target: TargetRepoRunnerTargetPlan,
    pub runner: TargetRepoRunnerRunnerPlan,
    pub owner: TargetRepoRunnerOwnerPlan,
    pub dedupe: TargetRepoRunnerDedupePlan,
    pub source_issue_closure_mode: OperationalPolicySourceIssueClosureMode,
    pub mutate_target_repo: bool,
    pub require_human_merge_gate: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetRepoRunnerSourcePlan {
    pub source_id: String,
    pub provider: OperationalPolicySourceProvider,
    pub locator: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_url: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetRepoRunnerSourceThreadPlan {
    pub required: bool,
    pub publish_mode: OperationalPolicyPublishMode,
    pub locator: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetRepoRunnerTargetPlan {
    pub repo: String,
    pub scafld_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetRepoRunnerRunnerPlan {
    pub runner_id: String,
    pub kind: OperationalPolicyRunnerKind,
    pub scafld_required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetRepoRunnerOwnerPlan {
    pub route_id: String,
    pub owners: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetRepoRunnerDedupePlan {
    pub strategy: OperationalPolicyDedupeStrategy,
    pub key: String,
    pub key_fields: Vec<String>,
    pub components: Vec<TargetRepoRunnerDedupeComponent>,
    pub on_duplicate: OperationalPolicyDuplicateBehavior,
    pub result: TargetRepoRunnerDedupeResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_pull_request: Option<TargetRepoRunnerExistingPullRequest>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetRepoRunnerDedupeLookupPlan {
    pub provider: TargetRepoRunnerProvider,
    pub target_repo: String,
    pub key: String,
    pub strategy: OperationalPolicyDedupeStrategy,
    pub query: TargetRepoRunnerDedupeLookupQuery,
    pub components: Vec<TargetRepoRunnerDedupeComponent>,
    pub source_issue_ref: Option<Reference>,
    pub source_thread_ref: Reference,
    pub result: TargetRepoRunnerDedupeResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_pull_request: Option<TargetRepoRunnerExistingPullRequest>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetRepoRunnerDedupeLookupQuery {
    pub markers: Vec<String>,
    pub required_refs: Vec<Reference>,
    pub result_limit: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetRepoRunnerExecutionPlan {
    pub checkout: TargetRepoRunnerCheckoutPlan,
    pub readiness: TargetRepoRunnerReadinessPlan,
    pub provider_lookup: TargetRepoRunnerDedupeLookupPlan,
    pub source_issue_ref: Option<Reference>,
    pub source_thread_ref: Reference,
    pub target_repo_ref: Reference,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetRepoRunnerCheckoutPlan {
    pub target_repo: String,
    pub public_repo_ref: Reference,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
    pub scafld_required: bool,
    pub local_path_hidden: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetRepoRunnerReadinessPlan {
    pub runner_id: String,
    pub runner_kind: OperationalPolicyRunnerKind,
    pub runner_scafld_required: bool,
    pub target_scafld_required: bool,
    pub scafld_ready: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetRepoRunnerReadinessObservation {
    pub target_repo: String,
    pub runner_id: String,
    pub scafld_ready: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetRepoRunnerProviderPullRequest {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub open: bool,
    #[serde(default)]
    pub markers: Vec<String>,
    #[serde(default)]
    pub refs: Vec<Reference>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetRepoRunnerDedupeLookupObservation {
    pub provider: TargetRepoRunnerProvider,
    pub target_repo: String,
    pub key: String,
    #[serde(default)]
    pub pull_requests: Vec<TargetRepoRunnerProviderPullRequest>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetRepoRunnerDedupeLookupExecution {
    pub provider: TargetRepoRunnerProvider,
    pub target_repo: String,
    pub key: String,
    pub result: TargetRepoRunnerDedupeResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_pull_request: Option<TargetRepoRunnerExistingPullRequest>,
    pub matched_required_refs: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetRepoRunnerPullRequestDisposition {
    Create,
    Reuse,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TargetRepoRunnerPullRequestReceiptPlan {
    pub act_form: crate::ActForm,
    pub disposition: TargetRepoRunnerPullRequestDisposition,
    pub target_repo_ref: Reference,
    pub source_issue_ref: Option<Reference>,
    pub source_thread_ref: Reference,
    pub pull_request_ref: Option<Reference>,
    pub metadata: JsonObject,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TargetRepoRunnerSourcePublicationReceiptPlan {
    pub source_issue_ref: Option<Reference>,
    pub source_thread_ref: Reference,
    pub pull_request_ref: Reference,
    pub metadata: JsonObject,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetRepoRunnerProvider {
    Github,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetRepoRunnerDedupeComponent {
    pub field: String,
    pub value: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetRepoRunnerDedupeResult {
    LookupRequired,
    Reused,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TargetRepoRunnerPlanError {
    Policy(OperationalPolicyError),
    AdmissionDenied(Box<OperationalPolicyAdmission>),
    MissingDedupeField(String),
    InconsistentAdmission(String),
    NotScafldReady { target_repo: String },
    ReadinessMismatch(String),
    ProviderLookupMismatch(String),
    PullRequestRequired,
}

struct TargetRepoRunnerAdmissionValues {
    source_id: String,
    target_repo: String,
    runner_id: String,
    owner_route_id: String,
    thread_locator: String,
}

struct TargetRepoRunnerPolicyContext<'a> {
    source: &'a OperationalPolicySourceRule,
    target: &'a OperationalPolicyTargetRule,
    runner: &'a OperationalPolicyRunnerRule,
}

impl fmt::Display for TargetRepoRunnerPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Policy(error) => write!(formatter, "operational policy error: {error}"),
            Self::AdmissionDenied(admission) => {
                let codes = admission
                    .findings
                    .iter()
                    .map(|finding| finding.code.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(formatter, "target repo runner admission denied: {codes}")
            }
            Self::MissingDedupeField(field) => {
                write!(
                    formatter,
                    "target repo runner dedupe field '{field}' is missing"
                )
            }
            Self::InconsistentAdmission(message) => formatter.write_str(message),
            Self::NotScafldReady { target_repo } => {
                write!(
                    formatter,
                    "target repo runner requires scafld-ready target repo '{target_repo}'"
                )
            }
            Self::ReadinessMismatch(message) | Self::ProviderLookupMismatch(message) => {
                formatter.write_str(message)
            }
            Self::PullRequestRequired => {
                formatter.write_str("target repo runner receipt planning requires a pull request")
            }
        }
    }
}

impl std::error::Error for TargetRepoRunnerPlanError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Policy(error) => Some(error),
            Self::AdmissionDenied(_)
            | Self::MissingDedupeField(_)
            | Self::InconsistentAdmission(_)
            | Self::NotScafldReady { .. }
            | Self::ReadinessMismatch(_)
            | Self::ProviderLookupMismatch(_)
            | Self::PullRequestRequired => None,
        }
    }
}

pub fn plan_target_repo_runner(
    policy: &OperationalPolicy,
    request: &TargetRepoRunnerPlanRequest,
) -> Result<TargetRepoRunnerPlan, TargetRepoRunnerPlanError> {
    let admission = allowed_runner_admission(policy, request)?;
    let values = runner_admission_values(&admission, request)?;
    let context = runner_policy_context(policy, &values)?;

    Ok(TargetRepoRunnerPlan {
        policy_id: policy.policy_id.clone(),
        action: request.action,
        source: TargetRepoRunnerSourcePlan {
            source_id: values.source_id,
            provider: request.source.provider,
            locator: request.source.locator.clone(),
            issue_url: request.source.issue_url.clone(),
        },
        source_thread: TargetRepoRunnerSourceThreadPlan {
            required: admission.source_thread_required,
            publish_mode: context.source.source_thread.publish_mode,
            locator: values.thread_locator,
        },
        target: TargetRepoRunnerTargetPlan {
            repo: values.target_repo,
            scafld_required: context.target.scafld_required,
            base_branch: context.target.base_branch.clone(),
        },
        runner: TargetRepoRunnerRunnerPlan {
            runner_id: values.runner_id,
            kind: context.runner.kind,
            scafld_required: context.runner.scafld_required,
        },
        owner: TargetRepoRunnerOwnerPlan {
            route_id: values.owner_route_id,
            owners: admission.owners.unwrap_or_default(),
        },
        dedupe: build_dedupe_plan(policy, request, &context.target.repo)?,
        source_issue_closure_mode: admission.source_issue_closure_mode,
        mutate_target_repo: admission.mutate_target_repo,
        require_human_merge_gate: admission.require_human_merge_gate,
    })
}

fn allowed_runner_admission(
    policy: &OperationalPolicy,
    request: &TargetRepoRunnerPlanRequest,
) -> Result<OperationalPolicyAdmission, TargetRepoRunnerPlanError> {
    let admission = admit_operational_policy_request(
        policy,
        &OperationalPolicyAdmissionRequest {
            source_id: request.source_id.clone(),
            target_repo: Some(request.target_repo.clone()),
            action: request.action,
            runner_id: request.runner_id.clone(),
            source_thread_locator: request.source.thread_locator.clone(),
        },
    )
    .map_err(TargetRepoRunnerPlanError::Policy)?;
    if admission.status == OperationalPolicyAdmissionStatus::Allow {
        Ok(admission)
    } else {
        Err(TargetRepoRunnerPlanError::AdmissionDenied(Box::new(
            admission,
        )))
    }
}

fn runner_admission_values(
    admission: &OperationalPolicyAdmission,
    request: &TargetRepoRunnerPlanRequest,
) -> Result<TargetRepoRunnerAdmissionValues, TargetRepoRunnerPlanError> {
    Ok(TargetRepoRunnerAdmissionValues {
        source_id: required_admission_value(&admission.source_id, "source_id")?,
        target_repo: required_admission_value(&admission.target_repo, "target_repo")?,
        runner_id: required_admission_value(&admission.runner_id, "runner_id")?,
        owner_route_id: required_admission_value(&admission.owner_route_id, "owner_route_id")?,
        thread_locator: required_plan_value(
            &request.source.thread_locator,
            "source_thread.locator",
        )?,
    })
}

fn runner_policy_context<'a>(
    policy: &'a OperationalPolicy,
    values: &TargetRepoRunnerAdmissionValues,
) -> Result<TargetRepoRunnerPolicyContext<'a>, TargetRepoRunnerPlanError> {
    Ok(TargetRepoRunnerPolicyContext {
        source: admitted_source(policy, &values.source_id)?,
        target: admitted_target(policy, &values.target_repo)?,
        runner: admitted_runner(policy, &values.runner_id)?,
    })
}

fn admitted_target<'a>(
    policy: &'a OperationalPolicy,
    target_repo: &str,
) -> Result<&'a OperationalPolicyTargetRule, TargetRepoRunnerPlanError> {
    policy
        .targets
        .iter()
        .find(|candidate| candidate.repo == target_repo)
        .ok_or_else(|| {
            TargetRepoRunnerPlanError::InconsistentAdmission(format!(
                "admission allowed unknown target repo '{target_repo}'"
            ))
        })
}

fn admitted_runner<'a>(
    policy: &'a OperationalPolicy,
    runner_id: &str,
) -> Result<&'a OperationalPolicyRunnerRule, TargetRepoRunnerPlanError> {
    policy
        .runners
        .iter()
        .find(|candidate| candidate.runner_id == runner_id)
        .ok_or_else(|| {
            TargetRepoRunnerPlanError::InconsistentAdmission(format!(
                "admission allowed unknown runner '{runner_id}'"
            ))
        })
}

fn admitted_source<'a>(
    policy: &'a OperationalPolicy,
    source_id: &str,
) -> Result<&'a OperationalPolicySourceRule, TargetRepoRunnerPlanError> {
    policy
        .sources
        .iter()
        .find(|candidate| candidate.source_id == source_id)
        .ok_or_else(|| {
            TargetRepoRunnerPlanError::InconsistentAdmission(format!(
                "admission allowed unknown source '{source_id}'"
            ))
        })
}

pub fn plan_target_repo_runner_dedupe_lookup(
    plan: &TargetRepoRunnerPlan,
) -> TargetRepoRunnerDedupeLookupPlan {
    let source_issue_ref = plan.source.issue_url.as_ref().map(|issue_url| Reference {
        reference_type: ReferenceType::GithubIssue,
        uri: issue_url.clone(),
        provider: Some(plan.source.provider.to_string()),
        locator: Some(plan.source.locator.clone()),
        label: Some("source issue".to_owned()),
        observed_at: None,
    });
    let source_thread_ref = Reference {
        reference_type: source_thread_reference_type(plan.source.provider),
        uri: plan.source_thread.locator.clone(),
        provider: Some(plan.source.provider.to_string()),
        locator: Some(plan.source_thread.locator.clone()),
        label: Some("source thread".to_owned()),
        observed_at: None,
    };
    TargetRepoRunnerDedupeLookupPlan {
        provider: TargetRepoRunnerProvider::Github,
        target_repo: plan.target.repo.clone(),
        key: plan.dedupe.key.clone(),
        strategy: plan.dedupe.strategy,
        query: TargetRepoRunnerDedupeLookupQuery {
            markers: dedupe_lookup_markers(&plan.dedupe),
            required_refs: [source_issue_ref.clone(), Some(source_thread_ref.clone())]
                .into_iter()
                .flatten()
                .collect(),
            result_limit: 20,
        },
        components: plan.dedupe.components.clone(),
        source_issue_ref,
        source_thread_ref,
        result: plan.dedupe.result,
        existing_pull_request: plan.dedupe.existing_pull_request.clone(),
    }
}

pub fn plan_target_repo_runner_execution(
    plan: &TargetRepoRunnerPlan,
    readiness: &TargetRepoRunnerReadinessObservation,
) -> Result<TargetRepoRunnerExecutionPlan, TargetRepoRunnerPlanError> {
    if readiness.target_repo != plan.target.repo {
        return Err(TargetRepoRunnerPlanError::ReadinessMismatch(format!(
            "readiness target '{}' does not match plan target '{}'",
            readiness.target_repo, plan.target.repo
        )));
    }
    if readiness.runner_id != plan.runner.runner_id {
        return Err(TargetRepoRunnerPlanError::ReadinessMismatch(format!(
            "readiness runner '{}' does not match plan runner '{}'",
            readiness.runner_id, plan.runner.runner_id
        )));
    }
    if (plan.target.scafld_required || plan.runner.scafld_required) && !readiness.scafld_ready {
        return Err(TargetRepoRunnerPlanError::NotScafldReady {
            target_repo: plan.target.repo.clone(),
        });
    }

    let provider_lookup = plan_target_repo_runner_dedupe_lookup(plan);
    Ok(TargetRepoRunnerExecutionPlan {
        checkout: TargetRepoRunnerCheckoutPlan {
            target_repo: plan.target.repo.clone(),
            public_repo_ref: target_repo_ref(&plan.target.repo),
            base_branch: plan.target.base_branch.clone(),
            scafld_required: plan.target.scafld_required,
            local_path_hidden: true,
        },
        readiness: TargetRepoRunnerReadinessPlan {
            runner_id: plan.runner.runner_id.clone(),
            runner_kind: plan.runner.kind,
            runner_scafld_required: plan.runner.scafld_required,
            target_scafld_required: plan.target.scafld_required,
            scafld_ready: readiness.scafld_ready,
        },
        source_issue_ref: provider_lookup.source_issue_ref.clone(),
        source_thread_ref: provider_lookup.source_thread_ref.clone(),
        target_repo_ref: target_repo_ref(&plan.target.repo),
        provider_lookup,
    })
}

pub fn execute_target_repo_runner_dedupe_lookup(
    lookup: &TargetRepoRunnerDedupeLookupPlan,
    observation: &TargetRepoRunnerDedupeLookupObservation,
) -> Result<TargetRepoRunnerDedupeLookupExecution, TargetRepoRunnerPlanError> {
    if observation.provider != lookup.provider {
        return Err(TargetRepoRunnerPlanError::ProviderLookupMismatch(
            "provider lookup observation provider does not match plan".to_owned(),
        ));
    }
    if observation.target_repo != lookup.target_repo {
        return Err(TargetRepoRunnerPlanError::ProviderLookupMismatch(
            "provider lookup observation target repo does not match plan".to_owned(),
        ));
    }
    if observation.key != lookup.key {
        return Err(TargetRepoRunnerPlanError::ProviderLookupMismatch(
            "provider lookup observation dedupe key does not match plan".to_owned(),
        ));
    }

    let existing_pull_request = observation
        .pull_requests
        .iter()
        .find(|pull_request| {
            pull_request.open
                && lookup.query.markers.iter().all(|marker| {
                    pull_request
                        .markers
                        .iter()
                        .any(|candidate| candidate == marker)
                })
                && lookup.query.required_refs.iter().all(|required| {
                    pull_request
                        .refs
                        .iter()
                        .any(|candidate| same_reference(candidate, required))
                })
        })
        .map(|pull_request| TargetRepoRunnerExistingPullRequest {
            url: pull_request.url.clone(),
            number: pull_request.number,
            branch: pull_request.branch.clone(),
        });

    let matched_required_refs = existing_pull_request.is_some();

    Ok(TargetRepoRunnerDedupeLookupExecution {
        provider: lookup.provider,
        target_repo: lookup.target_repo.clone(),
        key: lookup.key.clone(),
        result: if existing_pull_request.is_some() {
            TargetRepoRunnerDedupeResult::Reused
        } else {
            TargetRepoRunnerDedupeResult::LookupRequired
        },
        existing_pull_request,
        matched_required_refs,
    })
}

pub fn apply_target_repo_runner_dedupe_lookup_execution(
    plan: &TargetRepoRunnerPlan,
    execution: &TargetRepoRunnerDedupeLookupExecution,
) -> Result<TargetRepoRunnerPlan, TargetRepoRunnerPlanError> {
    if execution.target_repo != plan.target.repo || execution.key != plan.dedupe.key {
        return Err(TargetRepoRunnerPlanError::ProviderLookupMismatch(
            "provider lookup execution does not match target runner plan".to_owned(),
        ));
    }
    let mut plan = plan.clone();
    plan.dedupe.result = execution.result;
    plan.dedupe.existing_pull_request = execution.existing_pull_request.clone();
    Ok(plan)
}

pub fn plan_target_repo_runner_pull_request_receipt(
    plan: &TargetRepoRunnerPlan,
    pull_request: Option<&TargetRepoRunnerExistingPullRequest>,
) -> Result<TargetRepoRunnerPullRequestReceiptPlan, TargetRepoRunnerPlanError> {
    let disposition = if plan.dedupe.result == TargetRepoRunnerDedupeResult::Reused {
        TargetRepoRunnerPullRequestDisposition::Reuse
    } else {
        TargetRepoRunnerPullRequestDisposition::Create
    };
    let pull_request = pull_request
        .or(plan.dedupe.existing_pull_request.as_ref())
        .ok_or(TargetRepoRunnerPlanError::PullRequestRequired)?;
    let pull_request_ref = pull_request_ref(&plan.target.repo, pull_request);
    let source_issue_ref = source_issue_ref(plan);
    let source_thread_ref = source_thread_ref(plan);
    Ok(TargetRepoRunnerPullRequestReceiptPlan {
        act_form: crate::ActForm::Revision,
        disposition,
        target_repo_ref: target_repo_ref(&plan.target.repo),
        source_issue_ref: source_issue_ref.clone(),
        source_thread_ref: source_thread_ref.clone(),
        pull_request_ref: Some(pull_request_ref.clone()),
        metadata: pull_request_receipt_metadata(
            plan,
            disposition,
            pull_request,
            source_issue_ref.as_ref(),
            &source_thread_ref,
        ),
    })
}

pub fn plan_target_repo_runner_source_publication_receipt(
    plan: &TargetRepoRunnerPlan,
    pull_request: &TargetRepoRunnerExistingPullRequest,
) -> TargetRepoRunnerSourcePublicationReceiptPlan {
    let pull_request_ref = pull_request_ref(&plan.target.repo, pull_request);
    let source_issue_ref = source_issue_ref(plan);
    let source_thread_ref = source_thread_ref(plan);
    TargetRepoRunnerSourcePublicationReceiptPlan {
        source_issue_ref: source_issue_ref.clone(),
        source_thread_ref: source_thread_ref.clone(),
        pull_request_ref: pull_request_ref.clone(),
        metadata: source_publication_receipt_metadata(
            plan,
            &pull_request_ref,
            source_issue_ref.as_ref(),
            &source_thread_ref,
        ),
    }
}

fn build_dedupe_plan(
    policy: &OperationalPolicy,
    request: &TargetRepoRunnerPlanRequest,
    target_repo: &str,
) -> Result<TargetRepoRunnerDedupePlan, TargetRepoRunnerPlanError> {
    let key_fields = target_scoped_key_fields(&policy.dedupe.key_fields);
    let mut components = Vec::with_capacity(key_fields.len());
    for field in &key_fields {
        let value = dedupe_field_value(request, target_repo, field)
            .ok_or_else(|| TargetRepoRunnerPlanError::MissingDedupeField(field.clone()))?;
        components.push(TargetRepoRunnerDedupeComponent {
            field: field.clone(),
            value,
        });
    }

    Ok(TargetRepoRunnerDedupePlan {
        strategy: policy.dedupe.strategy,
        key: dedupe_key(policy.dedupe.strategy, &components),
        key_fields,
        components,
        on_duplicate: policy.dedupe.on_duplicate,
        result: if request.existing_pull_request.is_some() {
            TargetRepoRunnerDedupeResult::Reused
        } else {
            TargetRepoRunnerDedupeResult::LookupRequired
        },
        existing_pull_request: request.existing_pull_request.clone(),
    })
}

fn dedupe_lookup_markers(dedupe: &TargetRepoRunnerDedupePlan) -> Vec<String> {
    let mut markers = Vec::with_capacity(dedupe.components.len() + 1);
    markers.push(format!("runx-dedupe-key:{}", dedupe.key));
    for component in &dedupe.components {
        markers.push(format!(
            "runx-dedupe:{}={}",
            component.field, component.value
        ));
    }
    markers
}

fn source_thread_reference_type(provider: OperationalPolicySourceProvider) -> ReferenceType {
    match provider {
        OperationalPolicySourceProvider::Slack => ReferenceType::SlackThread,
        OperationalPolicySourceProvider::Github => ReferenceType::GithubIssue,
        OperationalPolicySourceProvider::Sentry => ReferenceType::SentryEvent,
        OperationalPolicySourceProvider::File
        | OperationalPolicySourceProvider::Api
        | OperationalPolicySourceProvider::Other => ReferenceType::ExternalUrl,
    }
}

fn source_issue_ref(plan: &TargetRepoRunnerPlan) -> Option<Reference> {
    plan.source.issue_url.as_ref().map(|issue_url| Reference {
        reference_type: ReferenceType::GithubIssue,
        uri: issue_url.clone(),
        provider: Some(plan.source.provider.to_string()),
        locator: Some(plan.source.locator.clone()),
        label: Some("source issue".to_owned()),
        observed_at: None,
    })
}

fn source_thread_ref(plan: &TargetRepoRunnerPlan) -> Reference {
    Reference {
        reference_type: source_thread_reference_type(plan.source.provider),
        uri: plan.source_thread.locator.clone(),
        provider: Some(plan.source.provider.to_string()),
        locator: Some(plan.source_thread.locator.clone()),
        label: Some("source thread".to_owned()),
        observed_at: None,
    }
}

fn target_repo_ref(repo: &str) -> Reference {
    Reference {
        reference_type: ReferenceType::GithubRepo,
        uri: format!("https://github.com/{repo}"),
        provider: Some("github".to_owned()),
        locator: Some(repo.to_owned()),
        label: Some("target repo".to_owned()),
        observed_at: None,
    }
}

fn pull_request_ref(repo: &str, pull_request: &TargetRepoRunnerExistingPullRequest) -> Reference {
    Reference {
        reference_type: ReferenceType::GithubPullRequest,
        uri: pull_request.url.clone(),
        provider: Some("github".to_owned()),
        locator: pull_request.number.map(|number| format!("{repo}#{number}")),
        label: Some("target pull request".to_owned()),
        observed_at: None,
    }
}

fn same_reference(left: &Reference, right: &Reference) -> bool {
    left.reference_type == right.reference_type && left.uri == right.uri
}

fn pull_request_receipt_metadata(
    plan: &TargetRepoRunnerPlan,
    disposition: TargetRepoRunnerPullRequestDisposition,
    pull_request: &TargetRepoRunnerExistingPullRequest,
    source_issue_ref: Option<&Reference>,
    source_thread_ref: &Reference,
) -> JsonObject {
    let mut metadata = JsonObject::new();
    metadata.insert("target_repo".to_owned(), string(plan.target.repo.clone()));
    metadata.insert(
        "pull_request".to_owned(),
        JsonValue::Object(pull_request_metadata(pull_request)),
    );
    metadata.insert(
        "dedupe".to_owned(),
        JsonValue::Object(dedupe_receipt_metadata(&plan.dedupe, disposition)),
    );
    metadata.insert(
        "disposition".to_owned(),
        static_string(match disposition {
            TargetRepoRunnerPullRequestDisposition::Create => "created",
            TargetRepoRunnerPullRequestDisposition::Reuse => "reused",
        }),
    );
    metadata.insert(
        "source".to_owned(),
        JsonValue::Object(source_metadata(source_issue_ref, source_thread_ref)),
    );
    metadata
}

fn source_publication_receipt_metadata(
    plan: &TargetRepoRunnerPlan,
    pull_request_ref: &Reference,
    source_issue_ref: Option<&Reference>,
    source_thread_ref: &Reference,
) -> JsonObject {
    let mut metadata = JsonObject::new();
    metadata.insert("target_repo".to_owned(), string(plan.target.repo.clone()));
    metadata.insert(
        "target_pull_request_url".to_owned(),
        string(pull_request_ref.uri.clone()),
    );
    metadata.insert(
        "source".to_owned(),
        JsonValue::Object(source_metadata(source_issue_ref, source_thread_ref)),
    );
    metadata
}

fn dedupe_receipt_metadata(
    dedupe: &TargetRepoRunnerDedupePlan,
    disposition: TargetRepoRunnerPullRequestDisposition,
) -> JsonObject {
    let mut metadata = JsonObject::new();
    metadata.insert(
        "strategy".to_owned(),
        string(dedupe_strategy_name(dedupe.strategy).to_owned()),
    );
    metadata.insert("key".to_owned(), string(dedupe.key.clone()));
    metadata.insert(
        "result".to_owned(),
        static_string(match dedupe.result {
            TargetRepoRunnerDedupeResult::LookupRequired => match disposition {
                TargetRepoRunnerPullRequestDisposition::Create => "created",
                TargetRepoRunnerPullRequestDisposition::Reuse => "reused",
            },
            TargetRepoRunnerDedupeResult::Reused => "reused",
        }),
    );
    metadata
}

fn pull_request_metadata(pull_request: &TargetRepoRunnerExistingPullRequest) -> JsonObject {
    let mut metadata = JsonObject::new();
    metadata.insert("url".to_owned(), string(pull_request.url.clone()));
    if let Some(number) = pull_request.number {
        metadata.insert(
            "number".to_owned(),
            JsonValue::Number(JsonNumber::U64(number)),
        );
    }
    if let Some(branch) = &pull_request.branch {
        metadata.insert("branch".to_owned(), string(branch.clone()));
    }
    metadata
}

fn source_metadata(
    source_issue_ref: Option<&Reference>,
    source_thread_ref: &Reference,
) -> JsonObject {
    let mut metadata = JsonObject::new();
    if let Some(source_issue_ref) = source_issue_ref {
        metadata.insert("issue_url".to_owned(), string(source_issue_ref.uri.clone()));
    }
    metadata.insert(
        "thread_uri".to_owned(),
        string(source_thread_ref.uri.clone()),
    );
    metadata
}

fn string(value: String) -> JsonValue {
    JsonValue::String(value)
}

fn static_string(value: &'static str) -> JsonValue {
    JsonValue::String(value.to_owned())
}

fn target_scoped_key_fields(configured: &[String]) -> Vec<String> {
    let mut fields = Vec::with_capacity(configured.len() + 1);
    for field in configured {
        if !fields.iter().any(|existing| existing == field) {
            fields.push(field.clone());
        }
    }
    if !fields
        .iter()
        .any(|field| field == "target_repo" || field == "target.repo")
    {
        fields.push("target_repo".to_owned());
    }
    fields
}

fn dedupe_field_value(
    request: &TargetRepoRunnerPlanRequest,
    target_repo: &str,
    field: &str,
) -> Option<String> {
    match field {
        "source.provider" => Some(request.source.provider.to_string()),
        "source.locator" | "source_locator" => {
            non_empty_owned(&Some(request.source.locator.clone()))
        }
        "source.thread_locator" | "source_thread.locator" => {
            non_empty_owned(&request.source.thread_locator)
        }
        "source.thread_ts" => non_empty_owned(&request.source.thread_ts),
        "source.issue_url" | "source_issue.url" => non_empty_owned(&request.source.issue_url),
        "signal.fingerprint" | "fingerprint" => non_empty_owned(&request.signal_fingerprint),
        "target_repo" | "target.repo" => Some(target_repo.to_owned()),
        _ => None,
    }
}

fn dedupe_key(
    strategy: OperationalPolicyDedupeStrategy,
    components: &[TargetRepoRunnerDedupeComponent],
) -> String {
    let mut material = format!("strategy={}\n", dedupe_strategy_name(strategy));
    for component in components {
        material.push_str("field=");
        material.push_str(&component.field);
        material.push('\0');
        material.push_str("value=");
        material.push_str(&component.value);
        material.push('\n');
    }
    format!(
        "{}:{}",
        dedupe_strategy_name(strategy),
        sha256_hex(&material)
    )
}

fn dedupe_strategy_name(strategy: OperationalPolicyDedupeStrategy) -> &'static str {
    match strategy {
        OperationalPolicyDedupeStrategy::SourceFingerprint => "source_fingerprint",
        OperationalPolicyDedupeStrategy::ProviderSearch => "provider_search",
        OperationalPolicyDedupeStrategy::Branch => "branch",
    }
}

fn sha256_hex(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

fn required_admission_value(
    value: &Option<String>,
    field: &'static str,
) -> Result<String, TargetRepoRunnerPlanError> {
    required_plan_value(value, field).map_err(|_| {
        TargetRepoRunnerPlanError::InconsistentAdmission(format!(
            "admission allowed without {field}"
        ))
    })
}

fn required_plan_value(
    value: &Option<String>,
    field: &'static str,
) -> Result<String, TargetRepoRunnerPlanError> {
    non_empty_owned(value)
        .ok_or_else(|| TargetRepoRunnerPlanError::MissingDedupeField(field.to_owned()))
}

fn non_empty_owned(value: &Option<String>) -> Option<String> {
    let trimmed = value.as_deref()?.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}
