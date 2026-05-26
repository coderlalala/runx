// rust-style-allow: large-file because harness replay owns fixture loading,
// adapter invocation, receipt assertion, and graph replay sealing as one
// deterministic proof path until MCP replay creates a separate module boundary.
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(feature = "cli-tool")]
use std::time::{SystemTime, UNIX_EPOCH};

use runx_contracts::{
    ClosureDisposition, ExecutionEvent, JsonObject, JsonValue, Receipt, ResolutionRequest,
    ResolutionResponse, ResolutionResponseActor,
};
use runx_core::state_machine::StepAdmissionWitness;
use runx_parser::{
    SkillRunnerDefinition, SkillRunnerManifest, parse_runner_manifest_yaml,
    validate_runner_manifest,
};
use thiserror::Error;

use super::super::graph::load_skill;
use super::assertions::{assert_expectations, status_from_disposition};
use super::fixtures::{
    HarnessExpectedStatus, HarnessFixture, HarnessFixtureError, HarnessFixtureKind,
    fixture_kind_name, load_harness_fixture,
};
use crate::RuntimeError;
use crate::adapter::{InvocationStatus, SkillAdapter, SkillInvocation, SkillOutput};
use crate::agent_invocation::{AgentActInvocationSourceType, agent_act_invocation_id};
use crate::execution::runner::{GraphRun, Runtime, RuntimeOptions, StepRun};
use crate::host::Host;
use crate::payment::ledger::{
    X402_PAY_PAYMENT_PROFILE, persist_x402_payment_ledger_projection_event,
};
#[cfg(feature = "cli-tool")]
use crate::payment::state::{
    FileBackedPaymentStateStore, PaymentIdempotencyKey, PaymentRecoveryState,
    RUNX_PAYMENT_STATE_PATH_ENV, RailMutationStatus,
};
#[cfg(feature = "cli-tool")]
use crate::payment::supervisor::{
    PAYMENT_RAIL_SUPERVISOR_VERIFIER_ID, PaymentRailSupervisor, PaymentSupervisorError,
    PaymentSupervisorSettlementEvidence, PaymentSupervisorSettlementRequest,
    RuntimePaymentSupervisor,
};
#[cfg(feature = "cli-tool")]
use crate::receipts::RuntimeReceiptSignaturePolicy;
#[cfg(feature = "cli-tool")]
use crate::receipts::paths::RUNX_CWD_ENV;
use crate::receipts::paths::RUNX_RECEIPT_DIR_ENV;
use crate::receipts::{
    GraphClosure, StepReceiptWithDisposition, graph_receipt_with_disposition_and_policy,
    step_receipt_with_disposition_and_policy,
};
use crate::services::{ReceiptServices, WorkspaceEnv};

#[derive(Clone, Debug)]
pub struct HarnessReplayOutput {
    pub fixture: HarnessFixture,
    pub status: HarnessExpectedStatus,
    pub receipt: Receipt,
    pub step_receipts: Vec<Receipt>,
    pub skill_output: Option<SkillOutput>,
}

#[derive(Debug, Error)]
pub enum HarnessReplayError {
    #[error(transparent)]
    Fixture(#[from] HarnessFixtureError),
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error("harness fixture target {target} has no parent directory")]
    TargetWithoutParent { target: PathBuf },
    #[error("harness expectation mismatch at {field}: expected {expected}, actual {actual}")]
    Mismatch {
        field: &'static str,
        expected: String,
        actual: String,
    },
    #[error("receipt digest failed: {message}")]
    ReceiptDigest { message: String },
    #[error("receipt proof failed for {receipt_id}: {findings}")]
    ReceiptProofInvalid {
        receipt_id: String,
        findings: String,
    },
    #[error("harness fixture mode {mode} at {field_path} is not yet supported by the Rust harness")]
    UnsupportedFixtureMode { mode: String, field_path: String },
    #[error("invalid harness replay metadata at {field}: {message}")]
    InvalidReplayMetadata { field: String, message: String },
    #[error(
        "native cli-tool harness replay is unavailable because runx-runtime was built without the cli-tool feature"
    )]
    CliToolFeatureDisabled,
}

pub fn run_harness_fixture(
    fixture_path: impl AsRef<Path>,
) -> Result<HarnessReplayOutput, HarnessReplayError> {
    #[cfg(feature = "cli-tool")]
    {
        run_harness_fixture_with_adapter(
            fixture_path,
            crate::adapters::cli_tool::CliToolAdapter,
            fixture_runtime_options()?,
        )
    }
    #[cfg(not(feature = "cli-tool"))]
    {
        let _ = fixture_path;
        Err(HarnessReplayError::CliToolFeatureDisabled)
    }
}

#[cfg(feature = "cli-tool")]
pub fn run_harness_fixture_cli_tool(
    fixture_path: impl AsRef<Path>,
) -> Result<HarnessReplayOutput, HarnessReplayError> {
    run_harness_fixture_with_adapter(
        fixture_path,
        crate::adapters::cli_tool::CliToolAdapter,
        fixture_runtime_options()?,
    )
}

#[cfg(feature = "cli-tool")]
fn fixture_runtime_options() -> Result<RuntimeOptions, HarnessReplayError> {
    Ok(RuntimeOptions {
        created_at: crate::time::DEFAULT_CREATED_AT.to_owned(),
        payment_supervisor: RuntimePaymentSupervisor::from_supervisor(FixturePaymentSupervisor),
        ..RuntimeOptions::from_process_env()?
    })
}

#[cfg(feature = "cli-tool")]
#[derive(Clone, Debug)]
struct FixturePaymentSupervisor;

#[cfg(feature = "cli-tool")]
impl PaymentRailSupervisor for FixturePaymentSupervisor {
    fn settlement_evidence(
        &self,
        request: PaymentSupervisorSettlementRequest<'_>,
    ) -> Result<PaymentSupervisorSettlementEvidence, PaymentSupervisorError> {
        match request.proof_ref {
            "receipt-proof:mock:x402-pay-approval-001"
            | "receipt-proof:mock:paid-echo-001"
            | "receipt-proof:stripe-spt:demo-search-001" => {
                Ok(PaymentSupervisorSettlementEvidence {
                    verifier_id: PAYMENT_RAIL_SUPERVISOR_VERIFIER_ID.to_owned(),
                    proof_ref: request.proof_ref.to_owned(),
                    rail: request.rail.to_owned(),
                    counterparty: request.counterparty.to_owned(),
                    amount_minor: request.amount_minor,
                    currency: request.currency.to_owned(),
                    idempotency_key: request.idempotency_key.to_owned(),
                    settlement_status: Some("fulfilled".to_owned()),
                    provider_event_ref: Some(format!("fixture:event:{}", request.proof_ref)),
                })
            }
            _ => Err(PaymentSupervisorError::InvalidSupervisorEvidence {
                message: format!(
                    "fixture supervisor has no settlement for {}",
                    request.proof_ref
                ),
            }),
        }
    }
}

pub fn run_harness_fixture_with_adapter<A>(
    fixture_path: impl AsRef<Path>,
    adapter: A,
    options: RuntimeOptions,
) -> Result<HarnessReplayOutput, HarnessReplayError>
where
    A: SkillAdapter,
{
    let fixture_path = fixture_path.as_ref();
    let fixture = load_harness_fixture(fixture_path)?;
    let target_path = resolve_target_path(fixture_path, &fixture.target)?;
    let output = match fixture.kind {
        HarnessFixtureKind::Skill | HarnessFixtureKind::A2a | HarnessFixtureKind::Agent => {
            run_skill_fixture(&fixture, target_path, adapter, options)?
        }
        HarnessFixtureKind::AgentStep => run_agent_step_fixture(&fixture, options)?,
        HarnessFixtureKind::Graph if is_fixture_replay_graph(&fixture) => {
            run_graph_replay_fixture(&fixture, options)?
        }
        HarnessFixtureKind::Graph if is_x402_idempotency_sequence_graph(&fixture) => {
            run_x402_idempotency_sequence_fixture(&fixture, &target_path, options)?
        }
        HarnessFixtureKind::Graph => run_graph_fixture(&fixture, &target_path, adapter, options)?,
        HarnessFixtureKind::Mcp => {
            return Err(HarnessReplayError::UnsupportedFixtureMode {
                mode: fixture_kind_name(&fixture.kind).to_owned(),
                field_path: "kind".to_owned(),
            });
        }
    };
    assert_expectations(&output)?;
    Ok(output)
}

fn run_agent_step_fixture(
    fixture: &HarnessFixture,
    options: RuntimeOptions,
) -> Result<HarnessReplayOutput, HarnessReplayError> {
    let replay_name = fixture.runner.as_deref().unwrap_or(&fixture.name);
    let request_id = format!("agent_step.{replay_name}.output");
    let output = agent_step_output(fixture, &request_id)?;
    let disposition = fixture
        .expect
        .status
        .as_ref()
        .map(disposition_from_expected_status)
        .unwrap_or_else(|| {
            if output.succeeded() {
                ClosureDisposition::Closed
            } else {
                ClosureDisposition::Failed
            }
        });
    let receipt = step_receipt_with_disposition_and_policy(
        StepReceiptWithDisposition {
            graph_name: &fixture.name,
            step_id: &fixture.name,
            attempt: 1,
            output: &output,
            created_at: &options.created_at,
            disposition: disposition.clone(),
            reason_code: process_reason_code(&disposition),
            summary: format!("agent-step {} completed", fixture.name),
        },
        options.signature_policy(),
    )?;
    Ok(HarnessReplayOutput {
        fixture: fixture.clone(),
        status: status_from_disposition(&receipt.seal.disposition),
        receipt,
        step_receipts: Vec::new(),
        skill_output: Some(output),
    })
}

#[derive(Clone, Debug)]
struct GraphReplayStep {
    step_id: String,
    task: String,
    request_id: String,
}

fn is_fixture_replay_graph(fixture: &HarnessFixture) -> bool {
    string_metadata(fixture, "graph_shape") == Some("fixture_replay")
}

fn is_x402_idempotency_sequence_graph(fixture: &HarnessFixture) -> bool {
    string_metadata(fixture, "graph_shape") == Some("x402_idempotency_sequence")
}

// rust-style-allow: long-function - drives the multi-stage x402 idempotency fixture as one linear
// replay sequence; the ordered assertions are clearer kept together than split across helpers.
#[cfg(feature = "cli-tool")]
fn run_x402_idempotency_sequence_fixture(
    fixture: &HarnessFixture,
    graph_path: &Path,
    options: RuntimeOptions,
) -> Result<HarnessReplayOutput, HarnessReplayError> {
    let scenario = required_string_metadata(
        &fixture.metadata,
        "metadata.x402_idempotency_scenario",
        "x402_idempotency_scenario",
    )?;
    let temp_dir = create_harness_temp_dir(&fixture.name)?;
    let payment_state_path = temp_dir.join("payment-state.json");
    let rail_count_path = temp_dir.join("rail-count.txt");

    let first = x402_idempotency_run_env(
        &options,
        fixture,
        &payment_state_path,
        &rail_count_path,
        &scenario,
        true,
    )?;
    let second = x402_idempotency_run_env(
        &options,
        fixture,
        &payment_state_path,
        &rail_count_path,
        &scenario,
        false,
    )?;
    let output = match scenario.as_str() {
        "replay" => {
            let first_run = run_x402_idempotency_graph(graph_path, fixture, first)?;
            let second_run = run_x402_idempotency_graph(graph_path, fixture, second)?;
            assert_x402_rail_invocation_count(&rail_count_path, 1)?;
            assert_x402_replay_state(&payment_state_path, "payment:paid-echo-001")?;
            let first_fulfill = step_receipt_digest(&first_run, "fulfill")?;
            let second_fulfill = step_receipt_digest(&second_run, "fulfill")?;
            if first_fulfill != second_fulfill {
                return Err(HarnessReplayError::Mismatch {
                    field: "metadata.x402_idempotency_replay.fulfill_receipt_digest",
                    expected: first_fulfill,
                    actual: second_fulfill,
                });
            }
            replay_output_from_graph(fixture, second_run)
        }
        "capability_reuse" => {
            let first_run = run_x402_idempotency_graph(graph_path, fixture, first)?;
            let second_error = run_x402_idempotency_graph(graph_path, fixture, second)
                .err()
                .ok_or_else(|| HarnessReplayError::Mismatch {
                    field: "metadata.x402_idempotency_scenario",
                    expected: "second run denied before rail".to_owned(),
                    actual: "second run succeeded".to_owned(),
                })?;
            assert_authority_denied_contains(&second_error, "already consumed")?;
            assert_x402_rail_invocation_count(&rail_count_path, 1)?;
            sequence_error_output(
                fixture,
                first_run,
                &options.created_at,
                options.signature_policy(),
                HarnessExpectedStatus::PolicyDenied,
                GraphClosure {
                    disposition: ClosureDisposition::Blocked,
                    reason_code: "x402_idempotency_capability_reuse_blocked".to_owned(),
                    summary: "second spend capability use denied before rail".to_owned(),
                },
            )?
        }
        "crash_recovery" => {
            let first_error = run_x402_idempotency_graph(graph_path, fixture, first)
                .err()
                .ok_or_else(|| HarnessReplayError::Mismatch {
                    field: "metadata.x402_idempotency_scenario",
                    expected: "first run failed after partial rail mutation".to_owned(),
                    actual: "first run succeeded".to_owned(),
                })?;
            assert_skill_failed_contains(&first_error, "partial rail mutation")?;
            let second_error = run_x402_idempotency_graph(graph_path, fixture, second)
                .err()
                .ok_or_else(|| HarnessReplayError::Mismatch {
                    field: "metadata.x402_idempotency_scenario",
                    expected: "second run escalated recovery before rail".to_owned(),
                    actual: "second run succeeded".to_owned(),
                })?;
            assert_authority_denied_contains(&second_error, "recovery escalated")?;
            assert_x402_rail_invocation_count(&rail_count_path, 1)?;
            assert_x402_escalated_state(&payment_state_path, "payment:paid-echo-001")?;
            empty_sequence_error_output(
                fixture,
                &options.created_at,
                options.signature_policy(),
                HarnessExpectedStatus::Escalated,
                GraphClosure {
                    disposition: ClosureDisposition::Deferred,
                    reason_code: "x402_idempotency_recovery_escalated".to_owned(),
                    summary: "partial rail mutation recovery escalated before retry".to_owned(),
                },
            )?
        }
        other => {
            return Err(HarnessReplayError::InvalidReplayMetadata {
                field: "metadata.x402_idempotency_scenario".to_owned(),
                message: format!("unsupported x402 idempotency scenario {other:?}"),
            });
        }
    };
    let _ = fs::remove_dir_all(&temp_dir);
    Ok(output)
}

#[cfg(not(feature = "cli-tool"))]
fn run_x402_idempotency_sequence_fixture(
    _fixture: &HarnessFixture,
    _graph_path: &Path,
    _options: RuntimeOptions,
) -> Result<HarnessReplayOutput, HarnessReplayError> {
    Err(HarnessReplayError::CliToolFeatureDisabled)
}

// rust-style-allow: long-function because graph replay receipt assembly keeps
// step runs, closure disposition, and parent receipt sealing in one invariant.
fn run_graph_replay_fixture(
    fixture: &HarnessFixture,
    options: RuntimeOptions,
) -> Result<HarnessReplayOutput, HarnessReplayError> {
    let mut runs = Vec::new();
    for replay_step in graph_replay_steps(fixture)? {
        let output = agent_step_output(fixture, &replay_step.request_id)?;
        let disposition = if output.succeeded() {
            ClosureDisposition::Closed
        } else {
            ClosureDisposition::Deferred
        };
        let receipt = step_receipt_with_disposition_and_policy(
            StepReceiptWithDisposition {
                graph_name: &fixture.name,
                step_id: &replay_step.step_id,
                attempt: 1,
                output: &output,
                created_at: &options.created_at,
                disposition: disposition.clone(),
                reason_code: process_reason_code(&disposition),
                summary: if output.succeeded() {
                    format!("agent-step {} replayed", replay_step.task)
                } else {
                    output.stderr.clone()
                },
            },
            options.signature_policy(),
        )?;
        let outputs = skill_output_object(&output);
        let succeeded = output.succeeded();
        let admission_witness =
            StepAdmissionWitness::local_runtime(&replay_step.step_id, receipt.id.as_str());
        runs.push(StepRun {
            step_id: replay_step.step_id,
            attempt: 1,
            skill: replay_step.task.clone(),
            runner: Some(replay_step.task),
            fanout_group: None,
            output,
            outputs,
            receipt,
            admission_witness,
        });
        if !succeeded {
            break;
        }
    }
    if runs.is_empty() {
        return Err(HarnessReplayError::InvalidReplayMetadata {
            field: "metadata.graph_replay_steps".to_owned(),
            message: "at least one replay step is required".to_owned(),
        });
    }
    let disposition = fixture
        .expect
        .status
        .as_ref()
        .map(disposition_from_expected_status)
        .unwrap_or_else(|| {
            if runs.iter().all(|run| run.output.succeeded()) {
                ClosureDisposition::Closed
            } else {
                ClosureDisposition::Deferred
            }
        });
    let receipt = graph_receipt_with_disposition_and_policy(
        &fixture.name,
        &mut runs,
        Vec::new(),
        &options.created_at,
        GraphClosure {
            disposition: disposition.clone(),
            reason_code: named_reason_code(&fixture.name, &disposition),
            summary: format!("graph {} replayed through fixture harness", fixture.name),
        },
        options.signature_policy(),
    )?;
    let step_receipts = runs
        .iter()
        .map(|run| run.receipt.clone())
        .collect::<Vec<_>>();
    let skill_output = runs
        .iter()
        .rev()
        .find(|run| run.output.succeeded())
        .or_else(|| runs.last())
        .map(|run| run.output.clone());
    Ok(HarnessReplayOutput {
        fixture: fixture.clone(),
        status: status_from_disposition(&receipt.seal.disposition),
        receipt,
        step_receipts,
        skill_output,
    })
}

fn graph_replay_steps(
    fixture: &HarnessFixture,
) -> Result<Vec<GraphReplayStep>, HarnessReplayError> {
    let Some(JsonValue::Array(raw_steps)) = fixture.metadata.get("graph_replay_steps") else {
        return Err(HarnessReplayError::InvalidReplayMetadata {
            field: "metadata.graph_replay_steps".to_owned(),
            message: "array is required for fixture replay graphs".to_owned(),
        });
    };
    raw_steps
        .iter()
        .enumerate()
        .map(|(index, raw_step)| {
            let JsonValue::Object(step) = raw_step else {
                return Err(HarnessReplayError::InvalidReplayMetadata {
                    field: format!("metadata.graph_replay_steps.{index}"),
                    message: "object is required".to_owned(),
                });
            };
            let step_id = required_string_metadata(
                step,
                &format!("metadata.graph_replay_steps.{index}.step_id"),
                "step_id",
            )?;
            let task = required_string_metadata(
                step,
                &format!("metadata.graph_replay_steps.{index}.task"),
                "task",
            )?;
            Ok(GraphReplayStep {
                request_id: format!("agent_step.{task}.output"),
                step_id,
                task,
            })
        })
        .collect()
}

#[cfg(feature = "cli-tool")]
fn x402_idempotency_run_env(
    options: &RuntimeOptions,
    fixture: &HarnessFixture,
    payment_state_path: &Path,
    rail_count_path: &Path,
    scenario: &str,
    first_run: bool,
) -> Result<RuntimeOptions, HarnessReplayError> {
    let mut env = options.env.clone();
    env.extend(fixture.env.clone());
    env.insert(
        RUNX_CWD_ENV.to_owned(),
        payment_state_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_string_lossy()
            .into_owned(),
    );
    env.insert(
        RUNX_PAYMENT_STATE_PATH_ENV.to_owned(),
        payment_state_path.to_string_lossy().into_owned(),
    );
    env.insert(
        "RUNX_PAYMENT_RAIL_COUNT_PATH".to_owned(),
        rail_count_path.to_string_lossy().into_owned(),
    );
    env.insert("RUNX_X402_GRAPH_NAME".to_owned(), fixture.name.clone());
    let (idempotency_key, rail_mode) = match (scenario, first_run) {
        ("replay", true | false) => ("payment:paid-echo-001", "sealed"),
        ("capability_reuse", true) => ("payment:paid-echo-001", "sealed"),
        ("capability_reuse", false) => ("payment:paid-echo-002", "sealed"),
        ("crash_recovery", true | false) => ("payment:paid-echo-001", "partial"),
        (other, _) => {
            return Err(HarnessReplayError::InvalidReplayMetadata {
                field: "metadata.x402_idempotency_scenario".to_owned(),
                message: format!("unsupported x402 idempotency scenario {other:?}"),
            });
        }
    };
    env.insert(
        "RUNX_X402_IDEMPOTENCY_KEY".to_owned(),
        idempotency_key.to_owned(),
    );
    env.insert("RUNX_X402_RAIL_MODE".to_owned(), rail_mode.to_owned());
    Ok(RuntimeOptions {
        created_at: options.created_at.clone(),
        env,
        receipt_signature: options.receipt_signature.clone(),
        payment_supervisor: options.payment_supervisor.clone(),
    })
}

#[cfg(feature = "cli-tool")]
fn run_x402_idempotency_graph(
    graph_path: &Path,
    fixture: &HarnessFixture,
    options: RuntimeOptions,
) -> Result<GraphRun, RuntimeError> {
    let runtime = Runtime::new(crate::adapters::cli_tool::CliToolAdapter, options);
    let mut host = FixtureHost::new(fixture);
    runtime.run_graph_file_with_host(graph_path, &mut host)
}

#[cfg(feature = "cli-tool")]
fn sequence_error_output(
    fixture: &HarnessFixture,
    graph_run: GraphRun,
    created_at: &str,
    signature_policy: RuntimeReceiptSignaturePolicy<'_>,
    status: HarnessExpectedStatus,
    closure: GraphClosure,
) -> Result<HarnessReplayOutput, HarnessReplayError> {
    let mut steps = graph_run.steps;
    sequence_output_from_steps(
        fixture,
        &mut steps,
        created_at,
        signature_policy,
        status,
        closure,
    )
}

#[cfg(feature = "cli-tool")]
fn empty_sequence_error_output(
    fixture: &HarnessFixture,
    created_at: &str,
    signature_policy: RuntimeReceiptSignaturePolicy<'_>,
    status: HarnessExpectedStatus,
    closure: GraphClosure,
) -> Result<HarnessReplayOutput, HarnessReplayError> {
    let mut steps = Vec::new();
    sequence_output_from_steps(
        fixture,
        &mut steps,
        created_at,
        signature_policy,
        status,
        closure,
    )
}

#[cfg(feature = "cli-tool")]
fn sequence_output_from_steps(
    fixture: &HarnessFixture,
    steps: &mut [StepRun],
    created_at: &str,
    signature_policy: RuntimeReceiptSignaturePolicy<'_>,
    status: HarnessExpectedStatus,
    closure: GraphClosure,
) -> Result<HarnessReplayOutput, HarnessReplayError> {
    let receipt = graph_receipt_with_disposition_and_policy(
        &fixture.name,
        steps,
        Vec::new(),
        created_at,
        closure,
        signature_policy,
    )?;
    let step_receipts = steps
        .iter()
        .map(|run| run.receipt.clone())
        .collect::<Vec<_>>();
    Ok(HarnessReplayOutput {
        fixture: fixture.clone(),
        status,
        receipt,
        step_receipts,
        skill_output: None,
    })
}

#[cfg(feature = "cli-tool")]
fn step_receipt_digest(graph_run: &GraphRun, step_id: &str) -> Result<String, HarnessReplayError> {
    graph_run
        .steps
        .iter()
        .find(|run| run.step_id == step_id)
        .map(|run| run.receipt.digest.to_string())
        .ok_or_else(|| HarnessReplayError::Mismatch {
            field: "metadata.x402_idempotency_replay.step",
            expected: step_id.to_owned(),
            actual: graph_run
                .steps
                .iter()
                .map(|run| run.step_id.as_str())
                .collect::<Vec<_>>()
                .join(","),
        })
}

#[cfg(feature = "cli-tool")]
fn assert_authority_denied_contains(
    error: &RuntimeError,
    expected: &str,
) -> Result<(), HarnessReplayError> {
    match error {
        RuntimeError::AuthorityDenied { reason, .. } if reason.contains(expected) => Ok(()),
        other => Err(HarnessReplayError::Mismatch {
            field: "metadata.x402_idempotency_authority_denial",
            expected: expected.to_owned(),
            actual: other.to_string(),
        }),
    }
}

#[cfg(feature = "cli-tool")]
fn assert_skill_failed_contains(
    error: &RuntimeError,
    expected: &str,
) -> Result<(), HarnessReplayError> {
    match error {
        RuntimeError::SkillFailed { message, .. } if message.contains(expected) => Ok(()),
        other => Err(HarnessReplayError::Mismatch {
            field: "metadata.x402_idempotency_skill_failure",
            expected: expected.to_owned(),
            actual: other.to_string(),
        }),
    }
}

#[cfg(feature = "cli-tool")]
fn assert_x402_rail_invocation_count(path: &Path, expected: u64) -> Result<(), HarnessReplayError> {
    let actual = fs::read_to_string(path)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(0);
    if actual == expected {
        return Ok(());
    }
    Err(HarnessReplayError::Mismatch {
        field: "metadata.x402_idempotency_rail_invocation_count",
        expected: expected.to_string(),
        actual: actual.to_string(),
    })
}

#[cfg(feature = "cli-tool")]
fn assert_x402_replay_state(path: &Path, key: &str) -> Result<(), HarnessReplayError> {
    let store = FileBackedPaymentStateStore::open(path).map_err(|source| {
        RuntimeError::payment_state("reading x402 replay fixture state", source)
    })?;
    let key = x402_paid_echo_key(key);
    let entry = store
        .lookup_idempotency(&key)
        .ok_or_else(|| HarnessReplayError::Mismatch {
            field: "metadata.x402_idempotency_replay_state",
            expected: "sealed idempotency entry".to_owned(),
            actual: "none".to_owned(),
        })?;
    let entry_text = serde_json::to_string(entry)
        .map_err(|source| RuntimeError::json("serializing x402 replay fixture state", source))?;
    if entry_text.contains("rail_session_material_ref") {
        return Err(HarnessReplayError::Mismatch {
            field: "metadata.x402_idempotency_replay_state",
            expected: "no rail session material".to_owned(),
            actual: "rail_session_material_ref persisted".to_owned(),
        });
    }
    Ok(())
}

#[cfg(feature = "cli-tool")]
fn assert_x402_escalated_state(path: &Path, key: &str) -> Result<(), HarnessReplayError> {
    let store = FileBackedPaymentStateStore::open(path).map_err(|source| {
        RuntimeError::payment_state("reading x402 recovery fixture state", source)
    })?;
    let key = x402_paid_echo_key(key);
    let mutation =
        store
            .lookup_rail_mutation(&key)
            .ok_or_else(|| HarnessReplayError::Mismatch {
                field: "metadata.x402_idempotency_recovery_state",
                expected: "rail mutation".to_owned(),
                actual: "none".to_owned(),
            })?;
    if mutation.status == RailMutationStatus::Escalated
        && mutation.recovery_state == PaymentRecoveryState::Escalated
    {
        return Ok(());
    }
    Err(HarnessReplayError::Mismatch {
        field: "metadata.x402_idempotency_recovery_state",
        expected: "escalated".to_owned(),
        actual: format!("{:?}/{:?}", mutation.status, mutation.recovery_state),
    })
}

#[cfg(feature = "cli-tool")]
fn x402_paid_echo_key(key: &str) -> PaymentIdempotencyKey {
    PaymentIdempotencyKey::new("mock", "merchant:paid-echo", key)
}

#[cfg(feature = "cli-tool")]
fn create_harness_temp_dir(name: &str) -> Result<PathBuf, HarnessReplayError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let path = std::env::temp_dir().join(format!(
        "runx-harness-{name}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&path)
        .map_err(|source| RuntimeError::io("creating x402 idempotency fixture temp dir", source))?;
    Ok(path)
}

fn agent_step_output(
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
            status: crate::InvocationStatus::Failure,
            stdout: String::new(),
            stderr: format!("missing replay answer for {request_id}"),
            exit_code: None,
            duration_ms: 0,
            metadata,
        });
    }
    Ok(SkillOutput {
        status: crate::InvocationStatus::Success,
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

fn skill_output_object(output: &SkillOutput) -> JsonObject {
    let mut object = JsonObject::new();
    if let Ok(parsed) = serde_json::from_str::<JsonValue>(&output.stdout) {
        object.insert("skill_claim".to_owned(), parsed);
    }
    object
}

fn string_metadata<'a>(fixture: &'a HarnessFixture, field: &str) -> Option<&'a str> {
    match fixture.metadata.get(field) {
        Some(JsonValue::String(value)) => Some(value),
        _ => None,
    }
}

fn required_string_metadata(
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

fn agent_answer_disposition(answer: &JsonValue) -> ClosureDisposition {
    match answer
        .as_object()
        .and_then(|object| object.get("closure"))
        .and_then(JsonValue::as_object)
        .and_then(|closure| closure.get("disposition"))
        .and_then(JsonValue::as_str)
    {
        Some("deferred") => ClosureDisposition::Deferred,
        Some("superseded") => ClosureDisposition::Superseded,
        Some("declined") => ClosureDisposition::Declined,
        Some("blocked") => ClosureDisposition::Blocked,
        Some("failed") => ClosureDisposition::Failed,
        Some("killed") => ClosureDisposition::Killed,
        Some("timed_out") => ClosureDisposition::TimedOut,
        _ => ClosureDisposition::Closed,
    }
}

fn disposition_from_expected_status(status: &HarnessExpectedStatus) -> ClosureDisposition {
    match status {
        HarnessExpectedStatus::Sealed => ClosureDisposition::Closed,
        HarnessExpectedStatus::Failure => ClosureDisposition::Failed,
        HarnessExpectedStatus::NeedsAgent => ClosureDisposition::Deferred,
        HarnessExpectedStatus::PolicyDenied => ClosureDisposition::Blocked,
        HarnessExpectedStatus::Escalated => ClosureDisposition::Deferred,
    }
}

fn process_reason_code(disposition: &ClosureDisposition) -> String {
    format!("process_{}", disposition_suffix(disposition))
}

fn named_reason_code(name: &str, disposition: &ClosureDisposition) -> String {
    format!("{name}_{}", disposition_suffix(disposition))
}

fn disposition_suffix(disposition: &ClosureDisposition) -> &'static str {
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

#[cfg(all(test, feature = "cli-tool"))]
mod tests {
    use std::collections::BTreeMap;

    use runx_contracts::ReceiptIssuerType;

    use super::super::fixtures::HarnessExpectation;
    use super::*;
    use crate::receipts::{
        Ed25519ReceiptSigner, Ed25519ReceiptVerifier, RuntimeReceiptSigningError,
    };

    const CREATED_AT: &str = "2026-05-25T00:00:00Z";
    const FIXTURE_KID: &str = "runx-harness-sequence-prod-key";
    const FIXTURE_SEED: [u8; 32] = [0x42; 32];

    #[test]
    // rust-style-allow: long-function because the test builds a minimal signed
    // harness sequence so the sequence seal policy is checked without shell fixtures.
    fn sequence_output_uses_supplied_production_signature_policy() -> Result<(), HarnessReplayError>
    {
        let signer =
            Ed25519ReceiptSigner::from_seed(FIXTURE_KID, ReceiptIssuerType::Hosted, &FIXTURE_SEED)
                .map_err(signing_error)?;
        let verifier = Ed25519ReceiptVerifier::new([signer.production_key()]);
        let signature_policy =
            RuntimeReceiptSignaturePolicy::production_signing(&signer, &verifier);
        let output = SkillOutput {
            status: InvocationStatus::Success,
            stdout: r#"{"ok":true}"#.to_owned(),
            stderr: String::new(),
            exit_code: Some(0),
            duration_ms: 1,
            metadata: JsonObject::new(),
        };
        let receipt = step_receipt_with_disposition_and_policy(
            StepReceiptWithDisposition {
                graph_name: "x402_sequence",
                step_id: "fulfill",
                attempt: 1,
                output: &output,
                created_at: CREATED_AT,
                disposition: ClosureDisposition::Closed,
                reason_code: "process_closed".to_owned(),
                summary: "fulfilled".to_owned(),
            },
            signature_policy,
        )?;
        let receipt_id = receipt.id.to_string();
        let mut steps = vec![StepRun {
            step_id: "fulfill".to_owned(),
            attempt: 1,
            skill: "fulfill".to_owned(),
            runner: None,
            fanout_group: None,
            output,
            outputs: JsonObject::new(),
            receipt,
            admission_witness: StepAdmissionWitness::local_runtime("fulfill", &receipt_id),
        }];
        let fixture = HarnessFixture {
            name: "x402-sequence".to_owned(),
            kind: HarnessFixtureKind::Graph,
            target: String::new(),
            runner: None,
            inputs: JsonObject::new(),
            env: BTreeMap::new(),
            caller: JsonObject::new(),
            expect: HarnessExpectation::default(),
            metadata: JsonObject::new(),
        };

        let output = sequence_output_from_steps(
            &fixture,
            &mut steps,
            CREATED_AT,
            signature_policy,
            HarnessExpectedStatus::PolicyDenied,
            GraphClosure {
                disposition: ClosureDisposition::Blocked,
                reason_code: "x402_idempotency_capability_reuse_blocked".to_owned(),
                summary: "second spend capability use denied before rail".to_owned(),
            },
        )?;

        assert!(output.receipt.signature.value.starts_with("base64:"));
        assert!(!output.receipt.signature.value.starts_with("sig:"));
        assert!(
            output
                .step_receipts
                .iter()
                .all(|receipt| receipt.signature.value.starts_with("base64:"))
        );
        Ok(())
    }

    fn signing_error(error: RuntimeReceiptSigningError) -> HarnessReplayError {
        HarnessReplayError::Runtime(RuntimeError::ReceiptInvalid {
            message: error.to_string(),
        })
    }
}

fn run_skill_fixture<A>(
    fixture: &HarnessFixture,
    skill_dir: PathBuf,
    adapter: A,
    options: RuntimeOptions,
) -> Result<HarnessReplayOutput, HarnessReplayError>
where
    A: SkillAdapter,
{
    let (skill_name, invocation) = skill_fixture_invocation(fixture, skill_dir, &options)?;
    let (skill_output, disposition, reason_code, summary) =
        run_skill_invocation(fixture, invocation, adapter)?;
    let receipt = step_receipt_with_disposition_and_policy(
        StepReceiptWithDisposition {
            graph_name: &fixture.name,
            step_id: &skill_name,
            attempt: 1,
            output: &skill_output,
            created_at: &options.created_at,
            disposition: disposition.clone(),
            reason_code,
            summary,
        },
        options.signature_policy(),
    )?;
    Ok(HarnessReplayOutput {
        fixture: fixture.clone(),
        status: status_from_disposition(&receipt.seal.disposition),
        receipt,
        step_receipts: Vec::new(),
        skill_output: Some(skill_output),
    })
}

fn skill_fixture_invocation(
    fixture: &HarnessFixture,
    skill_dir: PathBuf,
    options: &RuntimeOptions,
) -> Result<(String, SkillInvocation), HarnessReplayError> {
    let skill = load_skill(&skill_dir)?;
    let runner = load_harness_runner(&skill_dir, fixture.runner.as_deref())?;
    let mut env = options.env.clone();
    env.extend(fixture.env.clone());
    let skill_name = if fixture.runner.is_some() {
        runner
            .as_ref()
            .map_or_else(|| skill.name.clone(), |runner| runner.name.clone())
    } else {
        skill.name.clone()
    };
    let source = runner
        .as_ref()
        .map_or_else(|| skill.source.clone(), |runner| runner.source.clone());
    let invocation = SkillInvocation {
        skill_name: skill_name.clone(),
        source,
        inputs: fixture.inputs.clone(),
        resolved_inputs: JsonObject::new(),
        skill_directory: skill_dir,
        env,
        credential_delivery: crate::credentials::CredentialDelivery::none(),
    };
    Ok((skill_name, invocation))
}

fn run_skill_invocation<A>(
    fixture: &HarnessFixture,
    invocation: SkillInvocation,
    adapter: A,
) -> Result<(SkillOutput, ClosureDisposition, String, String), HarnessReplayError>
where
    A: SkillAdapter,
{
    let skill_name = invocation.skill_name.clone();
    let (skill_output, disposition, reason_code, summary) =
        match invocation.source.source_type.as_str() {
            "agent" | "agent-step" => replay_agent_skill_fixture(fixture, &invocation)?,
            _ => {
                let output = adapter.invoke(invocation)?;
                let disposition = if output.succeeded() {
                    ClosureDisposition::Closed
                } else {
                    ClosureDisposition::Failed
                };
                let reason_code = process_reason_code(&disposition);
                let summary = format!("step {skill_name} completed");
                (output, disposition, reason_code, summary)
            }
        };
    Ok((skill_output, disposition, reason_code, summary))
}

fn load_harness_runner(
    skill_dir: &Path,
    requested_runner: Option<&str>,
) -> Result<Option<SkillRunnerDefinition>, HarnessReplayError> {
    let manifest_path = skill_dir.join("X.yaml");
    if !manifest_path.exists() {
        if let Some(runner) = requested_runner {
            return Err(RuntimeError::UnsupportedRunnerSelection {
                runner: runner.to_owned(),
            }
            .into());
        }
        return Ok(None);
    }
    let source = fs::read_to_string(&manifest_path).map_err(|source| {
        RuntimeError::io(format!("reading {}", manifest_path.display()), source)
    })?;
    let parsed = parse_runner_manifest_yaml(&source).map_err(RuntimeError::from)?;
    let manifest = validate_runner_manifest(parsed).map_err(RuntimeError::from)?;
    select_harness_runner(&manifest, requested_runner)
        .cloned()
        .map(Some)
}

fn select_harness_runner<'a>(
    manifest: &'a SkillRunnerManifest,
    requested_runner: Option<&str>,
) -> Result<&'a SkillRunnerDefinition, HarnessReplayError> {
    if let Some(runner) = requested_runner {
        return manifest.runners.get(runner).ok_or_else(|| {
            RuntimeError::UnsupportedRunnerSelection {
                runner: runner.to_owned(),
            }
            .into()
        });
    }
    let defaults = manifest
        .runners
        .values()
        .filter(|runner| runner.default)
        .collect::<Vec<_>>();
    match defaults.as_slice() {
        [runner] => Ok(*runner),
        [] if manifest.runners.len() == 1 => manifest.runners.values().next().ok_or_else(|| {
            RuntimeError::UnsupportedRunnerSelection {
                runner: "default".to_owned(),
            }
            .into()
        }),
        [] => Err(RuntimeError::UnsupportedRunnerSelection {
            runner: "default".to_owned(),
        }
        .into()),
        _ => Err(RuntimeError::UnsupportedRunnerSelection {
            runner: "default".to_owned(),
        }
        .into()),
    }
}

fn replay_agent_skill_fixture(
    fixture: &HarnessFixture,
    invocation: &SkillInvocation,
) -> Result<(SkillOutput, ClosureDisposition, String, String), HarnessReplayError> {
    let source_type =
        AgentActInvocationSourceType::from_contract_value(invocation.source.source_type.as_str())
            .ok_or_else(|| RuntimeError::UnsupportedAdapter {
            adapter_type: invocation.source.source_type.as_str().to_owned(),
        })?;
    let request_id = agent_act_invocation_id(invocation, source_type);
    let mut metadata = JsonObject::new();
    metadata.insert(
        "agent_request_id".to_owned(),
        JsonValue::String(request_id.clone()),
    );
    let Some(answer) = fixture_answer(fixture, "answers", &request_id, &request_id) else {
        return Ok((
            SkillOutput {
                status: InvocationStatus::Failure,
                stdout: String::new(),
                stderr: format!("missing replay answer for {request_id}"),
                exit_code: None,
                duration_ms: 0,
                metadata,
            },
            ClosureDisposition::Deferred,
            "agent_act_deferred".to_owned(),
            format!("agent act {request_id} is awaiting replay answer"),
        ));
    };
    let stdout = serde_json::to_string(answer).map_err(|source| RuntimeError::Json {
        context: format!("serializing replay answer {request_id}"),
        source,
    })?;
    let disposition = agent_answer_disposition(answer);
    let succeeded = disposition == ClosureDisposition::Closed;
    Ok((
        SkillOutput {
            status: if succeeded {
                InvocationStatus::Success
            } else {
                InvocationStatus::Failure
            },
            stdout,
            stderr: if succeeded {
                String::new()
            } else {
                format!("agent act closed with {}", disposition_suffix(&disposition))
            },
            exit_code: succeeded.then_some(0),
            duration_ms: 0,
            metadata,
        },
        disposition.clone(),
        format!("agent_act_{}", disposition_suffix(&disposition)),
        format!("agent act closed with {}", disposition_suffix(&disposition)),
    ))
}

fn run_graph_fixture<A>(
    fixture: &HarnessFixture,
    graph_path: &Path,
    adapter: A,
    mut options: RuntimeOptions,
) -> Result<HarnessReplayOutput, HarnessReplayError>
where
    A: SkillAdapter,
{
    options.env.extend(fixture.env.clone());
    let runtime = Runtime::new(adapter, options);
    let mut host = FixtureHost::new(fixture);
    let graph_run = runtime.run_graph_file_for_harness(graph_path, &mut host)?;
    persist_payment_ledger_projection_if_configured(fixture, &graph_run, &runtime)?;
    let output = replay_output_from_graph(fixture, graph_run);
    Ok(output)
}

fn persist_payment_ledger_projection_if_configured<A>(
    fixture: &HarnessFixture,
    graph_run: &GraphRun,
    runtime: &Runtime<A>,
) -> Result<(), HarnessReplayError>
where
    A: SkillAdapter,
{
    if !runtime.options().env.contains_key(RUNX_RECEIPT_DIR_ENV) {
        return Ok(());
    }
    if string_metadata(fixture, "payment_ledger_profile") != Some(X402_PAY_PAYMENT_PROFILE) {
        return Ok(());
    }
    let cwd = std::env::current_dir().map_err(|source| {
        RuntimeError::io("resolving cwd for payment ledger projection", source)
    })?;
    let workspace = WorkspaceEnv::new(runtime.options().env.clone(), cwd);
    let receipts =
        ReceiptServices::from_signature_config(runtime.options().receipt_signature.clone());
    let receipt_path = receipts.resolve_path(&workspace, None, None);
    let scenario_id = required_string_metadata(
        &fixture.metadata,
        "metadata.payment_ledger_scenario_id",
        "payment_ledger_scenario_id",
    )?;
    persist_x402_payment_ledger_projection_event(
        &receipt_path.path,
        &format!("gx_{}", graph_run.graph.name),
        &runtime.options().created_at,
        &graph_run.receipt,
        &graph_run.steps,
        scenario_id.as_str(),
    )
    .map(|_| ())
    .map_err(|source| {
        RuntimeError::ReceiptInvalid {
            message: source.to_string(),
        }
        .into()
    })
}

struct FixtureHost<'a> {
    fixture: &'a HarnessFixture,
}

impl<'a> FixtureHost<'a> {
    fn new(fixture: &'a HarnessFixture) -> Self {
        Self { fixture }
    }
}

impl Host for FixtureHost<'_> {
    fn report(&mut self, _event: ExecutionEvent) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn resolve(
        &mut self,
        request: ResolutionRequest,
    ) -> Result<Option<ResolutionResponse>, RuntimeError> {
        match request {
            ResolutionRequest::Approval { id, gate } => {
                fixture_approval_response(self.fixture, &id, &gate.id)
            }
            ResolutionRequest::Input { .. } | ResolutionRequest::AgentAct { .. } => Ok(None),
        }
    }
}

fn fixture_approval_response(
    fixture: &HarnessFixture,
    request_id: &str,
    gate_id: &str,
) -> Result<Option<ResolutionResponse>, RuntimeError> {
    let Some(answer) = fixture_answer(fixture, "approvals", gate_id, request_id)
        .or_else(|| fixture_answer(fixture, "answers", request_id, gate_id))
    else {
        return Ok(None);
    };
    let approved = fixture_bool_answer(answer, request_id, gate_id)?;
    Ok(Some(ResolutionResponse {
        actor: fixture_answer_actor(answer, request_id, gate_id)?,
        payload: JsonValue::Bool(approved),
    }))
}

fn fixture_answer<'a>(
    fixture: &'a HarnessFixture,
    group: &str,
    primary_key: &str,
    secondary_key: &str,
) -> Option<&'a JsonValue> {
    fixture
        .caller
        .get(group)
        .and_then(JsonValue::as_object)
        .and_then(|answers| {
            answers
                .get(primary_key)
                .or_else(|| answers.get(secondary_key))
        })
}

fn fixture_bool_answer(
    answer: &JsonValue,
    request_id: &str,
    gate_id: &str,
) -> Result<bool, RuntimeError> {
    match answer {
        JsonValue::Bool(value) => Ok(*value),
        JsonValue::Object(object) => match object.get("approved").or_else(|| object.get("payload"))
        {
            Some(JsonValue::Bool(value)) => Ok(*value),
            Some(_) | None => Err(invalid_fixture_answer(request_id, gate_id)),
        },
        JsonValue::Null | JsonValue::Number(_) | JsonValue::String(_) | JsonValue::Array(_) => {
            Err(invalid_fixture_answer(request_id, gate_id))
        }
    }
}

fn fixture_answer_actor(
    answer: &JsonValue,
    request_id: &str,
    gate_id: &str,
) -> Result<ResolutionResponseActor, RuntimeError> {
    let Some(actor) = answer.as_object().and_then(|object| object.get("actor")) else {
        return Ok(ResolutionResponseActor::Human);
    };
    match actor {
        JsonValue::String(value) if value == "human" => Ok(ResolutionResponseActor::Human),
        JsonValue::String(value) if value == "agent" => Ok(ResolutionResponseActor::Agent),
        _ => Err(RuntimeError::ReceiptInvalid {
            message: format!(
                "harness fixture approval answer for request {request_id} gate {gate_id} has invalid actor"
            ),
        }),
    }
}

fn invalid_fixture_answer(request_id: &str, gate_id: &str) -> RuntimeError {
    RuntimeError::ReceiptInvalid {
        message: format!(
            "harness fixture approval answer for request {request_id} gate {gate_id} must be a boolean or object with a boolean approved field"
        ),
    }
}

fn replay_output_from_graph(fixture: &HarnessFixture, graph_run: GraphRun) -> HarnessReplayOutput {
    let step_receipts = graph_run
        .steps
        .iter()
        .map(|step| step.receipt.clone())
        .collect::<Vec<_>>();
    HarnessReplayOutput {
        fixture: fixture.clone(),
        status: status_from_disposition(&graph_run.receipt.seal.disposition),
        receipt: graph_run.receipt,
        step_receipts,
        skill_output: None,
    }
}

fn resolve_target_path(fixture_path: &Path, target: &str) -> Result<PathBuf, HarnessReplayError> {
    let Some(parent) = fixture_path.parent() else {
        return Err(HarnessReplayError::TargetWithoutParent {
            target: fixture_path.to_path_buf(),
        });
    };
    Ok(parent.join(target))
}
