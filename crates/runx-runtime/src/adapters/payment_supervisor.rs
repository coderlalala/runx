use std::future::Future;
use std::pin::Pin;

#[cfg(not(feature = "async-http"))]
compile_error!("runx-runtime feature 'payment-rails' requires feature 'async-http'.");

use runx_contracts::{
    EffectSettlementPhase, EffectSettlementReceipt, EffectSettlementReceiptSchema, JsonObject,
    ProofKind, Reference, ReferenceType, sha256_prefixed,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonWireValue;
use thiserror::Error;

use crate::runtime_http::{
    HttpMethod, RuntimeHttpError, RuntimeHttpHeader, RuntimeHttpRequest, RuntimeHttpTransport,
};
use crate::{PaymentAdmissionError, PaymentAdmissionToken, payment_admission_token_digest};

pub const X402_RAIL: &str = "x402";
pub const STRIPE_SPT_RAIL: &str = "stripe-spt";
pub const MPP_FIAT_RAIL: &str = "mpp-fiat";
pub const MPP_TEMPO_RAIL: &str = "mpp-tempo";
pub const PAYMENT_RAIL_SUPERVISOR_VERIFIER_ID: &str = "runx.payment_rail_supervisor.local.v1";
pub const EXTERNAL_SIGNER_REQUEST_SCHEMA: &str = "runx.external_signer.request.v1";
pub const EXTERNAL_SIGNER_RESPONSE_SCHEMA: &str = "runx.external_signer.response.v1";
pub const MPP_PAYMENT_CHALLENGE_SCHEMA: &str = "runx.mpp.payment_challenge.v1";
pub const MPP_PAYMENT_RECEIPT_SCHEMA: &str = "runx.mpp.payment_receipt.v1";

pub type RailSettlementFuture<'a> =
    Pin<Box<dyn Future<Output = Result<RailSettlementEvidence, RailSupervisorError>> + Send + 'a>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RailSettlementRequest<'a> {
    pub rail: &'a str,
    pub counterparty: &'a str,
    pub amount_minor: u64,
    pub currency: &'a str,
    pub idempotency_key: &'a str,
    pub proof_ref: &'a str,
    pub skill_settlement_status: Option<&'a str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RailSettlementEvidence {
    pub verifier_id: String,
    pub proof_ref: String,
    pub rail: String,
    pub counterparty: String,
    pub amount_minor: u64,
    pub currency: String,
    pub idempotency_key: String,
    pub settlement_status: Option<String>,
    pub provider_event_ref: Option<String>,
    pub shared_payment_token_ref: Option<String>,
    pub admission_token_digest: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StripeSptIssuanceRequest {
    pub money_movement_id: String,
    pub admission_token_digest: String,
    pub amount_minor: u64,
    pub currency: String,
    pub counterparty: String,
    pub rail: String,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StripeSptChargeEvidence {
    pub payment_intent_id: String,
    pub charge_id: String,
    pub event_id: String,
    pub shared_payment_token_id: String,
    pub admission_token_digest: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MppStripeChargeRequest {
    pub amount: String,
    pub currency: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "externalId", default)]
    pub external_id: Option<String>,
    #[serde(default)]
    pub recipient: Option<String>,
    #[serde(rename = "methodDetails")]
    pub method_details: MppStripeMethodDetails,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MppStripeMethodDetails {
    #[serde(rename = "networkId")]
    pub network_id: String,
    #[serde(rename = "paymentMethodTypes")]
    pub payment_method_types: Vec<String>,
    #[serde(default)]
    pub metadata: Option<JsonObject>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MppStripeCredentialPayload {
    pub spt: String,
    #[serde(rename = "externalId", default)]
    pub external_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MppTempoChargeRequest {
    pub amount: String,
    pub currency: String,
    #[serde(default)]
    pub recipient: Option<String>,
    #[serde(rename = "methodDetails")]
    pub method_details: MppTempoMethodDetails,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MppTempoMethodDetails {
    #[serde(rename = "chainId", default)]
    pub chain_id: Option<u64>,
    #[serde(rename = "feePayer", default)]
    pub fee_payer: Option<bool>,
    #[serde(default)]
    pub memo: Option<String>,
    #[serde(default)]
    pub splits: Option<Vec<MppTempoSplit>>,
    #[serde(rename = "supportedModes", default)]
    pub supported_modes: Option<Vec<MppTempoSubmissionMode>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MppTempoSplit {
    pub recipient: String,
    pub amount: String,
    #[serde(default)]
    pub memo: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MppTempoSubmissionMode {
    Pull,
    Push,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MppTempoCredentialType {
    Transaction,
    Hash,
    Proof,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MppTempoCredentialPayload {
    #[serde(rename = "type")]
    pub credential_type: MppTempoCredentialType,
    #[serde(default)]
    pub signature: Option<String>,
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct X402TransferAuthorizationChallenge {
    pub chain_id: u64,
    pub token_contract: String,
    pub verifying_contract: String,
    pub from: String,
    pub pay_to: String,
    pub valid_after: String,
    pub valid_before: String,
    pub currency: String,
    pub amount_minor: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct X402TransferAuthorizationTemplate {
    pub chain_id: u64,
    pub token_contract: String,
    pub verifying_contract: String,
    pub from: String,
    pub to: String,
    pub value: u64,
    pub valid_after: String,
    pub valid_before: String,
    pub nonce: String,
    pub currency: String,
    pub amount_minor: u64,
    pub counterparty: String,
    pub run_id: String,
    pub authority_digest: String,
    pub money_movement_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalSignerRequest {
    pub schema: String,
    pub admission_token: PaymentAdmissionToken,
    pub template: X402TransferAuthorizationTemplate,
    pub template_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalSignerSignedResponse {
    pub schema: String,
    pub status: String,
    pub signer_address: String,
    pub signature: String,
    pub template_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalSignerRefusalResponse {
    pub schema: String,
    pub status: String,
    pub code: ExternalSignerRefusalCode,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalSignerRefusalCode {
    TemplateMismatch,
    ExpiredToken,
    UnsupportedChain,
    SignerUnavailable,
}

#[derive(Clone, Debug)]
pub struct ExternalSignerClient<T> {
    endpoint_url: String,
    transport: T,
}

#[derive(Clone, Debug)]
pub struct X402FacilitatorClient<T> {
    base_url: String,
    transport: T,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct X402FacilitatorPayment {
    pub payment_signature: String,
    pub template_digest: String,
    pub money_movement_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct X402FacilitatorSettlement {
    pub tx_hash: String,
    pub log: JsonWireValue,
}

#[derive(Clone, Debug)]
pub struct EffectSettlementReceiptInput {
    pub created_at: String,
    pub phase: EffectSettlementPhase,
    pub original_receipt_ref: Reference,
    pub criterion_id: String,
    pub proof_ref: Option<Reference>,
    pub evidence_refs: Vec<Reference>,
    pub confirmation_depth: Option<u64>,
    pub payload: JsonObject,
}

impl<T> X402FacilitatorClient<T> {
    #[must_use]
    pub fn with_transport(base_url: impl AsRef<str>, transport: T) -> Self {
        Self {
            base_url: base_url.as_ref().trim_end_matches('/').to_owned(),
            transport,
        }
    }
}

impl<T> X402FacilitatorClient<T>
where
    T: RuntimeHttpTransport,
{
    pub fn verify(&self, payment: &X402FacilitatorPayment) -> Result<(), X402FacilitatorError> {
        let response = self.send_json("/verify", payment)?;
        let value: JsonWireValue =
            serde_json::from_str(&response.body).map_err(|source| X402FacilitatorError::Json {
                message: source.to_string(),
            })?;
        let status = json_status(&value)?;
        if status == "verified" && (200..300).contains(&response.status) {
            return Ok(());
        }
        Err(X402FacilitatorError::VerifyRefused {
            message: facilitator_message(&value)
                .unwrap_or_else(|| format!("facilitator verify returned {status}")),
        })
    }

    pub fn settle(
        &self,
        payment: &X402FacilitatorPayment,
    ) -> Result<X402FacilitatorSettlement, X402FacilitatorError> {
        let response = self.send_json("/settle", payment)?;
        let value: JsonWireValue =
            serde_json::from_str(&response.body).map_err(|source| X402FacilitatorError::Json {
                message: source.to_string(),
            })?;
        let status = json_status(&value)?;
        if status != "settled" || !(200..300).contains(&response.status) {
            return Err(X402FacilitatorError::SettleRefused {
                message: facilitator_message(&value)
                    .unwrap_or_else(|| format!("facilitator settle returned {status}")),
            });
        }
        let tx_hash = value
            .get("tx_hash")
            .and_then(JsonWireValue::as_str)
            .ok_or_else(|| X402FacilitatorError::MissingProof {
                message: "settled response missing tx_hash".to_owned(),
            })?;
        if !is_tx_hash(tx_hash) {
            return Err(X402FacilitatorError::MissingProof {
                message: "settled response tx_hash must be 0x-prefixed hex".to_owned(),
            });
        }
        let log = value.get("log").cloned().unwrap_or(JsonWireValue::Null);
        Ok(X402FacilitatorSettlement {
            tx_hash: tx_hash.to_owned(),
            log,
        })
    }

    fn send_json(
        &self,
        route: &str,
        body: &impl Serialize,
    ) -> Result<crate::runtime_http::RuntimeHttpResponse, X402FacilitatorError> {
        let body = serde_json::to_string(body).map_err(|source| X402FacilitatorError::Json {
            message: source.to_string(),
        })?;
        self.transport
            .send(RuntimeHttpRequest {
                method: HttpMethod::Post,
                url: format!("{}{}", self.base_url, route),
                headers: vec![RuntimeHttpHeader::new("content-type", "application/json")],
                body: Some(body),
            })
            .map_err(X402FacilitatorError::Transport)
    }
}

pub fn x402_settlement_evidence(
    request: RailSettlementRequest<'_>,
    settlement: X402FacilitatorSettlement,
) -> RailSettlementEvidence {
    RailSettlementEvidence {
        verifier_id: PAYMENT_RAIL_SUPERVISOR_VERIFIER_ID.to_owned(),
        proof_ref: settlement.tx_hash,
        rail: request.rail.to_owned(),
        counterparty: request.counterparty.to_owned(),
        amount_minor: request.amount_minor,
        currency: request.currency.to_owned(),
        idempotency_key: request.idempotency_key.to_owned(),
        settlement_status: request.skill_settlement_status.map(str::to_owned),
        provider_event_ref: Some("x402:facilitator:settled".to_owned()),
        shared_payment_token_ref: None,
        admission_token_digest: None,
    }
}

pub fn effect_settlement_receipt(input: EffectSettlementReceiptInput) -> EffectSettlementReceipt {
    let id = effect_settlement_receipt_id(&input);
    EffectSettlementReceipt {
        schema: EffectSettlementReceiptSchema::V1,
        id: id.into(),
        created_at: input.created_at.into(),
        family: "payment".into(),
        phase: input.phase,
        original_receipt_ref: input.original_receipt_ref,
        criterion_id: input.criterion_id.into(),
        proof_ref: input.proof_ref,
        evidence_refs: input.evidence_refs,
        confirmation_depth: input.confirmation_depth,
        payload: input.payload,
    }
}

pub fn x402_tx_proof_reference(tx_hash: &str) -> Reference {
    let mut reference = Reference::with_uri(ReferenceType::Verification, tx_hash);
    reference.proof_kind = Some(ProofKind::PaymentRail);
    reference.provider = Some("x402".into());
    reference
}

impl<T> ExternalSignerClient<T> {
    #[must_use]
    pub fn with_transport(endpoint_url: impl Into<String>, transport: T) -> Self {
        Self {
            endpoint_url: endpoint_url.into(),
            transport,
        }
    }
}

impl<T> ExternalSignerClient<T>
where
    T: RuntimeHttpTransport,
{
    pub fn sign(
        &self,
        request: &ExternalSignerRequest,
    ) -> Result<ExternalSignerSignedResponse, ExternalSignerError> {
        let body = serde_json::to_string(request).map_err(|source| ExternalSignerError::Json {
            message: source.to_string(),
        })?;
        let response = self
            .transport
            .send(RuntimeHttpRequest {
                method: HttpMethod::Post,
                url: self.endpoint_url.clone(),
                headers: vec![RuntimeHttpHeader::new("content-type", "application/json")],
                body: Some(body),
            })
            .map_err(ExternalSignerError::Transport)?;
        parse_external_signer_response(response.status, &response.body, request)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RailSupervisorError {
    #[error("payment rail supervisor is not configured: {message}")]
    SupervisorUnavailable { message: String },
    #[error("payment rail packet is invalid: {message}")]
    InvalidRailPacket { message: String },
    #[error("payment rail result status {status:?} is not fulfilled")]
    SettlementNotFulfilled { status: Option<String> },
}

#[derive(Debug, Error)]
pub enum ExternalSignerError {
    #[error("external signer template mismatch: {message}")]
    TemplateMismatch { message: String },
    #[error("external signer token expired: {message}")]
    ExpiredToken { message: String },
    #[error("external signer unsupported chain: {message}")]
    UnsupportedChain { message: String },
    #[error("external signer unavailable: {message}")]
    SignerUnavailable { message: String },
    #[error("external signer returned invalid response: {message}")]
    InvalidResponse { message: String },
    #[error("external signer JSON failed: {message}")]
    Json { message: String },
    #[error("external signer transport failed: {0}")]
    Transport(#[from] RuntimeHttpError),
}

#[derive(Debug, Error)]
pub enum X402FacilitatorError {
    #[error("x402 facilitator verify refused: {message}")]
    VerifyRefused { message: String },
    #[error("x402 facilitator settle refused: {message}")]
    SettleRefused { message: String },
    #[error("x402 facilitator settled without usable proof: {message}")]
    MissingProof { message: String },
    #[error("x402 facilitator returned invalid response: {message}")]
    InvalidResponse { message: String },
    #[error("x402 facilitator JSON failed: {message}")]
    Json { message: String },
    #[error("x402 facilitator transport failed: {0}")]
    Transport(#[from] RuntimeHttpError),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StripeSptIssuanceError {
    #[error("stripe SPT issuance requires rail stripe-spt, got {rail}")]
    WrongRail { rail: String },
    #[error("stripe SPT issuance requires a positive amount")]
    EmptyAmount,
    #[error("stripe SPT issuance field {field} is empty")]
    EmptyField { field: &'static str },
    #[error("stripe SPT issuance counterparty mismatch")]
    CounterpartyMismatch,
    #[error("stripe SPT issuance admission token digest failed: {0}")]
    Admission(#[from] PaymentAdmissionError),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MppWireError {
    #[error("MPP field {field} is empty")]
    EmptyField { field: &'static str },
    #[error("MPP amount must be a base-10 integer string")]
    InvalidAmount,
    #[error("MPP stripe paymentMethodTypes must not be empty")]
    EmptyPaymentMethodTypes,
    #[error("MPP stripe credential requires an spt_ token")]
    InvalidSharedPaymentToken,
    #[error("MPP tempo unsupported field {field}")]
    UnsupportedTempoField { field: &'static str },
    #[error("MPP tempo non-zero charges require supportedModes exactly [pull]")]
    UnsupportedTempoModes,
    #[error("MPP tempo credential type {credential_type} is unsupported for this amount")]
    UnsupportedTempoCredential { credential_type: &'static str },
    #[error("MPP tempo transaction credential requires a signature")]
    MissingTempoTransaction,
    #[error("MPP tempo proof credential requires a source DID")]
    MissingTempoProofSource,
}

pub fn validate_mpp_stripe_charge_request(
    request: &MppStripeChargeRequest,
) -> Result<(), MppWireError> {
    require_mpp_decimal_amount(&request.amount)?;
    require_mpp_field("currency", &request.currency)?;
    require_mpp_field(
        "methodDetails.networkId",
        &request.method_details.network_id,
    )?;
    if request.method_details.payment_method_types.is_empty() {
        return Err(MppWireError::EmptyPaymentMethodTypes);
    }
    for payment_method_type in &request.method_details.payment_method_types {
        require_mpp_field("methodDetails.paymentMethodTypes[]", payment_method_type)?;
    }
    Ok(())
}

pub fn validate_mpp_stripe_credential(
    credential: &MppStripeCredentialPayload,
) -> Result<(), MppWireError> {
    if credential.spt.starts_with("spt_") {
        Ok(())
    } else {
        Err(MppWireError::InvalidSharedPaymentToken)
    }
}

pub fn validate_mpp_tempo_charge_request(
    request: &MppTempoChargeRequest,
) -> Result<(), MppWireError> {
    require_mpp_decimal_amount(&request.amount)?;
    require_mpp_field("currency", &request.currency)?;
    let zero_amount = request.amount == "0";
    if request.method_details.fee_payer == Some(true) {
        return Err(MppWireError::UnsupportedTempoField { field: "feePayer" });
    }
    if request.method_details.memo.is_some() {
        return Err(MppWireError::UnsupportedTempoField { field: "memo" });
    }
    if request
        .method_details
        .splits
        .as_ref()
        .is_some_and(|splits| !splits.is_empty())
    {
        return Err(MppWireError::UnsupportedTempoField { field: "splits" });
    }
    if !zero_amount
        && request.method_details.supported_modes.as_deref()
            != Some(&[MppTempoSubmissionMode::Pull])
    {
        return Err(MppWireError::UnsupportedTempoModes);
    }
    Ok(())
}

pub fn validate_mpp_tempo_credential(
    request: &MppTempoChargeRequest,
    credential: &MppTempoCredentialPayload,
) -> Result<(), MppWireError> {
    let zero_amount = request.amount == "0";
    match credential.credential_type {
        MppTempoCredentialType::Transaction if !zero_amount => {
            if credential
                .signature
                .as_deref()
                .is_some_and(|signature| !signature.trim().is_empty())
            {
                Ok(())
            } else {
                Err(MppWireError::MissingTempoTransaction)
            }
        }
        MppTempoCredentialType::Proof if zero_amount => {
            if credential
                .source
                .as_deref()
                .is_some_and(|source| !source.trim().is_empty())
            {
                Ok(())
            } else {
                Err(MppWireError::MissingTempoProofSource)
            }
        }
        MppTempoCredentialType::Transaction => Err(MppWireError::UnsupportedTempoCredential {
            credential_type: "transaction",
        }),
        MppTempoCredentialType::Hash => Err(MppWireError::UnsupportedTempoCredential {
            credential_type: "hash",
        }),
        MppTempoCredentialType::Proof => Err(MppWireError::UnsupportedTempoCredential {
            credential_type: "proof",
        }),
    }
}

pub fn stripe_spt_settlement_evidence(
    request: RailSettlementRequest<'_>,
    charge: StripeSptChargeEvidence,
) -> RailSettlementEvidence {
    RailSettlementEvidence {
        verifier_id: PAYMENT_RAIL_SUPERVISOR_VERIFIER_ID.to_owned(),
        proof_ref: charge.charge_id,
        rail: request.rail.to_owned(),
        counterparty: request.counterparty.to_owned(),
        amount_minor: request.amount_minor,
        currency: request.currency.to_owned(),
        idempotency_key: request.idempotency_key.to_owned(),
        settlement_status: request.skill_settlement_status.map(str::to_owned),
        provider_event_ref: Some(charge.event_id),
        shared_payment_token_ref: Some(charge.shared_payment_token_id),
        admission_token_digest: Some(charge.admission_token_digest),
    }
}

pub fn stripe_spt_issuance_request(
    admission_token: &PaymentAdmissionToken,
    expected_counterparty: &str,
    idempotency_key: &str,
) -> Result<StripeSptIssuanceRequest, StripeSptIssuanceError> {
    if admission_token.rail != STRIPE_SPT_RAIL {
        return Err(StripeSptIssuanceError::WrongRail {
            rail: admission_token.rail.clone(),
        });
    }
    if admission_token.amount_minor == 0 {
        return Err(StripeSptIssuanceError::EmptyAmount);
    }
    require_stripe_spt_field("currency", &admission_token.currency)?;
    require_stripe_spt_field("money_movement_id", &admission_token.money_movement_id)?;
    require_stripe_spt_field("idempotency_key", idempotency_key)?;
    require_stripe_spt_field("expected_counterparty", expected_counterparty)?;
    let counterparty =
        admission_token
            .counterparty
            .as_deref()
            .ok_or(StripeSptIssuanceError::EmptyField {
                field: "counterparty",
            })?;
    if counterparty != expected_counterparty {
        return Err(StripeSptIssuanceError::CounterpartyMismatch);
    }
    Ok(StripeSptIssuanceRequest {
        money_movement_id: admission_token.money_movement_id.clone(),
        admission_token_digest: payment_admission_token_digest(admission_token)?,
        amount_minor: admission_token.amount_minor,
        currency: admission_token.currency.clone(),
        counterparty: counterparty.to_owned(),
        rail: STRIPE_SPT_RAIL.to_owned(),
        idempotency_key: idempotency_key.to_owned(),
    })
}

fn require_stripe_spt_field(
    field: &'static str,
    value: &str,
) -> Result<(), StripeSptIssuanceError> {
    if value.trim().is_empty() {
        return Err(StripeSptIssuanceError::EmptyField { field });
    }
    Ok(())
}

fn require_mpp_field(field: &'static str, value: &str) -> Result<(), MppWireError> {
    if value.trim().is_empty() {
        return Err(MppWireError::EmptyField { field });
    }
    Ok(())
}

fn require_mpp_decimal_amount(value: &str) -> Result<(), MppWireError> {
    if value.is_empty() || value.bytes().any(|byte| !byte.is_ascii_digit()) {
        return Err(MppWireError::InvalidAmount);
    }
    Ok(())
}

pub fn external_signer_request(
    admission_token: PaymentAdmissionToken,
    challenge: X402TransferAuthorizationChallenge,
) -> Result<ExternalSignerRequest, ExternalSignerError> {
    let template = x402_transfer_authorization_template(&admission_token, challenge)?;
    let template_digest = template_digest(&template)?;
    Ok(ExternalSignerRequest {
        schema: EXTERNAL_SIGNER_REQUEST_SCHEMA.to_owned(),
        admission_token,
        template,
        template_digest,
    })
}

pub fn x402_transfer_authorization_template(
    admission_token: &PaymentAdmissionToken,
    challenge: X402TransferAuthorizationChallenge,
) -> Result<X402TransferAuthorizationTemplate, ExternalSignerError> {
    expect_external_signer_field("rail", X402_RAIL, &admission_token.rail)?;
    expect_external_signer_u64(
        "amount_minor",
        admission_token.amount_minor,
        challenge.amount_minor,
    )?;
    expect_external_signer_field("currency", &admission_token.currency, &challenge.currency)?;
    let counterparty = admission_token.counterparty.as_deref().ok_or_else(|| {
        ExternalSignerError::TemplateMismatch {
            message: "admission token counterparty is required for x402 payTo binding".to_owned(),
        }
    })?;
    expect_external_signer_field("counterparty", counterparty, &challenge.pay_to)?;
    Ok(X402TransferAuthorizationTemplate {
        chain_id: challenge.chain_id,
        token_contract: challenge.token_contract,
        verifying_contract: challenge.verifying_contract,
        from: challenge.from,
        to: challenge.pay_to,
        value: admission_token.amount_minor,
        valid_after: challenge.valid_after,
        valid_before: challenge.valid_before,
        nonce: admission_token.money_movement_id.clone(),
        currency: admission_token.currency.clone(),
        amount_minor: admission_token.amount_minor,
        counterparty: counterparty.to_owned(),
        run_id: admission_token.run_id.clone(),
        authority_digest: admission_token.authority_digest.clone(),
        money_movement_id: admission_token.money_movement_id.clone(),
    })
}

pub fn template_digest(
    template: &X402TransferAuthorizationTemplate,
) -> Result<String, ExternalSignerError> {
    let value = serde_json::to_value(template).map_err(|source| ExternalSignerError::Json {
        message: source.to_string(),
    })?;
    let canonical = serde_json::to_string(&value).map_err(|source| ExternalSignerError::Json {
        message: source.to_string(),
    })?;
    Ok(sha256_prefixed(canonical.as_bytes()))
}

pub trait X402RailClient: Send + Sync {
    fn settlement_evidence<'a>(
        &'a self,
        request: RailSettlementRequest<'a>,
    ) -> RailSettlementFuture<'a>;
}

pub trait StripeSptRailClient: Send + Sync {
    fn settlement_evidence<'a>(
        &'a self,
        request: RailSettlementRequest<'a>,
    ) -> RailSettlementFuture<'a>;
}

pub trait MppFiatRailClient: Send + Sync {
    fn settlement_evidence<'a>(
        &'a self,
        request: RailSettlementRequest<'a>,
    ) -> RailSettlementFuture<'a>;
}

pub trait MppTempoRailClient: Send + Sync {
    fn settlement_evidence<'a>(
        &'a self,
        request: RailSettlementRequest<'a>,
    ) -> RailSettlementFuture<'a>;
}

#[derive(Clone, Debug)]
pub struct RailSupervisor<
    X = UnavailableX402RailClient,
    S = UnavailableStripeSptRailClient,
    F = UnavailableMppFiatRailClient,
    T = UnavailableMppTempoRailClient,
> {
    x402: X,
    stripe_spt: S,
    mpp_fiat: F,
    mpp_tempo: T,
}

impl
    RailSupervisor<
        UnavailableX402RailClient,
        UnavailableStripeSptRailClient,
        UnavailableMppFiatRailClient,
        UnavailableMppTempoRailClient,
    >
{
    #[must_use]
    pub const fn unavailable() -> Self {
        Self {
            x402: UnavailableX402RailClient,
            stripe_spt: UnavailableStripeSptRailClient,
            mpp_fiat: UnavailableMppFiatRailClient,
            mpp_tempo: UnavailableMppTempoRailClient,
        }
    }
}

impl<X>
    RailSupervisor<
        X,
        UnavailableStripeSptRailClient,
        UnavailableMppFiatRailClient,
        UnavailableMppTempoRailClient,
    >
{
    #[must_use]
    pub const fn new(x402: X) -> Self {
        Self {
            x402,
            stripe_spt: UnavailableStripeSptRailClient,
            mpp_fiat: UnavailableMppFiatRailClient,
            mpp_tempo: UnavailableMppTempoRailClient,
        }
    }
}

impl<X, S> RailSupervisor<X, S> {
    #[must_use]
    pub const fn with_rails(x402: X, stripe_spt: S) -> Self {
        Self {
            x402,
            stripe_spt,
            mpp_fiat: UnavailableMppFiatRailClient,
            mpp_tempo: UnavailableMppTempoRailClient,
        }
    }
}

impl<X, S, F, T> RailSupervisor<X, S, F, T> {
    #[must_use]
    pub const fn with_all_rails(x402: X, stripe_spt: S, mpp_fiat: F, mpp_tempo: T) -> Self {
        Self {
            x402,
            stripe_spt,
            mpp_fiat,
            mpp_tempo,
        }
    }
}

impl<X, S, F, T> RailSupervisor<X, S, F, T>
where
    X: X402RailClient,
    S: StripeSptRailClient,
    F: MppFiatRailClient,
    T: MppTempoRailClient,
{
    pub fn settlement_evidence(
        &self,
        request: RailSettlementRequest<'_>,
    ) -> Result<RailSettlementEvidence, RailSupervisorError> {
        match request.rail {
            X402_RAIL => block_on_rail(self.x402.settlement_evidence(request)),
            STRIPE_SPT_RAIL => block_on_rail(self.stripe_spt.settlement_evidence(request)),
            MPP_FIAT_RAIL => block_on_rail(self.mpp_fiat.settlement_evidence(request)),
            MPP_TEMPO_RAIL => block_on_rail(self.mpp_tempo.settlement_evidence(request)),
            rail => Err(RailSupervisorError::InvalidRailPacket {
                message: format!("unsupported payment rail '{rail}'"),
            }),
        }
    }
}

impl Default
    for RailSupervisor<
        UnavailableX402RailClient,
        UnavailableStripeSptRailClient,
        UnavailableMppFiatRailClient,
        UnavailableMppTempoRailClient,
    >
{
    fn default() -> Self {
        Self::unavailable()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableX402RailClient;

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableStripeSptRailClient;

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableMppFiatRailClient;

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableMppTempoRailClient;

impl X402RailClient for UnavailableX402RailClient {
    fn settlement_evidence<'a>(
        &'a self,
        _request: RailSettlementRequest<'a>,
    ) -> RailSettlementFuture<'a> {
        Box::pin(async {
            Err(RailSupervisorError::SupervisorUnavailable {
                message: "x402 rail client is not configured".to_owned(),
            })
        })
    }
}

impl StripeSptRailClient for UnavailableStripeSptRailClient {
    fn settlement_evidence<'a>(
        &'a self,
        _request: RailSettlementRequest<'a>,
    ) -> RailSettlementFuture<'a> {
        Box::pin(async {
            Err(RailSupervisorError::SupervisorUnavailable {
                message: "stripe-spt rail client is not configured".to_owned(),
            })
        })
    }
}

impl MppFiatRailClient for UnavailableMppFiatRailClient {
    fn settlement_evidence<'a>(
        &'a self,
        _request: RailSettlementRequest<'a>,
    ) -> RailSettlementFuture<'a> {
        Box::pin(async {
            Err(RailSupervisorError::SupervisorUnavailable {
                message: "mpp-fiat rail client is not configured".to_owned(),
            })
        })
    }
}

impl MppTempoRailClient for UnavailableMppTempoRailClient {
    fn settlement_evidence<'a>(
        &'a self,
        _request: RailSettlementRequest<'a>,
    ) -> RailSettlementFuture<'a> {
        Box::pin(async {
            Err(RailSupervisorError::SupervisorUnavailable {
                message: "mpp-tempo rail client is not configured".to_owned(),
            })
        })
    }
}

fn block_on_rail(
    future: RailSettlementFuture<'_>,
) -> Result<RailSettlementEvidence, RailSupervisorError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|source| RailSupervisorError::SupervisorUnavailable {
            message: format!("payment rail runtime unavailable: {source}"),
        })?;
    runtime.block_on(future)
}

fn parse_external_signer_response(
    status: u16,
    body: &str,
    request: &ExternalSignerRequest,
) -> Result<ExternalSignerSignedResponse, ExternalSignerError> {
    let value: JsonWireValue =
        serde_json::from_str(body).map_err(|source| ExternalSignerError::Json {
            message: source.to_string(),
        })?;
    let response_status = value
        .get("status")
        .and_then(JsonWireValue::as_str)
        .ok_or_else(|| ExternalSignerError::InvalidResponse {
            message: "missing status".to_owned(),
        })?;
    match response_status {
        "signed" => {
            if !(200..300).contains(&status) {
                return Err(ExternalSignerError::SignerUnavailable {
                    message: format!("external signer returned HTTP {status} for signed response"),
                });
            }
            let response: ExternalSignerSignedResponse =
                serde_json::from_value(value).map_err(|source| ExternalSignerError::Json {
                    message: source.to_string(),
                })?;
            validate_external_signer_signed_response(&response, request)?;
            Ok(response)
        }
        "refused" => {
            let refusal = parse_external_signer_refusal(value)?;
            Err(map_external_signer_refusal(refusal))
        }
        other => Err(ExternalSignerError::InvalidResponse {
            message: format!("unsupported status '{other}'"),
        }),
    }
}

fn parse_external_signer_refusal(
    value: JsonWireValue,
) -> Result<ExternalSignerRefusalResponse, ExternalSignerError> {
    let schema = value
        .get("schema")
        .and_then(JsonWireValue::as_str)
        .ok_or_else(|| ExternalSignerError::InvalidResponse {
            message: "refusal missing schema".to_owned(),
        })?;
    expect_external_signer_field("schema", EXTERNAL_SIGNER_RESPONSE_SCHEMA, schema)?;
    let code = value
        .get("code")
        .and_then(JsonWireValue::as_str)
        .ok_or_else(|| ExternalSignerError::InvalidResponse {
            message: "refusal missing code".to_owned(),
        })
        .and_then(parse_external_signer_refusal_code)?;
    let message = value
        .get("message")
        .and_then(JsonWireValue::as_str)
        .ok_or_else(|| ExternalSignerError::InvalidResponse {
            message: "refusal missing message".to_owned(),
        })?
        .to_owned();
    Ok(ExternalSignerRefusalResponse {
        schema: schema.to_owned(),
        status: "refused".to_owned(),
        code,
        message,
    })
}

fn parse_external_signer_refusal_code(
    code: &str,
) -> Result<ExternalSignerRefusalCode, ExternalSignerError> {
    match code {
        "template_mismatch" => Ok(ExternalSignerRefusalCode::TemplateMismatch),
        "expired_token" => Ok(ExternalSignerRefusalCode::ExpiredToken),
        "unsupported_chain" => Ok(ExternalSignerRefusalCode::UnsupportedChain),
        "signer_unavailable" => Ok(ExternalSignerRefusalCode::SignerUnavailable),
        other => Err(ExternalSignerError::InvalidResponse {
            message: format!("unsupported refusal code '{other}'"),
        }),
    }
}

fn validate_external_signer_signed_response(
    response: &ExternalSignerSignedResponse,
    request: &ExternalSignerRequest,
) -> Result<(), ExternalSignerError> {
    expect_external_signer_field("schema", EXTERNAL_SIGNER_RESPONSE_SCHEMA, &response.schema)?;
    expect_external_signer_field(
        "template_digest",
        &request.template_digest,
        &response.template_digest,
    )?;
    if !is_evm_address(&response.signer_address) {
        return Err(ExternalSignerError::InvalidResponse {
            message: "signer_address must be a 20-byte 0x-prefixed hex address".to_owned(),
        });
    }
    if !is_evm_signature(&response.signature) {
        return Err(ExternalSignerError::InvalidResponse {
            message: "signature must be a 65-byte 0x-prefixed EVM signature".to_owned(),
        });
    }
    Ok(())
}

fn map_external_signer_refusal(refusal: ExternalSignerRefusalResponse) -> ExternalSignerError {
    match refusal.code {
        ExternalSignerRefusalCode::TemplateMismatch => ExternalSignerError::TemplateMismatch {
            message: refusal.message,
        },
        ExternalSignerRefusalCode::ExpiredToken => ExternalSignerError::ExpiredToken {
            message: refusal.message,
        },
        ExternalSignerRefusalCode::UnsupportedChain => ExternalSignerError::UnsupportedChain {
            message: refusal.message,
        },
        ExternalSignerRefusalCode::SignerUnavailable => ExternalSignerError::SignerUnavailable {
            message: refusal.message,
        },
    }
}

fn expect_external_signer_field(
    field: &'static str,
    expected: &str,
    actual: &str,
) -> Result<(), ExternalSignerError> {
    if expected == actual {
        return Ok(());
    }
    Err(ExternalSignerError::TemplateMismatch {
        message: format!("{field} mismatch: expected {expected}, got {actual}"),
    })
}

fn expect_external_signer_u64(
    field: &'static str,
    expected: u64,
    actual: u64,
) -> Result<(), ExternalSignerError> {
    if expected == actual {
        return Ok(());
    }
    Err(ExternalSignerError::TemplateMismatch {
        message: format!("{field} mismatch: expected {expected}, got {actual}"),
    })
}

fn is_evm_address(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("0x") else {
        return false;
    };
    hex.len() == 40 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_evm_signature(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("0x") else {
        return false;
    };
    hex.len() == 130 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_tx_hash(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("0x") else {
        return false;
    };
    hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn json_status(value: &JsonWireValue) -> Result<&str, X402FacilitatorError> {
    value
        .get("status")
        .and_then(JsonWireValue::as_str)
        .ok_or_else(|| X402FacilitatorError::InvalidResponse {
            message: "missing status".to_owned(),
        })
}

fn facilitator_message(value: &JsonWireValue) -> Option<String> {
    value
        .get("message")
        .and_then(JsonWireValue::as_str)
        .map(str::to_owned)
}

fn effect_settlement_receipt_id(input: &EffectSettlementReceiptInput) -> String {
    let proof_ref = input
        .proof_ref
        .as_ref()
        .map(|reference| reference.uri.as_ref())
        .unwrap_or("none");
    let confirmation_depth = input
        .confirmation_depth
        .map(|depth| depth.to_string())
        .unwrap_or_else(|| "none".to_owned());
    let material = format!(
        "effect-settlement\nphase={:?}\noriginal={}\ncriterion={}\nproof={proof_ref}\ndepth={confirmation_depth}",
        input.phase, input.original_receipt_ref.uri, input.criterion_id
    );
    format!(
        "esr_{}",
        sha256_prefixed(material.as_bytes()).trim_start_matches("sha256:")
    )
}

#[cfg(test)]
mod tests {
    use super::{
        EXTERNAL_SIGNER_REQUEST_SCHEMA, EXTERNAL_SIGNER_RESPONSE_SCHEMA,
        EffectSettlementReceiptInput, ExternalSignerClient, ExternalSignerError,
        ExternalSignerRefusalCode, MPP_FIAT_RAIL, MPP_TEMPO_RAIL, MppFiatRailClient,
        MppStripeChargeRequest, MppStripeCredentialPayload, MppTempoChargeRequest,
        MppTempoCredentialPayload, MppTempoCredentialType, MppTempoRailClient, MppWireError,
        PAYMENT_RAIL_SUPERVISOR_VERIFIER_ID, RailSettlementEvidence, RailSettlementFuture,
        RailSettlementRequest, RailSupervisor, RailSupervisorError, STRIPE_SPT_RAIL,
        StripeSptChargeEvidence, StripeSptIssuanceError, StripeSptRailClient, X402_RAIL,
        X402FacilitatorClient, X402FacilitatorError, X402FacilitatorPayment, X402RailClient,
        X402TransferAuthorizationChallenge, effect_settlement_receipt, external_signer_request,
        stripe_spt_issuance_request, stripe_spt_settlement_evidence, template_digest,
        validate_mpp_stripe_charge_request, validate_mpp_stripe_credential,
        validate_mpp_tempo_charge_request, validate_mpp_tempo_credential, x402_settlement_evidence,
        x402_tx_proof_reference,
    };
    use crate::PaymentAdmissionToken;
    use crate::runtime_http::{
        RuntimeHttpError, RuntimeHttpRequest, RuntimeHttpResponse, RuntimeHttpTransport,
    };
    use runx_contracts::{
        EffectSettlementPhase, JsonObject, JsonValue, ProofKind, Reference, ReferenceType,
        sha256_prefixed,
    };

    #[derive(Clone, Copy, Debug)]
    struct FixtureX402RailClient;

    impl X402RailClient for FixtureX402RailClient {
        fn settlement_evidence<'a>(
            &'a self,
            request: RailSettlementRequest<'a>,
        ) -> RailSettlementFuture<'a> {
            Box::pin(async move {
                if request.skill_settlement_status != Some("fulfilled") {
                    return Err(RailSupervisorError::SettlementNotFulfilled {
                        status: request.skill_settlement_status.map(str::to_owned),
                    });
                }
                Ok(RailSettlementEvidence {
                    verifier_id: PAYMENT_RAIL_SUPERVISOR_VERIFIER_ID.to_owned(),
                    proof_ref: request.proof_ref.to_owned(),
                    rail: request.rail.to_owned(),
                    counterparty: request.counterparty.to_owned(),
                    amount_minor: request.amount_minor,
                    currency: request.currency.to_owned(),
                    idempotency_key: request.idempotency_key.to_owned(),
                    settlement_status: request.skill_settlement_status.map(str::to_owned),
                    provider_event_ref: Some(format!("runx-runtime:test:{}", request.proof_ref)),
                    shared_payment_token_ref: None,
                    admission_token_digest: None,
                })
            })
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct FixtureStripeSptRailClient;

    impl StripeSptRailClient for FixtureStripeSptRailClient {
        fn settlement_evidence<'a>(
            &'a self,
            request: RailSettlementRequest<'a>,
        ) -> RailSettlementFuture<'a> {
            Box::pin(async move {
                if request.skill_settlement_status != Some("fulfilled") {
                    return Err(RailSupervisorError::SettlementNotFulfilled {
                        status: request.skill_settlement_status.map(str::to_owned),
                    });
                }
                Ok(stripe_spt_settlement_evidence(
                    request,
                    StripeSptChargeEvidence {
                        payment_intent_id: "pi_test_123".to_owned(),
                        charge_id: "ch_test_123".to_owned(),
                        event_id: "evt_test_123".to_owned(),
                        shared_payment_token_id: "spt_test_123".to_owned(),
                        admission_token_digest: "sha256:admission".to_owned(),
                    },
                ))
            })
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct FixtureMppFiatRailClient;

    impl MppFiatRailClient for FixtureMppFiatRailClient {
        fn settlement_evidence<'a>(
            &'a self,
            request: RailSettlementRequest<'a>,
        ) -> RailSettlementFuture<'a> {
            Box::pin(async move {
                if request.skill_settlement_status != Some("fulfilled") {
                    return Err(RailSupervisorError::SettlementNotFulfilled {
                        status: request.skill_settlement_status.map(str::to_owned),
                    });
                }
                Ok(RailSettlementEvidence {
                    verifier_id: PAYMENT_RAIL_SUPERVISOR_VERIFIER_ID.to_owned(),
                    proof_ref: "pi_mpp_fiat_test_123".to_owned(),
                    rail: request.rail.to_owned(),
                    counterparty: request.counterparty.to_owned(),
                    amount_minor: request.amount_minor,
                    currency: request.currency.to_owned(),
                    idempotency_key: request.idempotency_key.to_owned(),
                    settlement_status: request.skill_settlement_status.map(str::to_owned),
                    provider_event_ref: Some(
                        "mpp-fiat:payment_intent:pi_mpp_fiat_test_123".to_owned(),
                    ),
                    shared_payment_token_ref: Some("spt_mpp_test_123".to_owned()),
                    admission_token_digest: Some("sha256:mpp-fiat-admission".to_owned()),
                })
            })
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct FixtureMppTempoRailClient;

    impl MppTempoRailClient for FixtureMppTempoRailClient {
        fn settlement_evidence<'a>(
            &'a self,
            request: RailSettlementRequest<'a>,
        ) -> RailSettlementFuture<'a> {
            Box::pin(async move {
                if request.skill_settlement_status != Some("fulfilled") {
                    return Err(RailSupervisorError::SettlementNotFulfilled {
                        status: request.skill_settlement_status.map(str::to_owned),
                    });
                }
                Ok(RailSettlementEvidence {
                    verifier_id: PAYMENT_RAIL_SUPERVISOR_VERIFIER_ID.to_owned(),
                    proof_ref: tx_hash(),
                    rail: request.rail.to_owned(),
                    counterparty: request.counterparty.to_owned(),
                    amount_minor: request.amount_minor,
                    currency: request.currency.to_owned(),
                    idempotency_key: request.idempotency_key.to_owned(),
                    settlement_status: request.skill_settlement_status.map(str::to_owned),
                    provider_event_ref: Some(format!("mpp-tempo:tx:{}", tx_hash())),
                    shared_payment_token_ref: None,
                    admission_token_digest: Some("sha256:mpp-tempo-admission".to_owned()),
                })
            })
        }
    }

    #[test]
    fn x402_dispatch_round_trips_settlement_evidence() {
        let supervisor = RailSupervisor::new(FixtureX402RailClient);
        let evidence = supervisor
            .settlement_evidence(request(X402_RAIL, Some("fulfilled")))
            .unwrap_or_else(|error| panic!("fixture x402 settlement failed: {error}"));

        assert_eq!(evidence.rail, X402_RAIL);
        assert_eq!(evidence.proof_ref, "proof_ref_1");
        assert_eq!(evidence.counterparty, "merchant.example");
        assert_eq!(evidence.amount_minor, 25);
        assert_eq!(evidence.currency, "USD");
        assert_eq!(evidence.idempotency_key, "mmid_1");
        assert_eq!(
            evidence.provider_event_ref.as_deref(),
            Some("runx-runtime:test:proof_ref_1")
        );
    }

    #[test]
    fn stripe_spt_dispatch_round_trips_charge_and_event_evidence() {
        let supervisor =
            RailSupervisor::with_rails(FixtureX402RailClient, FixtureStripeSptRailClient);
        let evidence = supervisor
            .settlement_evidence(request(STRIPE_SPT_RAIL, Some("fulfilled")))
            .unwrap_or_else(|error| panic!("fixture stripe-spt settlement failed: {error}"));

        assert_eq!(evidence.rail, STRIPE_SPT_RAIL);
        assert_eq!(evidence.proof_ref, "ch_test_123");
        assert_eq!(evidence.provider_event_ref.as_deref(), Some("evt_test_123"));
        assert_eq!(
            evidence.shared_payment_token_ref.as_deref(),
            Some("spt_test_123")
        );
        assert_eq!(
            evidence.admission_token_digest.as_deref(),
            Some("sha256:admission")
        );
    }

    #[test]
    fn mpp_fiat_dispatch_round_trips_payment_intent_evidence() {
        let supervisor = RailSupervisor::with_all_rails(
            FixtureX402RailClient,
            FixtureStripeSptRailClient,
            FixtureMppFiatRailClient,
            FixtureMppTempoRailClient,
        );
        let evidence = supervisor
            .settlement_evidence(request(MPP_FIAT_RAIL, Some("fulfilled")))
            .unwrap_or_else(|error| panic!("fixture mpp-fiat settlement failed: {error}"));

        assert_eq!(evidence.rail, MPP_FIAT_RAIL);
        assert_eq!(evidence.proof_ref, "pi_mpp_fiat_test_123");
        assert_eq!(
            evidence.provider_event_ref.as_deref(),
            Some("mpp-fiat:payment_intent:pi_mpp_fiat_test_123")
        );
        assert_eq!(
            evidence.shared_payment_token_ref.as_deref(),
            Some("spt_mpp_test_123")
        );
    }

    #[test]
    fn mpp_tempo_dispatch_round_trips_transaction_evidence() {
        let supervisor = RailSupervisor::with_all_rails(
            FixtureX402RailClient,
            FixtureStripeSptRailClient,
            FixtureMppFiatRailClient,
            FixtureMppTempoRailClient,
        );
        let evidence = supervisor
            .settlement_evidence(request(MPP_TEMPO_RAIL, Some("fulfilled")))
            .unwrap_or_else(|error| panic!("fixture mpp-tempo settlement failed: {error}"));

        assert_eq!(evidence.rail, MPP_TEMPO_RAIL);
        assert_eq!(evidence.proof_ref, tx_hash());
        assert_eq!(
            evidence.provider_event_ref.as_deref(),
            Some(format!("mpp-tempo:tx:{}", tx_hash()).as_str())
        );
        assert!(evidence.shared_payment_token_ref.is_none());
    }

    #[test]
    fn unknown_rail_fails_closed_before_x402_client() {
        let supervisor = RailSupervisor::new(FixtureX402RailClient);
        let error = supervisor
            .settlement_evidence(request("stripe", Some("fulfilled")))
            .unwrap_err();

        assert_eq!(
            error,
            RailSupervisorError::InvalidRailPacket {
                message: "unsupported payment rail 'stripe'".to_owned(),
            }
        );
    }

    #[test]
    fn x402_client_can_refuse_unfulfilled_settlement() {
        let supervisor = RailSupervisor::new(FixtureX402RailClient);
        let error = supervisor
            .settlement_evidence(request(X402_RAIL, Some("reserved")))
            .unwrap_err();

        assert_eq!(
            error,
            RailSupervisorError::SettlementNotFulfilled {
                status: Some("reserved".to_owned()),
            }
        );
    }

    #[test]
    fn default_x402_client_fails_closed() {
        let supervisor = RailSupervisor::unavailable();
        let error = supervisor
            .settlement_evidence(request(X402_RAIL, Some("fulfilled")))
            .unwrap_err();

        assert_eq!(
            error,
            RailSupervisorError::SupervisorUnavailable {
                message: "x402 rail client is not configured".to_owned(),
            }
        );
    }

    #[test]
    fn default_mpp_clients_fail_closed() {
        let supervisor = RailSupervisor::unavailable();
        let fiat_error = supervisor
            .settlement_evidence(request(MPP_FIAT_RAIL, Some("fulfilled")))
            .unwrap_err();
        let tempo_error = supervisor
            .settlement_evidence(request(MPP_TEMPO_RAIL, Some("fulfilled")))
            .unwrap_err();

        assert_eq!(
            fiat_error,
            RailSupervisorError::SupervisorUnavailable {
                message: "mpp-fiat rail client is not configured".to_owned(),
            }
        );
        assert_eq!(
            tempo_error,
            RailSupervisorError::SupervisorUnavailable {
                message: "mpp-tempo rail client is not configured".to_owned(),
            }
        );
    }

    #[test]
    fn mpp_stripe_wire_contract_pins_method_details_and_spt() {
        let request = serde_json::from_value::<MppStripeChargeRequest>(serde_json::json!({
            "amount": "5000",
            "currency": "usd",
            "description": "Premium API access",
            "externalId": "order_12345",
            "recipient": "acct_seller",
            "methodDetails": {
                "networkId": "profile_1MqDcVKA5fEO2tZvKQm9g8Yj",
                "paymentMethodTypes": ["card", "link"],
                "metadata": {"challenge_id": "ch_mpp_123"}
            }
        }))
        .unwrap_or_else(|error| panic!("mpp stripe request should parse: {error}"));
        validate_mpp_stripe_charge_request(&request)
            .unwrap_or_else(|error| panic!("mpp stripe request should validate: {error}"));
        let credential = MppStripeCredentialPayload {
            spt: "spt_1N4Zv32eZvKYlo2CPhVPkJlW".to_owned(),
            external_id: Some("client_order_789".to_owned()),
        };

        validate_mpp_stripe_credential(&credential)
            .unwrap_or_else(|error| panic!("mpp stripe credential should validate: {error}"));
        assert_eq!(
            request.method_details.network_id,
            "profile_1MqDcVKA5fEO2tZvKQm9g8Yj"
        );
        assert_eq!(
            request.method_details.payment_method_types,
            ["card", "link"]
        );
    }

    #[test]
    fn mpp_tempo_wire_contract_accepts_pull_transaction_and_rejects_push()
    -> Result<(), Box<dyn std::error::Error>> {
        let request = serde_json::from_value::<MppTempoChargeRequest>(serde_json::json!({
            "amount": "1000000",
            "currency": "0x20c0000000000000000000000000000000000000",
            "recipient": "0x742d35Cc6634C0532925a3b844Bc9e7595f8fE00",
            "methodDetails": {
                "chainId": 42431,
                "feePayer": false,
                "supportedModes": ["pull"]
            }
        }))?;
        validate_mpp_tempo_charge_request(&request)?;
        validate_mpp_tempo_credential(
            &request,
            &MppTempoCredentialPayload {
                credential_type: MppTempoCredentialType::Transaction,
                signature: Some("0x1234".to_owned()),
                hash: None,
                source: Some(
                    "did:pkh:eip155:42431:0x1111111111111111111111111111111111111111".to_owned(),
                ),
            },
        )?;
        let push_request = serde_json::from_value::<MppTempoChargeRequest>(serde_json::json!({
            "amount": "1000000",
            "currency": "0x20c0000000000000000000000000000000000000",
            "recipient": "0x742d35Cc6634C0532925a3b844Bc9e7595f8fE00",
            "methodDetails": {
                "chainId": 42431,
                "supportedModes": ["push"]
            }
        }))?;

        assert_eq!(
            validate_mpp_tempo_charge_request(&push_request),
            Err(MppWireError::UnsupportedTempoModes)
        );
        assert_eq!(
            validate_mpp_tempo_credential(
                &request,
                &MppTempoCredentialPayload {
                    credential_type: MppTempoCredentialType::Hash,
                    signature: None,
                    hash: Some(tx_hash()),
                    source: Some(
                        "did:pkh:eip155:42431:0x1111111111111111111111111111111111111111"
                            .to_owned()
                    ),
                },
            ),
            Err(MppWireError::UnsupportedTempoCredential {
                credential_type: "hash",
            })
        );
        Ok(())
    }

    #[test]
    fn mpp_tempo_wire_contract_rejects_fee_payer_memo_and_splits()
    -> Result<(), Box<dyn std::error::Error>> {
        for (field, request) in [
            (
                "feePayer",
                serde_json::json!({
                    "amount": "1000000",
                    "currency": "0x20c0000000000000000000000000000000000000",
                    "methodDetails": {
                        "chainId": 42431,
                        "feePayer": true,
                        "supportedModes": ["pull"]
                    }
                }),
            ),
            (
                "memo",
                serde_json::json!({
                    "amount": "1000000",
                    "currency": "0x20c0000000000000000000000000000000000000",
                    "methodDetails": {
                        "chainId": 42431,
                        "memo": "0x0000000000000000000000000000000000000000000000000000000000000000",
                        "supportedModes": ["pull"]
                    }
                }),
            ),
            (
                "splits",
                serde_json::json!({
                    "amount": "1000000",
                    "currency": "0x20c0000000000000000000000000000000000000",
                    "methodDetails": {
                        "chainId": 42431,
                        "supportedModes": ["pull"],
                        "splits": [{
                            "recipient": "0x742d35Cc6634C0532925a3b844Bc9e7595f8fE00",
                            "amount": "1"
                        }]
                    }
                }),
            ),
        ] {
            let request = serde_json::from_value::<MppTempoChargeRequest>(request)?;
            assert_eq!(
                validate_mpp_tempo_charge_request(&request),
                Err(MppWireError::UnsupportedTempoField { field })
            );
        }
        Ok(())
    }

    #[test]
    fn mpp_tempo_zero_amount_accepts_proof_only() -> Result<(), Box<dyn std::error::Error>> {
        let request = serde_json::from_value::<MppTempoChargeRequest>(serde_json::json!({
            "amount": "0",
            "currency": "0x20c0000000000000000000000000000000000000",
            "methodDetails": {
                "chainId": 42431
            }
        }))?;
        validate_mpp_tempo_charge_request(&request)?;
        validate_mpp_tempo_credential(
            &request,
            &MppTempoCredentialPayload {
                credential_type: MppTempoCredentialType::Proof,
                signature: Some("0xproof".to_owned()),
                hash: None,
                source: Some(
                    "did:pkh:eip155:42431:0x1111111111111111111111111111111111111111".to_owned(),
                ),
            },
        )?;
        assert_eq!(
            validate_mpp_tempo_credential(
                &request,
                &MppTempoCredentialPayload {
                    credential_type: MppTempoCredentialType::Transaction,
                    signature: Some("0x1234".to_owned()),
                    hash: None,
                    source: None,
                },
            ),
            Err(MppWireError::UnsupportedTempoCredential {
                credential_type: "transaction",
            })
        );
        Ok(())
    }

    #[test]
    fn external_signer_template_binds_admission_token_and_digest()
    -> Result<(), Box<dyn std::error::Error>> {
        let request = external_signer_request(admission_token(), challenge())?;

        assert_eq!(request.schema, EXTERNAL_SIGNER_REQUEST_SCHEMA);
        assert_eq!(request.template.chain_id, 84532);
        assert_eq!(
            request.template.token_contract,
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(
            request.template.verifying_contract,
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        );
        assert_eq!(
            request.template.from,
            "0x1111111111111111111111111111111111111111"
        );
        assert_eq!(
            request.template.to,
            "0x2222222222222222222222222222222222222222"
        );
        assert_eq!(request.template.value, 25);
        assert_eq!(
            request.template.nonce,
            request.admission_token.money_movement_id
        );
        assert_eq!(request.template.run_id, request.admission_token.run_id);
        assert_eq!(
            request.template.authority_digest,
            request.admission_token.authority_digest
        );
        assert_eq!(request.template_digest, template_digest(&request.template)?);
        let canonical = serde_json::to_string(&serde_json::to_value(&request.template)?)?;
        assert_eq!(
            request.template_digest,
            sha256_prefixed(canonical.as_bytes())
        );
        Ok(())
    }

    #[test]
    fn external_signer_template_refuses_admission_mismatch() {
        let mut challenge = challenge();
        challenge.amount_minor = 26;

        let error = external_signer_request(admission_token(), challenge).unwrap_err();

        assert!(matches!(
            error,
            ExternalSignerError::TemplateMismatch { message }
                if message == "amount_minor mismatch: expected 25, got 26"
        ));
    }

    #[test]
    fn external_signer_client_returns_signed_response() -> Result<(), Box<dyn std::error::Error>> {
        let request = external_signer_request(admission_token(), challenge())?;
        let client = ExternalSignerClient::with_transport(
            "https://signer.local/sign",
            FixtureHttpTransport::new(
                "https://signer.local/sign",
                200,
                signed_response_body(&request.template_digest)?,
            ),
        );

        let response = client.sign(&request)?;

        assert_eq!(response.schema, EXTERNAL_SIGNER_RESPONSE_SCHEMA);
        assert_eq!(response.status, "signed");
        assert_eq!(
            response.signer_address,
            "0x3333333333333333333333333333333333333333"
        );
        assert_eq!(response.template_digest, request.template_digest);
        assert!(response.signature.starts_with("0x"));
        assert_eq!(response.signature.len(), 132);
        Ok(())
    }

    #[test]
    fn external_signer_client_maps_template_mismatch_refusal()
    -> Result<(), Box<dyn std::error::Error>> {
        let request = external_signer_request(admission_token(), challenge())?;
        let client = ExternalSignerClient::with_transport(
            "https://signer.local/sign",
            FixtureHttpTransport::new(
                "https://signer.local/sign",
                400,
                refusal_response_body(
                    ExternalSignerRefusalCode::TemplateMismatch,
                    "amount mismatch",
                )?,
            ),
        );

        let error = client.sign(&request).unwrap_err();

        assert!(matches!(
            error,
            ExternalSignerError::TemplateMismatch { message } if message == "amount mismatch"
        ));
        Ok(())
    }

    #[test]
    fn external_signer_client_rejects_malformed_refusal_code()
    -> Result<(), Box<dyn std::error::Error>> {
        let request = external_signer_request(admission_token(), challenge())?;
        let client = ExternalSignerClient::with_transport(
            "https://signer.local/sign",
            FixtureHttpTransport::new(
                "https://signer.local/sign",
                400,
                serde_json::to_string(&serde_json::json!({
                    "schema": EXTERNAL_SIGNER_RESPONSE_SCHEMA,
                    "status": "refused",
                    "code": "template_Mismatch",
                    "message": "case drift",
                }))?,
            ),
        );

        let error = client.sign(&request).unwrap_err();

        assert!(matches!(
            error,
            ExternalSignerError::InvalidResponse { message }
                if message == "unsupported refusal code 'template_Mismatch'"
        ));
        Ok(())
    }

    #[test]
    fn external_signer_request_and_response_do_not_expose_private_key_material()
    -> Result<(), Box<dyn std::error::Error>> {
        let request = external_signer_request(admission_token(), challenge())?;
        let response = client_signed_response(&request)?;
        let rendered = format!(
            "{}\n{request:?}\n{}\n{response:?}",
            serde_json::to_string(&request)?,
            serde_json::to_string(&response)?,
        );

        for forbidden in ["private_key", "seed", "mnemonic", "secret_key"] {
            assert!(!rendered.contains(forbidden));
        }
        assert!(rendered.contains("template_digest"));
        assert!(rendered.contains("signature"));
        Ok(())
    }

    #[test]
    fn stripe_spt_issuance_request_binds_admission_scope_and_digest()
    -> Result<(), Box<dyn std::error::Error>> {
        let token = stripe_admission_token();
        let request =
            stripe_spt_issuance_request(&token, "acct_counterparty", "stripe-spt:idem-123")?;

        assert_eq!(request.rail, STRIPE_SPT_RAIL);
        assert_eq!(request.money_movement_id, token.money_movement_id);
        assert_eq!(request.amount_minor, token.amount_minor);
        assert_eq!(request.currency, token.currency);
        assert_eq!(request.counterparty, "acct_counterparty");
        assert_eq!(request.idempotency_key, "stripe-spt:idem-123");
        assert_eq!(
            request.admission_token_digest,
            crate::payment_admission_token_digest(&token)?
        );
        Ok(())
    }

    #[test]
    fn stripe_spt_issuance_request_refuses_counterparty_mismatch() {
        let error = stripe_spt_issuance_request(&stripe_admission_token(), "acct_other", "idem")
            .unwrap_err();

        assert_eq!(error, StripeSptIssuanceError::CounterpartyMismatch);
    }

    #[test]
    fn stripe_spt_issuance_request_refuses_wrong_rail() {
        let error = stripe_spt_issuance_request(&admission_token(), "acct_counterparty", "idem")
            .unwrap_err();

        assert_eq!(
            error,
            StripeSptIssuanceError::WrongRail {
                rail: X402_RAIL.to_owned(),
            }
        );
    }

    #[test]
    fn x402_facilitator_verify_and_settle_projects_tx_hash_evidence()
    -> Result<(), Box<dyn std::error::Error>> {
        let payment = facilitator_payment();
        let verifier = X402FacilitatorClient::with_transport(
            "https://facilitator.example",
            FixtureHttpTransport::new(
                "https://facilitator.example/verify",
                200,
                serde_json::to_string(&serde_json::json!({"status": "verified"}))?,
            ),
        );
        verifier.verify(&payment)?;
        let settler = X402FacilitatorClient::with_transport(
            "https://facilitator.example/",
            FixtureHttpTransport::new(
                "https://facilitator.example/settle",
                200,
                serde_json::to_string(&serde_json::json!({
                    "status": "settled",
                    "tx_hash": tx_hash(),
                    "log": {"block": "testnet"}
                }))?,
            ),
        );

        let settlement = settler.settle(&payment)?;
        let evidence = x402_settlement_evidence(request(X402_RAIL, Some("fulfilled")), settlement);

        assert_eq!(evidence.proof_ref, tx_hash());
        assert_eq!(evidence.rail, X402_RAIL);
        assert_eq!(
            evidence.provider_event_ref.as_deref(),
            Some("x402:facilitator:settled")
        );
        Ok(())
    }

    #[test]
    fn x402_facilitator_settle_requires_tx_hash_proof() -> Result<(), Box<dyn std::error::Error>> {
        let client = X402FacilitatorClient::with_transport(
            "https://facilitator.example",
            FixtureHttpTransport::new(
                "https://facilitator.example/settle",
                200,
                serde_json::to_string(&serde_json::json!({"status": "settled"}))?,
            ),
        );

        let error = client.settle(&facilitator_payment()).unwrap_err();

        assert!(matches!(
            error,
            X402FacilitatorError::MissingProof { message }
                if message == "settled response missing tx_hash"
        ));
        Ok(())
    }

    #[test]
    fn effect_settlement_receipts_link_provisional_inflight_and_sealed_phases() {
        let original = Reference::runx(ReferenceType::Receipt, "receipt_1");
        let provisional = effect_settlement_receipt(EffectSettlementReceiptInput {
            created_at: "2026-06-01T00:00:00Z".to_owned(),
            phase: EffectSettlementPhase::Provisional,
            original_receipt_ref: original.clone(),
            criterion_id: "criterion_payment_rail".to_owned(),
            proof_ref: None,
            evidence_refs: Vec::new(),
            confirmation_depth: None,
            payload: payload("status", "submitted"),
        });
        let proof = x402_tx_proof_reference(&tx_hash());
        let in_flight = effect_settlement_receipt(EffectSettlementReceiptInput {
            created_at: "2026-06-01T00:00:10Z".to_owned(),
            phase: EffectSettlementPhase::InFlight,
            original_receipt_ref: original.clone(),
            criterion_id: "criterion_payment_rail".to_owned(),
            proof_ref: Some(proof.clone()),
            evidence_refs: vec![Reference::runx(ReferenceType::Artifact, &provisional.id)],
            confirmation_depth: Some(1),
            payload: payload("status", "confirming"),
        });
        let sealed = effect_settlement_receipt(EffectSettlementReceiptInput {
            created_at: "2026-06-01T00:00:30Z".to_owned(),
            phase: EffectSettlementPhase::Sealed,
            original_receipt_ref: original.clone(),
            criterion_id: "criterion_payment_rail".to_owned(),
            proof_ref: Some(proof.clone()),
            evidence_refs: vec![Reference::runx(ReferenceType::Artifact, &in_flight.id)],
            confirmation_depth: Some(3),
            payload: payload("status", "settled"),
        });

        assert_eq!(provisional.family.as_ref(), "payment");
        assert_eq!(provisional.phase, EffectSettlementPhase::Provisional);
        assert_eq!(provisional.original_receipt_ref, original);
        assert_eq!(in_flight.phase, EffectSettlementPhase::InFlight);
        assert_eq!(in_flight.confirmation_depth, Some(1));
        assert_eq!(sealed.phase, EffectSettlementPhase::Sealed);
        assert_eq!(sealed.confirmation_depth, Some(3));
        assert_eq!(
            sealed.original_receipt_ref,
            provisional.original_receipt_ref
        );
        assert_eq!(sealed.proof_ref.as_ref(), Some(&proof));
        assert_eq!(sealed.evidence_refs.len(), 1);
        assert_eq!(proof.proof_kind, Some(ProofKind::PaymentRail));
        assert_ne!(provisional.id, in_flight.id);
        assert_ne!(in_flight.id, sealed.id);
        assert_ne!(provisional.id, sealed.id);
    }

    fn request<'a>(
        rail: &'a str,
        skill_settlement_status: Option<&'a str>,
    ) -> RailSettlementRequest<'a> {
        RailSettlementRequest {
            rail,
            counterparty: "merchant.example",
            amount_minor: 25,
            currency: "USD",
            idempotency_key: "mmid_1",
            proof_ref: "proof_ref_1",
            skill_settlement_status,
        }
    }

    #[derive(Clone, Debug)]
    struct FixtureHttpTransport {
        expected_url: String,
        status: u16,
        body: String,
    }

    impl FixtureHttpTransport {
        fn new(expected_url: &str, status: u16, body: String) -> Self {
            Self {
                expected_url: expected_url.to_owned(),
                status,
                body,
            }
        }
    }

    impl RuntimeHttpTransport for FixtureHttpTransport {
        fn send(
            &self,
            request: RuntimeHttpRequest,
        ) -> Result<RuntimeHttpResponse, RuntimeHttpError> {
            assert_eq!(request.method.as_str(), "POST");
            assert_eq!(request.url, self.expected_url);
            assert!(
                request
                    .headers
                    .iter()
                    .any(|header| header.name.eq_ignore_ascii_case("content-type")
                        && header.value == "application/json")
            );
            assert!(
                request
                    .body
                    .as_deref()
                    .unwrap_or_default()
                    .contains("template_digest")
            );
            Ok(RuntimeHttpResponse {
                status: self.status,
                body: self.body.clone(),
            })
        }
    }

    fn client_signed_response(
        request: &super::ExternalSignerRequest,
    ) -> Result<super::ExternalSignerSignedResponse, Box<dyn std::error::Error>> {
        let client = ExternalSignerClient::with_transport(
            "https://signer.local/sign",
            FixtureHttpTransport::new(
                "https://signer.local/sign",
                200,
                signed_response_body(&request.template_digest)?,
            ),
        );
        Ok(client.sign(request)?)
    }

    fn admission_token() -> PaymentAdmissionToken {
        PaymentAdmissionToken {
            purpose: "runx.payment_admission.v1".to_owned(),
            audience: "rail_settlement".to_owned(),
            principal: "principal_1".to_owned(),
            act: "act_pay_fulfill".to_owned(),
            rail: X402_RAIL.to_owned(),
            amount_minor: 25,
            currency: "USD".to_owned(),
            counterparty: Some("0x2222222222222222222222222222222222222222".to_owned()),
            run_id: "run_1".to_owned(),
            authority_digest: "sha256:authority".to_owned(),
            expires_at: "2026-06-01T00:05:00Z".to_owned(),
            money_movement_id: "sha256:money-movement".to_owned(),
            kid: "kid-admission-1".to_owned(),
            sig: "base64:signature".to_owned(),
        }
    }

    fn stripe_admission_token() -> PaymentAdmissionToken {
        PaymentAdmissionToken {
            purpose: "runx.payment_admission.v1".to_owned(),
            audience: "rail_settlement".to_owned(),
            principal: "principal_1".to_owned(),
            act: "act_stripe_pay".to_owned(),
            rail: STRIPE_SPT_RAIL.to_owned(),
            amount_minor: 1234,
            currency: "USD".to_owned(),
            counterparty: Some("acct_counterparty".to_owned()),
            run_id: "run_1".to_owned(),
            authority_digest: "sha256:authority".to_owned(),
            expires_at: "2026-06-01T00:05:00Z".to_owned(),
            money_movement_id: "sha256:stripe-money-movement".to_owned(),
            kid: "kid-admission-1".to_owned(),
            sig: "base64:signature".to_owned(),
        }
    }

    fn challenge() -> X402TransferAuthorizationChallenge {
        X402TransferAuthorizationChallenge {
            chain_id: 84532,
            token_contract: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
            verifying_contract: "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
            from: "0x1111111111111111111111111111111111111111".to_owned(),
            pay_to: "0x2222222222222222222222222222222222222222".to_owned(),
            valid_after: "2026-06-01T00:00:00Z".to_owned(),
            valid_before: "2026-06-01T00:05:00Z".to_owned(),
            currency: "USD".to_owned(),
            amount_minor: 25,
        }
    }

    fn signed_response_body(template_digest: &str) -> Result<String, Box<dyn std::error::Error>> {
        Ok(serde_json::to_string(&serde_json::json!({
            "schema": EXTERNAL_SIGNER_RESPONSE_SCHEMA,
            "status": "signed",
            "signer_address": "0x3333333333333333333333333333333333333333",
            "signature": format!("0x{}", "11".repeat(65)),
            "template_digest": template_digest,
        }))?)
    }

    fn facilitator_payment() -> X402FacilitatorPayment {
        X402FacilitatorPayment {
            payment_signature: format!("0x{}", "11".repeat(65)),
            template_digest: "sha256:template".to_owned(),
            money_movement_id: "sha256:money-movement".to_owned(),
        }
    }

    fn tx_hash() -> String {
        format!("0x{}", "aa".repeat(32))
    }

    fn payload(key: &str, value: &str) -> JsonObject {
        [(key.to_owned(), JsonValue::String(value.to_owned()))]
            .into_iter()
            .collect()
    }

    fn refusal_response_body(
        code: ExternalSignerRefusalCode,
        message: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        Ok(serde_json::to_string(&serde_json::json!({
            "schema": EXTERNAL_SIGNER_RESPONSE_SCHEMA,
            "status": "refused",
            "code": code,
            "message": message,
        }))?)
    }
}
