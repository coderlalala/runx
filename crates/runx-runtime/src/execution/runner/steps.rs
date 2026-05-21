// rust-style-allow: large-file because graph step execution currently keeps
// authority admission, native step execution, approval handling, and payment
// state persistence in one runtime boundary until the runner module split.
use std::path::Path;

use runx_contracts::{ApprovalGate, JsonNumber, JsonObject, JsonValue, ResolutionResponseActor};
use runx_parser::GraphStep;

use super::super::graph::{load_skill, output_object, resolve_inputs, skill_dir};
use super::authority::{
    enforce_step_authority_admission, enforce_step_authority_receipt_before_success,
};
use super::inputs::{optional_input_string, required_input_string, string_value, string_value_ref};
use super::{Runtime, StepRun};
use crate::RuntimeError;
use crate::adapter::{InvocationStatus, SkillAdapter, SkillInvocation, SkillOutput};
use crate::approval::{ApprovalResolution, request_approval};
use crate::host::Host;
use crate::payment_state::{
    FileBackedPaymentStateStore, MockRailMutation, MockRailMutationStatus, PaymentIdempotencyEntry,
    PaymentRecoveryState, SpendCapabilityConsumption, resolve_payment_state_path,
};
use crate::receipts::step_receipt;

pub(super) fn output_error(run: &StepRun) -> String {
    if run.output.stderr.is_empty() {
        "cli-tool failed without stderr".to_owned()
    } else {
        run.output.stderr.clone()
    }
}

pub(super) fn run_step<A>(
    runtime: &Runtime<A>,
    graph_dir: &Path,
    graph_name: &str,
    step: &GraphStep,
    attempt: u32,
    prior_runs: &[StepRun],
    host: &mut dyn Host,
) -> Result<StepRun, RuntimeError>
where
    A: SkillAdapter,
{
    let inputs = resolve_inputs(step, prior_runs)?;
    let authority =
        enforce_step_authority_admission(step, &inputs, &runtime.options.env, graph_dir)?;
    if step.run.is_some() {
        return run_native_step(runtime, graph_name, step, attempt, inputs, host);
    }

    let skill_dir = skill_dir(graph_dir, step)?;
    let skill = load_skill(&skill_dir)?;
    let skill_name = skill.name.clone();
    let output = runtime.adapter.invoke(SkillInvocation {
        skill_name: skill.name,
        source: skill.source,
        inputs,
        resolved_inputs: JsonObject::new(),
        skill_directory: skill_dir,
        env: runtime.options.env.clone(),
        credential_delivery: crate::credentials::CredentialDelivery::none(),
    })?;
    let outputs = output_object(&output);
    let receipt = step_receipt(
        graph_name,
        &step.id,
        attempt,
        &output,
        &runtime.options.created_at,
    )?;
    enforce_step_authority_receipt_before_success(step, authority.as_ref(), &output, &receipt)?;
    persist_payment_state_for_step(runtime, graph_dir, authority.as_ref(), &outputs, &receipt)?;
    Ok(StepRun {
        step_id: step.id.clone(),
        attempt,
        skill: skill_name,
        runner: step.runner.clone(),
        fanout_group: step.fanout_group.clone(),
        output,
        outputs,
        receipt,
    })
}

// rust-style-allow: long-function because payment state persistence binds
// authority, output, receipt, and recovery-state invariants in one transaction.
fn persist_payment_state_for_step<A>(
    runtime: &Runtime<A>,
    graph_dir: &Path,
    authority: Option<&super::authority::StepAuthorityContext>,
    outputs: &JsonObject,
    receipt: &runx_contracts::HarnessReceipt,
) -> Result<(), RuntimeError>
where
    A: SkillAdapter,
{
    let Some(payment) = authority.and_then(|authority| authority.payment.as_ref()) else {
        return Ok(());
    };
    let Some(path) = resolve_payment_state_path(&runtime.options.env, graph_dir) else {
        return Ok(());
    };
    let mut store = FileBackedPaymentStateStore::open(&path).map_err(|source| {
        RuntimeError::payment_state("opening payment state for payment step persistence", source)
    })?;
    let recovery_state = payment_recovery_state(outputs);
    let rail_touched = nested_string(
        outputs,
        &["payment_rail_packet", "data", "rail_result", "status"],
    )
    .is_some();
    if rail_touched
        && store
            .lookup_consumed_spend_capability(&payment.spend_capability_ref.uri)
            .is_none()
    {
        store
            .consume_spend_capability(SpendCapabilityConsumption {
                capability_ref: payment.spend_capability_ref.uri.clone(),
                idempotency_key: payment.idempotency_key.clone(),
                receipt_ref: Some(receipt.id.clone()),
                recovery_state: Some(recovery_state.clone()),
            })
            .map_err(|source| {
                RuntimeError::payment_state("recording consumed payment spend capability", source)
            })?;
    }

    if let Some(proof_ref) = nested_string(
        outputs,
        &["payment_rail_packet", "data", "rail_proof", "proof_ref"],
    ) && store.lookup_idempotency(&payment.idempotency_key).is_none()
    {
        store
            .record_idempotency(PaymentIdempotencyEntry {
                idempotency_key: payment.idempotency_key.clone(),
                receipt_ref: receipt.id.clone(),
                rail_proof_ref: proof_ref.to_owned(),
                amount_minor: nested_u64(
                    outputs,
                    &["payment_rail_packet", "data", "rail_result", "amount_minor"],
                )
                .unwrap_or(payment.amount_minor),
                currency: nested_string(
                    outputs,
                    &["payment_rail_packet", "data", "rail_result", "currency"],
                )
                .unwrap_or(&payment.currency)
                .to_owned(),
            })
            .map_err(|source| {
                RuntimeError::payment_state("recording payment idempotency entry", source)
            })?;
    }

    if rail_touched
        && store
            .lookup_mock_rail_mutation(&payment.idempotency_key)
            .is_none()
    {
        store
            .record_mock_rail_mutation(MockRailMutation {
                idempotency_key: payment.idempotency_key.clone(),
                rail: nested_string(
                    outputs,
                    &["payment_rail_packet", "data", "rail_result", "rail"],
                )
                .unwrap_or(&payment.rail)
                .to_owned(),
                amount_minor: nested_u64(
                    outputs,
                    &["payment_rail_packet", "data", "rail_result", "amount_minor"],
                )
                .unwrap_or(payment.amount_minor),
                currency: nested_string(
                    outputs,
                    &["payment_rail_packet", "data", "rail_result", "currency"],
                )
                .unwrap_or(&payment.currency)
                .to_owned(),
                counterparty: nested_string(
                    outputs,
                    &["payment_rail_packet", "data", "rail_result", "counterparty"],
                )
                .unwrap_or(&payment.counterparty)
                .to_owned(),
                status: mock_rail_mutation_status(&recovery_state),
                proof_ref: nested_string(
                    outputs,
                    &["payment_rail_packet", "data", "rail_proof", "proof_ref"],
                )
                .map(str::to_owned),
                recovery_state,
            })
            .map_err(|source| {
                RuntimeError::payment_state("recording mock payment rail mutation", source)
            })?;
    }

    Ok(())
}

fn payment_recovery_state(outputs: &JsonObject) -> PaymentRecoveryState {
    match nested_string(
        outputs,
        &["payment_rail_packet", "data", "recovery_hint", "status"],
    ) {
        Some("sealed") => PaymentRecoveryState::Sealed,
        Some("terminal_decline" | "escalated") => PaymentRecoveryState::Escalated,
        Some("recoverable_timeout" | "partial" | "in_flight") => PaymentRecoveryState::InFlight,
        _ if nested_string(
            outputs,
            &["payment_rail_packet", "data", "rail_proof", "proof_ref"],
        )
        .is_some() =>
        {
            PaymentRecoveryState::Sealed
        }
        _ => PaymentRecoveryState::InFlight,
    }
}

fn mock_rail_mutation_status(recovery_state: &PaymentRecoveryState) -> MockRailMutationStatus {
    match recovery_state {
        PaymentRecoveryState::Sealed => MockRailMutationStatus::Fulfilled,
        PaymentRecoveryState::Escalated => MockRailMutationStatus::Escalated,
        PaymentRecoveryState::InFlight => MockRailMutationStatus::Partial,
    }
}

fn nested_string<'a>(object: &'a JsonObject, path: &[&str]) -> Option<&'a str> {
    let mut value = object.get(path.first().copied()?)?;
    for segment in &path[1..] {
        let JsonValue::Object(object) = value else {
            return None;
        };
        value = object.get(*segment)?;
    }
    match value {
        JsonValue::String(value) => Some(value.as_str()),
        _ => None,
    }
}

fn nested_u64(object: &JsonObject, path: &[&str]) -> Option<u64> {
    let mut value = object.get(path.first().copied()?)?;
    for segment in &path[1..] {
        let JsonValue::Object(object) = value else {
            return None;
        };
        value = object.get(*segment)?;
    }
    match value {
        JsonValue::Number(JsonNumber::U64(value)) => Some(*value),
        JsonValue::Number(JsonNumber::I64(value)) => u64::try_from(*value).ok(),
        JsonValue::Number(JsonNumber::F64(value))
            if value.is_finite() && value.fract() == 0.0 && *value >= 0.0 =>
        {
            Some(*value as u64)
        }
        _ => None,
    }
}

pub(super) fn run_native_step<A>(
    runtime: &Runtime<A>,
    graph_name: &str,
    step: &GraphStep,
    attempt: u32,
    inputs: JsonObject,
    host: &mut dyn Host,
) -> Result<StepRun, RuntimeError>
where
    A: SkillAdapter,
{
    let run_type = run_type(step)?;
    match run_type.as_str() {
        "approval" => run_approval_step(runtime, graph_name, step, attempt, inputs, host),
        other => Err(RuntimeError::UnsupportedRunStep {
            step_id: step.id.clone(),
            run_type: other.to_owned(),
        }),
    }
}

pub(super) fn run_approval_step<A>(
    runtime: &Runtime<A>,
    graph_name: &str,
    step: &GraphStep,
    attempt: u32,
    inputs: JsonObject,
    host: &mut dyn Host,
) -> Result<StepRun, RuntimeError>
where
    A: SkillAdapter,
{
    let gate = approval_gate(step, &inputs)?;
    let request_id = format!("{}_approval", step.id);
    let resolution = request_approval(host, request_id, gate.clone()).map_err(|source| {
        RuntimeError::InvalidRunStep {
            step_id: step.id.clone(),
            reason: source.to_string(),
        }
    })?;
    let outputs = approval_outputs(step, &gate, &resolution)?;
    let stdout = serde_json::to_string(&outputs)
        .map_err(|source| RuntimeError::json("serializing approval run output", source))?;
    let output = SkillOutput {
        status: InvocationStatus::Success,
        stdout,
        stderr: String::new(),
        exit_code: Some(0),
        duration_ms: 0,
        metadata: JsonObject::new(),
    };
    let receipt = step_receipt(
        graph_name,
        &step.id,
        attempt,
        &output,
        &runtime.options.created_at,
    )?;
    Ok(StepRun {
        step_id: step.id.clone(),
        attempt,
        skill: "run:approval".to_owned(),
        runner: step.runner.clone(),
        fanout_group: step.fanout_group.clone(),
        output,
        outputs,
        receipt,
    })
}

pub(super) fn run_type(step: &GraphStep) -> Result<String, RuntimeError> {
    let Some(run) = &step.run else {
        return Err(RuntimeError::InvalidRunStep {
            step_id: step.id.clone(),
            reason: "missing run configuration".to_owned(),
        });
    };
    let Some(value) = run.get("type") else {
        return Err(RuntimeError::InvalidRunStep {
            step_id: step.id.clone(),
            reason: "run.type is required".to_owned(),
        });
    };
    string_value(step, "run.type", value)
}

pub(super) fn approval_gate(
    step: &GraphStep,
    inputs: &JsonObject,
) -> Result<ApprovalGate, RuntimeError> {
    let gate_id = required_input_string(step, inputs, "gate_id")?;
    let reason = required_input_string(step, inputs, "reason")?;
    let gate_type = optional_input_string(step, inputs, "gate_type")?;
    let summary = approval_summary(inputs);
    Ok(ApprovalGate {
        id: gate_id,
        reason,
        gate_type,
        summary,
    })
}

pub(super) fn approval_summary(inputs: &JsonObject) -> Option<JsonObject> {
    let mut summary = JsonObject::new();
    for (key, value) in inputs {
        if matches!(key.as_str(), "gate_id" | "reason" | "gate_type") {
            continue;
        }
        summary.insert(key.clone(), value.clone());
    }
    (!summary.is_empty()).then_some(summary)
}

pub(super) fn approval_outputs(
    step: &GraphStep,
    gate: &ApprovalGate,
    resolution: &ApprovalResolution,
) -> Result<JsonObject, RuntimeError> {
    let mut data = JsonObject::new();
    data.insert("approved".to_owned(), approved_value(resolution));
    data.insert("gate_id".to_owned(), JsonValue::String(gate.id.clone()));
    data.insert(
        "idempotency_key".to_owned(),
        JsonValue::String(resolution.idempotency_key().to_owned()),
    );
    data.insert(
        "status".to_owned(),
        JsonValue::String(approval_status(resolution).to_owned()),
    );
    if let Some(actor) = resolution.actor() {
        data.insert("actor".to_owned(), JsonValue::String(actor_name(actor)));
    }

    let mut packet = JsonObject::new();
    if let Some(packet_id) = artifact_packet(step)? {
        packet.insert("packet".to_owned(), JsonValue::String(packet_id));
    }
    packet.insert("data".to_owned(), JsonValue::Object(data));

    let mut outputs = JsonObject::new();
    outputs.insert(
        artifact_wrap_as(step)?.to_owned(),
        JsonValue::Object(packet),
    );
    Ok(outputs)
}

pub(super) fn approved_value(resolution: &ApprovalResolution) -> JsonValue {
    resolution
        .approved()
        .map_or(JsonValue::Null, JsonValue::Bool)
}

pub(super) fn approval_status(resolution: &ApprovalResolution) -> &'static str {
    match resolution {
        ApprovalResolution::Approved { .. } => "approved",
        ApprovalResolution::Denied { .. } => "denied",
        ApprovalResolution::Pending { .. } => "pending",
    }
}

pub(super) fn actor_name(actor: &ResolutionResponseActor) -> String {
    match actor {
        ResolutionResponseActor::Human => "human".to_owned(),
        ResolutionResponseActor::Agent => "agent".to_owned(),
    }
}

pub(super) fn artifact_wrap_as(step: &GraphStep) -> Result<&str, RuntimeError> {
    let Some(artifacts) = &step.artifacts else {
        return Ok("approval");
    };
    let Some(value) = artifacts.get("wrap_as") else {
        return Ok("approval");
    };
    string_value_ref(step, "artifacts.wrap_as", value)
}

pub(super) fn artifact_packet(step: &GraphStep) -> Result<Option<String>, RuntimeError> {
    let Some(artifacts) = &step.artifacts else {
        return Ok(None);
    };
    let Some(value) = artifacts.get("packet") else {
        return Ok(None);
    };
    Ok(Some(string_value(step, "artifacts.packet", value)?))
}

pub(super) fn runtime_error_step_run<A>(
    runtime: &Runtime<A>,
    graph_name: &str,
    step: &GraphStep,
    attempt: u32,
    error: RuntimeError,
) -> Result<StepRun, RuntimeError>
where
    A: SkillAdapter,
{
    let output = SkillOutput {
        status: InvocationStatus::Failure,
        stdout: String::new(),
        stderr: error.to_string(),
        exit_code: None,
        duration_ms: 0,
        metadata: JsonObject::new(),
    };
    let outputs = output_object(&output);
    let receipt = step_receipt(
        graph_name,
        &step.id,
        attempt,
        &output,
        &runtime.options.created_at,
    )?;
    Ok(StepRun {
        step_id: step.id.clone(),
        attempt,
        skill: step.skill.as_deref().unwrap_or(step.id.as_str()).to_owned(),
        runner: step.runner.clone(),
        fanout_group: step.fanout_group.clone(),
        output,
        outputs,
        receipt,
    })
}
