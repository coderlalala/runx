use runx_contracts::{ExecutionSemantics, JsonObject, JsonValue};

use crate::ValidationError;

mod catalog;
mod execution_semantics;
mod fixtures;
mod governance;
mod markdown;
mod runner_definition;
mod sandbox;
mod source;
mod types;

pub use catalog::{CatalogAudience, CatalogKind, CatalogMetadata, CatalogRole, CatalogVisibility};
pub use fixtures::{
    HarnessCallerFixture, HarnessExpectation, ReceiptExpectation, RunnerHarnessCase,
    RunnerHarnessManifest,
};
pub use governance::validate_skill_artifact_contract;
pub use markdown::{extract_skill_quality_profile, parse_skill_markdown};
pub use source::validate_skill_source;
pub use types::{
    InputMode, RawSkillIr, SkillArtifactContract, SkillHttpSource, SkillIdempotencyPolicy,
    SkillInput, SkillMcpServer, SkillQualityProfile, SkillRetryPolicy, SkillRunnerDefinition,
    SkillSandbox, SkillSource, SourceKind, ValidateSkillMode, ValidateSkillOptions, ValidatedSkill,
};

pub(crate) use catalog::validate_catalog_metadata;
pub(crate) use fixtures::validate_harness_manifest;
pub(crate) use runner_definition::validate_runner_definition;

use execution_semantics::validate_execution_semantics;
use governance::validate_skill_governance;
use governance::{
    validate_allowed_tools, validate_artifact_contract, validate_idempotency, validate_inputs,
    validate_mutating, validate_retry,
};
use sandbox::validate_sandbox;
use source::default_agent_source;
use source::validate_source;

struct SkillGovernance {
    retry: Option<SkillRetryPolicy>,
    idempotency: Option<SkillIdempotencyPolicy>,
    mutating: Option<bool>,
    artifacts: Option<SkillArtifactContract>,
    allowed_tools: Option<Vec<String>>,
    execution: Option<ExecutionSemantics>,
}

pub fn validate_skill(raw: RawSkillIr) -> Result<ValidatedSkill, ValidationError> {
    validate_skill_with_options(raw, ValidateSkillOptions::default())
}

pub fn validate_skill_with_options(
    raw: RawSkillIr,
    options: ValidateSkillOptions,
) -> Result<ValidatedSkill, ValidationError> {
    let runx = validate_runx_metadata(raw.frontmatter.get("runx"), options.mode)?;
    let source = raw
        .frontmatter
        .get("source")
        .map(|value| optional_object(Some(value), "source"))
        .transpose()?
        .flatten()
        .unwrap_or_else(default_agent_source);
    let risk = raw.frontmatter.get("risk").cloned();
    let governance = validate_skill_governance(&raw, runx.as_ref(), risk.as_ref())?;

    Ok(ValidatedSkill {
        name: required_string(raw.frontmatter.get("name"), "name")?,
        description: optional_string(raw.frontmatter.get("description"), "description")?,
        body: raw.body.clone(),
        source: validate_source(&source, runx.as_ref())?,
        inputs: validate_inputs(
            optional_object(raw.frontmatter.get("inputs"), "inputs")?.unwrap_or_default(),
        )?,
        auth: raw.frontmatter.get("auth").cloned(),
        risk: risk.clone(),
        runtime: raw.frontmatter.get("runtime").cloned(),
        retry: governance.retry,
        idempotency: governance.idempotency,
        mutating: governance.mutating,
        artifacts: governance.artifacts,
        quality_profile: extract_skill_quality_profile(&raw.body),
        allowed_tools: governance.allowed_tools,
        execution: governance.execution,
        runx,
        raw,
    })
}

fn validate_runx_metadata(
    value: Option<&JsonValue>,
    mode: ValidateSkillMode,
) -> Result<Option<JsonObject>, ValidationError> {
    match value {
        None | Some(JsonValue::Null) => Ok(None),
        Some(JsonValue::Object(value)) => Ok(Some(value.clone())),
        Some(_) if mode == ValidateSkillMode::Lenient => Ok(None),
        Some(_) => Err(ValidationError::InvalidField {
            field: "runx".to_owned(),
            message: "runx must be an object when present.".to_owned(),
        }),
    }
}

fn validation_error(message: impl Into<String>) -> ValidationError {
    ValidationError::InvalidField {
        field: "skill".to_owned(),
        message: message.into(),
    }
}

fn required_string(value: Option<&JsonValue>, field: &str) -> Result<String, ValidationError> {
    match optional_string(value, field)? {
        Some(value) if !value.is_empty() => Ok(value),
        _ => Err(ValidationError::MissingField {
            field: field.to_owned(),
        }),
    }
}

fn optional_string(
    value: Option<&JsonValue>,
    field: &str,
) -> Result<Option<String>, ValidationError> {
    match value {
        None | Some(JsonValue::Null) => Ok(None),
        Some(JsonValue::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(validation_error(format!("{field} must be a string."))),
    }
}

fn optional_non_empty_string(
    value: Option<&JsonValue>,
    field: &str,
) -> Result<Option<String>, ValidationError> {
    let Some(value) = optional_string(value, field)? else {
        return Ok(None);
    };
    if value.trim().is_empty() {
        return Err(validation_error(format!("{field} must not be empty.")));
    }
    Ok(Some(value))
}

fn required_object<'a>(
    value: Option<&'a JsonValue>,
    field: &str,
) -> Result<&'a JsonObject, ValidationError> {
    match value {
        Some(JsonValue::Object(value)) => Ok(value),
        None | Some(JsonValue::Null) => Err(validation_error(format!("{field} is required."))),
        Some(_) => Err(validation_error(format!("{field} must be an object."))),
    }
}

fn optional_object(
    value: Option<&JsonValue>,
    field: &str,
) -> Result<Option<JsonObject>, ValidationError> {
    match value {
        None | Some(JsonValue::Null) => Ok(None),
        Some(JsonValue::Object(value)) => Ok(Some(value.clone())),
        Some(_) => Err(validation_error(format!("{field} must be an object."))),
    }
}

fn required_plain_array<'a>(
    value: Option<&'a JsonValue>,
    field: &str,
) -> Result<&'a [JsonValue], ValidationError> {
    match value {
        Some(JsonValue::Array(values)) => Ok(values),
        None | Some(JsonValue::Null) => Err(validation_error(format!("{field} is required."))),
        Some(_) => Err(validation_error(format!("{field} must be an array."))),
    }
}

fn optional_string_array(
    value: Option<&JsonValue>,
    field: &str,
) -> Result<Option<Vec<String>>, ValidationError> {
    match value {
        None | Some(JsonValue::Null) => Ok(None),
        Some(JsonValue::Array(values)) => values
            .iter()
            .map(|value| match value {
                JsonValue::String(value) => Ok(value.clone()),
                _ => Err(validation_error(format!(
                    "{field} must be an array of strings."
                ))),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        Some(_) => Err(validation_error(format!(
            "{field} must be an array of strings."
        ))),
    }
}

fn optional_bool(value: Option<&JsonValue>, field: &str) -> Result<Option<bool>, ValidationError> {
    match value {
        None | Some(JsonValue::Null) => Ok(None),
        Some(JsonValue::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(validation_error(format!("{field} must be a boolean."))),
    }
}

fn optional_u64(value: Option<&JsonValue>, field: &str) -> Result<Option<u64>, ValidationError> {
    match value {
        None | Some(JsonValue::Null) => Ok(None),
        Some(JsonValue::Number(number)) => {
            let Some(value) = number.as_f64() else {
                return Err(validation_error(format!(
                    "{field} must be a finite number."
                )));
            };
            if value.fract() == 0.0 && value >= 0.0 && value <= u64::MAX as f64 {
                Ok(Some(value as u64))
            } else {
                Err(validation_error(format!(
                    "{field} must be a positive integer."
                )))
            }
        }
        Some(_) => Err(validation_error(format!(
            "{field} must be a finite number."
        ))),
    }
}

fn first_value<'a>(
    left: Option<&'a JsonValue>,
    right: Option<&'a JsonValue>,
) -> Option<&'a JsonValue> {
    match left {
        None | Some(JsonValue::Null) => right,
        Some(value) => Some(value),
    }
}

fn field_value<'a>(object: Option<&'a JsonObject>, field: &str) -> Option<&'a JsonValue> {
    object.and_then(|object| object.get(field))
}

fn nested_value<'a>(value: Option<&'a JsonValue>, field: &str) -> Option<&'a JsonValue> {
    match value {
        Some(JsonValue::Object(object)) => object.get(field),
        _ => None,
    }
}
