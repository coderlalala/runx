use std::cell::RefCell;

use runx_contracts::{
    ActForm, HarnessState, JsonValue, OperationalPolicy, OperationalPolicyAction, Reference,
    ReferenceType, TargetRepoRunnerDedupeLookupObservation, TargetRepoRunnerDedupeResult,
    TargetRepoRunnerExistingPullRequest, TargetRepoRunnerPlanRequest, TargetRepoRunnerProvider,
    TargetRepoRunnerProviderPullRequest, TargetRepoRunnerPullRequestDisposition,
    TargetRepoRunnerReadinessObservation, TargetRepoRunnerSourceContext, plan_target_repo_runner,
    plan_target_repo_runner_execution,
};
use runx_receipts::canonical_receipt_body_digest;
use runx_runtime::target_runner::{
    TargetRepoRunnerAdapter, TargetRepoRunnerAdapterError, TargetRepoRunnerCheckoutCommand,
    TargetRepoRunnerFixtureExecutionInput, TargetRepoRunnerGithubApiClient,
    TargetRepoRunnerGithubPullRequestSearchState, TargetRepoRunnerGovernedRunnerInvocation,
    TargetRepoRunnerGovernedRunnerObservation, TargetRepoRunnerHttpError,
    TargetRepoRunnerHttpMethod, TargetRepoRunnerHttpRequest, TargetRepoRunnerHttpResponse,
    TargetRepoRunnerHttpTransport, TargetRepoRunnerProviderDedupeLookupCommand,
    TargetRepoRunnerPullRequestMutation, TargetRepoRunnerPullRequestObservationRequest,
    TargetRepoRunnerRuntimeError, TargetRepoRunnerSourcePublicationCommand,
    TargetRepoRunnerSourcePublicationObservation, TargetRepoRunnerSourcePublicationRequest,
    execute_target_repo_runner_execution_fixture, execute_target_repo_runner_fixture,
    execute_target_repo_runner_with_adapter, target_repo_runner_provider_dedupe_lookup_command,
    target_repo_runner_provider_dedupe_observation_from_pull_requests,
};
use serde_json::json;

const NITROSEND_LIKE: &str =
    include_str!("../../../fixtures/operational-policy/nitrosend-like.json");
const CREATED_AT: &str = "2026-05-20T10:30:00Z";

#[test]
fn runtime_fails_closed_before_dedupe_or_pr_when_checkout_not_scafld_ready()
-> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let plan = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/api"))?;

    let error = execute_target_repo_runner_fixture(TargetRepoRunnerFixtureExecutionInput {
        plan,
        readiness: readiness(false),
        dedupe: empty_dedupe_observation("nitrosend/api", "unobserved"),
        created_pull_request: Some(created_pull_request()),
    })
    .err();

    assert!(matches!(
        error,
        Some(TargetRepoRunnerRuntimeError::Plan(
            runx_contracts::TargetRepoRunnerPlanError::NotScafldReady { .. }
        ))
    ));
    Ok(())
}

#[test]
fn runtime_rechecks_execution_readiness_before_mutation_boundary()
-> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let plan = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/api"))?;
    let execution_plan = plan_target_repo_runner_execution(&plan, &readiness(true))?;

    let error = execute_target_repo_runner_execution_fixture(
        &plan,
        &execution_plan,
        &readiness(false),
        &empty_dedupe_observation("nitrosend/api", &execution_plan.provider_lookup.key),
        Some(&created_pull_request()),
    )
    .err();

    assert!(matches!(
        error,
        Some(TargetRepoRunnerRuntimeError::ReadinessMismatch(_))
    ));
    Ok(())
}

#[test]
fn runtime_creates_pull_request_when_provider_dedupe_has_no_match()
-> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let plan = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/api"))?;
    let execution_plan = plan_target_repo_runner_execution(&plan, &readiness(true))?;
    let created = created_pull_request();

    let execution = execute_target_repo_runner_execution_fixture(
        &plan,
        &execution_plan,
        &readiness(true),
        &empty_dedupe_observation("nitrosend/api", &execution_plan.provider_lookup.key),
        Some(&created),
    )?;

    assert_eq!(
        execution.disposition,
        TargetRepoRunnerPullRequestDisposition::Create
    );
    assert_eq!(
        execution.dedupe_execution.result,
        TargetRepoRunnerDedupeResult::LookupRequired
    );
    assert_eq!(execution.pull_request.url, created.url);
    assert_eq!(
        nested_string(
            &execution.pull_request_receipt.metadata,
            &["dedupe", "result"]
        ),
        Some("created")
    );
    assert_eq!(
        nested_string(
            &execution.pull_request_receipt.metadata,
            &["source", "thread_uri"]
        ),
        Some("slack://nitrosend/C0APFMY0V8Q/1778834840.485629")
    );
    assert_eq!(
        execution.source_publication_receipt.pull_request_ref.uri,
        "https://github.com/nitrosend/api/pull/145"
    );
    assert_public_only(&execution)?;
    Ok(())
}

#[test]
fn runtime_reuses_provider_pull_request_when_dedupe_refs_match()
-> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let plan = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/api"))?;
    let execution_plan = plan_target_repo_runner_execution(&plan, &readiness(true))?;
    let observed = TargetRepoRunnerProviderPullRequest {
        url: "https://github.com/nitrosend/api/pull/144".to_owned(),
        number: Some(144),
        branch: Some("runx/source-482".to_owned()),
        open: true,
        markers: execution_plan.provider_lookup.query.markers.clone(),
        refs: execution_plan.provider_lookup.query.required_refs.clone(),
    };

    let execution = execute_target_repo_runner_execution_fixture(
        &plan,
        &execution_plan,
        &readiness(true),
        &TargetRepoRunnerDedupeLookupObservation {
            provider: TargetRepoRunnerProvider::Github,
            target_repo: "nitrosend/api".to_owned(),
            key: execution_plan.provider_lookup.key.clone(),
            pull_requests: vec![observed],
        },
        None,
    )?;

    assert_eq!(
        execution.disposition,
        TargetRepoRunnerPullRequestDisposition::Reuse
    );
    assert_eq!(
        execution.dedupe_execution.result,
        TargetRepoRunnerDedupeResult::Reused
    );
    assert_eq!(execution.pull_request.number, Some(144));
    assert_eq!(
        execution
            .deduped_plan
            .dedupe
            .existing_pull_request
            .as_ref()
            .map(|pull_request| pull_request.url.as_str()),
        Some("https://github.com/nitrosend/api/pull/144")
    );
    assert_eq!(
        nested_string(
            &execution.pull_request_receipt.metadata,
            &["dedupe", "result"]
        ),
        Some("reused")
    );
    assert_eq!(
        execution.source_publication_receipt.source_thread_ref.uri,
        "slack://nitrosend/C0APFMY0V8Q/1778834840.485629"
    );
    assert_public_only(&execution)?;
    Ok(())
}

#[test]
fn runtime_requires_created_pull_request_for_create_decision()
-> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let plan = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/api"))?;
    let execution_plan = plan_target_repo_runner_execution(&plan, &readiness(true))?;

    let error = execute_target_repo_runner_execution_fixture(
        &plan,
        &execution_plan,
        &readiness(true),
        &empty_dedupe_observation("nitrosend/api", &execution_plan.provider_lookup.key),
        None,
    )
    .err();

    assert!(matches!(
        error,
        Some(TargetRepoRunnerRuntimeError::CreatedPullRequestRequired { .. })
    ));
    Ok(())
}

#[test]
fn provider_lookup_command_is_concrete_github_pr_search() -> Result<(), Box<dyn std::error::Error>>
{
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let plan = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/api"))?;
    let execution_plan = plan_target_repo_runner_execution(&plan, &readiness(true))?;

    let command =
        target_repo_runner_provider_dedupe_lookup_command(&execution_plan.provider_lookup)?;

    assert_eq!(command.provider, TargetRepoRunnerProvider::Github);
    assert_eq!(command.repository.owner, "nitrosend");
    assert_eq!(command.repository.name, "api");
    assert_eq!(command.repository.full_name, "nitrosend/api");
    assert_eq!(command.result_limit, 20);
    assert_eq!(command.query.repo, "nitrosend/api");
    assert_eq!(
        command.query.state,
        TargetRepoRunnerGithubPullRequestSearchState::Open
    );
    assert_eq!(command.query.terms[0], "repo:nitrosend/api");
    assert_eq!(command.query.terms[1], "is:pr");
    assert_eq!(command.query.terms[2], "is:open");
    assert!(
        command
            .query
            .terms
            .iter()
            .any(|term| term.starts_with("\"runx-dedupe-key:"))
    );
    assert!(
        command
            .query
            .terms
            .iter()
            .any(|term| term == "\"https://github.com/nitrosend/nitrosend/issues/482\"")
    );
    assert!(
        command
            .query
            .terms
            .iter()
            .any(|term| term == "\"slack://nitrosend/C0APFMY0V8Q/1778834840.485629\"")
    );
    assert!(
        command
            .query
            .query
            .contains("repo:nitrosend/api is:pr is:open")
    );
    assert_public_only(&command)?;
    Ok(())
}

#[test]
fn github_provider_api_lookup_projects_search_readback() -> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let plan = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/api"))?;
    let execution_plan = plan_target_repo_runner_execution(&plan, &readiness(true))?;
    let command =
        target_repo_runner_provider_dedupe_lookup_command(&execution_plan.provider_lookup)?;
    let body = format!(
        "{}\n{}\n{}",
        command.markers.join("\n"),
        command
            .required_refs
            .iter()
            .map(|reference| reference.uri.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        "Human review remains the merge gate."
    );
    let transport = RecordingGithubTransport::with_body(
        json!({
            "items": [
                {
                    "html_url": "https://github.com/nitrosend/api/pull/144",
                    "number": 144,
                    "state": "open",
                    "title": "runx target fix",
                    "body": body,
                    "pull_request": {}
                }
            ]
        })
        .to_string(),
    );
    let client = TargetRepoRunnerGithubApiClient::with_transport(
        "https://api.github.example/",
        &transport,
        Some("SECRET_GITHUB_TOKEN".to_owned()),
    )?;

    let observation = client.provider_dedupe_lookup(&command)?;

    assert_eq!(observation.provider, TargetRepoRunnerProvider::Github);
    assert_eq!(observation.target_repo, "nitrosend/api");
    assert_eq!(observation.key, command.dedupe_key);
    assert_eq!(observation.pull_requests.len(), 1);
    assert_eq!(
        observation.pull_requests[0].url,
        "https://github.com/nitrosend/api/pull/144"
    );
    assert_eq!(observation.pull_requests[0].number, Some(144));
    assert!(observation.pull_requests[0].open);
    assert_eq!(observation.pull_requests[0].markers, command.markers);
    assert_eq!(observation.pull_requests[0].refs, command.required_refs);

    let sent = transport.requests.borrow();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].method, TargetRepoRunnerHttpMethod::Get);
    assert!(
        sent[0]
            .url
            .starts_with("https://api.github.example/search/issues?")
    );
    assert!(sent[0].url.contains("per_page=20"));
    assert!(sent[0].url.contains("q=repo%3Anitrosend%2Fapi"));
    assert!(
        sent[0]
            .headers
            .iter()
            .any(|header| header.name == "authorization")
    );
    assert!(!format!("{:?}", sent[0]).contains("SECRET_GITHUB_TOKEN"));
    assert_public_only(&observation)?;
    Ok(())
}

#[test]
fn github_provider_api_lookup_fails_closed_on_http_error() -> Result<(), Box<dyn std::error::Error>>
{
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let plan = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/api"))?;
    let execution_plan = plan_target_repo_runner_execution(&plan, &readiness(true))?;
    let command =
        target_repo_runner_provider_dedupe_lookup_command(&execution_plan.provider_lookup)?;
    let transport = RecordingGithubTransport::with_status(502, "{\"message\":\"bad gateway\"}");
    let client = TargetRepoRunnerGithubApiClient::with_transport(
        "https://api.github.example",
        &transport,
        None,
    )?;

    let error = client.provider_dedupe_lookup(&command).err();

    assert!(matches!(
        error,
        Some(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "provider_api_lookup",
            ..
        })
    ));
    assert_eq!(transport.requests.borrow().len(), 1);
    Ok(())
}

#[test]
fn provider_lookup_command_rejects_invalid_target_repo_before_adapter()
-> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let plan = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/api"))?;
    let mut lookup = plan_target_repo_runner_execution(&plan, &readiness(true))?.provider_lookup;
    lookup.target_repo = "nitrosend/api/extra".to_owned();

    let error = target_repo_runner_provider_dedupe_lookup_command(&lookup).err();

    assert!(matches!(
        error,
        Some(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "provider_dedupe_lookup",
            ..
        })
    ));
    Ok(())
}

#[test]
fn live_adapter_rejects_invalid_checkout_command_before_adapter_calls()
-> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let mut plan = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/api"))?;
    plan.target.repo = "nitrosend/api/extra".to_owned();
    let mut adapter = FakeTargetRepoRunnerAdapter {
        created_pull_request: created_pull_request(),
        pull_request_readback_override: None,
        provider_pull_requests: Vec::new(),
        expected_disposition: TargetRepoRunnerPullRequestDisposition::Create,
        omit_source_issue_publication: false,
        events: Vec::new(),
    };

    let error = execute_target_repo_runner_with_adapter(&plan, &mut adapter, CREATED_AT).err();

    assert!(matches!(
        error,
        Some(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "checkout",
            ..
        })
    ));
    assert!(adapter.events.is_empty());
    Ok(())
}

#[test]
fn live_adapter_composes_observations_into_revision_receipt_without_network()
-> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let plan = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/api"))?;
    let mut adapter = FakeTargetRepoRunnerAdapter {
        created_pull_request: created_pull_request(),
        pull_request_readback_override: None,
        provider_pull_requests: Vec::new(),
        expected_disposition: TargetRepoRunnerPullRequestDisposition::Create,
        omit_source_issue_publication: false,
        events: Vec::new(),
    };

    let live = execute_target_repo_runner_with_adapter(&plan, &mut adapter, CREATED_AT)?;

    assert_eq!(
        adapter.events,
        vec![
            "checkout",
            "dedupe",
            "runner",
            "pull_request",
            "source_publication"
        ]
    );
    assert_eq!(live.checkout_command.target_repo, "nitrosend/api");
    assert_eq!(
        live.checkout_command.public_repo_ref.uri,
        "https://github.com/nitrosend/api"
    );
    assert_eq!(
        live.provider_lookup_command.query.terms[0],
        "repo:nitrosend/api"
    );
    assert_eq!(
        live.pull_request_request.command.provider,
        TargetRepoRunnerProvider::Github
    );
    assert_eq!(
        live.pull_request_request.command.target_repo,
        "nitrosend/api"
    );
    assert_eq!(
        live.pull_request_request.command.repository.full_name,
        "nitrosend/api"
    );
    assert_eq!(
        live.pull_request_request.command.base_branch.as_deref(),
        Some("main")
    );
    assert!(live.pull_request_request.command.human_merge_gate_required);
    assert!(live.pull_request_request.command.local_path_hidden);
    match &live.pull_request_request.command.mutation {
        TargetRepoRunnerPullRequestMutation::Create(command) => {
            assert_eq!(command.runner_id, "aster-production");
            assert!(command.body.contains("runx-dedupe-key:"));
            assert!(
                command
                    .body
                    .contains("https://github.com/nitrosend/nitrosend/issues/482")
            );
            assert!(
                command
                    .body
                    .contains("slack://nitrosend/C0APFMY0V8Q/1778834840.485629")
            );
            assert!(
                command
                    .body
                    .contains("Human review remains the merge gate.")
            );
        }
        TargetRepoRunnerPullRequestMutation::Reuse(_) => {
            return Err("expected create pull request command".into());
        }
    }
    assert_eq!(
        live.execution.disposition,
        TargetRepoRunnerPullRequestDisposition::Create
    );
    assert_eq!(
        live.execution.dedupe_execution.result,
        TargetRepoRunnerDedupeResult::LookupRequired
    );
    assert_eq!(
        nested_string(
            &live.execution.pull_request_receipt.metadata,
            &["dedupe", "result"]
        ),
        Some("created")
    );
    assert_eq!(
        live.execution
            .source_publication_receipt
            .pull_request_ref
            .uri,
        "https://github.com/nitrosend/api/pull/145"
    );

    assert_eq!(live.revision_receipt.harness.state, HarnessState::Sealed);
    assert_eq!(live.revision_receipt.harness.revision.sequence, 1);
    assert_eq!(live.revision_receipt.harness.acts.len(), 1);
    let act = &live.revision_receipt.harness.acts[0];
    assert_eq!(act.form, ActForm::Revision);
    assert!(act.revision.is_some());
    assert!(act.verification.is_none());
    assert_eq!(act.closure.reason_code, "target_runner_pr_created");
    assert_eq!(
        live.revision_projection.pull_request_ref.uri,
        "https://github.com/nitrosend/api/pull/145"
    );
    assert_eq!(
        live.revision_projection.source_thread_ref.uri,
        "slack://nitrosend/C0APFMY0V8Q/1778834840.485629"
    );
    assert_eq!(
        live.revision_projection.disposition,
        TargetRepoRunnerPullRequestDisposition::Create
    );
    assert_eq!(
        nested_string(
            &live.revision_projection.metadata,
            &["target_runner", "contract"]
        ),
        Some("runx.target_repo_runner.v1")
    );
    assert_eq!(live.source_publication_request.commands.len(), 2);
    assert_eq!(
        live.source_publication_receipt.harness.state,
        HarnessState::Sealed
    );
    assert_eq!(live.source_publication_receipt.harness.acts.len(), 1);
    assert_eq!(
        live.source_publication_receipt.harness.acts[0].form,
        ActForm::Reply
    );
    assert_eq!(
        live.source_publication_receipt.harness.acts[0]
            .closure
            .reason_code,
        "target_runner_source_published"
    );
    assert_eq!(
        live.source_publication_projection.pull_request_ref.uri,
        "https://github.com/nitrosend/api/pull/145"
    );
    assert_eq!(
        live.source_publication_projection
            .source_issue_ref
            .as_ref()
            .map(|reference| reference.uri.as_str()),
        Some("https://github.com/nitrosend/nitrosend/issues/482")
    );
    assert_eq!(
        live.source_publication_projection.source_thread_ref.uri,
        "slack://nitrosend/C0APFMY0V8Q/1778834840.485629"
    );
    assert_eq!(
        nested_string(
            &live.source_publication_projection.metadata,
            &["target_runner", "contract"]
        ),
        Some("runx.target_repo_runner.source_publication.v1")
    );

    let digest = canonical_receipt_body_digest(&live.revision_receipt)?;
    assert_eq!(live.revision_receipt.seal.digest, digest);
    assert_eq!(
        live.revision_receipt.signature.value,
        format!("sig:{digest}")
    );
    assert_public_only(&live.revision_receipt)?;
    assert_public_only(&live.source_publication_receipt)?;
    assert_hard_cutover_vocabulary_only(&live.revision_receipt)?;
    assert_hard_cutover_vocabulary_only(&live.source_publication_receipt)?;
    Ok(())
}

#[test]
fn live_adapter_reuses_provider_pull_request_without_runner_and_seals_reuse_metadata()
-> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let plan = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/api"))?;
    let lookup = plan_target_repo_runner_execution(&plan, &readiness(true))?.provider_lookup;
    let existing = TargetRepoRunnerProviderPullRequest {
        url: "https://github.com/nitrosend/api/pull/144".to_owned(),
        number: Some(144),
        branch: Some("runx/source-482".to_owned()),
        open: true,
        markers: lookup.query.markers.clone(),
        refs: lookup.query.required_refs.clone(),
    };
    let mut adapter = FakeTargetRepoRunnerAdapter {
        created_pull_request: created_pull_request(),
        pull_request_readback_override: None,
        provider_pull_requests: vec![existing],
        expected_disposition: TargetRepoRunnerPullRequestDisposition::Reuse,
        omit_source_issue_publication: false,
        events: Vec::new(),
    };

    let live = execute_target_repo_runner_with_adapter(&plan, &mut adapter, CREATED_AT)?;

    assert_eq!(
        adapter.events,
        vec!["checkout", "dedupe", "pull_request", "source_publication"]
    );
    assert!(live.runner_observation.is_none());
    match &live.pull_request_request.command.mutation {
        TargetRepoRunnerPullRequestMutation::Reuse(command) => {
            assert_eq!(
                command.existing_pull_request.url,
                "https://github.com/nitrosend/api/pull/144"
            );
            assert_eq!(command.existing_pull_request.number, Some(144));
        }
        TargetRepoRunnerPullRequestMutation::Create(_) => {
            return Err("expected reuse pull request command".into());
        }
    }
    assert_eq!(
        live.execution.disposition,
        TargetRepoRunnerPullRequestDisposition::Reuse
    );
    assert_eq!(
        live.execution.dedupe_execution.result,
        TargetRepoRunnerDedupeResult::Reused
    );
    assert_eq!(live.execution.pull_request.number, Some(144));
    assert_eq!(
        nested_string(
            &live.execution.source_publication_receipt.metadata,
            &["dedupe", "result"]
        ),
        Some("reused")
    );
    assert_eq!(
        nested_string(&live.revision_projection.metadata, &["dedupe", "result"]),
        Some("reused")
    );
    assert_eq!(
        live.revision_receipt.harness.acts[0].closure.reason_code,
        "target_runner_pr_reused"
    );
    assert_eq!(
        live.revision_projection.pull_request_ref.uri,
        "https://github.com/nitrosend/api/pull/144"
    );
    assert_eq!(
        live.revision_projection.disposition,
        TargetRepoRunnerPullRequestDisposition::Reuse
    );
    assert_eq!(
        nested_string(
            &live.source_publication_projection.metadata,
            &["dedupe", "result"]
        ),
        Some("reused")
    );
    assert_eq!(
        live.source_publication_projection.pull_request_ref.uri,
        "https://github.com/nitrosend/api/pull/144"
    );
    assert_public_only(&live.revision_receipt)?;
    assert_public_only(&live.source_publication_receipt)?;
    assert_hard_cutover_vocabulary_only(&live.revision_receipt)?;
    assert_hard_cutover_vocabulary_only(&live.source_publication_receipt)?;
    Ok(())
}

#[test]
fn live_adapter_fails_when_source_publication_readback_omits_source_issue()
-> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let plan = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/api"))?;
    let mut adapter = FakeTargetRepoRunnerAdapter {
        created_pull_request: created_pull_request(),
        pull_request_readback_override: None,
        provider_pull_requests: Vec::new(),
        expected_disposition: TargetRepoRunnerPullRequestDisposition::Create,
        omit_source_issue_publication: true,
        events: Vec::new(),
    };

    let error = execute_target_repo_runner_with_adapter(&plan, &mut adapter, CREATED_AT).err();

    assert!(matches!(
        error,
        Some(TargetRepoRunnerRuntimeError::SourcePublicationMismatch(_))
    ));
    assert_eq!(
        adapter.events,
        vec![
            "checkout",
            "dedupe",
            "runner",
            "pull_request",
            "source_publication"
        ]
    );
    Ok(())
}

#[test]
fn live_adapter_fails_when_created_pr_readback_points_at_other_repo()
-> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let plan = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/api"))?;
    let mut adapter = FakeTargetRepoRunnerAdapter {
        created_pull_request: created_pull_request(),
        pull_request_readback_override: Some(TargetRepoRunnerExistingPullRequest {
            url: "https://github.com/nitrosend/app/pull/145".to_owned(),
            number: Some(145),
            branch: Some("runx/source-482-new".to_owned()),
        }),
        provider_pull_requests: Vec::new(),
        expected_disposition: TargetRepoRunnerPullRequestDisposition::Create,
        omit_source_issue_publication: false,
        events: Vec::new(),
    };

    let error = execute_target_repo_runner_with_adapter(&plan, &mut adapter, CREATED_AT).err();

    assert!(matches!(
        error,
        Some(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "pull_request",
            ..
        })
    ));
    assert_eq!(
        adapter.events,
        vec!["checkout", "dedupe", "runner", "pull_request"]
    );
    Ok(())
}

#[test]
fn live_adapter_fails_when_reuse_pr_readback_differs_from_dedupe()
-> Result<(), Box<dyn std::error::Error>> {
    let policy: OperationalPolicy = serde_json::from_str(NITROSEND_LIKE)?;
    let plan = plan_target_repo_runner(&policy, &nitrosend_request("nitrosend/api"))?;
    let lookup = plan_target_repo_runner_execution(&plan, &readiness(true))?.provider_lookup;
    let existing = TargetRepoRunnerProviderPullRequest {
        url: "https://github.com/nitrosend/api/pull/144".to_owned(),
        number: Some(144),
        branch: Some("runx/source-482".to_owned()),
        open: true,
        markers: lookup.query.markers.clone(),
        refs: lookup.query.required_refs.clone(),
    };
    let mut adapter = FakeTargetRepoRunnerAdapter {
        created_pull_request: created_pull_request(),
        pull_request_readback_override: Some(TargetRepoRunnerExistingPullRequest {
            url: "https://github.com/nitrosend/api/pull/145".to_owned(),
            number: Some(145),
            branch: Some("runx/source-482-new".to_owned()),
        }),
        provider_pull_requests: vec![existing],
        expected_disposition: TargetRepoRunnerPullRequestDisposition::Reuse,
        omit_source_issue_publication: false,
        events: Vec::new(),
    };

    let error = execute_target_repo_runner_with_adapter(&plan, &mut adapter, CREATED_AT).err();

    assert!(matches!(
        error,
        Some(TargetRepoRunnerRuntimeError::CommandValidation {
            operation: "pull_request",
            ..
        })
    ));
    assert_eq!(adapter.events, vec!["checkout", "dedupe", "pull_request"]);
    Ok(())
}

fn nitrosend_request(target_repo: &str) -> TargetRepoRunnerPlanRequest {
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
        existing_pull_request: None,
    }
}

struct RecordingGithubTransport {
    status: u16,
    body: String,
    requests: RefCell<Vec<TargetRepoRunnerHttpRequest>>,
}

impl RecordingGithubTransport {
    fn with_body(body: String) -> Self {
        Self {
            status: 200,
            body,
            requests: RefCell::new(Vec::new()),
        }
    }

    fn with_status(status: u16, body: &str) -> Self {
        Self {
            status,
            body: body.to_owned(),
            requests: RefCell::new(Vec::new()),
        }
    }
}

impl TargetRepoRunnerHttpTransport for &RecordingGithubTransport {
    fn send(
        &self,
        request: TargetRepoRunnerHttpRequest,
    ) -> Result<TargetRepoRunnerHttpResponse, TargetRepoRunnerHttpError> {
        self.requests.borrow_mut().push(request);
        Ok(TargetRepoRunnerHttpResponse {
            status: self.status,
            body: self.body.clone(),
        })
    }
}

struct FakeTargetRepoRunnerAdapter {
    created_pull_request: TargetRepoRunnerExistingPullRequest,
    pull_request_readback_override: Option<TargetRepoRunnerExistingPullRequest>,
    provider_pull_requests: Vec<TargetRepoRunnerProviderPullRequest>,
    expected_disposition: TargetRepoRunnerPullRequestDisposition,
    omit_source_issue_publication: bool,
    events: Vec<&'static str>,
}

impl TargetRepoRunnerAdapter for FakeTargetRepoRunnerAdapter {
    fn checkout_readiness(
        &mut self,
        command: &TargetRepoRunnerCheckoutCommand,
    ) -> Result<TargetRepoRunnerReadinessObservation, TargetRepoRunnerAdapterError> {
        self.events.push("checkout");
        assert_eq!(command.target_repo, "nitrosend/api");
        assert_eq!(
            command.public_repo_ref.uri,
            "https://github.com/nitrosend/api"
        );
        assert_eq!(command.base_branch.as_deref(), Some("main"));
        assert!(command.target_scafld_required);
        assert!(command.runner_scafld_required);
        assert!(command.mutate_target_repo);
        assert!(command.local_path_hidden);
        Ok(TargetRepoRunnerReadinessObservation {
            target_repo: command.target_repo.clone(),
            runner_id: command.runner_id.clone(),
            scafld_ready: true,
        })
    }

    fn provider_dedupe_lookup(
        &mut self,
        command: &TargetRepoRunnerProviderDedupeLookupCommand,
    ) -> Result<TargetRepoRunnerDedupeLookupObservation, TargetRepoRunnerAdapterError> {
        self.events.push("dedupe");
        assert_github_provider_lookup_command(command);
        target_repo_runner_provider_dedupe_observation_from_pull_requests(
            command,
            self.provider_pull_requests.clone(),
        )
        .map_err(|error| TargetRepoRunnerAdapterError::new("dedupe", error.to_string()))
    }

    fn invoke_governed_runner(
        &mut self,
        invocation: &TargetRepoRunnerGovernedRunnerInvocation,
    ) -> Result<TargetRepoRunnerGovernedRunnerObservation, TargetRepoRunnerAdapterError> {
        self.events.push("runner");
        Ok(TargetRepoRunnerGovernedRunnerObservation {
            runner_id: invocation.execution_plan.readiness.runner_id.clone(),
            target_repo: invocation.execution_plan.checkout.target_repo.clone(),
            summary: "Fake governed target runner prepared a branch for review.".to_owned(),
            revision_refs: vec![Reference {
                reference_type: ReferenceType::GithubPullRequest,
                uri: self.created_pull_request.url.clone(),
                provider: Some("github".to_owned()),
                locator: self
                    .created_pull_request
                    .number
                    .map(|number| format!("nitrosend/api#{number}")),
                label: Some("Nitrosend API target PR".to_owned()),
                observed_at: Some(CREATED_AT.to_owned()),
                proof_kind: None,
            }],
            artifact_refs: vec![Reference {
                reference_type: ReferenceType::Artifact,
                uri: "runx:artifact:target-runner-fake-diff".to_owned(),
                provider: None,
                locator: None,
                label: Some("fake sanitized diff".to_owned()),
                observed_at: Some(CREATED_AT.to_owned()),
                proof_kind: None,
            }],
            verification_refs: vec![Reference {
                reference_type: ReferenceType::Verification,
                uri: "runx:verification:target-runner-fake".to_owned(),
                provider: None,
                locator: None,
                label: Some("fake no-network verification".to_owned()),
                observed_at: Some(CREATED_AT.to_owned()),
                proof_kind: None,
            }],
        })
    }

    fn observe_pull_request(
        &mut self,
        request: &TargetRepoRunnerPullRequestObservationRequest,
    ) -> Result<TargetRepoRunnerExistingPullRequest, TargetRepoRunnerAdapterError> {
        self.events.push("pull_request");
        assert_eq!(request.disposition, self.expected_disposition);
        assert_pull_request_mutation_command(request)?;
        if let Some(readback) = &self.pull_request_readback_override {
            return Ok(readback.clone());
        }
        match request.disposition {
            TargetRepoRunnerPullRequestDisposition::Create => {
                assert!(request.existing_pull_request.is_none());
                assert!(request.runner_observation.is_some());
                Ok(self
                    .pull_request_readback_override
                    .clone()
                    .unwrap_or_else(|| self.created_pull_request.clone()))
            }
            TargetRepoRunnerPullRequestDisposition::Reuse => {
                assert!(request.runner_observation.is_none());
                if let Some(readback) = &self.pull_request_readback_override {
                    return Ok(readback.clone());
                }
                request.existing_pull_request.clone().ok_or_else(|| {
                    TargetRepoRunnerAdapterError::new(
                        "pull_request",
                        "expected existing pull request for reuse",
                    )
                })
            }
        }
    }

    fn publish_source_update(
        &mut self,
        request: &TargetRepoRunnerSourcePublicationRequest,
    ) -> Result<TargetRepoRunnerSourcePublicationObservation, TargetRepoRunnerAdapterError> {
        self.events.push("source_publication");
        assert_source_publication_commands(request);
        Ok(TargetRepoRunnerSourcePublicationObservation {
            source_issue_ref: if self.omit_source_issue_publication {
                None
            } else {
                request.publication.source_issue_ref.clone()
            },
            source_thread_ref: request.publication.source_thread_ref.clone(),
            pull_request_ref: request.publication.pull_request_ref.clone(),
            revision_receipt_ref: request.revision_receipt_ref.clone(),
            published_refs: vec![
                Reference {
                    reference_type: ReferenceType::ExternalUrl,
                    uri: "https://github.com/nitrosend/nitrosend/issues/482#issuecomment-9001"
                        .to_owned(),
                    provider: Some("github".to_owned()),
                    locator: Some("nitrosend/nitrosend#482-comment-9001".to_owned()),
                    label: Some("source issue target PR comment".to_owned()),
                    observed_at: Some(CREATED_AT.to_owned()),
                    proof_kind: None,
                },
                Reference {
                    reference_type: ReferenceType::SlackThread,
                    uri: "slack://nitrosend/C0APFMY0V8Q/1778834840.485629/reply/1778835000.000100"
                        .to_owned(),
                    provider: Some("slack".to_owned()),
                    locator: Some("C0APFMY0V8Q/1778835000.000100".to_owned()),
                    label: Some("source thread target PR reply".to_owned()),
                    observed_at: Some(CREATED_AT.to_owned()),
                    proof_kind: None,
                },
            ],
            metadata: request.publication.metadata.clone(),
        })
    }
}

fn assert_source_publication_commands(request: &TargetRepoRunnerSourcePublicationRequest) {
    assert_eq!(request.commands.len(), 2);
    let mut saw_issue_comment = false;
    let mut saw_thread_reply = false;
    for command in &request.commands {
        match command {
            TargetRepoRunnerSourcePublicationCommand::SourceIssueComment { target, body } => {
                saw_issue_comment = true;
                assert_eq!(
                    target.uri,
                    "https://github.com/nitrosend/nitrosend/issues/482"
                );
                assert!(body.contains(&request.publication.pull_request_ref.uri));
                assert!(body.contains(&request.revision_receipt_ref.uri));
                assert!(body.contains("Human review remains the merge gate."));
            }
            TargetRepoRunnerSourcePublicationCommand::SourceThreadReply { target, body } => {
                saw_thread_reply = true;
                assert_eq!(
                    target.uri,
                    "slack://nitrosend/C0APFMY0V8Q/1778834840.485629"
                );
                assert!(body.contains(&request.publication.pull_request_ref.uri));
                assert!(body.contains("Target repo: nitrosend/api"));
            }
        }
    }
    assert!(saw_issue_comment);
    assert!(saw_thread_reply);
}

fn assert_pull_request_mutation_command(
    request: &TargetRepoRunnerPullRequestObservationRequest,
) -> Result<(), TargetRepoRunnerAdapterError> {
    assert_eq!(request.command.provider, TargetRepoRunnerProvider::Github);
    assert_eq!(request.command.disposition, request.disposition);
    assert_eq!(request.command.target_repo, request.target_repo);
    assert_eq!(request.command.repository.full_name, "nitrosend/api");
    assert_eq!(
        request.command.target_repo_ref.uri,
        "https://github.com/nitrosend/api"
    );
    assert_eq!(request.command.base_branch.as_deref(), Some("main"));
    assert_eq!(request.command.dedupe_key, request.dedupe_key);
    assert_eq!(
        request.command.source_thread_ref.uri,
        "slack://nitrosend/C0APFMY0V8Q/1778834840.485629"
    );
    assert_eq!(
        request
            .command
            .source_issue_ref
            .as_ref()
            .map(|reference| reference.uri.as_str()),
        Some("https://github.com/nitrosend/nitrosend/issues/482")
    );
    assert!(request.command.human_merge_gate_required);
    assert!(request.command.local_path_hidden);
    match (&request.disposition, &request.command.mutation) {
        (
            TargetRepoRunnerPullRequestDisposition::Create,
            TargetRepoRunnerPullRequestMutation::Create(command),
        ) => {
            assert!(request.existing_pull_request.is_none());
            assert!(request.runner_observation.is_some());
            assert_eq!(command.runner_id, "aster-production");
            assert!(command.body.contains(&request.command.dedupe_key));
            assert!(
                command
                    .body
                    .contains("runx-dedupe:target_repo=nitrosend/api")
            );
            assert!(
                command
                    .body
                    .contains("https://github.com/nitrosend/nitrosend/issues/482")
            );
            assert!(
                command
                    .body
                    .contains("slack://nitrosend/C0APFMY0V8Q/1778834840.485629")
            );
        }
        (
            TargetRepoRunnerPullRequestDisposition::Reuse,
            TargetRepoRunnerPullRequestMutation::Reuse(command),
        ) => {
            assert!(request.runner_observation.is_none());
            assert_eq!(
                request
                    .existing_pull_request
                    .as_ref()
                    .map(|pull_request| pull_request.url.as_str()),
                Some(command.existing_pull_request.url.as_str())
            );
        }
        _ => {
            return Err(TargetRepoRunnerAdapterError::new(
                "pull_request",
                "pull request mutation command does not match disposition",
            ));
        }
    }
    Ok(())
}

fn assert_github_provider_lookup_command(command: &TargetRepoRunnerProviderDedupeLookupCommand) {
    assert_eq!(command.provider, TargetRepoRunnerProvider::Github);
    assert_eq!(command.target_repo, "nitrosend/api");
    assert_eq!(command.repository.full_name, "nitrosend/api");
    assert_eq!(command.query.repo, "nitrosend/api");
    assert_eq!(
        command.query.state,
        TargetRepoRunnerGithubPullRequestSearchState::Open
    );
    assert_eq!(command.query.terms[0], "repo:nitrosend/api");
    assert_eq!(command.query.terms[1], "is:pr");
    assert_eq!(command.query.terms[2], "is:open");
    assert!(
        command
            .markers
            .iter()
            .any(|marker| { marker == &format!("runx-dedupe-key:{}", command.dedupe_key) })
    );
    assert!(
        command.required_refs.iter().any(|reference| {
            reference.uri == "https://github.com/nitrosend/nitrosend/issues/482"
        })
    );
    assert!(
        command.required_refs.iter().any(|reference| {
            reference.uri == "slack://nitrosend/C0APFMY0V8Q/1778834840.485629"
        })
    );
}

fn readiness(scafld_ready: bool) -> TargetRepoRunnerReadinessObservation {
    TargetRepoRunnerReadinessObservation {
        target_repo: "nitrosend/api".to_owned(),
        runner_id: "aster-production".to_owned(),
        scafld_ready,
    }
}

fn empty_dedupe_observation(
    target_repo: &str,
    key: &str,
) -> TargetRepoRunnerDedupeLookupObservation {
    TargetRepoRunnerDedupeLookupObservation {
        provider: TargetRepoRunnerProvider::Github,
        target_repo: target_repo.to_owned(),
        key: key.to_owned(),
        pull_requests: Vec::new(),
    }
}

fn created_pull_request() -> TargetRepoRunnerExistingPullRequest {
    TargetRepoRunnerExistingPullRequest {
        url: "https://github.com/nitrosend/api/pull/145".to_owned(),
        number: Some(145),
        branch: Some("runx/source-482-new".to_owned()),
    }
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

fn assert_public_only<T: serde::Serialize>(value: &T) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string(value)?;
    assert!(!json.contains("/Users/"));
    assert!(!json.contains("/tmp/"));
    assert!(!json.contains("RUNX_"));
    assert!(!json.contains("SECRET"));
    Ok(())
}

fn assert_hard_cutover_vocabulary_only<T: serde::Serialize>(
    value: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string(value)?;
    for retired in [
        "work_item",
        "matter",
        "engagement",
        "judgment",
        "effect",
        "outcome",
    ] {
        assert!(!json.contains(retired), "retired field leaked: {retired}");
    }
    Ok(())
}
