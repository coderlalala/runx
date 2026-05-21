use std::cell::RefCell;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use runx_contracts::{
    JsonObject, JsonValue, ProofKind, ResolutionRequest, ResolutionResponse,
    ResolutionResponseActor,
};
use runx_core::state_machine::GraphStatus;
use runx_runtime::{
    Caller, InvocationStatus, Runtime, RuntimeError, RuntimeOptions, SkillAdapter, SkillInvocation,
    SkillOutput,
};
use serde_json::{Value, json};
use tempfile::TempDir;

const STRIPE_SPT_IDEMPOTENCY_KEY: &str = "payment:stripe-spt-demo-001";
const STRIPE_SPT_PROOF_REF: &str = "receipt-proof:stripe-spt:demo-search-001";
const STRIPE_SPT_CREDENTIAL_REF: &str = "credential:stripe-spt:demo-search-001";
const STRIPE_SPT_SESSION_MATERIAL_REF: &str = "rail-session-material:stripe-spt:demo-search-001";

#[test]
fn stripe_spt_payment_seals_happy_path_with_scoped_proof() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = StripeSptFixture::new()?;
    let adapter = StripeSptAdapter::new(StripeSptScenario::Fulfilled);
    let invocations = adapter.invocations();
    let runtime = Runtime::new(adapter, RuntimeOptions::default());
    let mut caller = ApprovalCaller::approved(true);

    let run = runtime.run_graph_file_with_caller(fixture.graph_path(), &mut caller)?;

    assert_eq!(run.state.status, GraphStatus::Succeeded);
    assert_eq!(
        invoked_skills(&invocations),
        vec!["pay-quote", "pay-reserve", "pay-fulfill-rail"],
        "stripe-spt settlement must pass through quote, reserve, and rail fulfill"
    );

    let fulfill = step_run(&run.steps, "fulfill")?;
    assert!(
        fulfill.receipt.harness.acts[0]
            .verification_refs
            .iter()
            .any(|reference| reference.uri == STRIPE_SPT_PROOF_REF
                && reference.locator.as_deref() == Some(STRIPE_SPT_IDEMPOTENCY_KEY)
                && reference.proof_kind.as_ref() == Some(&ProofKind::PaymentRail)),
        "stripe-spt fulfillment must seal a typed payment rail proof"
    );
    assert!(
        fulfill.receipt.harness.acts[0]
            .source_refs
            .iter()
            .any(|reference| reference.uri == STRIPE_SPT_CREDENTIAL_REF),
        "paid tool receives only the scoped credential reference"
    );

    let fulfill_inputs = invocations
        .borrow()
        .iter()
        .find(|invocation| invocation.skill_name == "pay-fulfill-rail")
        .cloned()
        .ok_or_else(|| std::io::Error::other("missing stripe-spt fulfill invocation"))?
        .inputs;
    assert_eq!(
        nested_string(&fulfill_inputs, &["idempotency", "key"]),
        Some(STRIPE_SPT_IDEMPOTENCY_KEY)
    );
    assert_eq!(
        nested_string(
            &fulfill_inputs,
            &[
                "reserved_payment_authority",
                "spend_capability_binding",
                "rail"
            ]
        ),
        Some("stripe-spt")
    );

    let graph_receipt_text = serde_json::to_string(&run.receipt)?;
    let fulfill_receipt_text = serde_json::to_string(&fulfill.receipt)?;
    for receipt_text in [&graph_receipt_text, &fulfill_receipt_text] {
        assert!(!receipt_text.contains("client_secret"));
        assert!(!receipt_text.contains("webhook_secret"));
        assert!(!receipt_text.contains("card_number"));
        assert!(!receipt_text.contains("credential_envelope"));
        assert!(!receipt_text.contains("rail_session_material_ref"));
        assert!(!receipt_text.contains(STRIPE_SPT_SESSION_MATERIAL_REF));
    }
    Ok(())
}

#[test]
fn stripe_spt_payment_decline_returns_governed_error_without_sealing_success()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = StripeSptFixture::new()?;
    let adapter = StripeSptAdapter::new(StripeSptScenario::Declined);
    let invocations = adapter.invocations();
    let runtime = Runtime::new(adapter, RuntimeOptions::default());
    let mut caller = ApprovalCaller::approved(true);

    let result = runtime.run_graph_file_with_caller(fixture.graph_path(), &mut caller);

    match result {
        Err(RuntimeError::SkillFailed {
            skill_name,
            message,
        }) => {
            assert_eq!(skill_name, "fulfill");
            assert!(message.contains("stripe-spt declined"));
            assert!(message.contains(STRIPE_SPT_IDEMPOTENCY_KEY));
        }
        Ok(run) => {
            return Err(std::io::Error::other(format!(
                "declined stripe-spt payment must not seal success, ran {:?}",
                step_ids(&run.steps)
            ))
            .into());
        }
        Err(error) => {
            return Err(std::io::Error::other(format!("unexpected runtime error: {error}")).into());
        }
    }
    assert_eq!(
        invoked_skills(&invocations),
        vec!["pay-quote", "pay-reserve", "pay-fulfill-rail"],
        "decline is terminal at rail fulfillment and must not mint another spend"
    );
    Ok(())
}

#[test]
fn stripe_spt_payment_timeout_preserves_idempotency_for_recovery()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = StripeSptFixture::new()?;
    let adapter = StripeSptAdapter::new(StripeSptScenario::Timeout);
    let invocations = adapter.invocations();
    let runtime = Runtime::new(adapter, RuntimeOptions::default());
    let mut caller = ApprovalCaller::approved(true);

    let result = runtime.run_graph_file_with_caller(fixture.graph_path(), &mut caller);

    match result {
        Err(RuntimeError::SkillFailed {
            skill_name,
            message,
        }) => {
            assert_eq!(skill_name, "fulfill");
            assert!(message.contains("stripe-spt timeout"));
            assert!(message.contains(STRIPE_SPT_IDEMPOTENCY_KEY));
        }
        Ok(run) => {
            return Err(std::io::Error::other(format!(
                "timed-out stripe-spt payment must not seal success, ran {:?}",
                step_ids(&run.steps)
            ))
            .into());
        }
        Err(error) => {
            return Err(std::io::Error::other(format!("unexpected runtime error: {error}")).into());
        }
    }

    let fulfill_inputs = invocations
        .borrow()
        .iter()
        .find(|invocation| invocation.skill_name == "pay-fulfill-rail")
        .cloned()
        .ok_or_else(|| std::io::Error::other("missing stripe-spt fulfill invocation"))?
        .inputs;
    assert_eq!(
        nested_string(&fulfill_inputs, &["idempotency", "key"]),
        Some(STRIPE_SPT_IDEMPOTENCY_KEY),
        "timeout recovery must keep the original idempotency key"
    );
    Ok(())
}

#[derive(Clone, Copy)]
enum StripeSptScenario {
    Fulfilled,
    Declined,
    Timeout,
}

#[derive(Clone, Debug)]
struct StripeSptInvocation {
    skill_name: String,
    inputs: JsonObject,
}

struct StripeSptAdapter {
    invocations: Rc<RefCell<Vec<StripeSptInvocation>>>,
    scenario: StripeSptScenario,
}

impl StripeSptAdapter {
    fn new(scenario: StripeSptScenario) -> Self {
        Self {
            invocations: Rc::new(RefCell::new(Vec::new())),
            scenario,
        }
    }

    fn invocations(&self) -> Rc<RefCell<Vec<StripeSptInvocation>>> {
        Rc::clone(&self.invocations)
    }
}

impl SkillAdapter for StripeSptAdapter {
    fn adapter_type(&self) -> &'static str {
        "stripe-spt-test"
    }

    fn invoke(&self, request: SkillInvocation) -> Result<SkillOutput, RuntimeError> {
        self.invocations.borrow_mut().push(StripeSptInvocation {
            skill_name: request.skill_name.clone(),
            inputs: request.inputs.clone(),
        });
        Ok(match request.skill_name.as_str() {
            "pay-quote" => skill_success(json!({
                "payment_quote_packet": {
                    "data": {
                        "payment_signal": {
                            "signal_type": "payment_required",
                            "challenge_id": "ch_stripe_spt_001",
                            "amount_minor": 125,
                            "currency": "USD",
                            "rail": "stripe-spt",
                            "counterparty": "merchant:stripe-demo",
                            "operation": "search.paid"
                        },
                        "payment_quote": {
                            "quote_id": "quote_stripe_spt_001",
                            "amount_minor": 125,
                            "currency": "USD",
                            "rails": ["stripe-spt"],
                            "counterparty": "merchant:stripe-demo",
                            "operation": "search.paid"
                        }
                    }
                }
            })),
            "pay-reserve" => skill_success(json!({
                "payment_reservation_packet": {
                    "data": {
                        "payment_decision": stripe_spt_reservation_decision(),
                        "reserved_payment_authority": stripe_spt_reserved_payment_authority(),
                        "spend_capability_ref": stripe_spt_spend_capability_ref(),
                        "idempotency": { "key": STRIPE_SPT_IDEMPOTENCY_KEY }
                    }
                }
            })),
            "pay-fulfill-rail" => stripe_spt_fulfill_output(self.scenario),
            other => skill_failure(&format!("unexpected skill {other}")),
        })
    }
}

fn stripe_spt_fulfill_output(scenario: StripeSptScenario) -> SkillOutput {
    match scenario {
        StripeSptScenario::Fulfilled => skill_success(stripe_spt_rail_packet(
            "fulfilled",
            Some(json!({
                "proof_ref": STRIPE_SPT_PROOF_REF,
                "idempotency_key": STRIPE_SPT_IDEMPOTENCY_KEY,
                "provider_event_ref": "stripe:event:evt_test_succeeded_001",
                "rail_session_material_ref": STRIPE_SPT_SESSION_MATERIAL_REF
            })),
            Some(json!({
                "form": "paid_tool_credential",
                "credential_ref": STRIPE_SPT_CREDENTIAL_REF
            })),
            json!({ "status": "sealed" }),
        )),
        StripeSptScenario::Declined => skill_failure_with_stdout(
            stripe_spt_rail_packet(
                "declined",
                None,
                None,
                json!({
                    "status": "terminal_decline",
                    "idempotency_key": STRIPE_SPT_IDEMPOTENCY_KEY
                }),
            ),
            &format!("stripe-spt declined payment for {STRIPE_SPT_IDEMPOTENCY_KEY}"),
        ),
        StripeSptScenario::Timeout => skill_failure_with_stdout(
            stripe_spt_rail_packet(
                "pending",
                None,
                None,
                json!({
                    "status": "recoverable_timeout",
                    "idempotency_key": STRIPE_SPT_IDEMPOTENCY_KEY,
                    "next_action": "recover_by_idempotency_key"
                }),
            ),
            &format!("stripe-spt timeout before terminal proof for {STRIPE_SPT_IDEMPOTENCY_KEY}"),
        ),
    }
}

fn stripe_spt_rail_packet(
    status: &str,
    rail_proof: Option<Value>,
    credential_envelope: Option<Value>,
    recovery_hint: Value,
) -> Value {
    let mut data = json!({
        "rail_result": {
            "status": status,
            "rail": "stripe-spt",
            "amount_minor": 125,
            "currency": "USD",
            "counterparty": "merchant:stripe-demo",
            "operation": "search.paid",
            "provider_intent_ref": "stripe:payment_intent:pi_test_demo_search_001"
        },
        "redactions": [
            "stripe_client_secret",
            "stripe_api_key",
            "stripe_webhook_secret",
            "card_number",
            "rail_session_material"
        ],
        "recovery_hint": recovery_hint
    });
    if let Some(rail_proof) = rail_proof {
        data["rail_proof"] = rail_proof;
    }
    if let Some(credential_envelope) = credential_envelope {
        data["credential_envelope"] = credential_envelope;
    }
    json!({ "payment_rail_packet": { "data": data } })
}

fn skill_success(value: Value) -> SkillOutput {
    SkillOutput {
        status: InvocationStatus::Success,
        stdout: serde_json::to_string(&value).expect("test JSON should serialize"),
        stderr: String::new(),
        exit_code: Some(0),
        duration_ms: 1,
        metadata: JsonObject::new(),
    }
}

fn skill_failure(message: &str) -> SkillOutput {
    SkillOutput {
        status: InvocationStatus::Failure,
        stdout: String::new(),
        stderr: message.to_owned(),
        exit_code: Some(1),
        duration_ms: 1,
        metadata: JsonObject::new(),
    }
}

fn skill_failure_with_stdout(value: Value, message: &str) -> SkillOutput {
    SkillOutput {
        status: InvocationStatus::Failure,
        stdout: serde_json::to_string(&value).expect("test JSON should serialize"),
        stderr: message.to_owned(),
        exit_code: Some(1),
        duration_ms: 1,
        metadata: JsonObject::new(),
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
    fn report(&mut self, _event: runx_contracts::ExecutionEvent) -> Result<(), RuntimeError> {
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

struct StripeSptFixture {
    _temp: TempDir,
    graph_path: PathBuf,
}

impl StripeSptFixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        write_cli_tool_skill(&temp.path().join("quote"), "pay-quote")?;
        write_cli_tool_skill(&temp.path().join("reserve"), "pay-reserve")?;
        write_cli_tool_skill(&temp.path().join("fulfill"), "pay-fulfill-rail")?;
        let graph_path = temp.path().join("graph.yaml");
        fs::write(&graph_path, stripe_spt_graph_yaml()?)?;
        Ok(Self {
            _temp: temp,
            graph_path,
        })
    }

    fn graph_path(&self) -> &Path {
        self.graph_path.as_path()
    }
}

fn write_cli_tool_skill(dir: &Path, name: &str) -> Result<(), std::io::Error> {
    fs::create_dir(dir)?;
    fs::write(
        dir.join("SKILL.md"),
        format!(
            r#"---
name: {name}
description: Stripe SPT fixture skill.
source:
  type: cli-tool
  command: runx-payment-test
---

Stripe SPT fixture skill.
"#
        ),
    )
}

fn stripe_spt_graph_yaml() -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&json!({
        "name": "stripe-spt-payment",
        "steps": [
            {
                "id": "quote",
                "skill": "./quote",
                "inputs": {
                    "payment_signal": {
                        "signal_type": "payment_required",
                        "challenge_id": "ch_stripe_spt_001",
                        "amount_minor": 125,
                        "currency": "USD",
                        "rail": "stripe-spt",
                        "counterparty": "merchant:stripe-demo",
                        "operation": "search.paid"
                    }
                }
            },
            {
                "id": "reserve",
                "skill": "./reserve",
                "context": {
                    "payment_quote_packet": "quote.payment_quote_packet.data"
                }
            },
            {
                "id": "approve-spend",
                "run": { "type": "approval" },
                "inputs": {
                    "gate_id": "stripe-spt.spend.approval",
                    "gate_type": "payment",
                    "reason": "Approve Stripe SPT settlement before rail execution.",
                    "amount_minor": 125,
                    "currency": "USD"
                },
                "artifacts": { "wrap_as": "payment_approval" }
            },
            {
                "id": "fulfill",
                "skill": "./fulfill",
                "scopes": ["payment:spend"],
                "mutation": true,
                "idempotency_key": "stripe-spt-fulfill",
                "context": {
                    "reserved_payment_authority": "reserve.payment_reservation_packet.data.reserved_payment_authority",
                    "spend_capability_ref": "reserve.payment_reservation_packet.data.spend_capability_ref",
                    "idempotency": "reserve.payment_reservation_packet.data.idempotency",
                    "quote_packet": "quote.payment_quote_packet.data"
                },
                "inputs": {
                    "payment_challenge": {
                        "signal_type": "payment_required",
                        "challenge_id": "ch_stripe_spt_001",
                        "amount_minor": 125,
                        "currency": "USD",
                        "rail": "stripe-spt",
                        "counterparty": "merchant:stripe-demo",
                        "operation": "search.paid"
                    },
                    "rail_profile_ref": "rail-profile:stripe-spt:test"
                }
            }
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

fn stripe_spt_reserved_payment_authority() -> Value {
    json!({
        "parent_authority": stripe_spt_payment_term("stripe-spt-parent", ["quote", "reserve", "spend", "verify"], 10_000),
        "child_authority": stripe_spt_payment_term("stripe-spt-child", ["reserve", "spend"], 2_500),
        "reservation_decision": stripe_spt_reservation_decision(),
        "subset_proof_present": true,
        "child_harness_ref": stripe_spt_child_harness_ref(),
        "spend_capability_binding": {
            "child_harness_ref": stripe_spt_child_harness_ref(),
            "act_id": "act_fulfill",
            "reservation_decision_id": "decision_stripe_spt_reservation",
            "idempotency_key": STRIPE_SPT_IDEMPOTENCY_KEY,
            "amount_minor": 125,
            "currency": "USD",
            "counterparty": "merchant:stripe-demo",
            "rail": "stripe-spt"
        },
        "consumed_spend_capability_refs": []
    })
}

fn stripe_spt_payment_term<const N: usize>(
    term_id: &str,
    verbs: [&str; N],
    max_per_call_minor: u64,
) -> Value {
    let verbs = verbs.as_slice();
    json!({
        "term_id": term_id,
        "principal_ref": reference("principal", "runx:principal:stripe-spt-agent"),
        "resource_ref": reference("grant", "runx:payment-grant:stripe-spt"),
        "resource_family": "payment",
        "verbs": verbs,
        "bounds": {
            "payment": {
                "currency": "USD",
                "max_per_call_minor": max_per_call_minor,
                "max_per_run_minor": 25_000,
                "rails": ["stripe-spt"],
                "counterparty": "merchant:stripe-demo",
                "operation": "search.paid",
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
        "issued_by_ref": reference("grant", "runx:grant:stripe-spt-issuer"),
        "credential_ref": reference("credential", "runx:credential:stripe-spt-session")
    })
}

fn stripe_spt_reservation_decision() -> Value {
    json!({
        "decision_id": "decision_stripe_spt_reservation",
        "choice": "continue",
        "inputs": {
            "signal_refs": [],
            "target_ref": null,
            "opportunity_refs": [],
            "selection_ref": null
        },
        "proposed_intent": {
            "purpose": "complete a bounded Stripe SPT payment",
            "legitimacy": "authorized by selected reservation decision",
            "success_criteria": [],
            "constraints": [],
            "derived_from": []
        },
        "selected_act_id": "act_fulfill",
        "selected_harness_ref": null,
        "justification": {
            "summary": "reservation selected a bounded Stripe SPT spend act",
            "evidence_refs": []
        },
        "closure": null,
        "artifact_refs": []
    })
}

fn stripe_spt_child_harness_ref() -> Value {
    reference("harness", "runx:harness:stripe-spt-payment_fulfill")
}

fn stripe_spt_spend_capability_ref() -> Value {
    reference("credential", "runx:payment-capability:stripe-spt-spend-1")
}

fn reference(reference_type: &str, uri: &str) -> Value {
    json!({ "type": reference_type, "uri": uri })
}

fn invoked_skills(invocations: &Rc<RefCell<Vec<StripeSptInvocation>>>) -> Vec<String> {
    invocations
        .borrow()
        .iter()
        .map(|invocation| invocation.skill_name.clone())
        .collect()
}

fn nested_string<'a>(object: &'a JsonObject, path: &[&str]) -> Option<&'a str> {
    let mut value = object.get(*path.first()?)?;
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
