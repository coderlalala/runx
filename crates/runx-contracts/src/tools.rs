//! Contract types for tool manifests and tool catalog JSON surfaces.
// rust-style-allow: large-file - tool catalog contracts keep serde parity shapes together.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::JsonNumber;
use crate::schema::RunxSchema;

pub const TOOL_MANIFEST_SCHEMA: &str = "runx.tool.manifest.v1";
pub const TOOL_BUILD_REPORT_SCHEMA: &str = "runx.tool.build.v1";

pub type JsonPayloadObject = BTreeMap<String, JsonPayload>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonPayload {
    Null,
    Bool(bool),
    Number(JsonNumber),
    String(String),
    Array(Vec<JsonPayload>),
    Object(JsonPayloadObject),
}

impl RunxSchema for JsonPayload {
    fn json_schema() -> Value {
        json!({})
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
pub enum ToolManifestSchema {
    #[serde(rename = "runx.tool.manifest.v1")]
    V1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
pub enum ToolBuildReportSchema {
    #[serde(rename = "runx.tool.build.v1")]
    V1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolCommandInputMode {
    Args,
    Stdin,
    None,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ToolSourceType {
    CliTool,
    Mcp,
    A2a,
    Catalog,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolBuildStatus {
    Success,
    Failure,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolInspectOrigin {
    Local,
    Imported,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, RunxSchema)]
#[serde(deny_unknown_fields)]
#[runx_schema(id = "runx.tool.manifest.v1")]
pub struct ToolManifest {
    pub schema: ToolManifestSchema,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub source: ToolSource,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, ToolInput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk: Option<JsonPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runx: Option<JsonPayloadObject>,
    pub runtime: RuntimeCommand,
    pub output: ToolOutput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<ToolRetryPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency: Option<ToolIdempotencyPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mutating: Option<bool>,
    pub source_hash: String,
    pub schema_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toolkit_version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, RunxSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolInput {
    #[serde(rename = "type")]
    pub input_type: String,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<JsonPayload>,
    /// Marks this input as a structured artifact packet (rather than a scalar
    /// or free-form blob). Consumers that fanout/dedupe on artifact identity
    /// honour this flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, RunxSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub packet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap_as: Option<String>,
    /// Map of named-emit label → output key, when this tool fans out to
    /// multiple distinct artifact streams. Each label points at an entry in
    /// `outputs`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub named_emits: BTreeMap<String, String>,
    /// Per-output packet bindings keyed by output name. Populated alongside
    /// `named_emits` when a tool emits more than one packet.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub outputs: BTreeMap<String, ToolOutputBinding>,
    #[serde(flatten)]
    pub extra: JsonPayloadObject,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, RunxSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolOutputBinding {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub packet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap_as: Option<String>,
    #[serde(flatten)]
    pub extra: JsonPayloadObject,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, RunxSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolSource {
    #[serde(rename = "type")]
    pub source_type: ToolSourceType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_mode: Option<ToolCommandInputMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<ToolSandbox>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<ToolMcpServer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<JsonPayloadObject>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_card_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_identity: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolMcpServer {
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeCommand {
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ToolSandboxProfile {
    Readonly,
    WorkspaceWrite,
    Network,
    UnrestrictedLocalDev,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ToolSandboxCwdPolicy {
    SkillDirectory,
    Workspace,
    Custom,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolSandbox {
    pub profile: ToolSandboxProfile,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd_policy: Option<ToolSandboxCwdPolicy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_allowlist: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub writable_paths: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_enforcement: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolRetryPolicy {
    pub max_attempts: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolIdempotencyPolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuiltToolItem {
    pub path: String,
    pub manifest: String,
    pub source_hash: String,
    pub schema_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolBuildReport {
    pub schema: ToolBuildReportSchema,
    pub status: ToolBuildStatus,
    pub built: Vec<BuiltToolItem>,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCatalogSearchOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCatalogSearchResult {
    pub tool_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub source: String,
    pub source_label: String,
    pub source_type: String,
    pub namespace: String,
    pub external_name: String,
    pub required_scopes: Vec<String>,
    pub tags: Vec<String>,
    pub catalog_ref: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCatalogSearchReport {
    pub status: ToolBuildStatus,
    pub query: String,
    pub source: String,
    pub results: Vec<ToolCatalogSearchResult>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolInspectResult {
    #[serde(rename = "ref")]
    pub tool_ref: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub execution_source_type: String,
    pub inputs: BTreeMap<String, ToolInput>,
    pub scopes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mutating: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<RuntimeCommand>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk: Option<JsonPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runx: Option<ToolInspectRunx>,
    pub reference_path: String,
    pub skill_directory: String,
    pub provenance: ToolInspectProvenance,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolInspectReport {
    pub status: ToolBuildStatus,
    pub tool: ToolInspectResult,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolInspectRunx {
    Imported {
        imported_from: ToolInspectImportedFrom,
    },
    Object(JsonPayloadObject),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolInspectImportedFrom {
    pub source: String,
    pub source_label: String,
    pub source_type: String,
    pub namespace: String,
    pub external_name: String,
    pub digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolInspectOptions {
    #[serde(rename = "ref")]
    pub tool_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_from_directory: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolInspectProvenance {
    pub origin: ToolInspectOrigin,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        RuntimeCommand, ToolBuildReport, ToolBuildReportSchema, ToolBuildStatus,
        ToolCatalogSearchResult, ToolCommandInputMode, ToolInput, ToolInspectOrigin,
        ToolInspectProvenance, ToolInspectResult, ToolManifest, ToolManifestSchema, ToolOutput,
        ToolSource, ToolSourceType,
    };

    #[test]
    fn tool_manifest_round_trips_snake_case_fields() -> Result<(), serde_json::Error> {
        let json = r#"{
          "schema": "runx.tool.manifest.v1",
          "name": "fs.read",
          "description": "Read a UTF-8 text file.",
          "source": {
            "type": "cli-tool",
            "command": "node",
            "args": ["./run.mjs"],
            "timeout_seconds": 30,
            "input_mode": "stdin"
          },
          "inputs": {
            "path": {
              "type": "string",
              "required": true,
              "description": "Path to read."
            }
          },
          "output": {
            "packet": "runx.fs.file_read.v1",
            "wrap_as": "file_read"
          },
          "scopes": ["fs.read"],
          "runtime": {
            "command": "node",
            "args": ["./run.mjs"]
          },
          "source_hash": "sha256:source",
          "schema_hash": "sha256:schema",
          "toolkit_version": "0.1.4"
        }"#;

        let manifest: ToolManifest = serde_json::from_str(json)?;

        assert_eq!(manifest.schema, ToolManifestSchema::V1);
        assert_eq!(manifest.source.source_type, ToolSourceType::CliTool);
        assert_eq!(
            manifest.source.input_mode,
            Some(ToolCommandInputMode::Stdin)
        );
        assert_eq!(manifest.output.wrap_as.as_deref(), Some("file_read"));

        let encoded = serde_json::to_value(&manifest)?;
        assert_eq!(encoded["source"]["timeout_seconds"], 30);
        assert_eq!(encoded["runtime"]["args"][0], "./run.mjs");
        assert!(encoded.get("risk").is_none());
        Ok(())
    }

    #[test]
    fn tool_optional_manifest_fields_are_omitted() -> Result<(), serde_json::Error> {
        let encoded = serde_json::to_value(catalog_tool_manifest_fixture())?;

        assert!(encoded.get("description").is_none());
        assert!(encoded["source"].get("args").is_none());
        assert!(encoded["runtime"].get("env").is_none());
        assert!(encoded.get("toolkit_version").is_none());
        Ok(())
    }

    fn catalog_tool_manifest_fixture() -> ToolManifest {
        ToolManifest {
            schema: ToolManifestSchema::V1,
            name: "fixture.echo".to_owned(),
            version: None,
            description: None,
            source: catalog_tool_source_fixture(),
            runtime: RuntimeCommand {
                command: "node".to_owned(),
                args: Vec::new(),
                cwd: None,
                env: Default::default(),
            },
            inputs: [(
                "message".to_owned(),
                ToolInput {
                    input_type: "string".to_owned(),
                    required: true,
                    description: None,
                    default: None,
                    artifact: None,
                },
            )]
            .into_iter()
            .collect(),
            output: ToolOutput {
                packet: None,
                wrap_as: None,
                named_emits: BTreeMap::new(),
                outputs: BTreeMap::new(),
                extra: Default::default(),
            },
            scopes: Vec::new(),
            risk: None,
            retry: None,
            idempotency: None,
            mutating: None,
            runx: None,
            source_hash: "sha256:source".to_owned(),
            schema_hash: "sha256:schema".to_owned(),
            toolkit_version: None,
        }
    }

    fn catalog_tool_source_fixture() -> ToolSource {
        ToolSource {
            source_type: ToolSourceType::Catalog,
            command: None,
            args: Vec::new(),
            cwd: None,
            timeout_seconds: None,
            input_mode: None,
            sandbox: None,
            server: None,
            catalog_ref: Some("fixture-mcp:fixture.echo".to_owned()),
            tool: None,
            arguments: None,
            agent_card_url: None,
            agent_identity: None,
        }
    }

    #[test]
    fn tool_build_report_uses_cli_json_shape() -> Result<(), serde_json::Error> {
        let report: ToolBuildReport = serde_json::from_str(
            r#"{
              "schema": "runx.tool.build.v1",
              "status": "success",
              "built": [{
                "path": "tools/demo/echo",
                "manifest": "tools/demo/echo/manifest.json",
                "source_hash": "sha256:source",
                "schema_hash": "sha256:schema"
              }],
              "errors": []
            }"#,
        )?;

        assert_eq!(report.schema, ToolBuildReportSchema::V1);
        assert_eq!(report.status, ToolBuildStatus::Success);
        assert_eq!(report.built[0].manifest, "tools/demo/echo/manifest.json");
        Ok(())
    }

    #[test]
    fn tool_catalog_search_result_uses_executor_json_shape() -> Result<(), serde_json::Error> {
        let result: ToolCatalogSearchResult = serde_json::from_str(
            r#"{
              "tool_id": "fixture-mcp/fixture.echo",
              "name": "fixture.echo",
              "summary": "Echo a message.",
              "source": "fixture-mcp",
              "source_label": "Fixture MCP",
              "source_type": "mcp",
              "namespace": "fixture",
              "external_name": "echo",
              "required_scopes": ["fixture.echo"],
              "tags": ["mcp"],
              "catalog_ref": "fixture-mcp:fixture.echo"
            }"#,
        )?;

        assert_eq!(result.catalog_ref, "fixture-mcp:fixture.echo");
        assert_eq!(result.required_scopes, ["fixture.echo"]);
        Ok(())
    }

    #[test]
    fn tool_inspect_result_uses_provenance_shape() -> Result<(), serde_json::Error> {
        let result: ToolInspectResult = serde_json::from_str(
            r#"{
              "ref": "fixture.echo",
              "name": "fixture.echo",
              "execution_source_type": "catalog",
              "inputs": {},
              "scopes": ["fixture.echo"],
              "reference_path": "catalog:fixture-mcp:fixture.echo",
              "skill_directory": ".",
              "provenance": {
                "origin": "imported",
                "source": "fixture-mcp",
                "source_label": "Fixture MCP",
                "source_type": "mcp",
                "namespace": "fixture",
                "external_name": "echo",
                "catalog_ref": "fixture-mcp:fixture.echo",
                "tool_id": "fixture-mcp/fixture.echo",
                "tags": ["mcp"]
              }
            }"#,
        )?;

        assert_eq!(result.tool_ref, "fixture.echo");
        assert_eq!(
            result.provenance,
            ToolInspectProvenance {
                origin: ToolInspectOrigin::Imported,
                source: Some("fixture-mcp".to_owned()),
                source_label: Some("Fixture MCP".to_owned()),
                source_type: Some("mcp".to_owned()),
                namespace: Some("fixture".to_owned()),
                external_name: Some("echo".to_owned()),
                catalog_ref: Some("fixture-mcp:fixture.echo".to_owned()),
                tool_id: Some("fixture-mcp/fixture.echo".to_owned()),
                tags: Some(vec!["mcp".to_owned()]),
            }
        );
        Ok(())
    }
}
