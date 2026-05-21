use runx_contracts::{
    ActForm, JsonValue, OperationalPolicy, OperationalPolicyAction, OperationalPolicyPublishMode,
    TargetRepoRunnerDedupeLookupObservation, TargetRepoRunnerDedupeResult,
    TargetRepoRunnerExistingPullRequest, TargetRepoRunnerPlan, TargetRepoRunnerPlanError,
    TargetRepoRunnerPlanRequest, TargetRepoRunnerProvider, TargetRepoRunnerProviderPullRequest,
    TargetRepoRunnerPullRequestDisposition, TargetRepoRunnerReadinessObservation,
    TargetRepoRunnerSourceContext, apply_target_repo_runner_dedupe_lookup_execution,
    execute_target_repo_runner_dedupe_lookup, plan_target_repo_runner,
    plan_target_repo_runner_dedupe_lookup, plan_target_repo_runner_execution,
    plan_target_repo_runner_pull_request_receipt,
    plan_target_repo_runner_source_publication_receipt,
};

const NITROSEND_LIKE: &str =
    include_str!("../../../fixtures/operational-policy/nitrosend-like.json");
const MINIMAL_SINGLE_REPO: &str =
    include_str!("../../../fixtures/operational-policy/minimal-single-repo.json");
const INVALID_UNKNOWN_RUNNER: &str =
    include_str!("../../../fixtures/operational-policy/invalid-unknown-runner.json");
const INVALID_NOT_SCAFLD_TARGET: &str =
    include_str!("../../../fixtures/operational-policy/invalid-not-scafld-target.json");

#[test]
fn plans_nitrosend_target_runner_with_dedupe_and_source_thread()
-> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let plan = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/api", None))?;

    assert_eq!(plan.policy_id, "nitrosend-issue-flow");
    assert_eq!(plan.action, OperationalPolicyAction::IssueToPr);
    assert_eq!(plan.source.source_id, "bugs-fixes");
    assert_eq!(plan.source.locator, "slack://nitrosend/C0APFMY0V8Q");
    assert_eq!(
        plan.source.issue_url.as_deref(),
        Some("https://github.com/nitrosend/nitrosend/issues/482")
    );
    assert!(plan.source_thread.required);
    assert_eq!(
        plan.source_thread.publish_mode,
        OperationalPolicyPublishMode::Reply
    );
    assert_eq!(
        plan.source_thread.locator,
        "slack://nitrosend/C0APFMY0V8Q/1778834840.485629"
    );
    assert_eq!(plan.target.repo, "nitrosend/api");
    assert_eq!(plan.target.base_branch.as_deref(), Some("main"));
    assert!(plan.target.scafld_required);
    assert_eq!(plan.runner.runner_id, "aster-production");
    assert!(plan.runner.scafld_required);
    assert_eq!(plan.owner.route_id, "product-surface");
    assert_eq!(plan.owner.owners, vec!["Kam".to_owned()]);
    assert_eq!(
        plan.dedupe.result,
        TargetRepoRunnerDedupeResult::LookupRequired
    );
    assert!(plan.dedupe.key.starts_with("source_fingerprint:"));
    assert_eq!(component_value(&plan, "target_repo"), Some("nitrosend/api"));
    assert_eq!(
        component_value(&plan, "source.thread_ts"),
        Some("1778834840.485629")
    );
    assert_eq!(
        component_value(&plan, "signal.fingerprint"),
        Some("sha256:nitrosend-source-482")
    );
    assert!(plan.mutate_target_repo);
    assert!(plan.require_human_merge_gate);

    let json = serde_json::to_string(&plan)?;
    assert!(json.contains(r#""target_repo""#));
    assert!(!json.contains("/Users/"));
    assert!(!json.contains("/tmp/"));
    Ok(())
}

#[test]
fn dedupe_key_is_scoped_by_target_repo() -> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let api_plan = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/api", None))?;
    let app_plan = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/app", None))?;

    assert_ne!(api_plan.dedupe.key, app_plan.dedupe.key);
    assert_eq!(
        component_value(&api_plan, "source.locator"),
        component_value(&app_plan, "source.locator")
    );
    assert_eq!(
        component_value(&api_plan, "signal.fingerprint"),
        component_value(&app_plan, "signal.fingerprint")
    );
    assert_eq!(
        component_value(&api_plan, "target_repo"),
        Some("nitrosend/api")
    );
    assert_eq!(
        component_value(&app_plan, "target_repo"),
        Some("nitrosend/app")
    );
    Ok(())
}

#[test]
fn same_repo_issue_to_pr_plans_through_target_runner_contract()
-> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(MINIMAL_SINGLE_REPO)?;
    let plan = plan_target_repo_runner(
        &policy,
        &fixture_request("github-issues", "example/project"),
    )?;

    assert_eq!(plan.policy_id, "single-repo-review-flow");
    assert_eq!(plan.action, OperationalPolicyAction::IssueToPr);
    assert_eq!(plan.source.source_id, "github-issues");
    assert_eq!(plan.target.repo, "example/project");
    assert_eq!(plan.runner.runner_id, "local-review");
    assert_eq!(plan.owner.route_id, "maintainers");
    assert!(plan.source_thread.required);
    assert!(plan.target.scafld_required);
    assert!(plan.runner.scafld_required);
    assert!(plan.mutate_target_repo);
    assert!(plan.require_human_merge_gate);

    let execution = plan_target_repo_runner_execution(
        &plan,
        &TargetRepoRunnerReadinessObservation {
            target_repo: "example/project".to_owned(),
            runner_id: "local-review".to_owned(),
            scafld_ready: true,
        },
    )?;
    assert_eq!(
        execution.target_repo_ref.uri,
        "https://github.com/example/project"
    );
    assert_eq!(
        execution.source_thread_ref.uri,
        "github://example/project/issues/42"
    );
    assert!(execution.checkout.local_path_hidden);

    let json = serde_json::to_string(&execution)?;
    assert!(!json.contains("/Users/"));
    assert!(!json.contains("/tmp/"));
    Ok(())
}

#[test]
fn existing_pull_request_marks_dedupe_reuse_without_changing_key()
-> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let pending = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/api", None))?;
    let reused = plan_target_repo_runner(
        &policy,
        &nitrosend_request(
            "nitrosend/api",
            Some(TargetRepoRunnerExistingPullRequest {
                url: "https://github.com/nitrosend/api/pull/144".to_owned(),
                number: Some(144),
                branch: Some("runx/source-482".to_owned()),
            }),
        ),
    )?;

    assert_eq!(reused.dedupe.key, pending.dedupe.key);
    assert_eq!(reused.dedupe.result, TargetRepoRunnerDedupeResult::Reused);
    assert_eq!(
        reused
            .dedupe
            .existing_pull_request
            .as_ref()
            .map(|pull_request| pull_request.url.as_str()),
        Some("https://github.com/nitrosend/api/pull/144")
    );
    Ok(())
}

#[test]
fn provider_dedupe_lookup_carries_source_refs_before_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let plan = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/api", None))?;
    let lookup = plan_target_repo_runner_dedupe_lookup(&plan);

    assert_eq!(lookup.provider, TargetRepoRunnerProvider::Github);
    assert_eq!(lookup.target_repo, "nitrosend/api");
    assert_eq!(lookup.key, plan.dedupe.key);
    assert_eq!(lookup.components, plan.dedupe.components);
    assert_eq!(lookup.result, TargetRepoRunnerDedupeResult::LookupRequired);
    assert!(lookup.existing_pull_request.is_none());
    assert!(
        lookup
            .query
            .markers
            .iter()
            .any(|marker| { marker == &format!("runx-dedupe-key:{}", plan.dedupe.key) })
    );
    assert!(
        lookup
            .query
            .markers
            .iter()
            .any(|marker| { marker == "runx-dedupe:target_repo=nitrosend/api" })
    );
    assert_eq!(
        lookup.source_thread_ref.reference_type,
        runx_contracts::ReferenceType::SlackThread
    );
    assert_eq!(
        lookup.source_thread_ref.uri,
        "slack://nitrosend/C0APFMY0V8Q/1778834840.485629"
    );
    assert_eq!(
        lookup
            .source_issue_ref
            .as_ref()
            .map(|reference| reference.uri.as_str()),
        Some("https://github.com/nitrosend/nitrosend/issues/482")
    );
    assert_eq!(lookup.query.required_refs.len(), 2);

    let json = serde_json::to_string(&lookup)?;
    assert!(!json.contains("/Users/"));
    assert!(!json.contains("/tmp/"));
    Ok(())
}

#[test]
fn provider_dedupe_lookup_preserves_reused_pr_state() -> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let plan = plan_target_repo_runner(
        &policy,
        &nitrosend_request(
            "nitrosend/api",
            Some(TargetRepoRunnerExistingPullRequest {
                url: "https://github.com/nitrosend/api/pull/144".to_owned(),
                number: Some(144),
                branch: Some("runx/source-482".to_owned()),
            }),
        ),
    )?;
    let lookup = plan_target_repo_runner_dedupe_lookup(&plan);

    assert_eq!(lookup.key, plan.dedupe.key);
    assert_eq!(lookup.result, TargetRepoRunnerDedupeResult::Reused);
    assert_eq!(
        lookup
            .existing_pull_request
            .as_ref()
            .and_then(|pull_request| pull_request.number),
        Some(144)
    );
    Ok(())
}

#[test]
fn target_runner_execution_requires_scafld_ready_checkout() -> Result<(), Box<dyn std::error::Error>>
{
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let plan = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/api", None))?;

    let error = plan_target_repo_runner_execution(
        &plan,
        &TargetRepoRunnerReadinessObservation {
            target_repo: "nitrosend/api".to_owned(),
            runner_id: "aster-production".to_owned(),
            scafld_ready: false,
        },
    )
    .err();
    assert!(matches!(
        error,
        Some(TargetRepoRunnerPlanError::NotScafldReady { .. })
    ));

    let execution = plan_target_repo_runner_execution(
        &plan,
        &TargetRepoRunnerReadinessObservation {
            target_repo: "nitrosend/api".to_owned(),
            runner_id: "aster-production".to_owned(),
            scafld_ready: true,
        },
    )?;

    assert_eq!(execution.checkout.target_repo, "nitrosend/api");
    assert_eq!(
        execution.checkout.public_repo_ref.uri,
        "https://github.com/nitrosend/api"
    );
    assert!(execution.checkout.local_path_hidden);
    assert_eq!(execution.readiness.runner_id, "aster-production");
    assert!(execution.readiness.scafld_ready);
    assert_eq!(
        execution.source_thread_ref.uri,
        "slack://nitrosend/C0APFMY0V8Q/1778834840.485629"
    );

    let json = serde_json::to_string(&execution)?;
    assert!(!json.contains("/Users/"));
    assert!(!json.contains("/tmp/"));
    Ok(())
}

#[test]
fn provider_lookup_execution_reuses_existing_pr_with_required_refs()
-> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let pending = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/api", None))?;
    let lookup = plan_target_repo_runner_dedupe_lookup(&pending);
    let execution = execute_target_repo_runner_dedupe_lookup(
        &lookup,
        &TargetRepoRunnerDedupeLookupObservation {
            provider: TargetRepoRunnerProvider::Github,
            target_repo: "nitrosend/api".to_owned(),
            key: lookup.key.clone(),
            pull_requests: vec![TargetRepoRunnerProviderPullRequest {
                url: "https://github.com/nitrosend/api/pull/144".to_owned(),
                number: Some(144),
                branch: Some("runx/source-482".to_owned()),
                open: true,
                markers: lookup.query.markers.clone(),
                refs: lookup.query.required_refs.clone(),
            }],
        },
    )?;

    assert_eq!(execution.result, TargetRepoRunnerDedupeResult::Reused);
    assert!(execution.matched_required_refs);
    assert_eq!(
        execution
            .existing_pull_request
            .as_ref()
            .and_then(|pull_request| pull_request.number),
        Some(144)
    );

    let reused = apply_target_repo_runner_dedupe_lookup_execution(&pending, &execution)?;
    assert_eq!(reused.dedupe.key, pending.dedupe.key);
    assert_eq!(reused.dedupe.result, TargetRepoRunnerDedupeResult::Reused);
    assert_eq!(
        reused
            .dedupe
            .existing_pull_request
            .as_ref()
            .map(|pull_request| pull_request.url.as_str()),
        Some("https://github.com/nitrosend/api/pull/144")
    );
    Ok(())
}

#[test]
fn provider_lookup_execution_ignores_candidates_without_source_thread_refs()
-> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let plan = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/api", None))?;
    let lookup = plan_target_repo_runner_dedupe_lookup(&plan);
    let execution = execute_target_repo_runner_dedupe_lookup(
        &lookup,
        &TargetRepoRunnerDedupeLookupObservation {
            provider: TargetRepoRunnerProvider::Github,
            target_repo: "nitrosend/api".to_owned(),
            key: lookup.key.clone(),
            pull_requests: vec![TargetRepoRunnerProviderPullRequest {
                url: "https://github.com/nitrosend/api/pull/145".to_owned(),
                number: Some(145),
                branch: Some("runx/source-482-duplicate".to_owned()),
                open: true,
                markers: lookup.query.markers.clone(),
                refs: Vec::new(),
            }],
        },
    )?;

    assert_eq!(
        execution.result,
        TargetRepoRunnerDedupeResult::LookupRequired
    );
    assert!(!execution.matched_required_refs);
    assert!(execution.existing_pull_request.is_none());
    Ok(())
}

#[test]
fn pull_request_receipt_metadata_records_dedupe_and_source_thread()
-> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let plan = plan_target_repo_runner(
        &policy,
        &nitrosend_request(
            "nitrosend/api",
            Some(TargetRepoRunnerExistingPullRequest {
                url: "https://github.com/nitrosend/api/pull/144".to_owned(),
                number: Some(144),
                branch: Some("runx/source-482".to_owned()),
            }),
        ),
    )?;
    let receipt = plan_target_repo_runner_pull_request_receipt(&plan, None)?;

    assert_eq!(receipt.act_form, ActForm::Revision);
    assert_eq!(
        receipt.disposition,
        TargetRepoRunnerPullRequestDisposition::Reuse
    );
    assert_eq!(
        receipt.target_repo_ref.uri,
        "https://github.com/nitrosend/api"
    );
    assert_eq!(
        receipt.source_thread_ref.uri,
        "slack://nitrosend/C0APFMY0V8Q/1778834840.485629"
    );
    assert_eq!(
        receipt
            .pull_request_ref
            .as_ref()
            .map(|reference| reference.uri.as_str()),
        Some("https://github.com/nitrosend/api/pull/144")
    );
    assert_eq!(
        nested_string(&receipt.metadata, &["dedupe", "strategy"]),
        Some("source_fingerprint")
    );
    assert_eq!(
        nested_string(&receipt.metadata, &["dedupe", "result"]),
        Some("reused")
    );
    assert_eq!(
        nested_string(&receipt.metadata, &["source", "thread_uri"]),
        Some("slack://nitrosend/C0APFMY0V8Q/1778834840.485629")
    );

    let json = serde_json::to_string(&receipt)?;
    assert!(!json.contains("/Users/"));
    assert!(!json.contains("/tmp/"));
    Ok(())
}

#[test]
fn created_pull_request_receipt_metadata_records_created_dedupe_result()
-> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let plan = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/api", None))?;
    let receipt = plan_target_repo_runner_pull_request_receipt(
        &plan,
        Some(&TargetRepoRunnerExistingPullRequest {
            url: "https://github.com/nitrosend/api/pull/145".to_owned(),
            number: Some(145),
            branch: Some("runx/source-482-new".to_owned()),
        }),
    )?;

    assert_eq!(receipt.act_form, ActForm::Revision);
    assert_eq!(
        receipt.disposition,
        TargetRepoRunnerPullRequestDisposition::Create
    );
    assert_eq!(
        nested_string(&receipt.metadata, &["dedupe", "result"]),
        Some("created")
    );
    assert_eq!(
        receipt.metadata.get("disposition"),
        Some(&JsonValue::String("created".to_owned()))
    );
    assert_eq!(
        nested_string(&receipt.metadata, &["source", "thread_uri"]),
        Some("slack://nitrosend/C0APFMY0V8Q/1778834840.485629")
    );
    Ok(())
}

#[test]
fn source_publication_receipt_carries_original_thread_and_target_pr()
-> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let plan = plan_target_repo_runner(
        &policy,
        &nitrosend_request(
            "nitrosend/api",
            Some(TargetRepoRunnerExistingPullRequest {
                url: "https://github.com/nitrosend/api/pull/144".to_owned(),
                number: Some(144),
                branch: Some("runx/source-482".to_owned()),
            }),
        ),
    )?;
    let publication = plan_target_repo_runner_source_publication_receipt(
        &plan,
        &TargetRepoRunnerExistingPullRequest {
            url: "https://github.com/nitrosend/api/pull/144".to_owned(),
            number: Some(144),
            branch: Some("runx/source-482".to_owned()),
        },
    );

    assert_eq!(
        publication
            .source_issue_ref
            .as_ref()
            .map(|reference| reference.uri.as_str()),
        Some("https://github.com/nitrosend/nitrosend/issues/482")
    );
    assert_eq!(
        publication
            .source_issue_ref
            .as_ref()
            .and_then(|reference| reference.provider.as_deref()),
        Some("github")
    );
    assert_eq!(
        publication
            .source_issue_ref
            .as_ref()
            .and_then(|reference| reference.locator.as_deref()),
        Some("nitrosend/nitrosend#482")
    );
    assert_eq!(
        publication.source_thread_ref.uri,
        "slack://nitrosend/C0APFMY0V8Q/1778834840.485629"
    );
    assert_eq!(
        publication.pull_request_ref.uri,
        "https://github.com/nitrosend/api/pull/144"
    );
    assert_eq!(
        nested_string(&publication.metadata, &["source", "thread_uri"]),
        Some("slack://nitrosend/C0APFMY0V8Q/1778834840.485629")
    );
    assert_eq!(
        publication.metadata.get("target_pull_request_url"),
        Some(&JsonValue::String(
            "https://github.com/nitrosend/api/pull/144".to_owned()
        ))
    );
    assert_eq!(
        nested_string(&publication.metadata, &["dedupe", "strategy"]),
        Some("source_fingerprint")
    );
    assert_eq!(
        nested_string(&publication.metadata, &["dedupe", "key"]),
        Some(plan.dedupe.key.as_str())
    );
    assert_eq!(
        nested_string(&publication.metadata, &["dedupe", "result"]),
        Some("reused")
    );
    assert_eq!(
        publication.metadata.get("disposition"),
        Some(&JsonValue::String("reused".to_owned()))
    );
    Ok(())
}

#[test]
fn unknown_target_denies_before_plan_materializes() -> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let error =
        plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/unknown", None)).err();

    let Some(TargetRepoRunnerPlanError::AdmissionDenied(admission)) = error else {
        return Err("expected admission denied planning error".into());
    };

    let admission = admission.as_ref();
    assert_eq!(admission.target_repo, None);
    assert_eq!(admission.runner_id, None);
    assert!(
        admission
            .findings
            .iter()
            .any(|finding| finding.code == "unknown_target_repo")
    );
    Ok(())
}

#[test]
fn missing_runner_fixture_denies_before_plan_materializes() -> Result<(), Box<dyn std::error::Error>>
{
    let policy: OperationalPolicy = serde_json::from_str(INVALID_UNKNOWN_RUNNER)?;
    let error = plan_target_repo_runner(
        &policy,
        &fixture_request("github-issues", "example/project"),
    )
    .err();

    let Some(TargetRepoRunnerPlanError::AdmissionDenied(admission)) = error else {
        return Err("expected admission denied planning error".into());
    };

    let admission = admission.as_ref();
    assert_eq!(admission.target_repo.as_deref(), Some("example/project"));
    assert_eq!(admission.runner_id, None);
    assert!(!admission.mutate_target_repo);
    assert!(
        admission
            .findings
            .iter()
            .any(|finding| finding.code == "unknown_runner")
    );
    Ok(())
}

#[test]
fn not_scafld_target_fixture_denies_before_mutation_plan() -> Result<(), Box<dyn std::error::Error>>
{
    let policy: OperationalPolicy = serde_json::from_str(INVALID_NOT_SCAFLD_TARGET)?;
    let error = plan_target_repo_runner(
        &policy,
        &fixture_request("github-issues", "example/project"),
    )
    .err();

    let Some(TargetRepoRunnerPlanError::AdmissionDenied(admission)) = error else {
        return Err("expected admission denied planning error".into());
    };

    let admission = admission.as_ref();
    assert_eq!(admission.target_repo.as_deref(), Some("example/project"));
    assert_eq!(admission.runner_id.as_deref(), Some("local-pr-runner"));
    assert!(admission.mutate_target_repo);
    assert!(
        admission
            .findings
            .iter()
            .any(|finding| finding.code == "mutation_without_scafld")
    );
    Ok(())
}

fn nitrosend_request(
    target_repo: &str,
    existing_pull_request: Option<TargetRepoRunnerExistingPullRequest>,
) -> TargetRepoRunnerPlanRequest {
    TargetRepoRunnerPlanRequest {
        source_id: Some("bugs-fixes".to_owned()),
        target_repo: target_repo.to_owned(),
        action: OperationalPolicyAction::IssueToPr,
        runner_id: None,
        source: TargetRepoRunnerSourceContext {
            provider: runx_contracts::OperationalPolicySourceProvider::Slack,
            locator: "slack://nitrosend/C0APFMY0V8Q".to_owned(),
            thread_locator: Some("slack://nitrosend/C0APFMY0V8Q/1778834840.485629".to_owned()),
            thread_ts: Some("1778834840.485629".to_owned()),
            issue_url: Some("https://github.com/nitrosend/nitrosend/issues/482".to_owned()),
        },
        signal_fingerprint: Some("sha256:nitrosend-source-482".to_owned()),
        existing_pull_request,
    }
}

fn fixture_request(source_id: &str, target_repo: &str) -> TargetRepoRunnerPlanRequest {
    TargetRepoRunnerPlanRequest {
        source_id: Some(source_id.to_owned()),
        target_repo: target_repo.to_owned(),
        action: OperationalPolicyAction::IssueToPr,
        runner_id: None,
        source: TargetRepoRunnerSourceContext {
            provider: runx_contracts::OperationalPolicySourceProvider::Github,
            locator: "github://example/project/issues".to_owned(),
            thread_locator: Some("github://example/project/issues/42".to_owned()),
            thread_ts: None,
            issue_url: Some("https://github.com/example/project/issues/42".to_owned()),
        },
        signal_fingerprint: Some("sha256:example-project-42".to_owned()),
        existing_pull_request: None,
    }
}

fn component_value<'a>(plan: &'a TargetRepoRunnerPlan, field: &str) -> Option<&'a str> {
    plan.dedupe
        .components
        .iter()
        .find(|component| component.field == field)
        .map(|component| component.value.as_str())
}

fn nested_string<'a>(
    object: &'a std::collections::BTreeMap<String, JsonValue>,
    path: &[&str],
) -> Option<&'a str> {
    let mut value = object.get(*path.first()?)?;
    for segment in &path[1..] {
        let JsonValue::Object(object) = value else {
            return None;
        };
        value = object.get(*segment)?;
    }
    match value {
        JsonValue::String(value) => Some(value.as_str()),
        _ => None,
    }
}
