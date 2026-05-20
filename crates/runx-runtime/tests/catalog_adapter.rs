#![cfg(feature = "catalog")]

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use runx_contracts::{JsonObject, JsonValue};
use runx_parser::SkillSource;
use runx_runtime::adapters::catalog::CatalogAdapter;
use runx_runtime::{InvocationStatus, RuntimeError, SkillAdapter, SkillInvocation};

#[test]
fn catalog_adapter_reports_missing_catalog_ref_as_user_failure() -> Result<(), RuntimeError> {
    let output = CatalogAdapter::fixture_catalog().invoke(invocation(None, JsonObject::new()))?;

    assert_eq!(output.status, InvocationStatus::Failure);
    assert_eq!(output.stdout, "");
    assert_eq!(
        output.stderr,
        "Catalog source requires source.catalog_ref metadata."
    );
    assert_eq!(output.exit_code, None);
    assert!(output.metadata.is_empty());
    Ok(())
}

#[test]
fn catalog_adapter_reports_missing_imported_tool_as_user_failure() -> Result<(), RuntimeError> {
    let output = CatalogAdapter::fixture_catalog().invoke(invocation(
        Some("fixture-mcp:fixture.nope"),
        JsonObject::new(),
    ))?;

    assert_eq!(output.status, InvocationStatus::Failure);
    assert_eq!(
        output.stderr,
        "Imported tool 'fixture-mcp:fixture.nope' was not found in configured tool catalogs."
    );
    assert_eq!(output.exit_code, None);
    assert!(output.metadata.is_empty());
    Ok(())
}

#[test]
fn catalog_adapter_invokes_fixture_echo_tool() -> Result<(), RuntimeError> {
    let mut inputs = JsonObject::new();
    inputs.insert("message".to_owned(), JsonValue::String("hello".to_owned()));

    let output =
        CatalogAdapter::fixture_catalog().invoke(invocation(Some("fixture.echo"), inputs))?;

    assert_eq!(output.status, InvocationStatus::Success);
    assert_eq!(output.stdout, "hello");
    assert_eq!(output.stderr, "");
    assert_eq!(output.exit_code, Some(0));
    assert_eq!(
        output.metadata.get("mcp"),
        Some(&JsonValue::Object(expected_mcp_metadata("echo")))
    );
    Ok(())
}

#[test]
fn catalog_adapter_propagates_fixture_failure() -> Result<(), RuntimeError> {
    let mut inputs = JsonObject::new();
    inputs.insert("message".to_owned(), JsonValue::String("boom".to_owned()));

    let output =
        CatalogAdapter::fixture_catalog().invoke(invocation(Some("fixture.fail"), inputs))?;

    assert_eq!(output.status, InvocationStatus::Failure);
    assert_eq!(output.stdout, "");
    assert_eq!(output.stderr, "MCP error -32000: fixture failure: boom");
    assert_eq!(output.exit_code, None);
    assert_eq!(
        output.metadata.get("mcp"),
        Some(&JsonValue::Object(expected_mcp_metadata("fail")))
    );
    Ok(())
}

#[test]
fn catalog_adapter_keeps_fixture_catalog_opt_in() -> Result<(), RuntimeError> {
    let output = CatalogAdapter::default().invoke(invocation(
        Some("fixture-mcp:fixture.echo"),
        JsonObject::new(),
    ))?;

    assert_eq!(output.status, InvocationStatus::Failure);
    assert_eq!(
        output.stderr,
        "Imported tool 'fixture-mcp:fixture.echo' was not found in configured tool catalogs."
    );
    assert!(output.metadata.is_empty());
    Ok(())
}

#[test]
fn catalog_adapter_prefers_local_manifest_before_fixture_catalog()
-> Result<(), Box<dyn std::error::Error>> {
    let case_dir = repo_root()?.join("fixtures/runtime/adapters/catalog/local-precedence");
    let mut inputs = JsonObject::new();
    inputs.insert(
        "message".to_owned(),
        JsonValue::String("catalog fixture collision".to_owned()),
    );

    let output = CatalogAdapter::fixture_catalog().invoke(invocation_in_directory(
        Some("fixture.echo"),
        inputs,
        case_dir,
        process_env(),
    ))?;

    assert_eq!(output.status, InvocationStatus::Success);
    assert_eq!(output.stdout, oracle_text("local-precedence", "stdout")?);
    assert_eq!(output.stderr, oracle_text("local-precedence", "stderr")?);
    assert_eq!(oracle_text("local-precedence", "status")?, "sealed\n");
    Ok(())
}

fn invocation(catalog_ref: Option<&str>, inputs: JsonObject) -> SkillInvocation {
    invocation_in_directory(catalog_ref, inputs, PathBuf::from("."), BTreeMap::new())
}

fn invocation_in_directory(
    catalog_ref: Option<&str>,
    inputs: JsonObject,
    skill_directory: PathBuf,
    env: BTreeMap<String, String>,
) -> SkillInvocation {
    SkillInvocation {
        skill_name: "fixture.catalog".to_owned(),
        source: SkillSource {
            source_type: "catalog".to_owned(),
            command: None,
            args: Vec::new(),
            cwd: None,
            timeout_seconds: None,
            input_mode: None,
            sandbox: None,
            server: None,
            catalog_ref: catalog_ref.map(str::to_owned),
            tool: None,
            arguments: None,
            agent_card_url: None,
            agent_identity: None,
            agent: None,
            task: None,
            hook: None,
            outputs: None,
            graph: None,
            raw: JsonObject::new(),
        },
        inputs,
        resolved_inputs: JsonObject::new(),
        skill_directory,
        env,
    }
}

fn expected_mcp_metadata(tool_name: &str) -> JsonObject {
    let mut mcp = JsonObject::new();
    mcp.insert("tool".to_owned(), JsonValue::String(tool_name.to_owned()));
    mcp.insert(
        "server_args_hash".to_owned(),
        JsonValue::String(
            "d4ae1dfdf0cefbd9a703697ec29358f080df41c1289657e5be139ce8952979b3".to_owned(),
        ),
    );
    mcp.insert(
        "server_command_hash".to_owned(),
        JsonValue::String(
            "545ea538461003efdc8c81c244531b003f6f26cfccf6c0073b3239fdedf49446".to_owned(),
        ),
    );
    mcp
}

fn repo_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?)
}

fn oracle_text(case_name: &str, extension: &str) -> Result<String, Box<dyn std::error::Error>> {
    let path = repo_root()?.join(format!(
        "fixtures/runtime/adapters/catalog/oracles/{case_name}.{extension}"
    ));
    Ok(fs::read_to_string(path)?)
}

fn process_env() -> BTreeMap<String, String> {
    [
        "PATH",
        "HOME",
        "TMPDIR",
        "TMP",
        "TEMP",
        "SystemRoot",
        "WINDIR",
        "COMSPEC",
        "PATHEXT",
    ]
    .into_iter()
    .filter_map(|key| std::env::var(key).ok().map(|value| (key.to_owned(), value)))
    .collect()
}
