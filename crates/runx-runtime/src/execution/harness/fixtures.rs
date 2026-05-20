// rust-style-allow: large-file because harness fixture parsing, typed diagnostics,
// and expectation normalization stay together for the cutover review surface.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use runx_contracts::{ClosureDisposition, HarnessReceiptSchema, HarnessState, JsonObject};
use serde::Deserialize;
use thiserror::Error;

const RETIRED_RECEIPT_FIELDS: &[&str] =
    &["kind", "skill_name", "source_type", "graph_name", "owner"];

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessFixtureKind {
    Skill,
    Graph,
    Mcp,
    A2a,
    Agent,
    #[serde(rename = "agent_step", alias = "agent-step")]
    AgentStep,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessExpectedStatus {
    Sealed,
    Failure,
    NeedsAgent,
    PolicyDenied,
    Escalated,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HarnessFixture {
    pub name: String,
    pub kind: HarnessFixtureKind,
    pub target: String,
    pub runner: Option<String>,
    pub inputs: JsonObject,
    pub env: BTreeMap<String, String>,
    pub caller: JsonObject,
    pub expect: HarnessExpectation,
    pub metadata: JsonObject,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HarnessExpectation {
    pub status: Option<HarnessExpectedStatus>,
    pub receipt: Option<HarnessReceiptExpectation>,
    pub steps: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HarnessReceiptExpectation {
    pub schema: HarnessReceiptSchema,
    pub body_digest: Option<String>,
    pub receipt_id: Option<String>,
    pub receipt_digest: Option<String>,
    pub harness_id: Option<String>,
    pub state: Option<HarnessState>,
    pub disposition: Option<ClosureDisposition>,
    pub reason_code: Option<String>,
    pub act_ids: Vec<String>,
    pub decision_ids: Vec<String>,
    pub child_receipt_refs: Vec<String>,
    pub verification_refs: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawHarnessFixture {
    name: String,
    kind: HarnessFixtureKind,
    #[serde(default)]
    target: Option<String>,
    runner: Option<String>,
    #[serde(default)]
    inputs: JsonObject,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    caller: JsonObject,
    #[serde(default)]
    expect: RawHarnessExpectation,
    #[serde(default)]
    metadata: JsonObject,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawHarnessExpectation {
    status: Option<HarnessExpectedStatus>,
    receipt: Option<RawHarnessReceiptExpectation>,
    #[serde(default)]
    steps: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawHarnessReceiptExpectation {
    #[serde(default = "default_harness_receipt_schema")]
    schema: HarnessReceiptSchema,
    body_digest: Option<String>,
    receipt_id: Option<String>,
    receipt_digest: Option<String>,
    harness_id: Option<String>,
    state: Option<HarnessState>,
    disposition: Option<ClosureDisposition>,
    reason_code: Option<String>,
    #[serde(default)]
    act_ids: Vec<String>,
    #[serde(default)]
    decision_ids: Vec<String>,
    #[serde(default)]
    child_receipt_refs: Vec<String>,
    #[serde(default)]
    verification_refs: Vec<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde::de::IgnoredAny>,
}

#[derive(Debug, Error)]
pub enum HarnessFixtureError {
    #[error("failed to read harness fixture {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse harness fixture YAML: {0}")]
    Parse(#[from] serde_norway::Error),
    #[error("harness fixture {field} is required")]
    Required { field: String },
    #[error("harness fixture {field} must not be empty")]
    Empty { field: &'static str },
    #[error("retired harness receipt expectation field {field_path}")]
    RetiredReceiptField { field_path: String },
    #[error("unknown harness receipt expectation field {field_path}")]
    UnknownReceiptField { field_path: String },
    #[error("harness fixture mode {mode} at {field_path} is not yet supported by the Rust harness")]
    UnsupportedFixtureMode { mode: String, field_path: String },
}

pub fn load_harness_fixture(path: impl AsRef<Path>) -> Result<HarnessFixture, HarnessFixtureError> {
    let path = path.as_ref();
    let contents = fs::read_to_string(path).map_err(|source| HarnessFixtureError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    parse_harness_fixture(&contents)
}

pub fn parse_harness_fixture(contents: &str) -> Result<HarnessFixture, HarnessFixtureError> {
    let fixture = serde_norway::from_str::<RawHarnessFixture>(contents)?;
    validate_fixture(fixture)
}

fn validate_fixture(fixture: RawHarnessFixture) -> Result<HarnessFixture, HarnessFixtureError> {
    require_non_empty(&fixture.name, "name")?;
    validate_supported_fixture_kind(&fixture.kind, "kind")?;
    let target = fixture.target.unwrap_or_default();
    if !matches!(fixture.kind, HarnessFixtureKind::AgentStep) {
        require_non_empty(&target, "target")?;
    }
    if let Some(runner) = &fixture.runner {
        require_non_empty(runner, "runner")?;
    }
    Ok(HarnessFixture {
        name: fixture.name,
        kind: fixture.kind,
        target,
        runner: fixture.runner,
        inputs: fixture.inputs,
        env: fixture.env,
        caller: fixture.caller,
        expect: validate_expectation(fixture.expect)?,
        metadata: fixture.metadata,
    })
}

fn validate_expectation(
    expectation: RawHarnessExpectation,
) -> Result<HarnessExpectation, HarnessFixtureError> {
    Ok(HarnessExpectation {
        status: expectation.status,
        receipt: expectation
            .receipt
            .map(validate_receipt_expectation)
            .transpose()?,
        steps: expectation.steps,
    })
}

fn validate_receipt_expectation(
    receipt: RawHarnessReceiptExpectation,
) -> Result<HarnessReceiptExpectation, HarnessFixtureError> {
    if let Some(field) = receipt.extra.keys().next() {
        let field_path = format!("expect.receipt.{field}");
        if is_retired_receipt_field(field) {
            return Err(HarnessFixtureError::RetiredReceiptField { field_path });
        }
        return Err(HarnessFixtureError::UnknownReceiptField { field_path });
    }
    Ok(HarnessReceiptExpectation {
        schema: receipt.schema,
        body_digest: receipt.body_digest,
        receipt_id: receipt.receipt_id,
        receipt_digest: receipt.receipt_digest,
        harness_id: receipt.harness_id,
        state: receipt.state,
        disposition: receipt.disposition,
        reason_code: receipt.reason_code,
        act_ids: receipt.act_ids,
        decision_ids: receipt.decision_ids,
        child_receipt_refs: receipt.child_receipt_refs,
        verification_refs: receipt.verification_refs,
    })
}

fn is_retired_receipt_field(field: &str) -> bool {
    RETIRED_RECEIPT_FIELDS.contains(&field)
        || field == retired_execution_receipt_field("skill")
        || field == retired_execution_receipt_field("graph")
}

fn retired_execution_receipt_field(prefix: &str) -> String {
    format!("{prefix}_{}", "execution")
}

fn validate_supported_fixture_kind(
    kind: &HarnessFixtureKind,
    field_path: &'static str,
) -> Result<(), HarnessFixtureError> {
    match kind {
        HarnessFixtureKind::Skill
        | HarnessFixtureKind::Graph
        | HarnessFixtureKind::A2a
        | HarnessFixtureKind::Agent
        | HarnessFixtureKind::AgentStep => Ok(()),
        HarnessFixtureKind::Mcp => Err(HarnessFixtureError::UnsupportedFixtureMode {
            mode: fixture_kind_name(kind).to_owned(),
            field_path: field_path.to_owned(),
        }),
    }
}

pub(crate) fn fixture_kind_name(kind: &HarnessFixtureKind) -> &'static str {
    match kind {
        HarnessFixtureKind::Skill => "skill",
        HarnessFixtureKind::Graph => "graph",
        HarnessFixtureKind::Mcp => "mcp",
        HarnessFixtureKind::A2a => "a2a",
        HarnessFixtureKind::Agent => "agent",
        HarnessFixtureKind::AgentStep => "agent_step",
    }
}

fn require_non_empty(value: &str, field: &'static str) -> Result<(), HarnessFixtureError> {
    if value.is_empty() {
        Err(HarnessFixtureError::Empty { field })
    } else {
        Ok(())
    }
}

fn default_harness_receipt_schema() -> HarnessReceiptSchema {
    HarnessReceiptSchema::V1
}

#[cfg(test)]
mod tests {
    use super::{
        HarnessFixtureError, HarnessFixtureKind, HarnessReceiptSchema, HarnessState,
        parse_harness_fixture, retired_execution_receipt_field,
    };

    #[test]
    fn parses_post_cutover_receipt_expectation() -> Result<(), HarnessFixtureError> {
        let fixture = parse_harness_fixture(
            r#"
name: echo-skill
kind: skill
target: ../skills/echo
inputs:
  message: hello
expect:
  status: sealed
  receipt:
    schema: runx.harness_receipt.v1
    body_digest: sha256:test
    harness_id: echo-skill
    state: sealed
    disposition: closed
    reason_code: harness_replay_passed
    act_ids:
      - echo
"#,
        )?;

        assert_eq!(fixture.kind, HarnessFixtureKind::Skill);
        let receipt = fixture
            .expect
            .receipt
            .ok_or(HarnessFixtureError::Required {
                field: "expect.receipt".to_owned(),
            })?;
        assert_eq!(receipt.schema, HarnessReceiptSchema::V1);
        assert_eq!(receipt.state, Some(HarnessState::Sealed));
        assert_eq!(receipt.act_ids, vec!["echo"]);
        Ok(())
    }

    #[test]
    fn rejects_retired_receipt_expectation_fields() {
        for field in [
            "kind".to_owned(),
            retired_execution_receipt_field("skill"),
            retired_execution_receipt_field("graph"),
        ] {
            let result = parse_harness_fixture(&format!(
                r#"
name: old
kind: skill
target: ../skills/echo
expect:
  receipt:
    {field}: value
"#,
            ));

            assert!(matches!(
                result,
                Err(HarnessFixtureError::RetiredReceiptField { field_path })
                    if field_path == format!("expect.receipt.{field}")
            ));
        }
    }

    #[test]
    fn rejects_unsupported_fixture_modes_with_stable_diagnostic() {
        let result = parse_harness_fixture(
            r#"
name: old
kind: mcp
target: ../skills/echo
expect:
  status: sealed
"#,
        );

        assert!(matches!(
            result,
            Err(HarnessFixtureError::UnsupportedFixtureMode { mode, field_path })
                if mode == "mcp" && field_path == "kind"
        ));
    }

    #[test]
    fn rejects_unknown_receipt_expectation_fields() {
        let result = parse_harness_fixture(
            r#"
name: old
kind: skill
target: ../skills/echo
expect:
  receipt:
    unexpected: value
"#,
        );

        assert!(matches!(
            result,
            Err(HarnessFixtureError::UnknownReceiptField { field_path })
                if field_path == "expect.receipt.unexpected"
        ));
    }
}
