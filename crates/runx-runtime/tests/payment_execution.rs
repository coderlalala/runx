use std::cell::RefCell;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

use runx_contracts::{
    ExecutionEvent, JsonObject, JsonValue, ResolutionRequest, ResolutionResponse,
    ResolutionResponseActor,
};
use runx_core::state_machine::GraphStatus;
use runx_runtime::{
    Caller, InvocationStatus, Runtime, RuntimeError, RuntimeOptions, SkillAdapter, SkillInvocation,
    SkillOutput,
};
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

#[derive(Default)]
struct RecordingAdapter {
    invocations: RefCell<Vec<String>>,
}

impl SkillAdapter for RecordingAdapter {
    fn adapter_type(&self) -> &'static str {
        "payment-execution-test"
    }

    fn invoke(&self, request: SkillInvocation) -> Result<SkillOutput, RuntimeError> {
        self.invocations.borrow_mut().push(request.skill_name);
        Ok(SkillOutput {
            status: InvocationStatus::Success,
            stdout: r#"{"fulfilled":true}"#.to_owned(),
            stderr: String::new(),
            exit_code: Some(0),
            duration_ms: 1,
            metadata: JsonObject::new(),
        })
    }
}

struct ApprovalCaller {
    requests: RefCell<Vec<ResolutionRequest>>,
    responses: RefCell<VecDeque<Option<ResolutionResponse>>>,
}

impl ApprovalCaller {
    fn approved(approved: bool) -> Self {
        Self {
            requests: RefCell::new(Vec::new()),
            responses: RefCell::new(VecDeque::from([Some(ResolutionResponse {
                actor: ResolutionResponseActor::Human,
                payload: JsonValue::Bool(approved),
            })])),
        }
    }
}

impl Caller for ApprovalCaller {
    fn report(&mut self, _event: ExecutionEvent) -> Result<(), RuntimeError> {
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
        fs::write(&graph_path, graph_yaml())?;
        Ok(Self {
            _temp: temp,
            graph_path,
        })
    }

    fn graph_path(&self) -> &Path {
        self.graph_path.as_path()
    }
}

fn graph_yaml() -> &'static str {
    r#"
name: payment-execution
steps:
  - id: approve-spend
    run:
      type: approval
    inputs:
      gate_id: spend-approval
      gate_type: payment
      reason: Approve payment before fulfillment.
      amount_minor: 125
      currency: USD
    artifacts:
      wrap_as: payment_approval
  - id: fulfill
    skill: ./fulfill
policy:
  transitions:
    - to: fulfill
      field: approve-spend.payment_approval.data.approved
      equals: true
"#
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
