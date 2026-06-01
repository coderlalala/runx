use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use ring::signature::Ed25519KeyPair;
use runx_contracts::sha256_prefixed;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

pub const PAYMENT_ADMISSION_PURPOSE: &str = "runx.payment_admission.v1";
pub const PAYMENT_ADMISSION_AUDIENCE: &str = "rail_settlement";
pub const MONEY_MOVEMENT_DOMAIN: &str = "runx.money_movement.v1";
pub const PAYMENT_ADMISSION_SIGNATURE_BASE64_PREFIX: &str = "base64:";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentAdmissionRequest {
    pub principal: String,
    pub act: String,
    pub rail: String,
    pub amount_minor: u64,
    pub currency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counterparty: Option<String>,
    pub run_id: String,
    pub authority_digest: String,
    pub expires_at: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentAdmissionToken {
    pub purpose: String,
    pub audience: String,
    pub principal: String,
    pub act: String,
    pub rail: String,
    pub amount_minor: u64,
    pub currency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counterparty: Option<String>,
    pub run_id: String,
    pub authority_digest: String,
    pub expires_at: String,
    pub money_movement_id: String,
    pub kid: String,
    pub sig: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentAdmissionIssueResponse {
    pub token: PaymentAdmissionToken,
    pub token_canonical_json: String,
    pub token_digest: String,
    pub money_movement_id: String,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PaymentAdmissionError {
    #[error("payment admission signer key id is missing")]
    MissingKeyId,
    #[error("payment admission signer key material is malformed")]
    MalformedSignerKey,
    #[error("payment admission canonical JSON serialization failed")]
    CanonicalJson,
    #[error("payment admission request field {0} is empty")]
    EmptyField(&'static str),
}

#[derive(Debug)]
pub struct PaymentAdmissionSigner {
    kid: String,
    key_pair: Ed25519KeyPair,
}

impl PaymentAdmissionSigner {
    pub fn from_seed_base64(
        kid: impl Into<String>,
        seed: &str,
    ) -> Result<Self, PaymentAdmissionError> {
        let kid = kid.into();
        if kid.trim().is_empty() {
            return Err(PaymentAdmissionError::MissingKeyId);
        }
        let seed = STANDARD
            .decode(seed)
            .map_err(|_| PaymentAdmissionError::MalformedSignerKey)?;
        let key_pair = Ed25519KeyPair::from_seed_unchecked(&seed)
            .map_err(|_| PaymentAdmissionError::MalformedSignerKey)?;
        Ok(Self { kid, key_pair })
    }

    pub fn issue(
        &self,
        request: &PaymentAdmissionRequest,
    ) -> Result<PaymentAdmissionIssueResponse, PaymentAdmissionError> {
        validate_request(request)?;
        let money_movement_id = derive_money_movement_id(request)?;
        let mut unsigned = token_payload_without_signature(request, &money_movement_id, &self.kid);
        let canonical_unsigned = canonical_json(&Value::Object(unsigned.clone()))?;
        let signature = self.key_pair.sign(canonical_unsigned.as_bytes());
        unsigned.insert(
            "sig".to_owned(),
            Value::String(format!(
                "{PAYMENT_ADMISSION_SIGNATURE_BASE64_PREFIX}{}",
                URL_SAFE_NO_PAD.encode(signature.as_ref())
            )),
        );
        let token: PaymentAdmissionToken = serde_json::from_value(Value::Object(unsigned))
            .map_err(|_| PaymentAdmissionError::CanonicalJson)?;
        let token_canonical_json = payment_admission_token_canonical_json(&token)?;
        let token_digest = payment_admission_token_digest(&token)?;
        Ok(PaymentAdmissionIssueResponse {
            token,
            token_canonical_json,
            token_digest,
            money_movement_id,
        })
    }
}

pub fn derive_money_movement_id(
    request: &PaymentAdmissionRequest,
) -> Result<String, PaymentAdmissionError> {
    validate_request(request)?;
    let preimage = stable_money_movement_preimage(request);
    let canonical = canonical_json(&Value::Object(preimage))?;
    Ok(sha256_prefixed(
        format!("{MONEY_MOVEMENT_DOMAIN}\n{canonical}").as_bytes(),
    ))
}

pub fn payment_admission_token_canonical_json(
    token: &PaymentAdmissionToken,
) -> Result<String, PaymentAdmissionError> {
    canonical_json(&serde_json::to_value(token).map_err(|_| PaymentAdmissionError::CanonicalJson)?)
}

pub fn payment_admission_token_digest(
    token: &PaymentAdmissionToken,
) -> Result<String, PaymentAdmissionError> {
    let canonical = payment_admission_token_canonical_json(token)?;
    Ok(sha256_prefixed(canonical.as_bytes()))
}

fn stable_money_movement_preimage(request: &PaymentAdmissionRequest) -> Map<String, Value> {
    let mut payload = Map::new();
    payload.insert("act".to_owned(), Value::String(request.act.clone()));
    payload.insert(
        "amount_minor".to_owned(),
        Value::Number(request.amount_minor.into()),
    );
    if let Some(counterparty) = &request.counterparty {
        payload.insert(
            "counterparty".to_owned(),
            Value::String(counterparty.clone()),
        );
    }
    payload.insert(
        "authority_digest".to_owned(),
        Value::String(request.authority_digest.clone()),
    );
    payload.insert(
        "currency".to_owned(),
        Value::String(request.currency.clone()),
    );
    payload.insert(
        "principal".to_owned(),
        Value::String(request.principal.clone()),
    );
    payload.insert("rail".to_owned(), Value::String(request.rail.clone()));
    payload.insert("run_id".to_owned(), Value::String(request.run_id.clone()));
    payload
}

fn token_payload_without_signature(
    request: &PaymentAdmissionRequest,
    money_movement_id: &str,
    kid: &str,
) -> Map<String, Value> {
    let mut payload = stable_money_movement_preimage(request);
    payload.insert(
        "audience".to_owned(),
        Value::String(PAYMENT_ADMISSION_AUDIENCE.to_owned()),
    );
    payload.insert(
        "expires_at".to_owned(),
        Value::String(request.expires_at.clone()),
    );
    payload.insert("kid".to_owned(), Value::String(kid.to_owned()));
    payload.insert(
        "money_movement_id".to_owned(),
        Value::String(money_movement_id.to_owned()),
    );
    payload.insert(
        "purpose".to_owned(),
        Value::String(PAYMENT_ADMISSION_PURPOSE.to_owned()),
    );
    payload
}

fn canonical_json(value: &Value) -> Result<String, PaymentAdmissionError> {
    serde_json::to_string(value).map_err(|_| PaymentAdmissionError::CanonicalJson)
}

fn validate_request(request: &PaymentAdmissionRequest) -> Result<(), PaymentAdmissionError> {
    require_non_empty("principal", &request.principal)?;
    require_non_empty("act", &request.act)?;
    require_non_empty("rail", &request.rail)?;
    require_non_empty("currency", &request.currency)?;
    if let Some(counterparty) = &request.counterparty {
        require_non_empty("counterparty", counterparty)?;
    }
    require_non_empty("run_id", &request.run_id)?;
    require_non_empty("authority_digest", &request.authority_digest)?;
    require_non_empty("expires_at", &request.expires_at)
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), PaymentAdmissionError> {
    if value.trim().is_empty() {
        return Err(PaymentAdmissionError::EmptyField(field));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SEED_BASE64: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

    fn request(expires_at: &str) -> PaymentAdmissionRequest {
        PaymentAdmissionRequest {
            principal: "principal_1".to_owned(),
            act: "act_pay_quote".to_owned(),
            rail: "x402".to_owned(),
            amount_minor: 1250,
            currency: "USD".to_owned(),
            counterparty: Some("merchant_1".to_owned()),
            run_id: "run_1".to_owned(),
            authority_digest: "sha256:authority".to_owned(),
            expires_at: expires_at.to_owned(),
        }
    }

    #[test]
    fn token_refresh_keeps_money_movement_id_stable() -> Result<(), PaymentAdmissionError> {
        let signer = PaymentAdmissionSigner::from_seed_base64("kid-admission-1", TEST_SEED_BASE64)?;
        let first = signer.issue(&request("2026-06-01T00:05:00Z"))?;
        let refreshed = signer.issue(&request("2026-06-01T00:10:00Z"))?;

        assert_eq!(first.money_movement_id, refreshed.money_movement_id);
        assert_ne!(first.token.expires_at, refreshed.token.expires_at);
        assert_ne!(first.token.sig, refreshed.token.sig);
        assert_ne!(first.token_digest, refreshed.token_digest);
        Ok(())
    }

    #[test]
    fn token_canonical_json_is_byte_stable_for_fixed_input() -> Result<(), PaymentAdmissionError> {
        let signer = PaymentAdmissionSigner::from_seed_base64("kid-admission-1", TEST_SEED_BASE64)?;
        let issued = signer.issue(&request("2026-06-01T00:05:00Z"))?;

        assert_eq!(
            issued.money_movement_id,
            "sha256:b1f910b08abe1053af9343df6b0467dbea9018a9052e4601d7a4616f1f73ff33"
        );
        assert!(
            issued
                .token_canonical_json
                .contains("\"purpose\":\"runx.payment_admission.v1\"")
        );
        assert_eq!(issued.token.money_movement_id, issued.money_movement_id);
        assert_eq!(
            issued.token_digest,
            sha256_prefixed(issued.token_canonical_json.as_bytes())
        );
        Ok(())
    }
}
