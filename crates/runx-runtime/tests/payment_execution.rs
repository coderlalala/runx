use std::cell::RefCell;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use runx_contracts::{
    ExecutionEvent, JsonObject, JsonValue, ResolutionRequest, ResolutionResponse,
    ResolutionResponseActor,
};
use runx_core::state_machine::GraphStatus;
use runx_receipts::ReceiptTreeConfig;
use runx_runtime::{
    Caller, InvocationStatus, Runtime, RuntimeError, RuntimeOptions, SkillAdapter, SkillInvocation,
    SkillOutput, validate_runtime_receipt_tree,
};
use serde_json::{Value, json};
use tempfile::TempDir;

#[test]
fn approved_payment_approval_emits_approval_output_and_runs_fulfill()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = GraphFixture::new()?;
    let runtime = Runtime::new(RecordingAdapter::default(), RuntimeOptions::default());
    let mut caller = ApprovalCaller::approved(true);

    let run = runtime.run_graph_file_with_caller(fixture.graph_path(), &mut caller)?;

    assert_eq!(run.state.status, GraphStatus::Succeeded);
    assert_eq!(step_ids(&run.steps), vec!["approve-spend", "fulfill"]);
    let approval_step = step_run(&run.steps, "approve-spend")?;
    assert_eq!(
        approval_value(approval_step, "approved")?,
        JsonValue::Bool(true)
    );
    assert_eq!(
        approval_value(approval_step, "gate_id")?,
        JsonValue::String("spend-approval".to_owned())
    );
    assert!(
        approval_step
            .outputs
            .get("payment_approval")
            .is_some_and(|value| matches!(value, JsonValue::Object(_)))
    );
    assert_eq!(caller.requests.borrow().len(), 1);
    Ok(())
}

#[test]
fn denied_payment_approval_emits_denied_output_and_blocks_fulfill()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = GraphFixture::new()?;
    let runtime = Runtime::new(RecordingAdapter::default(), RuntimeOptions::default());
    let mut caller = ApprovalCaller::approved(false);

    let checkpoint =
        runtime.run_graph_file_until_steps_with_caller(fixture.graph_path(), 1, &mut caller)?;

    assert_eq!(step_ids(&checkpoint.steps), vec!["approve-spend"]);
    let approval_step = step_run(&checkpoint.steps, "approve-spend")?;
    assert_eq!(
        approval_value(approval_step, "approved")?,
        JsonValue::Bool(false)
    );

    let result =
        runtime.resume_graph_file_with_caller(fixture.graph_path(), checkpoint, &mut caller);
    match result {
        Err(RuntimeError::GraphBlocked { step_id, reason }) => {
            assert_eq!(step_id, "fulfill");
            assert!(
                reason.contains("approve-spend.payment_approval.data.approved"),
                "blocked reason should name the failed transition gate"
            );
        }
        Ok(run) => {
            return Err(std::io::Error::other(format!(
                "expected fulfill to be blocked, ran steps {:?}",
                step_ids(&run.steps)
            ))
            .into());
        }
        Err(error) => {
            return Err(std::io::Error::other(format!("unexpected runtime error: {error}")).into());
        }
    }
    Ok(())
}

#[test]
fn payment_approval_step_is_recorded_with_receipt() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = GraphFixture::new()?;
    let runtime = Runtime::new(RecordingAdapter::default(), RuntimeOptions::default());
    let mut caller = ApprovalCaller::approved(true);

    let run = runtime.run_graph_file_with_caller(fixture.graph_path(), &mut caller)?;

    let approval_step = step_run(&run.steps, "approve-spend")?;
    assert_eq!(approval_step.attempt, 1);
    assert_eq!(
        approval_step.receipt.harness.harness_id,
        "hrn_payment-execution_approve-spend"
    );
    assert_eq!(
        run.state
            .steps
            .iter()
            .find(|step| step.step_id == "approve-spend")
            .and_then(|step| step.receipt_id.as_deref()),
        Some(approval_step.receipt.id.as_str())
    );
    Ok(())
}

#[test]
fn payment_graph_seals_with_strict_parent_child_receipt_proof()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = GraphFixture::new()?;
    let runtime = Runtime::new(RecordingAdapter::default(), RuntimeOptions::default());
    let mut caller = ApprovalCaller::approved(true);

    let run = runtime.run_graph_file_with_caller(fixture.graph_path(), &mut caller)?;
    let child_receipts = run
        .steps
        .iter()
        .map(|step| step.receipt.clone())
        .collect::<Vec<_>>();

    assert!(
        validate_runtime_receipt_tree(&run.receipt, child_receipts, ReceiptTreeConfig::default())
            .is_ok(),
        "payment graph receipt must validate through strict runtime proof acceptance"
    );
    let fulfill = step_run(&run.steps, "fulfill")?;
    assert!(
        fulfill.receipt.harness.acts[0]
            .verification_refs
            .iter()
            .any(|reference| reference.uri == "receipt-proof:mock:payment-execution-001"),
        "payment fulfillment act must carry the rail proof reference into the sealed receipt"
    );
    Ok(())
}

#[test]
fn payment_spend_success_without_rail_proof_is_denied_before_graph_success()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = GraphFixture::new()?;
    let runtime = Runtime::new(
        RecordingAdapter::without_rail_proof(),
        RuntimeOptions::default(),
    );
    let mut caller = ApprovalCaller::approved(true);

    let result = runtime.run_graph_file_with_caller(fixture.graph_path(), &mut caller);

    match result {
        Err(RuntimeError::PaymentAuthorityDenied { step_id, reason }) => {
            assert_eq!(step_id, "fulfill");
            assert!(
                reason.contains("rail proof"),
                "payment authority denial should identify the missing rail proof"
            );
        }
        Ok(run) => {
            assert_ne!(run.state.status, GraphStatus::Succeeded);
            return Err(std::io::Error::other(
                "payment spend step without rail proof must not succeed the graph",
            )
            .into());
        }
        Err(error) => {
            return Err(std::io::Error::other(format!("unexpected runtime error: {error}")).into());
        }
    }
    assert!(
        !caller
            .events
            .borrow()
            .iter()
            .any(|event| matches!(event, ExecutionEvent::Completed { .. })),
        "graph completion must not be reported after missing rail proof"
    );
    Ok(())
}

#[test]
fn payment_spend_missing_reserved_payment_authority_blocks_before_adapter_invocation()
-> Result<(), Box<dyn std::error::Error>> {
    assert_payment_admission_denied_before_adapter(
        FulfillAdmission::MissingReservedPaymentAuthority,
        "reserved_payment_authority",
    )
}

#[test]
fn payment_spend_missing_spend_capability_ref_blocks_before_adapter_invocation()
-> Result<(), Box<dyn std::error::Error>> {
    assert_payment_admission_denied_before_adapter(
        FulfillAdmission::MissingSpendCapabilityRef,
        "spend_capability_ref",
    )
}

#[test]
fn payment_spend_missing_idempotency_key_blocks_before_adapter_invocation()
-> Result<(), Box<dyn std::error::Error>> {
    assert_payment_admission_denied_before_adapter(
        FulfillAdmission::MissingIdempotencyKey,
        "idempotency.key",
    )
}

#[test]
fn payment_spend_missing_subset_proof_blocks_before_adapter_invocation()
-> Result<(), Box<dyn std::error::Error>> {
    assert_payment_admission_denied_before_adapter(
        FulfillAdmission::MissingSubsetProof,
        "subset proof",
    )
}

#[test]
fn payment_spend_amount_widening_blocks_before_adapter_invocation()
-> Result<(), Box<dyn std::error::Error>> {
    assert_payment_admission_denied_before_adapter(FulfillAdmission::AmountWidening, "not a subset")
}

#[test]
fn non_payment_step_without_rail_admission_inputs_invokes_adapter()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture =
        GraphFixture::with_fulfill_options(FulfillAdmission::MissingAll, FulfillScope::None)?;
    let adapter = RecordingAdapter::default();
    let invocations = adapter.invocations();
    let runtime = Runtime::new(adapter, RuntimeOptions::default());
    let mut caller = ApprovalCaller::approved(true);

    let run = runtime.run_graph_file_with_caller(fixture.graph_path(), &mut caller)?;

    assert_eq!(run.state.status, GraphStatus::Succeeded);
    assert_eq!(
        invocations.borrow().as_slice(),
        &["fulfill".to_owned()],
        "non-payment steps should not require payment rail admission inputs"
    );
    Ok(())
}

fn assert_payment_admission_denied_before_adapter(
    admission: FulfillAdmission,
    expected_reason_fragment: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = GraphFixture::with_fulfill_options(admission, FulfillScope::PaymentSpend)?;
    let adapter = RecordingAdapter::default();
    let invocations = adapter.invocations();
    let runtime = Runtime::new(adapter, RuntimeOptions::default());
    let mut caller = ApprovalCaller::approved(true);

    let result = runtime.run_graph_file_with_caller(fixture.graph_path(), &mut caller);

    match result {
        Err(RuntimeError::PaymentAuthorityDenied { step_id, reason }) => {
            assert_eq!(step_id, "fulfill");
            assert!(
                reason.contains(expected_reason_fragment),
                "payment authority denial should name the missing admission input"
            );
        }
        Ok(run) => {
            return Err(std::io::Error::other(format!(
                "expected fulfill to be denied, ran steps {:?}",
                step_ids(&run.steps)
            ))
            .into());
        }
        Err(error) => {
            return Err(std::io::Error::other(format!("unexpected runtime error: {error}")).into());
        }
    }
    assert!(
        invocations.borrow().is_empty(),
        "payment rail admission must deny before invoking the adapter"
    );
    Ok(())
}

struct RecordingAdapter {
    invocations: Rc<RefCell<Vec<String>>>,
    stdout: String,
}

impl Default for RecordingAdapter {
    fn default() -> Self {
        Self::with_stdout(
            r#"{"payment_rail_packet":{"data":{"rail_result":{"status":"fulfilled","rail":"mock","amount_minor":125,"currency":"USD"},"rail_proof":{"proof_ref":"receipt-proof:mock:payment-execution-001","idempotency_key":"payment:payment-execution-001"},"credential_envelope":{"form":"paid_tool_credential","credential_ref":"credential:mock:payment-execution-001"}}}}"#,
        )
    }
}

impl RecordingAdapter {
    fn without_rail_proof() -> Self {
        Self::with_stdout(
            r#"{"payment_rail_packet":{"data":{"rail_result":{"status":"fulfilled","rail":"mock","amount_minor":125,"currency":"USD"},"credential_envelope":{"form":"paid_tool_credential","credential_ref":"credential:mock:payment-execution-001"}}}}"#,
        )
    }

    fn with_stdout(stdout: &str) -> Self {
        Self {
            invocations: Rc::new(RefCell::new(Vec::new())),
            stdout: stdout.to_owned(),
        }
    }

    fn invocations(&self) -> Rc<RefCell<Vec<String>>> {
        Rc::clone(&self.invocations)
    }
}

impl SkillAdapter for RecordingAdapter {
    fn adapter_type(&self) -> &'static str {
        "payment-execution-test"
    }

    fn invoke(&self, request: SkillInvocation) -> Result<SkillOutput, RuntimeError> {
        self.invocations.borrow_mut().push(request.skill_name);
        Ok(SkillOutput {
            status: InvocationStatus::Success,
            stdout: self.stdout.clone(),
            stderr: String::new(),
            exit_code: Some(0),
            duration_ms: 1,
            metadata: JsonObject::new(),
        })
    }
}

struct ApprovalCaller {
    events: RefCell<Vec<ExecutionEvent>>,
    requests: RefCell<Vec<ResolutionRequest>>,
    responses: RefCell<VecDeque<Option<ResolutionResponse>>>,
}

impl ApprovalCaller {
    fn approved(approved: bool) -> Self {
        Self {
            events: RefCell::new(Vec::new()),
            requests: RefCell::new(Vec::new()),
            responses: RefCell::new(VecDeque::from([Some(ResolutionResponse {
                actor: ResolutionResponseActor::Human,
                payload: JsonValue::Bool(approved),
            })])),
        }
    }
}

impl Caller for ApprovalCaller {
    fn report(&mut self, event: ExecutionEvent) -> Result<(), RuntimeError> {
        self.events.borrow_mut().push(event);
        Ok(())
    }

    fn resolve(
        &mut self,
        request: ResolutionRequest,
    ) -> Result<Option<ResolutionResponse>, RuntimeError> {
        self.requests.borrow_mut().push(request);
        Ok(self.responses.borrow_mut().pop_front().flatten())
    }
}

struct GraphFixture {
    _temp: TempDir,
    graph_path: PathBuf,
}

impl GraphFixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Self::with_fulfill_options(FulfillAdmission::Valid, FulfillScope::PaymentSpend)
    }

    fn with_fulfill_options(
        admission: FulfillAdmission,
        scope: FulfillScope,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let fulfill_dir = temp.path().join("fulfill");
        fs::create_dir(&fulfill_dir)?;
        fs::write(
            fulfill_dir.join("SKILL.md"),
            r#"---
name: fulfill
description: Fulfill approved payment.
source:
  type: cli-tool
  command: runx-payment-test
---

Fulfill the approved payment.
"#,
        )?;
        let graph_path = temp.path().join("graph.yaml");
        fs::write(&graph_path, graph_yaml(admission, scope)?)?;
        Ok(Self {
            _temp: temp,
            graph_path,
        })
    }

    fn graph_path(&self) -> &Path {
        self.graph_path.as_path()
    }
}

#[derive(Clone, Copy)]
enum FulfillAdmission {
    Valid,
    MissingReservedPaymentAuthority,
    MissingSpendCapabilityRef,
    MissingIdempotencyKey,
    MissingSubsetProof,
    AmountWidening,
    MissingAll,
}

#[derive(Clone, Copy)]
enum FulfillScope {
    PaymentSpend,
    None,
}

fn graph_yaml(
    admission: FulfillAdmission,
    scope: FulfillScope,
) -> Result<String, serde_json::Error> {
    let mut fulfill = json!({
        "id": "fulfill",
        "skill": "./fulfill",
    });
    if matches!(scope, FulfillScope::PaymentSpend) {
        fulfill["scopes"] = json!(["payment:spend"]);
    }
    if let Some(inputs) = fulfill_inputs(admission) {
        fulfill["inputs"] = inputs;
    }
    serde_json::to_string_pretty(&json!({
        "name": "payment-execution",
        "steps": [
            {
                "id": "approve-spend",
                "run": { "type": "approval" },
                "inputs": {
                    "gate_id": "spend-approval",
                    "gate_type": "payment",
                    "reason": "Approve payment before fulfillment.",
                    "amount_minor": 125,
                    "currency": "USD"
                },
                "artifacts": { "wrap_as": "payment_approval" }
            },
            fulfill
        ],
        "policy": {
            "transitions": [
                {
                    "to": "fulfill",
                    "field": "approve-spend.payment_approval.data.approved",
                    "equals": true
                }
            ]
        }
    }))
}

fn fulfill_inputs(admission: FulfillAdmission) -> Option<Value> {
    match admission {
        FulfillAdmission::Valid => Some(valid_payment_inputs(2_500, true)),
        FulfillAdmission::MissingReservedPaymentAuthority => Some(json!({
            "spend_capability_ref": spend_capability_ref(),
            "idempotency": { "key": "payment:payment-execution-001" }
        })),
        FulfillAdmission::MissingSpendCapabilityRef => Some(json!({
            "reserved_payment_authority": reserved_payment_authority(2_500, true),
            "idempotency": { "key": "payment:payment-execution-001" }
        })),
        FulfillAdmission::MissingIdempotencyKey => Some(json!({
            "reserved_payment_authority": reserved_payment_authority(2_500, true),
            "spend_capability_ref": spend_capability_ref(),
            "idempotency": {}
        })),
        FulfillAdmission::MissingSubsetProof => Some(valid_payment_inputs(2_500, false)),
        FulfillAdmission::AmountWidening => Some(valid_payment_inputs(20_000, true)),
        FulfillAdmission::MissingAll => None,
    }
}

fn valid_payment_inputs(child_max_per_call_minor: u64, subset_proof_present: bool) -> Value {
    json!({
        "reserved_payment_authority": reserved_payment_authority(child_max_per_call_minor, subset_proof_present),
        "spend_capability_ref": spend_capability_ref(),
        "idempotency": { "key": "payment:payment-execution-001" }
    })
}

fn reserved_payment_authority(child_max_per_call_minor: u64, subset_proof_present: bool) -> Value {
    json!({
        "parent_authority": payment_term("parent", ["quote", "reserve", "spend", "verify"], 10_000),
        "child_authority": payment_term("child", ["reserve", "spend"], child_max_per_call_minor),
        "reservation_decision": reservation_decision(),
        "subset_proof_present": subset_proof_present,
        "child_harness_ref": child_harness_ref(),
        "spend_capability_binding": {
            "child_harness_ref": child_harness_ref(),
            "act_id": "act_fulfill",
            "reservation_decision_id": "decision_payment_reservation",
            "idempotency_key": "payment:payment-execution-001",
            "amount_minor": 125,
            "currency": "USD",
            "counterparty": "merchant-123",
            "rail": "mock"
        },
        "consumed_spend_capability_refs": []
    })
}

fn payment_term<const N: usize>(term_id: &str, verbs: [&str; N], max_per_call_minor: u64) -> Value {
    let verbs = verbs.as_slice();
    json!({
        "term_id": term_id,
        "principal_ref": reference("principal", "runx:principal:merchant-agent"),
        "resource_ref": reference("grant", "runx:payment-grant:checkout"),
        "resource_family": "payment",
        "verbs": verbs,
        "bounds": {
            "payment": {
                "currency": "USD",
                "max_per_call_minor": max_per_call_minor,
                "max_per_run_minor": 25_000,
                "rails": ["mock", "card"],
                "counterparty": "merchant-123",
                "operation": "checkout",
                "credential_form": "single_use_spend_capability",
                "quote_required": true,
                "reservation_required": true,
                "idempotency_required": true,
                "recovery_required": true,
                "receipt_before_success": true,
                "single_use_spend": true
            }
        },
        "capabilities": ["payment_single_use_spend"],
        "expires_at": "2026-05-21T00:00:00Z",
        "issued_by_ref": reference("grant", "runx:grant:issuer"),
        "credential_ref": reference("credential", "runx:credential:payment-session")
    })
}

fn reservation_decision() -> Value {
    json!({
        "decision_id": "decision_payment_reservation",
        "choice": "continue",
        "inputs": {
            "signal_refs": [],
            "target_ref": null,
            "opportunity_refs": [],
            "selection_ref": null
        },
        "proposed_intent": {
            "purpose": "complete a bounded checkout payment",
            "legitimacy": "authorized by selected reservation decision",
            "success_criteria": [],
            "constraints": [],
            "derived_from": []
        },
        "selected_act_id": "act_fulfill",
        "selected_harness_ref": null,
        "justification": {
            "summary": "reservation selected a bounded spend act",
            "evidence_refs": []
        },
        "closure": null,
        "artifact_refs": []
    })
}

fn child_harness_ref() -> Value {
    reference("harness", "runx:harness:payment-execution_fulfill")
}

fn spend_capability_ref() -> Value {
    reference("credential", "runx:payment-capability:spend-1")
}

fn reference(reference_type: &str, uri: &str) -> Value {
    json!({ "type": reference_type, "uri": uri })
}

fn step_ids(steps: &[runx_runtime::StepRun]) -> Vec<&str> {
    steps.iter().map(|step| step.step_id.as_str()).collect()
}

fn step_run<'a>(
    steps: &'a [runx_runtime::StepRun],
    step_id: &str,
) -> Result<&'a runx_runtime::StepRun, std::io::Error> {
    steps
        .iter()
        .find(|step| step.step_id == step_id)
        .ok_or_else(|| std::io::Error::other(format!("missing step {step_id}")))
}

fn approval_value(step: &runx_runtime::StepRun, field: &str) -> Result<JsonValue, std::io::Error> {
    let payment_approval = object_field(&step.outputs, "payment_approval")?;
    let data = object_field(payment_approval, "data")?;
    data.get(field)
        .cloned()
        .ok_or_else(|| std::io::Error::other(format!("missing payment_approval.data.{field}")))
}

fn object_field<'a>(object: &'a JsonObject, field: &str) -> Result<&'a JsonObject, std::io::Error> {
    match object.get(field) {
        Some(JsonValue::Object(value)) => Ok(value),
        Some(_) => Err(std::io::Error::other(format!("{field} is not an object"))),
        None => Err(std::io::Error::other(format!("{field} is missing"))),
    }
}
