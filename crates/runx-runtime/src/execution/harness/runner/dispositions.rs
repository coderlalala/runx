//! Disposition and metadata-shape helpers for harness replay. Pure functions
//! that translate fixture-shaped JSON into typed runtime values.

use runx_contracts::{ClosureDisposition, JsonObject, JsonValue};

use super::super::super::super::adapter::{InvocationStatus, SkillOutput};
use super::super::fixtures::{HarnessExpectedStatus, HarnessFixture};
use super::HarnessReplayError;
use crate::RuntimeError;
use crate::execution::disposition::parse_agent_answer_disposition;

pub(super) fn agent_task_output(
    fixture: &HarnessFixture,
    request_id: &str,
) -> Result<SkillOutput, HarnessReplayError> {
    let mut metadata = JsonObject::new();
    metadata.insert(
        "agent_request_id".to_owned(),
        JsonValue::String(request_id.to_owned()),
    );
    let payload = fixture
        .caller
        .get("answers")
        .and_then(JsonValue::as_object)
        .and_then(|answers| answers.get(request_id))
        .cloned()
        .unwrap_or(JsonValue::Null);
    if matches!(payload, JsonValue::Null) {
        return Ok(SkillOutput {
            status: InvocationStatus::Failure,
            stdout: String::new(),
            stderr: format!("missing replay answer for {request_id}"),
            exit_code: None,
            duration_ms: 0,
            metadata,
        });
    }
    Ok(SkillOutput {
        status: InvocationStatus::Success,
        stdout: serde_json::to_string(&payload).map_err(|source| RuntimeError::Json {
            context: format!("serializing replay answer {request_id}"),
            source,
        })?,
        stderr: String::new(),
        exit_code: Some(0),
        duration_ms: 0,
        metadata,
    })
}

pub(super) fn skill_output_object(output: &SkillOutput) -> JsonObject {
    let mut object = JsonObject::new();
    if let Ok(parsed) = serde_json::from_str::<JsonValue>(&output.stdout) {
        object.insert("skill_claim".to_owned(), parsed);
    }
    object
}

pub(super) fn string_metadata<'a>(fixture: &'a HarnessFixture, field: &str) -> Option<&'a str> {
    match fixture.metadata.get(field) {
        Some(JsonValue::String(value)) => Some(value),
        _ => None,
    }
}

pub(super) fn required_string_metadata(
    object: &JsonObject,
    field_path: &str,
    field: &str,
) -> Result<String, HarnessReplayError> {
    match object.get(field) {
        Some(JsonValue::String(value)) if !value.is_empty() => Ok(value.clone()),
        Some(_) => Err(HarnessReplayError::InvalidReplayMetadata {
            field: field_path.to_owned(),
            message: "non-empty string is required".to_owned(),
        }),
        None => Err(HarnessReplayError::InvalidReplayMetadata {
            field: field_path.to_owned(),
            message: "field is required".to_owned(),
        }),
    }
}

pub(super) fn agent_answer_disposition(
    answer: &JsonValue,
) -> Result<ClosureDisposition, HarnessReplayError> {
    parse_agent_answer_disposition(answer).map_err(|error| {
        HarnessReplayError::InvalidReplayMetadata {
            field: "caller.answers.*.closure.disposition".to_owned(),
            message: error.to_string(),
        }
    })
}

pub(super) fn disposition_from_expected_status(
    status: &HarnessExpectedStatus,
) -> ClosureDisposition {
    match status {
        HarnessExpectedStatus::Sealed => ClosureDisposition::Closed,
        HarnessExpectedStatus::Failure => ClosureDisposition::Failed,
        HarnessExpectedStatus::NeedsAgent => ClosureDisposition::Deferred,
        HarnessExpectedStatus::PolicyDenied => ClosureDisposition::Blocked,
        HarnessExpectedStatus::Escalated => ClosureDisposition::Deferred,
    }
}

pub(super) fn process_reason_code(disposition: &ClosureDisposition) -> String {
    format!("process_{}", disposition_suffix(disposition))
}

pub(super) fn named_reason_code(name: &str, disposition: &ClosureDisposition) -> String {
    format!("{name}_{}", disposition_suffix(disposition))
}

pub(super) fn disposition_suffix(disposition: &ClosureDisposition) -> &'static str {
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
