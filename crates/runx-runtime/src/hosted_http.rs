// rust-style-allow: large-file because the hosted HTTP transport keeps curl
// process execution, header validation, status parsing, and security-focused
// unit tests in one review unit.
use std::fmt;
use std::io::Write;
use std::process::{Command, Output, Stdio};

use url::Url;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpMethod {
    Get,
    Post,
    Delete,
}

impl HttpMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Delete => "DELETE",
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct HostedHttpHeader {
    pub name: String,
    pub value: String,
}

impl HostedHttpHeader {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

impl fmt::Debug for HostedHttpHeader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HostedHttpHeader")
            .field("name", &self.name)
            .field(
                "value",
                &if sensitive_header_name(&self.name) {
                    "[redacted]"
                } else {
                    self.value.as_str()
                },
            )
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct HostedHttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<HostedHttpHeader>,
    pub body: Option<String>,
}

impl fmt::Debug for HostedHttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HostedHttpRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &self.headers)
            .field(
                "body",
                &self.body.as_ref().map(|_| "[redacted body present]"),
            )
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct HostedHttpResponse {
    pub status: u16,
    pub body: String,
}

impl fmt::Debug for HostedHttpResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HostedHttpResponse")
            .field("status", &self.status)
            .field("body", &format_args!("{} bytes", self.body.len()))
            .finish()
    }
}

pub trait HostedTransport {
    fn send(&self, request: HostedHttpRequest) -> Result<HostedHttpResponse, HostedHttpError>;
}

#[derive(Clone, Debug)]
pub struct CommandHttpTransport {
    command: String,
}

impl CommandHttpTransport {
    pub fn new() -> Self {
        Self {
            command: "curl".to_owned(),
        }
    }

    pub fn with_command(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
        }
    }
}

impl Default for CommandHttpTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl HostedTransport for CommandHttpTransport {
    fn send(&self, request: HostedHttpRequest) -> Result<HostedHttpResponse, HostedHttpError> {
        let output = self.run_command(request)?;
        let stdout = self.success_stdout(output)?;
        let (body, status) = parse_transport_output(&stdout)?;
        Ok(HostedHttpResponse {
            status,
            body: body.to_owned(),
        })
    }
}

impl CommandHttpTransport {
    fn run_command(&self, request: HostedHttpRequest) -> Result<Output, HostedHttpError> {
        let mut command = self.request_command(&request)?;
        let mut child = command
            .spawn()
            .map_err(|error| self.spawn_error(error.to_string()))?;
        if let Some(body) = request.body {
            let stdin = child
                .stdin
                .as_mut()
                .ok_or_else(|| self.spawn_error("transport stdin was not available"))?;
            stdin
                .write_all(body.as_bytes())
                .map_err(|error| self.spawn_error(error.to_string()))?;
        }
        child
            .wait_with_output()
            .map_err(|error| self.spawn_error(error.to_string()))
    }

    fn request_command(&self, request: &HostedHttpRequest) -> Result<Command, HostedHttpError> {
        let mut command = Command::new(&self.command);
        command
            .arg("--silent")
            .arg("--show-error")
            .arg("--request")
            .arg(request.method.as_str())
            .arg("--output")
            .arg("-")
            .arg("--write-out")
            .arg("\n__RUNX_HTTP_STATUS__:%{http_code}");
        for header in &request.headers {
            validate_header(header)?;
            command.arg("--header").arg(format!(
                "{}: {}",
                header.name.trim(),
                header.value.as_str()
            ));
        }
        if request.body.is_some() {
            command.arg("--data-binary").arg("@-");
            command.stdin(Stdio::piped());
        } else {
            command.stdin(Stdio::null());
        }
        command
            .arg("--")
            .arg(&request.url)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        Ok(command)
    }

    fn success_stdout(&self, output: Output) -> Result<String, HostedHttpError> {
        let stderr =
            String::from_utf8(output.stderr).map_err(|error| HostedHttpError::TransportDecode {
                message: error.to_string(),
            })?;
        if !output.status.success() {
            return Err(HostedHttpError::TransportFailed {
                command: self.command.clone(),
                status: output.status.code(),
                stderr,
            });
        }
        String::from_utf8(output.stdout).map_err(|error| HostedHttpError::TransportDecode {
            message: error.to_string(),
        })
    }

    fn spawn_error(&self, message: impl Into<String>) -> HostedHttpError {
        HostedHttpError::TransportSpawn {
            command: self.command.clone(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct HostedHttpClient<T = CommandHttpTransport> {
    base_url: String,
    transport: T,
}

impl<T: HostedTransport> HostedHttpClient<T> {
    pub fn with_transport(
        base_url: impl AsRef<str>,
        transport: T,
    ) -> Result<Self, HostedHttpError> {
        let base_url = strip_one_trailing_slash(base_url.as_ref());
        Url::parse(&base_url)?;
        Ok(Self {
            base_url,
            transport,
        })
    }

    pub fn route_url(&self, route: &str) -> Result<String, HostedHttpError> {
        let normalized_route = route.trim_start_matches('/');
        let url = format!("{}/{}", self.base_url, normalized_route);
        Url::parse(&url)?;
        Ok(url)
    }

    pub fn request(
        &self,
        method: HttpMethod,
        route: &str,
    ) -> Result<HostedHttpRequest, HostedHttpError> {
        Ok(HostedHttpRequest {
            method,
            url: self.route_url(route)?,
            headers: Vec::new(),
            body: None,
        })
    }

    pub fn send(&self, request: HostedHttpRequest) -> Result<HostedHttpResponse, HostedHttpError> {
        self.transport.send(request)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HostedHttpError {
    #[error("invalid hosted HTTP url: {0}")]
    InvalidUrl(#[from] url::ParseError),
    #[error("hosted HTTP transport command '{command}' failed to start: {message}")]
    TransportSpawn { command: String, message: String },
    #[error("hosted HTTP transport command '{command}' failed with status {status:?}: {stderr}")]
    TransportFailed {
        command: String,
        status: Option<i32>,
        stderr: String,
    },
    #[error("hosted HTTP transport returned invalid output: {message}")]
    TransportDecode { message: String },
    #[error("invalid hosted HTTP header name '{name}': {message}")]
    InvalidHeaderName { name: String, message: String },
    #[error("invalid hosted HTTP header value for '{name}': {message}")]
    InvalidHeaderValue { name: String, message: String },
}

fn strip_one_trailing_slash(value: &str) -> String {
    value.strip_suffix('/').unwrap_or(value).to_owned()
}

fn sensitive_header_name(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    normalized == "authorization"
        || normalized == "proxy-authorization"
        || normalized.contains("token")
        || normalized.contains("secret")
        || normalized.contains("api-key")
}

fn validate_header(header: &HostedHttpHeader) -> Result<(), HostedHttpError> {
    let name = header.name.trim();
    if name.is_empty() || !name.bytes().all(is_header_token_byte) {
        return Err(HostedHttpError::InvalidHeaderName {
            name: header.name.clone(),
            message: "header names must be HTTP token characters".to_owned(),
        });
    }
    if header.value.contains('\r') || header.value.contains('\n') {
        return Err(HostedHttpError::InvalidHeaderValue {
            name: header.name.clone(),
            message: "header values must not contain line breaks".to_owned(),
        });
    }
    Ok(())
}

fn is_header_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

fn parse_transport_output(stdout: &str) -> Result<(&str, u16), HostedHttpError> {
    let (body, status_text) = stdout
        .rsplit_once("\n__RUNX_HTTP_STATUS__:")
        .ok_or_else(|| HostedHttpError::TransportDecode {
            message: "transport did not report an HTTP status".to_owned(),
        })?;
    let status =
        status_text
            .trim()
            .parse::<u16>()
            .map_err(|error| HostedHttpError::TransportDecode {
                message: error.to_string(),
            })?;
    Ok((body, status))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::io;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    use super::{
        CommandHttpTransport, HostedHttpClient, HostedHttpError, HostedHttpHeader,
        HostedHttpRequest, HostedHttpResponse, HostedTransport, HttpMethod,
    };

    #[derive(Default)]
    struct MockTransport {
        requests: RefCell<Vec<HostedHttpRequest>>,
    }

    impl HostedTransport for &MockTransport {
        fn send(&self, request: HostedHttpRequest) -> Result<HostedHttpResponse, HostedHttpError> {
            self.requests.borrow_mut().push(request);
            Ok(HostedHttpResponse {
                status: 204,
                body: String::new(),
            })
        }
    }

    #[derive(Debug, thiserror::Error)]
    enum HostedHttpTestError {
        #[error(transparent)]
        HostedHttp(#[from] HostedHttpError),
        #[error(transparent)]
        Io(#[from] io::Error),
        #[error("server thread panicked")]
        ServerThread,
    }

    #[test]
    fn client_normalizes_base_url_and_routes_requests() -> Result<(), HostedHttpTestError> {
        let transport = MockTransport::default();
        let client = HostedHttpClient::with_transport("https://api.example/", &transport)?;

        let mut request = client.request(HttpMethod::Delete, "/v1/grants/grant_1")?;
        request
            .headers
            .push(HostedHttpHeader::new("accept", "application/json"));
        request.body = Some("{\"ok\":true}".to_owned());
        let response = client.send(request)?;

        assert_eq!(response.status, 204);
        let sent = transport.requests.borrow();
        assert_eq!(sent[0].method, HttpMethod::Delete);
        assert_eq!(sent[0].url, "https://api.example/v1/grants/grant_1");
        assert_eq!(sent[0].headers[0].name, "accept");
        assert_eq!(sent[0].body.as_deref(), Some("{\"ok\":true}"));
        Ok(())
    }

    #[test]
    fn debug_output_redacts_sensitive_header_values() {
        let request = HostedHttpRequest {
            method: HttpMethod::Get,
            url: "https://api.example/v1/grants".to_owned(),
            headers: vec![
                HostedHttpHeader::new("authorization", "Bearer SECRET_CONNECT_TOKEN"),
                HostedHttpHeader::new("x-runx-token", "SECRET_HEADER_TOKEN"),
                HostedHttpHeader::new("accept", "application/json"),
            ],
            body: Some("SECRET_BODY".to_owned()),
        };

        let debug = format!("{request:?}");
        assert!(!debug.contains("SECRET_CONNECT_TOKEN"));
        assert!(!debug.contains("SECRET_HEADER_TOKEN"));
        assert!(!debug.contains("SECRET_BODY"));
        assert!(debug.contains("[redacted]"));
        assert!(debug.contains("application/json"));
    }

    #[test]
    fn invalid_base_urls_fail_closed() {
        assert!(HostedHttpClient::with_transport("not a url", &MockTransport::default()).is_err());
    }

    #[test]
    fn command_transport_does_not_follow_redirects() -> Result<(), HostedHttpTestError> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let server = std::thread::spawn(move || -> Result<String, std::io::Error> {
            let (mut stream, _) = listener.accept()?;
            let mut buffer = [0_u8; 1024];
            let bytes_read = stream.read(&mut buffer)?;
            stream.write_all(
                b"HTTP/1.1 302 Found\r\nLocation: /redirected\r\nContent-Length: 0\r\n\r\n",
            )?;
            Ok(String::from_utf8_lossy(&buffer[..bytes_read]).into_owned())
        });

        let client = HostedHttpClient::with_transport(
            format!("http://{address}"),
            CommandHttpTransport::new(),
        )?;
        let response = client.send(client.request(HttpMethod::Get, "/start")?)?;
        let request = server
            .join()
            .map_err(|_| HostedHttpTestError::ServerThread)??;

        assert_eq!(response.status, 302);
        assert!(request.starts_with("GET /start "));
        Ok(())
    }

    #[test]
    fn command_transport_rejects_header_injection() {
        let transport = CommandHttpTransport::new();
        let error = transport
            .send(HostedHttpRequest {
                method: HttpMethod::Get,
                url: "https://api.example/v1".to_owned(),
                headers: vec![HostedHttpHeader::new("x-runx", "good\nbad")],
                body: None,
            })
            .err();
        assert!(matches!(
            error,
            Some(HostedHttpError::InvalidHeaderValue { .. })
        ));
    }
}
