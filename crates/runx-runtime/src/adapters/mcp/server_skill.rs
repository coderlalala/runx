// rust-style-allow: large-file because skill execution, graph fallback,
// runx envelope construction, and caller plumbing for `runx mcp serve` stay
// adjacent to the McpServerState that orchestrates them.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use runx_contracts::{
    ExecutionEvent, JsonObject, JsonValue, Question, ResolutionRequest, ResolutionResponse,
};
use runx_core::state_machine::GraphStatus;
use runx_parser::{SkillInput, ValidatedSkill};

use crate::adapter::{SkillAdapter, SkillInvocation, SkillOutput};
use crate::caller::Caller;
use crate::receipts::step_receipt;
use crate::receipts::store::LocalReceiptStore;
use crate::{GraphRun, Runtime, RuntimeError, RuntimeOptions};

use super::adapter::McpAdapter;
use super::server::{McpServerState, mcp_tool_result_from_host_result};
use super::types::{
    McpHostRunResult, McpServerExecutionOptions, McpServerOptions, McpServerSkillExecution,
    McpServerTool, McpServerToolBehavior, McpToolResult,
};

impl McpServerOptions {
    pub fn from_skill_paths(
        skill_paths: &[PathBuf],
        package_name: impl Into<String>,
        package_version: impl Into<String>,
    ) -> Result<Self, RuntimeError> {
        Self::from_skill_paths_with_execution(
            skill_paths,
            package_name,
            package_version,
            McpServerExecutionOptions::default(),
        )
    }

    pub fn from_skill_paths_with_execution(
        skill_paths: &[PathBuf],
        package_name: impl Into<String>,
        package_version: impl Into<String>,
        execution: McpServerExecutionOptions,
    ) -> Result<Self, RuntimeError> {
        if let Some(runner) = &execution.runner {
            return Err(RuntimeError::UnsupportedRunnerSelection {
                runner: runner.clone(),
            });
        }
        let tools = skill_paths
            .iter()
            .map(|path| load_mcp_server_tool(path, &execution))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            package_name: package_name.into(),
            package_version: package_version.into(),
            tools,
        })
    }
}

pub(super) fn load_mcp_server_tool(
    skill_path: &Path,
    execution: &McpServerExecutionOptions,
) -> Result<McpServerTool, RuntimeError> {
    let skill = load_skill_for_mcp(skill_path)?;
    Ok(McpServerTool {
        name: skill.name.clone(),
        description: skill
            .description
            .clone()
            .unwrap_or_else(|| format!("runx skill {}", skill.name)),
        input_schema: skill_inputs_to_json_schema(&skill.inputs),
        result: McpServerToolBehavior::Skill(Box::new(McpServerSkillExecution {
            skill_path: skill_path.to_path_buf(),
            skill,
            receipt_dir: execution.receipt_dir.clone(),
            env: execution.env.clone(),
        })),
    })
}

fn load_skill_for_mcp(skill_path: &Path) -> Result<ValidatedSkill, RuntimeError> {
    let manifest_path = if skill_path.is_dir() {
        skill_path.join("SKILL.md")
    } else {
        skill_path.to_path_buf()
    };
    if !manifest_path.exists() {
        return Err(RuntimeError::SkillFileMissing {
            path: manifest_path,
        });
    }
    let source = fs::read_to_string(&manifest_path)
        .map_err(|source| RuntimeError::io("reading skill markdown", source))?;
    let raw = runx_parser::parse_skill_markdown(&source)?;
    runx_parser::validate_skill(raw).map_err(RuntimeError::from)
}

fn skill_inputs_to_json_schema(inputs: &BTreeMap<String, SkillInput>) -> JsonObject {
    let properties = inputs
        .iter()
        .map(|(name, input)| (name.clone(), JsonValue::Object(skill_input_schema(input))))
        .collect::<JsonObject>();
    let required = inputs
        .iter()
        .filter(|(_name, input)| input.required)
        .map(|(name, _input)| JsonValue::String(name.clone()))
        .collect::<Vec<_>>();
    [
        ("type".to_owned(), JsonValue::String("object".to_owned())),
        ("properties".to_owned(), JsonValue::Object(properties)),
        ("required".to_owned(), JsonValue::Array(required)),
        ("additionalProperties".to_owned(), JsonValue::Bool(false)),
    ]
    .into()
}

fn skill_input_schema(input: &SkillInput) -> JsonObject {
    let mut schema = JsonObject::new();
    if let Some(input_type) = normalize_input_type(&input.input_type) {
        schema.insert("type".to_owned(), JsonValue::String(input_type.to_owned()));
    }
    if let Some(description) = &input.description {
        schema.insert(
            "description".to_owned(),
            JsonValue::String(description.clone()),
        );
    }
    if let Some(default) = &input.default {
        schema.insert("default".to_owned(), default.clone());
    }
    schema
}

fn normalize_input_type(input_type: &str) -> Option<&str> {
    match input_type {
        "string" | "number" | "integer" | "boolean" | "object" | "array" => Some(input_type),
        _ => None,
    }
}

pub(super) fn execute_mcp_server_skill(
    state: &mut McpServerState,
    execution: McpServerSkillExecution,
    inputs: JsonObject,
) -> Result<McpToolResult, RuntimeError> {
    let inputs = apply_input_defaults(&execution.skill, inputs);
    if let Some(request) = input_resolution_request(&execution.skill, &inputs) {
        let skill_name = execution.skill.name.clone();
        let run_id = state.next_run_id(&execution.skill.name);
        return Ok(mcp_tool_result_from_host_result(
            McpHostRunResult::NeedsAgent {
                skill_name: skill_name.clone(),
                run_id: run_id.clone(),
                request_count: 1,
                runx: needs_agent_runx(&skill_name, &run_id, &[request])?,
            },
        ));
    }

    let run_id = state.next_run_id(&execution.skill.name);
    if execution.skill.source.source_type == "graph" {
        return execute_mcp_server_graph(state, &run_id, execution, inputs);
    }
    complete_mcp_server_skill(&run_id, execution, inputs)
}

fn execute_mcp_server_graph(
    _state: &mut McpServerState,
    run_id: &str,
    execution: McpServerSkillExecution,
    _inputs: JsonObject,
) -> Result<McpToolResult, RuntimeError> {
    let graph =
        execution
            .skill
            .source
            .graph
            .clone()
            .ok_or_else(|| RuntimeError::UnsupportedAdapter {
                adapter_type: "graph".to_owned(),
            })?;
    let graph_dir = skill_directory_for_execution(&execution.skill_path);
    let runtime = Runtime::new(
        McpServerGraphAdapter,
        RuntimeOptions {
            created_at: "2026-05-20T00:00:00Z".to_owned(),
            env: execution.env.clone(),
        },
    );
    let mut caller = McpServerCaller::default();
    let checkpoint =
        runtime.run_graph_until_steps_with_caller(&graph_dir, &graph, 1, &mut caller)?;
    if let Some(request) = caller.requests.first().cloned() {
        return Ok(mcp_tool_result_from_host_result(
            McpHostRunResult::NeedsAgent {
                skill_name: execution.skill.name.clone(),
                run_id: run_id.to_owned(),
                request_count: 1,
                runx: needs_agent_runx(&execution.skill.name, run_id, &[request])?,
            },
        ));
    }
    let run = runtime.resume_graph_with_caller(&graph_dir, graph, checkpoint, &mut caller)?;
    graph_run_mcp_result(&execution.skill.name, run_id, run)
}

fn graph_run_mcp_result(
    skill_name: &str,
    run_id: &str,
    run: GraphRun,
) -> Result<McpToolResult, RuntimeError> {
    let status = if run.state.status == GraphStatus::Succeeded {
        "completed"
    } else {
        "failed"
    };
    let result = if status == "completed" {
        McpHostRunResult::Completed {
            skill_name: skill_name.to_owned(),
            output: String::new(),
            receipt_id: run.receipt.id.clone(),
            runx: terminal_runx("completed", skill_name, run_id, &run.receipt.id),
        }
    } else {
        McpHostRunResult::Failed {
            skill_name: skill_name.to_owned(),
            receipt_id: Some(run.receipt.id.clone()),
            error: format!("graph ended with status {:?}", run.state.status),
            runx: terminal_runx("failed", skill_name, run_id, &run.receipt.id),
        }
    };
    Ok(mcp_tool_result_from_host_result(result))
}

fn complete_mcp_server_skill(
    run_id: &str,
    execution: McpServerSkillExecution,
    inputs: JsonObject,
) -> Result<McpToolResult, RuntimeError> {
    let output = invoke_mcp_server_skill(&execution, inputs)?;
    let receipt = step_receipt(
        run_id,
        &execution.skill.name,
        1,
        &output,
        "2026-05-20T00:00:00Z",
    )?;
    if let Some(receipt_dir) = &execution.receipt_dir {
        LocalReceiptStore::new(receipt_dir)
            .write_receipt(&receipt)
            .map_err(|source| RuntimeError::ReceiptInvalid {
                message: source.to_string(),
            })?;
    }
    let result = if output.succeeded() {
        McpHostRunResult::Completed {
            skill_name: execution.skill.name.clone(),
            output: output.stdout.clone(),
            receipt_id: receipt.id.clone(),
            runx: completed_runx(&execution.skill.name, run_id, &receipt.id, &output),
        }
    } else {
        McpHostRunResult::Failed {
            skill_name: execution.skill.name.clone(),
            receipt_id: Some(receipt.id.clone()),
            error: if output.stderr.is_empty() {
                "skill execution failed".to_owned()
            } else {
                output.stderr.clone()
            },
            runx: terminal_runx("failed", &execution.skill.name, run_id, &receipt.id),
        }
    };
    Ok(mcp_tool_result_from_host_result(result))
}

fn invoke_mcp_server_skill(
    execution: &McpServerSkillExecution,
    inputs: JsonObject,
) -> Result<SkillOutput, RuntimeError> {
    let invocation = SkillInvocation {
        skill_name: execution.skill.name.clone(),
        source: execution.skill.source.clone(),
        inputs,
        resolved_inputs: JsonObject::new(),
        skill_directory: skill_directory_for_execution(&execution.skill_path),
        env: execution.env.clone(),
    };
    match execution.skill.source.source_type.as_str() {
        "mcp" => McpAdapter::default().invoke(invocation),
        "cli-tool" => invoke_cli_tool_server_skill(invocation),
        "graph" => Err(RuntimeError::UnsupportedAdapter {
            adapter_type: "graph".to_owned(),
        }),
        other => Err(RuntimeError::UnsupportedAdapter {
            adapter_type: other.to_owned(),
        }),
    }
}

#[cfg(feature = "cli-tool")]
fn invoke_cli_tool_server_skill(invocation: SkillInvocation) -> Result<SkillOutput, RuntimeError> {
    crate::adapters::cli_tool::CliToolAdapter.invoke(invocation)
}

#[cfg(not(feature = "cli-tool"))]
fn invoke_cli_tool_server_skill(invocation: SkillInvocation) -> Result<SkillOutput, RuntimeError> {
    Err(RuntimeError::UnsupportedAdapter {
        adapter_type: invocation.source.source_type,
    })
}

#[derive(Clone, Copy, Debug, Default)]
struct McpServerGraphAdapter;

impl SkillAdapter for McpServerGraphAdapter {
    fn adapter_type(&self) -> &'static str {
        "mcp-server-graph"
    }

    fn invoke(&self, request: SkillInvocation) -> Result<SkillOutput, RuntimeError> {
        match request.source.source_type.as_str() {
            "mcp" => McpAdapter::default().invoke(request),
            "cli-tool" => invoke_cli_tool_server_skill(request),
            other => Err(RuntimeError::UnsupportedAdapter {
                adapter_type: other.to_owned(),
            }),
        }
    }
}

#[derive(Default)]
struct McpServerCaller {
    requests: Vec<ResolutionRequest>,
}

impl Caller for McpServerCaller {
    fn report(&mut self, _event: ExecutionEvent) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn resolve(
        &mut self,
        request: ResolutionRequest,
    ) -> Result<Option<ResolutionResponse>, RuntimeError> {
        self.requests.push(request);
        Ok(None)
    }
}

fn apply_input_defaults(skill: &ValidatedSkill, mut inputs: JsonObject) -> JsonObject {
    for (name, input) in &skill.inputs {
        if !inputs.contains_key(name)
            && let Some(default) = &input.default
        {
            inputs.insert(name.clone(), default.clone());
        }
    }
    inputs
}

fn input_resolution_request(
    skill: &ValidatedSkill,
    inputs: &JsonObject,
) -> Option<ResolutionRequest> {
    let questions = skill
        .inputs
        .iter()
        .filter(|(name, input)| input.required && missing_input(inputs.get(*name)))
        .map(|(name, input)| Question {
            id: name.clone(),
            prompt: input
                .description
                .clone()
                .unwrap_or_else(|| format!("Provide {name}.")),
            description: input.description.clone(),
            required: true,
            question_type: input.input_type.clone(),
        })
        .collect::<Vec<_>>();
    (!questions.is_empty()).then(|| ResolutionRequest::Input {
        id: format!(
            "input.{}.{}",
            identifier_segment(&skill.name),
            questions
                .iter()
                .map(|question| identifier_segment(&question.id))
                .collect::<Vec<_>>()
                .join(".")
        ),
        questions,
    })
}

fn missing_input(value: Option<&JsonValue>) -> bool {
    match value {
        None | Some(JsonValue::Null) => true,
        Some(JsonValue::String(value)) => value.is_empty(),
        Some(_) => false,
    }
}

fn completed_runx(
    skill_name: &str,
    run_id: &str,
    receipt_id: &str,
    output: &SkillOutput,
) -> JsonObject {
    let mut runx = terminal_runx("completed", skill_name, run_id, receipt_id);
    runx.insert(
        "output".to_owned(),
        JsonValue::String(output.stdout.clone()),
    );
    runx
}

pub(super) fn terminal_runx(
    status: &str,
    skill_name: &str,
    run_id: &str,
    receipt_id: &str,
) -> JsonObject {
    [
        ("status".to_owned(), JsonValue::String(status.to_owned())),
        (
            "skillName".to_owned(),
            JsonValue::String(skill_name.to_owned()),
        ),
        ("runId".to_owned(), JsonValue::String(run_id.to_owned())),
        (
            "receiptId".to_owned(),
            JsonValue::String(receipt_id.to_owned()),
        ),
        ("events".to_owned(), JsonValue::Array(Vec::new())),
    ]
    .into()
}

pub(super) fn needs_agent_runx(
    skill_name: &str,
    run_id: &str,
    requests: &[ResolutionRequest],
) -> Result<JsonObject, RuntimeError> {
    Ok([
        (
            "status".to_owned(),
            JsonValue::String("needs_agent".to_owned()),
        ),
        (
            "skillName".to_owned(),
            JsonValue::String(skill_name.to_owned()),
        ),
        ("runId".to_owned(), JsonValue::String(run_id.to_owned())),
        (
            "requests".to_owned(),
            JsonValue::Array(
                requests
                    .iter()
                    .map(serde_json_value)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        ),
        ("events".to_owned(), JsonValue::Array(Vec::new())),
    ]
    .into())
}

fn serde_json_value<T: serde::Serialize>(value: &T) -> Result<JsonValue, RuntimeError> {
    let serialized = serde_json::to_string(value)
        .map_err(|source| RuntimeError::json("serializing MCP host result", source))?;
    serde_json::from_str(&serialized)
        .map_err(|source| RuntimeError::json("deserializing MCP host result", source))
}

fn skill_directory_for_execution(skill_path: &Path) -> PathBuf {
    if skill_path.is_dir() {
        skill_path.to_path_buf()
    } else {
        skill_path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    }
}

pub(super) fn identifier_segment(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_owned()
}
