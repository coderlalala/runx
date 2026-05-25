//! Policy-proof contracts: the authority-proof receipt envelope and its two
//! standalone companions (`credential-envelope`, `scope-admission`).
//!
//! These carry the legacy bare `runx.ai/spec` `$id` (no `x-runx-schema`). They
//! are produced by the local policy engine and guarded as wire contracts; their
//! authoritative Rust shape lives here so the schema-wire-compat gate can cover
//! them.
use serde::{Deserialize, Serialize};

use crate::schema::RunxSchema;

/// The kind of authority a grant or request carries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityKind {
    ReadOnly,
    Constructive,
    Destructive,
}

/// Whether a scope-admission decision allowed or denied the request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScopeAdmissionStatus {
    Allow,
    Deny,
}

/// A scope-admission decision (`runx.ai/spec/scope-admission`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
#[runx_schema(spec_id = "https://runx.ai/spec/scope-admission.schema.json")]
pub struct ScopeAdmission {
    pub status: ScopeAdmissionStatus,
    pub requested_scopes: Vec<String>,
    pub granted_scopes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasons: Option<Vec<String>>,
    pub decision_summary: String,
}

/// The grant a credential or request descends from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct CredentialGrantReference {
    pub grant_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority_kind: Option<AuthorityKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_locator: Option<String>,
}

/// A resolved credential envelope (`runx.ai/spec/credential-envelope`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
#[runx_schema(spec_id = "https://runx.ai/spec/credential-envelope.schema.json")]
pub struct CredentialEnvelope {
    pub kind: String,
    pub grant_id: String,
    pub provider: String,
    pub auth_mode: String,
    pub material_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    pub scopes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_reference: Option<CredentialGrantReference>,
    pub material_ref: String,
}

/// The scopes/posture a skill requested before admission.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct AuthorityProofRequested {
    pub connected_auth: bool,
    pub scopes: Vec<String>,
    pub mutating: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority_kind: Option<AuthorityKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_locator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox_profile: Option<String>,
}

/// The credential-material posture recorded in an authority proof.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct AuthorityProofCredentialMaterial {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority_kind: Option<AuthorityKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_locator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_reference: Option<CredentialGrantReference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material_ref_hash: Option<String>,
}

/// The network posture inside a sandbox declaration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct AuthorityProofSandboxNetwork {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declared: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enforcement: Option<String>,
}

/// The filesystem posture inside a sandbox declaration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct AuthorityProofSandboxFilesystem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enforcement: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readonly_paths: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub writable_paths_enforced: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_tmp: Option<bool>,
}

/// The runtime enforcer posture inside a sandbox declaration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct AuthorityProofSandboxRuntime {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enforcer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The sandbox posture recorded in an authority proof.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct AuthorityProofSandbox {
    pub profile: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd_policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_enforcement: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<AuthorityProofSandboxNetwork>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filesystem: Option<AuthorityProofSandboxFilesystem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<AuthorityProofSandboxRuntime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_approved: Option<bool>,
}

/// The decision an approval gate reached, recorded in an authority proof.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct AuthorityProofApprovalDecision {
    pub gate_id: String,
    pub gate_type: String,
    pub decision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The redaction attestation recorded in an authority proof.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct AuthorityProofRedaction {
    pub status: String,
    pub secret_material: String,
    pub stdout: String,
    pub stderr: String,
    pub metadata_secret_keys: Vec<String>,
}

/// The authority proof emitted alongside a skill run
/// (`runx.ai/spec/authority-proof`): the requested posture, the scope-admission
/// decision, the credential-material posture, and the redaction attestation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, RunxSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
#[runx_schema(spec_id = "https://runx.ai/spec/authority-proof.schema.json")]
pub struct AuthorityProof {
    pub schema_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub skill_name: String,
    pub source_type: String,
    pub requested: AuthorityProofRequested,
    pub scope_admission: ScopeAdmission,
    pub credential_material: AuthorityProofCredentialMaterial,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<AuthorityProofSandbox>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_gate: Option<AuthorityProofApprovalDecision>,
    pub redaction: AuthorityProofRedaction,
}
