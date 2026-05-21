// rust-style-allow: large-file because harness replay owns fixture loading,
// adapter invocation, receipt assertion, and graph replay sealing as one
// deterministic proof path until MCP replay creates a separate module boundary.
use std::fs;
use std::path::{Path, PathBuf};

use runx_contracts::{
    ClosureDisposition, ExecutionEvent, HarnessReceipt, JsonObject, JsonValue, ResolutionRequest,
    ResolutionResponse, ResolutionResponseActor,
};
use runx_parser::{
    SkillRunnerDefinition, SkillRunnerManifest, parse_runner_manifest_yaml,
    validate_runner_manifest,
};
use thiserror::Error;

use super::super::graph::load_skill;
use super::assertions::{assert_expectations, status_from_disposition};
use super::fixtures::{
    HarnessExpectedStatus, HarnessFixture, HarnessFixtureError, HarnessFixtureKind,
    fixture_kind_name, load_harness_fixture,
};
use crate::RuntimeError;
use crate::adapter::{InvocationStatus, SkillAdapter, SkillInvocation, SkillOutput};
use crate::agent_invocation::{AgentActInvocationSourceType, agent_act_invocation_id};
use crate::execution::runner::{GraphRun, Runtime, RuntimeOptions, StepRun};
use crate::host::Host;
use crate::payment_ledger::persist_x402_payment_ledger_projection_event;
use crate::receipts::paths::{RUNX_RECEIPT_DIR_ENV, ReceiptPathInputs, resolve_receipt_path};
use crate::receipts::{
    StepReceiptWithDisposition, graph_receipt_with_disposition, step_receipt_with_disposition,
};

#[derive(Clone, Debug)]
pub struct HarnessReplayOutput {
    pub fixture: HarnessFixture,
    pub status: HarnessExpectedStatus,
    pub receipt: HarnessReceipt,
    pub step_receipts: Vec<HarnessReceipt>,
    pub skill_output: Option<SkillOutput>,
}

#[derive(Debug, Error)]
pub enum HarnessReplayError {
    #[error(transparent)]
    Fixture(#[from] HarnessFixtureError),
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error("harness fixture target {target} has no parent directory")]
    TargetWithoutParent { target: PathBuf },
    #[error("harness expectation mismatch at {field}: expected {expected}, actual {actual}")]
    Mismatch {
        field: &'static str,
        expected: String,
        actual: String,
    },
    #[error("harness receipt digest failed: {message}")]
    ReceiptDigest { message: String },
    #[error("harness receipt proof failed for {receipt_id}: {findings}")]
    ReceiptProofInvalid {
        receipt_id: String,
        findings: String,
    },
    #[error("harness fixture mode {mode} at {field_path} is not yet supported by the Rust harness")]
    UnsupportedFixtureMode { mode: String, field_path: String },
    #[error("invalid harness replay metadata at {field}: {message}")]
    InvalidReplayMetadata { field: String, message: String },
    #[error(
        "native cli-tool harness replay is unavailable because runx-runtime was built without the cli-tool feature"
    )]
    CliToolFeatureDisabled,
}

pub fn run_harness_fixture(
    fixture_path: impl AsRef<Path>,
) -> Result<HarnessReplayOutput, HarnessReplayError> {
    #[cfg(feature = "cli-tool")]
    {
        run_harness_fixture_with_adapter(
            fixture_path,
            crate::adapters::cli_tool::CliToolAdapter,
            RuntimeOptions::default(),
        )
    }
    #[cfg(not(feature = "cli-tool"))]
    {
        let _ = fixture_path;
        Err(HarnessReplayError::CliToolFeatureDisabled)
    }
}

#[cfg(feature = "cli-tool")]
pub fn run_harness_fixture_cli_tool(
    fixture_path: impl AsRef<Path>,
) -> Result<HarnessReplayOutput, HarnessReplayError> {
    run_harness_fixture_with_adapter(
        fixture_path,
        crate::adapters::cli_tool::CliToolAdapter,
        RuntimeOptions::default(),
    )
}

pub fn run_harness_fixture_with_adapter<A>(
    fixture_path: impl AsRef<Path>,
    adapter: A,
    options: RuntimeOptions,
) -> Result<HarnessReplayOutput, HarnessReplayError>
where
    A: SkillAdapter,
{
    let fixture_path = fixture_path.as_ref();
    let fixture = load_harness_fixture(fixture_path)?;
    let target_path = resolve_target_path(fixture_path, &fixture.target)?;
    let output = match fixture.kind {
        HarnessFixtureKind::Skill | HarnessFixtureKind::A2a | HarnessFixtureKind::Agent => {
            run_skill_fixture(&fixture, target_path, adapter, options)?
        }
        HarnessFixtureKind::AgentStep => run_agent_step_fixture(&fixture, options)?,
        HarnessFixtureKind::Graph if is_fixture_replay_graph(&fixture) => {
            run_graph_replay_fixture(&fixture, options)?
        }
        HarnessFixtureKind::Graph => run_graph_fixture(&fixture, &target_path, adapter, options)?,
        HarnessFixtureKind::Mcp => {
            return Err(HarnessReplayError::UnsupportedFixtureMode {
                mode: fixture_kind_name(&fixture.kind).to_owned(),
                field_path: "kind".to_owned(),
            });
        }
    };
    assert_expectations(&output)?;
    Ok(output)
}

fn run_agent_step_fixture(
    fixture: &HarnessFixture,
    options: RuntimeOptions,
) -> Result<HarnessReplayOutput, HarnessReplayError> {
    let replay_name = fixture.runner.as_deref().unwrap_or(&fixture.name);
    let request_id = format!("agent_step.{replay_name}.output");
    let output = agent_step_output(fixture, &request_id)?;
    let disposition = fixture
        .expect
        .status
        .as_ref()
        .map(disposition_from_expected_status)
        .unwrap_or_else(|| {
            if output.succeeded() {
                ClosureDisposition::Closed
            } else {
                ClosureDisposition::Failed
            }
        });
    let receipt = step_receipt_with_disposition(StepReceiptWithDisposition {
        graph_name: &fixture.name,
        step_id: &fixture.name,
        attempt: 1,
        output: &output,
        created_at: &options.created_at,
        disposition: disposition.clone(),
        reason_code: process_reason_code(&disposition),
        summary: format!("agent-step {} completed", fixture.name),
    })?;
    Ok(HarnessReplayOutput {
        fixture: fixture.clone(),
        status: status_from_disposition(&receipt.seal.disposition),
        receipt,
        step_receipts: Vec::new(),
        skill_output: Some(output),
    })
}

#[derive(Clone, Debug)]
struct GraphReplayStep {
    step_id: String,
    task: String,
    request_id: String,
}

fn is_fixture_replay_graph(fixture: &HarnessFixture) -> bool {
    string_metadata(fixture, "graph_shape") == Some("fixture_replay")
}

// rust-style-allow: long-function because graph replay receipt assembly keeps
// step runs, closure disposition, and parent receipt sealing in one invariant.
fn run_graph_replay_fixture(
    fixture: &HarnessFixture,
    options: RuntimeOptions,
) -> Result<HarnessReplayOutput, HarnessReplayError> {
    let mut runs = Vec::new();
    for replay_step in graph_replay_steps(fixture)? {
        let output = agent_step_output(fixture, &replay_step.request_id)?;
        let disposition = if output.succeeded() {
            ClosureDisposition::Closed
        } else {
            ClosureDisposition::Deferred
        };
        let receipt = step_receipt_with_disposition(StepReceiptWithDisposition {
            graph_name: &fixture.name,
            step_id: &replay_step.step_id,
            attempt: 1,
            output: &output,
            created_at: &options.created_at,
            disposition: disposition.clone(),
            reason_code: process_reason_code(&disposition),
            summary: if output.succeeded() {
                format!("agent-step {} replayed", replay_step.task)
            } else {
                output.stderr.clone()
            },
        })?;
        let outputs = skill_output_object(&output);
        let succeeded = output.succeeded();
        runs.push(StepRun {
            step_id: replay_step.step_id,
            attempt: 1,
            skill: replay_step.task.clone(),
            runner: Some(replay_step.task),
            fanout_group: None,
            output,
            outputs,
            receipt,
        });
        if !succeeded {
            break;
        }
    }
    if runs.is_empty() {
        return Err(HarnessReplayError::InvalidReplayMetadata {
            field: "metadata.graph_replay_steps".to_owned(),
            message: "at least one replay step is required".to_owned(),
        });
    }
    let disposition = fixture
        .expect
        .status
        .as_ref()
        .map(disposition_from_expected_status)
        .unwrap_or_else(|| {
            if runs.iter().all(|run| run.output.succeeded()) {
                ClosureDisposition::Closed
            } else {
                ClosureDisposition::Deferred
            }
        });
    let receipt = graph_receipt_with_disposition(
        &fixture.name,
        &mut runs,
        Vec::new(),
        &options.created_at,
        disposition.clone(),
        named_reason_code(&fixture.name, &disposition),
        format!("graph {} replayed through fixture harness", fixture.name),
    )?;
    let step_receipts = runs
        .iter()
        .map(|run| run.receipt.clone())
        .collect::<Vec<_>>();
    let skill_output = runs
        .iter()
        .rev()
        .find(|run| run.output.succeeded())
        .or_else(|| runs.last())
        .map(|run| run.output.clone());
    Ok(HarnessReplayOutput {
        fixture: fixture.clone(),
        status: status_from_disposition(&receipt.seal.disposition),
        receipt,
        step_receipts,
        skill_output,
    })
}

fn graph_replay_steps(
    fixture: &HarnessFixture,
) -> Result<Vec<GraphReplayStep>, HarnessReplayError> {
    let Some(JsonValue::Array(raw_steps)) = fixture.metadata.get("graph_replay_steps") else {
        return Err(HarnessReplayError::InvalidReplayMetadata {
            field: "metadata.graph_replay_steps".to_owned(),
            message: "array is required for fixture replay graphs".to_owned(),
        });
    };
    raw_steps
        .iter()
        .enumerate()
        .map(|(index, raw_step)| {
            let JsonValue::Object(step) = raw_step else {
                return Err(HarnessReplayError::InvalidReplayMetadata {
                    field: format!("metadata.graph_replay_steps.{index}"),
                    message: "object is required".to_owned(),
                });
            };
            let step_id = required_string_metadata(
                step,
                &format!("metadata.graph_replay_steps.{index}.step_id"),
                "step_id",
            )?;
            let task = required_string_metadata(
                step,
                &format!("metadata.graph_replay_steps.{index}.task"),
                "task",
            )?;
            Ok(GraphReplayStep {
                request_id: format!("agent_step.{task}.output"),
                step_id,
                task,
            })
        })
        .collect()
}

fn agent_step_output(
    fixture: &HarnessFixture,
    request_id: &str,
) -> Result<SkillOutput, HarnessReplayError> {
    let mut metadata = JsonObject::new();
    metadata.insert(
        "agent_request_id".to_owned(),
        JsonValue::String(request_id.to_owned()),
    );
    let payload = fixture
        .caller
        .get("answers")
        .and_then(json_object)
        .and_then(|answers| answers.get(request_id))
        .cloned()
        .unwrap_or(JsonValue::Null);
    if matches!(payload, JsonValue::Null) {
        return Ok(SkillOutput {
            status: crate::InvocationStatus::Failure,
            stdout: String::new(),
            stderr: format!("missing replay answer for {request_id}"),
            exit_code: None,
            duration_ms: 0,
            metadata,
        });
    }
    Ok(SkillOutput {
        status: crate::InvocationStatus::Success,
        stdout: serde_json::to_string(&payload).map_err(|source| RuntimeError::Json {
            context: format!("serializing replay answer {request_id}"),
            source,
        })?,
        stderr: String::new(),
        exit_code: Some(0),
        duration_ms: 0,
        metadata,
    })
}

fn skill_output_object(output: &SkillOutput) -> JsonObject {
    serde_json::from_str::<JsonValue>(&output.stdout)
        .ok()
        .and_then(|value| match value {
            JsonValue::Object(object) => Some(object),
            _ => None,
        })
        .unwrap_or_default()
}

fn string_metadata<'a>(fixture: &'a HarnessFixture, field: &str) -> Option<&'a str> {
    match fixture.metadata.get(field) {
        Some(JsonValue::String(value)) => Some(value),
        _ => None,
    }
}

fn required_string_metadata(
    object: &JsonObject,
    field_path: &str,
    field: &str,
) -> Result<String, HarnessReplayError> {
    match object.get(field) {
        Some(JsonValue::String(value)) if !value.is_empty() => Ok(value.clone()),
        Some(_) => Err(HarnessReplayError::InvalidReplayMetadata {
            field: field_path.to_owned(),
            message: "non-empty string is required".to_owned(),
        }),
        None => Err(HarnessReplayError::InvalidReplayMetadata {
            field: field_path.to_owned(),
            message: "field is required".to_owned(),
        }),
    }
}

fn json_object(value: &JsonValue) -> Option<&runx_contracts::JsonObject> {
    match value {
        JsonValue::Object(object) => Some(object),
        JsonValue::Null
        | JsonValue::Bool(_)
        | JsonValue::Number(_)
        | JsonValue::String(_)
        | JsonValue::Array(_) => None,
    }
}

fn json_string(value: &JsonValue) -> Option<&str> {
    match value {
        JsonValue::String(value) => Some(value),
        JsonValue::Null
        | JsonValue::Bool(_)
        | JsonValue::Number(_)
        | JsonValue::Object(_)
        | JsonValue::Array(_) => None,
    }
}

fn agent_answer_disposition(answer: &JsonValue) -> ClosureDisposition {
    match json_object(answer)
        .and_then(|object| object.get("closure"))
        .and_then(json_object)
        .and_then(|closure| closure.get("disposition"))
        .and_then(json_string)
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

fn disposition_from_expected_status(status: &HarnessExpectedStatus) -> ClosureDisposition {
    match status {
        HarnessExpectedStatus::Sealed => ClosureDisposition::Closed,
        HarnessExpectedStatus::Failure => ClosureDisposition::Failed,
        HarnessExpectedStatus::NeedsAgent => ClosureDisposition::Deferred,
        HarnessExpectedStatus::PolicyDenied => ClosureDisposition::Blocked,
        HarnessExpectedStatus::Escalated => ClosureDisposition::Deferred,
    }
}

fn process_reason_code(disposition: &ClosureDisposition) -> String {
    format!("process_{}", disposition_suffix(disposition))
}

fn named_reason_code(name: &str, disposition: &ClosureDisposition) -> String {
    format!("{name}_{}", disposition_suffix(disposition))
}

fn disposition_suffix(disposition: &ClosureDisposition) -> &'static str {
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

fn run_skill_fixture<A>(
    fixture: &HarnessFixture,
    skill_dir: PathBuf,
    adapter: A,
    options: RuntimeOptions,
) -> Result<HarnessReplayOutput, HarnessReplayError>
where
    A: SkillAdapter,
{
    let (skill_name, invocation) = skill_fixture_invocation(fixture, skill_dir, &options)?;
    let (skill_output, disposition, reason_code, summary) =
        run_skill_invocation(fixture, invocation, adapter)?;
    let receipt = step_receipt_with_disposition(StepReceiptWithDisposition {
        graph_name: &fixture.name,
        step_id: &skill_name,
        attempt: 1,
        output: &skill_output,
        created_at: &options.created_at,
        disposition: disposition.clone(),
        reason_code,
        summary,
    })?;
    Ok(HarnessReplayOutput {
        fixture: fixture.clone(),
        status: status_from_disposition(&receipt.seal.disposition),
        receipt,
        step_receipts: Vec::new(),
        skill_output: Some(skill_output),
    })
}

fn skill_fixture_invocation(
    fixture: &HarnessFixture,
    skill_dir: PathBuf,
    options: &RuntimeOptions,
) -> Result<(String, SkillInvocation), HarnessReplayError> {
    let skill = load_skill(&skill_dir)?;
    let runner = load_harness_runner(&skill_dir, fixture.runner.as_deref())?;
    let mut env = options.env.clone();
    env.extend(fixture.env.clone());
    let skill_name = if fixture.runner.is_some() {
        runner
            .as_ref()
            .map_or_else(|| skill.name.clone(), |runner| runner.name.clone())
    } else {
        skill.name.clone()
    };
    let source = runner
        .as_ref()
        .map_or_else(|| skill.source.clone(), |runner| runner.source.clone());
    let invocation = SkillInvocation {
        skill_name: skill_name.clone(),
        source,
        inputs: fixture.inputs.clone(),
        resolved_inputs: JsonObject::new(),
        skill_directory: skill_dir,
        env,
        credential_delivery: crate::credentials::CredentialDelivery::none(),
    };
    Ok((skill_name, invocation))
}

fn run_skill_invocation<A>(
    fixture: &HarnessFixture,
    invocation: SkillInvocation,
    adapter: A,
) -> Result<(SkillOutput, ClosureDisposition, String, String), HarnessReplayError>
where
    A: SkillAdapter,
{
    let skill_name = invocation.skill_name.clone();
    let (skill_output, disposition, reason_code, summary) =
        match invocation.source.source_type.as_str() {
            "agent" | "agent-step" => replay_agent_skill_fixture(fixture, &invocation)?,
            _ => {
                let output = adapter.invoke(invocation)?;
                let disposition = if output.succeeded() {
                    ClosureDisposition::Closed
                } else {
                    ClosureDisposition::Failed
                };
                let reason_code = process_reason_code(&disposition);
                let summary = format!("step {skill_name} completed");
                (output, disposition, reason_code, summary)
            }
        };
    Ok((skill_output, disposition, reason_code, summary))
}

fn load_harness_runner(
    skill_dir: &Path,
    requested_runner: Option<&str>,
) -> Result<Option<SkillRunnerDefinition>, HarnessReplayError> {
    let manifest_path = skill_dir.join("X.yaml");
    if !manifest_path.exists() {
        if let Some(runner) = requested_runner {
            return Err(RuntimeError::UnsupportedRunnerSelection {
                runner: runner.to_owned(),
            }
            .into());
        }
        return Ok(None);
    }
    let source = fs::read_to_string(&manifest_path).map_err(|source| {
        RuntimeError::io(format!("reading {}", manifest_path.display()), source)
    })?;
    let parsed = parse_runner_manifest_yaml(&source).map_err(RuntimeError::from)?;
    let manifest = validate_runner_manifest(parsed).map_err(RuntimeError::from)?;
    select_harness_runner(&manifest, requested_runner)
        .cloned()
        .map(Some)
}

fn select_harness_runner<'a>(
    manifest: &'a SkillRunnerManifest,
    requested_runner: Option<&str>,
) -> Result<&'a SkillRunnerDefinition, HarnessReplayError> {
    if let Some(runner) = requested_runner {
        return manifest.runners.get(runner).ok_or_else(|| {
            RuntimeError::UnsupportedRunnerSelection {
                runner: runner.to_owned(),
            }
            .into()
        });
    }
    let defaults = manifest
        .runners
        .values()
        .filter(|runner| runner.default)
        .collect::<Vec<_>>();
    match defaults.as_slice() {
        [runner] => Ok(*runner),
        [] if manifest.runners.len() == 1 => manifest.runners.values().next().ok_or_else(|| {
            RuntimeError::UnsupportedRunnerSelection {
                runner: "default".to_owned(),
            }
            .into()
        }),
        [] => Err(RuntimeError::UnsupportedRunnerSelection {
            runner: "default".to_owned(),
        }
        .into()),
        _ => Err(RuntimeError::UnsupportedRunnerSelection {
            runner: "default".to_owned(),
        }
        .into()),
    }
}

fn replay_agent_skill_fixture(
    fixture: &HarnessFixture,
    invocation: &SkillInvocation,
) -> Result<(SkillOutput, ClosureDisposition, String, String), HarnessReplayError> {
    let source_type =
        AgentActInvocationSourceType::from_contract_value(&invocation.source.source_type)
            .ok_or_else(|| RuntimeError::UnsupportedAdapter {
                adapter_type: invocation.source.source_type.clone(),
            })?;
    let request_id = agent_act_invocation_id(invocation, source_type);
    let mut metadata = JsonObject::new();
    metadata.insert(
        "agent_request_id".to_owned(),
        JsonValue::String(request_id.clone()),
    );
    let Some(answer) = fixture_answer(fixture, "answers", &request_id, &request_id) else {
        return Ok((
            SkillOutput {
                status: InvocationStatus::Failure,
                stdout: String::new(),
                stderr: format!("missing replay answer for {request_id}"),
                exit_code: None,
                duration_ms: 0,
                metadata,
            },
            ClosureDisposition::Deferred,
            "agent_act_deferred".to_owned(),
            format!("agent act {request_id} is awaiting replay answer"),
        ));
    };
    let stdout = serde_json::to_string(answer).map_err(|source| RuntimeError::Json {
        context: format!("serializing replay answer {request_id}"),
        source,
    })?;
    let disposition = agent_answer_disposition(answer);
    let succeeded = disposition == ClosureDisposition::Closed;
    Ok((
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
                format!("agent act closed with {}", disposition_suffix(&disposition))
            },
            exit_code: succeeded.then_some(0),
            duration_ms: 0,
            metadata,
        },
        disposition.clone(),
        format!("agent_act_{}", disposition_suffix(&disposition)),
        format!("agent act closed with {}", disposition_suffix(&disposition)),
    ))
}

fn run_graph_fixture<A>(
    fixture: &HarnessFixture,
    graph_path: &Path,
    adapter: A,
    mut options: RuntimeOptions,
) -> Result<HarnessReplayOutput, HarnessReplayError>
where
    A: SkillAdapter,
{
    options.env.extend(fixture.env.clone());
    let runtime = Runtime::new(adapter, options);
    let mut host = FixtureHost::new(fixture);
    let graph_run = runtime.run_graph_file_for_harness(graph_path, &mut host)?;
    persist_payment_ledger_projection_if_configured(fixture, &graph_run, &runtime)?;
    let output = replay_output_from_graph(fixture, graph_run);
    Ok(output)
}

fn persist_payment_ledger_projection_if_configured<A>(
    fixture: &HarnessFixture,
    graph_run: &GraphRun,
    runtime: &Runtime<A>,
) -> Result<(), HarnessReplayError>
where
    A: SkillAdapter,
{
    if !runtime.options().env.contains_key(RUNX_RECEIPT_DIR_ENV) {
        return Ok(());
    }
    let cwd = std::env::current_dir().map_err(|source| {
        RuntimeError::io("resolving cwd for payment ledger projection", source)
    })?;
    let receipt_path = resolve_receipt_path(ReceiptPathInputs {
        explicit_dir: None,
        runtime_config: None,
        env: &runtime.options().env,
        cwd: &cwd,
    });
    let scenario_id = x402_payment_scenario_id(fixture, graph_run);
    persist_x402_payment_ledger_projection_event(
        &receipt_path.path,
        &format!("gx_{}", graph_run.graph.name),
        &runtime.options().created_at,
        &graph_run.receipt,
        &graph_run.steps,
        &scenario_id,
    )
    .map(|_| ())
    .map_err(|source| {
        RuntimeError::ReceiptInvalid {
            message: source.to_string(),
        }
        .into()
    })
}

fn x402_payment_scenario_id(fixture: &HarnessFixture, graph_run: &GraphRun) -> String {
    string_metadata(fixture, "payment_ledger_scenario_id")
        .or_else(|| scenario_id_from_graph_name(&graph_run.graph.name))
        .unwrap_or("P1")
        .to_owned()
}

fn scenario_id_from_graph_name(graph_name: &str) -> Option<&'static str> {
    if graph_name.contains("paid-echo") {
        Some("P1.5")
    } else if graph_name.contains("cap-exceeded") {
        Some("P1.3")
    } else if graph_name.contains("ambiguous-bounds") {
        Some("P1.4")
    } else if graph_name.contains("proofless-rail") {
        Some("P1.12")
    } else {
        None
    }
}

struct FixtureHost<'a> {
    fixture: &'a HarnessFixture,
}

impl<'a> FixtureHost<'a> {
    fn new(fixture: &'a HarnessFixture) -> Self {
        Self { fixture }
    }
}

impl Host for FixtureHost<'_> {
    fn report(&mut self, _event: ExecutionEvent) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn resolve(
        &mut self,
        request: ResolutionRequest,
    ) -> Result<Option<ResolutionResponse>, RuntimeError> {
        match request {
            ResolutionRequest::Approval { id, gate } => {
                fixture_approval_response(self.fixture, &id, &gate.id)
            }
            ResolutionRequest::Input { .. } | ResolutionRequest::AgentAct { .. } => Ok(None),
        }
    }
}

fn fixture_approval_response(
    fixture: &HarnessFixture,
    request_id: &str,
    gate_id: &str,
) -> Result<Option<ResolutionResponse>, RuntimeError> {
    let Some(answer) = fixture_answer(fixture, "approvals", gate_id, request_id)
        .or_else(|| fixture_answer(fixture, "answers", request_id, gate_id))
    else {
        return Ok(None);
    };
    let approved = fixture_bool_answer(answer, request_id, gate_id)?;
    Ok(Some(ResolutionResponse {
        actor: fixture_answer_actor(answer, request_id, gate_id)?,
        payload: JsonValue::Bool(approved),
    }))
}

fn fixture_answer<'a>(
    fixture: &'a HarnessFixture,
    group: &str,
    primary_key: &str,
    secondary_key: &str,
) -> Option<&'a JsonValue> {
    fixture
        .caller
        .get(group)
        .and_then(json_object)
        .and_then(|answers| {
            answers
                .get(primary_key)
                .or_else(|| answers.get(secondary_key))
        })
}

fn fixture_bool_answer(
    answer: &JsonValue,
    request_id: &str,
    gate_id: &str,
) -> Result<bool, RuntimeError> {
    match answer {
        JsonValue::Bool(value) => Ok(*value),
        JsonValue::Object(object) => match object.get("approved").or_else(|| object.get("payload"))
        {
            Some(JsonValue::Bool(value)) => Ok(*value),
            Some(_) | None => Err(invalid_fixture_answer(request_id, gate_id)),
        },
        JsonValue::Null | JsonValue::Number(_) | JsonValue::String(_) | JsonValue::Array(_) => {
            Err(invalid_fixture_answer(request_id, gate_id))
        }
    }
}

fn fixture_answer_actor(
    answer: &JsonValue,
    request_id: &str,
    gate_id: &str,
) -> Result<ResolutionResponseActor, RuntimeError> {
    let Some(actor) = json_object(answer).and_then(|object| object.get("actor")) else {
        return Ok(ResolutionResponseActor::Human);
    };
    match actor {
        JsonValue::String(value) if value == "human" => Ok(ResolutionResponseActor::Human),
        JsonValue::String(value) if value == "agent" => Ok(ResolutionResponseActor::Agent),
        _ => Err(RuntimeError::ReceiptInvalid {
            message: format!(
                "harness fixture approval answer for request {request_id} gate {gate_id} has invalid actor"
            ),
        }),
    }
}

fn invalid_fixture_answer(request_id: &str, gate_id: &str) -> RuntimeError {
    RuntimeError::ReceiptInvalid {
        message: format!(
            "harness fixture approval answer for request {request_id} gate {gate_id} must be a boolean or object with a boolean approved field"
        ),
    }
}

fn replay_output_from_graph(fixture: &HarnessFixture, graph_run: GraphRun) -> HarnessReplayOutput {
    let step_receipts = graph_run
        .steps
        .iter()
        .map(|step| step.receipt.clone())
        .collect::<Vec<_>>();
    HarnessReplayOutput {
        fixture: fixture.clone(),
        status: status_from_disposition(&graph_run.receipt.seal.disposition),
        receipt: graph_run.receipt,
        step_receipts,
        skill_output: None,
    }
}

fn resolve_target_path(fixture_path: &Path, target: &str) -> Result<PathBuf, HarnessReplayError> {
    let Some(parent) = fixture_path.parent() else {
        return Err(HarnessReplayError::TargetWithoutParent {
            target: fixture_path.to_path_buf(),
        });
    };
    Ok(parent.join(target))
}
