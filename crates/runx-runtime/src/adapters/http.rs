//! Governed HTTP execution on the runtime HTTP transport.
//!
//! The keystone call-out front. Given a method, URL, and inputs, this builds a
//! request, sends it through the governed `runtime_http` transport (which enforces
//! SSRF and private-network filtering, header validation, no-redirect, SSL, and
//! timeouts), and maps the response to the universal [`SkillOutput`]. GET and DELETE
//! map inputs to the query string; POST maps them to a JSON body. It reuses the same
//! transport the Anthropic resolver and the registry client use, so there is one
//! governed HTTP path, not a parallel one.

use runx_contracts::{JsonObject, JsonValue};
use serde_json::Value as WireValue;

use crate::RuntimeError;
use crate::adapter::{InvocationStatus, SkillOutput};
use crate::runtime_http::{HttpMethod, RuntimeHttpHeader, RuntimeHttpRequest, RuntimeHttpTransport};

const HTTP_SKILL: &str = "http";

/// A governed HTTP call: a method and a URL. Inputs are mapped to the query string
/// (GET, DELETE) or a JSON body (POST).
#[derive(Clone, Debug)]
pub struct HttpCall {
    pub method: HttpMethod,
    pub url: String,
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
    let (url, body, headers) = match call.method {
        HttpMethod::Post => (
            call.url.clone(),
            Some(json_body(inputs)?),
            vec![RuntimeHttpHeader::new("content-type", "application/json")],
        ),
        HttpMethod::Get | HttpMethod::Delete => (with_query(&call.url, inputs), None, Vec::new()),
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
    fn get_maps_inputs_to_query_and_seals_the_response() {
        let transport = stub(200, r#"{"ok":true}"#);
        let call = HttpCall {
            method: HttpMethod::Get,
            url: "https://api.example.test/v1/pets".to_owned(),
        };
        let output = execute_http_call(&transport, &call, &inputs(&[("id", "p-7")]))
            .expect("the call should produce output");
        assert_eq!(output.status, InvocationStatus::Success);
        assert_eq!(output.stdout, r#"{"ok":true}"#);
        let sent = transport.requests.borrow();
        assert!(
            sent.len() == 1 && sent[0].url.contains("id=p-7") && sent[0].body.is_none(),
            "GET inputs must go on the query string with no body; got: {:?}",
            sent.first()
        );
    }

    #[test]
    fn post_maps_inputs_to_a_json_body() {
        let transport = stub(201, "");
        let call = HttpCall {
            method: HttpMethod::Post,
            url: "https://api.example.test/v1/pets".to_owned(),
        };
        execute_http_call(&transport, &call, &inputs(&[("name", "rex")]))
            .expect("the call should produce output");
        let sent = transport.requests.borrow();
        assert!(
            sent[0]
                .body
                .as_deref()
                .is_some_and(|body| body.contains(r#""name":"rex""#)),
            "POST inputs must go in the JSON body; got: {:?}",
            sent[0].body
        );
    }

    #[test]
    fn non_2xx_is_a_failure_but_still_captures_the_body() {
        let transport = stub(404, "not found");
        let call = HttpCall {
            method: HttpMethod::Get,
            url: "https://api.example.test/v1/pets/none".to_owned(),
        };
        let output = execute_http_call(&transport, &call, &JsonObject::new())
            .expect("a non-2xx response is captured, not an error");
        assert_eq!(output.status, InvocationStatus::Failure);
        assert_eq!(output.stdout, "not found");
    }
}
