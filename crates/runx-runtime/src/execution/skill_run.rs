// rust-style-allow: large-file - native skill execution keeps request parsing,
// continuation hydration, and sealed receipt assembly together for parity review.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use runx_contracts::{
    ClosureDisposition, JsonNumber, JsonObject, JsonValue, ResolutionRequest, ResolutionResponse,
    ResolutionResponseActor, sha256_hex,
};
use runx_core::state_machine::GraphStatus;
use runx_parser::{
    ExecutionGraph, HarnessCallerFixture, RunnerHarnessCase, SkillRunnerDefinition,
    SkillRunnerManifest, parse_runner_manifest_yaml, validate_runner_manifest,
};
use serde::{Deserialize, Serialize};
#[cfg(feature = "cli-tool")]
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::RuntimeError;
#[cfg(any(
    feature = "cli-tool",
    feature = "http",
    feature = "thread-outbox-provider"
))]
use crate::adapter::SkillAdapter;
use crate::adapter::{InvocationStatus, SkillInvocation, SkillOutput};
#[cfg(feature = "cli-tool")]
use crate::adapters::cli_tool::CliToolAdapter;
use crate::agent_invocation::{
    AgentActInvocationSourceType, agent_act_invocation_id, agent_act_resolution_request,
};
use crate::effects::RuntimeEffectRegistry;
use crate::execution::graph::materialize_graph_inputs;
use crate::execution::orchestrator::SkillRunRequest;
use crate::execution::runner::{
    GraphCheckpoint, GraphRun, RUNX_RUN_ID_ENV, Runtime, RuntimeOptions,
};
use crate::host::Host;
use crate::receipts::signing::strip_receipt_signing_env;
use crate::receipts::store::ReceiptStoreError;
use crate::receipts::{
    RuntimeReceiptSignatureConfig, StepReceiptWithDisposition,
    step_receipt_with_disposition_and_policy,
};
use crate::services::{ReceiptServices, WorkspaceEnv};

const SKILL_RUN_SCHEMA: &str = "runx.skill_run.v1";
const GRAPH_SKILL_STATE_SCHEMA: &str = "runx.graph_skill_state.v1";

#[derive(Debug, Error)]
pub enum SkillRunError {
    #[error("skill run failed: {0}")]
    Invalid(String),
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error(transparent)]
    ReceiptStore(#[from] ReceiptStoreError),
}

/// Optional, non-default knobs for a single skill run.
///
/// `execute_skill_run` keeps today's behavior (default runner, file-based
/// answers). The inline harness needs two extra capabilities without touching
/// the 35+ `SkillRunRequest` construction sites: select a named runner, and
/// seed answers inline for a single fresh pass (distinct from the `answers_path`
/// resume channel). Both default to "off", so `execute_skill_run` and every CLI
/// path are unchanged.
#[derive(Clone, Debug, Default)]
pub(crate) struct SkillRunOverrides {
    /// Select a runner by name instead of the manifest default.
    pub(crate) runner: Option<String>,
    /// Answers seeded for a single fresh run, keyed by resolution request id.
    /// Drives agent/graph runs to completion in one pass; `None` keeps the
    /// `answers_path` (resume-from-checkpoint) behavior.
    pub(crate) seeded_answers: Option<JsonObject>,
}

pub(crate) fn execute_skill_run_with_effects(
    request: &SkillRunRequest,
    effects: &RuntimeEffectRegistry,
) -> Result<JsonValue, SkillRunError> {
    execute_skill_run_with_overrides(request, &SkillRunOverrides::default(), effects)
}

pub(crate) fn execute_skill_run_with_overrides(
    request: &SkillRunRequest,
    overrides: &SkillRunOverrides,
    effects: &RuntimeEffectRegistry,
) -> Result<JsonValue, SkillRunError> {
    let raw_workspace = WorkspaceEnv::new(request.env.clone(), request.cwd.clone());
    let receipts = ReceiptServices::from_env(raw_workspace.env())
        .map_err(|error| SkillRunError::Invalid(error.to_string()))?;
    let mut runtime_env = request.env.clone();
    strip_receipt_signing_env(&mut runtime_env);
    let workspace = WorkspaceEnv::new(runtime_env, request.cwd.clone());
    let skill_dir = resolve_skill_dir(&request.skill_path)?;
    let manifest = load_runner_manifest(&skill_dir)?;
    let runner = selected_runner(&manifest, overrides.runner.as_deref())?;
    if runner.source.source_type == runx_parser::SourceKind::CliTool
        && request.local_credential.is_some()
    {
        return Err(invalid(
            "local credential process-env delivery is not supported for cli-tool runners",
        ));
    }
    let invocation = runner_invocation(
        &skill_dir,
        runner,
        &request.inputs,
        workspace.env(),
        request.local_credential.as_ref(),
    )?;
    if runner.source.source_type == runx_parser::SourceKind::CliTool {
        return execute_cli_tool_skill_run(
            request, &workspace, &receipts, &manifest, runner, invocation,
        );
    }
    if runner.source.source_type == runx_parser::SourceKind::Graph {
        return execute_graph_skill_run(
            request, overrides, effects, &workspace, &receipts, &manifest, runner,
        );
    }

    execute_agent_skill_run(
        request, overrides, &workspace, &receipts, &manifest, runner, invocation,
    )
}

/// Aggregate result of running a skill's declared inline harness (the
/// `harness.cases` in its runner manifest). Mirrors the publish-harness summary
/// the registry publish flow records: a status, counts, the per-case assertion
/// failures, the case names, the receipts each case sealed, and how many cases
/// exercised a graph (the stable-maturity graph-integration signal).
#[derive(Clone, Debug, Serialize)]
pub struct InlineHarnessReport {
    pub status: &'static str,
    pub case_count: usize,
    pub assertion_error_count: usize,
    pub assertion_errors: Vec<String>,
    pub case_names: Vec<String>,
    pub receipt_ids: Vec<String>,
    pub graph_case_count: usize,
}

impl InlineHarnessReport {
    fn not_declared() -> Self {
        Self {
            status: "not_declared",
            case_count: 0,
            assertion_error_count: 0,
            assertion_errors: Vec::new(),
            case_names: Vec::new(),
            receipt_ids: Vec::new(),
            graph_case_count: 0,
        }
    }
}

/// Run a skill's declared inline harness and summarize it. Each declared case is
/// run through the same path as `runx skill` (so a graph that blocks on an agent
/// step yields `needs_agent`, exactly as a real run would), with the case's
/// runner selected and its caller answers/approvals seeded for a single pass.
/// A skill with no declared harness is `not_declared` (not a failure). The
/// run is `passed` only when every case meets its declared expectation.
pub(crate) fn run_inline_harness_with_effects(
    skill_path: &Path,
    receipt_dir: Option<&Path>,
    effects: &RuntimeEffectRegistry,
) -> Result<InlineHarnessReport, SkillRunError> {
    let skill_dir = resolve_skill_dir(skill_path)?;
    let manifest = load_runner_manifest(&skill_dir)?;
    let Some(harness) = manifest.harness.as_ref() else {
        return Ok(InlineHarnessReport::not_declared());
    };
    if harness.cases.is_empty() {
        return Ok(InlineHarnessReport::not_declared());
    }

    let cwd = std::env::current_dir()
        .map_err(|source| RuntimeError::io("resolving cwd for inline harness", source))?;

    let mut assertion_errors = Vec::new();
    let mut case_names = Vec::with_capacity(harness.cases.len());
    let mut receipt_ids = Vec::new();
    let mut graph_case_count = 0;

    for case in &harness.cases {
        case_names.push(case.name.clone());
        let outcome =
            run_inline_harness_case(&skill_dir, receipt_dir, &manifest, case, &cwd, effects);
        if outcome.is_graph {
            graph_case_count += 1;
        }
        if let Some(receipt_id) = outcome.receipt_id {
            receipt_ids.push(receipt_id);
        }
        if let Some(error) = outcome.assertion_error {
            assertion_errors.push(error);
        }
    }

    let status = if assertion_errors.is_empty() {
        "passed"
    } else {
        "failed"
    };
    Ok(InlineHarnessReport {
        assertion_error_count: assertion_errors.len(),
        status,
        case_count: harness.cases.len(),
        assertion_errors,
        case_names,
        receipt_ids,
        graph_case_count,
    })
}

struct InlineHarnessCaseOutcome {
    is_graph: bool,
    receipt_id: Option<String>,
    assertion_error: Option<String>,
}

fn run_inline_harness_case(
    skill_dir: &Path,
    receipt_dir: Option<&Path>,
    manifest: &SkillRunnerManifest,
    case: &RunnerHarnessCase,
    cwd: &Path,
    effects: &RuntimeEffectRegistry,
) -> InlineHarnessCaseOutcome {
    let is_graph = match selected_runner(manifest, case.runner.as_deref()) {
        Ok(runner) => runner.source.source_type == runx_parser::SourceKind::Graph,
        Err(error) => return inline_harness_case_error(&case.name, error),
    };
    let request = inline_harness_case_request(skill_dir, receipt_dir, case, cwd);
    let overrides = SkillRunOverrides {
        runner: case.runner.clone(),
        seeded_answers: seeded_answers_from_caller(&case.caller),
    };
    match execute_skill_run_with_overrides(&request, &overrides, effects) {
        Ok(output) => InlineHarnessCaseOutcome {
            is_graph,
            receipt_id: receipt_id_from_output(&output),
            assertion_error: inline_harness_expectation_error(case, &output),
        },
        Err(error) => InlineHarnessCaseOutcome {
            is_graph,
            receipt_id: None,
            assertion_error: Some(format!("{}: {error}", case.name)),
        },
    }
}

fn inline_harness_case_request(
    skill_dir: &Path,
    receipt_dir: Option<&Path>,
    case: &RunnerHarnessCase,
    cwd: &Path,
) -> SkillRunRequest {
    let mut env: BTreeMap<String, String> = std::env::vars().collect();
    env.extend(case.env.clone());
    SkillRunRequest {
        skill_path: skill_dir.to_path_buf(),
        receipt_dir: receipt_dir.map(Path::to_path_buf),
        run_id: None,
        answers_path: None,
        inputs: case.inputs.clone(),
        env,
        cwd: cwd.to_path_buf(),
        local_credential: None,
    }
}

fn inline_harness_case_error(
    case_name: &str,
    error: impl std::fmt::Display,
) -> InlineHarnessCaseOutcome {
    InlineHarnessCaseOutcome {
        is_graph: false,
        receipt_id: None,
        assertion_error: Some(format!("{case_name}: {error}")),
    }
}

fn receipt_id_from_output(output: &JsonValue) -> Option<String> {
    output
        .as_object()
        .and_then(|object| object.get("receipt_id"))
        .and_then(JsonValue::as_str)
        .map(str::to_owned)
}

fn inline_harness_expectation_error(
    case: &RunnerHarnessCase,
    output: &JsonValue,
) -> Option<String> {
    let expected = case.expect.status.as_deref()?;
    let actual = inline_harness_actual_status(output);
    (actual != expected).then(|| format!("{}: expected status {expected}, got {actual}", case.name))
}

// Merge a harness case's caller answers + approvals into one map keyed by
// resolution request id, the shape the seeded agent/graph answer lookup expects.
// Approvals are recorded as booleans under their gate id.
fn seeded_answers_from_caller(caller: &HarnessCallerFixture) -> Option<JsonObject> {
    let mut merged = caller.answers.clone().unwrap_or_default();
    if let Some(approvals) = &caller.approvals {
        for (gate, approved) in approvals {
            merged
                .entry(gate.clone())
                .or_insert_with(|| JsonValue::Bool(*approved));
        }
    }
    if merged.is_empty() {
        None
    } else {
        Some(merged)
    }
}

// Map an `execute_skill_run` output onto the harness status vocabulary
// (sealed/failure/needs_agent/policy_denied). A pending run is needs_agent; a
// terminal run is derived from its closure disposition so the mapping matches
// the standalone harness `status_from_disposition`.
fn inline_harness_actual_status(output: &JsonValue) -> &'static str {
    let Some(object) = output.as_object() else {
        return "sealed";
    };
    if object.get("status").and_then(JsonValue::as_str) == Some("needs_agent") {
        return "needs_agent";
    }
    let disposition = object
        .get("closure")
        .and_then(JsonValue::as_object)
        .and_then(|closure| closure.get("disposition"))
        .and_then(JsonValue::as_str);
    match disposition {
        Some("deferred") => "needs_agent",
        Some("blocked") => "policy_denied",
        Some("declined" | "failed" | "killed" | "timed_out" | "superseded") => "failure",
        _ => "sealed",
    }
}

fn execute_agent_skill_run(
    request: &SkillRunRequest,
    overrides: &SkillRunOverrides,
    workspace: &WorkspaceEnv,
    receipts: &ReceiptServices,
    manifest: &SkillRunnerManifest,
    runner: &SkillRunnerDefinition,
    invocation: SkillInvocation,
) -> Result<JsonValue, SkillRunError> {
    let source_type = agent_invocation_source_type(runner.source.source_type.as_str())?;
    let request_id = agent_act_invocation_id(&invocation, source_type);
    let run_id = agent_run_id(request, &request_id)?;
    let resolution_request = agent_request(&invocation, source_type)?;

    // Seeded answers (inline, single pass) take priority over the file-based
    // resume channel; absent both, the run yields to the public agent loop.
    let seeded_answer = overrides
        .seeded_answers
        .as_ref()
        .and_then(|answers| answers.get(&request_id).cloned());
    let answer = match seeded_answer {
        Some(answer) => answer,
        None => match &request.answers_path {
            Some(answers_path) => read_answer(answers_path, &request_id)?,
            None => match try_inline_agent_resolution(&invocation)? {
                #[cfg(feature = "agent")]
                InlineAgentOutcome::Resolved(answer) => answer,
                InlineAgentOutcome::HostDrives => {
                    return Ok(JsonValue::Object(needs_agent_output(
                        &run_id,
                        &request_id,
                        resolution_request,
                    )));
                }
            },
        },
    };
    let stdout = serde_json::to_string(&answer)
        .map_err(|error| SkillRunError::Invalid(format!("failed to serialize answer: {error}")))?;
    let disposition = answer_disposition(&answer);
    let receipt = seal_skill_answer(
        &run_id,
        runner,
        &stdout,
        disposition,
        receipts.signature_config(),
    )?;
    write_skill_receipt(request, workspace, receipts, &receipt)?;

    Ok(JsonValue::Object(sealed_output(
        manifest,
        &run_id,
        &agent_skill_output(stdout, &receipt),
        &answer,
        &receipt,
        contract_json_value(&receipt)?,
    )))
}

/// Outcome of attempting the optional in-process managed-agent loop.
enum InlineAgentOutcome {
    /// The in-kernel loop ran and produced the agent answer payload.
    #[cfg(feature = "agent")]
    Resolved(JsonValue),
    /// No in-process provider is configured; yield to the host loop.
    HostDrives,
}

/// Optionally run the managed-agent loop in-process. This is opt-in: only when a
/// managed-agent provider (currently Anthropic) is configured does the runtime
/// drive the agent itself; otherwise it yields to the host (`needs_agent`), the
/// default shipped behavior. Per-call governance and receipt sealing are the same
/// either way; the loop only adds the bounded autonomous run.
#[cfg(feature = "agent")]
fn try_inline_agent_resolution(
    invocation: &SkillInvocation,
) -> Result<InlineAgentOutcome, SkillRunError> {
    use crate::adapters::agent::{
        AgentAdapterSourceType, AgentResolver, build_managed_agent_act_invocation,
    };
    use crate::adapters::agent_resolver::AnthropicAgentResolver;
    use crate::runtime_http::ReqwestHttpTransport;
    use runx_contracts::ResolutionRequest;

    let source_type = if invocation.source.source_type == runx_parser::SourceKind::Agent {
        AgentAdapterSourceType::Agent
    } else if invocation.source.source_type == runx_parser::SourceKind::AgentStep {
        AgentAdapterSourceType::AgentStep
    } else {
        return Ok(InlineAgentOutcome::HostDrives);
    };

    let config = match crate::config::load_managed_agent_config(
        &invocation.env,
        &invocation.skill_directory,
    )
    .map_err(|error| SkillRunError::Invalid(format!("managed agent config error: {error}")))?
    {
        Some(config) if config.provider.as_str().eq_ignore_ascii_case("anthropic") => config,
        _ => return Ok(InlineAgentOutcome::HostDrives),
    };

    let agent_act = build_managed_agent_act_invocation(invocation, source_type)?;
    let request = ResolutionRequest::AgentAct {
        id: agent_act.id.clone(),
        invocation: Box::new(agent_act),
    };
    let transport = ReqwestHttpTransport::new().map_err(|error| {
        SkillRunError::Invalid(format!("managed agent transport error: {error}"))
    })?;
    let resolver = AnthropicAgentResolver::new(
        transport,
        config.api_key,
        config.model,
        invocation.env.clone(),
        invocation.skill_directory.clone(),
        invocation.credential_delivery.clone(),
    );
    let resolution = resolver
        .resolve(request)
        .map_err(|error| SkillRunError::Invalid(error.sanitized_message().to_owned()))?;
    Ok(InlineAgentOutcome::Resolved(resolution.response.payload))
}

#[cfg(not(feature = "agent"))]
fn try_inline_agent_resolution(
    _invocation: &SkillInvocation,
) -> Result<InlineAgentOutcome, SkillRunError> {
    Ok(InlineAgentOutcome::HostDrives)
}

// rust-style-allow: long-function because graph-backed skill execution keeps
// checkpoint hydration, host resolution, and final receipt sealing in one path.
fn execute_graph_skill_run(
    request: &SkillRunRequest,
    overrides: &SkillRunOverrides,
    effects: &RuntimeEffectRegistry,
    workspace: &WorkspaceEnv,
    receipts: &ReceiptServices,
    manifest: &SkillRunnerManifest,
    runner: &SkillRunnerDefinition,
) -> Result<JsonValue, SkillRunError> {
    let graph = runner
        .source
        .graph
        .clone()
        .ok_or_else(|| invalid("graph runner is missing source.graph"))?;
    let graph_inputs = request
        .inputs
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<JsonObject>();
    let graph = materialize_graph_inputs(graph, &graph_inputs);
    let run_id = graph_run_id(request, runner)?;
    let skill_dir = resolve_skill_dir(&request.skill_path)?;
    let mut env = workspace.graph_env_for_skill(&skill_dir);
    env.insert(RUNX_RUN_ID_ENV.to_owned(), run_id.clone());
    let credential_delivery = credential_delivery_from_local(request.local_credential.as_ref())?;
    let runtime = Runtime::new(
        SkillRunGraphAdapter::default(),
        RuntimeOptions {
            created_at: crate::time::now_iso8601(),
            env,
            receipt_signature: receipts.signature_config().clone(),
            effects: effects.clone(),
            credential_delivery,
        },
    );
    // Seeded answers run a single fresh pass with the answers pre-loaded into the
    // host (they drive the graph to completion, or block -> needs_agent when a
    // step has no seeded answer). The file-based `answers_path` remains the
    // resume-from-checkpoint channel.
    let seeded = overrides.seeded_answers.clone();
    let resume = request.answers_path.is_some() && seeded.is_none();
    let answers = match &seeded {
        Some(seeded) => seeded.clone(),
        None => match &request.answers_path {
            Some(path) => read_answers(path)?,
            None => JsonObject::new(),
        },
    };
    let mut host = SkillRunGraphHost::new(answers);
    let mut checkpoint = if resume {
        read_graph_state(request, workspace, receipts, &run_id, &runner.name)?.checkpoint
    } else {
        runtime.run_graph_until_steps_with_host(&skill_dir, &graph, 0, &mut host)?
    };

    loop {
        let previous_checkpoint = checkpoint.clone();
        match runtime
            .resume_graph_until_steps_with_host(&skill_dir, &graph, checkpoint, 1, &mut host)
        {
            Ok(next_checkpoint) => {
                if next_checkpoint.state.status == GraphStatus::Succeeded {
                    let mut final_host = SkillRunGraphHost::new(JsonObject::new());
                    let run = runtime.seal_completed_graph_checkpoint_with_host(
                        graph.clone(),
                        next_checkpoint,
                        &mut final_host,
                    )?;
                    write_graph_receipts(request, workspace, receipts, &run)?;
                    let payload = graph_payload(&run)?;
                    let output = graph_skill_output(&payload, &run)?;
                    return Ok(JsonValue::Object(sealed_output(
                        manifest,
                        &run_id,
                        &output,
                        &payload,
                        &run.receipt,
                        contract_json_value(&run.receipt)?,
                    )));
                }
                write_graph_state(
                    request,
                    workspace,
                    receipts,
                    &run_id,
                    &GraphSkillRunState {
                        schema: GRAPH_SKILL_STATE_SCHEMA.to_owned(),
                        run_id: run_id.clone(),
                        runner_name: runner.name.clone(),
                        checkpoint: next_checkpoint.clone(),
                    },
                )?;
                checkpoint = next_checkpoint;
            }
            Err(RuntimeError::GraphBlocked { .. }) if host.pending_request().is_some() => {
                write_graph_state(
                    request,
                    workspace,
                    receipts,
                    &run_id,
                    &GraphSkillRunState {
                        schema: GRAPH_SKILL_STATE_SCHEMA.to_owned(),
                        run_id: run_id.clone(),
                        runner_name: runner.name.clone(),
                        checkpoint: previous_checkpoint,
                    },
                )?;
                let (request_id, request_value) = host
                    .pending_request()
                    .ok_or_else(|| invalid("graph blocked without pending request"))?;
                return Ok(JsonValue::Object(needs_agent_output(
                    &run_id,
                    request_id,
                    request_value.clone(),
                )));
            }
            Err(RuntimeError::GraphBlocked { step_id, reason }) => {
                return seal_blocked_graph_skill_run(BlockedGraphSkillRun {
                    request,
                    workspace,
                    receipts,
                    manifest,
                    graph: graph.clone(),
                    checkpoint: previous_checkpoint,
                    run_id: &run_id,
                    runtime: &runtime,
                    step_id: &step_id,
                    reason_code: "graph_blocked",
                    summary: format!("graph {} blocked at {step_id}: {reason}", graph.name),
                });
            }
            Err(RuntimeError::AuthorityDenied {
                verb,
                step_id,
                reason,
            }) => {
                return seal_blocked_graph_skill_run(BlockedGraphSkillRun {
                    request,
                    workspace,
                    receipts,
                    manifest,
                    graph: graph.clone(),
                    checkpoint: previous_checkpoint,
                    run_id: &run_id,
                    runtime: &runtime,
                    step_id: &step_id,
                    reason_code: "authority_denied",
                    summary: format!(
                        "graph {} denied {verb:?} at {step_id}: {reason}",
                        graph.name
                    ),
                });
            }
            Err(error) => return Err(error.into()),
        }
    }
}

struct BlockedGraphSkillRun<'a> {
    request: &'a SkillRunRequest,
    workspace: &'a WorkspaceEnv,
    receipts: &'a ReceiptServices,
    manifest: &'a SkillRunnerManifest,
    graph: ExecutionGraph,
    checkpoint: GraphCheckpoint,
    run_id: &'a str,
    runtime: &'a Runtime<SkillRunGraphAdapter>,
    step_id: &'a str,
    reason_code: &'a str,
    summary: String,
}

fn seal_blocked_graph_skill_run(
    context: BlockedGraphSkillRun<'_>,
) -> Result<JsonValue, SkillRunError> {
    let mut final_host = SkillRunGraphHost::new(JsonObject::new());
    let run = context.runtime.seal_blocked_graph_checkpoint_with_host(
        context.graph,
        context.checkpoint,
        context.step_id,
        context.reason_code,
        context.summary,
        &mut final_host,
    )?;
    write_graph_receipts(context.request, context.workspace, context.receipts, &run)?;
    let payload = graph_payload(&run)?;
    let output = graph_skill_output(&payload, &run)?;
    Ok(JsonValue::Object(sealed_output(
        context.manifest,
        context.run_id,
        &output,
        &payload,
        &run.receipt,
        contract_json_value(&run.receipt)?,
    )))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GraphSkillRunState {
    schema: String,
    run_id: String,
    runner_name: String,
    checkpoint: GraphCheckpoint,
}

type SourceHandlerFn = fn(SkillInvocation) -> Result<SkillOutput, RuntimeError>;

#[derive(Clone, Copy, Debug)]
struct SourceHandler {
    source_type: &'static str,
    handler: SourceHandlerFn,
}

#[derive(Clone, Debug)]
struct SourceAdapterRegistry {
    handlers: Vec<SourceHandler>,
}

impl SourceAdapterRegistry {
    fn builtins() -> Self {
        Self {
            handlers: builtin_source_handlers(),
        }
    }

    fn invoke(&self, request: SkillInvocation) -> Result<SkillOutput, RuntimeError> {
        let source_type = request.source.source_type.as_str();
        let Some(handler) = self
            .handlers
            .iter()
            .find(|registered| registered.source_type == source_type)
            .map(|registered| registered.handler)
        else {
            return Err(RuntimeError::UnsupportedSource {
                source_kind: source_type.to_owned(),
            });
        };
        handler(request)
    }
}

fn builtin_source_handlers() -> Vec<SourceHandler> {
    vec![
        #[cfg(feature = "cli-tool")]
        SourceHandler {
            source_type: "cli-tool",
            handler: invoke_graph_cli_tool,
        },
        #[cfg(feature = "catalog")]
        SourceHandler {
            source_type: "catalog",
            handler: invoke_graph_catalog_tool,
        },
        #[cfg(feature = "external-adapter")]
        SourceHandler {
            source_type: "external-adapter",
            handler: invoke_graph_external_adapter,
        },
        #[cfg(feature = "http")]
        SourceHandler {
            source_type: "http",
            handler: invoke_graph_http,
        },
        #[cfg(feature = "mcp")]
        SourceHandler {
            source_type: "mcp",
            handler: invoke_graph_mcp,
        },
        #[cfg(feature = "thread-outbox-provider")]
        SourceHandler {
            source_type: "thread-outbox-provider",
            handler: invoke_graph_thread_outbox_provider,
        },
    ]
}

#[derive(Clone, Debug)]
pub(crate) struct SkillRunGraphAdapter {
    sources: SourceAdapterRegistry,
}

impl Default for SkillRunGraphAdapter {
    fn default() -> Self {
        Self {
            sources: SourceAdapterRegistry::builtins(),
        }
    }
}

impl crate::adapter::SkillAdapter for SkillRunGraphAdapter {
    fn adapter_type(&self) -> &'static str {
        "skill-run-graph"
    }

    fn invoke(&self, request: SkillInvocation) -> Result<SkillOutput, RuntimeError> {
        self.sources.invoke(request)
    }
}

#[cfg(feature = "cli-tool")]
fn invoke_graph_cli_tool(request: SkillInvocation) -> Result<SkillOutput, RuntimeError> {
    CliToolAdapter.invoke(request)
}

#[cfg(feature = "catalog")]
fn invoke_graph_catalog_tool(request: SkillInvocation) -> Result<SkillOutput, RuntimeError> {
    crate::adapters::catalog::CatalogAdapter::default().invoke(request)
}

#[cfg(feature = "external-adapter")]
fn invoke_graph_external_adapter(request: SkillInvocation) -> Result<SkillOutput, RuntimeError> {
    crate::adapters::external_adapter::ExternalAdapterSkillAdapter::default().invoke(request)
}

#[cfg(feature = "http")]
fn invoke_graph_http(request: SkillInvocation) -> Result<SkillOutput, RuntimeError> {
    crate::adapters::http::HttpSkillAdapter.invoke(request)
}

#[cfg(feature = "mcp")]
fn invoke_graph_mcp(request: SkillInvocation) -> Result<SkillOutput, RuntimeError> {
    crate::adapter::SkillAdapter::invoke(&crate::adapters::mcp::McpAdapter::default(), request)
}

#[cfg(feature = "thread-outbox-provider")]
fn invoke_graph_thread_outbox_provider(
    request: SkillInvocation,
) -> Result<SkillOutput, RuntimeError> {
    crate::adapters::thread_outbox_provider::ThreadOutboxProviderSkillAdapter::default()
        .invoke(request)
}

#[derive(Default)]
struct SkillRunGraphHost {
    answers: JsonObject,
    pending: Vec<(String, JsonValue)>,
}

impl SkillRunGraphHost {
    fn new(answers: JsonObject) -> Self {
        Self {
            answers,
            pending: Vec::new(),
        }
    }

    fn pending_request(&self) -> Option<(&str, &JsonValue)> {
        self.pending
            .first()
            .map(|(request_id, request)| (request_id.as_str(), request))
    }
}

impl Host for SkillRunGraphHost {
    fn report(&mut self, _event: runx_contracts::ExecutionEvent) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn resolve(
        &mut self,
        request: ResolutionRequest,
    ) -> Result<Option<ResolutionResponse>, RuntimeError> {
        let request_id = resolution_request_id(&request).to_owned();
        if let Some(answer) = self.answers.get(&request_id) {
            return Ok(Some(ResolutionResponse {
                actor: ResolutionResponseActor::Agent,
                payload: answer.clone(),
            }));
        }
        let request_value = serde_json::to_value(&request)
            .and_then(serde_json::from_value)
            .map_err(|source| RuntimeError::json("serializing graph resolution request", source))?;
        self.pending.push((request_id, request_value));
        Ok(None)
    }
}

fn resolution_request_id(request: &ResolutionRequest) -> &str {
    match request {
        ResolutionRequest::Input { id, .. }
        | ResolutionRequest::Approval { id, .. }
        | ResolutionRequest::AgentAct { id, .. } => id.as_str(),
    }
}

fn graph_run_id(
    request: &SkillRunRequest,
    runner: &SkillRunnerDefinition,
) -> Result<String, SkillRunError> {
    match (&request.run_id, &request.answers_path) {
        (Some(run_id), Some(_)) => Ok(run_id.clone()),
        (Some(_), None) => Err(invalid("runx skill --run-id requires --answers")),
        (None, Some(_)) => Err(invalid("runx skill --answers requires --run-id")),
        (None, None) => {
            let input_bytes = serde_json::to_vec(&request.inputs).unwrap_or_default();
            let digest = sha256_hex(&input_bytes);
            Ok(format!(
                "run_{}_{}",
                identifier_segment(&runner.name),
                digest.chars().take(12).collect::<String>()
            ))
        }
    }
}

fn read_answers(path: &Path) -> Result<JsonObject, SkillRunError> {
    let raw = fs::read_to_string(path)
        .map_err(|source| RuntimeError::io(format!("reading {}", path.display()), source))?;
    let value = serde_json::from_str::<JsonValue>(&raw).map_err(|source| {
        RuntimeError::json(format!("parsing answers file {}", path.display()), source)
    })?;
    let answers = match value {
        JsonValue::Object(mut object) => match object.remove("answers") {
            Some(JsonValue::Object(nested)) => nested,
            Some(_) => return Err(invalid("answers field must be a JSON object")),
            None => object,
        },
        _ => return Err(invalid("answers file must be a JSON object")),
    };
    Ok(answers)
}

fn graph_state_path(
    request: &SkillRunRequest,
    workspace: &WorkspaceEnv,
    receipts: &ReceiptServices,
    run_id: &str,
) -> PathBuf {
    let receipt_path = receipts.resolve_path(workspace, request.receipt_dir.as_deref(), None);
    receipt_path
        .path
        .join("runs")
        .join(format!("{}.graph-state.json", identifier_segment(run_id)))
}

fn write_graph_state(
    request: &SkillRunRequest,
    workspace: &WorkspaceEnv,
    receipts: &ReceiptServices,
    run_id: &str,
    state: &GraphSkillRunState,
) -> Result<(), SkillRunError> {
    let path = graph_state_path(request, workspace, receipts, run_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|source| RuntimeError::io(format!("creating {}", parent.display()), source))?;
    }
    let bytes = serde_json::to_vec_pretty(state)
        .map_err(|source| RuntimeError::json("serializing graph state", source))?;
    let temp_path = graph_state_temp_path(&path);
    fs::write(&temp_path, bytes)
        .map_err(|source| RuntimeError::io(format!("writing {}", temp_path.display()), source))?;
    fs::rename(&temp_path, &path).map_err(|source| {
        let _ignored = fs::remove_file(&temp_path);
        RuntimeError::io(
            format!("replacing {} with {}", path.display(), temp_path.display()),
            source,
        )
    })?;
    Ok(())
}

fn read_graph_state(
    request: &SkillRunRequest,
    workspace: &WorkspaceEnv,
    receipts: &ReceiptServices,
    run_id: &str,
    runner_name: &str,
) -> Result<GraphSkillRunState, SkillRunError> {
    let path = graph_state_path(request, workspace, receipts, run_id);
    let raw = fs::read_to_string(&path)
        .map_err(|source| RuntimeError::io(format!("reading {}", path.display()), source))?;
    let state: GraphSkillRunState = serde_json::from_str(&raw).map_err(|source| {
        invalid(format!(
            "graph state file {} is malformed; the run cannot resume safely without a valid checkpoint: {source}",
            path.display()
        ))
    })?;
    if state.schema != GRAPH_SKILL_STATE_SCHEMA {
        return Err(invalid(format!(
            "graph state schema mismatch for run {run_id}: expected {GRAPH_SKILL_STATE_SCHEMA}, got {}",
            state.schema
        )));
    }
    if state.run_id != run_id {
        return Err(invalid(format!(
            "graph state run_id mismatch: expected {run_id}, got {}",
            state.run_id
        )));
    }
    if state.runner_name != runner_name {
        return Err(invalid(format!(
            "graph state runner_name mismatch for run {run_id}: expected {runner_name}, got {}",
            state.runner_name
        )));
    }
    Ok(state)
}

fn graph_state_temp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("graph-state.json");
    path.with_file_name(format!("{file_name}.{}.tmp", std::process::id()))
}

fn graph_payload(run: &GraphRun) -> Result<JsonValue, SkillRunError> {
    let mut payload = JsonObject::new();
    payload.insert(
        "graph".to_owned(),
        JsonValue::String(run.graph.name.clone()),
    );
    payload.insert(
        "graph_status".to_owned(),
        JsonValue::String(format!("{:?}", run.state.status)),
    );
    let mut step_outputs = JsonObject::new();
    let mut step_summaries = Vec::new();
    for step in &run.steps {
        let mut summary = JsonObject::new();
        summary.insert(
            "step_id".to_owned(),
            JsonValue::String(step.step_id.clone()),
        );
        summary.insert("skill".to_owned(), JsonValue::String(step.skill.clone()));
        summary.insert(
            "status".to_owned(),
            JsonValue::String(if step.output.succeeded() {
                "success".to_owned()
            } else {
                "failure".to_owned()
            }),
        );
        summary.insert(
            "receipt_id".to_owned(),
            JsonValue::String(step.receipt.id.to_string()),
        );
        step_summaries.push(JsonValue::Object(summary));
        step_outputs.insert(
            step.step_id.clone(),
            JsonValue::Object(step.outputs.clone()),
        );
    }
    payload.insert("steps".to_owned(), JsonValue::Array(step_summaries));
    payload.insert("step_outputs".to_owned(), JsonValue::Object(step_outputs));
    Ok(JsonValue::Object(payload))
}

fn graph_skill_output(payload: &JsonValue, run: &GraphRun) -> Result<SkillOutput, SkillRunError> {
    let stdout = serde_json::to_string(payload)
        .map_err(|source| RuntimeError::json("serializing graph payload", source))?;
    Ok(SkillOutput {
        status: if run.state.status == GraphStatus::Succeeded {
            InvocationStatus::Success
        } else {
            InvocationStatus::Failure
        },
        stdout,
        stderr: String::new(),
        exit_code: Some(0),
        duration_ms: 0,
        metadata: JsonObject::new(),
    })
}

fn agent_run_id(request: &SkillRunRequest, request_id: &str) -> Result<String, SkillRunError> {
    match (&request.run_id, &request.answers_path) {
        (Some(run_id), Some(_)) => Ok(run_id.clone()),
        (Some(_), None) => Err(invalid("runx skill --run-id requires --answers")),
        (None, Some(_)) => Err(invalid("runx skill --answers requires --run-id")),
        (None, None) => Ok(format!("run_{}", identifier_segment(request_id))),
    }
}

fn agent_skill_output(stdout: String, receipt: &runx_contracts::Receipt) -> SkillOutput {
    let succeeded = receipt.seal.disposition == ClosureDisposition::Closed;
    SkillOutput {
        status: if succeeded {
            InvocationStatus::Success
        } else {
            InvocationStatus::Failure
        },
        stdout,
        stderr: if succeeded {
            String::new()
        } else {
            format!(
                "agent act closed with {}",
                closure_disposition_label(&receipt.seal.disposition)
            )
        },
        exit_code: succeeded.then_some(0),
        duration_ms: 0,
        metadata: JsonObject::new(),
    }
}

fn resolve_skill_dir(path: &Path) -> Result<PathBuf, SkillRunError> {
    if path.is_dir() {
        return Ok(path.to_path_buf());
    }
    if path.file_name().and_then(|name| name.to_str()) == Some("SKILL.md") {
        return path
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| invalid(format!("skill path has no parent: {}", path.display())));
    }
    Err(invalid(format!(
        "runx skill requires a skill package directory or SKILL.md: {}",
        path.display()
    )))
}

fn load_runner_manifest(skill_dir: &Path) -> Result<SkillRunnerManifest, SkillRunError> {
    let manifest_path = skill_dir.join("X.yaml");
    let raw = fs::read_to_string(&manifest_path).map_err(|source| {
        RuntimeError::io(format!("reading {}", manifest_path.display()), source)
    })?;
    let parsed = parse_runner_manifest_yaml(&raw).map_err(RuntimeError::from)?;
    validate_runner_manifest(parsed)
        .map_err(RuntimeError::from)
        .map_err(Into::into)
}

fn selected_runner<'a>(
    manifest: &'a SkillRunnerManifest,
    requested: Option<&str>,
) -> Result<&'a SkillRunnerDefinition, SkillRunError> {
    if let Some(name) = requested {
        return manifest
            .runners
            .get(name)
            .ok_or_else(|| invalid(format!("runner {name} is not declared in the manifest")));
    }
    let defaults = manifest
        .runners
        .values()
        .filter(|runner| runner.default)
        .collect::<Vec<_>>();
    match defaults.as_slice() {
        [runner] => Ok(*runner),
        [] if manifest.runners.len() == 1 => manifest
            .runners
            .values()
            .next()
            .ok_or_else(|| invalid("runner manifest declares no runners")),
        [] => Err(invalid("runner manifest has no default runner")),
        _ => Err(invalid("runner manifest declares multiple default runners")),
    }
}

fn runner_invocation(
    skill_dir: &Path,
    runner: &SkillRunnerDefinition,
    inputs: &BTreeMap<String, JsonValue>,
    env: &BTreeMap<String, String>,
    local_credential: Option<&crate::execution::orchestrator::LocalCredentialDescriptor>,
) -> Result<SkillInvocation, SkillRunError> {
    if !matches!(
        runner.source.source_type.as_str(),
        "agent" | "agent-task" | "cli-tool" | "graph"
    ) {
        return Err(invalid(format!(
            "runx skill native execution only supports agent, agent-task, graph, and cli-tool runners, got {}",
            runner.source.source_type
        )));
    }
    let credential_delivery = credential_delivery_from_local(local_credential)?;
    Ok(SkillInvocation {
        skill_name: runner.name.clone(),
        source: runner.source.clone(),
        inputs: inputs.clone().into_iter().collect(),
        resolved_inputs: JsonObject::new(),
        current_context: Vec::new(),
        skill_directory: skill_dir.to_path_buf(),
        env: env.clone(),
        credential_delivery,
    })
}

fn credential_delivery_from_local(
    local_credential: Option<&crate::execution::orchestrator::LocalCredentialDescriptor>,
) -> Result<crate::credentials::CredentialDelivery, SkillRunError> {
    Ok(match local_credential {
        Some(descriptor) => crate::credentials::CredentialDelivery::from_local_descriptor(
            descriptor.provider.clone(),
            descriptor.auth_mode.clone(),
            descriptor.env_var.clone(),
            descriptor.material_ref.clone(),
            descriptor.scopes.clone(),
            descriptor.secret.clone(),
        )
        .map_err(|error| invalid(format!("local credential provision failed: {error}")))?,
        None => crate::credentials::CredentialDelivery::none(),
    })
}

#[cfg(feature = "cli-tool")]
fn execute_cli_tool_skill_run(
    request: &SkillRunRequest,
    workspace: &WorkspaceEnv,
    receipts: &ReceiptServices,
    manifest: &SkillRunnerManifest,
    runner: &SkillRunnerDefinition,
    invocation: SkillInvocation,
) -> Result<JsonValue, SkillRunError> {
    if request.answers_path.is_some() {
        return Err(invalid(
            "runx skill cli-tool runners do not support --answers",
        ));
    }
    let run_id = request
        .run_id
        .clone()
        .unwrap_or_else(|| cli_tool_run_id(runner, &request.inputs));
    let credential_observation = invocation.credential_delivery.public_observation().cloned();
    let mut output = CliToolAdapter.invoke(invocation)?;
    if let Some(observation) = &credential_observation {
        record_credential_observation(&mut output, observation)?;
    }
    let disposition = if output.succeeded() {
        ClosureDisposition::Closed
    } else {
        ClosureDisposition::Failed
    };
    let receipt = seal_skill_output(
        &run_id,
        runner,
        &output,
        disposition.clone(),
        format!("process_{}", closure_disposition_label(&disposition)),
        format!("cli-tool {} completed", runner.name),
        receipts.signature_config(),
    )?;
    write_skill_receipt(request, workspace, receipts, &receipt)?;
    Ok(JsonValue::Object(sealed_output(
        manifest,
        &run_id,
        &output,
        &parse_output_payload(&output.stdout),
        &receipt,
        contract_json_value(&receipt)?,
    )))
}

#[cfg(not(feature = "cli-tool"))]
fn execute_cli_tool_skill_run(
    _request: &SkillRunRequest,
    _workspace: &WorkspaceEnv,
    _receipts: &ReceiptServices,
    _manifest: &SkillRunnerManifest,
    _runner: &SkillRunnerDefinition,
    _invocation: SkillInvocation,
) -> Result<JsonValue, SkillRunError> {
    Err(invalid(
        "runx skill cli-tool execution is unavailable because runx-runtime was built without the cli-tool feature",
    ))
}

fn write_skill_receipt(
    request: &SkillRunRequest,
    workspace: &WorkspaceEnv,
    receipts: &ReceiptServices,
    receipt: &runx_contracts::Receipt,
) -> Result<(), SkillRunError> {
    let receipt_path = receipts.resolve_path(workspace, request.receipt_dir.as_deref(), None);
    receipts
        .write_local_receipt(receipt, &receipt_path)
        .map_err(Into::into)
}

fn write_graph_receipts(
    request: &SkillRunRequest,
    workspace: &WorkspaceEnv,
    receipts: &ReceiptServices,
    run: &GraphRun,
) -> Result<(), SkillRunError> {
    for step in &run.steps {
        write_skill_receipt(request, workspace, receipts, &step.receipt)?;
    }
    write_skill_receipt(request, workspace, receipts, &run.receipt)
}

fn agent_invocation_source_type(
    value: &str,
) -> Result<AgentActInvocationSourceType, SkillRunError> {
    AgentActInvocationSourceType::from_contract_value(value)
        .ok_or_else(|| invalid(format!("unsupported agent source type {value}")))
}

fn agent_request(
    invocation: &SkillInvocation,
    source_type: AgentActInvocationSourceType,
) -> Result<JsonValue, SkillRunError> {
    contract_json_value(&agent_act_resolution_request(invocation, source_type)?)
}

fn needs_agent_output(run_id: &str, request_id: &str, request: JsonValue) -> JsonObject {
    let mut output = JsonObject::new();
    output.insert(
        "schema".to_owned(),
        JsonValue::String(SKILL_RUN_SCHEMA.to_owned()),
    );
    output.insert(
        "status".to_owned(),
        JsonValue::String("needs_agent".to_owned()),
    );
    output.insert("run_id".to_owned(), JsonValue::String(run_id.to_owned()));
    output.insert(
        "requests".to_owned(),
        JsonValue::Array(vec![request_for_public_loop(request_id, request)]),
    );
    output
}

fn request_for_public_loop(request_id: &str, request: JsonValue) -> JsonValue {
    let mut object = match request {
        JsonValue::Object(object) => object,
        _ => JsonObject::new(),
    };
    object.insert("id".to_owned(), JsonValue::String(request_id.to_owned()));
    object
        .entry("kind".to_owned())
        .or_insert_with(|| JsonValue::String("agent_act".to_owned()));
    JsonValue::Object(object)
}

fn read_answer(path: &Path, request_id: &str) -> Result<JsonValue, SkillRunError> {
    let raw = fs::read_to_string(path)
        .map_err(|source| RuntimeError::io(format!("reading {}", path.display()), source))?;
    let value = serde_json::from_str::<JsonValue>(&raw).map_err(|source| {
        RuntimeError::json(format!("parsing answers file {}", path.display()), source)
    })?;
    let answers = match &value {
        JsonValue::Object(object) => match object.get("answers") {
            Some(JsonValue::Object(nested)) => nested,
            _ => object,
        },
        _ => return Err(invalid("answers file must be a JSON object")),
    };
    answers
        .get(request_id)
        .cloned()
        .ok_or_else(|| invalid(format!("answers file did not include {request_id}")))
}

fn seal_skill_answer(
    run_id: &str,
    runner: &SkillRunnerDefinition,
    stdout: &str,
    disposition: ClosureDisposition,
    signature_config: &RuntimeReceiptSignatureConfig,
) -> Result<runx_contracts::Receipt, SkillRunError> {
    let disposition_label = closure_disposition_label(&disposition);
    let succeeded = disposition == ClosureDisposition::Closed;
    let status = if succeeded {
        InvocationStatus::Success
    } else {
        InvocationStatus::Failure
    };
    let skill_output = SkillOutput {
        status,
        stdout: stdout.to_owned(),
        stderr: if succeeded {
            String::new()
        } else {
            format!("agent act closed with {disposition_label}")
        },
        exit_code: succeeded.then_some(0),
        duration_ms: 0,
        metadata: JsonObject::new(),
    };
    seal_skill_output(
        run_id,
        runner,
        &skill_output,
        disposition,
        format!("agent_act_{disposition_label}"),
        format!("agent act closed with {disposition_label}"),
        signature_config,
    )
}

/// Record the non-secret credential-delivery observation on the skill output so
/// the sealed receipt carries an auditable trace that a credential was
/// provisioned for the run. The observation contains no secret material.
#[cfg(feature = "cli-tool")]
fn record_credential_observation(
    output: &mut SkillOutput,
    observation: &runx_contracts::CredentialDeliveryObservation,
) -> Result<(), SkillRunError> {
    let value: JsonValue = serde_json::to_value(observation)
        .and_then(serde_json::from_value)
        .map_err(|error| {
            SkillRunError::Invalid(format!(
                "serializing credential delivery observation: {error}"
            ))
        })?;
    output.metadata.insert(
        crate::adapter::CREDENTIAL_DELIVERY_OBSERVATIONS_METADATA.to_owned(),
        JsonValue::Array(vec![value]),
    );
    Ok(())
}

fn seal_skill_output(
    run_id: &str,
    runner: &SkillRunnerDefinition,
    output: &SkillOutput,
    disposition: ClosureDisposition,
    reason_code: String,
    summary: String,
    signature_config: &RuntimeReceiptSignatureConfig,
) -> Result<runx_contracts::Receipt, SkillRunError> {
    let graph_name = identifier_segment(run_id);
    let step_id = identifier_segment(&runner.name);
    Ok(step_receipt_with_disposition_and_policy(
        StepReceiptWithDisposition {
            graph_name: &graph_name,
            step_id: &step_id,
            attempt: 1,
            output,
            created_at: &crate::time::now_iso8601(),
            disposition,
            reason_code,
            summary,
        },
        signature_config.signature_policy(),
    )?)
}

fn answer_disposition(answer: &JsonValue) -> ClosureDisposition {
    match answer
        .as_object()
        .and_then(|object| object.get("closure"))
        .and_then(JsonValue::as_object)
        .and_then(|closure| closure.get("disposition"))
        .and_then(JsonValue::as_str)
    {
        Some("deferred") => ClosureDisposition::Deferred,
        Some("superseded") => ClosureDisposition::Superseded,
        Some("declined") => ClosureDisposition::Declined,
        Some("blocked") => ClosureDisposition::Blocked,
        Some("failed") => ClosureDisposition::Failed,
        Some("killed") => ClosureDisposition::Killed,
        Some("timed_out") => ClosureDisposition::TimedOut,
        _ => ClosureDisposition::Closed,
    }
}

fn sealed_output(
    manifest: &SkillRunnerManifest,
    run_id: &str,
    skill_output: &SkillOutput,
    payload: &JsonValue,
    receipt: &runx_contracts::Receipt,
    receipt_value: JsonValue,
) -> JsonObject {
    let mut execution = JsonObject::new();
    execution.insert(
        "stdout".to_owned(),
        JsonValue::String(skill_output.stdout.clone()),
    );
    execution.insert(
        "stderr".to_owned(),
        JsonValue::String(skill_output.stderr.clone()),
    );
    execution.insert(
        "exit_code".to_owned(),
        skill_output.exit_code.map_or(JsonValue::Null, |exit_code| {
            JsonValue::Number(JsonNumber::I64(i64::from(exit_code)))
        }),
    );
    execution.insert("structured_output".to_owned(), payload.clone());
    execution.insert("skill_claim".to_owned(), payload.clone());
    if let Some(observations) = skill_output
        .metadata
        .get(crate::adapter::CREDENTIAL_DELIVERY_OBSERVATIONS_METADATA)
    {
        execution.insert(
            crate::adapter::CREDENTIAL_DELIVERY_OBSERVATIONS_METADATA.to_owned(),
            observations.clone(),
        );
    }

    let mut output = JsonObject::new();
    output.insert(
        "schema".to_owned(),
        JsonValue::String(SKILL_RUN_SCHEMA.to_owned()),
    );
    output.insert("status".to_owned(), JsonValue::String("sealed".to_owned()));
    output.insert(
        "skill_name".to_owned(),
        JsonValue::String(manifest.skill.clone().unwrap_or_else(|| "skill".to_owned())),
    );
    output.insert("run_id".to_owned(), JsonValue::String(run_id.to_owned()));
    output.insert(
        "receipt_id".to_owned(),
        JsonValue::String(receipt.id.to_string()),
    );
    output.insert(
        "closure".to_owned(),
        JsonValue::Object(closure_output(&receipt.seal)),
    );
    output.insert("receipt".to_owned(), receipt_value);
    output.insert("execution".to_owned(), JsonValue::Object(execution));
    output.insert("payload".to_owned(), payload.clone());
    output
}

fn closure_output(seal: &runx_contracts::Seal) -> JsonObject {
    let mut closure = JsonObject::new();
    closure.insert(
        "disposition".to_owned(),
        JsonValue::String(closure_disposition_label(&seal.disposition).to_owned()),
    );
    closure.insert(
        "reason_code".to_owned(),
        JsonValue::String(seal.reason_code.to_string()),
    );
    closure.insert(
        "summary".to_owned(),
        JsonValue::String(seal.summary.to_string()),
    );
    closure.insert(
        "closed_at".to_owned(),
        JsonValue::String(seal.closed_at.to_string()),
    );
    closure
}

fn closure_disposition_label(disposition: &ClosureDisposition) -> &'static str {
    match disposition {
        ClosureDisposition::Closed => "closed",
        ClosureDisposition::Deferred => "deferred",
        ClosureDisposition::Superseded => "superseded",
        ClosureDisposition::Declined => "declined",
        ClosureDisposition::Blocked => "blocked",
        ClosureDisposition::Failed => "failed",
        ClosureDisposition::Killed => "killed",
        ClosureDisposition::TimedOut => "timed_out",
    }
}

fn normalize_request_id(value: &str) -> String {
    let mut normalized = String::new();
    let mut replaced = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-') {
            normalized.push(character);
            replaced = false;
        } else if !replaced {
            normalized.push('_');
            replaced = true;
        }
    }
    normalized
}

fn identifier_segment(value: &str) -> String {
    normalize_request_id(value)
        .trim_matches(['.', '_', '-'])
        .replace('.', "-")
}

#[cfg(feature = "cli-tool")]
fn cli_tool_run_id(runner: &SkillRunnerDefinition, inputs: &BTreeMap<String, JsonValue>) -> String {
    let input_bytes = serde_json::to_vec(inputs).unwrap_or_default();
    let digest = Sha256::digest(input_bytes);
    format!(
        "run_{}_{}",
        identifier_segment(&runner.name),
        hex_prefix(&digest, 12)
    )
}

#[cfg(feature = "cli-tool")]
fn hex_prefix(bytes: &[u8], chars: usize) -> String {
    let full = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    full.chars().take(chars).collect()
}

#[cfg(feature = "cli-tool")]
fn parse_output_payload(stdout: &str) -> JsonValue {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return JsonValue::String(String::new());
    }
    serde_json::from_str(trimmed).unwrap_or_else(|_| JsonValue::String(trimmed.to_owned()))
}

fn contract_json_value(value: &impl serde::Serialize) -> Result<JsonValue, SkillRunError> {
    let value = serde_json::to_value(value)
        .map_err(|source| RuntimeError::json("serializing native skill contract value", source))?;
    serde_json::from_value(value).map_err(|source| {
        RuntimeError::json("normalizing native skill contract value", source).into()
    })
}

fn invalid(message: impl Into<String>) -> SkillRunError {
    SkillRunError::Invalid(message.into())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use runx_parser::{SkillSource, SourceKind};

    use super::*;
    use crate::adapter::SkillAdapter;

    #[test]
    fn graph_source_registry_fails_closed_on_unregistered_source() {
        let mut raw = JsonObject::new();
        raw.insert("type".to_owned(), JsonValue::String("a2a".to_owned()));
        let invocation = SkillInvocation {
            skill_name: "fixture-a2a".to_owned(),
            source: SkillSource {
                source_type: SourceKind::A2a,
                command: None,
                args: Vec::new(),
                cwd: None,
                timeout_seconds: None,
                input_mode: None,
                sandbox: None,
                server: None,
                catalog_ref: None,
                tool: None,
                arguments: None,
                agent_card_url: None,
                agent_identity: None,
                agent: None,
                task: None,
                hook: None,
                outputs: None,
                graph: None,
                http: None,
                raw,
            },
            inputs: JsonObject::new(),
            resolved_inputs: JsonObject::new(),
            current_context: Vec::new(),
            skill_directory: PathBuf::from("."),
            env: BTreeMap::new(),
            credential_delivery: crate::credentials::CredentialDelivery::none(),
        };

        let result = SkillRunGraphAdapter::default().invoke(invocation);
        assert!(
            matches!(
                &result,
                Err(RuntimeError::UnsupportedSource { source_kind }) if source_kind == "a2a"
            ),
            "unexpected unregistered graph source result: {result:?}"
        );
    }

    #[cfg(feature = "external-adapter")]
    #[test]
    fn graph_source_registry_routes_external_adapter() {
        let mut raw = JsonObject::new();
        raw.insert(
            "type".to_owned(),
            JsonValue::String("external-adapter".to_owned()),
        );
        let invocation = SkillInvocation {
            skill_name: "fixture-external".to_owned(),
            source: SkillSource {
                source_type: SourceKind::ExternalAdapter,
                command: None,
                args: Vec::new(),
                cwd: None,
                timeout_seconds: None,
                input_mode: None,
                sandbox: None,
                server: None,
                catalog_ref: None,
                tool: None,
                arguments: None,
                agent_card_url: None,
                agent_identity: None,
                agent: None,
                task: None,
                hook: None,
                outputs: None,
                graph: None,
                http: None,
                raw,
            },
            inputs: JsonObject::new(),
            resolved_inputs: JsonObject::new(),
            current_context: Vec::new(),
            skill_directory: PathBuf::from("."),
            env: BTreeMap::new(),
            credential_delivery: crate::credentials::CredentialDelivery::none(),
        };

        let result = SkillRunGraphAdapter::default().invoke(invocation);
        assert!(
            matches!(&result, Err(RuntimeError::SkillFailed { .. })),
            "external-adapter source should route to the external adapter and fail on the \
             missing manifest, not fall through as UnsupportedSource; got: {result:?}"
        );
    }

    #[cfg(feature = "thread-outbox-provider")]
    #[test]
    fn graph_source_registry_routes_thread_outbox_provider() {
        let mut raw = JsonObject::new();
        raw.insert(
            "type".to_owned(),
            JsonValue::String("thread-outbox-provider".to_owned()),
        );
        let invocation = SkillInvocation {
            skill_name: "fixture-thread-outbox-provider".to_owned(),
            source: SkillSource {
                source_type: SourceKind::ThreadOutboxProvider,
                command: None,
                args: Vec::new(),
                cwd: None,
                timeout_seconds: None,
                input_mode: None,
                sandbox: None,
                server: None,
                catalog_ref: None,
                tool: None,
                arguments: None,
                agent_card_url: None,
                agent_identity: None,
                agent: None,
                task: None,
                hook: None,
                outputs: None,
                graph: None,
                http: None,
                raw,
            },
            inputs: JsonObject::new(),
            resolved_inputs: JsonObject::new(),
            current_context: Vec::new(),
            skill_directory: PathBuf::from("."),
            env: BTreeMap::new(),
            credential_delivery: crate::credentials::CredentialDelivery::none(),
        };

        let result = SkillRunGraphAdapter::default().invoke(invocation);
        assert!(
            matches!(&result, Err(RuntimeError::SkillFailed { .. })),
            "thread-outbox-provider source should route to the Rust provider front and fail on \
             missing config, not fall through as UnsupportedSource; got: {result:?}"
        );
    }
}
