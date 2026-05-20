//! Operational policy contracts for governed source, runner, target, and owner routing.

mod types;
mod validation;

pub use types::{
    OperationalPolicy, OperationalPolicyAction, OperationalPolicyAdmission,
    OperationalPolicyAdmissionRequest, OperationalPolicyAdmissionStatus,
    OperationalPolicyAutomationPermissions, OperationalPolicyDedupePolicy,
    OperationalPolicyDedupeStrategy, OperationalPolicyDuplicateBehavior, OperationalPolicyError,
    OperationalPolicyMissingBehavior, OperationalPolicyOwnerRoute,
    OperationalPolicyPostMergePolicy, OperationalPolicyPublishMode, OperationalPolicyReadback,
    OperationalPolicyRunnerKind, OperationalPolicyRunnerReadback, OperationalPolicyRunnerRule,
    OperationalPolicyRunnerState, OperationalPolicySchema, OperationalPolicySentryPolicy,
    OperationalPolicySourceIssueClosureMode, OperationalPolicySourceProvider,
    OperationalPolicySourceReadback, OperationalPolicySourceRule,
    OperationalPolicySourceThreadPolicy, OperationalPolicyTargetReadback,
    OperationalPolicyTargetRule, OperationalPolicyValidationFinding,
};
pub use validation::{
    admit_operational_policy_request, lint_operational_policy_contract,
    project_operational_policy_readback, validate_operational_policy_contract,
    validate_operational_policy_semantics,
};
