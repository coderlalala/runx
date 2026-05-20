//! Operational policy contract types: schemas, rules, decisions, and findings.
use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum OperationalPolicySchema {
    #[serde(rename = "runx.operational_policy.v1")]
    V1,
}

impl fmt::Display for OperationalPolicySchema {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("runx.operational_policy.v1")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OperationalPolicyAction {
    ReplyOnly,
    IssueIntake,
    WorkPlan,
    IssueToPr,
    ManualReview,
    PrReview,
    PrFixUp,
    MergeAssist,
}

impl fmt::Display for OperationalPolicyAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(action_name(*self))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OperationalPolicySourceProvider {
    Slack,
    Sentry,
    Github,
    File,
    Api,
    Other,
}

impl fmt::Display for OperationalPolicySourceProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Slack => "slack",
            Self::Sentry => "sentry",
            Self::Github => "github",
            Self::File => "file",
            Self::Api => "api",
            Self::Other => "other",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OperationalPolicyRunnerKind {
    Local,
    GithubActions,
    Aster,
    Other,
}

impl fmt::Display for OperationalPolicyRunnerKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Local => "local",
            Self::GithubActions => "github-actions",
            Self::Aster => "aster",
            Self::Other => "other",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OperationalPolicyRunnerState {
    Available,
    Disabled,
    Maintenance,
}

impl fmt::Display for OperationalPolicyRunnerState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Available => "available",
            Self::Disabled => "disabled",
            Self::Maintenance => "maintenance",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationalPolicyDedupeStrategy {
    SourceFingerprint,
    ProviderSearch,
    Branch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationalPolicySourceIssueClosureMode {
    Never,
    WhenVerified,
    WhenTerminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OperationalPolicyPublishMode {
    Reply,
    Comment,
    None,
}

impl fmt::Display for OperationalPolicyPublishMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Reply => "reply",
            Self::Comment => "comment",
            Self::None => "none",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum OperationalPolicyMissingBehavior {
    #[serde(rename = "fail_closed")]
    FailClosed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OperationalPolicyDuplicateBehavior {
    Reuse,
    Comment,
    Block,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationalPolicy {
    pub schema: OperationalPolicySchema,
    pub schema_version: OperationalPolicySchema,
    pub policy_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    pub sources: Vec<OperationalPolicySourceRule>,
    pub runners: Vec<OperationalPolicyRunnerRule>,
    pub owner_routes: Vec<OperationalPolicyOwnerRoute>,
    pub targets: Vec<OperationalPolicyTargetRule>,
    pub dedupe: OperationalPolicyDedupePolicy,
    pub post_merge: OperationalPolicyPostMergePolicy,
    pub permissions: OperationalPolicyAutomationPermissions,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationalPolicySourceRule {
    pub source_id: String,
    pub provider: OperationalPolicySourceProvider,
    pub allowed_locators: Vec<String>,
    pub allowed_actions: Vec<OperationalPolicyAction>,
    pub source_thread: OperationalPolicySourceThreadPolicy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum_confidence: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sentry: Option<OperationalPolicySentryPolicy>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationalPolicySourceThreadPolicy {
    pub required: bool,
    pub publish_mode: OperationalPolicyPublishMode,
    pub missing_behavior: OperationalPolicyMissingBehavior,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationalPolicySentryPolicy {
    pub production_only: bool,
    pub unresolved_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regressed_only: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationalPolicyRunnerRule {
    pub runner_id: String,
    pub kind: OperationalPolicyRunnerKind,
    pub state: OperationalPolicyRunnerState,
    pub allowed_actions: Vec<OperationalPolicyAction>,
    pub target_repos: Vec<String>,
    pub scafld_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationalPolicyOwnerRoute {
    pub route_id: String,
    pub owners: Vec<String>,
    pub target_repos: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationalPolicyTargetRule {
    pub repo: String,
    pub runner_ids: Vec<String>,
    pub allowed_actions: Vec<OperationalPolicyAction>,
    pub default_owner_route: String,
    pub scafld_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationalPolicyDedupePolicy {
    pub strategy: OperationalPolicyDedupeStrategy,
    pub key_fields: Vec<String>,
    pub on_duplicate: OperationalPolicyDuplicateBehavior,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationalPolicyPostMergePolicy {
    pub observe_provider: bool,
    pub verification_required: bool,
    pub source_issue_closure_mode: OperationalPolicySourceIssueClosureMode,
    pub publish_source_thread_closure_update: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationalPolicyAutomationPermissions {
    pub auto_merge: bool,
    pub mutate_target_repo: bool,
    pub require_human_merge_gate: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct OperationalPolicyValidationFinding {
    pub code: String,
    pub path: String,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationalPolicyAdmissionRequest {
    pub source_id: Option<String>,
    pub target_repo: Option<String>,
    pub action: OperationalPolicyAction,
    pub runner_id: Option<String>,
    pub source_thread_locator: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OperationalPolicyAdmission {
    pub status: OperationalPolicyAdmissionStatus,
    pub findings: Vec<OperationalPolicyValidationFinding>,
    pub policy_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_route_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owners: Option<Vec<String>>,
    pub dedupe_strategy: OperationalPolicyDedupeStrategy,
    pub source_issue_closure_mode: OperationalPolicySourceIssueClosureMode,
    pub source_thread_required: bool,
    pub mutate_target_repo: bool,
    pub require_human_merge_gate: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OperationalPolicyAdmissionStatus {
    Allow,
    Deny,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationalPolicyError {
    Contract(OperationalPolicyValidationFinding),
    Semantic(OperationalPolicyValidationFinding),
}

impl OperationalPolicyError {
    pub fn finding(&self) -> &OperationalPolicyValidationFinding {
        match self {
            Self::Contract(finding) | Self::Semantic(finding) => finding,
        }
    }
}

impl fmt::Display for OperationalPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let finding = self.finding();
        write!(
            formatter,
            "{} failed validation ({}): {}",
            finding.path, finding.code, finding.message
        )
    }
}

impl std::error::Error for OperationalPolicyError {}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OperationalPolicyReadback {
    pub policy_id: String,
    pub schema_version: OperationalPolicySchema,
    pub valid: bool,
    pub findings: Vec<OperationalPolicyValidationFinding>,
    pub sources: Vec<OperationalPolicySourceReadback>,
    pub runners: Vec<OperationalPolicyRunnerReadback>,
    pub targets: Vec<OperationalPolicyTargetReadback>,
    pub post_merge: OperationalPolicyPostMergePolicy,
    pub permissions: OperationalPolicyAutomationPermissions,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OperationalPolicySourceReadback {
    pub source_id: String,
    pub provider: OperationalPolicySourceProvider,
    pub locator_count: usize,
    pub allowed_actions: Vec<OperationalPolicyAction>,
    pub source_thread_required: bool,
    pub publish_mode: OperationalPolicyPublishMode,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OperationalPolicyRunnerReadback {
    pub runner_id: String,
    pub kind: OperationalPolicyRunnerKind,
    pub state: OperationalPolicyRunnerState,
    pub target_repos: Vec<String>,
    pub allowed_actions: Vec<OperationalPolicyAction>,
    pub scafld_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OperationalPolicyTargetReadback {
    pub repo: String,
    pub runner_ids: Vec<String>,
    pub default_owner_route: String,
    pub owner_count: usize,
    pub allowed_actions: Vec<OperationalPolicyAction>,
    pub scafld_required: bool,
    pub available_runner_count: usize,
}

pub(super) fn action_name(action: OperationalPolicyAction) -> &'static str {
    match action {
        OperationalPolicyAction::ReplyOnly => "reply-only",
        OperationalPolicyAction::IssueIntake => "issue-intake",
        OperationalPolicyAction::WorkPlan => "work-plan",
        OperationalPolicyAction::IssueToPr => "issue-to-pr",
        OperationalPolicyAction::ManualReview => "manual-review",
        OperationalPolicyAction::PrReview => "pr-review",
        OperationalPolicyAction::PrFixUp => "pr-fix-up",
        OperationalPolicyAction::MergeAssist => "merge-assist",
    }
}
