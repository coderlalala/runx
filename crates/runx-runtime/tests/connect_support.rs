#![allow(dead_code)]

use std::cell::RefCell;

use runx_runtime::connect::{
    ConnectAuthorityKind, ConnectError, ConnectGrantStatus, ConnectOpener, HostedHttpError,
    HostedHttpRequest, HostedHttpResponse, HostedTransport, HttpConnectGrant,
};

#[derive(Default)]
pub struct MockConnectTransport {
    responses: RefCell<Vec<HostedHttpResponse>>,
    requests: RefCell<Vec<HostedHttpRequest>>,
}

impl MockConnectTransport {
    pub fn with_json(responses: Vec<serde_json::Value>) -> Self {
        Self {
            responses: RefCell::new(
                responses
                    .into_iter()
                    .map(|value| HostedHttpResponse {
                        status: 200,
                        body: value.to_string(),
                    })
                    .collect(),
            ),
            requests: RefCell::new(Vec::new()),
        }
    }

    pub fn with_response(status: u16, body: impl Into<String>) -> Self {
        Self {
            responses: RefCell::new(vec![HostedHttpResponse {
                status,
                body: body.into(),
            }]),
            requests: RefCell::new(Vec::new()),
        }
    }

    pub fn requests(&self) -> Vec<HostedHttpRequest> {
        self.requests.borrow().clone()
    }
}

impl HostedTransport for &MockConnectTransport {
    fn send(&self, request: HostedHttpRequest) -> Result<HostedHttpResponse, HostedHttpError> {
        self.requests.borrow_mut().push(request);
        Ok(self.responses.borrow_mut().remove(0))
    }
}

#[derive(Default)]
pub struct RecordingOpener {
    opened: RefCell<Vec<String>>,
}

impl RecordingOpener {
    pub fn opened(&self) -> Vec<String> {
        self.opened.borrow().clone()
    }
}

impl ConnectOpener for &RecordingOpener {
    fn open(&self, url: &str) -> Result<(), ConnectError> {
        self.opened.borrow_mut().push(url.to_owned());
        Ok(())
    }
}

pub struct FailingOpener;

impl ConnectOpener for FailingOpener {
    fn open(&self, url: &str) -> Result<(), ConnectError> {
        Err(ConnectError::OpenerFailed {
            message: format!("failed to open {url}"),
        })
    }
}

pub fn grant_fixture(id: &str) -> HttpConnectGrant {
    HttpConnectGrant {
        grant_id: id.to_owned(),
        principal_id: Some("principal_1".to_owned()),
        provider: "github".to_owned(),
        scopes: vec!["repo:read".to_owned()],
        scope_family: Some("github_repo".to_owned()),
        authority_kind: Some(ConnectAuthorityKind::ReadOnly),
        target_repo: Some("runxhq/aster".to_owned()),
        target_locator: Some("github:repo:runxhq/aster".to_owned()),
        connection_id: Some("conn_1".to_owned()),
        status: ConnectGrantStatus::Active,
        created_at: Some("2026-05-19T00:00:00Z".to_owned()),
    }
}
