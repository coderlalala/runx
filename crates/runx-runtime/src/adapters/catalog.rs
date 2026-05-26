use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use runx_contracts::{JsonNumber, JsonObject, JsonValue, sha256_hex};
use runx_parser::{SkillArtifactContract, SkillSource};

use crate::RuntimeError;
use crate::adapter::{
    FanoutExecutionMode, InvocationStatus, SkillAdapter, SkillInvocation, SkillOutput,
};
use crate::adapters::cli_tool::CliToolAdapter;
use crate::tool_catalogs::search::{FixtureTool, fixture_tool};
use crate::tool_catalogs::{ToolCatalogError, ToolInspectOptions, resolve_local_tool};

const MISSING_CATALOG_REF: &str = "Catalog source requires source.catalog_ref metadata.";

#[derive(Clone, Debug, Default)]
pub struct CatalogAdapter {
    fixture_catalog_enabled: bool,
}

impl CatalogAdapter {
    #[must_use]
    pub fn fixture_catalog() -> Self {
        Self {
            fixture_catalog_enabled: true,
        }
    }
}

impl SkillAdapter for CatalogAdapter {
    fn adapter_type(&self) -> &'static str {
        "catalog"
    }

    fn invoke(&self, request: SkillInvocation) -> Result<SkillOutput, RuntimeError> {
        let started = Instant::now();
        if request.source.source_type != runx_parser::SourceKind::Catalog {
            return Err(RuntimeError::UnsupportedAdapter {
                adapter_type: request.source.source_type.as_str().to_owned(),
            });
        }
        let Some(catalog_ref) = request.source.catalog_ref.as_deref() else {
            return Ok(failure(MISSING_CATALOG_REF, started));
        };
        let catalog_ref = catalog_ref.trim();
        if catalog_ref.is_empty() {
            return Ok(failure(MISSING_CATALOG_REF, started));
        }

        if let Some(output) = invoke_local_tool(catalog_ref, &request, started)? {
            return Ok(output);
        }
        if !self.fixture_catalog_enabled {
            return Ok(missing_imported_tool(catalog_ref, started));
        }
        let Some(tool) = fixture_tool(catalog_ref) else {
            return Ok(missing_imported_tool(catalog_ref, started));
        };

        Ok(invoke_fixture_tool(
            &tool,
            &request.inputs,
            &request.env,
            started,
        ))
    }

    fn fanout_execution_mode(&self, source: &SkillSource) -> FanoutExecutionMode {
        if source.source_type == runx_parser::SourceKind::Catalog {
            FanoutExecutionMode::IsolatedParallel
        } else {
            FanoutExecutionMode::Serial
        }
    }

    fn clone_for_fanout(&self) -> Option<Box<dyn SkillAdapter + Send + Sync>> {
        Some(Box::new(self.clone()))
    }
}

fn invoke_local_tool(
    catalog_ref: &str,
    request: &SkillInvocation,
    started: Instant,
) -> Result<Option<SkillOutput>, RuntimeError> {
    let resolution = match resolve_local_tool(&ToolInspectOptions {
        root: workspace_root(&request.env, &request.skill_directory),
        tool_ref: catalog_ref.to_owned(),
        source: None,
        search_from_directory: request.skill_directory.clone(),
        tool_roots: configured_tool_roots(&request.env),
        fixture_catalog_enabled: false,
    }) {
        Ok(resolution) => resolution,
        Err(error) if local_lookup_miss(&error) => return Ok(None),
        Err(error) => return Err(catalog_error(&request.skill_name, error)),
    };

    if resolution.tool.source.source_type != runx_parser::SourceKind::CliTool {
        return Ok(Some(failure(
            format!(
                "Resolved catalog tool '{}' uses unsupported Rust adapter '{}'.",
                resolution.tool.name, resolution.tool.source.source_type
            ),
            started,
        )));
    }

    let artifacts = resolution.tool.artifacts.clone();
    let mut source = resolution.tool.source;
    let skill_directory = manifest_directory(&resolution.manifest_path, &request.skill_directory);
    normalize_local_cli_source(&mut source, &skill_directory);
    let invocation = SkillInvocation {
        skill_name: resolution.tool.name,
        source,
        inputs: request.inputs.clone(),
        resolved_inputs: request.resolved_inputs.clone(),
        skill_directory,
        env: request.env.clone(),
        credential_delivery: request.credential_delivery.clone(),
    };
    let mut output = CliToolAdapter.invoke(invocation)?;
    apply_local_tool_artifact_wrappers(&mut output, artifacts.as_ref())?;
    Ok(Some(output))
}

fn apply_local_tool_artifact_wrappers(
    output: &mut SkillOutput,
    artifacts: Option<&SkillArtifactContract>,
) -> Result<(), RuntimeError> {
    let Some(artifacts) = artifacts else {
        return Ok(());
    };
    let Ok(JsonValue::Object(mut object)) = serde_json::from_str::<JsonValue>(&output.stdout)
    else {
        return Ok(());
    };

    let mut changed = false;
    if let Some(wrap_as) = artifacts.wrap_as.as_deref()
        && !object.contains_key(wrap_as)
    {
        let mut wrapper = JsonObject::new();
        wrapper.insert("data".to_owned(), JsonValue::Object(object.clone()));
        object.insert(wrap_as.to_owned(), JsonValue::Object(wrapper));
        changed = true;
    }

    if let Some(named_emits) = &artifacts.named_emits {
        for name in named_emits.keys() {
            let Some(value) = object.get(name).cloned() else {
                continue;
            };
            let mut wrapper = JsonObject::new();
            wrapper.insert("data".to_owned(), value);
            object.insert(name.clone(), JsonValue::Object(wrapper));
            changed = true;
        }
    }

    if changed {
        output.stdout = serde_json::to_string(&JsonValue::Object(object)).map_err(|source| {
            RuntimeError::json("serializing catalog artifact wrappers", source)
        })?;
    }
    Ok(())
}

fn configured_tool_roots(env: &std::collections::BTreeMap<String, String>) -> Vec<PathBuf> {
    env.get("RUNX_TOOL_ROOTS")
        .map(|value| {
            std::env::split_paths(value)
                .filter(|path| !path.as_os_str().is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn workspace_root(env: &std::collections::BTreeMap<String, String>, fallback: &Path) -> PathBuf {
    env.get("RUNX_CWD")
        .or_else(|| env.get("RUNX_PROJECT_DIR"))
        .map(PathBuf::from)
        .unwrap_or_else(|| fallback.to_path_buf())
}

fn local_lookup_miss(error: &ToolCatalogError) -> bool {
    match error {
        ToolCatalogError::NotFound(_) => true,
        ToolCatalogError::InvalidRequest(message) => message.contains("must include a namespace"),
        ToolCatalogError::Io { .. }
        | ToolCatalogError::Json { .. }
        | ToolCatalogError::InvalidManifest { .. } => false,
    }
}

fn catalog_error(skill_name: &str, error: ToolCatalogError) -> RuntimeError {
    RuntimeError::SkillFailed {
        skill_name: skill_name.to_owned(),
        message: error.to_string(),
    }
}

fn manifest_directory(manifest_path: &Path, fallback: &Path) -> PathBuf {
    manifest_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| fallback.to_path_buf())
}

fn normalize_local_cli_source(source: &mut SkillSource, skill_directory: &Path) {
    if source.cwd.is_none() {
        source.cwd = Some(skill_directory.to_string_lossy().into_owned());
    }
}

fn invoke_fixture_tool(
    tool: &FixtureTool,
    inputs: &JsonObject,
    env: &BTreeMap<String, String>,
    started: Instant,
) -> SkillOutput {
    match tool.name {
        "echo" => success(
            json_string(inputs.get("message")).unwrap_or_default(),
            tool.name,
            started,
        ),
        "env" => success(env_value(inputs.get("name"), env), tool.name, started),
        "fail" => failure_with_metadata(
            format!(
                "MCP error -32000: fixture failure: {}",
                json_string(inputs.get("message")).unwrap_or_default()
            ),
            tool.name,
            started,
        ),
        "sleep" => failure_with_metadata(
            "MCP call timed out after 60000ms.".to_owned(),
            tool.name,
            started,
        ),
        _ => failure_with_metadata(
            format!("MCP error -32601: tool not found: {}", tool.name),
            tool.name,
            started,
        ),
    }
}

fn success(stdout: String, tool_name: &str, started: Instant) -> SkillOutput {
    SkillOutput {
        status: InvocationStatus::Success,
        stdout,
        stderr: String::new(),
        exit_code: Some(0),
        duration_ms: duration_ms(started),
        metadata: mcp_metadata(tool_name),
    }
}

fn failure(message: impl Into<String>, started: Instant) -> SkillOutput {
    let message = message.into();
    SkillOutput {
        status: InvocationStatus::Failure,
        stdout: String::new(),
        stderr: message,
        exit_code: None,
        duration_ms: duration_ms(started),
        metadata: JsonObject::new(),
    }
}

fn failure_with_metadata(message: String, tool_name: &str, started: Instant) -> SkillOutput {
    SkillOutput {
        status: InvocationStatus::Failure,
        stdout: String::new(),
        stderr: message,
        exit_code: None,
        duration_ms: duration_ms(started),
        metadata: mcp_metadata(tool_name),
    }
}

fn missing_imported_tool(catalog_ref: &str, started: Instant) -> SkillOutput {
    failure(
        format!("Imported tool '{catalog_ref}' was not found in configured tool catalogs."),
        started,
    )
}

fn json_string(value: Option<&JsonValue>) -> Option<String> {
    match value {
        Some(JsonValue::String(value)) => Some(value.clone()),
        Some(JsonValue::Bool(value)) => Some(value.to_string()),
        Some(JsonValue::Number(value)) => Some(json_number_string(value)),
        Some(JsonValue::Null) | None => None,
        Some(JsonValue::Array(_)) | Some(JsonValue::Object(_)) => {
            Some("[object Object]".to_owned())
        }
    }
}

fn json_number_string(value: &JsonNumber) -> String {
    match value {
        JsonNumber::I64(value) => value.to_string(),
        JsonNumber::U64(value) => value.to_string(),
        JsonNumber::F64(value) if value.fract() == 0.0 => format!("{value:.0}"),
        JsonNumber::F64(value) => value.to_string(),
    }
}

fn env_value(name: Option<&JsonValue>, env: &BTreeMap<String, String>) -> String {
    let Some(name) = json_string(name) else {
        return String::new();
    };
    env.get(&name).cloned().unwrap_or_default()
}

fn mcp_metadata(tool_name: &str) -> JsonObject {
    let mut mcp = JsonObject::new();
    mcp.insert("tool".to_owned(), JsonValue::String(tool_name.to_owned()));
    mcp.insert(
        "server_command_hash".to_owned(),
        JsonValue::String(sha256_hex(b"runx-runtime-fixture-catalog")),
    );
    mcp.insert(
        "server_args_hash".to_owned(),
        JsonValue::String(sha256_hex(b"[]")),
    );

    let mut metadata = JsonObject::new();
    metadata.insert("mcp".to_owned(), JsonValue::Object(mcp));
    metadata
}

fn duration_ms(started: Instant) -> u64 {
    let millis = started.elapsed().as_millis();
    u64::try_from(millis).unwrap_or(u64::MAX)
}
