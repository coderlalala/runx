use std::collections::BTreeMap;

use runx_contracts::{JsonObject, JsonValue};
use serde::{Deserialize, Serialize};

use crate::skill::{
    SkillArtifactContract, SkillIdempotencyPolicy, SkillInput, SkillRetryPolicy, SkillSource,
    validate_skill_artifact_contract, validate_skill_source,
};
use crate::{
    ParseError, ValidationError, assert_yaml_parity_subset,
    json_fields::{self, JsonFieldReader},
};

const FIELDS: JsonFieldReader = JsonFieldReader::new("tool_manifest");

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RawToolManifestIr {
    pub document: JsonObject,
    pub raw: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidatedTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub source: SkillSource,
    pub inputs: BTreeMap<String, SkillInput>,
    pub scopes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<SkillRetryPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency: Option<SkillIdempotencyPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mutating: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<SkillArtifactContract>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runx: Option<JsonObject>,
    pub raw: RawToolManifestIr,
}

pub fn parse_tool_manifest_yaml(yaml: &str) -> Result<RawToolManifestIr, ParseError> {
    assert_yaml_parity_subset("tool_manifest", yaml)?;
    let parsed: JsonValue =
        serde_norway::from_str(yaml).map_err(|error| ParseError::InvalidYaml {
            field: "tool_manifest".to_owned(),
            message: error.to_string(),
        })?;
    manifest_from_value(parsed, yaml, "Tool manifest YAML must parse to an object.")
}

pub fn parse_tool_manifest_json(json: &str) -> Result<RawToolManifestIr, ParseError> {
    let parsed: JsonValue =
        serde_json::from_str(json).map_err(|error| ParseError::InvalidJson {
            field: "tool_manifest".to_owned(),
            message: format!("Tool manifest JSON is invalid: {error}"),
        })?;
    manifest_from_value(parsed, json, "Tool manifest JSON must parse to an object.")
}

pub fn validate_tool_manifest(raw: RawToolManifestIr) -> Result<ValidatedTool, ValidationError> {
    let runx = FIELDS.optional_object(raw.document.get("runx"), "runx")?;
    let risk = raw.document.get("risk").cloned();
    let source = validate_tool_source(
        validate_skill_source(
            &FIELDS
                .required_object(raw.document.get("source"), "source")?
                .clone(),
            runx.as_ref(),
        )?,
        "source.type",
    )?;
    Ok(ValidatedTool {
        name: FIELDS.required_string(raw.document.get("name"), "name")?,
        description: FIELDS.optional_string(raw.document.get("description"), "description")?,
        source,
        inputs: validate_inputs(
            FIELDS
                .optional_object(raw.document.get("inputs"), "inputs")?
                .unwrap_or_default(),
        )?,
        scopes: FIELDS
            .optional_string_array(raw.document.get("scopes"), "scopes")?
            .unwrap_or_default(),
        risk: risk.clone(),
        runtime: raw.document.get("runtime").cloned(),
        retry: validate_retry(
            json_fields::first_value(
                raw.document.get("retry"),
                json_fields::field_value(runx.as_ref(), "retry"),
            ),
            "retry",
        )?,
        idempotency: validate_idempotency(
            json_fields::first_value(
                raw.document.get("idempotency"),
                json_fields::field_value(runx.as_ref(), "idempotency"),
            ),
            "idempotency",
        )?,
        mutating: validate_mutating(
            json_fields::first_value(
                json_fields::first_value(
                    raw.document.get("mutating"),
                    json_fields::nested_value(risk.as_ref(), "mutating"),
                ),
                json_fields::field_value(runx.as_ref(), "mutating"),
            ),
            "mutating",
        )?,
        artifacts: validate_skill_artifact_contract(
            json_fields::field_value(runx.as_ref(), "artifacts"),
            "runx.artifacts",
        )?,
        runx,
        raw,
    })
}

fn validate_tool_source(source: SkillSource, field: &str) -> Result<SkillSource, ValidationError> {
    if matches!(
        source.source_type.as_str(),
        "cli-tool" | "mcp" | "a2a" | "catalog" | "http"
    ) {
        return Ok(source);
    }
    Err(FIELDS.validation_error(format!(
        "{field} must be one of cli-tool, mcp, a2a, catalog, or http for tool manifests."
    )))
}

fn manifest_from_value(
    value: JsonValue,
    raw: &str,
    object_error: &str,
) -> Result<RawToolManifestIr, ParseError> {
    let JsonValue::Object(document) = value else {
        return Err(ParseError::InvalidDocument {
            field: "tool_manifest".to_owned(),
            message: object_error.to_owned(),
        });
    };
    Ok(RawToolManifestIr {
        document,
        raw: raw.to_owned(),
    })
}

fn validate_inputs(inputs: JsonObject) -> Result<BTreeMap<String, SkillInput>, ValidationError> {
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

fn validate_retry(
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

fn validate_idempotency(
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

fn validate_mutating(
    value: Option<&JsonValue>,
    field: &str,
) -> Result<Option<bool>, ValidationError> {
    FIELDS.optional_bool(value, field)
}
