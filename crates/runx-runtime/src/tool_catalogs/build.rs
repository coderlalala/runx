// rust-style-allow: large-file because tool-manifest build keeps source/schema
// hashing, raw payload normalization, output binding shape, and stable JSON
// emission together so the TS doctor and the rust runtime agree byte-for-byte.
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use runx_contracts::sha256_prefixed;
use runx_contracts::tools::{
    BuiltToolItem, JsonPayload, JsonPayloadObject, RuntimeCommand, ToolBuildReport,
    ToolBuildReportSchema, ToolBuildStatus, ToolManifest, ToolManifestSchema, ToolOutput,
};
use serde::Deserialize;

use super::error::ToolCatalogError;
use super::hash::sha256_stable;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolBuildOptions {
    pub root: PathBuf,
    pub tool_path: Option<PathBuf>,
    pub all: bool,
    pub toolkit_version: String,
}

#[derive(Deserialize)]
struct RawToolManifest {
    #[serde(default)]
    name: String,
    version: Option<String>,
    description: Option<String>,
    source: runx_contracts::tools::ToolSource,
    #[serde(default)]
    inputs: BTreeMap<String, runx_contracts::tools::ToolInput>,
    output: Option<ToolOutput>,
    #[serde(default)]
    scopes: Vec<String>,
    risk: Option<JsonPayload>,
    runtime: Option<RuntimeCommand>,
    retry: Option<runx_contracts::tools::ToolRetryPolicy>,
    idempotency: Option<runx_contracts::tools::ToolIdempotencyPolicy>,
    mutating: Option<bool>,
    runx: Option<JsonPayloadObject>,
}

pub fn build_tool_catalogs(
    options: &ToolBuildOptions,
) -> Result<ToolBuildReport, ToolCatalogError> {
    let tool_dirs = if options.all {
        discover_tool_directories(&options.root)?
    } else {
        vec![resolve_tool_path(
            &options.root,
            options.tool_path.as_deref(),
        )?]
    };
    let mut built = Vec::new();
    let mut errors = Vec::new();
    for tool_dir in tool_dirs {
        match build_tool_manifest(&options.root, &tool_dir, &options.toolkit_version) {
            Ok(item) => built.push(item),
            Err(error) => errors.push(format!(
                "{}: {}",
                project_path(&options.root, &tool_dir),
                error.concise_message()
            )),
        }
    }
    Ok(ToolBuildReport {
        schema: ToolBuildReportSchema::V1,
        status: if errors.is_empty() {
            ToolBuildStatus::Success
        } else {
            ToolBuildStatus::Failure
        },
        built,
        errors,
    })
}

fn build_tool_manifest(
    root: &Path,
    tool_dir: &Path,
    toolkit_version: &str,
) -> Result<BuiltToolItem, ToolCatalogError> {
    let manifest_path = tool_dir.join("manifest.json");
    let source = fs::read_to_string(&manifest_path)
        .map_err(|error| ToolCatalogError::io("reading tool manifest", &manifest_path, error))?;
    let raw: RawToolManifest = serde_json::from_str(&source)
        .map_err(|error| ToolCatalogError::json("parsing tool manifest", &manifest_path, error))?;
    let raw_payload: JsonPayload = serde_json::from_str(&source)
        .map_err(|error| ToolCatalogError::json("parsing tool manifest", &manifest_path, error))?;
    let JsonPayload::Object(raw_object) = raw_payload else {
        return Err(ToolCatalogError::InvalidManifest {
            path: manifest_path,
            message: "manifest.json must be an object.".to_owned(),
        });
    };
    let output = raw
        .output
        .unwrap_or_else(|| normalize_tool_output(raw.runx.as_ref()));
    let source_hash = hash_tool_source(tool_dir)?;
    let schema_hash = schema_hash(&raw_object, &output);
    let manifest = ToolManifest {
        schema: ToolManifestSchema::V1,
        name: raw.name,
        version: raw.version,
        description: raw.description,
        source: raw.source,
        runtime: raw
            .runtime
            .unwrap_or_else(|| runtime_from_source(&raw_object)),
        inputs: raw.inputs,
        output,
        scopes: raw.scopes,
        risk: raw.risk,
        retry: raw.retry,
        idempotency: raw.idempotency,
        mutating: raw.mutating,
        runx: raw.runx,
        source_hash,
        schema_hash,
        toolkit_version: Some(toolkit_version.to_owned()),
    };
    validate_manifest(&manifest, &manifest_path)?;
    write_manifest(&manifest_path, &manifest)?;
    Ok(BuiltToolItem {
        path: project_path(root, tool_dir),
        manifest: project_path(root, &manifest_path),
        source_hash: manifest.source_hash,
        schema_hash: manifest.schema_hash,
    })
}

fn normalize_tool_output(runx: Option<&JsonPayloadObject>) -> ToolOutput {
    let artifacts = runx
        .and_then(|runx| runx.get("artifacts"))
        .and_then(|value| match value {
            JsonPayload::Object(value) => Some(value),
            _ => None,
        });
    let wrap_as = artifacts
        .and_then(|artifacts| artifacts.get("wrap_as"))
        .and_then(|value| match value {
            JsonPayload::String(value) => Some(value.clone()),
            _ => None,
        });
    let mut extra = JsonPayloadObject::new();
    if let Some(JsonPayload::Object(named_emits)) =
        artifacts.and_then(|artifacts| artifacts.get("named_emits"))
    {
        extra.insert(
            "named_emits".to_owned(),
            JsonPayload::Object(named_emits.clone()),
        );
    }
    ToolOutput {
        packet: None,
        wrap_as,
        named_emits: BTreeMap::new(),
        outputs: BTreeMap::new(),
        extra,
    }
}

fn runtime_from_source(raw_object: &JsonPayloadObject) -> RuntimeCommand {
    let source = raw_object.get("source").and_then(|value| match value {
        JsonPayload::Object(value) => Some(value),
        _ => None,
    });
    let command = source
        .and_then(|source| source.get("command"))
        .and_then(|value| match value {
            JsonPayload::String(value) => Some(value.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "node".to_owned());
    let args = source
        .and_then(|source| source.get("args"))
        .and_then(|value| match value {
            JsonPayload::Array(values) => Some(
                values
                    .iter()
                    .filter_map(|value| match value {
                        JsonPayload::String(value) => Some(value.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .filter(|args| !args.is_empty())
        .unwrap_or_else(|| vec!["./run.mjs".to_owned()]);
    RuntimeCommand {
        command,
        args,
        cwd: None,
        env: BTreeMap::new(),
    }
}

fn schema_hash(raw: &JsonPayloadObject, output: &ToolOutput) -> String {
    let mut payload = JsonPayloadObject::new();
    if let Some(inputs) = raw.get("inputs") {
        payload.insert("inputs".to_owned(), inputs.clone());
    }
    payload.insert("output".to_owned(), tool_output_payload(output));
    if let Some(artifacts) = raw.get("runx").and_then(|value| match value {
        JsonPayload::Object(value) => value.get("artifacts"),
        _ => None,
    }) {
        payload.insert("artifacts".to_owned(), artifacts.clone());
    }
    sha256_stable(&JsonPayload::Object(payload))
}

fn tool_output_payload(output: &ToolOutput) -> JsonPayload {
    let mut object = output.extra.clone();
    if let Some(packet) = &output.packet {
        object.insert("packet".to_owned(), JsonPayload::String(packet.clone()));
    }
    if let Some(wrap_as) = &output.wrap_as {
        object.insert("wrap_as".to_owned(), JsonPayload::String(wrap_as.clone()));
    }
    if !output.named_emits.is_empty() {
        let mut named = JsonPayloadObject::new();
        for (label, key) in &output.named_emits {
            named.insert(label.clone(), JsonPayload::String(key.clone()));
        }
        object.insert("named_emits".to_owned(), JsonPayload::Object(named));
    }
    if !output.outputs.is_empty() {
        let mut outputs = JsonPayloadObject::new();
        for (name, binding) in &output.outputs {
            outputs.insert(name.clone(), tool_output_binding_payload(binding));
        }
        object.insert("outputs".to_owned(), JsonPayload::Object(outputs));
    }
    JsonPayload::Object(object)
}

fn tool_output_binding_payload(
    binding: &runx_contracts::tools::ToolOutputBinding,
) -> JsonPayload {
    let mut object = binding.extra.clone();
    if let Some(packet) = &binding.packet {
        object.insert("packet".to_owned(), JsonPayload::String(packet.clone()));
    }
    if let Some(wrap_as) = &binding.wrap_as {
        object.insert("wrap_as".to_owned(), JsonPayload::String(wrap_as.clone()));
    }
    JsonPayload::Object(object)
}

fn hash_tool_source(tool_dir: &Path) -> Result<String, ToolCatalogError> {
    let candidates = [tool_dir.join("src/index.ts"), tool_dir.join("run.mjs")];
    let mut found = false;
    let mut bytes = Vec::new();
    for candidate in candidates {
        if !candidate.exists() {
            continue;
        }
        found = true;
        bytes.extend(project_path(tool_dir, &candidate).as_bytes());
        bytes.push(0);
        let mut file = fs::File::open(&candidate)
            .map_err(|error| ToolCatalogError::io("reading tool source", &candidate, error))?;
        file.read_to_end(&mut bytes)
            .map_err(|error| ToolCatalogError::io("reading tool source", &candidate, error))?;
        bytes.push(0);
    }
    if !found {
        bytes.extend(b"no-source");
    }
    Ok(sha256_prefixed(&bytes))
}

fn validate_manifest(manifest: &ToolManifest, path: &Path) -> Result<(), ToolCatalogError> {
    let json = serde_json::to_string(manifest)
        .map_err(|error| ToolCatalogError::json("serializing tool manifest", path, error))?;
    let raw = runx_parser::parse_tool_manifest_json(&json).map_err(|error| {
        ToolCatalogError::InvalidManifest {
            path: path.to_path_buf(),
            message: error.to_string(),
        }
    })?;
    runx_parser::validate_tool_manifest(raw).map_err(|error| {
        ToolCatalogError::InvalidManifest {
            path: path.to_path_buf(),
            message: error.to_string(),
        }
    })?;
    Ok(())
}

fn write_manifest(path: &Path, manifest: &ToolManifest) -> Result<(), ToolCatalogError> {
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|error| ToolCatalogError::json("serializing tool manifest", path, error))?;
    fs::write(path, format!("{json}\n"))
        .map_err(|error| ToolCatalogError::io("writing tool manifest", path, error))
}

fn discover_tool_directories(root: &Path) -> Result<Vec<PathBuf>, ToolCatalogError> {
    let tools_root = root.join("tools");
    let mut directories = Vec::new();
    for namespace in read_dirs(&tools_root)? {
        for tool in read_dirs(&namespace)? {
            directories.push(tool);
        }
    }
    directories.sort();
    Ok(directories)
}

fn read_dirs(path: &Path) -> Result<Vec<PathBuf>, ToolCatalogError> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(ToolCatalogError::io("reading directory", path, error)),
    };
    let mut dirs = Vec::new();
    for entry in entries {
        let entry =
            entry.map_err(|error| ToolCatalogError::io("reading directory", path, error))?;
        let file_type = entry.file_type().map_err(|error| {
            ToolCatalogError::io("reading directory entry", entry.path(), error)
        })?;
        if file_type.is_dir() {
            dirs.push(entry.path());
        }
    }
    dirs.sort();
    Ok(dirs)
}

fn resolve_tool_path(root: &Path, tool_path: Option<&Path>) -> Result<PathBuf, ToolCatalogError> {
    let Some(tool_path) = tool_path else {
        return Err(ToolCatalogError::InvalidRequest(
            "runx tool build requires a tool directory or --all".to_owned(),
        ));
    };
    if tool_path.is_absolute() {
        Ok(tool_path.to_path_buf())
    } else {
        Ok(root.join(tool_path))
    }
}

pub(crate) fn project_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map_or(path, |path| path)
        .to_string_lossy()
        .replace('\\', "/")
}
