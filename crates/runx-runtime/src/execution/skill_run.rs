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
#[cfg(feature = "cli-tool")]
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::RuntimeError;
#[cfg(feature = "cli-tool")]
use crate::adapter::SkillAdapter;
use crate::adapter::{InvocationStatus, SkillInvocation, SkillOutput};
#[cfg(feature = "cli-tool")]
use crate::adapters::cli_tool::CliToolAdapter;
use crate::agent_invocation::{
    AgentActInvocationSourceType, agent_act_invocation_id, agent_act_resolution_request,
};
use crate::execution::orchestrator::SkillRunRequest;
use crate::receipts::paths::{ReceiptPathInputs, resolve_receipt_path};
use crate::receipts::store::{LocalReceiptStore, ReceiptStoreError};
use crate::receipts::{StepReceiptWithDisposition, step_receipt_with_disposition};
use crate::time::DEFAULT_CREATED_AT;

const SKILL_RUN_SCHEMA: &str = "runx.skill_run.v1";

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
    let invocation = runner_invocation(
        &skill_dir,
        runner,
        &request.inputs,
        &request.env,
        request.local_credential.as_ref(),
    )?;
    if runner.source.source_type == "cli-tool" {
        return execute_cli_tool_skill_run(request, &manifest, runner, invocation);
    }

    execute_agent_skill_run(request, &manifest, runner, invocation)
}

fn execute_agent_skill_run(
    request: &SkillRunRequest,
    manifest: &SkillRunnerManifest,
    runner: &SkillRunnerDefinition,
    invocation: SkillInvocation,
) -> Result<JsonValue, SkillRunError> {
    let source_type = agent_invocation_source_type(&runner.source.source_type)?;
    let request_id = agent_act_invocation_id(&invocation, source_type);
    let run_id = agent_run_id(request, &request_id)?;
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
    write_skill_receipt(request, &receipt)?;

    Ok(JsonValue::Object(sealed_output(
        manifest,
        &run_id,
        &agent_skill_output(stdout, &receipt),
        &answer,
        &receipt,
        contract_json_value(&receipt)?,
    )))
}

fn agent_run_id(request: &SkillRunRequest, request_id: &str) -> Result<String, SkillRunError> {
    match (&request.run_id, &request.answers_path) {
        (Some(run_id), Some(_)) => Ok(run_id.clone()),
        (Some(_), None) => Err(invalid("runx skill --run-id requires --answers")),
        (None, Some(_)) => Err(invalid("runx skill --answers requires --run-id")),
        (None, None) => Ok(format!("run_{}", identifier_segment(request_id))),
    }
}

fn agent_skill_output(stdout: String, receipt: &runx_contracts::HarnessReceipt) -> SkillOutput {
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
    local_credential: Option<&crate::execution::orchestrator::LocalCredentialDescriptor>,
) -> Result<SkillInvocation, SkillRunError> {
    if !matches!(
        runner.source.source_type.as_str(),
        "agent" | "agent-step" | "cli-tool"
    ) {
        return Err(invalid(format!(
            "runx skill native execution only supports agent, agent-step, and cli-tool runners, got {}",
            runner.source.source_type
        )));
    }
    let credential_delivery = match local_credential {
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
    };
    Ok(SkillInvocation {
        skill_name: runner.name.clone(),
        source: runner.source.clone(),
        inputs: inputs.clone().into_iter().collect(),
        resolved_inputs: JsonObject::new(),
        skill_directory: skill_dir.to_path_buf(),
        env: env.clone(),
        credential_delivery,
    })
}

#[cfg(feature = "cli-tool")]
fn execute_cli_tool_skill_run(
    request: &SkillRunRequest,
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
    let output = CliToolAdapter.invoke(invocation)?;
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
    )?;
    write_skill_receipt(request, &receipt)?;
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
    receipt: &runx_contracts::HarnessReceipt,
) -> Result<(), SkillRunError> {
    let receipt_path = resolve_receipt_path(ReceiptPathInputs {
        explicit_dir: request.receipt_dir.as_deref(),
        runtime_config: None,
        env: &request.env,
        cwd: &request.cwd,
    });
    LocalReceiptStore::new(&receipt_path.path)
        .write_receipt(receipt)
        .map_err(Into::into)
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
    )
}

fn seal_skill_output(
    run_id: &str,
    runner: &SkillRunnerDefinition,
    output: &SkillOutput,
    disposition: ClosureDisposition,
    reason_code: String,
    summary: String,
) -> Result<runx_contracts::HarnessReceipt, SkillRunError> {
    let graph_name = identifier_segment(run_id);
    let step_id = identifier_segment(&runner.name);
    Ok(step_receipt_with_disposition(StepReceiptWithDisposition {
        graph_name: &graph_name,
        step_id: &step_id,
        attempt: 1,
        output,
        created_at: DEFAULT_CREATED_AT,
        disposition,
        reason_code,
        summary,
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
    skill_output: &SkillOutput,
    payload: &JsonValue,
    receipt: &runx_contracts::HarnessReceipt,
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
    output.insert("payload".to_owned(), payload.clone());
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
