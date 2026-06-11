use runx_contracts::{
    ExecutionSemantics, GovernedDisposition, InputContextCapture, JsonValue, OutcomeState,
    ReceiptOutcome, ReceiptSurfaceRef,
};

use crate::ValidationError;

use super::FIELDS;

pub(super) fn validate_execution_semantics(
    value: Option<&JsonValue>,
    field: &str,
) -> Result<Option<ExecutionSemantics>, ValidationError> {
    let Some(record) = FIELDS.optional_object(value, field)? else {
        return Ok(None);
    };
    Ok(Some(ExecutionSemantics {
        disposition: optional_disposition(
            record.get("disposition"),
            &format!("{field}.disposition"),
        )?,
        outcome_state: optional_outcome_state(
            record.get("outcome_state"),
            &format!("{field}.outcome_state"),
        )?,
        outcome: validate_outcome(record.get("outcome"), &format!("{field}.outcome"))?,
        input_context: validate_input_context(
            record.get("input_context"),
            &format!("{field}.input_context"),
        )?,
        surface_refs: validate_surface_refs(
            record.get("surface_refs"),
            &format!("{field}.surface_refs"),
        )?,
        evidence_refs: validate_surface_refs(
            record.get("evidence_refs"),
            &format!("{field}.evidence_refs"),
        )?,
    }))
}

fn validate_outcome(
    value: Option<&JsonValue>,
    field: &str,
) -> Result<Option<ReceiptOutcome>, ValidationError> {
    let Some(record) = FIELDS.optional_object(value, field)? else {
        return Ok(None);
    };
    Ok(Some(ReceiptOutcome {
        code: FIELDS.optional_string(record.get("code"), &format!("{field}.code"))?,
        summary: FIELDS.optional_string(record.get("summary"), &format!("{field}.summary"))?,
        observed_at: FIELDS
            .optional_string(record.get("observed_at"), &format!("{field}.observed_at"))?,
        data: FIELDS.optional_object(record.get("data"), &format!("{field}.data"))?,
    }))
}

fn validate_input_context(
    value: Option<&JsonValue>,
    field: &str,
) -> Result<Option<InputContextCapture>, ValidationError> {
    let Some(record) = FIELDS.optional_object(value, field)? else {
        return Ok(None);
    };
    let max_bytes = FIELDS.optional_u64(record.get("max_bytes"), &format!("{field}.max_bytes"))?;
    if matches!(max_bytes, Some(0)) {
        return Err(
            FIELDS.validation_error(format!("{field}.max_bytes must be a positive integer."))
        );
    }
    Ok(Some(InputContextCapture {
        capture: FIELDS.optional_bool(record.get("capture"), &format!("{field}.capture"))?,
        source: FIELDS.optional_string(record.get("source"), &format!("{field}.source"))?,
        max_bytes,
        snapshot: record.get("snapshot").cloned(),
    }))
}

fn validate_surface_refs(
    value: Option<&JsonValue>,
    field: &str,
) -> Result<Option<Vec<ReceiptSurfaceRef>>, ValidationError> {
    let Some(values) = optional_array(value, field)? else {
        return Ok(None);
    };
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let record = FIELDS.required_object(Some(value), &format!("{field}[{index}]"))?;
            Ok(ReceiptSurfaceRef {
                surface_type: FIELDS
                    .required_string(record.get("type"), &format!("{field}[{index}].type"))?,
                uri: FIELDS.required_string(record.get("uri"), &format!("{field}[{index}].uri"))?,
                label: FIELDS
                    .optional_string(record.get("label"), &format!("{field}[{index}].label"))?,
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn optional_array<'a>(
    value: Option<&'a JsonValue>,
    field: &str,
) -> Result<Option<&'a [JsonValue]>, ValidationError> {
    match value {
        None | Some(JsonValue::Null) => Ok(None),
        Some(JsonValue::Array(values)) => Ok(Some(values)),
        Some(_) => Err(FIELDS.validation_error(format!("{field} must be an array when present."))),
    }
}

fn optional_disposition(
    value: Option<&JsonValue>,
    field: &str,
) -> Result<Option<GovernedDisposition>, ValidationError> {
    match FIELDS.optional_string(value, field)?.as_deref() {
        None => Ok(None),
        Some("completed") => Ok(Some(GovernedDisposition::Completed)),
        Some("needs_agent") => Ok(Some(GovernedDisposition::NeedsAgent)),
        Some("policy_denied") => Ok(Some(GovernedDisposition::PolicyDenied)),
        Some("approval_required") => Ok(Some(GovernedDisposition::ApprovalRequired)),
        Some("observing") => Ok(Some(GovernedDisposition::Observing)),
        Some("escalated") => Ok(Some(GovernedDisposition::Escalated)),
        Some(_) => Err(FIELDS.validation_error(format!(
            "{field} must be one of completed, needs_agent, policy_denied, approval_required, observing, escalated."
        ))),
    }
}

fn optional_outcome_state(
    value: Option<&JsonValue>,
    field: &str,
) -> Result<Option<OutcomeState>, ValidationError> {
    match FIELDS.optional_string(value, field)?.as_deref() {
        None => Ok(None),
        Some("pending") => Ok(Some(OutcomeState::Pending)),
        Some("complete") => Ok(Some(OutcomeState::Complete)),
        Some("expired") => Ok(Some(OutcomeState::Expired)),
        Some(_) => Err(FIELDS.validation_error(format!(
            "{field} must be one of pending, complete, or expired."
        ))),
    }
}
