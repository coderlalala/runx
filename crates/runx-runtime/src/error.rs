use std::path::PathBuf;

use runx_core::state_machine::FanoutSyncDecision;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("runtime I/O failed while {context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
    #[error("graph parse failed: {0}")]
    ParseGraph(#[from] runx_parser::ParseError),
    #[error("graph validation failed: {0}")]
    ValidateGraph(#[from] runx_parser::ValidationError),
    #[error("JSON serialization failed while {context}: {source}")]
    Json {
        context: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("graph step '{step_id}' is missing")]
    StepMissing { step_id: String },
    #[error("graph step '{step_id}' has no skill target")]
    StepMissingSkill { step_id: String },
    #[error("graph step '{step_id}' has invalid run configuration: {reason}")]
    InvalidRunStep { step_id: String, reason: String },
    #[error("graph step '{step_id}' uses unsupported run type '{run_type}'")]
    UnsupportedRunStep { step_id: String, run_type: String },
    #[error("graph step '{step_id}' is blocked: {reason}")]
    GraphBlocked { step_id: String, reason: String },
    #[error("graph step '{step_id}' failed planning: {reason}")]
    GraphPlanningFailed { step_id: String, reason: String },
    #[error("graph step '{step_id}' paused: {reason}")]
    GraphPaused {
        step_id: String,
        reason: String,
        sync_decision: Box<FanoutSyncDecision>,
    },
    #[error("graph step '{step_id}' escalated: {reason}")]
    GraphEscalated {
        step_id: String,
        reason: String,
        sync_decision: Box<FanoutSyncDecision>,
    },
    #[error("checkpoint graph '{checkpoint_graph}' cannot resume graph '{graph}'")]
    CheckpointGraphMismatch {
        checkpoint_graph: String,
        graph: String,
    },
    #[error("unsupported adapter '{adapter_type}'")]
    UnsupportedAdapter { adapter_type: String },
    #[error("cli-tool source is missing command")]
    MissingCommand,
    #[error("sandbox violation: {message}")]
    SandboxViolation { message: String },
    #[error("skill file is missing at {path}")]
    SkillFileMissing { path: PathBuf },
    #[error("skill '{skill_name}' failed: {message}")]
    SkillFailed { skill_name: String, message: String },
    #[error("harness receipt validation failed: {message}")]
    ReceiptInvalid { message: String },
}

impl RuntimeError {
    pub(crate) fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }

    pub(crate) fn json(context: impl Into<String>, source: serde_json::Error) -> Self {
        Self::Json {
            context: context.into(),
            source,
        }
    }
}
