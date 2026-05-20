use runx_contracts::{ClosureDisposition, HarnessReceipt, HarnessReceiptSchema, HarnessState};
use runx_receipts::{
    ReceiptProofContextProvider, canonical_receipt_body_digest, canonical_receipt_digest,
    validate_harness_receipt_proof, verify_harness_receipt_proof,
};

use crate::execution::harness::fixtures::{HarnessExpectedStatus, HarnessReceiptExpectation};
use crate::execution::harness::runner::{HarnessReplayError, HarnessReplayOutput};
use crate::receipts::RuntimeReceiptProofContextProvider;

#[derive(Clone, Debug, PartialEq)]
pub struct HarnessReplayReceipt {
    pub receipt_id: String,
    pub harness_id: String,
    pub state: HarnessState,
    pub disposition: ClosureDisposition,
    pub reason_code: String,
    pub act_ids: Vec<String>,
    pub decision_ids: Vec<String>,
    pub child_receipt_refs: Vec<String>,
    pub verification_refs: Vec<String>,
}

pub(super) fn assert_expectations(output: &HarnessReplayOutput) -> Result<(), HarnessReplayError> {
    if let Some(expected_status) = &output.fixture.expect.status {
        assert_equal(
            "expect.status",
            status_name(expected_status),
            status_name(&output.status),
        )?;
    }
    if let Some(expected_receipt) = &output.fixture.expect.receipt {
        assert_receipt(expected_receipt, &output.receipt)?;
    }
    if !output.fixture.expect.steps.is_empty() {
        let actual = output
            .step_receipts
            .iter()
            .map(receipt_step_name)
            .collect::<Vec<_>>();
        assert_equal(
            "expect.steps",
            output.fixture.expect.steps.join(","),
            actual.join(","),
        )?;
    }
    Ok(())
}

pub(super) fn status_from_disposition(disposition: &ClosureDisposition) -> HarnessExpectedStatus {
    match disposition {
        ClosureDisposition::Closed => HarnessExpectedStatus::Sealed,
        ClosureDisposition::Deferred => HarnessExpectedStatus::NeedsAgent,
        ClosureDisposition::Blocked => HarnessExpectedStatus::PolicyDenied,
        ClosureDisposition::TimedOut
        | ClosureDisposition::Declined
        | ClosureDisposition::Failed
        | ClosureDisposition::Killed
        | ClosureDisposition::Superseded => HarnessExpectedStatus::Failure,
    }
}

fn assert_receipt(
    expected: &HarnessReceiptExpectation,
    actual: &HarnessReceipt,
) -> Result<(), HarnessReplayError> {
    assert_receipt_proof(actual)?;
    assert_equal(
        "expect.receipt.schema",
        schema_name(&expected.schema),
        schema_name(&actual.schema),
    )?;
    if let Some(expected_id) = &expected.receipt_id {
        assert_equal("expect.receipt.receipt_id", expected_id, &actual.id)?;
    }
    let summary = summarize_receipt(actual);
    assert_receipt_identity(expected, &summary)?;
    assert_receipt_lists(expected, &summary)?;
    assert_receipt_digests(expected, actual)
}

fn assert_receipt_identity(
    expected: &HarnessReceiptExpectation,
    summary: &HarnessReplayReceipt,
) -> Result<(), HarnessReplayError> {
    if let Some(expected_harness_id) = &expected.harness_id {
        assert_equal(
            "expect.receipt.harness_id",
            expected_harness_id,
            &summary.harness_id,
        )?;
    }
    if let Some(expected_state) = &expected.state {
        assert_equal(
            "expect.receipt.state",
            state_name(expected_state),
            state_name(&summary.state),
        )?;
    }
    if let Some(expected_disposition) = &expected.disposition {
        assert_equal(
            "expect.receipt.disposition",
            disposition_name(expected_disposition),
            disposition_name(&summary.disposition),
        )?;
    }
    if let Some(expected_reason_code) = &expected.reason_code {
        assert_equal(
            "expect.receipt.reason_code",
            expected_reason_code,
            &summary.reason_code,
        )?;
    }
    Ok(())
}

fn assert_receipt_lists(
    expected: &HarnessReceiptExpectation,
    summary: &HarnessReplayReceipt,
) -> Result<(), HarnessReplayError> {
    assert_optional_list(
        "expect.receipt.act_ids",
        &expected.act_ids,
        &summary.act_ids,
    )?;
    assert_optional_list(
        "expect.receipt.decision_ids",
        &expected.decision_ids,
        &summary.decision_ids,
    )?;
    assert_optional_list(
        "expect.receipt.child_receipt_refs",
        &expected.child_receipt_refs,
        &summary.child_receipt_refs,
    )?;
    assert_optional_list(
        "expect.receipt.verification_refs",
        &expected.verification_refs,
        &summary.verification_refs,
    )
}

fn assert_receipt_digests(
    expected: &HarnessReceiptExpectation,
    actual: &HarnessReceipt,
) -> Result<(), HarnessReplayError> {
    if let Some(expected_body_digest) = &expected.body_digest {
        let body_digest = canonical_receipt_body_digest(actual).map_err(receipt_digest_error)?;
        assert_equal(
            "expect.receipt.body_digest",
            expected_body_digest,
            body_digest,
        )?;
    }
    if let Some(expected_digest) = &expected.receipt_digest {
        let receipt_digest = canonical_receipt_digest(actual).map_err(receipt_digest_error)?;
        assert_equal(
            "expect.receipt.receipt_digest",
            expected_digest,
            receipt_digest,
        )?;
    }
    Ok(())
}

fn assert_receipt_proof(receipt: &HarnessReceipt) -> Result<(), HarnessReplayError> {
    let proof_contexts = RuntimeReceiptProofContextProvider::local_development();
    let context = proof_contexts.proof_context(receipt);
    validate_harness_receipt_proof(receipt, &context).map_err(|verification| {
        HarnessReplayError::ReceiptProofInvalid {
            receipt_id: receipt.id.clone(),
            findings: format!("{:?}", verification.findings),
        }
    })?;
    let verification = verify_harness_receipt_proof(receipt, &context);
    if verification.valid {
        Ok(())
    } else {
        Err(HarnessReplayError::ReceiptProofInvalid {
            receipt_id: receipt.id.clone(),
            findings: format!("{:?}", verification.findings),
        })
    }
}

fn receipt_digest_error(error: runx_receipts::ReceiptError) -> HarnessReplayError {
    HarnessReplayError::ReceiptDigest {
        message: error.to_string(),
    }
}

fn summarize_receipt(receipt: &HarnessReceipt) -> HarnessReplayReceipt {
    HarnessReplayReceipt {
        receipt_id: receipt.id.clone(),
        harness_id: receipt.harness.harness_id.clone(),
        state: receipt.harness.state.clone(),
        disposition: receipt.seal.disposition.clone(),
        reason_code: receipt.seal.reason_code.clone(),
        act_ids: receipt
            .harness
            .acts
            .iter()
            .map(|act| act.act_id.clone())
            .collect(),
        decision_ids: receipt
            .harness
            .decisions
            .iter()
            .map(|decision| decision.decision_id.clone())
            .collect(),
        child_receipt_refs: receipt
            .harness
            .child_harness_receipt_refs
            .iter()
            .map(|reference| reference.uri.clone())
            .collect(),
        verification_refs: receipt
            .harness
            .acts
            .iter()
            .flat_map(|act| act.verification_refs.iter())
            .map(|reference| reference.uri.clone())
            .collect(),
    }
}

fn receipt_step_name(receipt: &HarnessReceipt) -> String {
    receipt.harness.acts.first().map_or_else(
        || receipt.harness.harness_id.clone(),
        |act| act.act_id.trim_start_matches("act_").to_owned(),
    )
}

fn assert_optional_list(
    field: &'static str,
    expected: &[String],
    actual: &[String],
) -> Result<(), HarnessReplayError> {
    if expected.is_empty() {
        return Ok(());
    }
    assert_equal(field, expected.join(","), actual.join(","))
}

fn assert_equal(
    field: &'static str,
    expected: impl AsRef<str>,
    actual: impl AsRef<str>,
) -> Result<(), HarnessReplayError> {
    let expected = expected.as_ref();
    let actual = actual.as_ref();
    if expected == actual {
        return Ok(());
    }
    Err(HarnessReplayError::Mismatch {
        field,
        expected: expected.to_owned(),
        actual: actual.to_owned(),
    })
}

fn schema_name(schema: &HarnessReceiptSchema) -> &'static str {
    match schema {
        HarnessReceiptSchema::V1 => "runx.harness_receipt.v1",
    }
}

fn state_name(state: &HarnessState) -> &'static str {
    match state {
        HarnessState::Forming => "forming",
        HarnessState::Admitted => "admitted",
        HarnessState::Running => "running",
        HarnessState::Waiting => "waiting",
        HarnessState::Delegated => "delegated",
        HarnessState::Sealing => "sealing",
        HarnessState::Sealed => "sealed",
        HarnessState::Killed => "killed",
        HarnessState::TimedOut => "timed_out",
        HarnessState::Failed => "failed",
        HarnessState::Superseded => "superseded",
    }
}

fn disposition_name(disposition: &ClosureDisposition) -> &'static str {
    match disposition {
        ClosureDisposition::Closed => "closed",
        ClosureDisposition::Deferred => "deferred",
        ClosureDisposition::Superseded => "superseded",
        ClosureDisposition::Declined => "declined",
        ClosureDisposition::Blocked => "blocked",
        ClosureDisposition::Failed => "failed",
        ClosureDisposition::Killed => "killed",
        ClosureDisposition::TimedOut => "timed_out",
    }
}

fn status_name(status: &HarnessExpectedStatus) -> &'static str {
    match status {
        HarnessExpectedStatus::Sealed => "sealed",
        HarnessExpectedStatus::Failure => "failure",
        HarnessExpectedStatus::NeedsAgent => "needs_agent",
        HarnessExpectedStatus::PolicyDenied => "policy_denied",
        HarnessExpectedStatus::Escalated => "escalated",
    }
}
