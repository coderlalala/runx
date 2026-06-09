use runx_contracts::JsonObject;
use serde::{Deserialize, Serialize};

use crate::ValidationError;

use super::{optional_string, optional_string_array, required_string, validation_error};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogKind {
    Skill,
    Graph,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogAudience {
    Public,
    Builder,
    Operator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogVisibility {
    Public,
    Internal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CatalogRole {
    Canonical,
    Branded,
    Context,
    GraphStage,
    RuntimePath,
    HarnessFixture,
}

impl CatalogKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            CatalogKind::Skill => "skill",
            CatalogKind::Graph => "graph",
        }
    }
}

impl CatalogAudience {
    pub fn as_str(&self) -> &'static str {
        match self {
            CatalogAudience::Public => "public",
            CatalogAudience::Builder => "builder",
            CatalogAudience::Operator => "operator",
        }
    }
}

impl CatalogVisibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            CatalogVisibility::Public => "public",
            CatalogVisibility::Internal => "internal",
        }
    }
}

impl CatalogRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            CatalogRole::Canonical => "canonical",
            CatalogRole::Branded => "branded",
            CatalogRole::Context => "context",
            CatalogRole::GraphStage => "graph-stage",
            CatalogRole::RuntimePath => "runtime-path",
            CatalogRole::HarnessFixture => "harness-fixture",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogMetadata {
    pub kind: CatalogKind,
    pub audience: CatalogAudience,
    pub visibility: CatalogVisibility,
    pub role: CatalogRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_skill: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub part_of: Vec<String>,
}

pub(crate) fn validate_catalog_metadata(
    value: Option<JsonObject>,
    label: &str,
) -> Result<Option<CatalogMetadata>, ValidationError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let kind = parse_catalog_kind(&value, label)?;
    let audience = parse_catalog_audience(&value, label)?;
    let visibility = parse_catalog_visibility(&value, label)?;
    let role = parse_catalog_role(&value, label)?;
    validate_catalog_role(visibility, role, label)?;
    let canonical_skill = optional_string(
        value.get("canonical_skill"),
        &format!("{label}.canonical_skill"),
    )?;
    let provider = optional_string(value.get("provider"), &format!("{label}.provider"))?;
    let runtime_path =
        optional_string(value.get("runtime_path"), &format!("{label}.runtime_path"))?;
    let part_of = optional_string_array(value.get("part_of"), &format!("{label}.part_of"))?
        .unwrap_or_default();
    validate_catalog_bindings(role, &canonical_skill, &provider, &part_of, label)?;
    Ok(Some(CatalogMetadata {
        kind,
        audience,
        visibility,
        role,
        canonical_skill,
        provider,
        runtime_path,
        part_of,
    }))
}

fn parse_catalog_kind(value: &JsonObject, label: &str) -> Result<CatalogKind, ValidationError> {
    match required_string(value.get("kind"), &format!("{label}.kind"))?.as_str() {
        "skill" => Ok(CatalogKind::Skill),
        "graph" => Ok(CatalogKind::Graph),
        _ => Err(validation_error(format!(
            "{label}.kind must be skill or graph."
        ))),
    }
}

fn parse_catalog_audience(
    value: &JsonObject,
    label: &str,
) -> Result<CatalogAudience, ValidationError> {
    match required_string(value.get("audience"), &format!("{label}.audience"))?.as_str() {
        "public" => Ok(CatalogAudience::Public),
        "builder" => Ok(CatalogAudience::Builder),
        "operator" => Ok(CatalogAudience::Operator),
        _ => Err(validation_error(format!(
            "{label}.audience must be public, builder, or operator."
        ))),
    }
}

fn parse_catalog_visibility(
    value: &JsonObject,
    label: &str,
) -> Result<CatalogVisibility, ValidationError> {
    match optional_string(value.get("visibility"), &format!("{label}.visibility"))?.as_deref() {
        Some("public") | None => Ok(CatalogVisibility::Public),
        Some("internal") => Ok(CatalogVisibility::Internal),
        Some(_) => Err(validation_error(format!(
            "{label}.visibility must be public or internal."
        ))),
    }
}

fn parse_catalog_role(value: &JsonObject, label: &str) -> Result<CatalogRole, ValidationError> {
    match required_string(value.get("role"), &format!("{label}.role"))?.as_str() {
        "canonical" => Ok(CatalogRole::Canonical),
        "branded" => Ok(CatalogRole::Branded),
        "context" => Ok(CatalogRole::Context),
        "graph-stage" => Ok(CatalogRole::GraphStage),
        "runtime-path" => Ok(CatalogRole::RuntimePath),
        "harness-fixture" => Ok(CatalogRole::HarnessFixture),
        _ => Err(validation_error(format!(
            "{label}.role must be canonical, branded, context, graph-stage, runtime-path, or harness-fixture."
        ))),
    }
}

fn validate_catalog_role(
    visibility: CatalogVisibility,
    role: CatalogRole,
    label: &str,
) -> Result<(), ValidationError> {
    if visibility == CatalogVisibility::Public
        && matches!(
            role,
            CatalogRole::GraphStage | CatalogRole::RuntimePath | CatalogRole::HarnessFixture
        )
    {
        return Err(validation_error(format!(
            "{label}.role cannot be {} when visibility is public.",
            role.as_str()
        )));
    }
    Ok(())
}

fn validate_catalog_bindings(
    role: CatalogRole,
    canonical_skill: &Option<String>,
    provider: &Option<String>,
    part_of: &[String],
    label: &str,
) -> Result<(), ValidationError> {
    if role == CatalogRole::Branded {
        if canonical_skill.is_none() {
            return Err(validation_error(format!(
                "{label}.canonical_skill is required when catalog.role is branded."
            )));
        }
        if provider.is_none() {
            return Err(validation_error(format!(
                "{label}.provider is required when catalog.role is branded."
            )));
        }
    }
    if matches!(
        role,
        CatalogRole::GraphStage | CatalogRole::RuntimePath | CatalogRole::HarnessFixture
    ) && part_of.is_empty()
    {
        return Err(validation_error(format!(
            "{label}.part_of is required when catalog.role is {}.",
            role.as_str()
        )));
    }
    Ok(())
}
