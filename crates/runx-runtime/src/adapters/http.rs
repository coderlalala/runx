//! Governed HTTP execution on the runtime HTTP transport.
//!
//! The keystone call-out front. Given a method, URL, and inputs, this builds a
//! request, sends it through the governed `runtime_http` transport (which enforces
//! SSRF and private-network filtering, header validation, no-redirect, SSL, and
//! timeouts), and maps the response to the universal [`SkillOutput`]. GET and DELETE
//! map inputs to the query string; POST, PUT, and PATCH map them to a JSON body. It reuses the same
//! transport the Anthropic resolver and the registry client use, so there is one
//! governed HTTP path, not a parallel one.

use std::collections::BTreeMap;

use runx_contracts::{JsonObject, JsonValue};
use serde_json::Value as WireValue;

use crate::RuntimeError;
use crate::adapter::{InvocationStatus, SkillAdapter, SkillInvocation, SkillOutput};
use crate::credentials::SecretEnv;
use crate::runtime_http::{
    HttpMethod, ReqwestHttpTransport, RuntimeHttpHeader, RuntimeHttpRequest, RuntimeHttpTransport,
};
use runx_parser::SourceKind;

const HTTP_SKILL: &str = "http";

/// A governed HTTP call: a method, a URL, and the request headers (auth and the
/// like, already resolved). Inputs are mapped to the query string (GET, DELETE) or
/// a JSON body (POST, PUT, PATCH).
#[derive(Clone, Debug)]
pub struct HttpCall {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<RuntimeHttpHeader>,
}

fn failure(message: String) -> RuntimeError {
    RuntimeError::SkillFailed {
        skill_name: HTTP_SKILL.to_owned(),
        message,
    }
}

fn scalar(value: &WireValue) -> Option<String> {
    match value {
        WireValue::String(value) => Some(value.clone()),
        WireValue::Bool(value) => Some(value.to_string()),
        WireValue::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn with_query(url: &str, inputs: &JsonObject) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    let mut any = false;
    for (key, value) in inputs {
        if let Some(value) = scalar(&serde_json::to_value(value).unwrap_or(WireValue::Null)) {
            serializer.append_pair(key, &value);
            any = true;
        }
    }
    if !any {
        return url.to_owned();
    }
    let separator = if url.contains('?') { '&' } else { '?' };
    format!("{url}{separator}{}", serializer.finish())
}

fn json_body(inputs: &JsonObject) -> Result<String, RuntimeError> {
    serde_json::to_string(&serde_json::to_value(inputs).unwrap_or(WireValue::Null))
        .map_err(|error| failure(format!("serializing http request body: {error}")))
}

/// Execute a governed HTTP call and seal the response into a [`SkillOutput`]. A
/// non-2xx status is a clean failure (the body is still captured), not an error.
pub fn execute_http_call<T: RuntimeHttpTransport>(
    transport: &T,
    call: &HttpCall,
    inputs: &JsonObject,
) -> Result<SkillOutput, RuntimeError> {
    let mut headers = call.headers.clone();
    let (url, body) = match call.method {
        HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch => {
            if !headers
                .iter()
                .any(|header| header.name.eq_ignore_ascii_case("content-type"))
            {
                headers.push(RuntimeHttpHeader::new("content-type", "application/json"));
            }
            (call.url.clone(), Some(json_body(inputs)?))
        }
        HttpMethod::Get | HttpMethod::Delete => (with_query(&call.url, inputs), None),
    };
    let response = transport
        .send(RuntimeHttpRequest {
            method: call.method,
            url,
            headers,
            body,
        })
        .map_err(|error| failure(format!("http request failed: {error}")))?;
    let success = (200..300).contains(&response.status);
    let mut metadata = JsonObject::new();
    metadata.insert(
        "http_status".to_owned(),
        JsonValue::String(response.status.to_string()),
    );
    Ok(SkillOutput {
        status: if success {
            InvocationStatus::Success
        } else {
            InvocationStatus::Failure
        },
        stdout: response.body,
        stderr: String::new(),
        exit_code: Some(i32::from(!success)),
        duration_ms: 0,
        metadata,
    })
}

const SECRET_PREFIX: &str = "${secret:";

/// Resolve `${secret:NAME}` references in a header value against the run's secret
/// env, mirroring how the cli-tool front lets a command reference a delivered
/// secret. A reference to a secret that was not delivered fails closed.
fn substitute_secrets(value: &str, secrets: &SecretEnv) -> Result<String, RuntimeError> {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find(SECRET_PREFIX) {
        out.push_str(&rest[..start]);
        let after = &rest[start + SECRET_PREFIX.len()..];
        let end = after.find('}').ok_or_else(|| {
            failure("http header secret reference is missing a closing '}'".to_owned())
        })?;
        let name = &after[..end];
        let secret = secrets.get(name).ok_or_else(|| {
            failure(format!(
                "http header references secret {name}, which was not delivered to this run"
            ))
        })?;
        out.push_str(secret);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Build the request headers from the source's validated `headers` map, resolving
/// any `${secret:NAME}` references. Header names and values are otherwise passed
/// through verbatim; the transport validates them and redacts sensitive ones.
fn resolve_headers(
    headers: Option<&BTreeMap<String, String>>,
    secrets: &SecretEnv,
) -> Result<Vec<RuntimeHttpHeader>, RuntimeError> {
    let Some(headers) = headers else {
        return Ok(Vec::new());
    };
    headers
        .iter()
        .map(|(name, value)| {
            Ok(RuntimeHttpHeader::new(
                name.clone(),
                substitute_secrets(value, secrets)?,
            ))
        })
        .collect()
}

/// Parse a manifest method string into an [`HttpMethod`], defaulting to GET. The
/// parser already restricts `source.method` to GET, POST, or DELETE, so this is a
/// total mapping with a fail-closed arm.
fn parse_method(raw: Option<&str>) -> Result<HttpMethod, RuntimeError> {
    match raw.map(str::to_ascii_uppercase).as_deref() {
        None | Some("GET") => Ok(HttpMethod::Get),
        Some("POST") => Ok(HttpMethod::Post),
        Some("PUT") => Ok(HttpMethod::Put),
        Some("PATCH") => Ok(HttpMethod::Patch),
        Some("DELETE") => Ok(HttpMethod::Delete),
        Some(other) => Err(failure(format!("unsupported http method {other}"))),
    }
}

/// Merge an invocation's raw and resolved inputs, with resolved inputs (the
/// materialized `$input.*` values) taking precedence.
fn merged_inputs(invocation: &SkillInvocation) -> JsonObject {
    let mut inputs = invocation.inputs.clone();
    for (key, value) in &invocation.resolved_inputs {
        inputs.insert(key.clone(), value.clone());
    }
    inputs
}

/// The governed HTTP skill adapter: reads `url`/`method`/`headers` from an `http`
/// source, resolves credential headers, and runs the call through the governed
/// transport. The default constructs a [`ReqwestHttpTransport`]; the engine itself
/// ([`execute_http_call`]) is transport-generic and unit-tested with a stub.
#[derive(Clone, Copy, Debug, Default)]
pub struct HttpSkillAdapter;

impl SkillAdapter for HttpSkillAdapter {
    fn adapter_type(&self) -> &'static str {
        HTTP_SKILL
    }

    fn invoke(&self, request: SkillInvocation) -> Result<SkillOutput, RuntimeError> {
        if request.source.source_type != SourceKind::Http {
            return Err(RuntimeError::UnsupportedAdapter {
                adapter_type: request.source.source_type.as_str().to_owned(),
            });
        }
        let http = request
            .source
            .http
            .as_ref()
            .ok_or_else(|| failure("http source is missing its http config".to_owned()))?;
        let call = HttpCall {
            method: parse_method(http.method.as_deref())?,
            url: http.url.clone(),
            headers: resolve_headers(
                http.headers.as_ref(),
                request.credential_delivery.secret_env(),
            )?,
        };
        // Default transport blocks private/loopback networks; the source must
        // explicitly opt in to reach them (mirrors the sandbox network opt-in).
        let transport = if http.allow_private_network.unwrap_or(false) {
            ReqwestHttpTransport::with_private_network_access()
        } else {
            ReqwestHttpTransport::new()
        }
        .map_err(|error| failure(format!("http transport unavailable: {error}")))?;
        execute_http_call(&transport, &call, &merged_inputs(&request))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_http::{RuntimeHttpError, RuntimeHttpResponse};
    use std::cell::RefCell;

    struct StubTransport {
        status: u16,
        body: String,
        requests: RefCell<Vec<RuntimeHttpRequest>>,
    }

    impl RuntimeHttpTransport for StubTransport {
        fn send(
            &self,
            request: RuntimeHttpRequest,
        ) -> Result<RuntimeHttpResponse, RuntimeHttpError> {
            self.requests.borrow_mut().push(request);
            Ok(RuntimeHttpResponse {
                status: self.status,
                body: self.body.clone(),
            })
        }
    }

    fn stub(status: u16, body: &str) -> StubTransport {
        StubTransport {
            status,
            body: body.to_owned(),
            requests: RefCell::new(Vec::new()),
        }
    }

    fn inputs(pairs: &[(&str, &str)]) -> JsonObject {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), JsonValue::String((*value).to_owned())))
            .collect()
    }

    #[test]
    fn get_maps_inputs_to_query_and_seals_the_response() -> Result<(), RuntimeError> {
        let transport = stub(200, r#"{"ok":true}"#);
        let call = HttpCall {
            method: HttpMethod::Get,
            url: "https://api.example.test/v1/pets".to_owned(),
            headers: Vec::new(),
        };
        let output = execute_http_call(&transport, &call, &inputs(&[("id", "p-7")]))?;
        assert_eq!(output.status, InvocationStatus::Success);
        assert_eq!(output.stdout, r#"{"ok":true}"#);
        let sent = transport.requests.borrow();
        assert!(
            sent.len() == 1 && sent[0].url.contains("id=p-7") && sent[0].body.is_none(),
            "GET inputs must go on the query string with no body; got: {:?}",
            sent.first()
        );
        Ok(())
    }

    #[test]
    fn post_maps_inputs_to_a_json_body() -> Result<(), RuntimeError> {
        let transport = stub(201, "");
        let call = HttpCall {
            method: HttpMethod::Post,
            url: "https://api.example.test/v1/pets".to_owned(),
            headers: Vec::new(),
        };
        execute_http_call(&transport, &call, &inputs(&[("name", "rex")]))?;
        let sent = transport.requests.borrow();
        assert!(
            sent[0]
                .body
                .as_deref()
                .is_some_and(|body| body.contains(r#""name":"rex""#)),
            "POST inputs must go in the JSON body; got: {:?}",
            sent[0].body
        );
        Ok(())
    }

    #[test]
    fn put_carries_a_json_body_and_the_method_reaches_the_wire() -> Result<(), RuntimeError> {
        let transport = stub(200, "{}");
        let call = HttpCall {
            method: HttpMethod::Put,
            url: "https://api.example.test/v1/pets/p-7".to_owned(),
            headers: Vec::new(),
        };
        execute_http_call(&transport, &call, &inputs(&[("name", "rex")]))?;
        let sent = transport.requests.borrow();
        assert!(
            sent[0].method == HttpMethod::Put
                && sent[0]
                    .body
                    .as_deref()
                    .is_some_and(|body| body.contains(r#""name":"rex""#)),
            "PUT must carry the inputs as a JSON body; got: {:?}",
            sent.first()
        );
        Ok(())
    }

    #[test]
    fn caller_headers_reach_the_wire_and_post_keeps_an_explicit_content_type()
    -> Result<(), RuntimeError> {
        let transport = stub(200, "{}");
        let call = HttpCall {
            method: HttpMethod::Post,
            url: "https://api.example.test/v1/pets".to_owned(),
            headers: vec![
                RuntimeHttpHeader::new("authorization", "Bearer t"),
                RuntimeHttpHeader::new("content-type", "application/cbor"),
            ],
        };
        execute_http_call(&transport, &call, &inputs(&[("name", "rex")]))?;
        let sent = transport.requests.borrow();
        let content_types = sent[0]
            .headers
            .iter()
            .filter(|header| header.name.eq_ignore_ascii_case("content-type"))
            .count();
        assert!(
            sent[0]
                .headers
                .iter()
                .any(|header| header.name == "authorization" && header.value == "Bearer t")
                && content_types == 1,
            "caller headers must pass through and an explicit content-type must not be duplicated; got: {:?}",
            sent[0].headers
        );
        Ok(())
    }

    #[test]
    fn substitute_secrets_resolves_a_delivered_secret_and_fails_closed_on_a_missing_one()
    -> Result<(), Box<dyn std::error::Error>> {
        let delivery = crate::credentials::CredentialDelivery::from_local_descriptor(
            "github",
            "api_key",
            "GITHUB_TOKEN",
            "ref-1",
            Vec::new(),
            "ghp_secret",
        )?;
        let secrets = delivery.secret_env();
        assert_eq!(
            substitute_secrets("Bearer ${secret:GITHUB_TOKEN}", secrets)?,
            "Bearer ghp_secret"
        );
        assert!(
            substitute_secrets("Bearer ${secret:MISSING}", secrets).is_err(),
            "a reference to an undelivered secret must fail closed"
        );
        Ok(())
    }

    #[test]
    fn non_2xx_is_a_failure_but_still_captures_the_body() -> Result<(), RuntimeError> {
        let transport = stub(404, "not found");
        let call = HttpCall {
            method: HttpMethod::Get,
            url: "https://api.example.test/v1/pets/none".to_owned(),
            headers: Vec::new(),
        };
        let output = execute_http_call(&transport, &call, &JsonObject::new())?;
        assert_eq!(output.status, InvocationStatus::Failure);
        assert_eq!(output.stdout, "not found");
        Ok(())
    }
}
