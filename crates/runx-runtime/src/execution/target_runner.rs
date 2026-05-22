// rust-style-allow: large-file because target-runner execution currently keeps
// provider dedupe, governed runner observation, PR receipt sealing, and public
// projection in one Rust cutover slice; split after live provider wiring lands.
//! Runtime support for target-repo runner execution.

use std::fmt::{self, Write as _};

use serde::Deserialize;
use serde::Serialize;
use sha2::{Digest, Sha256};
use url::Url;

use runx_contracts::{
    ActForm, AuthorityAttenuation, AuthoritySubsetProof, AuthoritySubsetResult, ClosureDisposition,
    CriterionStatus, JsonNumber, JsonObject, JsonValue, Lineage, RECEIPT_CANONICALIZATION, Receipt,
    ReceiptAct, ReceiptAuthority, ReceiptCriterion, ReceiptEnforcement, ReceiptIdempotency,
    ReceiptIssuer, ReceiptIssuerType, ReceiptSchema, ReceiptSubjectKind, Reference, ReferenceType,
    Seal, SignatureAlgorithm, Subject, TargetRepoRunnerDedupeLookupExecution,
    TargetRepoRunnerDedupeLookupObservation, TargetRepoRunnerDedupeLookupPlan,
    TargetRepoRunnerDedupeResult, TargetRepoRunnerExecutionPlan,
    TargetRepoRunnerExistingPullRequest, TargetRepoRunnerPlan, TargetRepoRunnerPlanError,
    TargetRepoRunnerProvider, TargetRepoRunnerProviderPullRequest,
    TargetRepoRunnerPullRequestDisposition, TargetRepoRunnerPullRequestReceiptPlan,
    TargetRepoRunnerReadinessObservation, TargetRepoRunnerSourcePublicationReceiptPlan,
    apply_target_repo_runner_dedupe_lookup_execution, execute_target_repo_runner_dedupe_lookup,
    plan_target_repo_runner_execution, plan_target_repo_runner_pull_request_receipt,
    plan_target_repo_runner_source_publication_receipt,
};
use runx_receipts::{canonical_receipt_body_digest, validate_receipt};

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
    pub git_mutation_command: Option<TargetRepoRunnerGitMutationCommand>,
    pub git_mutation_observation: Option<TargetRepoRunnerGitMutationObservation>,
    pub pull_request_request: TargetRepoRunnerPullRequestObservationRequest,
    pub pull_request_observation: TargetRepoRunnerPullRequestObservation,
    pub execution: TargetRepoRunnerFixtureExecution,
    pub revision_receipt: Receipt,
    pub revision_projection: TargetRepoRunnerRevisionReceiptProjection,
    pub source_publication_request: TargetRepoRunnerSourcePublicationRequest,
    pub source_publication_observation: TargetRepoRunnerSourcePublicationObservation,
    pub source_publication_receipt: Receipt,
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

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TargetRepoRunnerGitMutationCommand {
    pub provider: TargetRepoRunnerProvider,
    pub target_repo: String,
    pub repository: TargetRepoRunnerGithubRepository,
    pub target_repo_ref: Reference,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
    pub branch: String,
    pub dedupe_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_issue_ref: Option<Reference>,
    pub source_thread_ref: Reference,
    pub runner_id: String,
    pub runner_summary: String,
    pub runner_revision_refs: Vec<Reference>,
    pub artifact_refs: Vec<Reference>,
    pub verification_refs: Vec<Reference>,
    pub human_merge_gate_required: bool,
    pub local_path_hidden: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TargetRepoRunnerGitMutationObservation {
    pub target_repo: String,
    pub branch: String,
    pub head_sha: String,
    pub revision_refs: Vec<Reference>,
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
    pub command: TargetRepoRunnerPullRequestMutationCommand,
    pub disposition: TargetRepoRunnerPullRequestDisposition,
    pub target_repo: String,
    pub dedupe_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_pull_request: Option<TargetRepoRunnerExistingPullRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_observation: Option<TargetRepoRunnerGovernedRunnerObservation>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TargetRepoRunnerPullRequestMutationCommand {
    pub provider: TargetRepoRunnerProvider,
    pub disposition: TargetRepoRunnerPullRequestDisposition,
    pub target_repo: String,
    pub repository: TargetRepoRunnerGithubRepository,
    pub target_repo_ref: Reference,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
    pub dedupe_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_issue_ref: Option<Reference>,
    pub source_thread_ref: Reference,
    pub mutation: TargetRepoRunnerPullRequestMutation,
    pub human_merge_gate_required: bool,
    pub local_path_hidden: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TargetRepoRunnerPullRequestMutation {
    Create(TargetRepoRunnerPullRequestCreateCommand),
    Reuse(TargetRepoRunnerPullRequestReuseCommand),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TargetRepoRunnerPullRequestCreateCommand {
    pub title: String,
    pub body: String,
    pub head_branch: String,
    pub head_sha: String,
    pub runner_id: String,
    pub runner_summary: String,
    pub runner_revision_refs: Vec<Reference>,
    pub git_revision_refs: Vec<Reference>,
    pub artifact_refs: Vec<Reference>,
    pub verification_refs: Vec<Reference>,
    pub git_verification_refs: Vec<Reference>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetRepoRunnerPullRequestReuseCommand {
    pub existing_pull_request: TargetRepoRunnerExistingPullRequest,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TargetRepoRunnerPullRequestObservation {
    pub provider: TargetRepoRunnerProvider,
    pub target_repo: String,
    pub pull_request: TargetRepoRunnerExistingPullRequest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_sha: Option<String>,
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

    fn apply_git_mutation(
        &mut self,
        _command: &TargetRepoRunnerGitMutationCommand,
    ) -> Result<TargetRepoRunnerGitMutationObservation, TargetRepoRunnerAdapterError> {
        Err(TargetRepoRunnerAdapterError::new(
            "git_mutation",
            "adapter does not implement target git mutation readback",
        ))
    }

    fn observe_pull_request(
        &mut self,
        request: &TargetRepoRunnerPullRequestObservationRequest,
    ) -> Result<TargetRepoRunnerPullRequestObservation, TargetRepoRunnerAdapterError>;

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
    let git_mutation_command = runner_observation
        .as_ref()
        .map(|observation| {
            target_repo_runner_git_mutation_command(&execution_plan, &dedupe_execution, observation)
        })
        .transpose()?;
    let git_mutation_observation = git_mutation_command
        .as_ref()
        .map(|command| -> Result<_, TargetRepoRunnerRuntimeError> {
            let observation = adapter.apply_git_mutation(command)?;
            validate_git_mutation_readback(command, &observation)?;
            Ok(observation)
        })
        .transpose()?;
    let pull_request_request = target_repo_runner_pull_request_observation_request(
        &execution_plan,
        &dedupe_execution,
        disposition,
        runner_observation.clone(),
        git_mutation_observation.as_ref(),
    )?;
    let pull_request_observation = adapter.observe_pull_request(&pull_request_request)?;
    validate_pull_request_readback(&pull_request_request.command, &pull_request_observation)?;
    let pull_request = pull_request_observation.pull_request.clone();

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
    let source_publication_receipt = target_repo_runner_source_publication_receipt_node(
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
        git_mutation_command,
        git_mutation_observation,
        pull_request_request,
        pull_request_observation,
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

fn target_repo_runner_git_mutation_command(
    execution_plan: &TargetRepoRunnerExecutionPlan,
    dedupe_execution: &TargetRepoRunnerDedupeLookupExecution,
    runner_observation: &TargetRepoRunnerGovernedRunnerObservation,
) -> Result<TargetRepoRunnerGitMutationCommand, TargetRepoRunnerRuntimeError> {
    if runner_observation.target_repo != execution_plan.checkout.target_repo {
        return Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "git_mutation",
            message: "runner observation target repo does not match execution target".to_owned(),
        });
    }
    let repository = github_repository(&execution_plan.checkout.target_repo, "git_mutation")?;
    let branch = target_repo_runner_branch_name(execution_plan, dedupe_execution);
    validate_branch_for_operation(&branch, "git_mutation")?;
    Ok(TargetRepoRunnerGitMutationCommand {
        provider: execution_plan.provider_lookup.provider,
        target_repo: execution_plan.checkout.target_repo.clone(),
        repository,
        target_repo_ref: execution_plan.target_repo_ref.clone(),
        base_branch: execution_plan.checkout.base_branch.clone(),
        branch,
        dedupe_key: execution_plan.provider_lookup.key.clone(),
        source_issue_ref: execution_plan.source_issue_ref.clone(),
        source_thread_ref: execution_plan.source_thread_ref.clone(),
        runner_id: runner_observation.runner_id.clone(),
        runner_summary: runner_observation.summary.clone(),
        runner_revision_refs: runner_observation.revision_refs.clone(),
        artifact_refs: runner_observation.artifact_refs.clone(),
        verification_refs: runner_observation.verification_refs.clone(),
        human_merge_gate_required: true,
        local_path_hidden: true,
    })
}

fn target_repo_runner_branch_name(
    execution_plan: &TargetRepoRunnerExecutionPlan,
    dedupe_execution: &TargetRepoRunnerDedupeLookupExecution,
) -> String {
    format!(
        "runx/{}/{}",
        safe_id(&execution_plan.checkout.target_repo),
        short_key_hash(&dedupe_execution.key)
    )
}

fn short_key_hash(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut hex = String::with_capacity(12);
    for byte in digest.iter().take(6) {
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

fn validate_git_mutation_readback(
    command: &TargetRepoRunnerGitMutationCommand,
    observation: &TargetRepoRunnerGitMutationObservation,
) -> Result<(), TargetRepoRunnerRuntimeError> {
    if observation.target_repo != command.target_repo {
        return Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "git_mutation",
            message: "git mutation readback target repo does not match command".to_owned(),
        });
    }
    if observation.branch != command.branch {
        return Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "git_mutation",
            message: "git mutation readback branch does not match command".to_owned(),
        });
    }
    validate_branch_for_operation(&observation.branch, "git_mutation")?;
    validate_head_sha(&observation.head_sha, "git_mutation")?;
    Ok(())
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

fn target_repo_runner_pull_request_observation_request(
    execution_plan: &TargetRepoRunnerExecutionPlan,
    dedupe_execution: &TargetRepoRunnerDedupeLookupExecution,
    disposition: TargetRepoRunnerPullRequestDisposition,
    runner_observation: Option<TargetRepoRunnerGovernedRunnerObservation>,
    git_mutation_observation: Option<&TargetRepoRunnerGitMutationObservation>,
) -> Result<TargetRepoRunnerPullRequestObservationRequest, TargetRepoRunnerRuntimeError> {
    let command = target_repo_runner_pull_request_mutation_command(
        execution_plan,
        dedupe_execution,
        disposition,
        runner_observation.as_ref(),
        git_mutation_observation,
    )?;
    Ok(TargetRepoRunnerPullRequestObservationRequest {
        command,
        disposition,
        target_repo: execution_plan.checkout.target_repo.clone(),
        dedupe_key: execution_plan.provider_lookup.key.clone(),
        existing_pull_request: dedupe_execution.existing_pull_request.clone(),
        runner_observation,
    })
}

// rust-style-allow: long-function - assembles the pull-request mutation command from the execution
// plan in one pass so every field of the command is mapped in a single reviewable place.
fn target_repo_runner_pull_request_mutation_command(
    execution_plan: &TargetRepoRunnerExecutionPlan,
    dedupe_execution: &TargetRepoRunnerDedupeLookupExecution,
    disposition: TargetRepoRunnerPullRequestDisposition,
    runner_observation: Option<&TargetRepoRunnerGovernedRunnerObservation>,
    git_mutation_observation: Option<&TargetRepoRunnerGitMutationObservation>,
) -> Result<TargetRepoRunnerPullRequestMutationCommand, TargetRepoRunnerRuntimeError> {
    let repository = github_repository(&execution_plan.checkout.target_repo, "pull_request")?;
    let mutation = match disposition {
        TargetRepoRunnerPullRequestDisposition::Create => {
            let observation = runner_observation.ok_or_else(|| {
                TargetRepoRunnerRuntimeError::CommandValidation {
                    operation: "pull_request",
                    message: "runner observation is required before creating a pull request"
                        .to_owned(),
                }
            })?;
            let git_observation = git_mutation_observation.ok_or_else(|| {
                TargetRepoRunnerRuntimeError::CommandValidation {
                    operation: "pull_request",
                    message: "git mutation readback is required before creating a pull request"
                        .to_owned(),
                }
            })?;
            validate_pull_request_git_readback(execution_plan, dedupe_execution, git_observation)?;
            TargetRepoRunnerPullRequestMutation::Create(TargetRepoRunnerPullRequestCreateCommand {
                title: pull_request_create_title(execution_plan),
                body: pull_request_create_body(
                    execution_plan,
                    dedupe_execution,
                    observation,
                    git_observation,
                ),
                head_branch: git_observation.branch.clone(),
                head_sha: git_observation.head_sha.clone(),
                runner_id: observation.runner_id.clone(),
                runner_summary: observation.summary.clone(),
                runner_revision_refs: observation.revision_refs.clone(),
                git_revision_refs: git_observation.revision_refs.clone(),
                artifact_refs: observation.artifact_refs.clone(),
                verification_refs: observation.verification_refs.clone(),
                git_verification_refs: git_observation.verification_refs.clone(),
            })
        }
        TargetRepoRunnerPullRequestDisposition::Reuse => {
            if git_mutation_observation.is_some() {
                return Err(TargetRepoRunnerRuntimeError::CommandValidation {
                    operation: "pull_request",
                    message:
                        "git mutation readback must not be supplied when reusing a pull request"
                            .to_owned(),
                });
            }
            let existing_pull_request =
                dedupe_execution
                    .existing_pull_request
                    .clone()
                    .ok_or_else(|| TargetRepoRunnerRuntimeError::CommandValidation {
                        operation: "pull_request",
                        message: "existing pull request is required for reuse".to_owned(),
                    })?;
            TargetRepoRunnerPullRequestMutation::Reuse(TargetRepoRunnerPullRequestReuseCommand {
                existing_pull_request,
                reason: "Provider dedupe returned a matching open pull request.".to_owned(),
            })
        }
    };

    Ok(TargetRepoRunnerPullRequestMutationCommand {
        provider: execution_plan.provider_lookup.provider,
        disposition,
        target_repo: execution_plan.checkout.target_repo.clone(),
        repository,
        target_repo_ref: execution_plan.target_repo_ref.clone(),
        base_branch: execution_plan.checkout.base_branch.clone(),
        dedupe_key: execution_plan.provider_lookup.key.clone(),
        source_issue_ref: execution_plan.source_issue_ref.clone(),
        source_thread_ref: execution_plan.source_thread_ref.clone(),
        mutation,
        human_merge_gate_required: true,
        local_path_hidden: true,
    })
}

fn pull_request_create_title(execution_plan: &TargetRepoRunnerExecutionPlan) -> String {
    format!(
        "Runx target update for {}",
        execution_plan.checkout.target_repo
    )
}

fn pull_request_create_body(
    execution_plan: &TargetRepoRunnerExecutionPlan,
    dedupe_execution: &TargetRepoRunnerDedupeLookupExecution,
    runner_observation: &TargetRepoRunnerGovernedRunnerObservation,
    git_observation: &TargetRepoRunnerGitMutationObservation,
) -> String {
    let source_issue = execution_plan
        .source_issue_ref
        .as_ref()
        .map(|reference| reference.uri.as_str())
        .unwrap_or("none");
    let markers = execution_plan
        .provider_lookup
        .query
        .markers
        .iter()
        .map(|marker| format!("- {marker}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Runx target runner prepared this pull request for human review.\n\nTarget repo: {}\nHead branch: {}\nHead commit: {}\nSource thread: {}\nSource issue: {source_issue}\nDedupe key: {}\n\nDedupe markers:\n{markers}\n\nRunner: {}\n{}\n\nHuman review remains the merge gate.",
        execution_plan.checkout.target_repo,
        git_observation.branch,
        git_observation.head_sha,
        execution_plan.source_thread_ref.uri,
        dedupe_execution.key,
        runner_observation.runner_id,
        runner_observation.summary
    )
}

fn validate_pull_request_git_readback(
    execution_plan: &TargetRepoRunnerExecutionPlan,
    dedupe_execution: &TargetRepoRunnerDedupeLookupExecution,
    git_observation: &TargetRepoRunnerGitMutationObservation,
) -> Result<(), TargetRepoRunnerRuntimeError> {
    if git_observation.target_repo != execution_plan.checkout.target_repo {
        return Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "pull_request",
            message: "git mutation readback target repo does not match execution target".to_owned(),
        });
    }
    let expected_branch = target_repo_runner_branch_name(execution_plan, dedupe_execution);
    if git_observation.branch != expected_branch {
        return Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "pull_request",
            message: "git mutation readback branch does not match the dedupe branch".to_owned(),
        });
    }
    validate_branch_for_operation(&git_observation.branch, "pull_request")?;
    validate_head_sha(&git_observation.head_sha, "pull_request")
}

// rust-style-allow: long-function - pull-request readback validation checks all command invariants
// in one gate; keeping the checks together makes the accept/reject boundary auditable at a glance.
fn validate_pull_request_readback(
    command: &TargetRepoRunnerPullRequestMutationCommand,
    observation: &TargetRepoRunnerPullRequestObservation,
) -> Result<(), TargetRepoRunnerRuntimeError> {
    if observation.provider != command.provider {
        return Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "pull_request",
            message: "pull request readback provider does not match command".to_owned(),
        });
    }
    if observation.target_repo != command.target_repo {
        return Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "pull_request",
            message: "pull request readback target repo does not match command".to_owned(),
        });
    }
    let pull_request = &observation.pull_request;
    let url_number = github_pull_request_number(&command.repository.full_name, &pull_request.url)?;
    if let Some(readback_number) = pull_request.number {
        if readback_number != url_number {
            return Err(TargetRepoRunnerRuntimeError::CommandValidation {
                operation: "pull_request",
                message: "pull request readback number does not match its URL".to_owned(),
            });
        }
    }
    if let Some(branch) = &pull_request.branch {
        validate_pull_request_branch(branch)?;
    }

    match &command.mutation {
        TargetRepoRunnerPullRequestMutation::Create(create) => {
            let Some(head_branch) = observation.head_branch.as_deref() else {
                return Err(TargetRepoRunnerRuntimeError::CommandValidation {
                    operation: "pull_request",
                    message: "created pull request readback head branch is required".to_owned(),
                });
            };
            if head_branch != create.head_branch {
                return Err(TargetRepoRunnerRuntimeError::CommandValidation {
                    operation: "pull_request",
                    message: "created pull request readback head branch does not match command"
                        .to_owned(),
                });
            }
            let Some(head_sha) = observation.head_sha.as_deref() else {
                return Err(TargetRepoRunnerRuntimeError::CommandValidation {
                    operation: "pull_request",
                    message: "created pull request readback head sha is required".to_owned(),
                });
            };
            if head_sha != create.head_sha {
                return Err(TargetRepoRunnerRuntimeError::CommandValidation {
                    operation: "pull_request",
                    message: "created pull request readback head sha does not match command"
                        .to_owned(),
                });
            }
            match pull_request.branch.as_deref() {
                Some(branch) if branch == create.head_branch => Ok(()),
                Some(_) => Err(TargetRepoRunnerRuntimeError::CommandValidation {
                    operation: "pull_request",
                    message: "created pull request branch does not match git mutation readback"
                        .to_owned(),
                }),
                None => Err(TargetRepoRunnerRuntimeError::CommandValidation {
                    operation: "pull_request",
                    message: "created pull request branch readback is required".to_owned(),
                }),
            }
        }
        TargetRepoRunnerPullRequestMutation::Reuse(reuse) => {
            if pull_request.url != reuse.existing_pull_request.url {
                return Err(TargetRepoRunnerRuntimeError::CommandValidation {
                    operation: "pull_request",
                    message: "reused pull request readback does not match provider dedupe"
                        .to_owned(),
                });
            }
            if let Some(expected_number) = reuse.existing_pull_request.number {
                if pull_request.number != Some(expected_number) {
                    return Err(TargetRepoRunnerRuntimeError::CommandValidation {
                        operation: "pull_request",
                        message: "reused pull request number does not match provider dedupe"
                            .to_owned(),
                    });
                }
            }
            if let (Some(expected_branch), Some(readback_branch)) =
                (&reuse.existing_pull_request.branch, &pull_request.branch)
            {
                if expected_branch != readback_branch {
                    return Err(TargetRepoRunnerRuntimeError::CommandValidation {
                        operation: "pull_request",
                        message: "reused pull request branch does not match provider dedupe"
                            .to_owned(),
                    });
                }
            }
            Ok(())
        }
    }
}

fn github_pull_request_number(repo: &str, url: &str) -> Result<u64, TargetRepoRunnerRuntimeError> {
    let prefix = format!("https://github.com/{repo}/pull/");
    let Some(number) = url.strip_prefix(&prefix) else {
        return Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "pull_request",
            message: "pull request readback URL must belong to the target repo".to_owned(),
        });
    };
    let number = number.strip_suffix('/').unwrap_or(number);
    if number.is_empty() || !number.chars().all(|character| character.is_ascii_digit()) {
        return Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "pull_request",
            message: "pull request readback URL must end with a pull request number".to_owned(),
        });
    }
    number
        .parse::<u64>()
        .map_err(|error| TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "pull_request",
            message: format!("pull request readback number is invalid: {error}"),
        })
}

fn validate_pull_request_branch(branch: &str) -> Result<(), TargetRepoRunnerRuntimeError> {
    validate_branch_for_operation(branch, "pull_request")
}

fn validate_branch_for_operation(
    branch: &str,
    operation: &'static str,
) -> Result<(), TargetRepoRunnerRuntimeError> {
    if branch.trim().is_empty()
        || branch.starts_with('/')
        || branch.ends_with('/')
        || branch.contains("..")
        || branch.chars().any(|character| {
            character.is_control() || character.is_whitespace() || character == '\\'
        })
    {
        return Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation,
            message: "git branch is not a safe branch name".to_owned(),
        });
    }
    Ok(())
}

fn validate_head_sha(
    head_sha: &str,
    operation: &'static str,
) -> Result<(), TargetRepoRunnerRuntimeError> {
    if head_sha.len() != 40
        || !head_sha
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(TargetRepoRunnerRuntimeError::CommandValidation {
            operation,
            message: "head sha must be a 40 character hex commit".to_owned(),
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
) -> Result<Receipt, TargetRepoRunnerRuntimeError> {
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
        summary: &summary,
        target_repo_ref: &target_repo_ref,
        source_thread_ref: &execution.pull_request_receipt.source_thread_ref,
        source_issue_ref: execution.pull_request_receipt.source_issue_ref.as_ref(),
        pull_request_ref: &pull_request_ref,
        artifact_refs: &artifact_refs,
        verification_refs: &verification_refs,
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
    let mut receipt = Receipt {
        schema: ReceiptSchema::V1,
        id: receipt_id.clone(),
        created_at: created_at.to_owned(),
        canonicalization: RECEIPT_CANONICALIZATION.to_owned(),
        issuer: local_issuer(),
        signature: runx_contracts::ReceiptSignature {
            alg: SignatureAlgorithm::Ed25519,
            value: "sig:pending".to_owned(),
        },
        digest: "sha256:pending".to_owned(),
        idempotency: ReceiptIdempotency {
            intent_key: stable_hash(&format!(
                "target-runner:{}:{}",
                execution.execution_plan.checkout.target_repo, execution.dedupe_execution.key
            )),
            trigger_fingerprint: stable_hash(&execution.dedupe_execution.key),
            content_hash: stable_hash(&receipt_id),
        },
        subject: Subject {
            kind: ReceiptSubjectKind::Skill,
            reference: Reference::runx(ReferenceType::Harness, "target-runner"),
            commitments: Vec::new(),
        },
        authority: target_runner_authority(execution),
        acts: vec![act],
        seal,
        lineage: Some(Lineage {
            signal_refs: vec![execution.execution_plan.source_thread_ref.clone()],
            ..Lineage::default()
        }),
        metadata: Some(revision_receipt_metadata(execution)),
    };
    seal_revision_receipt(&mut receipt)?;
    Ok(receipt)
}

fn target_repo_runner_source_publication_request(
    execution: &TargetRepoRunnerFixtureExecution,
    revision_receipt: &Receipt,
    revision_projection: &TargetRepoRunnerRevisionReceiptProjection,
) -> TargetRepoRunnerSourcePublicationRequest {
    let publication = execution.source_publication_receipt.clone();
    let revision_receipt_ref = Reference::runx(ReferenceType::Receipt, &revision_receipt.id);
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
fn target_repo_runner_source_publication_receipt_node(
    request: &TargetRepoRunnerSourcePublicationRequest,
    observation: &TargetRepoRunnerSourcePublicationObservation,
    created_at: &str,
) -> Result<Receipt, TargetRepoRunnerRuntimeError> {
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
    // Role refs the projection reconstructs: PR (source role) + thread/issue
    // (target role) ride on the act's artifact_refs alongside published refs.
    let mut role_refs = vec![observation.pull_request_ref.clone()];
    role_refs.extend(target_refs.clone());
    role_refs.extend(observation.published_refs.clone());
    let act = ReceiptAct {
        id: act_id.to_owned(),
        form: ActForm::Reply,
        summary: summary.clone(),
        criteria: vec![ReceiptCriterion {
            criterion_id: criterion_id.to_owned(),
            status: CriterionStatus::Verified,
            evidence_refs: evidence_refs.clone(),
            verification_refs: Vec::new(),
            summary: Some(summary.clone()),
        }],
        by: None,
        artifact_refs: role_refs,
        detail_ref: Some(Reference::runx(
            ReferenceType::Act,
            &format!("{act_id}_detail"),
        )),
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
    let mut receipt = Receipt {
        schema: ReceiptSchema::V1,
        id: receipt_id.clone(),
        created_at: created_at.to_owned(),
        canonicalization: RECEIPT_CANONICALIZATION.to_owned(),
        issuer: local_issuer(),
        signature: runx_contracts::ReceiptSignature {
            alg: SignatureAlgorithm::Ed25519,
            value: "sig:pending".to_owned(),
        },
        digest: "sha256:pending".to_owned(),
        idempotency: ReceiptIdempotency {
            intent_key: stable_hash(&format!(
                "target-runner-source-publication:{}:{}",
                observation.source_thread_ref.uri, observation.pull_request_ref.uri
            )),
            trigger_fingerprint: stable_hash(&observation.pull_request_ref.uri),
            content_hash: stable_hash(&receipt_id),
        },
        subject: Subject {
            kind: ReceiptSubjectKind::Skill,
            reference: Reference::runx(ReferenceType::Harness, "target-runner-source-publication"),
            commitments: Vec::new(),
        },
        authority: source_publication_authority(request, created_at),
        acts: vec![act],
        seal,
        lineage: Some(Lineage {
            parent: Some(request.revision_receipt_ref.clone()),
            previous: Some(request.revision_receipt_ref.clone()),
            signal_refs: vec![observation.source_thread_ref.clone()],
            ..Lineage::default()
        }),
        metadata: Some(source_publication_receipt_metadata(request, observation)),
    };
    seal_revision_receipt(&mut receipt)?;
    Ok(receipt)
}

struct RevisionActInput<'a> {
    act_id: &'a str,
    criterion_id: &'a str,
    summary: &'a str,
    target_repo_ref: &'a Reference,
    source_thread_ref: &'a Reference,
    source_issue_ref: Option<&'a Reference>,
    pull_request_ref: &'a Reference,
    artifact_refs: &'a [Reference],
    verification_refs: &'a [Reference],
}

// The receipt records result bindings only; the full intent/target/surface refs
// and the revision body live behind `detail_ref` (an act-receipt). The role refs
// the projection needs (repo/PR/thread/issue) ride on the act's artifact_refs.
fn revision_act(input: RevisionActInput<'_>) -> ReceiptAct {
    let RevisionActInput {
        act_id,
        criterion_id,
        summary,
        target_repo_ref,
        source_thread_ref,
        source_issue_ref,
        pull_request_ref,
        artifact_refs,
        verification_refs,
    } = input;
    let target_refs = vec![target_repo_ref.clone(), pull_request_ref.clone()];
    let mut role_refs = vec![
        target_repo_ref.clone(),
        pull_request_ref.clone(),
        source_thread_ref.clone(),
    ];
    if let Some(source_issue_ref) = source_issue_ref {
        role_refs.push(source_issue_ref.clone());
    }
    role_refs.extend(artifact_refs.iter().cloned());
    ReceiptAct {
        id: act_id.to_owned(),
        form: ActForm::Revision,
        summary: summary.to_owned(),
        criteria: vec![ReceiptCriterion {
            criterion_id: criterion_id.to_owned(),
            status: CriterionStatus::Verified,
            evidence_refs: target_refs,
            verification_refs: verification_refs.to_vec(),
            summary: Some(summary.to_owned()),
        }],
        by: None,
        artifact_refs: role_refs,
        detail_ref: Some(Reference::runx(
            ReferenceType::Act,
            &format!("{act_id}_detail"),
        )),
    }
}

fn target_runner_authority(execution: &TargetRepoRunnerFixtureExecution) -> ReceiptAuthority {
    ReceiptAuthority {
        actor_ref: Reference::runx(ReferenceType::Principal, "target_runner"),
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
            profile_hash: stable_hash(&format!(
                "target-runner:{}",
                execution.execution_plan.checkout.target_repo
            )),
            redaction_refs: Vec::new(),
            setup_refs: Vec::new(),
            teardown_refs: Vec::new(),
        },
    }
}

fn source_publication_authority(
    request: &TargetRepoRunnerSourcePublicationRequest,
    created_at: &str,
) -> ReceiptAuthority {
    let target_repo =
        metadata_path_string(&request.publication.metadata, &["target_repo"]).unwrap_or("target");
    ReceiptAuthority {
        actor_ref: Reference::runx(ReferenceType::Principal, "target_runner_source_publication"),
        authority_proof_refs: Vec::new(),
        grant_refs: Vec::new(),
        scope_refs: Vec::new(),
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
        enforcement: ReceiptEnforcement {
            profile_hash: stable_hash(&format!(
                "target-runner-source-publication:{target_repo}"
            )),
            redaction_refs: Vec::new(),
            setup_refs: Vec::new(),
            teardown_refs: Vec::new(),
        },
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

fn receipt_seal(inputs: ReceiptSealInputs<'_>) -> Seal {
    let _ = inputs.act_id;
    let _ = inputs.artifact_refs;
    Seal {
        disposition: ClosureDisposition::Closed,
        reason_code: inputs.reason_code.to_owned(),
        summary: inputs.summary.to_owned(),
        closed_at: inputs.created_at.to_owned(),
        criteria: vec![ReceiptCriterion {
            criterion_id: inputs.criterion_id.to_owned(),
            status: CriterionStatus::Verified,
            verification_refs: inputs.verification_refs.to_vec(),
            evidence_refs: inputs.evidence_refs.to_vec(),
            summary: Some(inputs.summary.to_owned()),
        }],
    }
}

fn seal_revision_receipt(receipt: &mut Receipt) -> Result<(), TargetRepoRunnerRuntimeError> {
    let digest = canonical_receipt_body_digest(receipt)
        .map_err(|error| TargetRepoRunnerRuntimeError::Receipt(error.to_string()))?;
    receipt.digest = digest.clone();
    receipt.signature.value = format!("sig:{digest}");
    validate_receipt(receipt)
        .map_err(|verification| TargetRepoRunnerRuntimeError::Receipt(format!("{verification:?}")))
}

// rust-style-allow: long-function because projection validates a sealed receipt
// and reconstructs the public target-runner view without partial helpers.
pub fn project_target_repo_runner_revision_receipt(
    receipt: &Receipt,
) -> Result<TargetRepoRunnerRevisionReceiptProjection, TargetRepoRunnerRuntimeError> {
    if matches!(receipt.seal.disposition, ClosureDisposition::Deferred) {
        return Err(TargetRepoRunnerRuntimeError::ReceiptProjection(
            "receipt is not sealed".to_owned(),
        ));
    }
    let act = receipt
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
    let pull_request_ref = find_ref(&act.artifact_refs, ReferenceType::GithubPullRequest)
        .ok_or_else(|| {
            TargetRepoRunnerRuntimeError::ReceiptProjection(
                "pull request ref is required".to_owned(),
            )
        })?;
    let target_repo_ref =
        find_ref(&act.artifact_refs, ReferenceType::GithubRepo).ok_or_else(|| {
            TargetRepoRunnerRuntimeError::ReceiptProjection(
                "target repo ref is required".to_owned(),
            )
        })?;
    let source_thread_ref = find_ref(&act.artifact_refs, ReferenceType::SlackThread)
        .or_else(|| act.artifact_refs.first().cloned())
        .ok_or_else(|| {
            TargetRepoRunnerRuntimeError::ReceiptProjection(
                "source thread ref is required".to_owned(),
            )
        })?;
    let source_issue_ref = find_ref(&act.artifact_refs, ReferenceType::GithubIssue);
    Ok(TargetRepoRunnerRevisionReceiptProjection {
        receipt_ref: Reference::runx(ReferenceType::Receipt, &receipt.id),
        act_id: act.id.clone(),
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
    receipt: &Receipt,
) -> Result<TargetRepoRunnerSourcePublicationProjection, TargetRepoRunnerRuntimeError> {
    if matches!(receipt.seal.disposition, ClosureDisposition::Deferred) {
        return Err(TargetRepoRunnerRuntimeError::ReceiptProjection(
            "source publication receipt is not sealed".to_owned(),
        ));
    }
    let act = receipt
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
    let pull_request_ref = find_ref(&act.artifact_refs, ReferenceType::GithubPullRequest)
        .ok_or_else(|| {
            TargetRepoRunnerRuntimeError::ReceiptProjection(
                "source publication pull request ref is required".to_owned(),
            )
        })?;
    let source_thread_ref =
        find_ref(&act.artifact_refs, ReferenceType::SlackThread).ok_or_else(|| {
            TargetRepoRunnerRuntimeError::ReceiptProjection(
                "source publication thread ref is required".to_owned(),
            )
        })?;
    let source_issue_ref = find_ref(&act.artifact_refs, ReferenceType::GithubIssue);
    let published_refs = act
        .artifact_refs
        .iter()
        .filter(|reference| {
            !matches!(
                reference.reference_type,
                ReferenceType::GithubPullRequest
                    | ReferenceType::SlackThread
                    | ReferenceType::GithubIssue
            )
        })
        .cloned()
        .collect();
    Ok(TargetRepoRunnerSourcePublicationProjection {
        receipt_ref: Reference::runx(ReferenceType::Receipt, &receipt.id),
        source_issue_ref,
        source_thread_ref,
        pull_request_ref,
        published_refs,
        summary: receipt.seal.summary.clone(),
        metadata,
    })
}

fn find_ref(refs: &[Reference], reference_type: ReferenceType) -> Option<Reference> {
    refs.iter()
        .find(|reference| reference.reference_type == reference_type)
        .cloned()
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
