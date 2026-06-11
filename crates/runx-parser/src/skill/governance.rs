use std::collections::BTreeMap;

use runx_contracts::{JsonObject, JsonValue};
use runx_core::policy::admit_agent_tool_ref;

use crate::ValidationError;

use super::{
    FIELDS, RawSkillIr, SkillArtifactContract, SkillGovernance, SkillIdempotencyPolicy, SkillInput,
    SkillRetryPolicy, field_value, first_value, nested_value, validate_execution_semantics,
};

pub(super) fn validate_skill_governance(
    raw: &RawSkillIr,
    runx: Option<&JsonObject>,
    risk: Option<&JsonValue>,
) -> Result<SkillGovernance, ValidationError> {
    Ok(SkillGovernance {
        retry: validate_retry(
            first_value(raw.frontmatter.get("retry"), field_value(runx, "retry")),
            "retry",
        )?,
        idempotency: validate_idempotency(
            first_value(
                raw.frontmatter.get("idempotency"),
                field_value(runx, "idempotency"),
            ),
            "idempotency",
        )?,
        mutating: validate_mutating(
            first_value(
                first_value(
                    raw.frontmatter.get("mutating"),
                    nested_value(risk, "mutating"),
                ),
                field_value(runx, "mutating"),
            ),
            "mutating",
        )?,
        artifacts: validate_artifact_contract(field_value(runx, "artifacts"), "runx.artifacts")?,
        allowed_tools: validate_allowed_tools(
            field_value(runx, "allowed_tools"),
            "runx.allowed_tools",
        )?,
        execution: validate_execution_semantics(
            first_value(
                raw.frontmatter.get("execution"),
                field_value(runx, "execution"),
            ),
            "execution",
        )?,
    })
}

pub fn validate_skill_artifact_contract(
    value: Option<&JsonValue>,
    field: &str,
) -> Result<Option<SkillArtifactContract>, ValidationError> {
    validate_artifact_contract(value, field)
}

pub(super) fn validate_artifact_contract(
    value: Option<&JsonValue>,
    field: &str,
) -> Result<Option<SkillArtifactContract>, ValidationError> {
    let Some(record) = FIELDS.optional_object(value, field)? else {
        return Ok(None);
    };
    let emits = match record.get("emits") {
        Some(JsonValue::String(value)) => Some(vec![value.clone()]),
        value => FIELDS.optional_string_array(value, &format!("{field}.emits"))?,
    };
    let named_emits = validate_named_emits(
        first_value(record.get("named_emits"), record.get("namedEmits")),
        &format!("{field}.named_emits"),
    )?;
    let wrap_as = FIELDS.optional_non_empty_string(
        first_value(record.get("wrap_as"), record.get("wrapAs")),
        &format!("{field}.wrap_as"),
    )?;
    if emits.is_none() && named_emits.is_none() && wrap_as.is_none() {
        return Ok(None);
    }
    Ok(Some(SkillArtifactContract {
        emits,
        named_emits,
        wrap_as,
    }))
}

pub(super) fn validate_inputs(
    inputs: JsonObject,
) -> Result<BTreeMap<String, SkillInput>, ValidationError> {
    inputs
        .into_iter()
        .map(|(name, value)| {
            let field = format!("inputs.{name}");
            let input = FIELDS.required_object(Some(&value), &field)?;
            Ok((
                name.clone(),
                SkillInput {
                    input_type: FIELDS
                        .optional_string(input.get("type"), &format!("{field}.type"))?
                        .unwrap_or_else(|| "string".to_owned()),
                    required: FIELDS
                        .optional_bool(input.get("required"), &format!("{field}.required"))?
                        .unwrap_or(false),
                    description: FIELDS.optional_string(
                        input.get("description"),
                        &format!("{field}.description"),
                    )?,
                    default: input.get("default").cloned(),
                },
            ))
        })
        .collect()
}

pub(super) fn validate_retry(
    value: Option<&JsonValue>,
    field: &str,
) -> Result<Option<SkillRetryPolicy>, ValidationError> {
    let Some(retry) = FIELDS.optional_object(value, field)? else {
        return Ok(None);
    };
    let max_attempts = FIELDS
        .optional_u64(retry.get("max_attempts"), &format!("{field}.max_attempts"))?
        .unwrap_or(1);
    if max_attempts == 0 {
        return Err(
            FIELDS.validation_error(format!("{field}.max_attempts must be a positive integer."))
        );
    }
    Ok(Some(SkillRetryPolicy { max_attempts }))
}

pub(super) fn validate_idempotency(
    value: Option<&JsonValue>,
    field: &str,
) -> Result<Option<SkillIdempotencyPolicy>, ValidationError> {
    match value {
        None | Some(JsonValue::Null) => Ok(None),
        Some(JsonValue::String(value)) if value.trim().is_empty() => {
            Err(FIELDS.validation_error(format!("{field} must not be empty.")))
        }
        Some(JsonValue::String(value)) => Ok(Some(SkillIdempotencyPolicy {
            key: Some(value.clone()),
        })),
        Some(value) => {
            let record = FIELDS.required_object(Some(value), field)?;
            Ok(Some(SkillIdempotencyPolicy {
                key: FIELDS
                    .optional_non_empty_string(record.get("key"), &format!("{field}.key"))?,
            }))
        }
    }
}

pub(super) fn validate_mutating(
    value: Option<&JsonValue>,
    field: &str,
) -> Result<Option<bool>, ValidationError> {
    FIELDS.optional_bool(value, field)
}

fn validate_named_emits(
    value: Option<&JsonValue>,
    field: &str,
) -> Result<Option<BTreeMap<String, String>>, ValidationError> {
    let Some(record) = FIELDS.optional_object(value, field)? else {
        return Ok(None);
    };
    record
        .into_iter()
        .map(|(key, value)| {
            let JsonValue::String(value) = value else {
                return Err(
                    FIELDS.validation_error(format!("{field}.{key} must be a non-empty string."))
                );
            };
            if value.trim().is_empty() {
                return Err(
                    FIELDS.validation_error(format!("{field}.{key} must be a non-empty string."))
                );
            }
            Ok((key, value))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()
        .map(Some)
}

pub(super) fn validate_allowed_tools(
    value: Option<&JsonValue>,
    field: &str,
) -> Result<Option<Vec<String>>, ValidationError> {
    let Some(values) = FIELDS.optional_string_array(value, field)? else {
        return Ok(None);
    };
    for value in &values {
        let admission = admit_agent_tool_ref(value);
        if !admission.allowed {
            return Err(FIELDS.validation_error(format!(
                "{field} entry {value:?} is not an admissible agent tool ref: {}.",
                admission.reason
            )));
        }
    }
    Ok(Some(values))
}
