use std::env;
use std::path::PathBuf;

use runx_contracts::JsonValue;
#[cfg(not(feature = "payment-rails"))]
use runx_pay::DeterministicPaymentRailSupervisor;
use runx_pay::{
    PaymentRuntimeEffect,
    ledger::{X402_PAY_PAYMENT_PROFILE, persist_x402_payment_ledger_projection_event},
};
use runx_runtime::{
    HarnessReplayOutput, LocalOrchestrator, RUNX_RECEIPT_DIR_ENV, RuntimeEffectRegistry,
};

#[must_use]
pub fn local_orchestrator() -> LocalOrchestrator {
    LocalOrchestrator::with_effects(payment_effect_registry())
}

#[must_use]
pub fn payment_effect_registry() -> RuntimeEffectRegistry {
    #[cfg(feature = "payment-rails")]
    {
        return RuntimeEffectRegistry::with_effect(PaymentRuntimeEffect::new(
            payment_rails::RuntimeRailSupervisor::default(),
        ));
    }

    #[cfg(not(feature = "payment-rails"))]
    RuntimeEffectRegistry::with_effect(PaymentRuntimeEffect::new(
        DeterministicPaymentRailSupervisor,
    ))
}

pub fn persist_payment_ledger_projection(output: &HarnessReplayOutput) -> Result<(), String> {
    if metadata_string(output, "payment_ledger_profile") != Some(X402_PAY_PAYMENT_PROFILE) {
        return Ok(());
    }
    let Some(receipt_dir) = env::var_os(RUNX_RECEIPT_DIR_ENV).map(PathBuf::from) else {
        return Ok(());
    };
    let scenario_id = metadata_string(output, "payment_ledger_scenario_id")
        .ok_or_else(|| "metadata.payment_ledger_scenario_id is required".to_owned())?;
    persist_x402_payment_ledger_projection_event(
        receipt_dir,
        &format!("gx_{}", output.fixture.name),
        output.receipt.created_at.as_str(),
        &output.receipt,
        &output.steps,
        scenario_id,
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

fn metadata_string<'a>(output: &'a HarnessReplayOutput, key: &str) -> Option<&'a str> {
    output
        .fixture
        .metadata
        .get(key)
        .and_then(|value| match value {
            JsonValue::String(value) => Some(value.as_str()),
            _ => None,
        })
}

#[cfg(feature = "payment-rails")]
mod payment_rails {
    use runx_pay::PaymentRailSupervisor;
    use runx_pay::supervisor::{
        PaymentSupervisorError, PaymentSupervisorSettlementEvidence,
        PaymentSupervisorSettlementRequest,
    };
    use runx_runtime::adapters::payment_supervisor::{
        RailSettlementEvidence, RailSettlementRequest, RailSupervisor, RailSupervisorError,
    };

    #[derive(Clone, Debug, Default)]
    pub struct RuntimeRailSupervisor {
        dispatcher: RailSupervisor,
    }

    impl PaymentRailSupervisor for RuntimeRailSupervisor {
        fn settlement_evidence(
            &self,
            request: PaymentSupervisorSettlementRequest<'_>,
        ) -> Result<PaymentSupervisorSettlementEvidence, PaymentSupervisorError> {
            self.dispatcher
                .settlement_evidence(RailSettlementRequest {
                    rail: request.rail,
                    counterparty: request.counterparty,
                    amount_minor: request.amount_minor,
                    currency: request.currency,
                    idempotency_key: request.idempotency_key,
                    proof_ref: request.proof_ref,
                    skill_settlement_status: request.skill_settlement_status,
                })
                .map(map_evidence)
                .map_err(map_error)
        }
    }

    fn map_evidence(evidence: RailSettlementEvidence) -> PaymentSupervisorSettlementEvidence {
        PaymentSupervisorSettlementEvidence {
            verifier_id: evidence.verifier_id,
            proof_ref: evidence.proof_ref,
            rail: evidence.rail,
            counterparty: evidence.counterparty,
            amount_minor: evidence.amount_minor,
            currency: evidence.currency,
            idempotency_key: evidence.idempotency_key,
            settlement_status: evidence.settlement_status,
            provider_event_ref: evidence.provider_event_ref,
            shared_payment_token_ref: evidence.shared_payment_token_ref,
            admission_token_digest: evidence.admission_token_digest,
        }
    }

    fn map_error(error: RailSupervisorError) -> PaymentSupervisorError {
        match error {
            RailSupervisorError::SupervisorUnavailable { .. } => {
                PaymentSupervisorError::SupervisorUnavailable
            }
            RailSupervisorError::InvalidRailPacket { message } => {
                PaymentSupervisorError::InvalidRailPacket { message }
            }
            RailSupervisorError::SettlementNotFulfilled { status } => {
                PaymentSupervisorError::SettlementNotFulfilled { status }
            }
        }
    }
}
