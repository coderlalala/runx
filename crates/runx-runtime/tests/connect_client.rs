mod connect_support;

use runx_runtime::connect::HttpMethod;
use runx_runtime::{ConnectClient, ConnectError, HttpConnectPreprovisionRequest};
use serde_json::json;

use connect_support::{MockConnectTransport, RecordingOpener, grant_fixture};

#[test]
fn connect_list_sends_authorized_get_and_parses_grants() -> Result<(), Box<dyn std::error::Error>> {
    let transport = MockConnectTransport::with_json(vec![json!({
        "grants": [grant_fixture("grant_1")]
    })]);
    let opener = RecordingOpener::default();
    let client = ConnectClient::with_transport_and_opener(
        "https://connect.example/",
        "SECRET_CONNECT_ACCESS_TOKEN_DO_NOT_LEAK",
        &transport,
        &opener,
        None,
        None,
    )?;

    let response = client.list()?;

    assert_eq!(response.grants[0].grant_id, "grant_1");
    let requests = transport.requests();
    assert_eq!(requests[0].method, HttpMethod::Get);
    assert_eq!(requests[0].url, "https://connect.example/v1/grants");
    assert!(requests[0].headers.iter().any(|header| {
        header.name == "authorization"
            && header.value == "Bearer SECRET_CONNECT_ACCESS_TOKEN_DO_NOT_LEAK"
    }));
    Ok(())
}

#[test]
fn connect_preprovision_created_serializes_typed_body() -> Result<(), Box<dyn std::error::Error>> {
    let transport = MockConnectTransport::with_json(vec![json!({
        "status": "created",
        "grant": grant_fixture("grant_created")
    })]);
    let opener = RecordingOpener::default();
    let client = ConnectClient::with_transport_and_opener(
        "https://connect.example",
        "token",
        &transport,
        &opener,
        None,
        None,
    )?;

    let response = client.preprovision(&HttpConnectPreprovisionRequest {
        provider: "github".to_owned(),
        scopes: vec!["repo:read".to_owned()],
        scope_family: Some("github_repo".to_owned()),
        authority_kind: Some(runx_runtime::connect::ConnectAuthorityKind::ReadOnly),
        target_repo: Some("runxhq/aster".to_owned()),
        target_locator: None,
    })?;

    assert_eq!(response.grant.grant_id, "grant_created");
    let requests = transport.requests();
    assert_eq!(requests[0].method, HttpMethod::Post);
    let body = requests[0].body.as_deref().unwrap_or_default();
    assert!(body.contains("\"provider\":\"github\""));
    assert!(body.contains("\"scope_family\":\"github_repo\""));
    Ok(())
}

#[test]
fn connect_oauth_flow_opens_then_polls_immediately() -> Result<(), Box<dyn std::error::Error>> {
    let transport = MockConnectTransport::with_json(vec![
        json!({
            "status": "oauth_required",
            "flow_id": "flow_fixture",
            "authorize_url": "https://auth.example/authorize?token=SECRET_AUTHORIZE_QUERY_DO_NOT_LEAK",
            "poll_after_ms": 0
        }),
        json!({
            "status": "pending",
            "flow_id": "flow_fixture",
            "poll_after_ms": 0
        }),
        json!({
            "status": "created",
            "grant": grant_fixture("grant_oauth")
        }),
    ]);
    let opener = RecordingOpener::default();
    let client = ConnectClient::with_transport_and_opener(
        "https://connect.example",
        "token",
        &transport,
        &opener,
        Some(0),
        Some(1000),
    )?;

    let response = client.preprovision(&HttpConnectPreprovisionRequest {
        provider: "github".to_owned(),
        scopes: vec![],
        scope_family: None,
        authority_kind: None,
        target_repo: None,
        target_locator: None,
    })?;

    assert_eq!(response.grant.grant_id, "grant_oauth");
    assert_eq!(opener.opened().len(), 1);
    let requests = transport.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[0].url,
        "https://connect.example/v1/connect/sessions"
    );
    assert_eq!(
        requests[1].url,
        "https://connect.example/v1/connect/sessions/flow_fixture"
    );
    Ok(())
}

#[test]
fn connect_revoke_rejects_active_or_unknown_status() -> Result<(), Box<dyn std::error::Error>> {
    let active = MockConnectTransport::with_json(vec![json!({
        "status": "active",
        "grant": grant_fixture("grant_active")
    })]);
    let opener = RecordingOpener::default();
    let client = ConnectClient::with_transport_and_opener(
        "https://connect.example",
        "token",
        &active,
        &opener,
        None,
        None,
    )?;
    assert!(matches!(
        client.revoke("grant_active"),
        Err(ConnectError::Contract { .. })
    ));

    let unknown = MockConnectTransport::with_json(vec![json!({
        "status": "deleted",
        "grant": grant_fixture("grant_deleted")
    })]);
    let client = ConnectClient::with_transport_and_opener(
        "https://connect.example",
        "token",
        &unknown,
        &opener,
        None,
        None,
    )?;
    assert!(matches!(
        client.revoke("grant_deleted"),
        Err(ConnectError::Contract { .. })
    ));
    Ok(())
}

#[test]
fn connect_start_rejects_unmodeled_status_without_fallback()
-> Result<(), Box<dyn std::error::Error>> {
    let transport = MockConnectTransport::with_json(vec![json!({
        "status": "connect_unavailable",
        "error": "try device flow"
    })]);
    let opener = RecordingOpener::default();
    let client = ConnectClient::with_transport_and_opener(
        "https://connect.example",
        "token",
        &transport,
        &opener,
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
        Ok(_) => return Err("unmodeled status should fail".into()),
        Err(error) => error,
    };

    assert!(matches!(error, ConnectError::UnsupportedStatus { .. }));
    Ok(())
}
