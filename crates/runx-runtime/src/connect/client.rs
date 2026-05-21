// rust-style-allow: large-file because the connect client keeps OAuth polling,
// redacted HTTP error handling, and typed response validation in one security
// review unit until the connect module is split.
use std::collections::BTreeMap;
use std::fmt;
use std::time::{Duration, Instant};

use crate::runtime_http::{
    HostedHttpClient, HostedHttpError, HostedHttpHeader, HostedHttpResponse, HostedTransport,
    HttpMethod, ReqwestHttpTransport,
};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::opener::{ConnectOpener, ProcessConnectOpener};
use super::redaction::redact_connect_text;
use super::types::{
    ConnectReadyStatus, HttpConnectFlowResponse, HttpConnectListResponse,
    HttpConnectPreprovisionRequest, HttpConnectReadyResponse, HttpConnectRevokeResponse,
    HttpConnectStartResponse, ready_response,
};

pub type ConnectResult<T> = Result<T, ConnectError>;

#[derive(Clone, Eq, PartialEq)]
pub struct ConnectClientOptions {
    pub base_url: String,
    pub access_token: String,
    pub open_command: Option<String>,
    pub poll_interval_ms: Option<u64>,
    pub timeout_ms: Option<u64>,
}

impl fmt::Debug for ConnectClientOptions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConnectClientOptions")
            .field("base_url", &self.base_url)
            .field("access_token", &"[redacted]")
            .field("open_command", &self.open_command)
            .field("poll_interval_ms", &self.poll_interval_ms)
            .field("timeout_ms", &self.timeout_ms)
            .finish()
    }
}

#[derive(Clone)]
pub struct ConnectClient<T = ReqwestHttpTransport, O = ProcessConnectOpener> {
    http: HostedHttpClient<T>,
    access_token: String,
    opener: O,
    poll_interval_ms: Option<u64>,
    timeout_ms: u64,
}

impl<T: fmt::Debug, O: fmt::Debug> fmt::Debug for ConnectClient<T, O> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConnectClient")
            .field("http", &self.http)
            .field("access_token", &"[redacted]")
            .field("opener", &self.opener)
            .field("poll_interval_ms", &self.poll_interval_ms)
            .field("timeout_ms", &self.timeout_ms)
            .finish()
    }
}

#[cfg(feature = "async-http")]
impl ConnectClient<ReqwestHttpTransport, ProcessConnectOpener> {
    pub fn new(
        options: ConnectClientOptions,
        env: BTreeMap<String, String>,
    ) -> ConnectResult<Self> {
        Self::with_transport_and_opener(
            options.base_url,
            options.access_token,
            ReqwestHttpTransport::new()?,
            ProcessConnectOpener::new(options.open_command, env),
            options.poll_interval_ms,
            options.timeout_ms,
        )
    }
}

impl<T: HostedTransport, O: ConnectOpener> ConnectClient<T, O> {
    pub fn with_transport_and_opener(
        base_url: impl AsRef<str>,
        access_token: impl Into<String>,
        transport: T,
        opener: O,
        poll_interval_ms: Option<u64>,
        timeout_ms: Option<u64>,
    ) -> ConnectResult<Self> {
        let access_token = access_token.into();
        if access_token.trim().is_empty() {
            return Err(ConnectError::MissingConfiguration(
                "RUNX_CONNECT_ACCESS_TOKEN",
            ));
        }
        Ok(Self {
            http: HostedHttpClient::with_transport(base_url, transport)?,
            access_token,
            opener,
            poll_interval_ms,
            timeout_ms: timeout_ms.unwrap_or(60_000),
        })
    }

    pub fn list(&self) -> ConnectResult<HttpConnectListResponse> {
        self.request_json(HttpMethod::Get, "/v1/grants", None)
    }

    pub fn revoke(&self, grant_id: &str) -> ConnectResult<HttpConnectRevokeResponse> {
        self.request_json(
            HttpMethod::Delete,
            &format!("/v1/grants/{}", encode_segment(grant_id)),
            None,
        )
    }

    // rust-style-allow: long-function because preprovision is the hosted
    // Connect state transition boundary and keeps every redacted terminal
    // response arm in one auditable match.
    pub fn preprovision(
        &self,
        request: &HttpConnectPreprovisionRequest,
    ) -> ConnectResult<HttpConnectReadyResponse> {
        let body = serde_json::to_string(request).map_err(|error| ConnectError::Serialize {
            message: error.to_string(),
        })?;
        match self.request_start("/v1/connect/sessions", Some(body))? {
            HttpConnectStartResponse::Created { grant } => {
                Ok(ready_response(ConnectReadyStatus::Created, grant))
            }
            HttpConnectStartResponse::Unchanged { grant } => {
                Ok(ready_response(ConnectReadyStatus::Unchanged, grant))
            }
            HttpConnectStartResponse::OauthRequired {
                session_id: _,
                flow_id,
                authorize_url,
                poll_after_ms,
                expires_at: _,
            } => {
                if let Err(error) = self.opener.open(&authorize_url) {
                    return Err(ConnectError::OpenerFailed {
                        message: redact_connect_text(&error.to_string()),
                    });
                }
                self.wait_for_connect_flow(&flow_id, poll_after_ms)
            }
            HttpConnectStartResponse::CredentialRequired {
                session_id: _,
                flow_id: _,
                provider,
                auth_mode,
                credential_schema,
            } => Err(ConnectError::CredentialRequired {
                provider,
                auth_mode,
                field_count: credential_schema.fields.len(),
            }),
            HttpConnectStartResponse::Unsupported {
                provider,
                provider_status: _,
                connect_mode: _,
                demand_id,
                request_count: _,
                error,
            } => Err(ConnectError::UnsupportedProvider {
                provider: provider.unwrap_or_else(|| "<unknown>".to_owned()),
                demand_id,
                message: redact_connect_text(
                    error
                        .as_deref()
                        .unwrap_or("Provider is not supported by hosted Connect yet."),
                ),
            }),
            HttpConnectStartResponse::ConnectUnavailable {
                provider,
                provider_status: _,
                connect_mode: _,
                error,
            } => Err(ConnectError::UnsupportedProvider {
                provider: provider.unwrap_or_else(|| "<unknown>".to_owned()),
                demand_id: None,
                message: redact_connect_text(
                    error
                        .as_deref()
                        .unwrap_or("Provider is not supported by hosted Connect yet."),
                ),
            }),
        }
    }

    fn wait_for_connect_flow(
        &self,
        flow_id: &str,
        initial_poll_after_ms: Option<u64>,
    ) -> ConnectResult<HttpConnectReadyResponse> {
        let started_at = Instant::now();
        loop {
            match self.request_flow(&format!("/v1/connect/sessions/{}", encode_segment(flow_id)))? {
                HttpConnectFlowResponse::Created { grant } => {
                    return Ok(ready_response(ConnectReadyStatus::Created, grant));
                }
                HttpConnectFlowResponse::Unchanged { grant } => {
                    return Ok(ready_response(ConnectReadyStatus::Unchanged, grant));
                }
                HttpConnectFlowResponse::Failed { flow_id: _, error } => {
                    return Err(ConnectError::FlowFailed {
                        message: redact_connect_text(&error),
                    });
                }
                HttpConnectFlowResponse::Pending {
                    flow_id: _,
                    poll_after_ms,
                } => {
                    if started_at.elapsed() >= Duration::from_millis(self.timeout_ms) {
                        return Err(ConnectError::Timeout);
                    }
                    let delay_ms = poll_after_ms
                        .or(initial_poll_after_ms)
                        .or(self.poll_interval_ms)
                        .unwrap_or(750);
                    if delay_ms > 0 {
                        std::thread::sleep(Duration::from_millis(delay_ms));
                    }
                }
            }
        }
    }

    fn request_start(
        &self,
        route: &str,
        body: Option<String>,
    ) -> ConnectResult<HttpConnectStartResponse> {
        let text = self.request_text(HttpMethod::Post, route, body)?;
        let envelope = response_envelope(route, &text)?;
        match envelope.status.as_str() {
            "created"
            | "unchanged"
            | "oauth_required"
            | "credential_required"
            | "unsupported"
            | "connect_unavailable" => {
                serde_json::from_str(&text).map_err(|error| ConnectError::Contract {
                    route: safe_route(route),
                    message: error.to_string(),
                })
            }
            status => Err(ConnectError::UnsupportedStatus {
                route: safe_route(route),
                status: redact_connect_text(status),
            }),
        }
    }

    fn request_flow(&self, route: &str) -> ConnectResult<HttpConnectFlowResponse> {
        let text = self.request_text(HttpMethod::Get, route, None)?;
        let envelope = response_envelope(route, &text)?;
        match envelope.status.as_str() {
            "created" | "unchanged" | "pending" | "failed" => {
                serde_json::from_str(&text).map_err(|error| ConnectError::Contract {
                    route: safe_route(route),
                    message: error.to_string(),
                })
            }
            status => Err(ConnectError::UnsupportedStatus {
                route: safe_route(route),
                status: redact_connect_text(status),
            }),
        }
    }

    fn request_json<R: DeserializeOwned>(
        &self,
        method: HttpMethod,
        route: &str,
        body: Option<String>,
    ) -> ConnectResult<R> {
        let text = self.request_text(method, route, body)?;
        validate_json(route, &text)?;
        serde_json::from_str(&text).map_err(|error| ConnectError::Contract {
            route: safe_route(route),
            message: error.to_string(),
        })
    }

    fn request_text(
        &self,
        method: HttpMethod,
        route: &str,
        body: Option<String>,
    ) -> ConnectResult<String> {
        let response = self.send(method, route, body)?;
        if !(200..=299).contains(&response.status) {
            return Err(ConnectError::HttpStatus {
                status: response.status,
                message: http_error_message(response.status, &response.body),
            });
        }
        Ok(response.body)
    }

    fn send(
        &self,
        method: HttpMethod,
        route: &str,
        body: Option<String>,
    ) -> ConnectResult<HostedHttpResponse> {
        let mut request = self.http.request(method, route)?;
        request.headers = self.auth_headers();
        request.body = body;
        self.http.send(request).map_err(ConnectError::from)
    }

    fn auth_headers(&self) -> Vec<HostedHttpHeader> {
        vec![
            HostedHttpHeader::new("authorization", format!("Bearer {}", self.access_token)),
            HostedHttpHeader::new("accept", "application/json"),
            HostedHttpHeader::new("content-type", "application/json"),
        ]
    }
}

pub fn load_connect_options_from_env(
    env: &BTreeMap<String, String>,
) -> ConnectResult<ConnectClientOptions> {
    let base_url = env
        .get("RUNX_CONNECT_BASE_URL")
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or(ConnectError::MissingConfiguration("RUNX_CONNECT_BASE_URL"))?;
    let access_token = env
        .get("RUNX_CONNECT_ACCESS_TOKEN")
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or(ConnectError::MissingConfiguration(
            "RUNX_CONNECT_ACCESS_TOKEN",
        ))?;
    Ok(ConnectClientOptions {
        base_url,
        access_token,
        open_command: env.get("RUNX_CONNECT_OPEN_COMMAND").cloned(),
        poll_interval_ms: parse_optional_u64(env.get("RUNX_CONNECT_POLL_INTERVAL_MS")),
        timeout_ms: parse_optional_u64(env.get("RUNX_CONNECT_TIMEOUT_MS")),
    })
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error("runx connect requires {0}")]
    MissingConfiguration(&'static str),
    #[error(transparent)]
    Http(#[from] HostedHttpError),
    #[error("connect request failed with HTTP {status}: {message}")]
    HttpStatus { status: u16, message: String },
    #[error("connect route {route} returned invalid JSON: {message}")]
    InvalidJson { route: String, message: String },
    #[error("connect route {route} contract error: {message}")]
    Contract { route: String, message: String },
    #[error("unsupported connect status '{status}' from {route}")]
    UnsupportedStatus { route: String, status: String },
    #[error(
        "connect provider '{provider}' requires BYO credential submission ({auth_mode}, {field_count} field(s)); use hosted Connect credential entry"
    )]
    CredentialRequired {
        provider: String,
        auth_mode: String,
        field_count: usize,
    },
    #[error("connect provider '{provider}' is unsupported: {message}")]
    UnsupportedProvider {
        provider: String,
        demand_id: Option<String>,
        message: String,
    },
    #[error("connect flow failed: {message}")]
    FlowFailed { message: String },
    #[error("timed out waiting for OAuth flow to complete")]
    Timeout,
    #[error("{message}")]
    OpenerFailed { message: String },
    #[error("connect request serialization failed: {message}")]
    Serialize { message: String },
}

#[derive(Deserialize)]
struct ConnectStatusEnvelope {
    status: String,
}

#[derive(Deserialize)]
struct ConnectErrorEnvelope {
    error: String,
}

fn response_envelope(route: &str, body: &str) -> ConnectResult<ConnectStatusEnvelope> {
    validate_json(route, body)?;
    serde_json::from_str(body).map_err(|error| ConnectError::Contract {
        route: safe_route(route),
        message: error.to_string(),
    })
}

fn validate_json(route: &str, body: &str) -> ConnectResult<()> {
    serde_json::from_str::<runx_contracts::JsonValue>(body)
        .map(|_| ())
        .map_err(|error| ConnectError::InvalidJson {
            route: safe_route(route),
            message: error.to_string(),
        })
}

fn http_error_message(status: u16, body: &str) -> String {
    if body.is_empty() {
        return format!("HTTP {status}");
    }
    if let Ok(value) = serde_json::from_str::<ConnectErrorEnvelope>(body) {
        return redact_connect_text(&value.error);
    }
    format!("HTTP {status} with {} byte response body", body.len())
}

fn parse_optional_u64(value: Option<&String>) -> Option<u64> {
    value.and_then(|value| value.parse::<u64>().ok())
}

fn encode_segment(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                output.push(byte as char);
            }
            _ => output.push_str(&format!("%{byte:02X}")),
        }
    }
    output
}

fn safe_route(route: &str) -> String {
    if route.strip_prefix("/v1/connect/sessions/").is_some() {
        return "/v1/connect/sessions/[flow_id]".to_owned();
    }
    route.to_owned()
}
