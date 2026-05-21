use std::fmt;

use runx_core::policy::{AuthorityKind, LocalAdmissionGrant, LocalAdmissionGrantStatus};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectAuthorityKind {
    ReadOnly,
    Constructive,
    Destructive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectGrantStatus {
    Active,
    Revoked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectGrantAuthMode {
    Oauth,
    Byo,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectGrantMaterialKind {
    NangoConnection,
    ByoCredential,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectGrantVerificationStatus {
    Pending,
    Verified,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectReadyStatus {
    Created,
    Unchanged,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectRevokeStatus {
    Revoked,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpConnectGrant {
    pub grant_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub principal_id: Option<String>,
    pub provider: String,
    pub scopes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority_kind: Option<ConnectAuthorityKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_locator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_mode: Option<ConnectGrantAuthMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material_kind: Option<ConnectGrantMaterialKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_status: Option<ConnectGrantVerificationStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<String>,
    pub status: ConnectGrantStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpConnectListResponse {
    pub grants: Vec<HttpConnectGrant>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpConnectPreprovisionRequest {
    pub provider: String,
    pub scopes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority_kind: Option<ConnectAuthorityKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_locator: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpConnectReadyResponse {
    pub status: ConnectReadyStatus,
    pub grant: HttpConnectGrant,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpConnectRevokeResponse {
    pub status: ConnectRevokeStatus,
    pub grant: HttpConnectGrant,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpConnectCredentialSchema {
    pub fields: Vec<HttpConnectCredentialField>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpConnectCredentialField {
    pub name: String,
    pub label: String,
    pub secret: bool,
    #[serde(default)]
    pub required: bool,
}

#[derive(Clone, Eq, PartialEq, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum HttpConnectStartResponse {
    Created {
        grant: HttpConnectGrant,
    },
    Unchanged {
        grant: HttpConnectGrant,
    },
    OauthRequired {
        session_id: Option<String>,
        flow_id: String,
        authorize_url: String,
        poll_after_ms: Option<u64>,
        expires_at: Option<String>,
    },
    CredentialRequired {
        session_id: String,
        flow_id: Option<String>,
        provider: String,
        auth_mode: String,
        credential_schema: HttpConnectCredentialSchema,
    },
    Unsupported {
        provider: Option<String>,
        provider_status: Option<String>,
        connect_mode: Option<String>,
        demand_id: Option<String>,
        request_count: Option<u64>,
        error: Option<String>,
    },
    ConnectUnavailable {
        provider: Option<String>,
        provider_status: Option<String>,
        connect_mode: Option<String>,
        error: Option<String>,
    },
}

impl fmt::Debug for HttpConnectStartResponse {
    // rust-style-allow: long-function because every hosted Connect start
    // response variant redacts a different secret-bearing field set.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created { grant } => formatter
                .debug_struct("Created")
                .field("grant", grant)
                .finish(),
            Self::Unchanged { grant } => formatter
                .debug_struct("Unchanged")
                .field("grant", grant)
                .finish(),
            Self::OauthRequired {
                session_id: _,
                flow_id: _,
                authorize_url: _,
                poll_after_ms,
                expires_at,
            } => formatter
                .debug_struct("OauthRequired")
                .field("session_id", &"[redacted]")
                .field("flow_id", &"[redacted]")
                .field("authorize_url", &"[redacted-url]")
                .field("poll_after_ms", poll_after_ms)
                .field("expires_at", expires_at)
                .finish(),
            Self::CredentialRequired {
                session_id: _,
                flow_id: _,
                provider,
                auth_mode,
                credential_schema,
            } => formatter
                .debug_struct("CredentialRequired")
                .field("session_id", &"[redacted]")
                .field("flow_id", &"[redacted]")
                .field("provider", provider)
                .field("auth_mode", auth_mode)
                .field("field_count", &credential_schema.fields.len())
                .finish(),
            Self::Unsupported {
                provider,
                provider_status,
                connect_mode,
                demand_id,
                request_count,
                error: _,
            } => formatter
                .debug_struct("Unsupported")
                .field("provider", provider)
                .field("provider_status", provider_status)
                .field("connect_mode", connect_mode)
                .field("demand_id", demand_id)
                .field("request_count", request_count)
                .field("error", &"[redacted]")
                .finish(),
            Self::ConnectUnavailable {
                provider,
                provider_status,
                connect_mode,
                error: _,
            } => formatter
                .debug_struct("ConnectUnavailable")
                .field("provider", provider)
                .field("provider_status", provider_status)
                .field("connect_mode", connect_mode)
                .field("error", &"[redacted]")
                .finish(),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum HttpConnectFlowResponse {
    Created {
        grant: HttpConnectGrant,
    },
    Unchanged {
        grant: HttpConnectGrant,
    },
    Pending {
        flow_id: String,
        poll_after_ms: Option<u64>,
    },
    Failed {
        flow_id: String,
        error: String,
    },
}

impl fmt::Debug for HttpConnectFlowResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created { grant } => formatter
                .debug_struct("Created")
                .field("grant", grant)
                .finish(),
            Self::Unchanged { grant } => formatter
                .debug_struct("Unchanged")
                .field("grant", grant)
                .finish(),
            Self::Pending {
                flow_id: _,
                poll_after_ms,
            } => formatter
                .debug_struct("Pending")
                .field("flow_id", &"[redacted]")
                .field("poll_after_ms", poll_after_ms)
                .finish(),
            Self::Failed {
                flow_id: _,
                error: _,
            } => formatter
                .debug_struct("Failed")
                .field("flow_id", &"[redacted]")
                .field("error", &"[redacted]")
                .finish(),
        }
    }
}

pub fn connect_grant_to_local_admission(grant: &HttpConnectGrant) -> LocalAdmissionGrant {
    LocalAdmissionGrant {
        grant_id: grant.grant_id.clone(),
        provider: grant.provider.clone(),
        scopes: grant.scopes.clone(),
        status: Some(match grant.status {
            ConnectGrantStatus::Active => LocalAdmissionGrantStatus::Active,
            ConnectGrantStatus::Revoked => LocalAdmissionGrantStatus::Revoked,
        }),
        scope_family: grant.scope_family.clone(),
        authority_kind: grant.authority_kind.map(|kind| match kind {
            ConnectAuthorityKind::ReadOnly => AuthorityKind::ReadOnly,
            ConnectAuthorityKind::Constructive => AuthorityKind::Constructive,
            ConnectAuthorityKind::Destructive => AuthorityKind::Destructive,
        }),
        target_repo: grant.target_repo.clone(),
        target_locator: grant.target_locator.clone(),
    }
}

pub(crate) fn ready_response(
    status: ConnectReadyStatus,
    grant: HttpConnectGrant,
) -> HttpConnectReadyResponse {
    HttpConnectReadyResponse { status, grant }
}
