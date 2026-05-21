// rust-style-allow: large-file - native skill execution keeps request parsing,
// continuation hydration, and sealed receipt assembly together for parity review.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use runx_contracts::{ClosureDisposition, JsonNumber, JsonObject, JsonValue};
use runx_parser::{
    SkillRunnerDefinition, SkillRunnerManifest, parse_runner_manifest_yaml,
    validate_runner_manifest,
};
use thiserror::Error;

use crate::RuntimeError;
use crate::adapter::{InvocationStatus, SkillInvocation, SkillOutput};
use crate::agent_invocation::{
    AgentActInvocationSourceType, agent_act_invocation_id, agent_act_resolution_request,
};
use crate::execution::orchestrator::SkillRunRequest;
use crate::receipts::paths::{ReceiptPathInputs, resolve_receipt_path};
use crate::receipts::store::{LocalReceiptStore, ReceiptStoreError};
use crate::receipts::{StepReceiptWithDisposition, step_receipt_with_disposition};

const SKILL_RUN_SCHEMA: &str = "runx.skill_run.v1";
const DEFAULT_CREATED_AT: &str = "2026-05-18T00:00:00Z";

#[derive(Debug, Error)]
pub enum SkillRunError {
    #[error("skill run failed: {0}")]
    Invalid(String),
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error(transparent)]
    ReceiptStore(#[from] ReceiptStoreError),
}

pub(crate) fn execute_skill_run(request: &SkillRunRequest) -> Result<JsonValue, SkillRunError> {
    let skill_dir = resolve_skill_dir(&request.skill_path)?;
    let manifest = load_runner_manifest(&skill_dir)?;
    let runner = selected_runner(&manifest)?;
    let invocation = runner_invocation(&skill_dir, runner, &request.inputs, &request.env)?;
    let source_type = agent_invocation_source_type(&runner.source.source_type)?;
    let request_id = agent_act_invocation_id(&invocation, source_type);
    let run_id = match (&request.run_id, &request.answers_path) {
        (Some(run_id), Some(_)) => run_id.clone(),
        (Some(_), None) => return Err(invalid("runx skill --run-id requires --answers")),
        (None, Some(_)) => return Err(invalid("runx skill --answers requires --run-id")),
        (None, None) => format!("run_{}", identifier_segment(&request_id)),
    };
    let resolution_request = agent_request(&invocation, source_type)?;

    let Some(answers_path) = &request.answers_path else {
        return Ok(JsonValue::Object(needs_agent_output(
            &run_id,
            &request_id,
            resolution_request,
        )));
    };

    let answer = read_answer(answers_path, &request_id)?;
    let stdout = serde_json::to_string(&answer)
        .map_err(|error| SkillRunError::Invalid(format!("failed to serialize answer: {error}")))?;
    let disposition = answer_disposition(&answer);
    let receipt = seal_skill_answer(&run_id, runner, &stdout, disposition)?;
    let receipt_path = resolve_receipt_path(ReceiptPathInputs {
        explicit_dir: request.receipt_dir.as_deref(),
        runtime_config: None,
        env: &request.env,
        cwd: &request.cwd,
    });
    LocalReceiptStore::new(&receipt_path.path).write_receipt(&receipt)?;

    Ok(JsonValue::Object(sealed_output(
        &manifest,
        &run_id,
        &stdout,
        &answer,
        &receipt,
        contract_json_value(&receipt)?,
    )))
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

fn selected_runner(
    manifest: &SkillRunnerManifest,
) -> Result<&SkillRunnerDefinition, SkillRunError> {
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
) -> Result<SkillInvocation, SkillRunError> {
    if !matches!(runner.source.source_type.as_str(), "agent" | "agent-step") {
        return Err(invalid(format!(
            "runx skill native execution only supports agent and agent-step runners, got {}",
            runner.source.source_type
        )));
    }
    Ok(SkillInvocation {
        skill_name: runner.name.clone(),
        source: runner.source.clone(),
        inputs: inputs.clone().into_iter().collect(),
        resolved_inputs: JsonObject::new(),
        skill_directory: skill_dir.to_path_buf(),
        env: env.clone(),
    })
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
    contract_json_value(&agent_act_resolution_request(invocation, source_type))
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
) -> Result<runx_contracts::HarnessReceipt, SkillRunError> {
    let graph_name = identifier_segment(run_id);
    let step_id = identifier_segment(&runner.name);
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
    Ok(step_receipt_with_disposition(StepReceiptWithDisposition {
        graph_name: &graph_name,
        step_id: &step_id,
        attempt: 1,
        output: &skill_output,
        created_at: DEFAULT_CREATED_AT,
        disposition,
        reason_code: format!("agent_act_{disposition_label}"),
        summary: format!("agent act closed with {disposition_label}"),
    })?)
}

fn answer_disposition(answer: &JsonValue) -> ClosureDisposition {
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

fn json_object(value: &JsonValue) -> Option<&JsonObject> {
    match value {
        JsonValue::Object(object) => Some(object),
        _ => None,
    }
}

fn json_string(value: &JsonValue) -> Option<&str> {
    match value {
        JsonValue::String(value) => Some(value),
        _ => None,
    }
}

fn sealed_output(
    manifest: &SkillRunnerManifest,
    run_id: &str,
    stdout: &str,
    answer: &JsonValue,
    receipt: &runx_contracts::HarnessReceipt,
    receipt_value: JsonValue,
) -> JsonObject {
    let mut execution = JsonObject::new();
    execution.insert("stdout".to_owned(), JsonValue::String(stdout.to_owned()));
    execution.insert("stderr".to_owned(), JsonValue::String(String::new()));
    execution.insert(
        "exit_code".to_owned(),
        if receipt.seal.disposition == ClosureDisposition::Closed {
            JsonValue::Number(JsonNumber::I64(0))
        } else {
            JsonValue::Null
        },
    );
    execution.insert("structured_output".to_owned(), answer.clone());

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
        JsonValue::String(receipt.id.clone()),
    );
    output.insert(
        "closure".to_owned(),
        JsonValue::Object(closure_output(&receipt.seal)),
    );
    output.insert("receipt".to_owned(), receipt_value);
    output.insert("execution".to_owned(), JsonValue::Object(execution));
    output.insert("payload".to_owned(), answer.clone());
    output
}

fn closure_output(seal: &runx_contracts::HarnessSeal) -> JsonObject {
    let mut closure = JsonObject::new();
    closure.insert(
        "disposition".to_owned(),
        JsonValue::String(closure_disposition_label(&seal.disposition).to_owned()),
    );
    closure.insert(
        "reason_code".to_owned(),
        JsonValue::String(seal.reason_code.clone()),
    );
    closure.insert(
        "summary".to_owned(),
        JsonValue::String(seal.summary.clone()),
    );
    closure.insert(
        "closed_at".to_owned(),
        JsonValue::String(seal.closed_at.clone()),
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
