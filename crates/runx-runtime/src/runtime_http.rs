// rust-style-allow: large-file because the runtime HTTP transport keeps request
// modeling, header validation, status parsing, and security-focused unit tests
// in one review unit.
#[cfg(feature = "async-http")]
use std::error::Error as StdError;
use std::fmt;
#[cfg(feature = "async-http")]
use std::net::SocketAddr;
#[cfg(any(feature = "async-http", test))]
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
#[cfg(feature = "async-http")]
use std::time::Duration;

#[cfg(any(feature = "async-http", test))]
use url::Url;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl HttpMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct RuntimeHttpHeader {
    pub name: String,
    pub value: String,
}

impl RuntimeHttpHeader {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

impl fmt::Debug for RuntimeHttpHeader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeHttpHeader")
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
pub struct RuntimeHttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<RuntimeHttpHeader>,
    pub body: Option<String>,
}

impl fmt::Debug for RuntimeHttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeHttpRequest")
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
pub struct RuntimeHttpResponse {
    pub status: u16,
    pub body: String,
}

impl fmt::Debug for RuntimeHttpResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeHttpResponse")
            .field("status", &self.status)
            .field("body", &format_args!("{} bytes", self.body.len()))
            .finish()
    }
}

pub trait RuntimeHttpTransport {
    fn send(&self, request: RuntimeHttpRequest) -> Result<RuntimeHttpResponse, RuntimeHttpError>;
}

#[derive(Clone, Debug)]
pub struct ReqwestHttpTransport {
    #[cfg(feature = "async-http")]
    client: reqwest::Client,
    #[cfg(feature = "async-http")]
    allow_private_networks: bool,
}

#[cfg(feature = "async-http")]
const MAX_HTTP_RESPONSE_BYTES: usize = 1024 * 1024;

#[cfg(feature = "async-http")]
impl ReqwestHttpTransport {
    pub fn new() -> Result<Self, RuntimeHttpError> {
        Self::with_timeouts_and_private_networks(
            Duration::from_secs(30),
            Duration::from_secs(10),
            false,
        )
    }

    fn with_timeouts_and_private_networks(
        request_timeout: Duration,
        connect_timeout: Duration,
        allow_private_networks: bool,
    ) -> Result<Self, RuntimeHttpError> {
        // reqwest is built with `rustls-no-provider`, so the process needs a
        // default crypto provider before a TLS client can be constructed.
        // Install ring once; an Err means another transport already set it.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(request_timeout)
            .connect_timeout(connect_timeout);
        if !allow_private_networks {
            builder = builder.dns_resolver(GuardedDnsResolver::new(TokioDnsResolver));
        }
        let client = builder
            .build()
            .map_err(|error| RuntimeHttpError::Transport {
                message: error.to_string(),
            })?;
        Ok(Self {
            client,
            allow_private_networks,
        })
    }

    /// Build a transport that may reach private or loopback networks. This is the
    /// explicit, opt-in escape from the default SSRF/private-network block; callers
    /// must require an operator-declared opt-in (e.g. an `http` source's
    /// `allowPrivateNetwork`) before choosing it, never as a default.
    pub fn with_private_network_access() -> Result<Self, RuntimeHttpError> {
        Self::with_timeouts_and_private_networks(
            Duration::from_secs(30),
            Duration::from_secs(10),
            true,
        )
    }

    #[cfg(test)]
    fn with_private_network_access_for_tests() -> Result<Self, RuntimeHttpError> {
        Self::with_private_network_access()
    }

    #[cfg(test)]
    fn with_private_network_timeouts_for_tests(
        request_timeout: Duration,
        connect_timeout: Duration,
    ) -> Result<Self, RuntimeHttpError> {
        Self::with_timeouts_and_private_networks(request_timeout, connect_timeout, true)
    }
}

#[cfg(feature = "async-http")]
impl RuntimeHttpTransport for ReqwestHttpTransport {
    fn send(&self, request: RuntimeHttpRequest) -> Result<RuntimeHttpResponse, RuntimeHttpError> {
        validate_http_url(&request.url, self.allow_private_networks)?;
        let client = self.client.clone();
        block_on_http(async move {
            let method = reqwest_method(request.method);
            let mut builder = client.request(method, request.url);
            for header in request.headers {
                validate_header(&header)?;
                let name = reqwest::header::HeaderName::from_bytes(header.name.trim().as_bytes())
                    .map_err(|error| RuntimeHttpError::InvalidHeaderName {
                    name: header.name.clone(),
                    message: error.to_string(),
                })?;
                let value =
                    reqwest::header::HeaderValue::from_str(&header.value).map_err(|error| {
                        RuntimeHttpError::InvalidHeaderValue {
                            name: header.name.clone(),
                            message: error.to_string(),
                        }
                    })?;
                builder = builder.header(name, value);
            }
            if let Some(body) = request.body {
                builder = builder.body(body);
            }
            let response = builder
                .send()
                .await
                .map_err(|error| RuntimeHttpError::Transport {
                    message: error.to_string(),
                })?;
            let status = response.status().as_u16();
            let body = read_limited_response_body(response, MAX_HTTP_RESPONSE_BYTES).await?;
            Ok(RuntimeHttpResponse { status, body })
        })
    }
}

#[cfg(feature = "async-http")]
#[derive(Clone, Debug)]
struct GuardedDnsResolver<R> {
    inner: R,
}

#[cfg(feature = "async-http")]
impl<R> GuardedDnsResolver<R> {
    fn new(inner: R) -> Self {
        Self { inner }
    }
}

#[cfg(feature = "async-http")]
impl<R> reqwest::dns::Resolve for GuardedDnsResolver<R>
where
    R: reqwest::dns::Resolve + Clone + Send + Sync + 'static,
{
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_owned();
        let inner = self.inner.clone();
        Box::pin(async move {
            let addrs = inner.resolve(name).await?;
            let mut public_addrs = Vec::new();
            for addr in addrs {
                if is_private_network_ip(addr.ip()) {
                    return Err(PrivateDnsResolutionError { host, addr }.into());
                }
                public_addrs.push(addr);
            }
            if public_addrs.is_empty() {
                return Err(EmptyDnsResolutionError { host }.into());
            }
            Ok(Box::new(public_addrs.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

#[cfg(feature = "async-http")]
#[derive(Clone, Copy, Debug, Default)]
struct TokioDnsResolver;

#[cfg(feature = "async-http")]
impl reqwest::dns::Resolve for TokioDnsResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let addrs = tokio::net::lookup_host((host.as_str(), 0))
                .await
                .map_err(|error| Box::new(error) as Box<dyn StdError + Send + Sync>)?;
            let addrs = addrs.collect::<Vec<_>>();
            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

#[cfg(feature = "async-http")]
#[derive(Debug)]
struct PrivateDnsResolutionError {
    host: String,
    addr: SocketAddr,
}

#[cfg(feature = "async-http")]
impl fmt::Display for PrivateDnsResolutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "runtime HTTP DNS resolved '{}' to non-public address {}",
            self.host, self.addr
        )
    }
}

#[cfg(feature = "async-http")]
impl StdError for PrivateDnsResolutionError {}

#[cfg(feature = "async-http")]
#[derive(Debug)]
struct EmptyDnsResolutionError {
    host: String,
}

#[cfg(feature = "async-http")]
impl fmt::Display for EmptyDnsResolutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "runtime HTTP DNS returned no addresses for '{}'",
            self.host
        )
    }
}

#[cfg(feature = "async-http")]
impl StdError for EmptyDnsResolutionError {}

#[derive(Clone, Debug)]
#[cfg(any(feature = "async-http", test))]
#[allow(dead_code)]
pub struct RuntimeHttpClient<T = ReqwestHttpTransport> {
    base_url: String,
    transport: T,
}

#[cfg(any(feature = "async-http", test))]
#[allow(dead_code)]
impl<T: RuntimeHttpTransport> RuntimeHttpClient<T> {
    pub fn with_transport(
        base_url: impl AsRef<str>,
        transport: T,
    ) -> Result<Self, RuntimeHttpError> {
        let base_url = strip_one_trailing_slash(base_url.as_ref());
        validate_http_url(&base_url, false)?;
        Ok(Self {
            base_url,
            transport,
        })
    }

    pub fn route_url(&self, route: &str) -> Result<String, RuntimeHttpError> {
        let normalized_route = route.trim_start_matches('/');
        let url = format!("{}/{}", self.base_url, normalized_route);
        validate_http_url(&url, false)?;
        Ok(url)
    }

    pub fn request(
        &self,
        method: HttpMethod,
        route: &str,
    ) -> Result<RuntimeHttpRequest, RuntimeHttpError> {
        Ok(RuntimeHttpRequest {
            method,
            url: self.route_url(route)?,
            headers: Vec::new(),
            body: None,
        })
    }

    pub fn send(
        &self,
        request: RuntimeHttpRequest,
    ) -> Result<RuntimeHttpResponse, RuntimeHttpError> {
        self.transport.send(request)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeHttpError {
    #[error("invalid runtime HTTP url: {0}")]
    InvalidUrl(#[from] url::ParseError),
    #[error("runtime HTTP transport failed: {message}")]
    Transport { message: String },
    #[error("runtime HTTP transport cannot block inside an active async runtime")]
    BlockingHttpInsideAsyncRuntime,
    #[error("runtime HTTP async runtime is unavailable: {message}")]
    AsyncRuntimeUnavailable { message: String },
    #[error("runtime HTTP transport returned invalid output: {message}")]
    TransportDecode { message: String },
    #[error("runtime HTTP response body exceeds {limit} byte limit")]
    ResponseBodyTooLarge { limit: usize },
    #[error("unsupported runtime HTTP url scheme '{scheme}': only http and https are allowed")]
    UnsupportedUrlScheme { scheme: String },
    #[error("runtime HTTP url host '{host}' is not publicly routable")]
    PrivateNetworkUrl { host: String },
    #[error("invalid runtime HTTP header name '{name}': {message}")]
    InvalidHeaderName { name: String, message: String },
    #[error("invalid runtime HTTP header value for '{name}': {message}")]
    InvalidHeaderValue { name: String, message: String },
}

pub(crate) fn strip_one_trailing_slash(value: &str) -> String {
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

#[cfg(feature = "async-http")]
fn validate_header(header: &RuntimeHttpHeader) -> Result<(), RuntimeHttpError> {
    let name = header.name.trim();
    if name.is_empty() || !name.bytes().all(is_header_token_byte) {
        return Err(RuntimeHttpError::InvalidHeaderName {
            name: header.name.clone(),
            message: "header names must be HTTP token characters".to_owned(),
        });
    }
    if header.value.contains('\r') || header.value.contains('\n') {
        return Err(RuntimeHttpError::InvalidHeaderValue {
            name: header.name.clone(),
            message: "header values must not contain line breaks".to_owned(),
        });
    }
    Ok(())
}

#[cfg(any(feature = "async-http", test))]
#[allow(dead_code)]
fn validate_http_url(value: &str, allow_private_networks: bool) -> Result<(), RuntimeHttpError> {
    let url = Url::parse(value)?;
    match url.scheme() {
        "http" | "https" => validate_public_host(&url, allow_private_networks),
        scheme => Err(RuntimeHttpError::UnsupportedUrlScheme {
            scheme: scheme.to_owned(),
        }),
    }
}

#[cfg(any(feature = "async-http", test))]
fn validate_public_host(url: &Url, allow_private_networks: bool) -> Result<(), RuntimeHttpError> {
    if allow_private_networks {
        return Ok(());
    }
    let Some(host) = url.host_str() else {
        return Err(RuntimeHttpError::PrivateNetworkUrl {
            host: "<missing>".to_owned(),
        });
    };
    let normalized = host.trim_end_matches('.').to_ascii_lowercase();
    if normalized == "localhost"
        || normalized.ends_with(".localhost")
        || normalized == "metadata.google.internal"
    {
        return Err(RuntimeHttpError::PrivateNetworkUrl {
            host: host.to_owned(),
        });
    }
    let ip_host = normalized
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(&normalized);
    if let Ok(ip) = ip_host.parse::<IpAddr>() {
        if is_private_network_ip(ip) {
            return Err(RuntimeHttpError::PrivateNetworkUrl {
                host: host.to_owned(),
            });
        }
    }
    Ok(())
}

#[cfg(any(feature = "async-http", test))]
fn is_private_network_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_private_network_ipv4(ip),
        IpAddr::V6(ip) => is_private_network_ipv6(ip),
    }
}

#[cfg(any(feature = "async-http", test))]
fn is_private_network_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || octets[0] == 0
        || (octets[0] == 100 && (octets[1] & 0xc0) == 0x40)
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
        || (octets[0] == 198 && (octets[1] == 18 || octets[1] == 19))
        || octets[0] >= 240
        || octets == [169, 254, 169, 254]
}

#[cfg(any(feature = "async-http", test))]
fn is_private_network_ipv6(ip: Ipv6Addr) -> bool {
    ip.to_ipv4_mapped().is_some_and(is_private_network_ipv4)
        || ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || is_unique_local_ipv6(ip)
        || is_unicast_link_local_ipv6(ip)
        || is_documentation_ipv6(ip)
        || nat64_embedded_ipv4(ip).is_some_and(is_private_network_ipv4)
        || six_to_four_embedded_ipv4(ip).is_some_and(is_private_network_ipv4)
}

#[cfg(any(feature = "async-http", test))]
fn is_unique_local_ipv6(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xfe00) == 0xfc00
}

#[cfg(any(feature = "async-http", test))]
fn is_unicast_link_local_ipv6(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xffc0) == 0xfe80
}

#[cfg(any(feature = "async-http", test))]
fn is_documentation_ipv6(ip: Ipv6Addr) -> bool {
    ip.segments()[0] == 0x2001 && ip.segments()[1] == 0x0db8
}

#[cfg(any(feature = "async-http", test))]
fn nat64_embedded_ipv4(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    let segments = ip.segments();
    if segments[..6] != [0x0064, 0xff9b, 0, 0, 0, 0] {
        return None;
    }
    Some(Ipv4Addr::new(
        (segments[6] >> 8) as u8,
        segments[6] as u8,
        (segments[7] >> 8) as u8,
        segments[7] as u8,
    ))
}

#[cfg(any(feature = "async-http", test))]
fn six_to_four_embedded_ipv4(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    let segments = ip.segments();
    if segments[0] != 0x2002 {
        return None;
    }
    Some(Ipv4Addr::new(
        (segments[1] >> 8) as u8,
        segments[1] as u8,
        (segments[2] >> 8) as u8,
        segments[2] as u8,
    ))
}

#[cfg(feature = "async-http")]
async fn read_limited_response_body(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<String, RuntimeHttpError> {
    if declared_response_length(&response)?.is_some_and(|length| length > limit as u64) {
        return Err(RuntimeHttpError::ResponseBodyTooLarge { limit });
    }
    let mut body = Vec::new();
    while let Some(chunk) =
        response
            .chunk()
            .await
            .map_err(|error| RuntimeHttpError::TransportDecode {
                message: error.to_string(),
            })?
    {
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(RuntimeHttpError::ResponseBodyTooLarge { limit });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&body).into_owned())
}

#[cfg(feature = "async-http")]
fn declared_response_length(response: &reqwest::Response) -> Result<Option<u64>, RuntimeHttpError> {
    let Some(value) = response.headers().get(reqwest::header::CONTENT_LENGTH) else {
        return Ok(response.content_length());
    };
    let value = value
        .to_str()
        .map_err(|error| RuntimeHttpError::TransportDecode {
            message: format!("invalid Content-Length header: {error}"),
        })?;
    value
        .parse::<u64>()
        .map(Some)
        .map_err(|error| RuntimeHttpError::TransportDecode {
            message: format!("invalid Content-Length header: {error}"),
        })
}

#[cfg(feature = "async-http")]
fn reqwest_method(method: HttpMethod) -> reqwest::Method {
    match method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Post => reqwest::Method::POST,
        HttpMethod::Put => reqwest::Method::PUT,
        HttpMethod::Patch => reqwest::Method::PATCH,
        HttpMethod::Delete => reqwest::Method::DELETE,
    }
}

#[cfg(feature = "async-http")]
fn block_on_http<F, T>(future: F) -> Result<T, RuntimeHttpError>
where
    F: std::future::Future<Output = Result<T, RuntimeHttpError>>,
{
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(RuntimeHttpError::BlockingHttpInsideAsyncRuntime);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| RuntimeHttpError::AsyncRuntimeUnavailable {
            message: error.to_string(),
        })?;
    runtime.block_on(future)
}

#[cfg(feature = "async-http")]
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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::io;
    #[cfg(feature = "async-http")]
    use std::io::{Read, Write};
    #[cfg(feature = "async-http")]
    use std::net::TcpListener;
    #[cfg(feature = "async-http")]
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    #[cfg(feature = "async-http")]
    use std::time::Duration;

    #[cfg(feature = "async-http")]
    use super::{GuardedDnsResolver, MAX_HTTP_RESPONSE_BYTES, ReqwestHttpTransport, block_on_http};
    use super::{
        HttpMethod, RuntimeHttpClient, RuntimeHttpError, RuntimeHttpHeader, RuntimeHttpRequest,
        RuntimeHttpResponse, RuntimeHttpTransport,
    };
    #[cfg(feature = "async-http")]
    use reqwest::dns::Resolve as _;

    #[derive(Default)]
    struct MockTransport {
        requests: RefCell<Vec<RuntimeHttpRequest>>,
    }

    impl RuntimeHttpTransport for &MockTransport {
        fn send(
            &self,
            request: RuntimeHttpRequest,
        ) -> Result<RuntimeHttpResponse, RuntimeHttpError> {
            self.requests.borrow_mut().push(request);
            Ok(RuntimeHttpResponse {
                status: 204,
                body: String::new(),
            })
        }
    }

    #[cfg(feature = "async-http")]
    #[derive(Clone, Debug)]
    struct StaticDnsResolver {
        addrs: Vec<SocketAddr>,
    }

    #[cfg(feature = "async-http")]
    impl reqwest::dns::Resolve for StaticDnsResolver {
        fn resolve(&self, _name: reqwest::dns::Name) -> reqwest::dns::Resolving {
            let addrs = self.addrs.clone();
            Box::pin(async move { Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs) })
        }
    }

    #[derive(Debug, thiserror::Error)]
    enum RuntimeHttpTestError {
        #[error(transparent)]
        RuntimeHttp(#[from] RuntimeHttpError),
        #[error(transparent)]
        Io(#[from] io::Error),
        #[cfg(feature = "async-http")]
        #[error("server thread panicked")]
        ServerThread,
    }

    #[test]
    fn client_normalizes_base_url_and_routes_requests() -> Result<(), RuntimeHttpTestError> {
        let transport = MockTransport::default();
        let client = RuntimeHttpClient::with_transport("https://api.example/", &transport)?;

        let mut request = client.request(HttpMethod::Delete, "/v1/grants/grant_1")?;
        request
            .headers
            .push(RuntimeHttpHeader::new("accept", "application/json"));
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
        let request = RuntimeHttpRequest {
            method: HttpMethod::Get,
            url: "https://api.example/v1/grants".to_owned(),
            headers: vec![
                RuntimeHttpHeader::new("authorization", "Bearer SECRET_RUNTIME_TOKEN"),
                RuntimeHttpHeader::new("x-runx-token", "SECRET_HEADER_TOKEN"),
                RuntimeHttpHeader::new("accept", "application/json"),
            ],
            body: Some("SECRET_BODY".to_owned()),
        };

        let debug = format!("{request:?}");
        assert!(!debug.contains("SECRET_RUNTIME_TOKEN"));
        assert!(!debug.contains("SECRET_HEADER_TOKEN"));
        assert!(!debug.contains("SECRET_BODY"));
        assert!(debug.contains("[redacted]"));
        assert!(debug.contains("application/json"));
    }

    #[test]
    fn invalid_base_urls_fail_closed() {
        assert!(RuntimeHttpClient::with_transport("not a url", &MockTransport::default()).is_err());
        assert!(matches!(
            RuntimeHttpClient::with_transport("file:///tmp/runx.sock", &MockTransport::default()),
            Err(RuntimeHttpError::UnsupportedUrlScheme { .. })
        ));
    }

    #[test]
    fn private_network_base_urls_fail_closed() {
        for value in [
            "http://localhost",
            "http://service.localhost",
            "http://127.0.0.1",
            "http://10.0.0.1",
            "http://172.16.0.1",
            "http://192.168.0.1",
            "http://169.254.169.254",
            "http://100.64.0.1",
            "http://100.127.255.255",
            "http://192.0.0.1",
            "http://198.18.0.1",
            "http://240.0.0.1",
            "http://0.1.2.3",
            "http://[::1]",
            "http://[::ffff:127.0.0.1]",
            "http://[64:ff9b::7f00:1]",
            "http://[2002:7f00:1::]",
            "http://[fc00::1]",
            "http://[fe80::1]",
            "http://metadata.google.internal",
        ] {
            assert!(
                matches!(
                    RuntimeHttpClient::with_transport(value, &MockTransport::default()),
                    Err(RuntimeHttpError::PrivateNetworkUrl { .. })
                ),
                "{value} should be rejected as private"
            );
        }
    }

    #[test]
    fn public_base_urls_are_allowed() -> Result<(), RuntimeHttpTestError> {
        RuntimeHttpClient::with_transport("https://api.example", &MockTransport::default())?;
        RuntimeHttpClient::with_transport("http://8.8.8.8", &MockTransport::default())?;
        RuntimeHttpClient::with_transport("http://[64:ff9b::808:808]", &MockTransport::default())?;
        Ok(())
    }

    #[test]
    #[cfg(feature = "async-http")]
    fn guarded_dns_resolver_rejects_private_resolved_addresses() -> Result<(), RuntimeHttpTestError>
    {
        let resolver = GuardedDnsResolver::new(StaticDnsResolver {
            addrs: vec![SocketAddr::V4(SocketAddrV4::new(
                Ipv4Addr::new(127, 0, 0, 1),
                0,
            ))],
        });
        let name = "public.example"
            .parse()
            .map_err(|error| RuntimeHttpError::Transport {
                message: format!("test DNS name should parse: {error}"),
            })?;
        let error =
            block_on_http(async {
                resolver.resolve(name).await.map(|_| ()).map_err(|error| {
                    RuntimeHttpError::Transport {
                        message: error.to_string(),
                    }
                })
            })
            .err();

        assert!(
            matches!(error, Some(RuntimeHttpError::Transport { ref message }) if message.contains("non-public address")),
            "expected private DNS resolution to fail closed, got: {error:?}"
        );
        Ok(())
    }

    #[test]
    #[cfg(feature = "async-http")]
    fn reqwest_transport_does_not_follow_redirects() -> Result<(), RuntimeHttpTestError> {
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

        let transport = ReqwestHttpTransport::with_private_network_access_for_tests()?;
        let response = transport.send(RuntimeHttpRequest {
            method: HttpMethod::Get,
            url: format!("http://{address}/start"),
            headers: Vec::new(),
            body: None,
        })?;
        let request = server
            .join()
            .map_err(|_| RuntimeHttpTestError::ServerThread)??;

        assert_eq!(response.status, 302);
        assert!(request.starts_with("GET /start "));
        Ok(())
    }

    #[test]
    #[cfg(feature = "async-http")]
    fn reqwest_transport_rejects_header_injection() -> Result<(), RuntimeHttpTestError> {
        let transport = ReqwestHttpTransport::new()?;
        let error = transport
            .send(RuntimeHttpRequest {
                method: HttpMethod::Get,
                url: "https://api.example/v1".to_owned(),
                headers: vec![RuntimeHttpHeader::new("x-runx", "good\nbad")],
                body: None,
            })
            .err();
        assert!(matches!(
            error,
            Some(RuntimeHttpError::InvalidHeaderValue { .. })
        ));
        Ok(())
    }

    #[cfg(feature = "async-http")]
    #[test]
    fn reqwest_transport_rejects_non_http_urls_before_sending() -> Result<(), RuntimeHttpTestError>
    {
        let transport = ReqwestHttpTransport::new()?;
        let error = transport
            .send(RuntimeHttpRequest {
                method: HttpMethod::Get,
                url: "file:///etc/passwd".to_owned(),
                headers: Vec::new(),
                body: None,
            })
            .err();

        assert!(matches!(
            error,
            Some(RuntimeHttpError::UnsupportedUrlScheme { .. })
        ));
        Ok(())
    }

    #[cfg(feature = "async-http")]
    #[test]
    fn reqwest_transport_rejects_oversized_content_length() -> Result<(), RuntimeHttpTestError> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let server = std::thread::spawn(move || -> Result<(), std::io::Error> {
            let (mut stream, _) = listener.accept()?;
            let mut buffer = [0_u8; 1024];
            let _ = stream.read(&mut buffer)?;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                MAX_HTTP_RESPONSE_BYTES + 1
            );
            stream.write_all(response.as_bytes())?;
            Ok(())
        });

        let transport = ReqwestHttpTransport::with_private_network_access_for_tests()?;
        let error = transport
            .send(RuntimeHttpRequest {
                method: HttpMethod::Get,
                url: format!("http://{address}/too-large"),
                headers: Vec::new(),
                body: None,
            })
            .err();
        server
            .join()
            .map_err(|_| RuntimeHttpTestError::ServerThread)??;

        assert!(matches!(
            error,
            Some(RuntimeHttpError::ResponseBodyTooLarge { limit })
                if limit == MAX_HTTP_RESPONSE_BYTES
        ));
        Ok(())
    }

    #[cfg(feature = "async-http")]
    #[test]
    fn reqwest_transport_caps_streamed_response_body() -> Result<(), RuntimeHttpTestError> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let server = std::thread::spawn(move || -> Result<(), std::io::Error> {
            let (mut stream, _) = listener.accept()?;
            let mut buffer = [0_u8; 1024];
            let _ = stream.read(&mut buffer)?;
            stream.write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n")?;
            let _ = stream.write_all(&vec![b'a'; MAX_HTTP_RESPONSE_BYTES + 1]);
            Ok(())
        });

        let transport = ReqwestHttpTransport::with_private_network_access_for_tests()?;
        let error = transport
            .send(RuntimeHttpRequest {
                method: HttpMethod::Get,
                url: format!("http://{address}/stream-too-large"),
                headers: Vec::new(),
                body: None,
            })
            .err();
        server
            .join()
            .map_err(|_| RuntimeHttpTestError::ServerThread)??;

        assert!(matches!(
            error,
            Some(RuntimeHttpError::ResponseBodyTooLarge { limit })
                if limit == MAX_HTTP_RESPONSE_BYTES
        ));
        Ok(())
    }

    #[cfg(feature = "async-http")]
    #[test]
    fn reqwest_transport_times_out_stalled_response() -> Result<(), RuntimeHttpTestError> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let server = std::thread::spawn(move || -> Result<(), std::io::Error> {
            let (_stream, _) = listener.accept()?;
            std::thread::sleep(Duration::from_millis(500));
            Ok(())
        });

        let transport = ReqwestHttpTransport::with_private_network_timeouts_for_tests(
            Duration::from_millis(100),
            Duration::from_millis(100),
        )?;
        let error = transport
            .send(RuntimeHttpRequest {
                method: HttpMethod::Get,
                url: format!("http://{address}/stall"),
                headers: Vec::new(),
                body: None,
            })
            .err();
        server
            .join()
            .map_err(|_| RuntimeHttpTestError::ServerThread)??;

        assert!(matches!(error, Some(RuntimeHttpError::Transport { .. })));
        Ok(())
    }
}
