mod connect_support;

use runx_runtime::{ConnectClient, ConnectError, HttpConnectPreprovisionRequest};
use serde_json::json;

use connect_support::{FailingOpener, MockConnectTransport, RecordingOpener};

#[test]
fn connect_errors_do_not_leak_cloud_body_or_bearer_values() -> Result<(), Box<dyn std::error::Error>>
{
    let transport = MockConnectTransport::with_response(
        500,
        "SECRET_CREDENTIAL_BODY_DO_NOT_LEAK bearer Bearer abc.def",
    );
    let opener = RecordingOpener::default();
    let client = ConnectClient::with_transport_and_opener(
        "https://connect.example",
        "SECRET_CONNECT_ACCESS_TOKEN_DO_NOT_LEAK",
        &transport,
        &opener,
        None,
        None,
    )?;

    let error = match client.list() {
        Ok(_) => return Err("http failure should fail".into()),
        Err(error) => error,
    };
    let display = error.to_string();

    assert!(!display.contains("SECRET_CREDENTIAL_BODY_DO_NOT_LEAK"));
    assert!(!display.contains("abc.def"));
    assert!(display.contains("byte response body"));
    Ok(())
}

#[test]
fn opener_failure_redacts_authorize_url_and_timeout_hides_flow_id()
-> Result<(), Box<dyn std::error::Error>> {
    let opener_failure = MockConnectTransport::with_json(vec![json!({
        "status": "oauth_required",
        "flow_id": "flow_SECRET_FLOW_ID_DO_NOT_LEAK",
        "authorize_url": "https://auth.example/authorize?code=SECRET_AUTHORIZE_QUERY_DO_NOT_LEAK",
        "poll_after_ms": 0
    })]);
    let client = ConnectClient::with_transport_and_opener(
        "https://connect.example",
        "token",
        &opener_failure,
        FailingOpener,
        None,
        None,
    )?;
    let error = match client.preprovision(&HttpConnectPreprovisionRequest {
        provider: "github".to_owned(),
        scopes: vec![],
        scope_family: None,
        authority_kind: None,
        target_repo: None,
        target_locator: None,
    }) {
        Ok(_) => return Err("opener failure should fail".into()),
        Err(error) => error,
    };
    let display = error.to_string();
    assert!(!display.contains("SECRET_AUTHORIZE_QUERY_DO_NOT_LEAK"));
    assert!(!display.contains("https://auth.example"));

    let timeout = MockConnectTransport::with_json(vec![
        json!({
            "status": "oauth_required",
            "flow_id": "flow_SECRET_FLOW_ID_DO_NOT_LEAK",
            "authorize_url": "https://auth.example/authorize",
            "poll_after_ms": 0
        }),
        json!({
            "status": "pending",
            "flow_id": "flow_SECRET_FLOW_ID_DO_NOT_LEAK",
            "poll_after_ms": 0
        }),
    ]);
    let opener = RecordingOpener::default();
    let client = ConnectClient::with_transport_and_opener(
        "https://connect.example",
        "token",
        &timeout,
        &opener,
        Some(0),
        Some(0),
    )?;
    let error = match client.preprovision(&HttpConnectPreprovisionRequest {
        provider: "github".to_owned(),
        scopes: vec![],
        scope_family: None,
        authority_kind: None,
        target_repo: None,
        target_locator: None,
    }) {
        Ok(_) => return Err("timeout should fail".into()),
        Err(error) => error,
    };
    assert!(!error.to_string().contains("SECRET_FLOW_ID_DO_NOT_LEAK"));
    Ok(())
}

#[test]
fn hosted_response_debug_does_not_leak_body() {
    let response = runx_runtime::connect::HostedHttpResponse {
        status: 500,
        body: "SECRET_CREDENTIAL_BODY_DO_NOT_LEAK".to_owned(),
    };
    let debug = format!("{response:?}");

    assert!(!debug.contains("SECRET_CREDENTIAL_BODY_DO_NOT_LEAK"));
    assert!(debug.contains("bytes"));
}

#[test]
fn flow_failed_error_is_redacted() -> Result<(), Box<dyn std::error::Error>> {
    let transport = MockConnectTransport::with_json(vec![
        json!({
            "status": "oauth_required",
            "flow_id": "flow_1",
            "authorize_url": "https://auth.example/authorize",
            "poll_after_ms": 0
        }),
        json!({
            "status": "failed",
            "flow_id": "flow_1",
            "error": "SECRET_CLOUD_ERROR_DO_NOT_LEAK"
        }),
    ]);
    let opener = RecordingOpener::default();
    let client = ConnectClient::with_transport_and_opener(
        "https://connect.example",
        "token",
        &transport,
        &opener,
        Some(0),
        None,
    )?;

    let error = match client.preprovision(&HttpConnectPreprovisionRequest {
        provider: "github".to_owned(),
        scopes: vec![],
        scope_family: None,
        authority_kind: None,
        target_repo: None,
        target_locator: None,
    }) {
        Ok(_) => return Err("failed flow should fail".into()),
        Err(error) => error,
    };

    assert!(!error.to_string().contains("SECRET_CLOUD_ERROR_DO_NOT_LEAK"));
    assert!(matches!(error, ConnectError::FlowFailed { .. }));
    Ok(())
}

#[test]
fn poll_route_errors_do_not_leak_flow_id() -> Result<(), Box<dyn std::error::Error>> {
    let transport = MockConnectTransport::with_json(vec![
        json!({
            "status": "oauth_required",
            "flow_id": "flow_SECRET_FLOW_ID_DO_NOT_LEAK",
            "authorize_url": "https://auth.example/authorize",
            "poll_after_ms": 0
        }),
        json!({
            "status": "connect_unavailable",
            "flow_id": "flow_SECRET_FLOW_ID_DO_NOT_LEAK"
        }),
    ]);
    let opener = RecordingOpener::default();
    let client = ConnectClient::with_transport_and_opener(
        "https://connect.example",
        "token",
        &transport,
        &opener,
        Some(0),
        None,
    )?;

    let error = match client.preprovision(&HttpConnectPreprovisionRequest {
        provider: "github".to_owned(),
        scopes: vec![],
        scope_family: None,
        authority_kind: None,
        target_repo: None,
        target_locator: None,
    }) {
        Ok(_) => return Err("unsupported poll status should fail".into()),
        Err(error) => error,
    };

    assert!(!error.to_string().contains("SECRET_FLOW_ID_DO_NOT_LEAK"));
    assert!(error.to_string().contains("/v1/connect/sessions/[flow_id]"));
    Ok(())
}
