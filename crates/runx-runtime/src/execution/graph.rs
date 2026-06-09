use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use runx_contracts::{JsonObject, JsonValue};
use runx_core::state_machine::{RetryPolicy, SequentialGraphStepDefinition};
use runx_parser::{
    ExecutionGraph, GraphStep, SkillRunnerDefinition, SkillRunnerManifest, SkillSource,
    ValidatedSkill, parse_graph_yaml, parse_runner_manifest_yaml, validate_graph,
    validate_runner_manifest,
};

use crate::{RuntimeError, StepRun};

use super::graph_index::PriorRunIndex;

#[derive(Clone)]
pub(crate) struct LoadedStepSkill {
    pub(crate) name: String,
    pub(crate) source: SkillSource,
    pub(crate) directory: PathBuf,
}

#[derive(Default)]
pub(crate) struct StepSkillCache {
    loaded: BTreeMap<String, LoadedStepSkill>,
}

impl StepSkillCache {
    pub(crate) fn load(
        &mut self,
        graph_dir: &Path,
        step: &GraphStep,
    ) -> Result<LoadedStepSkill, RuntimeError> {
        if let Some(skill) = self.loaded.get(&step.id) {
            return Ok(skill.clone());
        }
        let skill = load_step_skill(graph_dir, step)?;
        self.loaded.insert(step.id.clone(), skill.clone());
        Ok(skill)
    }
}

pub(crate) fn load_graph(graph_path: &Path) -> Result<ExecutionGraph, RuntimeError> {
    let source = fs::read_to_string(graph_path)
        .map_err(|source| RuntimeError::io("reading graph file", source))?;
    let raw = parse_graph_yaml(&source)?;
    validate_graph(raw).map_err(RuntimeError::from)
}

pub(crate) fn materialize_graph_inputs(
    mut graph: ExecutionGraph,
    graph_inputs: &JsonObject,
) -> ExecutionGraph {
    for step in &mut graph.steps {
        let mut inputs = graph_inputs.clone();
        for (key, value) in &step.inputs {
            if let Some(value) = materialize_graph_input_value(value, graph_inputs) {
                inputs.insert(key.clone(), value);
            } else {
                inputs.remove(key);
            }
        }
        step.inputs = inputs;
    }
    graph
}

fn materialize_graph_input_value(
    value: &JsonValue,
    graph_inputs: &JsonObject,
) -> Option<JsonValue> {
    match value {
        JsonValue::String(value) => {
            if let Some(path) = value.strip_prefix("$input.") {
                return resolve_graph_input_path(graph_inputs, path).cloned();
            }
            if value.starts_with("{{") && value.ends_with("}}") {
                let path = value.trim_start_matches('{').trim_end_matches('}').trim();
                return resolve_graph_input_path(graph_inputs, path).cloned();
            }
            Some(JsonValue::String(value.clone()))
        }
        JsonValue::Array(values) => Some(JsonValue::Array(
            values
                .iter()
                .filter_map(|value| materialize_graph_input_value(value, graph_inputs))
                .collect(),
        )),
        JsonValue::Object(object) => Some(JsonValue::Object(
            object
                .iter()
                .filter_map(|(key, value)| {
                    materialize_graph_input_value(value, graph_inputs)
                        .map(|value| (key.clone(), value))
                })
                .collect(),
        )),
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) => Some(value.clone()),
    }
}

fn resolve_graph_input_path<'a>(value: &'a JsonObject, path: &str) -> Option<&'a JsonValue> {
    let mut current: Option<&JsonValue> = None;
    for segment in path.split('.') {
        current = match current {
            None => value.get(segment),
            Some(JsonValue::Object(object)) => object.get(segment),
            Some(_) => return None,
        };
    }
    current
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

pub(crate) fn load_step_skill(
    graph_dir: &Path,
    step: &GraphStep,
) -> Result<LoadedStepSkill, RuntimeError> {
    let directory = skill_dir(graph_dir, step)?;
    if let Some(runner) = load_step_runner(&directory, step.runner.as_deref())? {
        return Ok(LoadedStepSkill {
            name: runner.name,
            source: runner.source,
            directory,
        });
    }
    let skill = load_skill(&directory)?;
    Ok(LoadedStepSkill {
        name: skill.name,
        source: skill.source,
        directory,
    })
}

fn load_step_runner(
    skill_dir: &Path,
    requested_runner: Option<&str>,
) -> Result<Option<SkillRunnerDefinition>, RuntimeError> {
    let manifest_path = skill_dir.join("X.yaml");
    if !manifest_path.exists() {
        if let Some(runner) = requested_runner {
            return Err(RuntimeError::UnsupportedRunnerSelection {
                runner: runner.to_owned(),
            });
        }
        return Ok(None);
    }
    let source = fs::read_to_string(&manifest_path).map_err(|source| {
        RuntimeError::io(format!("reading {}", manifest_path.display()), source)
    })?;
    let parsed = parse_runner_manifest_yaml(&source).map_err(RuntimeError::from)?;
    let manifest = validate_runner_manifest(parsed).map_err(RuntimeError::from)?;
    select_step_runner(&manifest, requested_runner)
        .cloned()
        .map(Some)
}

fn select_step_runner<'a>(
    manifest: &'a SkillRunnerManifest,
    requested_runner: Option<&str>,
) -> Result<&'a SkillRunnerDefinition, RuntimeError> {
    if let Some(runner) = requested_runner {
        return manifest.runners.get(runner).ok_or_else(|| {
            RuntimeError::UnsupportedRunnerSelection {
                runner: runner.to_owned(),
            }
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
        }),
        [] => Err(RuntimeError::UnsupportedRunnerSelection {
            runner: "default".to_owned(),
        }),
        _ => Err(RuntimeError::UnsupportedRunnerSelection {
            runner: "default".to_owned(),
        }),
    }
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
    if let Some(skill) = &step.skill {
        return Ok(graph_dir.join(skill));
    }
    if let Some(stage) = &step.stage {
        return stage_dir(graph_dir, step, stage);
    }
    Err(RuntimeError::StepMissingSkill {
        step_id: step.id.clone(),
    })
}

fn stage_dir(graph_dir: &Path, step: &GraphStep, stage: &str) -> Result<PathBuf, RuntimeError> {
    let stage_path = Path::new(stage);
    if stage_path.is_absolute()
        || stage_path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(RuntimeError::InvalidRunStep {
            step_id: step.id.clone(),
            reason: format!("stage reference {stage:?} must be relative below graph"),
        });
    }
    let root = graph_dir.join("graph");
    if !root.is_dir() {
        return Err(RuntimeError::InvalidRunStep {
            step_id: step.id.clone(),
            reason: "stage reference requires a graph directory in the current skill package"
                .to_owned(),
        });
    };
    Ok(root.join(stage_path))
}

pub(crate) fn resolve_inputs(
    step: &GraphStep,
    prior_runs: &[StepRun],
) -> Result<JsonObject, RuntimeError> {
    let prior_run_index = PriorRunIndex::new(prior_runs);
    resolve_inputs_with_index(step, &prior_run_index)
}

pub(crate) fn resolve_inputs_with_index(
    step: &GraphStep,
    prior_run_index: &PriorRunIndex<'_>,
) -> Result<JsonObject, RuntimeError> {
    let mut inputs = step.inputs.clone();
    if step.context_edges.is_empty() {
        return Ok(inputs);
    }
    for edge in &step.context_edges {
        let value = prior_run_index.output(&edge.from_step, &edge.output)?;
        inputs.insert(edge.input.clone(), value);
    }
    Ok(inputs)
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
