use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};
use runx_contracts::{
    ReceiptIssuer, ReceiptIssuerType, ReceiptSignature, SignatureAlgorithm, sha256_prefixed,
};
use runx_receipts::{SignatureVerificationFailure, SignatureVerifier};
use thiserror::Error;

pub const RECEIPT_SIGNATURE_BASE64_PREFIX: &str = "base64:";

pub trait RuntimeReceiptSigner {
    fn issuer(&self) -> ReceiptIssuer;
    fn sign_receipt_body(
        &self,
        body_digest: &str,
    ) -> Result<ReceiptSignature, RuntimeReceiptSigningError>;
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum RuntimeReceiptSigningError {
    #[error("production receipt signing requires a signer")]
    MissingSigner,
    #[error("production receipt signing requires a verifier")]
    MissingVerifier,
    #[error("production receipt signer key id is missing")]
    MissingKeyId,
    #[error("production receipt signer public key hash is missing")]
    MissingPublicKeySha256,
    #[error("production receipt signer public key hash is malformed")]
    MalformedPublicKeySha256,
    #[error("production receipt signer returned an unsupported signature algorithm")]
    UnsupportedAlgorithm,
    #[error("production receipt signer returned a local pseudo signature")]
    PseudoSignature,
    #[error("production receipt signer key material is malformed")]
    MalformedSignerKey,
    #[error("production receipt verifier key material is malformed")]
    MalformedVerifierKey,
    #[error("production receipt signature did not verify: {0:?}")]
    SignatureVerification(SignatureVerificationFailure),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductionReceiptKey {
    kid: String,
    public_key: Vec<u8>,
}

impl ProductionReceiptKey {
    #[must_use]
    pub fn new(kid: impl Into<String>, public_key: impl Into<Vec<u8>>) -> Self {
        Self {
            kid: kid.into(),
            public_key: public_key.into(),
        }
    }

    #[must_use]
    pub fn kid(&self) -> &str {
        &self.kid
    }

    #[must_use]
    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }

    #[must_use]
    pub fn public_key_sha256(&self) -> String {
        sha256_prefixed(&self.public_key)
    }
}

pub struct Ed25519ReceiptSigner {
    issuer: ReceiptIssuer,
    key_pair: Ed25519KeyPair,
}

impl Ed25519ReceiptSigner {
    pub fn from_seed(
        kid: impl Into<String>,
        issuer_type: ReceiptIssuerType,
        seed: &[u8],
    ) -> Result<Self, RuntimeReceiptSigningError> {
        let key_pair = Ed25519KeyPair::from_seed_unchecked(seed)
            .map_err(|_| RuntimeReceiptSigningError::MalformedSignerKey)?;
        let kid = kid.into();
        let issuer = ReceiptIssuer {
            issuer_type,
            kid,
            public_key_sha256: sha256_prefixed(key_pair.public_key().as_ref()),
        };
        validate_production_issuer(&issuer)?;
        Ok(Self { issuer, key_pair })
    }

    pub fn from_seed_base64(
        kid: impl Into<String>,
        issuer_type: ReceiptIssuerType,
        seed: &str,
    ) -> Result<Self, RuntimeReceiptSigningError> {
        let seed = decode_key_material(seed)
            .map_err(|_| RuntimeReceiptSigningError::MalformedSignerKey)?;
        Self::from_seed(kid, issuer_type, &seed)
    }

    #[must_use]
    pub fn production_key(&self) -> ProductionReceiptKey {
        ProductionReceiptKey::new(
            self.issuer.kid.clone(),
            self.key_pair.public_key().as_ref().to_vec(),
        )
    }

    #[must_use]
    pub fn public_key(&self) -> &[u8] {
        self.key_pair.public_key().as_ref()
    }
}

impl RuntimeReceiptSigner for Ed25519ReceiptSigner {
    fn issuer(&self) -> ReceiptIssuer {
        self.issuer.clone()
    }

    fn sign_receipt_body(
        &self,
        body_digest: &str,
    ) -> Result<ReceiptSignature, RuntimeReceiptSigningError> {
        let signature = self.key_pair.sign(body_digest.as_bytes());
        Ok(ReceiptSignature {
            alg: SignatureAlgorithm::Ed25519,
            value: format!(
                "{RECEIPT_SIGNATURE_BASE64_PREFIX}{}",
                URL_SAFE_NO_PAD.encode(signature.as_ref())
            ),
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ed25519ReceiptVerifier {
    keys: Vec<ProductionReceiptKey>,
}

impl Ed25519ReceiptVerifier {
    #[must_use]
    pub fn new(keys: impl IntoIterator<Item = ProductionReceiptKey>) -> Self {
        Self {
            keys: keys.into_iter().collect(),
        }
    }

    #[must_use]
    pub fn from_public_key(kid: impl Into<String>, public_key: impl Into<Vec<u8>>) -> Self {
        Self::new([ProductionReceiptKey::new(kid, public_key)])
    }

    pub fn from_public_key_base64(
        kid: impl Into<String>,
        public_key: &str,
    ) -> Result<Self, RuntimeReceiptSigningError> {
        let public_key = decode_key_material(public_key)
            .map_err(|_| RuntimeReceiptSigningError::MalformedVerifierKey)?;
        Ok(Self::from_public_key(kid, public_key))
    }

    #[must_use]
    pub fn keys(&self) -> &[ProductionReceiptKey] {
        &self.keys
    }

    fn resolve_key(
        &self,
        issuer: &ReceiptIssuer,
    ) -> Result<&ProductionReceiptKey, SignatureVerificationFailure> {
        if issuer.kid.trim().is_empty() {
            return Err(SignatureVerificationFailure::MissingKey);
        }
        self.keys
            .iter()
            .find(|key| key.kid == issuer.kid)
            .ok_or(SignatureVerificationFailure::MissingKey)
    }
}

impl SignatureVerifier for Ed25519ReceiptVerifier {
    fn verify(
        &self,
        issuer: &ReceiptIssuer,
        signature: &ReceiptSignature,
        body_digest: &str,
    ) -> Result<(), SignatureVerificationFailure> {
        let key = self.resolve_key(issuer)?;
        if key.public_key.len() != 32 {
            return Err(SignatureVerificationFailure::MalformedKey);
        }
        if issuer.public_key_sha256 != key.public_key_sha256() {
            return Err(SignatureVerificationFailure::KeyHashMismatch);
        }
        let signature_bytes = decode_signature_value(&signature.value)?;
        if signature_bytes.len() != 64 {
            return Err(SignatureVerificationFailure::MalformedSignature);
        }
        UnparsedPublicKey::new(&ED25519, &key.public_key)
            .verify(body_digest.as_bytes(), &signature_bytes)
            .map_err(|_| SignatureVerificationFailure::SignatureMismatch)
    }
}

pub(crate) fn validate_production_issuer(
    issuer: &ReceiptIssuer,
) -> Result<(), RuntimeReceiptSigningError> {
    if issuer.kid.trim().is_empty() {
        return Err(RuntimeReceiptSigningError::MissingKeyId);
    }
    if issuer.public_key_sha256.trim().is_empty() {
        return Err(RuntimeReceiptSigningError::MissingPublicKeySha256);
    }
    if !is_well_formed_sha256(&issuer.public_key_sha256) {
        return Err(RuntimeReceiptSigningError::MalformedPublicKeySha256);
    }
    Ok(())
}

pub(crate) fn is_local_pseudo_signature(value: &str) -> bool {
    value.starts_with("sig:")
}

fn decode_signature_value(value: &str) -> Result<Vec<u8>, SignatureVerificationFailure> {
    let Some(encoded) = value.strip_prefix(RECEIPT_SIGNATURE_BASE64_PREFIX) else {
        return Err(SignatureVerificationFailure::MalformedSignature);
    };
    URL_SAFE_NO_PAD
        .decode(encoded)
        .or_else(|_| STANDARD.decode(encoded))
        .map_err(|_| SignatureVerificationFailure::MalformedSignature)
}

fn decode_key_material(value: &str) -> Result<Vec<u8>, ()> {
    URL_SAFE_NO_PAD
        .decode(value)
        .or_else(|_| STANDARD.decode(value))
        .map_err(|_| ())
}

fn is_well_formed_sha256(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}
