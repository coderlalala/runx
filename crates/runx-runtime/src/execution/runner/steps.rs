// rust-style-allow: large-file because graph step execution currently keeps
// authority admission, native step execution, approval handling, and payment
// state persistence in one runtime boundary until the runner module split.
use std::path::Path;

use runx_contracts::{
    ApprovalGate, AuthorityVerb, ClosureDisposition, ExecutionEvent, JsonObject, JsonValue,
    ProofKind, ResolutionRequest, ResolutionResponse, ResolutionResponseActor,
};
use runx_core::state_machine::StepAdmissionWitness;
use runx_parser::{GraphStep, SkillSource, SourceKind};

use super::super::graph::{load_step_skill, output_object, resolve_inputs, skill_claim_object};
use super::authority::{
    StepPaymentReplay, attach_payment_supervisor_evidence_before_gate, authority_denied,
    enforce_step_authority_admission, enforce_step_authority_receipt_before_success,
    escalate_in_flight_payment_recovery, sealed_payment_replay,
    validate_replayed_payment_supervisor_proof,
};
use super::inputs::{optional_input_string, required_input_string, string_value, string_value_ref};
use super::{Runtime, StepRun};
use crate::RuntimeError;
use crate::adapter::{InvocationStatus, SkillAdapter, SkillInvocation, SkillOutput};
#[cfg(feature = "catalog")]
use crate::adapters::catalog::CatalogAdapter;
use crate::agent_invocation::{
    AgentActInvocationSourceType, agent_act_invocation_id, agent_act_resolution_request,
};
use crate::approval::{ApprovalResolution, request_approval};
use crate::host::Host;
use crate::payment::state::{PaymentStepStateInput, persist_payment_step_state};
use crate::payment::supervisor::{
    PaymentSupervisorProof, insert_payment_supervisor_proof_metadata,
};
use crate::receipts::{
    StepReceiptWithDisposition, step_receipt_with_disposition_and_policy,
    step_receipt_with_signature_policy,
};

const EXTERNAL_ADAPTER_HOST_RESOLUTION_REQUEST_METADATA: &str =
    "external_adapter_host_resolution_request";
const EXTERNAL_ADAPTER_HOST_RESOLUTION_RESPONSE_METADATA: &str =
    "external_adapter_host_resolution_response";

struct AgentSkillStepInvocation {
    skill_name: String,
    invocation: SkillInvocation,
    source_type: AgentActInvocationSourceType,
}

pub(super) fn output_error(run: &StepRun) -> String {
    if run.output.stderr.is_empty() {
        "cli-tool failed without stderr".to_owned()
    } else {
        run.output.stderr.clone()
    }
}

// rust-style-allow: long-function - step execution is one linear admit/run/seal sequence; splitting
// it would scatter the ordering invariants between admission, invocation, and receipt sealing.
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
        return run_replayed_payment_step(runtime, graph_dir, graph_name, step, attempt, replay);
    }
    escalate_in_flight_payment_recovery(step, &inputs, &runtime.options.env, graph_dir)?;
    let authority =
        enforce_step_authority_admission(step, &inputs, &runtime.options.env, graph_dir)?;
    if step.run.is_some() {
        return run_native_step(runtime, graph_dir, graph_name, step, attempt, inputs, host);
    }
    if step.tool.is_some() {
        return run_tool_step(runtime, graph_dir, graph_name, step, attempt, inputs);
    }

    let skill = load_step_skill(graph_dir, step)?;
    let skill_name = skill.name.clone();
    let invocation = SkillInvocation {
        skill_name: skill.name,
        source: skill.source,
        inputs,
        resolved_inputs: JsonObject::new(),
        skill_directory: skill.directory,
        env: runtime.options.env.clone(),
        credential_delivery: crate::credentials::CredentialDelivery::none(),
    };
    if let Some(source_type) = agent_skill_source_type(invocation.source.source_type) {
        return run_agent_skill_step(
            runtime,
            graph_name,
            step,
            attempt,
            AgentSkillStepInvocation {
                skill_name,
                invocation,
                source_type,
            },
            host,
        );
    }

    let mut output = runtime.adapter.invoke(invocation)?;
    route_external_adapter_host_resolution(step, host, &mut output)?;
    let mut outputs = step_output_object(step, &output);
    expose_skill_claim_fields(step, &output, &mut outputs);
    let skill_claim = skill_claim_object(&output);
    attach_payment_supervisor_evidence_before_gate(
        step,
        authority.as_ref(),
        &skill_claim,
        &mut output,
        &runtime.options.payment_supervisor,
    )?;
    let receipt = step_receipt_with_signature_policy(
        graph_name,
        &step.id,
        attempt,
        &output,
        &runtime.options.created_at,
        runtime.options.signature_policy(),
    )?;
    let supervisor_proof = enforce_step_authority_receipt_before_success(
        step,
        authority.as_ref(),
        &output,
        &skill_claim,
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
        &skill_claim,
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
    receipt: &runx_contracts::Receipt,
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
            spend_capability_ref: payment.spend_capability_ref.uri.clone().into_string(),
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

// rust-style-allow: long-function - replayed payment step reconstruction is one linear recovery
// path; the reconstruction order must stay together to rebuild state deterministically.
fn run_replayed_payment_step(
    runtime: &Runtime<impl SkillAdapter>,
    graph_dir: &Path,
    graph_name: &str,
    step: &GraphStep,
    attempt: u32,
    replay: StepPaymentReplay,
) -> Result<StepRun, RuntimeError> {
    let skill = load_step_skill(graph_dir, step)?;
    let skill_name = skill.name.clone();
    let mut output = replay_skill_output(step, &replay.outputs)?;
    if !output.succeeded() {
        return Err(authority_denied(
            step,
            AuthorityVerb::Spend,
            "sealed payment replay requires a successful stored rail output".to_owned(),
        ));
    }
    insert_payment_supervisor_proof_metadata(&mut output.metadata, &replay.supervisor_proof)
        .map_err(|source| {
            authority_denied(
                step,
                AuthorityVerb::Spend,
                format!("recording replayed supervisor proof metadata failed: {source}"),
            )
        })?;
    let receipt = step_receipt_with_signature_policy(
        graph_name,
        &step.id,
        attempt,
        &output,
        &replay.receipt_created_at,
        runtime.options.signature_policy(),
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
    if receipt.digest != replay.receipt_digest {
        return Err(authority_denied(
            step,
            AuthorityVerb::Spend,
            format!(
                "sealed payment replay rebuilt receipt digest {}, expected {}",
                receipt.digest, replay.receipt_digest
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
    let outputs = step_output_object(step, &output);
    let admission_witness = StepAdmissionWitness::local_runtime(&step.id, &replay.receipt_ref);
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

fn step_output_object(step: &GraphStep, output: &SkillOutput) -> JsonObject {
    let mut outputs = output_object(output);
    expose_declared_run_outputs(step, output, &mut outputs);
    expose_declared_artifacts(step, output, &mut outputs);
    outputs
}

fn expose_declared_run_outputs(step: &GraphStep, output: &SkillOutput, outputs: &mut JsonObject) {
    let Some(run) = &step.run else {
        return;
    };
    let Some(JsonValue::Object(declared_outputs)) = run.get("outputs") else {
        return;
    };
    let claim = skill_claim_object(output);
    if claim.is_empty() {
        return;
    }

    for name in declared_outputs.keys() {
        if is_reserved_output_field(name) {
            continue;
        }
        let Some(value) = claim.get(name).cloned() else {
            continue;
        };
        outputs.insert(name.clone(), value);
    }
}

fn expose_skill_claim_fields(_step: &GraphStep, output: &SkillOutput, outputs: &mut JsonObject) {
    let claim = skill_claim_object(output);
    if claim.is_empty() {
        return;
    }

    for (name, value) in claim {
        if is_reserved_output_field(&name) || outputs.contains_key(&name) {
            continue;
        }
        outputs.insert(name, value);
    }
}

fn expose_declared_artifacts(step: &GraphStep, output: &SkillOutput, outputs: &mut JsonObject) {
    let Some(artifacts) = &step.artifacts else {
        return;
    };
    let claim = skill_claim_object(output);
    if claim.is_empty() {
        return;
    }

    if let Some(wrap_as) = artifacts.get("wrap_as").and_then(json_string) {
        let value = claim.get(wrap_as).cloned().unwrap_or_else(|| {
            let mut wrapper = JsonObject::new();
            wrapper.insert("data".to_owned(), JsonValue::Object(claim.clone()));
            JsonValue::Object(wrapper)
        });
        outputs.insert(wrap_as.to_owned(), value);
    }

    if let Some(JsonValue::Object(named_emits)) = artifacts.get("named_emits") {
        for name in named_emits.keys() {
            let Some(value) = claim.get(name).cloned() else {
                continue;
            };
            outputs.insert(name.clone(), artifact_data_wrapper(value));
        }
    }
}

fn is_reserved_output_field(name: &str) -> bool {
    matches!(name, "raw" | "skill_claim" | "stdout" | "stderr" | "status")
}

fn artifact_data_wrapper(value: JsonValue) -> JsonValue {
    match value {
        JsonValue::Object(object) if object.contains_key("data") => JsonValue::Object(object),
        other => {
            let mut wrapper = JsonObject::new();
            wrapper.insert("data".to_owned(), other);
            JsonValue::Object(wrapper)
        }
    }
}

fn receipt_has_payment_rail_proof(receipt: &runx_contracts::Receipt, rail_proof_ref: &str) -> bool {
    receipt.acts.iter().any(|act| {
        act.criterion_bindings
            .iter()
            .flat_map(|criterion| criterion.verification_refs.iter())
            .any(|reference| {
                reference.uri == rail_proof_ref
                    && reference.proof_kind.as_ref() == Some(&ProofKind::PaymentRail)
            })
    })
}

pub(super) fn run_native_step<A>(
    runtime: &Runtime<A>,
    graph_dir: &Path,
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
        "agent-step" => run_agent_step(runtime, graph_dir, graph_name, step, attempt, inputs, host),
        other => Err(RuntimeError::UnsupportedRunStep {
            step_id: step.id.clone(),
            run_type: other.to_owned(),
        }),
    }
}

// rust-style-allow: long-function because agent-step execution is one
// request/resolve/seal trust-boundary path.
fn run_agent_step<A>(
    runtime: &Runtime<A>,
    graph_dir: &Path,
    graph_name: &str,
    step: &GraphStep,
    attempt: u32,
    inputs: JsonObject,
    host: &mut dyn Host,
) -> Result<StepRun, RuntimeError>
where
    A: SkillAdapter,
{
    let source = agent_step_source(step)?;
    let invocation = SkillInvocation {
        skill_name: step.id.clone(),
        source,
        inputs,
        resolved_inputs: JsonObject::new(),
        skill_directory: graph_dir.to_path_buf(),
        env: runtime.options.env.clone(),
        credential_delivery: crate::credentials::CredentialDelivery::none(),
    };
    let source_type = AgentActInvocationSourceType::AgentStep;
    let request_id = agent_act_invocation_id(&invocation, source_type);
    let request = agent_act_resolution_request(&invocation, source_type)?;
    host.report(ExecutionEvent::ResolutionRequested {
        message: format!("agent step '{}' requested resolution", step.id),
        data: Some(resolution_event_data(step, &request)?),
    })?;
    let Some(response) = host.resolve(request)? else {
        return Err(RuntimeError::GraphBlocked {
            step_id: step.id.clone(),
            reason: format!("agent act {request_id} requires resolution"),
        });
    };
    let disposition = agent_answer_disposition_value(&response.payload);
    let mut output = agent_step_output(response)?;
    apply_graph_step_artifact_wrappers(&mut output, step)?;
    let outputs = step_output_object(step, &output);
    let disposition_label = closure_disposition_label(&disposition);
    let receipt = step_receipt_with_disposition_and_policy(
        StepReceiptWithDisposition {
            graph_name,
            step_id: &step.id,
            attempt,
            output: &output,
            created_at: &runtime.options.created_at,
            disposition,
            reason_code: format!("agent_act_{disposition_label}"),
            summary: format!("agent act closed with {disposition_label}"),
        },
        runtime.options.signature_policy(),
    )?;
    let admission_witness = StepAdmissionWitness::local_runtime(&step.id, receipt.id.as_str());
    Ok(StepRun {
        step_id: step.id.clone(),
        attempt,
        skill: "run:agent-step".to_owned(),
        runner: step.runner.clone(),
        fanout_group: step.fanout_group.clone(),
        output,
        outputs,
        receipt,
        admission_witness,
    })
}

fn run_agent_skill_step<A>(
    runtime: &Runtime<A>,
    graph_name: &str,
    step: &GraphStep,
    attempt: u32,
    agent_step: AgentSkillStepInvocation,
    host: &mut dyn Host,
) -> Result<StepRun, RuntimeError>
where
    A: SkillAdapter,
{
    let AgentSkillStepInvocation {
        skill_name,
        invocation,
        source_type,
    } = agent_step;
    let request_id = agent_act_invocation_id(&invocation, source_type);
    let request = agent_act_resolution_request(&invocation, source_type)?;
    host.report(ExecutionEvent::ResolutionRequested {
        message: format!(
            "agent skill step '{}' requested resolution for {}",
            step.id, skill_name
        ),
        data: Some(resolution_event_data(step, &request)?),
    })?;
    let Some(response) = host.resolve(request)? else {
        return Err(RuntimeError::GraphBlocked {
            step_id: step.id.clone(),
            reason: format!("agent act {request_id} requires resolution"),
        });
    };
    let disposition = agent_answer_disposition_value(&response.payload);
    let mut output = agent_step_output(response)?;
    apply_graph_step_artifact_wrappers(&mut output, step)?;
    let outputs = step_output_object(step, &output);
    let disposition_label = closure_disposition_label(&disposition);
    let receipt = step_receipt_with_disposition_and_policy(
        StepReceiptWithDisposition {
            graph_name,
            step_id: &step.id,
            attempt,
            output: &output,
            created_at: &runtime.options.created_at,
            disposition,
            reason_code: format!("agent_act_{disposition_label}"),
            summary: format!("agent act closed with {disposition_label}"),
        },
        runtime.options.signature_policy(),
    )?;
    let admission_witness = StepAdmissionWitness::local_runtime(&step.id, receipt.id.as_str());
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

fn agent_skill_source_type(source_type: SourceKind) -> Option<AgentActInvocationSourceType> {
    match source_type {
        SourceKind::Agent => Some(AgentActInvocationSourceType::Agent),
        SourceKind::AgentStep => Some(AgentActInvocationSourceType::AgentStep),
        _ => None,
    }
}

fn agent_step_source(step: &GraphStep) -> Result<SkillSource, RuntimeError> {
    let Some(run) = &step.run else {
        return Err(RuntimeError::InvalidRunStep {
            step_id: step.id.clone(),
            reason: "missing run configuration".to_owned(),
        });
    };
    let mut raw = run.clone();
    if let Some(instructions) = &step.instructions {
        raw.insert(
            "instructions".to_owned(),
            JsonValue::String(instructions.clone()),
        );
    }
    if let Some(allowed_tools) = &step.allowed_tools {
        raw.insert(
            "allowed_tools".to_owned(),
            JsonValue::Array(
                allowed_tools
                    .iter()
                    .cloned()
                    .map(JsonValue::String)
                    .collect(),
            ),
        );
    }
    Ok(SkillSource {
        source_type: SourceKind::AgentStep,
        command: None,
        args: Vec::new(),
        cwd: None,
        timeout_seconds: None,
        input_mode: None,
        sandbox: None,
        server: None,
        catalog_ref: None,
        tool: None,
        arguments: None,
        agent_card_url: None,
        agent_identity: None,
        agent: optional_string(run, "agent"),
        task: optional_string(run, "task"),
        hook: None,
        outputs: optional_object(run, "outputs"),
        graph: None,
        raw,
    })
}

// rust-style-allow: long-function because tool execution keeps lookup,
// invocation, and receipt sealing in one audited boundary.
fn run_tool_step<A>(
    runtime: &Runtime<A>,
    graph_dir: &Path,
    graph_name: &str,
    step: &GraphStep,
    attempt: u32,
    inputs: JsonObject,
) -> Result<StepRun, RuntimeError>
where
    A: SkillAdapter,
{
    #[cfg(not(feature = "catalog"))]
    {
        let _ = (runtime, graph_dir, graph_name, step, attempt, inputs);
        return Err(RuntimeError::UnsupportedAdapter {
            adapter_type: "catalog".to_owned(),
        });
    }

    #[cfg(feature = "catalog")]
    {
        let tool_ref = step
            .tool
            .as_deref()
            .ok_or_else(|| RuntimeError::InvalidRunStep {
                step_id: step.id.clone(),
                reason: "tool step missing tool reference".to_owned(),
            })?;
        let invocation = SkillInvocation {
            skill_name: tool_ref.to_owned(),
            source: catalog_source(tool_ref),
            inputs,
            resolved_inputs: JsonObject::new(),
            skill_directory: graph_dir.to_path_buf(),
            env: runtime.options.env.clone(),
            credential_delivery: crate::credentials::CredentialDelivery::none(),
        };
        let mut output = CatalogAdapter::default().invoke(invocation)?;
        apply_graph_step_artifact_wrappers(&mut output, step)?;
        let mut outputs = step_output_object(step, &output);
        expose_skill_claim_fields(step, &output, &mut outputs);
        let receipt = step_receipt_with_signature_policy(
            graph_name,
            &step.id,
            attempt,
            &output,
            &runtime.options.created_at,
            runtime.options.signature_policy(),
        )?;
        let admission_witness = StepAdmissionWitness::local_runtime(&step.id, receipt.id.as_str());
        Ok(StepRun {
            step_id: step.id.clone(),
            attempt,
            skill: format!("tool:{tool_ref}"),
            runner: step.runner.clone(),
            fanout_group: step.fanout_group.clone(),
            output,
            outputs,
            receipt,
            admission_witness,
        })
    }
}

#[cfg(feature = "catalog")]
#[cfg(feature = "catalog")]
fn catalog_source(tool_ref: &str) -> SkillSource {
    let mut raw = JsonObject::new();
    raw.insert("type".to_owned(), JsonValue::String("catalog".to_owned()));
    raw.insert(
        "catalog_ref".to_owned(),
        JsonValue::String(tool_ref.to_owned()),
    );
    SkillSource {
        source_type: SourceKind::Catalog,
        command: None,
        args: Vec::new(),
        cwd: None,
        timeout_seconds: None,
        input_mode: None,
        sandbox: None,
        server: None,
        catalog_ref: Some(tool_ref.to_owned()),
        tool: None,
        arguments: None,
        agent_card_url: None,
        agent_identity: None,
        agent: None,
        task: None,
        hook: None,
        outputs: None,
        graph: None,
        raw,
    }
}

fn apply_graph_step_artifact_wrappers(
    output: &mut SkillOutput,
    step: &GraphStep,
) -> Result<(), RuntimeError> {
    let Some(artifacts) = &step.artifacts else {
        return Ok(());
    };
    let Ok(JsonValue::Object(mut object)) = serde_json::from_str::<JsonValue>(&output.stdout)
    else {
        return Ok(());
    };

    let mut changed = false;
    if let Some(wrap_as) = artifacts.get("wrap_as").and_then(json_string)
        && !object.contains_key(wrap_as)
    {
        let mut wrapper = JsonObject::new();
        wrapper.insert("data".to_owned(), JsonValue::Object(object.clone()));
        object.insert(wrap_as.to_owned(), JsonValue::Object(wrapper));
        changed = true;
    }

    if let Some(JsonValue::Object(named_emits)) = artifacts.get("named_emits") {
        for name in named_emits.keys() {
            let Some(value) = object.get(name).cloned() else {
                continue;
            };
            let mut wrapper = JsonObject::new();
            wrapper.insert("data".to_owned(), value);
            object.insert(name.clone(), JsonValue::Object(wrapper));
            changed = true;
        }
    }

    if changed {
        output.stdout = serde_json::to_string(&JsonValue::Object(object)).map_err(|source| {
            RuntimeError::json("serializing graph step artifact wrappers", source)
        })?;
    }
    Ok(())
}

fn agent_step_output(response: ResolutionResponse) -> Result<SkillOutput, RuntimeError> {
    let disposition = agent_answer_disposition_value(&response.payload);
    let succeeded = disposition == ClosureDisposition::Closed;
    let stdout = serde_json::to_string(&response.payload)
        .map_err(|source| RuntimeError::json("serializing agent-step response", source))?;
    Ok(SkillOutput {
        status: if succeeded {
            InvocationStatus::Success
        } else {
            InvocationStatus::Failure
        },
        stdout,
        stderr: if succeeded {
            String::new()
        } else {
            format!(
                "agent act closed with {}",
                closure_disposition_label(&disposition)
            )
        },
        exit_code: succeeded.then_some(0),
        duration_ms: 0,
        metadata: JsonObject::new(),
    })
}

fn resolution_event_data(
    step: &GraphStep,
    request: &ResolutionRequest,
) -> Result<JsonValue, RuntimeError> {
    let request_value = serde_json::to_value(request)
        .and_then(serde_json::from_value)
        .map_err(|source| RuntimeError::json("serializing agent-step request", source))?;
    let mut data = JsonObject::new();
    data.insert("step_id".to_owned(), JsonValue::String(step.id.clone()));
    data.insert("request".to_owned(), request_value);
    Ok(JsonValue::Object(data))
}

fn optional_string(object: &JsonObject, field: &str) -> Option<String> {
    object.get(field).and_then(json_string).map(str::to_owned)
}

fn optional_object(object: &JsonObject, field: &str) -> Option<JsonObject> {
    match object.get(field) {
        Some(JsonValue::Object(value)) => Some(value.clone()),
        _ => None,
    }
}

fn json_string(value: &JsonValue) -> Option<&str> {
    match value {
        JsonValue::String(value) => Some(value),
        _ => None,
    }
}

fn agent_answer_disposition_value(answer: &JsonValue) -> ClosureDisposition {
    match json_object(answer)
        .and_then(|object| object.get("closure"))
        .and_then(json_object)
        .and_then(|closure| closure.get("disposition"))
        .and_then(json_string)
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

fn json_object(value: &JsonValue) -> Option<&JsonObject> {
    match value {
        JsonValue::Object(object) => Some(object),
        _ => None,
    }
}

fn closure_disposition_label(disposition: &ClosureDisposition) -> &'static str {
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
    let receipt = step_receipt_with_signature_policy(
        graph_name,
        &step.id,
        attempt,
        &output,
        &runtime.options.created_at,
        runtime.options.signature_policy(),
    )?;
    let admission_witness = StepAdmissionWitness::local_runtime(&step.id, receipt.id.as_str());
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
        id: gate_id.into(),
        reason: reason.into(),
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
    data.insert(
        "gate_id".to_owned(),
        JsonValue::String(gate.id.as_str().to_owned()),
    );
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
    let receipt = step_receipt_with_signature_policy(
        graph_name,
        &step.id,
        attempt,
        &output,
        &runtime.options.created_at,
        runtime.options.signature_policy(),
    )?;
    let admission_witness = StepAdmissionWitness::local_runtime(&step.id, receipt.id.as_str());
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
