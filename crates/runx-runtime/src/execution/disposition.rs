use runx_contracts::{ClosureDisposition, JsonValue};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub(crate) enum ClosureDispositionParseError {
    #[error("agent answer closure must be an object")]
    ClosureNotObject,
    #[error("agent answer closure.disposition is required")]
    MissingDisposition,
    #[error("agent answer closure.disposition must be a string")]
    DispositionNotString,
    #[error("agent answer closure.disposition {0:?} is not supported")]
    UnsupportedDisposition(String),
}

pub(crate) fn parse_agent_answer_disposition(
    answer: &JsonValue,
) -> Result<ClosureDisposition, ClosureDispositionParseError> {
    let closure = answer
        .as_object()
        .and_then(|object| object.get("closure"))
        .ok_or(ClosureDispositionParseError::MissingDisposition)?;
    let closure = closure
        .as_object()
        .ok_or(ClosureDispositionParseError::ClosureNotObject)?;
    let disposition = closure
        .get("disposition")
        .ok_or(ClosureDispositionParseError::MissingDisposition)?;
    let disposition = disposition
        .as_str()
        .ok_or(ClosureDispositionParseError::DispositionNotString)?;
    parse_closure_disposition(disposition)
}

pub(crate) fn parse_closure_disposition(
    disposition: &str,
) -> Result<ClosureDisposition, ClosureDispositionParseError> {
    match disposition {
        "closed" => Ok(ClosureDisposition::Closed),
        "deferred" => Ok(ClosureDisposition::Deferred),
        "superseded" => Ok(ClosureDisposition::Superseded),
        "declined" => Ok(ClosureDisposition::Declined),
        "blocked" => Ok(ClosureDisposition::Blocked),
        "failed" => Ok(ClosureDisposition::Failed),
        "killed" => Ok(ClosureDisposition::Killed),
        "timed_out" => Ok(ClosureDisposition::TimedOut),
        other => Err(ClosureDispositionParseError::UnsupportedDisposition(
            other.to_owned(),
        )),
    }
}
