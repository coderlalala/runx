// rust-style-allow: large-file because the client-side transport keeps stdio
// framing, response buffering, and bounded read/write helpers adjacent to the
// transport implementations they coordinate.
#[cfg(feature = "mcp-rmcp")]
use std::future::Future;
#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
use std::io::{Read, Write};
#[cfg(any(feature = "mcp", feature = "mcp-rmcp"))]
use std::process::Stdio;
#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
use std::process::{Child, ChildStdin, Command};
#[cfg(feature = "mcp-rmcp")]
use std::sync::Arc;
#[cfg(feature = "mcp-rmcp")]
use std::sync::Mutex;
#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
use std::thread;
use std::time::Duration;
#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
use std::time::Instant;

#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
use runx_contracts::JsonNumber;
use runx_contracts::{JsonObject, JsonValue};
#[cfg(feature = "mcp-rmcp")]
use serde_json::{self, Value as JsonWireValue};

use crate::credentials::SecretEnv;
use crate::sandbox::SandboxPlan;

#[cfg(any(feature = "mcp", feature = "mcp-rmcp"))]
use super::framing::{content_length, find_header_end};
use super::jsonrpc::text_content;
#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
use super::jsonrpc::{
    initialize_request, initialized_notification, parse_mcp_tools_list, tool_call_request,
    tools_list_request,
};
use super::templates::js_string;
use super::types::{
    McpListToolsRequest, McpToolCallRequest, McpToolDescriptor, McpTransport, McpTransportError,
};

#[cfg(any(feature = "mcp", feature = "mcp-rmcp"))]
const MAX_CLIENT_RESPONSE_BYTES: usize = 1024 * 1024;
#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Clone, Copy, Debug, Default)]
pub struct FixtureMcpTransport;

impl FixtureMcpTransport {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl McpTransport for FixtureMcpTransport {
    fn call_tool(&self, request: McpToolCallRequest) -> Result<JsonValue, McpTransportError> {
        match request.tool.as_str() {
            "echo" => Ok(text_content(js_string(request.arguments.get("message")))),
            "env" => Ok(text_content(mcp_env_value(&request))),
            "fail" => Err(McpTransportError::tool_error(
                -32000,
                format!(
                    "fixture failure: {}",
                    js_string(request.arguments.get("message"))
                ),
            )),
            "sleep" => Err(McpTransportError::timeout(request.timeout)),
            "malformed-json" => Err(McpTransportError::failed("MCP server sent invalid JSON.")),
            _ => Err(McpTransportError::tool_error(-32601, "tool not found")),
        }
    }
}

fn mcp_env_value(request: &McpToolCallRequest) -> String {
    let name = js_string(request.arguments.get("name"));
    request
        .sandbox
        .env
        .get(&name)
        .cloned()
        .or_else(|| request.secret_env.get(&name).map(str::to_owned))
        .unwrap_or_default()
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessMcpTransport;

impl ProcessMcpTransport {
    pub fn list_tools(
        &self,
        request: McpListToolsRequest,
    ) -> Result<Vec<McpToolDescriptor>, McpTransportError> {
        #[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
        {
            let mut client =
                initialize_mcp_client(&request.sandbox, &SecretEnv::default(), request.timeout)?;
            let result = client.request(2, &tools_list_request(2))?;
            Ok(parse_mcp_tools_list(result))
        }
        #[cfg(all(feature = "mcp-rmcp", not(feature = "mcp")))]
        {
            list_tools_with_rmcp(request)
        }
        #[cfg(all(feature = "mcp", feature = "mcp-rmcp"))]
        {
            let _ = &request;
            Err(McpTransportError::failed(
                "features `mcp` and `mcp-rmcp` are mutually exclusive",
            ))
        }
    }
}

impl McpTransport for ProcessMcpTransport {
    fn call_tool(&self, request: McpToolCallRequest) -> Result<JsonValue, McpTransportError> {
        #[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
        {
            let mut client =
                initialize_mcp_client(&request.sandbox, &request.secret_env, request.timeout)?;
            client.request(2, &tool_call_request(2, &request.tool, &request.arguments))
        }
        #[cfg(all(feature = "mcp-rmcp", not(feature = "mcp")))]
        {
            call_tool_with_rmcp(request)
        }
        #[cfg(all(feature = "mcp", feature = "mcp-rmcp"))]
        {
            let _ = &request;
            Err(McpTransportError::failed(
                "features `mcp` and `mcp-rmcp` are mutually exclusive",
            ))
        }
    }
}

#[cfg(feature = "mcp-rmcp")]
fn list_tools_with_rmcp(
    request: McpListToolsRequest,
) -> Result<Vec<McpToolDescriptor>, McpTransportError> {
    block_on_rmcp(list_tools_with_rmcp_async(request))
}

#[cfg(feature = "mcp-rmcp")]
fn call_tool_with_rmcp(request: McpToolCallRequest) -> Result<JsonValue, McpTransportError> {
    block_on_rmcp(call_tool_with_rmcp_async(request))
}

#[cfg(feature = "mcp-rmcp")]
fn block_on_rmcp<T>(
    future: impl Future<Output = Result<T, McpTransportError>>,
) -> Result<T, McpTransportError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(|_| McpTransportError::failed("MCP client runtime initialization failed."))?
        .block_on(future)
}

#[cfg(feature = "mcp-rmcp")]
async fn list_tools_with_rmcp_async(
    request: McpListToolsRequest,
) -> Result<Vec<McpToolDescriptor>, McpTransportError> {
    let mut child = spawn_tokio_mcp_server(&request.sandbox, &SecretEnv::default())?;
    drain_tokio_stderr(child.stderr.take());
    let result = tokio::time::timeout(request.timeout, async {
        let error_state = RmcpTransportErrorState::default();
        let mut service = serve_rmcp_client(&mut child, error_state.clone()).await?;
        let tools = service
            .peer()
            .list_all_tools()
            .await
            .map_err(|error| rmcp_service_error(error, &error_state))?;
        let _closed = service.close_with_timeout(Duration::from_millis(100)).await;
        tools
            .into_iter()
            .map(mcp_tool_descriptor_from_rmcp)
            .collect::<Result<Vec<_>, _>>()
    })
    .await;
    terminate_tokio_child(&mut child).await;
    match result {
        Ok(result) => result,
        Err(_) => Err(McpTransportError::timeout(request.timeout)),
    }
}

#[cfg(feature = "mcp-rmcp")]
async fn call_tool_with_rmcp_async(
    request: McpToolCallRequest,
) -> Result<JsonValue, McpTransportError> {
    let mut child = spawn_tokio_mcp_server(&request.sandbox, &request.secret_env)?;
    drain_tokio_stderr(child.stderr.take());
    let timeout = request.timeout;
    let result = tokio::time::timeout(timeout, async {
        let error_state = RmcpTransportErrorState::default();
        let mut service = serve_rmcp_client(&mut child, error_state.clone()).await?;
        let arguments = rmcp_json_object(request.arguments)?;
        let result = service
            .peer()
            .call_tool(
                rmcp::model::CallToolRequestParams::new(request.tool).with_arguments(arguments),
            )
            .await
            .map_err(|error| rmcp_service_error(error, &error_state))?;
        let _closed = service.close_with_timeout(Duration::from_millis(100)).await;
        rmcp_call_tool_result_json(result)
    })
    .await;
    terminate_tokio_child(&mut child).await;
    match result {
        Ok(result) => result,
        Err(_) => Err(McpTransportError::timeout(timeout)),
    }
}

#[cfg(feature = "mcp-rmcp")]
async fn serve_rmcp_client(
    child: &mut tokio::process::Child,
    error_state: RmcpTransportErrorState,
) -> Result<
    rmcp::service::RunningService<rmcp::RoleClient, rmcp::model::ClientInfo>,
    McpTransportError,
> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| McpTransportError::failed("MCP server stdout unavailable."))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| McpTransportError::failed("MCP server stdin unavailable."))?;
    let transport = RmcpContentLengthTransport::new(stdout, stdin, error_state.clone());
    serve_rmcp_transport(transport, &error_state).await
}

#[cfg(feature = "mcp-rmcp")]
async fn serve_rmcp_transport<T, E>(
    transport: T,
    error_state: &RmcpTransportErrorState,
) -> Result<
    rmcp::service::RunningService<rmcp::RoleClient, rmcp::model::ClientInfo>,
    McpTransportError,
>
where
    T: rmcp::transport::Transport<rmcp::RoleClient, Error = E> + Send + 'static,
    E: std::error::Error + Send + Sync + 'static,
{
    rmcp::serve_client(rmcp::model::ClientInfo::default(), transport)
        .await
        .map_err(|error| rmcp_initialization_error(error, error_state))
}

#[cfg(feature = "mcp-rmcp")]
struct RmcpContentLengthTransport<R, W> {
    read: R,
    write: Arc<tokio::sync::Mutex<W>>,
    buffer: Vec<u8>,
    error_state: RmcpTransportErrorState,
}

#[cfg(feature = "mcp-rmcp")]
#[derive(Clone, Default)]
struct RmcpTransportErrorState {
    message: Arc<Mutex<Option<String>>>,
}

#[cfg(feature = "mcp-rmcp")]
impl RmcpTransportErrorState {
    fn record(&self, error: std::io::Error) {
        if let Ok(mut message) = self.message.lock() {
            *message = Some(error.to_string());
        }
    }

    fn take(&self) -> Option<String> {
        self.message
            .lock()
            .ok()
            .and_then(|mut message| message.take())
    }
}

#[cfg(feature = "mcp-rmcp")]
impl<R, W> RmcpContentLengthTransport<R, W> {
    fn new(read: R, write: W, error_state: RmcpTransportErrorState) -> Self {
        Self {
            read,
            write: Arc::new(tokio::sync::Mutex::new(write)),
            buffer: Vec::new(),
            error_state,
        }
    }
}

#[cfg(feature = "mcp-rmcp")]
impl<R, W> rmcp::transport::Transport<rmcp::RoleClient> for RmcpContentLengthTransport<R, W>
where
    R: tokio::io::AsyncRead + Send + Unpin + 'static,
    W: tokio::io::AsyncWrite + Send + Unpin + 'static,
{
    type Error = std::io::Error;

    fn send(
        &mut self,
        item: rmcp::service::TxJsonRpcMessage<rmcp::RoleClient>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        let write = Arc::clone(&self.write);
        async move {
            let body = serde_json::to_vec(&item).map_err(std::io::Error::other)?;
            let header = format!("Content-Length: {}\r\n\r\n", body.len());
            let mut write = write.lock().await;
            tokio::io::AsyncWriteExt::write_all(&mut *write, header.as_bytes()).await?;
            tokio::io::AsyncWriteExt::write_all(&mut *write, &body).await?;
            tokio::io::AsyncWriteExt::flush(&mut *write).await
        }
    }

    async fn receive(&mut self) -> Option<rmcp::service::RxJsonRpcMessage<rmcp::RoleClient>> {
        match next_rmcp_framed_message(&mut self.read, &mut self.buffer).await {
            Ok(Some(message)) => Some(message),
            Ok(None) => None,
            Err(error) => {
                self.error_state.record(error);
                None
            }
        }
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        let mut write = self.write.lock().await;
        tokio::io::AsyncWriteExt::shutdown(&mut *write).await
    }
}

#[cfg(feature = "mcp-rmcp")]
async fn next_rmcp_framed_message<R>(
    read: &mut R,
    buffer: &mut Vec<u8>,
) -> Result<Option<rmcp::service::RxJsonRpcMessage<rmcp::RoleClient>>, std::io::Error>
where
    R: tokio::io::AsyncRead + Unpin,
{
    loop {
        if let Some(message) = parse_next_rmcp_framed_message(buffer)? {
            return Ok(Some(message));
        }
        if buffer.len() > MAX_CLIENT_RESPONSE_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "MCP server response exceeded size limit.",
            ));
        }
        let mut chunk = [0_u8; 8192];
        let read = tokio::io::AsyncReadExt::read(read, &mut chunk).await?;
        if read == 0 {
            return Ok(None);
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
}

#[cfg(feature = "mcp-rmcp")]
fn parse_next_rmcp_framed_message(
    buffer: &mut Vec<u8>,
) -> Result<Option<rmcp::service::RxJsonRpcMessage<rmcp::RoleClient>>, std::io::Error> {
    let Some(header_end) = find_header_end(buffer) else {
        return Ok(None);
    };
    if header_end > MAX_CLIENT_RESPONSE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "MCP server response exceeded size limit.",
        ));
    }
    let header = std::str::from_utf8(&buffer[..header_end])
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    let content_length = content_length(header).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "MCP server sent a response without Content-Length.",
        )
    })?;
    if content_length > MAX_CLIENT_RESPONSE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "MCP server response exceeded size limit.",
        ));
    }
    let body_start = header_end + 4;
    let body_end = body_start.saturating_add(content_length);
    if buffer.len() < body_end {
        return Ok(None);
    }
    let body = buffer[body_start..body_end].to_vec();
    buffer.drain(..body_end);
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

#[cfg(feature = "mcp-rmcp")]
fn spawn_tokio_mcp_server(
    plan: &SandboxPlan,
    secret_env: &SecretEnv,
) -> Result<tokio::process::Child, McpTransportError> {
    let mut command = tokio::process::Command::new(&plan.command);
    command
        .args(&plan.args)
        .current_dir(&plan.cwd)
        .env_clear()
        .envs(&plan.env)
        .envs(secret_env.iter())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
        .spawn()
        .map_err(|_| McpTransportError::failed("MCP server failed to spawn."))
}

#[cfg(feature = "mcp-rmcp")]
async fn terminate_tokio_child(child: &mut tokio::process::Child) {
    let _ = child.start_kill();
    let _ = child.wait().await;
}

#[cfg(feature = "mcp-rmcp")]
fn drain_tokio_stderr(stderr: Option<tokio::process::ChildStderr>) {
    if let Some(mut stderr) = stderr {
        tokio::spawn(async move {
            let mut sink = [0_u8; 8192];
            let mut read_total = 0_usize;
            while read_total < MAX_CLIENT_RESPONSE_BYTES {
                match tokio::io::AsyncReadExt::read(&mut stderr, &mut sink).await {
                    Ok(0) | Err(_) => return,
                    Ok(read) => read_total = read_total.saturating_add(read),
                }
            }
        });
    }
}

#[cfg(feature = "mcp-rmcp")]
fn mcp_tool_descriptor_from_rmcp(
    tool: rmcp::model::Tool,
) -> Result<McpToolDescriptor, McpTransportError> {
    Ok(McpToolDescriptor {
        name: tool.name.into_owned(),
        description: tool.description.map(std::borrow::Cow::into_owned),
        input_schema: Some(runx_json_object(JsonWireValue::Object(
            (*tool.input_schema).clone(),
        ))?),
    })
}

#[cfg(feature = "mcp-rmcp")]
fn rmcp_call_tool_result_json(
    result: rmcp::model::CallToolResult,
) -> Result<JsonValue, McpTransportError> {
    let value = serde_json::to_value(result)
        .map_err(|_| McpTransportError::failed("MCP response serialization failed."))?;
    serde_json::from_value(value)
        .map_err(|_| McpTransportError::failed("MCP response conversion failed."))
}

#[cfg(feature = "mcp-rmcp")]
fn rmcp_json_object(value: JsonObject) -> Result<rmcp::model::JsonObject, McpTransportError> {
    match serde_json::to_value(value)
        .map_err(|_| McpTransportError::failed("MCP request conversion failed."))?
    {
        JsonWireValue::Object(record) => Ok(record),
        _ => Err(McpTransportError::failed("MCP request conversion failed.")),
    }
}

#[cfg(feature = "mcp-rmcp")]
fn runx_json_object(value: JsonWireValue) -> Result<JsonObject, McpTransportError> {
    serde_json::from_value(value)
        .map_err(|_| McpTransportError::failed("MCP response conversion failed."))
}

#[cfg(feature = "mcp-rmcp")]
fn rmcp_service_error(
    error: rmcp::ServiceError,
    error_state: &RmcpTransportErrorState,
) -> McpTransportError {
    if let Some(message) = error_state.take() {
        return McpTransportError::failed(message);
    }
    match error {
        rmcp::ServiceError::McpError(error) => {
            McpTransportError::tool_error(i64::from(error.code.0), "MCP server returned error.")
        }
        _ => McpTransportError::failed("MCP server request failed."),
    }
}

#[cfg(feature = "mcp-rmcp")]
fn rmcp_initialization_error(
    _error: rmcp::service::ClientInitializeError,
    error_state: &RmcpTransportErrorState,
) -> McpTransportError {
    if let Some(message) = error_state.take() {
        return McpTransportError::failed(message);
    }
    McpTransportError::failed("MCP client initialization failed.")
}

#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
fn spawn_mcp_server(
    plan: &SandboxPlan,
    secret_env: &SecretEnv,
) -> Result<Child, McpTransportError> {
    Command::new(&plan.command)
        .args(&plan.args)
        .current_dir(&plan.cwd)
        .env_clear()
        .envs(&plan.env)
        .envs(secret_env.iter())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| McpTransportError::failed("MCP server failed to spawn."))
}

#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
struct InitializedMcpClient {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Result<JsonValue, McpTransportError>>,
    deadline: Instant,
    timeout: Duration,
}

#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
impl InitializedMcpClient {
    fn request(&mut self, id: i64, message: &JsonValue) -> Result<JsonValue, McpTransportError> {
        write_message(&mut self.stdin, message)?;
        wait_for_response(&mut self.child, &self.rx, id, self.deadline, self.timeout)
    }

    fn notify(&mut self, message: &JsonValue) -> Result<(), McpTransportError> {
        write_message(&mut self.stdin, message)
    }
}

#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
impl Drop for InitializedMcpClient {
    fn drop(&mut self) {
        terminate_child(&mut self.child);
    }
}

#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
fn initialize_mcp_client(
    sandbox: &SandboxPlan,
    secret_env: &SecretEnv,
    timeout: Duration,
) -> Result<InitializedMcpClient, McpTransportError> {
    let mut child = spawn_mcp_server(sandbox, secret_env)?;
    let Some(stdin) = child.stdin.take() else {
        terminate_child(&mut child);
        return Err(McpTransportError::failed("MCP server stdin unavailable."));
    };
    let Some(stdout) = child.stdout.take() else {
        terminate_child(&mut child);
        return Err(McpTransportError::failed("MCP server stdout unavailable."));
    };
    drain_stderr(child.stderr.take());
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || read_stdout_frames(stdout, tx));
    let mut client = InitializedMcpClient {
        child,
        stdin,
        rx,
        deadline: Instant::now() + timeout,
        timeout,
    };
    client.request(1, &initialize_request(1))?;
    client.notify(&initialized_notification())?;
    Ok(client)
}

#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
fn write_message(stdin: &mut impl Write, message: &JsonValue) -> Result<(), McpTransportError> {
    let body = serde_json::to_vec(message)
        .map_err(|_| McpTransportError::failed("MCP request serialization failed."))?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    stdin
        .write_all(header.as_bytes())
        .and_then(|()| stdin.write_all(&body))
        .map_err(|_| McpTransportError::failed("MCP server stdin write failed."))
}

#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
fn wait_for_response(
    child: &mut Child,
    rx: &Receiver<Result<JsonValue, McpTransportError>>,
    id: i64,
    deadline: Instant,
    timeout: Duration,
) -> Result<JsonValue, McpTransportError> {
    loop {
        let now = Instant::now();
        if now >= deadline {
            terminate_child(child);
            return Err(McpTransportError::timeout(timeout));
        }
        let remaining = deadline.saturating_duration_since(now);
        match rx.recv_timeout(POLL_INTERVAL.min(remaining)) {
            Ok(Ok(message)) => {
                if response_id(&message) != Some(id) {
                    continue;
                }
                return response_result(message);
            }
            Ok(Err(error)) => return Err(error),
            Err(RecvTimeoutError::Timeout) => {
                if process_exited(child)? {
                    return Err(McpTransportError::failed(
                        "MCP server exited before responding.",
                    ));
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(McpTransportError::failed(
                    "MCP server exited before responding.",
                ));
            }
        }
    }
}

#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
fn process_exited(child: &mut Child) -> Result<bool, McpTransportError> {
    child
        .try_wait()
        .map(|status| status.is_some())
        .map_err(|_| McpTransportError::failed("MCP server status check failed."))
}

#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
fn read_stdout_frames(mut stdout: impl Read, tx: Sender<Result<JsonValue, McpTransportError>>) {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        match stdout.read(&mut chunk) {
            Ok(0) => return,
            Ok(read) => {
                buffer.extend_from_slice(&chunk[..read]);
                match parse_available_messages(&mut buffer) {
                    Ok(messages) => {
                        for message in messages {
                            if tx.send(Ok(message)).is_err() {
                                return;
                            }
                        }
                    }
                    Err(error) => {
                        let _ = tx.send(Err(error));
                        return;
                    }
                }
                if buffered_client_response_exceeds_limit(&buffer) {
                    let _ = tx.send(Err(McpTransportError::failed(
                        "MCP server response exceeded size limit.",
                    )));
                    return;
                }
            }
            Err(_) => {
                let _ = tx.send(Err(McpTransportError::failed(
                    "MCP server stdout read failed.",
                )));
                return;
            }
        }
    }
}

#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
fn drain_stderr(stderr: Option<impl Read + Send + 'static>) {
    if let Some(mut stderr) = stderr {
        thread::spawn(move || {
            let mut sink = [0_u8; 8192];
            let mut read_total = 0_usize;
            while read_total < MAX_CLIENT_RESPONSE_BYTES {
                match stderr.read(&mut sink) {
                    Ok(0) | Err(_) => return,
                    Ok(read) => read_total = read_total.saturating_add(read),
                }
            }
        });
    }
}

#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
fn parse_available_messages(buffer: &mut Vec<u8>) -> Result<Vec<JsonValue>, McpTransportError> {
    let mut messages = Vec::new();
    while let Some(header_end) = find_header_end(buffer) {
        if header_end > MAX_CLIENT_RESPONSE_BYTES {
            return Err(McpTransportError::failed(
                "MCP server response exceeded size limit.",
            ));
        }
        let header = std::str::from_utf8(&buffer[..header_end])
            .map_err(|_| McpTransportError::failed("MCP server sent an invalid header."))?;
        let Some(content_length) = content_length(header) else {
            return Err(McpTransportError::failed(
                "MCP server sent a response without Content-Length.",
            ));
        };
        if content_length > MAX_CLIENT_RESPONSE_BYTES {
            return Err(McpTransportError::failed(
                "MCP server response exceeded size limit.",
            ));
        }
        let body_start = header_end + 4;
        let body_end = body_start.saturating_add(content_length);
        if buffer.len() < body_end {
            break;
        }
        let body = buffer[body_start..body_end].to_vec();
        buffer.drain(..body_end);
        let message = serde_json::from_slice::<JsonValue>(&body)
            .map_err(|_| McpTransportError::failed("MCP server sent invalid JSON."))?;
        messages.push(message);
    }
    Ok(messages)
}

#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
fn buffered_client_response_exceeds_limit(buffer: &[u8]) -> bool {
    if buffer.len() <= MAX_CLIENT_RESPONSE_BYTES {
        return false;
    }
    let Some(header_end) = find_header_end(buffer) else {
        return true;
    };
    if header_end > MAX_CLIENT_RESPONSE_BYTES {
        return true;
    }
    let Ok(header) = std::str::from_utf8(&buffer[..header_end]) else {
        return false;
    };
    let Some(content_length) = content_length(header) else {
        return false;
    };
    if content_length > MAX_CLIENT_RESPONSE_BYTES {
        return false;
    }
    let Some(body_end) = header_end
        .checked_add(4)
        .and_then(|body_start| body_start.checked_add(content_length))
    else {
        return true;
    };
    buffer.len() > body_end
}

#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
fn response_id(message: &JsonValue) -> Option<i64> {
    let JsonValue::Object(record) = message else {
        return None;
    };
    match record.get("id") {
        Some(JsonValue::Number(JsonNumber::I64(value))) => Some(*value),
        Some(JsonValue::Number(JsonNumber::U64(value))) => i64::try_from(*value).ok(),
        _ => None,
    }
}

#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
fn response_result(message: JsonValue) -> Result<JsonValue, McpTransportError> {
    let JsonValue::Object(mut record) = message else {
        return Err(McpTransportError::failed(
            "MCP server response was invalid.",
        ));
    };
    if let Some(JsonValue::Object(error)) = record.remove("error") {
        let code = error_code(&error);
        return Err(McpTransportError::tool_error(
            code,
            "MCP server returned error.",
        ));
    }
    Ok(record.remove("result").unwrap_or(JsonValue::Null))
}

#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
fn error_code(error: &JsonObject) -> i64 {
    match error.get("code") {
        Some(JsonValue::Number(JsonNumber::I64(value))) => *value,
        Some(JsonValue::Number(JsonNumber::U64(value))) => i64::try_from(*value).unwrap_or(0),
        _ => 0,
    }
}

#[cfg(all(feature = "mcp", not(feature = "mcp-rmcp")))]
fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(all(test, feature = "mcp-rmcp"))]
mod rmcp_transport_tests {
    use rmcp::transport::Transport;
    use tokio::io::AsyncWriteExt;

    use super::{RmcpContentLengthTransport, RmcpTransportErrorState, serve_rmcp_transport};

    // rust-style-allow: long-function because these adjacent transport
    // regression tests share one in-memory Content-Length fixture.
    #[test]
    fn rmcp_receive_records_malformed_json_as_transport_error() {
        let message = receive_error_message(b"Content-Length: 1\r\n\r\n{");

        assert!(
            message
                .as_deref()
                .is_some_and(|message| message.contains("EOF while parsing an object")),
            "{message:?}"
        );
    }

    #[test]
    fn rmcp_receive_records_missing_content_length_as_transport_error() {
        let message = receive_error_message(b"X-Test: true\r\n\r\n{}");

        assert_eq!(
            message.as_deref(),
            Some("MCP server sent a response without Content-Length.")
        );
    }

    #[test]
    fn rmcp_receive_records_oversized_body_as_transport_error() {
        let message = receive_error_message(b"Content-Length: 1048577\r\n\r\n{}");

        assert_eq!(
            message.as_deref(),
            Some("MCP server response exceeded size limit.")
        );
    }

    #[test]
    fn rmcp_initialize_surfaces_recorded_transport_error() {
        let message = initialize_error_message(b"Content-Length: 1\r\n\r\n{");

        assert!(
            message
                .as_deref()
                .is_some_and(|message| message.contains("EOF while parsing an object")),
            "{message:?}"
        );
    }

    fn initialize_error_message(bytes: &'static [u8]) -> Option<String> {
        tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()
            .ok()?
            .block_on(async move {
                let (mut writer, reader) = tokio::io::duplex(bytes.len().max(1));
                writer.write_all(bytes).await.ok()?;
                drop(writer);

                let error_state = RmcpTransportErrorState::default();
                let transport =
                    RmcpContentLengthTransport::new(reader, tokio::io::sink(), error_state.clone());

                serve_rmcp_transport(transport, &error_state)
                    .await
                    .err()
                    .map(|error| error.message_for_test().to_owned())
            })
    }

    fn receive_error_message(bytes: &'static [u8]) -> Option<String> {
        tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()
            .ok()?
            .block_on(async move {
                let (mut writer, reader) = tokio::io::duplex(bytes.len().max(1));
                writer.write_all(bytes).await.ok()?;
                drop(writer);

                let error_state = RmcpTransportErrorState::default();
                let mut transport =
                    RmcpContentLengthTransport::new(reader, tokio::io::sink(), error_state.clone());

                let message = Transport::<rmcp::RoleClient>::receive(&mut transport).await;
                assert!(message.is_none());
                error_state.take()
            })
    }
}
