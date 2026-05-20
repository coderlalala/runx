#![cfg(feature = "mcp")]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use runx_contracts::{JsonNumber, JsonObject, JsonValue};
use runx_parser::{SkillMcpServer, SkillSource};
use runx_runtime::adapters::mcp::{
    McpAdapter, McpToolCallRequest, McpTransport, McpTransportError, ProcessMcpTransport,
    map_mcp_arguments,
};
use runx_runtime::{InvocationStatus, RuntimeError, SkillAdapter, SkillInvocation};

#[test]
fn mcp_argument_templates_map_structured_and_embedded_values() -> Result<(), RuntimeError> {
    let mut inputs = JsonObject::new();
    inputs.insert("name".to_owned(), JsonValue::String("Ada".to_owned()));
    inputs.insert("count".to_owned(), JsonValue::Number(JsonNumber::U64(3)));

    let mut nested = JsonObject::new();
    nested.insert("ok".to_owned(), JsonValue::Bool(true));

    let mut resolved_inputs = JsonObject::new();
    resolved_inputs.insert("payload".to_owned(), JsonValue::Object(nested.clone()));

    let mut template = JsonObject::new();
    template.insert(
        "exact".to_owned(),
        JsonValue::String("{{ payload }}".to_owned()),
    );
    template.insert(
        "embedded".to_owned(),
        JsonValue::String("hello {{name}} #{{ count }}".to_owned()),
    );
    template.insert(
        "invalid".to_owned(),
        JsonValue::String("keep {{ not valid }}".to_owned()),
    );

    let mapped = map_mcp_arguments(Some(&template), &inputs, &resolved_inputs)?;

    assert_eq!(mapped.get("exact"), Some(&JsonValue::Object(nested)));
    assert_eq!(
        mapped.get("embedded"),
        Some(&JsonValue::String("hello Ada #3".to_owned()))
    );
    assert_eq!(
        mapped.get("invalid"),
        Some(&JsonValue::String("keep {{ not valid }}".to_owned()))
    );
    Ok(())
}

#[test]
fn mcp_adapter_clamps_min_timeout_and_sanitizes_tool_error() -> Result<(), RuntimeError> {
    let seen = Arc::new(Mutex::new(None));
    let adapter = McpAdapter::new(TimeoutProbeTransport {
        seen: Arc::clone(&seen),
    });
    let mut inputs = JsonObject::new();
    inputs.insert(
        "secret".to_owned(),
        JsonValue::String("sk-live-do-not-leak".to_owned()),
    );

    let output = adapter.invoke(invocation("fail", Some(0), inputs))?;

    assert_eq!(output.status, InvocationStatus::Failure);
    assert_eq!(output.stderr, "MCP tool returned error -32000.");
    assert!(!output.stderr.contains("sk-live-do-not-leak"));
    assert_eq!(
        *seen.lock().expect("timeout probe poisoned"),
        Some(Duration::from_millis(50))
    );
    Ok(())
}

#[test]
fn mcp_adapter_malformed_json_response_is_sanitized() -> Result<(), RuntimeError> {
    let adapter = McpAdapter::new(ProcessMcpTransport);
    let mut inputs = JsonObject::new();
    inputs.insert(
        "secret".to_owned(),
        JsonValue::String("malformed-json-secret".to_owned()),
    );
    let mut request = invocation("malformed-json", Some(1), inputs);
    let Some(server) = request.source.server.as_mut() else {
        unreachable!("test invocation always includes MCP server metadata");
    };
    server.command = "/bin/sh".to_owned();
    server.args = vec![
        "-c".to_owned(),
        "IFS= read -r _ || true; printf 'Content-Length: 1\\r\\n\\r\\n{'; sleep 1".to_owned(),
    ];

    let output = adapter.invoke(request)?;

    assert_eq!(output.status, InvocationStatus::Failure);
    assert_eq!(output.stderr, "MCP adapter failed.");
    assert!(!output.stderr.contains("malformed-json-secret"));
    assert!(output.stdout.is_empty());
    assert_eq!(output.exit_code, None);
    Ok(())
}

#[derive(Clone, Debug)]
struct TimeoutProbeTransport {
    seen: Arc<Mutex<Option<Duration>>>,
}

impl McpTransport for TimeoutProbeTransport {
    fn call_tool(&self, request: McpToolCallRequest) -> Result<JsonValue, McpTransportError> {
        assert_eq!(request.tool, "fail");
        assert_eq!(
            request.arguments.get("secret"),
            Some(&JsonValue::String("sk-live-do-not-leak".to_owned()))
        );
        *self.seen.lock().expect("timeout probe poisoned") = Some(request.timeout);
        Err(McpTransportError::tool_error(
            -32000,
            "provider failure: sk-live-do-not-leak",
        ))
    }
}

fn invocation(tool: &str, timeout_seconds: Option<u64>, inputs: JsonObject) -> SkillInvocation {
    SkillInvocation {
        skill_name: "fixture.mcp".to_owned(),
        source: SkillSource {
            source_type: "mcp".to_owned(),
            command: None,
            args: Vec::new(),
            cwd: None,
            timeout_seconds,
            input_mode: None,
            sandbox: None,
            server: Some(SkillMcpServer {
                command: "/bin/echo".to_owned(),
                args: Vec::new(),
                cwd: None,
            }),
            catalog_ref: None,
            tool: Some(tool.to_owned()),
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
        skill_directory: std::env::current_dir().expect("current directory is available"),
        env: BTreeMap::new(),
    }
}
