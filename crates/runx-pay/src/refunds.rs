use runx_contracts::EffectSettlementPhase;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefundAdmissionCase {
    pub name: String,
    pub input: RefundAdmissionInput,
    pub expected: RefundAdmissionDecision,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefundAdmissionInput {
    pub charge: RefundableCharge,
    pub refund: RefundRequest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefundableCharge {
    pub money_movement_id: String,
    pub rail: String,
    pub phase: EffectSettlementPhase,
    pub amount_minor: u64,
    pub currency: String,
    pub payer_ref: String,
    pub proof_ref: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefundRequest {
    pub amount_minor: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_counterparty: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum RefundAdmissionDecision {
    Admitted {
        reversal: RefundReversal,
    },
    Refused {
        code: RefundRefusalCode,
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefundReversal {
    pub rail: String,
    pub amount_minor: u64,
    pub currency: String,
    pub counterparty: String,
    pub original_money_movement_id: String,
    pub original_proof_ref: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefundRefusalCode {
    ChargeNotSealed,
    ChargeReversed,
    EmptyRefund,
    RefundExceedsCharge,
    CounterpartyMismatch,
}

#[derive(Debug, Error)]
pub enum RefundAdmissionError {
    #[error("refund admission fixture {name} expected {expected:?}, got {actual:?}")]
    FixtureMismatch {
        name: String,
        expected: RefundAdmissionDecision,
        actual: RefundAdmissionDecision,
    },
}

pub fn admit_refund(input: &RefundAdmissionInput) -> RefundAdmissionDecision {
    if input.charge.phase == EffectSettlementPhase::Reversed {
        return refused(
            RefundRefusalCode::ChargeReversed,
            "refund refused because the linked charge is already reversed",
        );
    }
    if input.charge.phase != EffectSettlementPhase::Sealed {
        return refused(
            RefundRefusalCode::ChargeNotSealed,
            "refund refused because the linked charge is not sealed",
        );
    }
    if input.refund.amount_minor == 0 {
        return refused(
            RefundRefusalCode::EmptyRefund,
            "refund amount must be positive",
        );
    }
    if input.refund.amount_minor > input.charge.amount_minor {
        return refused(
            RefundRefusalCode::RefundExceedsCharge,
            "refund amount exceeds the linked charge",
        );
    }
    if let Some(counterparty) = input.refund.requested_counterparty.as_deref()
        && counterparty != input.charge.payer_ref
    {
        return refused(
            RefundRefusalCode::CounterpartyMismatch,
            "refund reversal must target the recorded payer",
        );
    }
    RefundAdmissionDecision::Admitted {
        reversal: RefundReversal {
            rail: input.charge.rail.clone(),
            amount_minor: input.refund.amount_minor,
            currency: input.charge.currency.clone(),
            counterparty: input.charge.payer_ref.clone(),
            original_money_movement_id: input.charge.money_movement_id.clone(),
            original_proof_ref: input.charge.proof_ref.clone(),
        },
    }
}

pub fn verify_refund_admission_case(
    case: &RefundAdmissionCase,
) -> Result<(), RefundAdmissionError> {
    let actual = admit_refund(&case.input);
    if actual == case.expected {
        Ok(())
    } else {
        Err(RefundAdmissionError::FixtureMismatch {
            name: case.name.clone(),
            expected: case.expected.clone(),
            actual,
        })
    }
}

fn refused(code: RefundRefusalCode, reason: &str) -> RefundAdmissionDecision {
    RefundAdmissionDecision::Refused {
        code,
        reason: reason.to_owned(),
    }
}
