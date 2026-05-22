// rust-style-allow: large-file because graph step execution currently keeps
// authority admission, native step execution, approval handling, and payment
// state persistence in one runtime boundary until the runner module split.
use std::path::Path;

use runx_contracts::{
    ApprovalGate, AuthorityVerb, ExecutionEvent, JsonObject, JsonValue, ProofKind,
    ResolutionRequest, ResolutionResponseActor,
};
use runx_core::state_machine::StepAdmissionWitness;
use runx_parser::GraphStep;

use super::super::graph::{load_skill, output_object, resolve_inputs, skill_dir};
use super::authority::{
    StepPaymentReplay, authority_denied, enforce_step_authority_admission,
    enforce_step_authority_receipt_before_success, escalate_in_flight_payment_recovery,
    sealed_payment_replay, validate_replayed_payment_supervisor_proof,
};
use super::inputs::{optional_input_string, required_input_string, string_value, string_value_ref};
use super::{Runtime, StepRun};
use crate::RuntimeError;
use crate::adapter::{InvocationStatus, SkillAdapter, SkillInvocation, SkillOutput};
use crate::approval::{ApprovalResolution, request_approval};
use crate::host::Host;
use crate::payment_state::{PaymentStepStateInput, persist_payment_step_state};
use crate::payment_supervisor::{PaymentSupervisorProof, insert_payment_supervisor_proof_metadata};
use crate::receipts::step_receipt;

const EXTERNAL_ADAPTER_HOST_RESOLUTION_REQUEST_METADATA: &str =
    "external_adapter_host_resolution_request";
const EXTERNAL_ADAPTER_HOST_RESOLUTION_RESPONSE_METADATA: &str =
    "external_adapter_host_resolution_response";

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
    if let Some(replay) = sealed_payment_replay(step, &inputs, &runtime.options.env, graph_dir)? {
        return run_replayed_payment_step(graph_dir, graph_name, step, attempt, replay);
    }
    escalate_in_flight_payment_recovery(step, &inputs, &runtime.options.env, graph_dir)?;
    let authority =
        enforce_step_authority_admission(step, &inputs, &runtime.options.env, graph_dir)?;
    if step.run.is_some() {
        return run_native_step(runtime, graph_name, step, attempt, inputs, host);
    }

    let skill_dir = skill_dir(graph_dir, step)?;
    let skill = load_skill(&skill_dir)?;
    let skill_name = skill.name.clone();
    let mut output = runtime.adapter.invoke(SkillInvocation {
        skill_name: skill.name,
        source: skill.source,
        inputs,
        resolved_inputs: JsonObject::new(),
        skill_directory: skill_dir,
        env: runtime.options.env.clone(),
        credential_delivery: crate::credentials::CredentialDelivery::none(),
    })?;
    route_external_adapter_host_resolution(step, host, &mut output)?;
    let outputs = output_object(&output);
    let receipt = step_receipt(
        graph_name,
        &step.id,
        attempt,
        &output,
        &runtime.options.created_at,
    )?;
    let supervisor_proof = enforce_step_authority_receipt_before_success(
        step,
        authority.as_ref(),
        &output,
        &outputs,
        &receipt,
    )?;
    if let Some(proof) = supervisor_proof.as_ref() {
        insert_payment_supervisor_proof_metadata(&mut output.metadata, proof).map_err(
            |source| {
                authority_denied(
                    step,
                    AuthorityVerb::Spend,
                    format!("recording supervisor proof metadata failed: {source}"),
                )
            },
        )?;
    }
    persist_payment_state_for_step(
        runtime,
        graph_dir,
        step,
        authority.as_ref(),
        &outputs,
        &receipt,
        supervisor_proof.as_ref(),
    )?;
    let admission_witness = step_admission_witness(&step.id, &receipt.id, authority.as_ref());
    Ok(StepRun {
        step_id: step.id.clone(),
        attempt,
        skill: skill_name,
        runner: step.runner.clone(),
        fanout_group: step.fanout_group.clone(),
        output,
        outputs,
        receipt,
        admission_witness,
    })
}

fn route_external_adapter_host_resolution(
    step: &GraphStep,
    host: &mut dyn Host,
    output: &mut SkillOutput,
) -> Result<(), RuntimeError> {
    let Some(JsonValue::Object(request_object)) = output
        .metadata
        .get(EXTERNAL_ADAPTER_HOST_RESOLUTION_REQUEST_METADATA)
        .cloned()
    else {
        return Ok(());
    };
    let request: ResolutionRequest =
        serde_json::to_value(JsonValue::Object(request_object.clone()))
            .and_then(serde_json::from_value)
            .map_err(|source| {
                RuntimeError::json("parsing external adapter host-resolution request", source)
            })?;
    host.report(ExecutionEvent::ResolutionRequested {
        message: format!(
            "external adapter step '{}' requested host resolution",
            step.id
        ),
        data: Some(JsonValue::Object(host_resolution_event_data(
            step,
            JsonValue::Object(request_object),
        ))),
    })?;
    let Some(response) = host.resolve(request)? else {
        return Ok(());
    };
    let response_value: JsonValue = serde_json::to_value(&response)
        .and_then(serde_json::from_value)
        .map_err(|source| {
            RuntimeError::json(
                "serializing external adapter host-resolution response",
                source,
            )
        })?;
    output.metadata.insert(
        EXTERNAL_ADAPTER_HOST_RESOLUTION_RESPONSE_METADATA.to_owned(),
        response_value.clone(),
    );
    host.report(ExecutionEvent::ResolutionResolved {
        message: format!(
            "external adapter step '{}' host resolution resolved",
            step.id
        ),
        data: Some(JsonValue::Object(host_resolution_event_data(
            step,
            response_value,
        ))),
    })
}

fn host_resolution_event_data(step: &GraphStep, payload: JsonValue) -> JsonObject {
    let mut data = JsonObject::new();
    data.insert("step_id".to_owned(), JsonValue::String(step.id.clone()));
    data.insert("payload".to_owned(), payload);
    data
}

fn persist_payment_state_for_step<A>(
    runtime: &Runtime<A>,
    graph_dir: &Path,
    step: &GraphStep,
    authority: Option<&super::authority::StepAuthorityContext>,
    outputs: &JsonObject,
    receipt: &runx_contracts::HarnessReceipt,
    supervisor_proof: Option<&PaymentSupervisorProof>,
) -> Result<(), RuntimeError>
where
    A: SkillAdapter,
{
    let Some(payment) = authority.and_then(|authority| authority.payment.as_ref()) else {
        return Ok(());
    };
    persist_payment_step_state(
        &runtime.options.env,
        graph_dir,
        &PaymentStepStateInput {
            idempotency_key: payment.idempotency_key.clone(),
            spend_capability_ref: payment.spend_capability_ref.uri.clone(),
            rail: payment.rail.clone(),
            counterparty: payment.counterparty.clone(),
            amount_minor: payment.amount_minor,
            currency: payment.currency.clone(),
            act_id: format!("act_{}", step.id),
        },
        outputs,
        receipt,
        supervisor_proof,
    )
    .map_err(|source| RuntimeError::payment_state("persisting payment step state", source))
}

fn run_replayed_payment_step(
    graph_dir: &Path,
    graph_name: &str,
    step: &GraphStep,
    attempt: u32,
    replay: StepPaymentReplay,
) -> Result<StepRun, RuntimeError> {
    let skill_dir = skill_dir(graph_dir, step)?;
    let skill = load_skill(&skill_dir)?;
    let skill_name = skill.name.clone();
    let output = replay_skill_output(step, &replay.outputs)?;
    if !output.succeeded() {
        return Err(authority_denied(
            step,
            AuthorityVerb::Spend,
            "sealed payment replay requires a successful stored rail output".to_owned(),
        ));
    }
    let receipt = step_receipt(
        graph_name,
        &step.id,
        attempt,
        &output,
        &replay.receipt_created_at,
    )?;
    if receipt.id != replay.receipt_ref {
        return Err(authority_denied(
            step,
            AuthorityVerb::Spend,
            format!(
                "sealed payment replay rebuilt receipt {}, expected {}",
                receipt.id, replay.receipt_ref
            ),
        ));
    }
    if receipt.seal.digest != replay.receipt_digest {
        return Err(authority_denied(
            step,
            AuthorityVerb::Spend,
            format!(
                "sealed payment replay rebuilt receipt digest {}, expected {}",
                receipt.seal.digest, replay.receipt_digest
            ),
        ));
    }
    if !receipt_has_payment_rail_proof(&receipt, &replay.rail_proof_ref) {
        return Err(authority_denied(
            step,
            AuthorityVerb::Spend,
            format!(
                "sealed payment replay rebuilt receipt without rail proof {}",
                replay.rail_proof_ref
            ),
        ));
    }
    validate_replayed_payment_supervisor_proof(step, &replay)?;
    let mut output = output;
    insert_payment_supervisor_proof_metadata(&mut output.metadata, &replay.supervisor_proof)
        .map_err(|source| {
            authority_denied(
                step,
                AuthorityVerb::Spend,
                format!("recording replayed supervisor proof metadata failed: {source}"),
            )
        })?;
    let admission_witness = StepAdmissionWitness::local_runtime(&step.id, &replay.receipt_ref);
    Ok(StepRun {
        step_id: step.id.clone(),
        attempt,
        skill: skill_name,
        runner: step.runner.clone(),
        fanout_group: step.fanout_group.clone(),
        output,
        outputs: replay.outputs,
        receipt,
        admission_witness,
    })
}

fn replay_skill_output(
    step: &GraphStep,
    outputs: &JsonObject,
) -> Result<SkillOutput, RuntimeError> {
    let status = match outputs.get("status") {
        Some(JsonValue::String(value)) if value == "success" => InvocationStatus::Success,
        Some(JsonValue::String(value)) if value == "failure" => InvocationStatus::Failure,
        Some(JsonValue::String(value)) => {
            return Err(RuntimeError::InvalidRunStep {
                step_id: step.id.clone(),
                reason: format!("payment replay output status {value:?} is not supported"),
            });
        }
        Some(_) => {
            return Err(RuntimeError::InvalidRunStep {
                step_id: step.id.clone(),
                reason: "payment replay output status must be a string".to_owned(),
            });
        }
        None => InvocationStatus::Success,
    };
    let stdout = match outputs.get("stdout") {
        Some(JsonValue::String(value)) => value.clone(),
        Some(_) => {
            return Err(RuntimeError::InvalidRunStep {
                step_id: step.id.clone(),
                reason: "payment replay output stdout must be a string".to_owned(),
            });
        }
        None => serde_json::to_string(&JsonValue::Object(replay_stdout_payload(outputs)))
            .map_err(|source| RuntimeError::json("serializing payment replay stdout", source))?,
    };
    let stderr = match outputs.get("stderr") {
        Some(JsonValue::String(value)) => value.clone(),
        Some(_) => {
            return Err(RuntimeError::InvalidRunStep {
                step_id: step.id.clone(),
                reason: "payment replay output stderr must be a string".to_owned(),
            });
        }
        None => String::new(),
    };
    Ok(SkillOutput {
        exit_code: Some(if status == InvocationStatus::Success {
            0
        } else {
            1
        }),
        status,
        stdout,
        stderr,
        duration_ms: 0,
        metadata: JsonObject::new(),
    })
}

fn replay_stdout_payload(outputs: &JsonObject) -> JsonObject {
    let mut payload = outputs.clone();
    payload.remove("stdout");
    payload.remove("stderr");
    payload.remove("status");
    payload
}

fn receipt_has_payment_rail_proof(
    receipt: &runx_contracts::HarnessReceipt,
    rail_proof_ref: &str,
) -> bool {
    receipt.harness.acts.iter().any(|act| {
        act.verification_refs.iter().any(|reference| {
            reference.uri == rail_proof_ref
                && reference.proof_kind.as_ref() == Some(&ProofKind::PaymentRail)
        })
    })
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
    let admission_witness = StepAdmissionWitness::local_runtime(&step.id, &receipt.id);
    Ok(StepRun {
        step_id: step.id.clone(),
        attempt,
        skill: "run:approval".to_owned(),
        runner: step.runner.clone(),
        fanout_group: step.fanout_group.clone(),
        output,
        outputs,
        receipt,
        admission_witness,
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
    let admission_witness = StepAdmissionWitness::local_runtime(&step.id, &receipt.id);
    Ok(StepRun {
        step_id: step.id.clone(),
        attempt,
        skill: step.skill.as_deref().unwrap_or(step.id.as_str()).to_owned(),
        runner: step.runner.clone(),
        fanout_group: step.fanout_group.clone(),
        output,
        outputs,
        receipt,
        admission_witness,
    })
}

fn step_admission_witness(
    step_id: &str,
    receipt_id: &str,
    authority: Option<&super::authority::StepAuthorityContext>,
) -> StepAdmissionWitness {
    authority.map_or_else(
        || StepAdmissionWitness::local_runtime(step_id, receipt_id),
        |authority| {
            StepAdmissionWitness::with_authority(
                step_id,
                receipt_id,
                authority.admission_witness.clone(),
            )
        },
    )
}
