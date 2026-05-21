use std::fs;
use std::path::{Path, PathBuf};

use runx_contracts::{JsonObject, JsonValue};
use runx_core::state_machine::{RetryPolicy, SequentialGraphStepDefinition};
use runx_parser::{ExecutionGraph, GraphStep, ValidatedSkill, parse_graph_yaml, validate_graph};

use crate::adapter::SkillOutput;
use crate::{RuntimeError, StepRun};

pub(crate) fn load_graph(graph_path: &Path) -> Result<ExecutionGraph, RuntimeError> {
    let source = fs::read_to_string(graph_path)
        .map_err(|source| RuntimeError::io("reading graph file", source))?;
    let raw = parse_graph_yaml(&source)?;
    validate_graph(raw).map_err(RuntimeError::from)
}

pub(crate) fn load_skill(skill_dir: &Path) -> Result<ValidatedSkill, RuntimeError> {
    let skill_path = skill_dir.join("SKILL.md");
    if !skill_path.exists() {
        return Err(RuntimeError::SkillFileMissing { path: skill_path });
    }
    let source = fs::read_to_string(&skill_path)
        .map_err(|source| RuntimeError::io("reading skill markdown", source))?;
    let raw = runx_parser::parse_skill_markdown(&source)?;
    runx_parser::validate_skill(raw).map_err(RuntimeError::from)
}

pub(crate) fn step_definitions(graph: &ExecutionGraph) -> Vec<SequentialGraphStepDefinition> {
    graph
        .steps
        .iter()
        .map(|step| SequentialGraphStepDefinition {
            id: step.id.clone(),
            context_from: context_from(step),
            retry: step.retry.as_ref().map(|retry| RetryPolicy {
                max_attempts: retry_attempts(retry.max_attempts),
            }),
            fanout_group: step.fanout_group.clone(),
        })
        .collect()
}

pub(crate) fn find_step<'a>(
    graph: &'a ExecutionGraph,
    step_id: &str,
) -> Result<&'a GraphStep, RuntimeError> {
    graph
        .steps
        .iter()
        .find(|step| step.id == step_id)
        .ok_or_else(|| RuntimeError::StepMissing {
            step_id: step_id.to_owned(),
        })
}

pub(crate) fn skill_dir(graph_dir: &Path, step: &GraphStep) -> Result<PathBuf, RuntimeError> {
    let Some(skill) = &step.skill else {
        return Err(RuntimeError::StepMissingSkill {
            step_id: step.id.clone(),
        });
    };
    Ok(graph_dir.join(skill))
}

pub(crate) fn resolve_inputs(
    step: &GraphStep,
    prior_runs: &[StepRun],
) -> Result<JsonObject, RuntimeError> {
    let mut inputs = step.inputs.clone();
    for edge in &step.context_edges {
        let value = context_output(prior_runs, &edge.from_step, &edge.output)?;
        inputs.insert(edge.input.clone(), value);
    }
    Ok(inputs)
}

pub(crate) fn output_object(output: &SkillOutput) -> JsonObject {
    let mut object = JsonObject::new();
    if let Ok(JsonValue::Object(parsed)) = serde_json::from_str::<JsonValue>(&output.stdout) {
        object.extend(parsed);
    }
    object.insert(
        "stdout".to_owned(),
        JsonValue::String(output.stdout.clone()),
    );
    object.insert(
        "stderr".to_owned(),
        JsonValue::String(output.stderr.clone()),
    );
    object.insert(
        "status".to_owned(),
        JsonValue::String(if output.succeeded() {
            "success".to_owned()
        } else {
            "failure".to_owned()
        }),
    );
    object
}

fn context_from(step: &GraphStep) -> Option<Vec<String>> {
    let refs = step
        .context_edges
        .iter()
        .map(|edge| edge.from_step.clone())
        .collect::<Vec<_>>();
    (!refs.is_empty()).then_some(refs)
}

fn retry_attempts(max_attempts: u64) -> u32 {
    u32::try_from(max_attempts).unwrap_or(u32::MAX)
}

fn context_output(
    prior_runs: &[StepRun],
    from_step: &str,
    output: &str,
) -> Result<JsonValue, RuntimeError> {
    let Some(run) = prior_runs.iter().find(|run| run.step_id == from_step) else {
        return Err(RuntimeError::GraphBlocked {
            step_id: from_step.to_owned(),
            reason: "context source step has not run".to_owned(),
        });
    };
    Ok(resolve_output_path(&run.outputs, output).unwrap_or(JsonValue::Null))
}

fn resolve_output_path(outputs: &JsonObject, output: &str) -> Option<JsonValue> {
    let mut segments = output.split('.');
    let first = segments.next()?;
    let mut value = outputs.get(first)?;
    for segment in segments {
        let JsonValue::Object(object) = value else {
            return None;
        };
        value = object.get(segment)?;
    }
    Some(value.clone())
}
