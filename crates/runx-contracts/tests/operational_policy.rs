use runx_contracts::{
    OperationalPolicy, OperationalPolicyAction, OperationalPolicyAdmissionRequest,
    OperationalPolicyAdmissionStatus, OperationalPolicyDedupeStrategy, OperationalPolicySchema,
    OperationalPolicySourceIssueClosureMode, admit_operational_policy_request,
    lint_operational_policy_contract, project_operational_policy_readback,
    validate_operational_policy_contract, validate_operational_policy_semantics,
};

const NITROSEND_LIKE: &str =
    include_str!("../../../fixtures/operational-policy/nitrosend-like.json");
const MINIMAL_SINGLE_REPO: &str =
    include_str!("../../../fixtures/operational-policy/minimal-single-repo.json");
const INVALID_UNKNOWN_RUNNER: &str =
    include_str!("../../../fixtures/operational-policy/invalid-unknown-runner.json");
const INVALID_OWNER_ROUTE_MISMATCH: &str =
    include_str!("../../../fixtures/operational-policy/invalid-owner-route-mismatch.json");
const INVALID_SOURCE_THREAD_MISSING: &str =
    include_str!("../../../fixtures/operational-policy/invalid-source-thread-missing.json");
const INVALID_NO_AVAILABLE_RUNNER: &str =
    include_str!("../../../fixtures/operational-policy/invalid-no-available-runner.json");
const INVALID_SCHEMA_LITERAL: &str =
    include_str!("../../../fixtures/operational-policy/invalid-schema-literal.json");
const INVALID_SECRET_FIELD: &str =
    include_str!("../../../fixtures/operational-policy/invalid-secret-field.json");

#[test]
fn positive_operational_policy_fixtures_are_valid() -> Result<(), Box<dyn std::error::Error>> {
    for fixture in [NITROSEND_LIKE, MINIMAL_SINGLE_REPO] {
        let policy: OperationalPolicy = serde_json::from_str(fixture)?;

        validate_operational_policy_contract(&policy)?;
        validate_operational_policy_semantics(&policy)?;
        assert!(lint_operational_policy_contract(&policy)?.is_empty());
        assert_eq!(policy.schema, OperationalPolicySchema::V1);
        assert_eq!(
            policy.schema_version.to_string(),
            "runx.operational_policy.v1"
        );
    }
    Ok(())
}

#[test]
fn semantic_fixture_findings_are_stable() -> Result<(), Box<dyn std::error::Error>> {
    for (fixture, code) in [
        (INVALID_UNKNOWN_RUNNER, "unknown_runner"),
        (INVALID_OWNER_ROUTE_MISMATCH, "owner_route_target_mismatch"),
        (INVALID_SOURCE_THREAD_MISSING, "source_thread_required"),
        (INVALID_NO_AVAILABLE_RUNNER, "target_action_without_runner"),
    ] {
        let policy: OperationalPolicy = serde_json::from_str(fixture)?;
        let findings = lint_operational_policy_contract(&policy)?;

        assert!(findings.iter().any(|finding| finding.code == code));
        assert!(validate_operational_policy_semantics(&policy).is_err());
    }
    Ok(())
}

#[test]
fn schema_invalid_fixtures_are_rejected() {
    assert!(serde_json::from_str::<OperationalPolicy>(INVALID_SCHEMA_LITERAL).is_err());
    assert!(serde_json::from_str::<OperationalPolicy>(INVALID_SECRET_FIELD).is_err());
}

#[test]
fn invalid_created_at_is_rejected_like_typescript_schema() -> Result<(), Box<dyn std::error::Error>>
{
    let mut policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;

    policy.created_at = Some("2026-05-19 00:00:00".to_owned());
    let missing_t = validate_operational_policy_contract(&policy);

    policy.created_at = Some("2026-05-19T00:00:00+10:00".to_owned());
    let offset = validate_operational_policy_contract(&policy);

    assert!(missing_t.is_err());
    assert!(offset.is_err());
    Ok(())
}

#[test]
fn readback_redacts_source_locators() -> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let readback = project_operational_policy_readback(&policy)?;
    let json = serde_json::to_string(&readback)?;

    assert!(readback.valid);
    assert_eq!(readback.sources[0].locator_count, 1);
    assert!(json.contains(r#""locator_count":1"#));
    assert!(!json.contains("slack://nitrosend"));
    Ok(())
}

#[test]
fn nitrosend_policy_admits_each_target_repo_route() -> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;

    for repo in ["nitrosend/nitrosend", "nitrosend/api", "nitrosend/app"] {
        let admission = admit_operational_policy_request(
            &policy,
            &OperationalPolicyAdmissionRequest {
                source_id: Some("bugs-fixes".to_owned()),
                target_repo: Some(repo.to_owned()),
                action: OperationalPolicyAction::IssueToPr,
                runner_id: None,
                source_thread_locator: Some(
                    "slack://nitrosend/C0APFMY0V8Q/1778834840.485629".to_owned(),
                ),
            },
        )?;

        assert_eq!(admission.status, OperationalPolicyAdmissionStatus::Allow);
        assert!(admission.findings.is_empty());
        assert_eq!(admission.policy_id, "nitrosend-issue-flow");
        assert_eq!(admission.source_id.as_deref(), Some("bugs-fixes"));
        assert_eq!(admission.target_repo.as_deref(), Some(repo));
        assert_eq!(admission.runner_id.as_deref(), Some("aster-production"));
        assert_eq!(admission.owner_route_id.as_deref(), Some("product-surface"));
        assert_eq!(admission.owners.as_deref(), Some(&["Kam".to_owned()][..]));
        assert_eq!(
            admission.dedupe_strategy,
            OperationalPolicyDedupeStrategy::SourceFingerprint
        );
        assert_eq!(
            admission.source_issue_closure_mode,
            OperationalPolicySourceIssueClosureMode::WhenVerified
        );
        assert!(admission.source_thread_required);
        assert!(admission.mutate_target_repo);
        assert!(admission.require_human_merge_gate);
    }

    Ok(())
}

#[test]
fn nitrosend_policy_denies_unknown_target_before_runner_selection()
-> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let admission = admit_operational_policy_request(
        &policy,
        &OperationalPolicyAdmissionRequest {
            source_id: Some("bugs-fixes".to_owned()),
            target_repo: Some("nitrosend/unknown".to_owned()),
            action: OperationalPolicyAction::IssueToPr,
            runner_id: None,
            source_thread_locator: Some(
                "slack://nitrosend/C0APFMY0V8Q/1778834840.485629".to_owned(),
            ),
        },
    )?;

    assert_eq!(admission.status, OperationalPolicyAdmissionStatus::Deny);
    assert!(admission.target_repo.is_none());
    assert!(admission.runner_id.is_none());
    assert!(
        admission
            .findings
            .iter()
            .any(|finding| finding.code == "unknown_target_repo")
    );
    Ok(())
}

#[test]
fn nitrosend_policy_denies_pr_admission_without_source_thread()
-> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let admission = admit_operational_policy_request(
        &policy,
        &OperationalPolicyAdmissionRequest {
            source_id: Some("bugs-fixes".to_owned()),
            target_repo: Some("nitrosend/api".to_owned()),
            action: OperationalPolicyAction::IssueToPr,
            runner_id: Some("aster-production".to_owned()),
            source_thread_locator: None,
        },
    )?;

    assert_eq!(admission.status, OperationalPolicyAdmissionStatus::Deny);
    assert_eq!(admission.target_repo.as_deref(), Some("nitrosend/api"));
    assert_eq!(admission.runner_id.as_deref(), Some("aster-production"));
    assert!(
        admission
            .findings
            .iter()
            .any(|finding| finding.code == "source_thread_locator_required")
    );
    Ok(())
}

#[test]
fn typed_action_names_match_contract_literals() {
    assert_eq!(
        OperationalPolicyAction::IssueToPr.to_string(),
        "issue-to-pr"
    );
    assert_eq!(OperationalPolicyAction::PrFixUp.to_string(), "pr-fix-up");
    assert_eq!(
        OperationalPolicyAction::MergeAssist.to_string(),
        "merge-assist"
    );
}
