//! Runtime support for target-repo runner execution.

use std::fmt;

use serde::Serialize;

use runx_contracts::{
    TargetRepoRunnerDedupeLookupExecution, TargetRepoRunnerDedupeLookupObservation,
    TargetRepoRunnerDedupeResult, TargetRepoRunnerExecutionPlan,
    TargetRepoRunnerExistingPullRequest, TargetRepoRunnerPlan, TargetRepoRunnerPlanError,
    TargetRepoRunnerPullRequestDisposition, TargetRepoRunnerPullRequestReceiptPlan,
    TargetRepoRunnerReadinessObservation, TargetRepoRunnerSourcePublicationReceiptPlan,
    apply_target_repo_runner_dedupe_lookup_execution, execute_target_repo_runner_dedupe_lookup,
    plan_target_repo_runner_execution, plan_target_repo_runner_pull_request_receipt,
    plan_target_repo_runner_source_publication_receipt,
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

#[derive(Clone, Debug, PartialEq)]
pub enum TargetRepoRunnerRuntimeError {
    Plan(TargetRepoRunnerPlanError),
    ReadinessMismatch(String),
    CheckoutNotScafldReady { target_repo: String },
    CreatedPullRequestRequired { target_repo: String },
}

impl fmt::Display for TargetRepoRunnerRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Plan(error) => write!(formatter, "{error}"),
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
            Self::ReadinessMismatch(_)
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
