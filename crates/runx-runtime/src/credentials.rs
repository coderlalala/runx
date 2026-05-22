use std::collections::BTreeMap;
use std::fmt;

use runx_core::policy::{CredentialBindingDecision, CredentialEnvelope};
use thiserror::Error;

const REDACTED_CREDENTIAL: &str = "[redacted-credential]";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialDeliveryProfile {
    provider: String,
    auth_mode: String,
    env_bindings: Vec<CredentialEnvBinding>,
}

impl CredentialDeliveryProfile {
    pub fn env_token(
        provider: impl Into<String>,
        auth_mode: impl Into<String>,
        env_var: impl Into<String>,
    ) -> Result<Self, CredentialDeliveryError> {
        let env_var = env_var.into();
        validate_env_name(&env_var)?;
        Ok(Self {
            provider: provider.into(),
            auth_mode: auth_mode.into(),
            env_bindings: vec![CredentialEnvBinding {
                role: CredentialMaterialRole::AccessToken,
                env_var,
                required: true,
            }],
        })
    }

    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    #[must_use]
    pub fn auth_mode(&self) -> &str {
        &self.auth_mode
    }

    pub fn from_contract_profile(
        profile: &runx_contracts::CredentialDeliveryProfile,
    ) -> Result<Self, CredentialDeliveryError> {
        if profile.delivery_mode != runx_contracts::CredentialDeliveryMode::ProcessEnv {
            return Err(CredentialDeliveryError::UnsupportedDeliveryMode {
                mode: format!("{:?}", profile.delivery_mode),
            });
        }
        let mut env_bindings = Vec::with_capacity(profile.env_bindings.len());
        for binding in &profile.env_bindings {
            let role = match CredentialMaterialRole::from_contract_role(binding.role.clone()) {
                Ok(role) => role,
                Err(_) if !binding.required => continue,
                Err(error) => return Err(error),
            };
            validate_env_name(&binding.env_var)?;
            env_bindings.push(CredentialEnvBinding {
                role,
                env_var: binding.env_var.clone(),
                required: binding.required,
            });
        }
        Ok(Self {
            provider: profile.provider.clone(),
            auth_mode: profile.auth_mode.clone(),
            env_bindings,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CredentialEnvBinding {
    role: CredentialMaterialRole,
    env_var: String,
    required: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CredentialMaterialRole {
    AccessToken,
}

impl CredentialMaterialRole {
    const fn label(self) -> &'static str {
        match self {
            Self::AccessToken => "access_token",
        }
    }

    fn from_contract_role(
        role: runx_contracts::CredentialMaterialRole,
    ) -> Result<Self, CredentialDeliveryError> {
        match role {
            runx_contracts::CredentialMaterialRole::AccessToken => Ok(Self::AccessToken),
            _ => Err(CredentialDeliveryError::UnsupportedMaterialRole {
                role: format!("{role:?}"),
            }),
        }
    }
}

pub trait MaterialResolver {
    fn resolve_material(
        &self,
        material_ref: &str,
    ) -> Result<ResolvedCredentialMaterial, CredentialDeliveryError>;
}

#[derive(Clone, Debug, Default)]
pub struct InMemoryMaterialResolver {
    materials: BTreeMap<String, ResolvedCredentialMaterial>,
}

impl InMemoryMaterialResolver {
    #[must_use]
    pub fn with_material(
        material_ref: impl Into<String>,
        material: ResolvedCredentialMaterial,
    ) -> Self {
        let mut materials = BTreeMap::new();
        materials.insert(material_ref.into(), material);
        Self { materials }
    }
}

impl MaterialResolver for InMemoryMaterialResolver {
    fn resolve_material(
        &self,
        material_ref: &str,
    ) -> Result<ResolvedCredentialMaterial, CredentialDeliveryError> {
        self.materials.get(material_ref).cloned().ok_or_else(|| {
            CredentialDeliveryError::MaterialNotFound {
                material_ref: material_ref.to_owned(),
            }
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedCredentialMaterial {
    material_ref: String,
    values: BTreeMap<CredentialMaterialRole, SecretString>,
}

impl ResolvedCredentialMaterial {
    #[must_use]
    pub fn access_token(material_ref: impl Into<String>, value: impl Into<String>) -> Self {
        let mut values = BTreeMap::new();
        values.insert(
            CredentialMaterialRole::AccessToken,
            SecretString::new(value),
        );
        Self {
            material_ref: material_ref.into(),
            values,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SecretString(String);

impl SecretString {
    fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED_CREDENTIAL)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SecretEnv {
    values: BTreeMap<String, SecretString>,
}

impl SecretEnv {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(SecretString::expose)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values
            .iter()
            .map(|(key, value)| (key.as_str(), value.expose()))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CredentialDelivery {
    secret_env: SecretEnv,
    public_observation: Option<runx_contracts::CredentialDeliveryObservation>,
}

impl CredentialDelivery {
    #[must_use]
    pub const fn none() -> Self {
        Self {
            secret_env: SecretEnv {
                values: BTreeMap::new(),
            },
            public_observation: None,
        }
    }

    /// Build a delivery from a one-shot, per-run local credential descriptor.
    ///
    /// This is the OSS local-provision path: no network, no persistence, no
    /// brokerage. It derives a delivery profile, a credential envelope, and an
    /// allowed binding decision purely from the supplied descriptor, resolves
    /// the secret in-memory, and routes it through the same
    /// [`Self::from_allowed_binding`] seam so policy checks and redaction stay
    /// centralized. The secret value is held only for the lifetime of this run.
    pub fn from_local_descriptor(
        provider: impl Into<String>,
        auth_mode: impl Into<String>,
        env_var: impl Into<String>,
        material_ref: impl Into<String>,
        scopes: Vec<String>,
        secret: impl Into<String>,
    ) -> Result<Self, CredentialDeliveryError> {
        let provider = provider.into();
        let auth_mode = auth_mode.into();
        let material_ref = material_ref.into();

        let profile =
            CredentialDeliveryProfile::env_token(provider.clone(), auth_mode.clone(), env_var)?;
        let envelope = CredentialEnvelope {
            kind: "runx.credential-envelope.v1".to_owned(),
            grant_id: material_ref.clone(),
            provider,
            auth_mode,
            material_kind: "access_token".to_owned(),
            connection_id: None,
            scopes,
            grant_reference: None,
            material_ref: material_ref.clone(),
        };
        let decision = CredentialBindingDecision::Allow {
            reasons: vec!["local per-run credential provision".to_owned()],
        };
        let resolver = InMemoryMaterialResolver::with_material(
            material_ref.clone(),
            ResolvedCredentialMaterial::access_token(material_ref, secret),
        );

        Self::from_allowed_binding(&decision, &envelope, &profile, &resolver)
    }

    pub fn from_allowed_binding<R: MaterialResolver>(
        decision: &CredentialBindingDecision,
        credential: &CredentialEnvelope,
        profile: &CredentialDeliveryProfile,
        resolver: &R,
    ) -> Result<Self, CredentialDeliveryError> {
        require_allowed_binding(decision)?;
        if credential.provider != profile.provider {
            return Err(CredentialDeliveryError::ProviderMismatch {
                credential_provider: credential.provider.clone(),
                profile_provider: profile.provider.clone(),
            });
        }
        let material = resolver.resolve_material(&credential.material_ref)?;
        if material.material_ref != credential.material_ref {
            return Err(CredentialDeliveryError::MaterialRefMismatch {
                expected: credential.material_ref.clone(),
                actual: material.material_ref,
            });
        }
        Ok(Self {
            secret_env: apply_profile(profile, &material)?,
            public_observation: None,
        })
    }

    #[must_use]
    pub fn secret_env(&self) -> &SecretEnv {
        &self.secret_env
    }

    #[must_use]
    pub fn with_public_observation(
        mut self,
        observation: runx_contracts::CredentialDeliveryObservation,
    ) -> Self {
        self.public_observation = Some(observation);
        self
    }

    #[must_use]
    pub fn public_observation(&self) -> Option<&runx_contracts::CredentialDeliveryObservation> {
        self.public_observation.as_ref()
    }

    #[must_use]
    pub fn credential_refs(&self) -> Option<Vec<runx_contracts::Reference>> {
        self.public_observation.as_ref().and_then(|observation| {
            (!observation.credential_refs.is_empty()).then(|| observation.credential_refs.clone())
        })
    }

    #[must_use]
    pub fn redact_text(&self, text: impl Into<String>) -> String {
        let mut redacted = text.into();
        for value in self.secret_env.values.values() {
            let secret = value.expose();
            if !secret.is_empty() {
                redacted = redacted.replace(secret, REDACTED_CREDENTIAL);
            }
        }
        redacted
    }

    #[must_use]
    pub fn redact_bytes_to_string(&self, bytes: Vec<u8>, limit_bytes: usize) -> String {
        let mut text = String::from_utf8_lossy(&bytes).into_owned();
        text = self.redact_text(text);
        truncate_utf8_string(&text, limit_bytes)
    }
}

#[derive(Debug, Error)]
pub enum CredentialDeliveryError {
    #[error("credential binding denied: {}", reasons.join("; "))]
    BindingDenied { reasons: Vec<String> },
    #[error(
        "credential provider '{credential_provider}' does not match delivery profile provider '{profile_provider}'"
    )]
    ProviderMismatch {
        credential_provider: String,
        profile_provider: String,
    },
    #[error("credential material '{material_ref}' was not found")]
    MaterialNotFound { material_ref: String },
    #[error("credential material ref mismatch: expected '{expected}', got '{actual}'")]
    MaterialRefMismatch { expected: String, actual: String },
    #[error("credential material is missing role '{role}'")]
    MissingRole { role: String },
    #[error("credential material for role '{role}' is empty")]
    EmptyMaterial { role: String },
    #[error("invalid credential delivery env var '{name}'")]
    InvalidEnvName { name: String },
    #[error("unsupported credential delivery mode '{mode}'")]
    UnsupportedDeliveryMode { mode: String },
    #[error("unsupported credential material role '{role}'")]
    UnsupportedMaterialRole { role: String },
}

fn require_allowed_binding(
    decision: &CredentialBindingDecision,
) -> Result<(), CredentialDeliveryError> {
    match decision {
        CredentialBindingDecision::Allow { .. } => Ok(()),
        CredentialBindingDecision::Deny { reasons } => {
            Err(CredentialDeliveryError::BindingDenied {
                reasons: reasons.clone(),
            })
        }
    }
}

fn apply_profile(
    profile: &CredentialDeliveryProfile,
    material: &ResolvedCredentialMaterial,
) -> Result<SecretEnv, CredentialDeliveryError> {
    let mut values = BTreeMap::new();
    for binding in &profile.env_bindings {
        let Some(secret) = material.values.get(&binding.role) else {
            if !binding.required {
                continue;
            }
            return Err(CredentialDeliveryError::MissingRole {
                role: binding.role.label().to_owned(),
            });
        };
        if secret.expose().trim().is_empty() {
            return Err(CredentialDeliveryError::EmptyMaterial {
                role: binding.role.label().to_owned(),
            });
        }
        values.insert(binding.env_var.clone(), secret.clone());
    }
    Ok(SecretEnv { values })
}

fn validate_env_name(name: &str) -> Result<(), CredentialDeliveryError> {
    let mut chars = name.chars();
    let valid = chars
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_ascii_uppercase())
        && chars.all(|ch| ch == '_' || ch.is_ascii_uppercase() || ch.is_ascii_digit());
    if valid {
        Ok(())
    } else {
        Err(CredentialDeliveryError::InvalidEnvName {
            name: name.to_owned(),
        })
    }
}

fn truncate_utf8_string(text: &str, limit_bytes: usize) -> String {
    if text.len() <= limit_bytes {
        return text.to_owned();
    }
    let mut end = limit_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_env_binding_is_skipped_when_material_role_is_missing()
    -> Result<(), CredentialDeliveryError> {
        let profile = CredentialDeliveryProfile {
            provider: "github".to_owned(),
            auth_mode: "oauth_bearer".to_owned(),
            env_bindings: vec![CredentialEnvBinding {
                role: CredentialMaterialRole::AccessToken,
                env_var: "GITHUB_TOKEN".to_owned(),
                required: false,
            }],
        };
        let material = ResolvedCredentialMaterial {
            material_ref: "secret://github/main".to_owned(),
            values: BTreeMap::new(),
        };

        let env = apply_profile(&profile, &material)?;

        assert!(env.is_empty());
        Ok(())
    }

    #[test]
    fn required_env_binding_fails_when_material_role_is_missing() {
        let profile = CredentialDeliveryProfile {
            provider: "github".to_owned(),
            auth_mode: "oauth_bearer".to_owned(),
            env_bindings: vec![CredentialEnvBinding {
                role: CredentialMaterialRole::AccessToken,
                env_var: "GITHUB_TOKEN".to_owned(),
                required: true,
            }],
        };
        let material = ResolvedCredentialMaterial {
            material_ref: "secret://github/main".to_owned(),
            values: BTreeMap::new(),
        };

        let result = apply_profile(&profile, &material);

        assert!(matches!(
            result,
            Err(CredentialDeliveryError::MissingRole { role }) if role == "access_token"
        ));
    }
}
