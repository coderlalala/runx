//! Skill output declaration types: the value shape of the `runx.ai/spec`
//! output map (a field is either a bare type name or a typed field spec).
//!
//! The standalone `output.schema.json` document is a top-level open map carrying
//! a bare `$id`; it is modeled here as the transparent map newtype [`Output`],
//! whose `RunxSchema` derive emits the committed `patternProperties` shape. The
//! same `BTreeMap<String, OutputField>` is embedded by the agent-context
//! envelope's `output` field.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::schema::{NonEmptyString, RunxSchema};

/// A declared output value type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(rename_all = "lowercase")]
pub enum OutputType {
    String,
    Number,
    Integer,
    Boolean,
    Array,
    Object,
    Null,
}

/// The expanded form of an output field declaration. Committed with
/// `additionalProperties: false` and `minProperties: 1` (the latter is a
/// numeric bound the emitter does not express).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputFieldSpec {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<OutputType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap_as: Option<NonEmptyString>,
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
}

/// A single output field declaration: either a bare type name or a typed spec.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(untagged)]
pub enum OutputField {
    Type(OutputType),
    Spec(OutputFieldSpec),
}

/// The standalone `output.schema.json` document: a top-level open map of field
/// name to [`OutputField`], carrying the bare `runx.ai/spec` `$id`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(transparent)]
#[runx_schema(spec_id = "https://runx.ai/spec/output.schema.json")]
pub struct Output(pub BTreeMap<String, OutputField>);
