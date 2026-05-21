// rust-style-allow: large-file because target-runner execution currently keeps
// provider dedupe, governed runner observation, PR receipt sealing, and public
// projection in one Rust cutover slice; split after live provider wiring lands.
//! Runtime support for target-repo runner execution.

use std::fmt;

use serde::Deserialize;
use serde::Serialize;
use sha2::{Digest, Sha256};
use url::Url;

use runx_contracts::{
    Act, ActForm, Authority, AuthorityAttenuation, AuthoritySubsetProof, AuthoritySubsetResult,
    ChangePlan, ChangeRequest, Closure, ClosureDisposition, CriterionBinding, CriterionStatus,
    Decision, DecisionChoice, DecisionInputs, DecisionJustification, Harness, HarnessEnforcement,
    HarnessIdempotency, HarnessReceipt, HarnessReceiptSchema, HarnessRevision, HarnessSandbox,
    HarnessSeal, HarnessState, Intent, JsonNumber, JsonObject, JsonValue, ReceiptIssuer,
    ReceiptIssuerType, ReceiptVerificationSummary, Reference, ReferenceType, RevisionDetails,
    SealCriterion, SignatureAlgorithm, SuccessCriterion, TargetRepoRunnerDedupeLookupExecution,
    TargetRepoRunnerDedupeLookupObservation, TargetRepoRunnerDedupeLookupPlan,
    TargetRepoRunnerDedupeResult, TargetRepoRunnerExecutionPlan,
    TargetRepoRunnerExistingPullRequest, TargetRepoRunnerPlan, TargetRepoRunnerPlanError,
    TargetRepoRunnerProvider, TargetRepoRunnerProviderPullRequest,
    TargetRepoRunnerPullRequestDisposition, TargetRepoRunnerPullRequestReceiptPlan,
    TargetRepoRunnerReadinessObservation, TargetRepoRunnerSourcePublicationReceiptPlan,
    TargetSurface, Verification, VerificationCheck, VerificationStatus,
    apply_target_repo_runner_dedupe_lookup_execution, execute_target_repo_runner_dedupe_lookup,
    plan_target_repo_runner_execution, plan_target_repo_runner_pull_request_receipt,
    plan_target_repo_runner_source_publication_receipt,
};
use runx_receipts::{canonical_receipt_body_digest, validate_harness_receipt};

pub use crate::runtime_http::{
    HostedHttpError as TargetRepoRunnerHttpError, HostedHttpHeader as TargetRepoRunnerHttpHeader,
    HostedHttpRequest as TargetRepoRunnerHttpRequest,
    HostedHttpResponse as TargetRepoRunnerHttpResponse,
    HostedTransport as TargetRepoRunnerHttpTransport, HttpMethod as TargetRepoRunnerHttpMethod,
    ReqwestHttpTransport as TargetRepoRunnerDefaultHttpTransport,
};

#[derive(Clone, Debug, PartialEq)]
pub struct TargetRepoRunnerFixtureExecutionInput {
    pub plan: TargetRepoRunnerPlan,
    pub readiness: TargetRepoRunnerReadinessObservation,
    pub dedupe: TargetRepoRunnerDedupeLookupObservation,
    pub created_pull_request: Option<TargetRepoRunnerExistingPullRequest>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TargetRepoRunnerFixtureExecution {
    pub execution_plan: TargetRepoRunnerExecutionPlan,
    pub dedupe_execution: TargetRepoRunnerDedupeLookupExecution,
    pub deduped_plan: TargetRepoRunnerPlan,
    pub disposition: TargetRepoRunnerPullRequestDisposition,
    pub pull_request: TargetRepoRunnerExistingPullRequest,
    pub pull_request_receipt: TargetRepoRunnerPullRequestReceiptPlan,
    pub source_publication_receipt: TargetRepoRunnerSourcePublicationReceiptPlan,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TargetRepoRunnerLiveExecution {
    pub checkout_command: TargetRepoRunnerCheckoutCommand,
    pub readiness: TargetRepoRunnerReadinessObservation,
    pub provider_lookup_command: TargetRepoRunnerProviderDedupeLookupCommand,
    pub dedupe_observation: TargetRepoRunnerDedupeLookupObservation,
    pub runner_observation: Option<TargetRepoRunnerGovernedRunnerObservation>,
    pub execution: TargetRepoRunnerFixtureExecution,
    pub revision_receipt: HarnessReceipt,
    pub revision_projection: TargetRepoRunnerRevisionReceiptProjection,
    pub source_publication_request: TargetRepoRunnerSourcePublicationRequest,
    pub source_publication_observation: TargetRepoRunnerSourcePublicationObservation,
    pub source_publication_receipt: HarnessReceipt,
    pub source_publication_projection: TargetRepoRunnerSourcePublicationProjection,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TargetRepoRunnerGovernedRunnerInvocation {
    pub execution_plan: TargetRepoRunnerExecutionPlan,
    pub deduped_plan: TargetRepoRunnerPlan,
    pub disposition: TargetRepoRunnerPullRequestDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TargetRepoRunnerGovernedRunnerObservation {
    pub runner_id: String,
    pub target_repo: String,
    pub summary: String,
    pub revision_refs: Vec<Reference>,
    pub artifact_refs: Vec<Reference>,
    pub verification_refs: Vec<Reference>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetRepoRunnerCheckoutCommand {
    pub target_repo: String,
    pub public_repo_ref: Reference,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
    pub runner_id: String,
    pub runner_kind: runx_contracts::OperationalPolicyRunnerKind,
    pub target_scafld_required: bool,
    pub runner_scafld_required: bool,
    pub mutate_target_repo: bool,
    pub local_path_hidden: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetRepoRunnerProviderDedupeLookupCommand {
    pub provider: TargetRepoRunnerProvider,
    pub target_repo: String,
    pub repository: TargetRepoRunnerGithubRepository,
    pub dedupe_key: String,
    pub result_limit: u16,
    pub query: TargetRepoRunnerGithubPullRequestSearchCommand,
    pub markers: Vec<String>,
    pub required_refs: Vec<Reference>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetRepoRunnerGithubRepository {
    pub owner: String,
    pub name: String,
    pub full_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetRepoRunnerGithubPullRequestSearchCommand {
    pub repo: String,
    pub state: TargetRepoRunnerGithubPullRequestSearchState,
    pub query: String,
    pub terms: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct TargetRepoRunnerGithubApiClient<T = TargetRepoRunnerDefaultHttpTransport> {
    base_url: String,
    transport: T,
    token: Option<String>,
}

#[cfg(feature = "async-http")]
impl TargetRepoRunnerGithubApiClient<TargetRepoRunnerDefaultHttpTransport> {
    pub fn new(token: Option<String>) -> Result<Self, TargetRepoRunnerRuntimeError> {
        Self::with_transport(
            "https://api.github.com",
            TargetRepoRunnerDefaultHttpTransport::new().map_err(|error| {
                TargetRepoRunnerRuntimeError::CommandValidation {
                    operation: "provider_api_lookup",
                    message: error.to_string(),
                }
            })?,
            token,
        )
    }
}

impl<T: TargetRepoRunnerHttpTransport> TargetRepoRunnerGithubApiClient<T> {
    pub fn with_transport(
        base_url: impl AsRef<str>,
        transport: T,
        token: Option<String>,
    ) -> Result<Self, TargetRepoRunnerRuntimeError> {
        let base_url = strip_one_trailing_slash(base_url.as_ref());
        let url = Url::parse(&base_url).map_err(|error| {
            TargetRepoRunnerRuntimeError::CommandValidation {
                operation: "provider_api_lookup",
                message: format!("invalid github api base url: {error}"),
            }
        })?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(TargetRepoRunnerRuntimeError::CommandValidation {
                operation: "provider_api_lookup",
                message: "github api base url must use http or https".to_owned(),
            });
        }
        Ok(Self {
            base_url,
            transport,
            token: token.filter(|value| !value.trim().is_empty()),
        })
    }

    pub fn provider_dedupe_lookup(
        &self,
        command: &TargetRepoRunnerProviderDedupeLookupCommand,
    ) -> Result<TargetRepoRunnerDedupeLookupObservation, TargetRepoRunnerRuntimeError> {
        self.require_github_command(command)?;
        let url = self.github_search_url(command)?;
        let headers = self.github_headers();
        let response = self
            .transport
            .send(TargetRepoRunnerHttpRequest {
                method: TargetRepoRunnerHttpMethod::Get,
                url: url.to_string(),
                headers,
                body: None,
            })
            .map_err(|error| TargetRepoRunnerRuntimeError::CommandValidation {
                operation: "provider_api_lookup",
                message: error.to_string(),
            })?;
        if !(200..=299).contains(&response.status) {
            return Err(TargetRepoRunnerRuntimeError::CommandValidation {
                operation: "provider_api_lookup",
                message: format!("github search API returned HTTP {}", response.status),
            });
        }
        let payload: GithubIssueSearchResponse =
            serde_json::from_str(&response.body).map_err(|error| {
                TargetRepoRunnerRuntimeError::CommandValidation {
                    operation: "provider_api_lookup",
                    message: format!("github search API returned invalid JSON: {error}"),
                }
            })?;
        let pull_requests = payload
            .items
            .into_iter()
            .filter_map(|item| github_search_item_to_pull_request(command, item))
            .collect();
        target_repo_runner_provider_dedupe_observation_from_pull_requests(command, pull_requests)
    }

    fn require_github_command(
        &self,
        command: &TargetRepoRunnerProviderDedupeLookupCommand,
    ) -> Result<(), TargetRepoRunnerRuntimeError> {
        if command.provider == TargetRepoRunnerProvider::Github {
            return Ok(());
        }
        Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "provider_api_lookup",
            message: "github provider lookup client only supports github commands".to_owned(),
        })
    }

    fn github_search_url(
        &self,
        command: &TargetRepoRunnerProviderDedupeLookupCommand,
    ) -> Result<String, TargetRepoRunnerRuntimeError> {
        let mut url = Url::parse(&format!("{}/search/issues", self.base_url)).map_err(|error| {
            TargetRepoRunnerRuntimeError::CommandValidation {
                operation: "provider_api_lookup",
                message: format!("invalid github search url: {error}"),
            }
        })?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("q", &command.query.query);
            pairs.append_pair("per_page", &command.result_limit.to_string());
        }
        Ok(url.to_string())
    }

    fn github_headers(&self) -> Vec<TargetRepoRunnerHttpHeader> {
        let mut headers = vec![
            TargetRepoRunnerHttpHeader::new("accept", "application/vnd.github+json"),
            TargetRepoRunnerHttpHeader::new("user-agent", "runx-target-repo-runner"),
            TargetRepoRunnerHttpHeader::new("x-github-api-version", "2022-11-28"),
        ];
        if let Some(token) = &self.token {
            headers.push(TargetRepoRunnerHttpHeader::new(
                "authorization",
                format!("Bearer {token}"),
            ));
        }
        headers
    }
}

#[derive(Clone, Debug, Deserialize)]
struct GithubIssueSearchResponse {
    #[serde(default)]
    items: Vec<GithubIssueSearchItem>,
}

#[derive(Clone, Debug, Deserialize)]
struct GithubIssueSearchItem {
    html_url: String,
    #[serde(default)]
    number: Option<u64>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    pull_request: Option<GithubPullRequestMarker>,
}

#[derive(Clone, Debug, Deserialize)]
struct GithubPullRequestMarker {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetRepoRunnerGithubPullRequestSearchState {
    Open,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TargetRepoRunnerPullRequestObservationRequest {
    pub disposition: TargetRepoRunnerPullRequestDisposition,
    pub target_repo: String,
    pub dedupe_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_pull_request: Option<TargetRepoRunnerExistingPullRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_observation: Option<TargetRepoRunnerGovernedRunnerObservation>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TargetRepoRunnerSourcePublicationRequest {
    pub publication: TargetRepoRunnerSourcePublicationReceiptPlan,
    pub revision_receipt_ref: Reference,
    pub revision_projection: TargetRepoRunnerRevisionReceiptProjection,
    pub commands: Vec<TargetRepoRunnerSourcePublicationCommand>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TargetRepoRunnerSourcePublicationCommand {
    SourceIssueComment { target: Reference, body: String },
    SourceThreadReply { target: Reference, body: String },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TargetRepoRunnerSourcePublicationObservation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_issue_ref: Option<Reference>,
    pub source_thread_ref: Reference,
    pub pull_request_ref: Reference,
    pub revision_receipt_ref: Reference,
    pub published_refs: Vec<Reference>,
    pub metadata: JsonObject,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TargetRepoRunnerRevisionReceiptProjection {
    pub receipt_ref: Reference,
    pub act_id: String,
    pub disposition: TargetRepoRunnerPullRequestDisposition,
    pub target_repo_ref: Reference,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_issue_ref: Option<Reference>,
    pub source_thread_ref: Reference,
    pub pull_request_ref: Reference,
    pub summary: String,
    pub metadata: JsonObject,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TargetRepoRunnerSourcePublicationProjection {
    pub receipt_ref: Reference,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_issue_ref: Option<Reference>,
    pub source_thread_ref: Reference,
    pub pull_request_ref: Reference,
    pub published_refs: Vec<Reference>,
    pub summary: String,
    pub metadata: JsonObject,
}

pub fn target_repo_runner_checkout_command(
    plan: &TargetRepoRunnerPlan,
) -> Result<TargetRepoRunnerCheckoutCommand, TargetRepoRunnerRuntimeError> {
    let repository = github_repository(&plan.target.repo, "checkout")?;
    if repository.full_name != plan.target.repo {
        return Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "checkout",
            message: "target repo must be a canonical github owner/repo".to_owned(),
        });
    }
    if !plan.mutate_target_repo {
        return Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "checkout",
            message: "target repo mutation must be admitted before checkout".to_owned(),
        });
    }
    if !plan.require_human_merge_gate {
        return Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "checkout",
            message: "target runner requires a human merge gate".to_owned(),
        });
    }

    Ok(TargetRepoRunnerCheckoutCommand {
        target_repo: plan.target.repo.clone(),
        public_repo_ref: Reference {
            reference_type: ReferenceType::GithubRepo,
            uri: format!("https://github.com/{}", plan.target.repo),
            provider: Some("github".to_owned()),
            locator: Some(plan.target.repo.clone()),
            label: Some("target repo".to_owned()),
            observed_at: None,
            proof_kind: None,
        },
        base_branch: plan.target.base_branch.clone(),
        runner_id: plan.runner.runner_id.clone(),
        runner_kind: plan.runner.kind,
        target_scafld_required: plan.target.scafld_required,
        runner_scafld_required: plan.runner.scafld_required,
        mutate_target_repo: plan.mutate_target_repo,
        local_path_hidden: true,
    })
}

pub fn target_repo_runner_provider_dedupe_lookup_command(
    lookup: &TargetRepoRunnerDedupeLookupPlan,
) -> Result<TargetRepoRunnerProviderDedupeLookupCommand, TargetRepoRunnerRuntimeError> {
    let repository = github_repository(&lookup.target_repo, "provider_dedupe_lookup")?;
    validate_provider_dedupe_lookup(lookup)?;
    let terms = provider_dedupe_lookup_terms(&repository, lookup);
    let query = terms.join(" ");

    Ok(TargetRepoRunnerProviderDedupeLookupCommand {
        provider: lookup.provider,
        target_repo: lookup.target_repo.clone(),
        repository,
        dedupe_key: lookup.key.clone(),
        result_limit: lookup.query.result_limit,
        query: TargetRepoRunnerGithubPullRequestSearchCommand {
            repo: lookup.target_repo.clone(),
            state: TargetRepoRunnerGithubPullRequestSearchState::Open,
            query,
            terms,
        },
        markers: lookup.query.markers.clone(),
        required_refs: lookup.query.required_refs.clone(),
    })
}

fn validate_provider_dedupe_lookup(
    lookup: &TargetRepoRunnerDedupeLookupPlan,
) -> Result<(), TargetRepoRunnerRuntimeError> {
    if lookup.query.result_limit == 0 {
        return Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "provider_dedupe_lookup",
            message: "provider lookup result limit must be greater than zero".to_owned(),
        });
    }
    if lookup.query.markers.is_empty() {
        return Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "provider_dedupe_lookup",
            message: "provider lookup requires at least one dedupe marker".to_owned(),
        });
    }
    if lookup.query.required_refs.is_empty() {
        return Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "provider_dedupe_lookup",
            message: "provider lookup requires source references".to_owned(),
        });
    }
    for marker in &lookup.query.markers {
        validate_provider_lookup_term(marker, "marker")?;
    }
    for reference in &lookup.query.required_refs {
        validate_provider_lookup_term(&reference.uri, "source reference")?;
    }
    Ok(())
}

fn provider_dedupe_lookup_terms(
    repository: &TargetRepoRunnerGithubRepository,
    lookup: &TargetRepoRunnerDedupeLookupPlan,
) -> Vec<String> {
    let mut terms = vec![
        format!("repo:{}", repository.full_name),
        "is:pr".to_owned(),
        "is:open".to_owned(),
    ];
    terms.extend(
        lookup
            .query
            .markers
            .iter()
            .map(|marker| github_search_exact_term(marker)),
    );
    terms.extend(
        lookup
            .query
            .required_refs
            .iter()
            .map(|reference| github_search_exact_term(&reference.uri)),
    );
    terms
}

pub fn target_repo_runner_provider_dedupe_observation_from_pull_requests(
    command: &TargetRepoRunnerProviderDedupeLookupCommand,
    pull_requests: Vec<TargetRepoRunnerProviderPullRequest>,
) -> Result<TargetRepoRunnerDedupeLookupObservation, TargetRepoRunnerRuntimeError> {
    if pull_requests.len() > usize::from(command.result_limit) {
        return Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "provider_dedupe_lookup",
            message: "provider lookup readback exceeded the command result limit".to_owned(),
        });
    }
    Ok(TargetRepoRunnerDedupeLookupObservation {
        provider: command.provider,
        target_repo: command.target_repo.clone(),
        key: command.dedupe_key.clone(),
        pull_requests,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetRepoRunnerAdapterError {
    pub operation: &'static str,
    pub message: String,
}

impl TargetRepoRunnerAdapterError {
    pub fn new(operation: &'static str, message: impl Into<String>) -> Self {
        Self {
            operation,
            message: message.into(),
        }
    }
}

impl fmt::Display for TargetRepoRunnerAdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} failed: {}", self.operation, self.message)
    }
}

impl std::error::Error for TargetRepoRunnerAdapterError {}

pub trait TargetRepoRunnerAdapter {
    fn checkout_readiness(
        &mut self,
        command: &TargetRepoRunnerCheckoutCommand,
    ) -> Result<TargetRepoRunnerReadinessObservation, TargetRepoRunnerAdapterError>;

    fn provider_dedupe_lookup(
        &mut self,
        command: &TargetRepoRunnerProviderDedupeLookupCommand,
    ) -> Result<TargetRepoRunnerDedupeLookupObservation, TargetRepoRunnerAdapterError>;

    fn invoke_governed_runner(
        &mut self,
        invocation: &TargetRepoRunnerGovernedRunnerInvocation,
    ) -> Result<TargetRepoRunnerGovernedRunnerObservation, TargetRepoRunnerAdapterError>;

    fn observe_pull_request(
        &mut self,
        request: &TargetRepoRunnerPullRequestObservationRequest,
    ) -> Result<TargetRepoRunnerExistingPullRequest, TargetRepoRunnerAdapterError>;

    fn publish_source_update(
        &mut self,
        _request: &TargetRepoRunnerSourcePublicationRequest,
    ) -> Result<TargetRepoRunnerSourcePublicationObservation, TargetRepoRunnerAdapterError> {
        Err(TargetRepoRunnerAdapterError::new(
            "source_publication",
            "adapter does not implement source publication readback",
        ))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TargetRepoRunnerRuntimeError {
    Plan(TargetRepoRunnerPlanError),
    Adapter(TargetRepoRunnerAdapterError),
    CommandValidation {
        operation: &'static str,
        message: String,
    },
    Receipt(String),
    ReceiptProjection(String),
    SourcePublicationMismatch(String),
    ReadinessMismatch(String),
    CheckoutNotScafldReady {
        target_repo: String,
    },
    CreatedPullRequestRequired {
        target_repo: String,
    },
}

impl fmt::Display for TargetRepoRunnerRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Plan(error) => write!(formatter, "{error}"),
            Self::Adapter(error) => write!(formatter, "{error}"),
            Self::CommandValidation { operation, message } => {
                write!(
                    formatter,
                    "target repo runner {operation} command is invalid: {message}"
                )
            }
            Self::Receipt(message) => {
                write!(formatter, "target repo runner receipt failed: {message}")
            }
            Self::ReceiptProjection(message) => {
                write!(
                    formatter,
                    "target repo runner receipt projection failed: {message}"
                )
            }
            Self::SourcePublicationMismatch(message) => {
                write!(
                    formatter,
                    "target repo runner source publication failed: {message}"
                )
            }
            Self::ReadinessMismatch(message) => formatter.write_str(message),
            Self::CheckoutNotScafldReady { target_repo } => write!(
                formatter,
                "target repo runner fixture requires scafld-ready checkout for '{target_repo}'"
            ),
            Self::CreatedPullRequestRequired { target_repo } => write!(
                formatter,
                "target repo runner fixture needs a created pull request for '{target_repo}'"
            ),
        }
    }
}

impl std::error::Error for TargetRepoRunnerRuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Plan(error) => Some(error),
            Self::Adapter(error) => Some(error),
            Self::CommandValidation { .. }
            | Self::Receipt(_)
            | Self::ReceiptProjection(_)
            | Self::SourcePublicationMismatch(_)
            | Self::ReadinessMismatch(_)
            | Self::CheckoutNotScafldReady { .. }
            | Self::CreatedPullRequestRequired { .. } => None,
        }
    }
}

impl From<TargetRepoRunnerPlanError> for TargetRepoRunnerRuntimeError {
    fn from(error: TargetRepoRunnerPlanError) -> Self {
        Self::Plan(error)
    }
}

impl From<TargetRepoRunnerAdapterError> for TargetRepoRunnerRuntimeError {
    fn from(error: TargetRepoRunnerAdapterError) -> Self {
        Self::Adapter(error)
    }
}

// rust-style-allow: long-function because this is the live target-runner
// orchestration boundary: readiness, dedupe, mutation observation, revision
// seal, and source publication must stay visibly ordered.
pub fn execute_target_repo_runner_with_adapter<A: TargetRepoRunnerAdapter>(
    plan: &TargetRepoRunnerPlan,
    adapter: &mut A,
    created_at: &str,
) -> Result<TargetRepoRunnerLiveExecution, TargetRepoRunnerRuntimeError> {
    let checkout_command = target_repo_runner_checkout_command(plan)?;
    let readiness = adapter.checkout_readiness(&checkout_command)?;
    let execution_plan = plan_target_repo_runner_execution(plan, &readiness)?;
    let provider_lookup_command =
        target_repo_runner_provider_dedupe_lookup_command(&execution_plan.provider_lookup)?;
    let dedupe_observation = adapter.provider_dedupe_lookup(&provider_lookup_command)?;
    validate_provider_dedupe_lookup_observation(&provider_lookup_command, &dedupe_observation)?;
    let dedupe_execution = execute_target_repo_runner_dedupe_lookup(
        &execution_plan.provider_lookup,
        &dedupe_observation,
    )?;
    let deduped_plan = apply_target_repo_runner_dedupe_lookup_execution(plan, &dedupe_execution)?;
    let disposition = if dedupe_execution.result == TargetRepoRunnerDedupeResult::Reused {
        TargetRepoRunnerPullRequestDisposition::Reuse
    } else {
        TargetRepoRunnerPullRequestDisposition::Create
    };
    let runner_observation = if disposition == TargetRepoRunnerPullRequestDisposition::Create {
        Some(
            adapter.invoke_governed_runner(&TargetRepoRunnerGovernedRunnerInvocation {
                execution_plan: execution_plan.clone(),
                deduped_plan: deduped_plan.clone(),
                disposition,
            })?,
        )
    } else {
        None
    };
    let pull_request =
        adapter.observe_pull_request(&TargetRepoRunnerPullRequestObservationRequest {
            disposition,
            target_repo: execution_plan.checkout.target_repo.clone(),
            dedupe_key: execution_plan.provider_lookup.key.clone(),
            existing_pull_request: dedupe_execution.existing_pull_request.clone(),
            runner_observation: runner_observation.clone(),
        })?;

    let execution = execute_target_repo_runner_execution_fixture(
        plan,
        &execution_plan,
        &readiness,
        &dedupe_observation,
        Some(&pull_request),
    )?;
    let revision_receipt =
        target_repo_runner_revision_receipt(&execution, runner_observation.as_ref(), created_at)?;
    let revision_projection = project_target_repo_runner_revision_receipt(&revision_receipt)?;
    let source_publication_request = target_repo_runner_source_publication_request(
        &execution,
        &revision_receipt,
        &revision_projection,
    );
    let source_publication_observation =
        adapter.publish_source_update(&source_publication_request)?;
    let source_publication_receipt = target_repo_runner_source_publication_harness_receipt(
        &source_publication_request,
        &source_publication_observation,
        created_at,
    )?;
    let source_publication_projection =
        project_target_repo_runner_source_publication_receipt(&source_publication_receipt)?;

    Ok(TargetRepoRunnerLiveExecution {
        checkout_command,
        readiness,
        provider_lookup_command,
        dedupe_observation,
        runner_observation,
        execution,
        revision_receipt,
        revision_projection,
        source_publication_request,
        source_publication_observation,
        source_publication_receipt,
        source_publication_projection,
    })
}

pub fn execute_target_repo_runner_fixture(
    input: TargetRepoRunnerFixtureExecutionInput,
) -> Result<TargetRepoRunnerFixtureExecution, TargetRepoRunnerRuntimeError> {
    let execution_plan = plan_target_repo_runner_execution(&input.plan, &input.readiness)?;
    execute_target_repo_runner_execution_fixture(
        &input.plan,
        &execution_plan,
        &input.readiness,
        &input.dedupe,
        input.created_pull_request.as_ref(),
    )
}

pub fn execute_target_repo_runner_execution_fixture(
    plan: &TargetRepoRunnerPlan,
    execution_plan: &TargetRepoRunnerExecutionPlan,
    readiness: &TargetRepoRunnerReadinessObservation,
    dedupe_observation: &TargetRepoRunnerDedupeLookupObservation,
    created_pull_request: Option<&TargetRepoRunnerExistingPullRequest>,
) -> Result<TargetRepoRunnerFixtureExecution, TargetRepoRunnerRuntimeError> {
    validate_readiness_boundary(execution_plan, readiness)?;
    if execution_plan.readiness.target_scafld_required && !execution_plan.readiness.scafld_ready {
        return Err(TargetRepoRunnerRuntimeError::CheckoutNotScafldReady {
            target_repo: execution_plan.checkout.target_repo.clone(),
        });
    }
    if execution_plan.readiness.runner_scafld_required && !readiness.scafld_ready {
        return Err(TargetRepoRunnerRuntimeError::CheckoutNotScafldReady {
            target_repo: execution_plan.checkout.target_repo.clone(),
        });
    }

    let dedupe_execution = execute_target_repo_runner_dedupe_lookup(
        &execution_plan.provider_lookup,
        dedupe_observation,
    )?;
    let deduped_plan = apply_target_repo_runner_dedupe_lookup_execution(plan, &dedupe_execution)?;
    let disposition = if dedupe_execution.result == TargetRepoRunnerDedupeResult::Reused {
        TargetRepoRunnerPullRequestDisposition::Reuse
    } else {
        TargetRepoRunnerPullRequestDisposition::Create
    };
    let pull_request = match disposition {
        TargetRepoRunnerPullRequestDisposition::Reuse => {
            dedupe_execution.existing_pull_request.clone().ok_or(
                TargetRepoRunnerRuntimeError::Plan(TargetRepoRunnerPlanError::PullRequestRequired),
            )?
        }
        TargetRepoRunnerPullRequestDisposition::Create => {
            created_pull_request.cloned().ok_or_else(|| {
                TargetRepoRunnerRuntimeError::CreatedPullRequestRequired {
                    target_repo: execution_plan.checkout.target_repo.clone(),
                }
            })?
        }
    };

    let pull_request_receipt =
        plan_target_repo_runner_pull_request_receipt(&deduped_plan, Some(&pull_request))?;
    let source_publication_receipt =
        plan_target_repo_runner_source_publication_receipt(&deduped_plan, &pull_request);

    Ok(TargetRepoRunnerFixtureExecution {
        execution_plan: execution_plan.clone(),
        dedupe_execution,
        deduped_plan,
        disposition,
        pull_request,
        pull_request_receipt,
        source_publication_receipt,
    })
}

fn validate_readiness_boundary(
    execution_plan: &TargetRepoRunnerExecutionPlan,
    readiness: &TargetRepoRunnerReadinessObservation,
) -> Result<(), TargetRepoRunnerRuntimeError> {
    if readiness.target_repo != execution_plan.checkout.target_repo {
        return Err(TargetRepoRunnerRuntimeError::ReadinessMismatch(format!(
            "readiness target '{}' does not match execution target '{}'",
            readiness.target_repo, execution_plan.checkout.target_repo
        )));
    }
    if readiness.runner_id != execution_plan.readiness.runner_id {
        return Err(TargetRepoRunnerRuntimeError::ReadinessMismatch(format!(
            "readiness runner '{}' does not match execution runner '{}'",
            readiness.runner_id, execution_plan.readiness.runner_id
        )));
    }
    if readiness.scafld_ready != execution_plan.readiness.scafld_ready {
        return Err(TargetRepoRunnerRuntimeError::ReadinessMismatch(
            "readiness observation changed after execution planning".to_owned(),
        ));
    }
    Ok(())
}

fn validate_provider_dedupe_lookup_observation(
    command: &TargetRepoRunnerProviderDedupeLookupCommand,
    observation: &TargetRepoRunnerDedupeLookupObservation,
) -> Result<(), TargetRepoRunnerRuntimeError> {
    if observation.provider != command.provider {
        return Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "provider_dedupe_lookup",
            message: "provider lookup readback provider does not match command".to_owned(),
        });
    }
    if observation.target_repo != command.target_repo {
        return Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "provider_dedupe_lookup",
            message: "provider lookup readback target repo does not match command".to_owned(),
        });
    }
    if observation.key != command.dedupe_key {
        return Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "provider_dedupe_lookup",
            message: "provider lookup readback dedupe key does not match command".to_owned(),
        });
    }
    if observation.pull_requests.len() > usize::from(command.result_limit) {
        return Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "provider_dedupe_lookup",
            message: "provider lookup readback exceeded the command result limit".to_owned(),
        });
    }
    Ok(())
}

fn github_repository(
    repo: &str,
    operation: &'static str,
) -> Result<TargetRepoRunnerGithubRepository, TargetRepoRunnerRuntimeError> {
    let mut parts = repo.split('/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    if owner.is_empty() || name.is_empty() || parts.next().is_some() {
        return Err(invalid_github_repo(operation));
    }
    if !valid_github_owner(owner) || !valid_github_repo_name(name) {
        return Err(invalid_github_repo(operation));
    }
    Ok(TargetRepoRunnerGithubRepository {
        owner: owner.to_owned(),
        name: name.to_owned(),
        full_name: format!("{owner}/{name}"),
    })
}

fn invalid_github_repo(operation: &'static str) -> TargetRepoRunnerRuntimeError {
    TargetRepoRunnerRuntimeError::CommandValidation {
        operation,
        message: "target repo must be a github owner/repo with safe path segments".to_owned(),
    }
}

fn valid_github_owner(owner: &str) -> bool {
    !owner.is_empty()
        && owner.len() <= 39
        && !owner.starts_with('-')
        && !owner.ends_with('-')
        && owner
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn valid_github_repo_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name != "."
        && name != ".."
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
}

fn validate_provider_lookup_term(
    value: &str,
    field: &'static str,
) -> Result<(), TargetRepoRunnerRuntimeError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "provider_dedupe_lookup",
            message: format!("provider lookup {field} must be non-empty text"),
        });
    }
    Ok(())
}

fn github_search_exact_term(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn github_search_item_to_pull_request(
    command: &TargetRepoRunnerProviderDedupeLookupCommand,
    item: GithubIssueSearchItem,
) -> Option<TargetRepoRunnerProviderPullRequest> {
    item.pull_request.as_ref()?;
    let expected_prefix = format!("https://github.com/{}/pull/", command.target_repo);
    if !item.html_url.starts_with(&expected_prefix) {
        return None;
    }
    let text = [item.title.as_deref(), item.body.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n");
    let markers = command
        .markers
        .iter()
        .filter(|marker| text.contains(marker.as_str()))
        .cloned()
        .collect();
    let refs = command
        .required_refs
        .iter()
        .filter(|reference| text.contains(reference.uri.as_str()))
        .cloned()
        .collect();
    Some(TargetRepoRunnerProviderPullRequest {
        url: item.html_url,
        number: item.number,
        branch: None,
        open: item
            .state
            .as_deref()
            .is_none_or(|state| state.eq_ignore_ascii_case("open")),
        markers,
        refs,
    })
}

fn strip_one_trailing_slash(value: &str) -> String {
    value.strip_suffix('/').unwrap_or(value).to_owned()
}

// rust-style-allow: long-function because revision receipt assembly must keep
// the act, seal, metadata, and signature hash in one auditable construction.
fn target_repo_runner_revision_receipt(
    execution: &TargetRepoRunnerFixtureExecution,
    runner_observation: Option<&TargetRepoRunnerGovernedRunnerObservation>,
    created_at: &str,
) -> Result<HarnessReceipt, TargetRepoRunnerRuntimeError> {
    let pull_request_ref = execution
        .pull_request_receipt
        .pull_request_ref
        .clone()
        .ok_or_else(|| {
            TargetRepoRunnerRuntimeError::Receipt("pull request ref is required".to_owned())
        })?;
    let act_id = "act_target_runner_pull_request";
    let criterion_id = "target_runner.pull_request_ready";
    let disposition_name = disposition_name(execution.disposition);
    let target_repo_ref = execution.pull_request_receipt.target_repo_ref.clone();
    let mut evidence_refs = vec![
        execution.pull_request_receipt.source_thread_ref.clone(),
        target_repo_ref.clone(),
        pull_request_ref.clone(),
    ];
    if let Some(source_issue_ref) = &execution.pull_request_receipt.source_issue_ref {
        evidence_refs.push(source_issue_ref.clone());
    }
    let artifact_refs =
        runner_observation.map_or_else(Vec::new, |observation| observation.artifact_refs.clone());
    let verification_refs = runner_observation.map_or_else(Vec::new, |observation| {
        observation.verification_refs.clone()
    });
    let summary = format!(
        "Target runner {disposition_name} pull request {} for {}.",
        pull_request_ref.uri, execution.execution_plan.checkout.target_repo
    );
    let reason_code = format!("target_runner_pr_{}", disposition_name);
    let act = revision_act(RevisionActInput {
        act_id,
        criterion_id,
        created_at,
        disposition: execution.disposition,
        summary: &summary,
        target_repo_ref: &target_repo_ref,
        source_thread_ref: &execution.pull_request_receipt.source_thread_ref,
        source_issue_ref: execution.pull_request_receipt.source_issue_ref.as_ref(),
        pull_request_ref: &pull_request_ref,
        artifact_refs: &artifact_refs,
        verification_refs: &verification_refs,
        runner_observation,
    });
    let seal = receipt_seal(ReceiptSealInputs {
        reason_code: &reason_code,
        summary: &summary,
        created_at,
        act_id,
        criterion_id,
        evidence_refs: &evidence_refs,
        verification_refs: &verification_refs,
        artifact_refs: &artifact_refs,
    });
    let receipt_id = format!(
        "hrn_rcpt_target_runner_{}_{}",
        safe_id(&execution.execution_plan.checkout.target_repo),
        pull_request_id_fragment(&execution.pull_request)
    );
    let harness = receipt_harness(
        execution,
        &receipt_id,
        act,
        seal.clone(),
        created_at,
        &evidence_refs,
        &artifact_refs,
    );
    let mut receipt = HarnessReceipt {
        schema: HarnessReceiptSchema::V1,
        id: receipt_id,
        created_at: created_at.to_owned(),
        issuer: local_issuer(),
        signature: runx_contracts::ReceiptSignature {
            alg: SignatureAlgorithm::Ed25519,
            value: "sig:pending".to_owned(),
        },
        harness,
        seal,
        sync_points: Vec::new(),
        metadata: Some(revision_receipt_metadata(execution)),
    };
    seal_revision_receipt(&mut receipt)?;
    Ok(receipt)
}

fn target_repo_runner_source_publication_request(
    execution: &TargetRepoRunnerFixtureExecution,
    revision_receipt: &HarnessReceipt,
    revision_projection: &TargetRepoRunnerRevisionReceiptProjection,
) -> TargetRepoRunnerSourcePublicationRequest {
    let publication = execution.source_publication_receipt.clone();
    let revision_receipt_ref = reference(ReferenceType::HarnessReceipt, &revision_receipt.id);
    let commands = source_publication_commands(&publication, &revision_receipt_ref);
    TargetRepoRunnerSourcePublicationRequest {
        publication,
        revision_receipt_ref,
        revision_projection: revision_projection.clone(),
        commands,
    }
}

fn source_publication_commands(
    publication: &TargetRepoRunnerSourcePublicationReceiptPlan,
    revision_receipt_ref: &Reference,
) -> Vec<TargetRepoRunnerSourcePublicationCommand> {
    let body = source_publication_body(publication, revision_receipt_ref);
    let mut commands = Vec::new();
    if let Some(source_issue_ref) = &publication.source_issue_ref {
        commands.push(
            TargetRepoRunnerSourcePublicationCommand::SourceIssueComment {
                target: source_issue_ref.clone(),
                body: body.clone(),
            },
        );
    }
    commands.push(
        TargetRepoRunnerSourcePublicationCommand::SourceThreadReply {
            target: publication.source_thread_ref.clone(),
            body,
        },
    );
    commands
}

fn source_publication_body(
    publication: &TargetRepoRunnerSourcePublicationReceiptPlan,
    revision_receipt_ref: &Reference,
) -> String {
    let target_repo = metadata_path_string(&publication.metadata, &["target_repo"])
        .or(publication.pull_request_ref.locator.as_deref())
        .unwrap_or("target repo");
    let dedupe_result =
        metadata_path_string(&publication.metadata, &["dedupe", "result"]).unwrap_or("unknown");
    let dedupe_key =
        metadata_path_string(&publication.metadata, &["dedupe", "key"]).unwrap_or("unknown");
    format!(
        "Target pull request ready: {}\nTarget repo: {target_repo}\nDedupe: {dedupe_result} ({dedupe_key})\nHuman review remains the merge gate.\nReceipt: {}",
        publication.pull_request_ref.uri, revision_receipt_ref.uri
    )
}

// rust-style-allow: long-function because source publication receipt assembly
// keeps the reply act, criteria, seal, metadata, and signature hash together.
fn target_repo_runner_source_publication_harness_receipt(
    request: &TargetRepoRunnerSourcePublicationRequest,
    observation: &TargetRepoRunnerSourcePublicationObservation,
    created_at: &str,
) -> Result<HarnessReceipt, TargetRepoRunnerRuntimeError> {
    validate_source_publication_observation(request, observation)?;

    let act_id = "act_target_runner_source_publication";
    let criterion_id = "target_runner.source_publication_published";
    let target_refs = source_publication_target_refs(observation);
    let mut evidence_refs = vec![
        observation.pull_request_ref.clone(),
        observation.revision_receipt_ref.clone(),
    ];
    evidence_refs.extend(target_refs.clone());
    evidence_refs.extend(observation.published_refs.clone());
    let summary = format!(
        "Published target pull request {} to the source issue/thread.",
        observation.pull_request_ref.uri
    );
    let success_criterion = SuccessCriterion {
        criterion_id: criterion_id.to_owned(),
        statement: "Target pull request link is published back to the source issue/thread."
            .to_owned(),
        required: true,
    };
    let act = Act {
        schema: None,
        act_id: act_id.to_owned(),
        form: ActForm::Reply,
        intent: Intent {
            purpose: "Publish the target pull request link back to the original source context."
                .to_owned(),
            legitimacy: "Operational policy admitted source-thread publication for this runner."
                .to_owned(),
            success_criteria: vec![success_criterion.clone()],
            constraints: vec![
                "Public publication must use repo names and URLs, not local checkout paths."
                    .to_owned(),
                "Source issue/thread references must match the target-runner plan.".to_owned(),
            ],
            derived_from: vec![
                observation.pull_request_ref.clone(),
                observation.revision_receipt_ref.clone(),
            ],
        },
        summary: summary.clone(),
        closure: Closure {
            disposition: ClosureDisposition::Closed,
            reason_code: "target_runner_source_published".to_owned(),
            summary: summary.clone(),
            closed_at: created_at.to_owned(),
        },
        criterion_bindings: vec![CriterionBinding {
            criterion_id: criterion_id.to_owned(),
            status: CriterionStatus::Verified,
            evidence_refs: evidence_refs.clone(),
            verification_refs: Vec::new(),
            summary: Some(summary.clone()),
        }],
        source_refs: vec![
            observation.pull_request_ref.clone(),
            observation.revision_receipt_ref.clone(),
        ],
        target_refs: target_refs.clone(),
        surface_refs: target_refs.clone(),
        artifact_refs: observation.published_refs.clone(),
        verification_refs: Vec::new(),
        harness_refs: vec![observation.revision_receipt_ref.clone()],
        revision: None,
        verification: None,
        performed_at: created_at.to_owned(),
    };
    let seal = receipt_seal(ReceiptSealInputs {
        reason_code: "target_runner_source_published",
        summary: &summary,
        created_at,
        act_id,
        criterion_id,
        evidence_refs: &evidence_refs,
        verification_refs: &[],
        artifact_refs: &observation.published_refs,
    });
    let target_repo =
        metadata_path_string(&request.publication.metadata, &["target_repo"]).unwrap_or("target");
    let receipt_id = format!(
        "hrn_rcpt_target_runner_source_publication_{}_{}",
        safe_id(target_repo),
        reference_id_fragment(&observation.pull_request_ref)
    );
    let harness = source_publication_receipt_harness(
        request,
        observation,
        &receipt_id,
        act,
        seal.clone(),
        created_at,
        &evidence_refs,
    );
    let mut receipt = HarnessReceipt {
        schema: HarnessReceiptSchema::V1,
        id: receipt_id,
        created_at: created_at.to_owned(),
        issuer: local_issuer(),
        signature: runx_contracts::ReceiptSignature {
            alg: SignatureAlgorithm::Ed25519,
            value: "sig:pending".to_owned(),
        },
        harness,
        seal,
        sync_points: Vec::new(),
        metadata: Some(source_publication_receipt_metadata(request, observation)),
    };
    seal_revision_receipt(&mut receipt)?;
    Ok(receipt)
}

struct RevisionActInput<'a> {
    act_id: &'a str,
    criterion_id: &'a str,
    created_at: &'a str,
    disposition: TargetRepoRunnerPullRequestDisposition,
    summary: &'a str,
    target_repo_ref: &'a Reference,
    source_thread_ref: &'a Reference,
    source_issue_ref: Option<&'a Reference>,
    pull_request_ref: &'a Reference,
    artifact_refs: &'a [Reference],
    verification_refs: &'a [Reference],
    runner_observation: Option<&'a TargetRepoRunnerGovernedRunnerObservation>,
}

// rust-style-allow: long-function because the act payload binds intent,
// closure, revision details, and reference roles as one governed shape.
fn revision_act(input: RevisionActInput<'_>) -> Act {
    let RevisionActInput {
        act_id,
        criterion_id,
        created_at,
        disposition,
        summary,
        target_repo_ref,
        source_thread_ref,
        source_issue_ref,
        pull_request_ref,
        artifact_refs,
        verification_refs,
        runner_observation,
    } = input;
    let mut source_refs = vec![source_thread_ref.clone()];
    if let Some(source_issue_ref) = source_issue_ref {
        source_refs.push(source_issue_ref.clone());
    }
    let target_refs = vec![target_repo_ref.clone(), pull_request_ref.clone()];
    let success_criterion = SuccessCriterion {
        criterion_id: criterion_id.to_owned(),
        statement: "Target pull request is ready and linked to the source thread.".to_owned(),
        required: true,
    };
    Act {
        schema: None,
        act_id: act_id.to_owned(),
        form: ActForm::Revision,
        intent: Intent {
            purpose: "Run the governed target runner and surface the target pull request."
                .to_owned(),
            legitimacy: "Operational policy admitted this target repo runner execution.".to_owned(),
            success_criteria: vec![success_criterion.clone()],
            constraints: vec![
                "Dedupe must run before creating a target pull request.".to_owned(),
                "Public output must not include local checkout paths.".to_owned(),
            ],
            derived_from: source_refs.clone(),
        },
        summary: summary.to_owned(),
        closure: Closure {
            disposition: ClosureDisposition::Closed,
            reason_code: format!("target_runner_pr_{}", disposition_name(disposition)),
            summary: summary.to_owned(),
            closed_at: created_at.to_owned(),
        },
        criterion_bindings: vec![CriterionBinding {
            criterion_id: criterion_id.to_owned(),
            status: CriterionStatus::Verified,
            evidence_refs: target_refs.clone(),
            verification_refs: verification_refs.to_vec(),
            summary: Some(summary.to_owned()),
        }],
        source_refs,
        target_refs: target_refs.clone(),
        surface_refs: target_refs.clone(),
        artifact_refs: artifact_refs.to_vec(),
        verification_refs: verification_refs.to_vec(),
        harness_refs: Vec::new(),
        revision: Some(revision_details(
            disposition,
            &success_criterion,
            target_repo_ref,
            pull_request_ref,
            runner_observation,
            verification_refs,
            created_at,
        )),
        verification: None,
        performed_at: created_at.to_owned(),
    }
}

// rust-style-allow: long-function because revision details preserve target
// surfaces, output bindings, and runner observations without lossy helpers.
fn revision_details(
    disposition: TargetRepoRunnerPullRequestDisposition,
    success_criterion: &SuccessCriterion,
    target_repo_ref: &Reference,
    pull_request_ref: &Reference,
    runner_observation: Option<&TargetRepoRunnerGovernedRunnerObservation>,
    verification_refs: &[Reference],
    created_at: &str,
) -> RevisionDetails {
    let target_surfaces = vec![
        TargetSurface {
            surface_ref: target_repo_ref.clone(),
            mutating: true,
            rationale: Some("Target runner is authorized for this repository.".to_owned()),
        },
        TargetSurface {
            surface_ref: pull_request_ref.clone(),
            mutating: true,
            rationale: Some(format!(
                "Pull request path was {} for the dedupe key.",
                disposition_name(disposition)
            )),
        },
    ];
    RevisionDetails {
        change_request: ChangeRequest {
            request_id: format!("change_target_runner_pr_{}", disposition_name(disposition)),
            summary: "Prepare the target pull request for human review.".to_owned(),
            target_surfaces: target_surfaces.clone(),
            success_criteria: vec![success_criterion.clone()],
        },
        change_plan: ChangePlan {
            plan_id: "plan_target_runner_pr".to_owned(),
            summary: "Use provider dedupe, run the governed target runner when needed, and record the target pull request.".to_owned(),
            steps: vec![
                "Check out and verify the target repo readiness.".to_owned(),
                "Look up provider pull requests for the dedupe key.".to_owned(),
                "Create or reuse the target pull request observation.".to_owned(),
            ],
            risks: Vec::new(),
        },
        target_surfaces,
        invariants: vec![
            "No mutation occurs before scafld readiness is observed.".to_owned(),
            "Dedupe is authoritative for create versus reuse.".to_owned(),
        ],
        verification: Some(Verification {
            schema: None,
            verification_id: Some("ver_target_runner_pr_ready".to_owned()),
            status: VerificationStatus::Passed,
            checks: vec![VerificationCheck {
                check_id: "check_target_runner_pr_ready".to_owned(),
                criterion_ids: vec![success_criterion.criterion_id.clone()],
                status: VerificationStatus::Passed,
                summary: Some(runner_observation.map_or_else(
                    || "Existing pull request was reused.".to_owned(),
                    |observation| observation.summary.clone(),
                )),
                checked_refs: vec![target_repo_ref.clone(), pull_request_ref.clone()],
                evidence_refs: verification_refs.to_vec(),
                verified_at: Some(created_at.to_owned()),
            }],
            verified_at: Some(created_at.to_owned()),
            evidence_refs: verification_refs.to_vec(),
        }),
        handoff_refs: Vec::new(),
        revision_refs: runner_observation.map_or_else(
            || vec![pull_request_ref.clone()],
            |observation| {
                let mut refs = observation.revision_refs.clone();
                if !refs.iter().any(|reference| reference.uri == pull_request_ref.uri) {
                    refs.push(pull_request_ref.clone());
                }
                refs
            },
        ),
    }
}

// rust-style-allow: long-function because the harness payload is the sealed
// authority boundary that must visibly contain decision, act, and enforcement.
fn receipt_harness(
    execution: &TargetRepoRunnerFixtureExecution,
    receipt_id: &str,
    act: Act,
    seal: HarnessSeal,
    created_at: &str,
    evidence_refs: &[Reference],
    artifact_refs: &[Reference],
) -> Harness {
    let decision_id = "dec_target_runner_pr";
    Harness {
        schema: None,
        harness_id: format!(
            "hrn_target_runner_{}",
            safe_id(&execution.execution_plan.checkout.target_repo)
        ),
        parent_harness_ref: None,
        state: HarnessState::Sealed,
        host_ref: reference(ReferenceType::Host, "target_runner_adapter"),
        harness_ref: reference(ReferenceType::Harness, "target-runner"),
        authority: Authority {
            schema: None,
            actor_ref: reference(ReferenceType::Principal, "target_runner"),
            authority_proof_refs: Vec::new(),
            grant_refs: Vec::new(),
            scope_refs: Vec::new(),
            policy_refs: Vec::new(),
            terms: Vec::new(),
            attenuation: AuthorityAttenuation {
                parent_authority_ref: None,
                subset_proof: None,
            },
            mandate_ref: None,
        },
        enforcement: HarnessEnforcement {
            harness_ref: None,
            version: "target-runner-adapter.v1".to_owned(),
            enforcement_profile_hash: stable_hash(&format!(
                "target-runner:{}",
                execution.execution_plan.checkout.target_repo
            )),
            enforcer_ref: None,
            sandbox: HarnessSandbox {
                profile: "target-runner-adapter".to_owned(),
                cwd_policy: "target-checkout-hidden".to_owned(),
                network: "adapter-declared".to_owned(),
                filesystem: "target-repo-scoped".to_owned(),
            },
            redaction_refs: Vec::new(),
            stdout_hash: None,
            stderr_hash: None,
            setup_receipt_refs: Vec::new(),
            teardown_receipt_refs: Vec::new(),
        },
        idempotency: HarnessIdempotency {
            intent_key: stable_hash(&format!(
                "target-runner:{}:{}",
                execution.execution_plan.checkout.target_repo, execution.dedupe_execution.key
            )),
            trigger_fingerprint: stable_hash(&execution.dedupe_execution.key),
            content_hash: stable_hash(receipt_id),
        },
        revision: HarnessRevision {
            sequence: 1,
            previous_ref: None,
        },
        signal_refs: vec![execution.execution_plan.source_thread_ref.clone()],
        decisions: vec![Decision {
            decision_id: decision_id.to_owned(),
            choice: DecisionChoice::Close,
            inputs: DecisionInputs {
                signal_refs: vec![execution.execution_plan.source_thread_ref.clone()],
                target_ref: Some(execution.execution_plan.target_repo_ref.clone()),
                opportunity_refs: Vec::new(),
                selection_ref: None,
            },
            proposed_intent: act.intent.clone(),
            selected_act_id: Some(act.act_id.clone()),
            selected_harness_ref: None,
            justification: DecisionJustification {
                summary: "Selected the policy-admitted target runner path.".to_owned(),
                evidence_refs: evidence_refs.to_vec(),
            },
            closure: Some(Closure {
                disposition: ClosureDisposition::Closed,
                reason_code: "target_runner_decision_closed".to_owned(),
                summary: "Target pull request path was recorded.".to_owned(),
                closed_at: created_at.to_owned(),
            }),
            artifact_refs: artifact_refs.to_vec(),
        }],
        acts: vec![act],
        child_harness_receipt_refs: Vec::new(),
        artifact_refs: artifact_refs.to_vec(),
        seal: Some(seal),
    }
}

// rust-style-allow: long-function because this sealed child harness records
// authority attenuation, enforcement, decision, and reply act in one boundary.
fn source_publication_receipt_harness(
    request: &TargetRepoRunnerSourcePublicationRequest,
    observation: &TargetRepoRunnerSourcePublicationObservation,
    receipt_id: &str,
    act: Act,
    seal: HarnessSeal,
    created_at: &str,
    evidence_refs: &[Reference],
) -> Harness {
    let decision_id = "dec_target_runner_source_publication";
    let target_repo =
        metadata_path_string(&request.publication.metadata, &["target_repo"]).unwrap_or("target");
    Harness {
        schema: None,
        harness_id: format!(
            "hrn_target_runner_source_publication_{}",
            safe_id(target_repo)
        ),
        parent_harness_ref: Some(request.revision_receipt_ref.clone()),
        state: HarnessState::Sealed,
        host_ref: reference(
            ReferenceType::Host,
            "target_runner_source_publication_adapter",
        ),
        harness_ref: reference(ReferenceType::Harness, "target-runner-source-publication"),
        authority: Authority {
            schema: None,
            actor_ref: reference(ReferenceType::Principal, "target_runner_source_publication"),
            authority_proof_refs: Vec::new(),
            grant_refs: Vec::new(),
            scope_refs: Vec::new(),
            policy_refs: Vec::new(),
            terms: Vec::new(),
            attenuation: AuthorityAttenuation {
                parent_authority_ref: Some(request.revision_receipt_ref.clone()),
                subset_proof: Some(AuthoritySubsetProof {
                    parent_authority_ref: request.revision_receipt_ref.clone(),
                    comparison_algorithm: "runx.target-runner.publication-subset.v1".to_owned(),
                    result: AuthoritySubsetResult::Subset,
                    compared_terms: Vec::new(),
                    proof_ref: None,
                    checked_at: created_at.to_owned(),
                }),
            },
            mandate_ref: None,
        },
        enforcement: HarnessEnforcement {
            harness_ref: None,
            version: "target-runner-source-publication-adapter.v1".to_owned(),
            enforcement_profile_hash: stable_hash(&format!(
                "target-runner-source-publication:{}",
                target_repo
            )),
            enforcer_ref: None,
            sandbox: HarnessSandbox {
                profile: "source-publication-adapter".to_owned(),
                cwd_policy: "no-local-checkout".to_owned(),
                network: "adapter-declared".to_owned(),
                filesystem: "no-local-filesystem".to_owned(),
            },
            redaction_refs: Vec::new(),
            stdout_hash: None,
            stderr_hash: None,
            setup_receipt_refs: Vec::new(),
            teardown_receipt_refs: Vec::new(),
        },
        idempotency: HarnessIdempotency {
            intent_key: stable_hash(&format!(
                "target-runner-source-publication:{}:{}",
                observation.source_thread_ref.uri, observation.pull_request_ref.uri
            )),
            trigger_fingerprint: stable_hash(&observation.pull_request_ref.uri),
            content_hash: stable_hash(receipt_id),
        },
        revision: HarnessRevision {
            sequence: 1,
            previous_ref: Some(request.revision_receipt_ref.clone()),
        },
        signal_refs: vec![observation.source_thread_ref.clone()],
        decisions: vec![Decision {
            decision_id: decision_id.to_owned(),
            choice: DecisionChoice::Close,
            inputs: DecisionInputs {
                signal_refs: vec![observation.source_thread_ref.clone()],
                target_ref: Some(observation.pull_request_ref.clone()),
                opportunity_refs: Vec::new(),
                selection_ref: None,
            },
            proposed_intent: act.intent.clone(),
            selected_act_id: Some(act.act_id.clone()),
            selected_harness_ref: None,
            justification: DecisionJustification {
                summary: "Selected the source-publication path for the target pull request."
                    .to_owned(),
                evidence_refs: evidence_refs.to_vec(),
            },
            closure: Some(Closure {
                disposition: ClosureDisposition::Closed,
                reason_code: "target_runner_source_publication_closed".to_owned(),
                summary: "Target pull request link was published to the source context.".to_owned(),
                closed_at: created_at.to_owned(),
            }),
            artifact_refs: observation.published_refs.clone(),
        }],
        acts: vec![act],
        child_harness_receipt_refs: Vec::new(),
        artifact_refs: observation.published_refs.clone(),
        seal: Some(seal),
    }
}

fn validate_source_publication_observation(
    request: &TargetRepoRunnerSourcePublicationRequest,
    observation: &TargetRepoRunnerSourcePublicationObservation,
) -> Result<(), TargetRepoRunnerRuntimeError> {
    if !same_reference(
        &observation.source_thread_ref,
        &request.publication.source_thread_ref,
    ) {
        return Err(TargetRepoRunnerRuntimeError::SourcePublicationMismatch(
            "source thread readback does not match publication plan".to_owned(),
        ));
    }
    if !same_reference(
        &observation.pull_request_ref,
        &request.publication.pull_request_ref,
    ) {
        return Err(TargetRepoRunnerRuntimeError::SourcePublicationMismatch(
            "target pull request readback does not match publication plan".to_owned(),
        ));
    }
    if !same_reference(
        &observation.revision_receipt_ref,
        &request.revision_receipt_ref,
    ) {
        return Err(TargetRepoRunnerRuntimeError::SourcePublicationMismatch(
            "revision receipt readback does not match publication request".to_owned(),
        ));
    }
    match (
        &request.publication.source_issue_ref,
        &observation.source_issue_ref,
    ) {
        (Some(expected), Some(actual)) if same_reference(actual, expected) => {}
        (Some(_), Some(_)) => {
            return Err(TargetRepoRunnerRuntimeError::SourcePublicationMismatch(
                "source issue readback does not match publication plan".to_owned(),
            ));
        }
        (Some(_), None) => {
            return Err(TargetRepoRunnerRuntimeError::SourcePublicationMismatch(
                "source issue publication readback is required".to_owned(),
            ));
        }
        (None, Some(_)) => {
            return Err(TargetRepoRunnerRuntimeError::SourcePublicationMismatch(
                "source issue readback was returned for a plan without a source issue".to_owned(),
            ));
        }
        (None, None) => {}
    }
    if observation.published_refs.len() < request.commands.len() {
        return Err(TargetRepoRunnerRuntimeError::SourcePublicationMismatch(
            "publication readback did not return a ref for every source command".to_owned(),
        ));
    }
    Ok(())
}

fn source_publication_target_refs(
    observation: &TargetRepoRunnerSourcePublicationObservation,
) -> Vec<Reference> {
    let mut target_refs = vec![observation.source_thread_ref.clone()];
    if let Some(source_issue_ref) = &observation.source_issue_ref {
        target_refs.push(source_issue_ref.clone());
    }
    target_refs
}

fn source_publication_receipt_metadata(
    request: &TargetRepoRunnerSourcePublicationRequest,
    observation: &TargetRepoRunnerSourcePublicationObservation,
) -> JsonObject {
    let mut metadata = request.publication.metadata.clone();
    let mut target_runner = JsonObject::new();
    target_runner.insert(
        "contract".to_owned(),
        JsonValue::String("runx.target_repo_runner.source_publication.v1".to_owned()),
    );
    target_runner.insert(
        "revision_receipt".to_owned(),
        JsonValue::String(request.revision_receipt_ref.uri.clone()),
    );
    target_runner.insert(
        "command_count".to_owned(),
        JsonValue::Number(JsonNumber::U64(request.commands.len() as u64)),
    );
    metadata.insert("target_runner".to_owned(), JsonValue::Object(target_runner));
    metadata.insert(
        "published_refs".to_owned(),
        JsonValue::Array(
            observation
                .published_refs
                .iter()
                .map(|reference| JsonValue::String(reference.uri.clone()))
                .collect(),
        ),
    );
    metadata
}

struct ReceiptSealInputs<'a> {
    reason_code: &'a str,
    summary: &'a str,
    created_at: &'a str,
    act_id: &'a str,
    criterion_id: &'a str,
    evidence_refs: &'a [Reference],
    verification_refs: &'a [Reference],
    artifact_refs: &'a [Reference],
}

fn receipt_seal(inputs: ReceiptSealInputs<'_>) -> HarnessSeal {
    HarnessSeal {
        disposition: ClosureDisposition::Closed,
        reason_code: inputs.reason_code.to_owned(),
        summary: inputs.summary.to_owned(),
        closed_at: inputs.created_at.to_owned(),
        last_observed_at: inputs.created_at.to_owned(),
        canonicalization: "runx.harness-receipt.c14n.v1".to_owned(),
        digest: "sha256:pending".to_owned(),
        criteria: vec![SealCriterion {
            criterion_id: inputs.criterion_id.to_owned(),
            status: CriterionStatus::Verified,
            act_id: Some(inputs.act_id.to_owned()),
            verification_refs: inputs.verification_refs.to_vec(),
            evidence_refs: inputs.evidence_refs.to_vec(),
            summary: Some(inputs.summary.to_owned()),
        }],
        verification_summary: Some(ReceiptVerificationSummary {
            signature_valid: true,
            hash_commitments_valid: true,
            authority_attenuation_valid: true,
            criteria_bound: true,
            redaction_valid: true,
            external_attestations_present: !inputs.verification_refs.is_empty(),
        }),
        redaction_refs: Vec::new(),
        artifact_refs: inputs.artifact_refs.to_vec(),
        hash_commitments: Vec::new(),
    }
}

fn seal_revision_receipt(receipt: &mut HarnessReceipt) -> Result<(), TargetRepoRunnerRuntimeError> {
    let digest = canonical_receipt_body_digest(receipt)
        .map_err(|error| TargetRepoRunnerRuntimeError::Receipt(error.to_string()))?;
    receipt.seal.digest = digest.clone();
    if let Some(harness_seal) = receipt.harness.seal.as_mut() {
        harness_seal.digest = digest.clone();
    }
    receipt.signature.value = format!("sig:{digest}");
    validate_harness_receipt(receipt)
        .map_err(|verification| TargetRepoRunnerRuntimeError::Receipt(format!("{verification:?}")))
}

// rust-style-allow: long-function because projection validates a sealed receipt
// and reconstructs the public target-runner view without partial helpers.
pub fn project_target_repo_runner_revision_receipt(
    receipt: &HarnessReceipt,
) -> Result<TargetRepoRunnerRevisionReceiptProjection, TargetRepoRunnerRuntimeError> {
    if receipt.harness.state != HarnessState::Sealed {
        return Err(TargetRepoRunnerRuntimeError::ReceiptProjection(
            "receipt harness is not sealed".to_owned(),
        ));
    }
    let act = receipt
        .harness
        .acts
        .iter()
        .find(|act| act.form == ActForm::Revision)
        .ok_or_else(|| {
            TargetRepoRunnerRuntimeError::ReceiptProjection("revision act is required".to_owned())
        })?;
    let metadata = receipt.metadata.clone().ok_or_else(|| {
        TargetRepoRunnerRuntimeError::ReceiptProjection(
            "target runner metadata is required".to_owned(),
        )
    })?;
    let pull_request_ref = act
        .target_refs
        .iter()
        .find(|reference| reference.reference_type == ReferenceType::GithubPullRequest)
        .cloned()
        .ok_or_else(|| {
            TargetRepoRunnerRuntimeError::ReceiptProjection(
                "pull request ref is required".to_owned(),
            )
        })?;
    let target_repo_ref = act
        .target_refs
        .iter()
        .find(|reference| reference.reference_type == ReferenceType::GithubRepo)
        .cloned()
        .ok_or_else(|| {
            TargetRepoRunnerRuntimeError::ReceiptProjection(
                "target repo ref is required".to_owned(),
            )
        })?;
    let source_thread_ref = act.source_refs.first().cloned().ok_or_else(|| {
        TargetRepoRunnerRuntimeError::ReceiptProjection("source thread ref is required".to_owned())
    })?;
    let source_issue_ref = act
        .source_refs
        .iter()
        .find(|reference| reference.reference_type == ReferenceType::GithubIssue)
        .cloned();
    Ok(TargetRepoRunnerRevisionReceiptProjection {
        receipt_ref: reference(ReferenceType::HarnessReceipt, &receipt.id),
        act_id: act.act_id.clone(),
        disposition: projection_disposition(&metadata)?,
        target_repo_ref,
        source_issue_ref,
        source_thread_ref,
        pull_request_ref,
        summary: receipt.seal.summary.clone(),
        metadata,
    })
}

pub fn project_target_repo_runner_source_publication_receipt(
    receipt: &HarnessReceipt,
) -> Result<TargetRepoRunnerSourcePublicationProjection, TargetRepoRunnerRuntimeError> {
    if receipt.harness.state != HarnessState::Sealed {
        return Err(TargetRepoRunnerRuntimeError::ReceiptProjection(
            "source publication receipt harness is not sealed".to_owned(),
        ));
    }
    let act = receipt
        .harness
        .acts
        .iter()
        .find(|act| act.form == ActForm::Reply)
        .ok_or_else(|| {
            TargetRepoRunnerRuntimeError::ReceiptProjection(
                "source publication reply act is required".to_owned(),
            )
        })?;
    let metadata = receipt.metadata.clone().ok_or_else(|| {
        TargetRepoRunnerRuntimeError::ReceiptProjection(
            "source publication metadata is required".to_owned(),
        )
    })?;
    let pull_request_ref = act
        .source_refs
        .iter()
        .find(|reference| reference.reference_type == ReferenceType::GithubPullRequest)
        .cloned()
        .ok_or_else(|| {
            TargetRepoRunnerRuntimeError::ReceiptProjection(
                "source publication pull request ref is required".to_owned(),
            )
        })?;
    let source_thread_ref = act.target_refs.first().cloned().ok_or_else(|| {
        TargetRepoRunnerRuntimeError::ReceiptProjection(
            "source publication thread ref is required".to_owned(),
        )
    })?;
    let source_issue_ref = act
        .target_refs
        .iter()
        .skip(1)
        .find(|reference| reference.reference_type == ReferenceType::GithubIssue)
        .cloned();
    Ok(TargetRepoRunnerSourcePublicationProjection {
        receipt_ref: reference(ReferenceType::HarnessReceipt, &receipt.id),
        source_issue_ref,
        source_thread_ref,
        pull_request_ref,
        published_refs: act.artifact_refs.clone(),
        summary: receipt.seal.summary.clone(),
        metadata,
    })
}

fn revision_receipt_metadata(execution: &TargetRepoRunnerFixtureExecution) -> JsonObject {
    let mut target_runner = JsonObject::new();
    target_runner.insert(
        "contract".to_owned(),
        JsonValue::String("runx.target_repo_runner.v1".to_owned()),
    );
    target_runner.insert(
        "disposition".to_owned(),
        JsonValue::String(disposition_name(execution.disposition).to_owned()),
    );
    target_runner.insert(
        "dedupe_key".to_owned(),
        JsonValue::String(execution.dedupe_execution.key.clone()),
    );
    target_runner.insert(
        "target_repo".to_owned(),
        JsonValue::String(execution.execution_plan.checkout.target_repo.clone()),
    );
    let mut metadata = execution.pull_request_receipt.metadata.clone();
    metadata.insert("target_runner".to_owned(), JsonValue::Object(target_runner));
    metadata
}

fn projection_disposition(
    metadata: &JsonObject,
) -> Result<TargetRepoRunnerPullRequestDisposition, TargetRepoRunnerRuntimeError> {
    let Some(JsonValue::Object(target_runner)) = metadata.get("target_runner") else {
        return Err(TargetRepoRunnerRuntimeError::ReceiptProjection(
            "target runner metadata object is required".to_owned(),
        ));
    };
    match target_runner.get("disposition").and_then(json_string) {
        Some("created") => Ok(TargetRepoRunnerPullRequestDisposition::Create),
        Some("reused") => Ok(TargetRepoRunnerPullRequestDisposition::Reuse),
        _ => Err(TargetRepoRunnerRuntimeError::ReceiptProjection(
            "target runner disposition is invalid".to_owned(),
        )),
    }
}

fn disposition_name(disposition: TargetRepoRunnerPullRequestDisposition) -> &'static str {
    match disposition {
        TargetRepoRunnerPullRequestDisposition::Create => "created",
        TargetRepoRunnerPullRequestDisposition::Reuse => "reused",
    }
}

fn local_issuer() -> ReceiptIssuer {
    ReceiptIssuer {
        issuer_type: ReceiptIssuerType::Local,
        kid: "target-runner-runtime".to_owned(),
        public_key_sha256: "sha256:target-runner-runtime-public".to_owned(),
    }
}

fn reference(reference_type: ReferenceType, id: &str) -> Reference {
    Reference {
        uri: format!("runx:{}:{id}", reference_type_name(&reference_type)),
        reference_type,
        provider: None,
        locator: None,
        label: None,
        observed_at: None,
        proof_kind: None,
    }
}

fn reference_type_name(reference_type: &ReferenceType) -> &'static str {
    match reference_type {
        ReferenceType::GithubIssue => "github_issue",
        ReferenceType::GithubPullRequest => "github_pull_request",
        ReferenceType::GithubRepo => "github_repo",
        ReferenceType::SlackThread => "slack_thread",
        ReferenceType::SentryEvent => "sentry_event",
        ReferenceType::Signal => "signal",
        ReferenceType::Act => "act",
        ReferenceType::Receipt => "receipt",
        ReferenceType::GraphReceipt => "graph_receipt",
        ReferenceType::HarnessReceipt => "harness_receipt",
        ReferenceType::Artifact => "artifact",
        ReferenceType::Verification => "verification",
        ReferenceType::Harness => "harness",
        ReferenceType::Host => "host",
        ReferenceType::Deployment => "deployment",
        ReferenceType::Surface => "surface",
        ReferenceType::Target => "target",
        ReferenceType::Opportunity => "opportunity",
        ReferenceType::ThesisAssessment => "thesis_assessment",
        ReferenceType::Selection => "selection",
        ReferenceType::SkillBinding => "skill_binding",
        ReferenceType::TargetTransitionEntry => "target_transition_entry",
        ReferenceType::SelectionCycle => "selection_cycle",
        ReferenceType::Decision => "decision",
        ReferenceType::ReflectionEntry => "reflection_entry",
        ReferenceType::FeedEntry => "feed_entry",
        ReferenceType::Principal => "principal",
        ReferenceType::AuthorityProof => "authority_proof",
        ReferenceType::ScopeAdmission => "scope_admission",
        ReferenceType::Grant => "grant",
        ReferenceType::Mandate => "mandate",
        ReferenceType::Credential => "credential",
        ReferenceType::WebhookDelivery => "webhook_delivery",
        ReferenceType::RedactionPolicy => "redaction_policy",
        ReferenceType::ExternalUrl => "external_url",
    }
}

fn stable_hash(value: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(value.as_bytes()))
}

fn safe_id(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn pull_request_id_fragment(pull_request: &TargetRepoRunnerExistingPullRequest) -> String {
    pull_request
        .number
        .map(|number| number.to_string())
        .unwrap_or_else(|| safe_id(&pull_request.url))
}

fn reference_id_fragment(reference: &Reference) -> String {
    reference
        .locator
        .as_deref()
        .map(safe_id)
        .unwrap_or_else(|| safe_id(&reference.uri))
}

fn same_reference(left: &Reference, right: &Reference) -> bool {
    left.reference_type == right.reference_type && left.uri == right.uri
}

fn metadata_path_string<'a>(object: &'a JsonObject, path: &[&str]) -> Option<&'a str> {
    let mut value = object.get(*path.first()?)?;
    for segment in &path[1..] {
        let JsonValue::Object(object) = value else {
            return None;
        };
        value = object.get(*segment)?;
    }
    json_string(value)
}

fn json_string(value: &JsonValue) -> Option<&str> {
    match value {
        JsonValue::String(value) => Some(value.as_str()),
        _ => None,
    }
}
