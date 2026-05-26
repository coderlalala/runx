use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use runx_contracts::JsonValue;
use runx_runtime::{
    LocalOrchestrator, LocalReceiptStore, RUNX_RECEIPT_DIR_ENV,
    RUNX_RECEIPT_SIGN_ED25519_SEED_BASE64_ENV, RUNX_RECEIPT_SIGN_KID_ENV, RunResult,
    RuntimeOptions, RuntimeReceiptSignatureConfig, SkillRunRequest,
};
use tempfile::tempdir;

const FIXTURE_CREATED_AT: &str = "2026-05-18T00:00:00Z";

#[test]
fn runtime_options_default_uses_live_timestamp() {
    let options = RuntimeOptions::default();

    assert_ne!(options.created_at, FIXTURE_CREATED_AT);
    assert!(options.created_at.ends_with('Z'));
    assert!(options.created_at.contains('T'));
}

#[test]
fn native_skill_run_pauses_with_agent_act_request() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let skill_dir = write_agent_step_skill(temp.path())?;
    let result = run_skill(SkillRunRequest {
        skill_path: skill_dir,
        receipt_dir: None,
        run_id: None,
        answers_path: None,
        inputs: [(
            "thread_title".to_owned(),
            JsonValue::String("Docs bug".to_owned()),
        )]
        .into_iter()
        .collect(),
        env: BTreeMap::new(),
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    })?;

    let output = object(&result.output, "skill run result")?;
    assert_eq!(string_field(output, "schema"), Some("runx.skill_run.v1"));
    assert_eq!(string_field(output, "status"), Some("needs_agent"));
    assert_eq!(
        string_field(output, "run_id"),
        Some("run_agent_step-issue-intake-output")
    );
    let requests = array_field(output, "requests").ok_or("missing requests")?;
    assert_eq!(requests.len(), 1);
    let request = object(&requests[0], "request")?;
    assert_eq!(string_field(request, "kind"), Some("agent_act"));
    assert_eq!(
        string_field(request, "id"),
        Some("agent_step.issue-intake.output")
    );
    let invocation = object_field(request, "invocation").ok_or("missing invocation")?;
    assert_eq!(string_field(invocation, "source_type"), Some("agent-step"));
    let envelope = object_field(invocation, "envelope").ok_or("missing envelope")?;
    let inputs = object_field(envelope, "inputs").ok_or("missing inputs")?;
    assert_eq!(
        inputs.get("thread_title"),
        Some(&JsonValue::String("Docs bug".to_owned()))
    );
    assert!(
        object_field(envelope, "execution_location")
            .and_then(|location| string_field(location, "skill_directory"))
            .is_some()
    );

    Ok(())
}

#[test]
fn native_skill_run_resumes_and_seals_receipt() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let skill_dir = write_agent_step_skill(temp.path())?;
    let receipt_dir = temp.path().join("receipts");
    let answers_path = temp.path().join("answers.json");
    fs::write(
        &answers_path,
        serde_json::json!({
            "answers": {
                "agent_step.issue-intake.output": {
                    "intake_report": {
                        "summary": "Docs bug is bounded."
                    },
                    "closure": {
                        "disposition": "declined"
                    }
                }
            }
        })
        .to_string(),
    )?;

    let result = run_skill(SkillRunRequest {
        skill_path: skill_dir,
        receipt_dir: Some(receipt_dir.clone()),
        run_id: Some("issue-intake-run".to_owned()),
        answers_path: Some(answers_path),
        inputs: BTreeMap::new(),
        env: BTreeMap::new(),
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    })?;

    let output = object(&result.output, "skill run result")?;
    assert_eq!(string_field(output, "status"), Some("sealed"));
    assert_eq!(string_field(output, "run_id"), Some("issue-intake-run"));
    let closure = object_field(output, "closure").ok_or("missing closure")?;
    assert_eq!(string_field(closure, "disposition"), Some("declined"));
    let receipt_id = string_field(output, "receipt_id").ok_or("missing receipt_id")?;
    // Receipt ids are content-addressed (`id = hash(canonical_body)`).
    assert!(receipt_id.starts_with("sha256:"));
    assert!(receipt_dir.join(format!("{receipt_id}.json")).exists());

    let receipt = LocalReceiptStore::new(&receipt_dir).read_exact(receipt_id)?;
    assert_ne!(receipt.created_at, FIXTURE_CREATED_AT);
    assert_eq!(
        serde_json::to_value(&receipt.schema)?,
        serde_json::json!("runx.receipt.v1")
    );
    assert_eq!(serde_json::to_value(&receipt.seal.disposition)?, "declined");
    assert_eq!(receipt.acts.len(), 1);
    assert_eq!(
        serde_json::to_value(&receipt.acts[0].criterion_bindings[0].status)?,
        "failed"
    );

    let payload = object_field(output, "payload").ok_or("missing payload")?;
    assert!(object_field(payload, "intake_report").is_some());

    Ok(())
}

#[test]
fn native_skill_run_treats_structured_stdout_as_claim_not_receipt_proof()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let skill_dir = write_agent_step_skill(temp.path())?;
    let receipt_dir = temp.path().join("receipts");
    let answers_path = temp.path().join("answers.json");
    fs::write(
        &answers_path,
        serde_json::json!({
            "answers": {
                "agent_step.issue-intake.output": {
                    "intake_report": {
                        "summary": "Malicious proof refs stay claim-scoped."
                    },
                    "rail_proof": {
                        "proof_ref": "receipt-proof:evil:stdout",
                        "idempotency_key": "payment:evil:stdout"
                    },
                    "verification": {
                        "verification_id": "stdout-verification"
                    },
                    "signal": {
                        "signal_id": "stdout-signal",
                        "source_events": [
                            {
                                "provider": "github",
                                "source_locator": "https://example.invalid/evil",
                                "title": "Injected source"
                            }
                        ]
                    }
                }
            }
        })
        .to_string(),
    )?;

    let result = run_skill(SkillRunRequest {
        skill_path: skill_dir,
        receipt_dir: Some(receipt_dir.clone()),
        run_id: Some("malicious-stdout-run".to_owned()),
        answers_path: Some(answers_path),
        inputs: BTreeMap::new(),
        env: BTreeMap::new(),
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    })?;

    let output = object(&result.output, "skill run result")?;
    let execution = object_field(output, "execution").ok_or("missing execution")?;
    assert!(object_field(execution, "skill_claim").is_some());
    let receipt_id = string_field(output, "receipt_id").ok_or("missing receipt_id")?;
    let receipt = LocalReceiptStore::new(&receipt_dir).read_exact(receipt_id)?;
    let refs = receipt.acts[0]
        .criterion_bindings
        .iter()
        .flat_map(|criterion| {
            criterion
                .verification_refs
                .iter()
                .chain(criterion.evidence_refs.iter())
        })
        .collect::<Vec<_>>();
    assert!(
        refs.iter().all(|reference| {
            reference.uri != "receipt-proof:evil:stdout"
                && reference.uri != "runx:verification:stdout-verification"
                && reference.uri != "https://example.invalid/evil"
        }),
        "stdout claim refs must not be promoted into receipt proof refs"
    );

    Ok(())
}

#[test]
fn native_skill_run_preserves_deferred_closure_disposition()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let skill_dir = write_agent_step_skill(temp.path())?;
    let receipt_dir = temp.path().join("receipts");
    let answers_path = temp.path().join("answers.json");
    fs::write(
        &answers_path,
        serde_json::json!({
            "answers": {
                "agent_step.issue-intake.output": {
                    "intake_report": {
                        "summary": "Docs bug needs more context."
                    },
                    "closure": {
                        "disposition": "deferred"
                    }
                }
            }
        })
        .to_string(),
    )?;

    let result = run_skill(SkillRunRequest {
        skill_path: skill_dir,
        receipt_dir: Some(receipt_dir.clone()),
        run_id: Some("issue-intake-deferred".to_owned()),
        answers_path: Some(answers_path),
        inputs: BTreeMap::new(),
        env: BTreeMap::new(),
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    })?;

    let output = object(&result.output, "skill run result")?;
    assert_eq!(string_field(output, "status"), Some("sealed"));
    let closure = object_field(output, "closure").ok_or("missing closure")?;
    assert_eq!(string_field(closure, "disposition"), Some("deferred"));
    let execution = object_field(output, "execution").ok_or("missing execution")?;
    assert_eq!(execution.get("exit_code"), Some(&JsonValue::Null));
    let receipt_id = string_field(output, "receipt_id").ok_or("missing receipt_id")?;
    let receipt = LocalReceiptStore::new(&receipt_dir).read_exact(receipt_id)?;
    assert_eq!(serde_json::to_value(&receipt.seal.disposition)?, "deferred");

    Ok(())
}

#[test]
fn native_skill_run_uses_runtime_receipt_path_resolution() -> Result<(), Box<dyn std::error::Error>>
{
    let temp = tempdir()?;
    let skill_dir = write_agent_step_skill(temp.path())?;
    let env_receipt_dir = temp.path().join("env-receipts");
    let answers_path = temp.path().join("answers.json");
    fs::write(
        &answers_path,
        serde_json::json!({
            "answers": {
                "agent_step.issue-intake.output": {
                    "intake_report": {
                        "summary": "Docs bug is bounded."
                    }
                }
            }
        })
        .to_string(),
    )?;

    let result = run_skill(SkillRunRequest {
        skill_path: skill_dir,
        receipt_dir: None,
        run_id: Some("env-receipt-run".to_owned()),
        answers_path: Some(answers_path),
        inputs: BTreeMap::new(),
        env: [(
            RUNX_RECEIPT_DIR_ENV.to_owned(),
            env_receipt_dir.to_string_lossy().into_owned(),
        )]
        .into_iter()
        .collect(),
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    })?;

    let output = object(&result.output, "skill run result")?;
    let receipt_id = string_field(output, "receipt_id").ok_or("missing receipt_id")?;
    assert!(env_receipt_dir.join(format!("{receipt_id}.json")).exists());

    Ok(())
}

#[test]
fn native_skill_run_uses_production_receipt_signing_env() -> Result<(), Box<dyn std::error::Error>>
{
    let temp = tempdir()?;
    let skill_dir = write_agent_step_skill(temp.path())?;
    let receipt_dir = temp.path().join("receipts");
    let answers_path = temp.path().join("answers.json");
    fs::write(
        &answers_path,
        serde_json::json!({
            "answers": {
                "agent_step.issue-intake.output": {
                    "intake_report": {
                        "summary": "Docs bug is bounded."
                    }
                }
            }
        })
        .to_string(),
    )?;
    let env = [
        (
            RUNX_RECEIPT_SIGN_KID_ENV.to_owned(),
            "runx-runtime-prod-fixture-key".to_owned(),
        ),
        (
            RUNX_RECEIPT_SIGN_ED25519_SEED_BASE64_ENV.to_owned(),
            "QkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkI=".to_owned(),
        ),
    ]
    .into_iter()
    .collect::<BTreeMap<_, _>>();

    let result = run_skill(SkillRunRequest {
        skill_path: skill_dir,
        receipt_dir: Some(receipt_dir.clone()),
        run_id: Some("production-signed-run".to_owned()),
        answers_path: Some(answers_path),
        inputs: BTreeMap::new(),
        env: env.clone(),
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    })?;

    let output = object(&result.output, "skill run result")?;
    let receipt_id = string_field(output, "receipt_id").ok_or("missing receipt_id")?;
    let signature_config = RuntimeReceiptSignatureConfig::from_env(&env)?;
    let receipt = LocalReceiptStore::new(&receipt_dir)
        .read_exact_with_policy(receipt_id, signature_config.signature_policy())?;
    assert_eq!(receipt.issuer.kid, "runx-runtime-prod-fixture-key");
    assert!(receipt.signature.value.starts_with("base64:"));
    assert!(!receipt.signature.value.starts_with("sig:"));

    Ok(())
}

#[test]
fn native_graph_skill_run_pauses_and_resumes_agent_step() -> Result<(), Box<dyn std::error::Error>>
{
    let temp = tempdir()?;
    let skill_dir = write_graph_agent_step_skill(temp.path())?;
    let receipt_dir = temp.path().join("receipts");
    let inputs = [(
        "thread_title".to_owned(),
        JsonValue::String("Graph bug".to_owned()),
    )]
    .into_iter()
    .collect::<BTreeMap<_, _>>();

    let initial = run_skill(SkillRunRequest {
        skill_path: skill_dir.clone(),
        receipt_dir: Some(receipt_dir.clone()),
        run_id: None,
        answers_path: None,
        inputs: inputs.clone(),
        env: BTreeMap::new(),
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    })?;

    let output = object(&initial.output, "graph skill run result")?;
    assert_eq!(string_field(output, "status"), Some("needs_agent"));
    let run_id = string_field(output, "run_id").ok_or("missing run_id")?;
    let requests = array_field(output, "requests").ok_or("missing requests")?;
    assert_eq!(requests.len(), 1);
    let request = object(&requests[0], "request")?;
    assert_eq!(
        string_field(request, "id"),
        Some("agent_step.graph-decide.output")
    );
    let invocation = object_field(request, "invocation").ok_or("missing invocation")?;
    let envelope = object_field(invocation, "envelope").ok_or("missing envelope")?;
    assert_eq!(
        string_field(envelope, "instructions"),
        Some("Use the full issue context.")
    );
    let envelope_inputs = object_field(envelope, "inputs").ok_or("missing inputs")?;
    assert_eq!(
        envelope_inputs.get("thread_title"),
        Some(&JsonValue::String("Graph bug".to_owned()))
    );

    let state_path = receipt_dir
        .join("runs")
        .join(format!("{run_id}.graph-state.json"));
    let original_state = fs::read_to_string(&state_path)?;
    let mut mismatched_state: JsonValue = serde_json::from_str(&original_state)?;
    object_mut(&mut mismatched_state, "graph state")?.insert(
        "runner_name".to_owned(),
        JsonValue::String("other-runner".to_owned()),
    );
    fs::write(
        &state_path,
        serde_json::to_string_pretty(&mismatched_state)?,
    )?;
    let bad_answers_path = temp.path().join("bad-graph-answers.json");
    fs::write(&bad_answers_path, "{}")?;
    let mismatch = match run_skill(SkillRunRequest {
        skill_path: skill_dir.clone(),
        receipt_dir: Some(receipt_dir.clone()),
        run_id: Some(run_id.to_owned()),
        answers_path: Some(bad_answers_path),
        inputs: inputs.clone(),
        env: BTreeMap::new(),
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    }) {
        Ok(_) => return Err("mismatched graph state should fail".into()),
        Err(error) => error,
    };
    assert!(
        mismatch
            .to_string()
            .contains("graph state runner_name mismatch")
    );
    fs::write(&state_path, original_state)?;

    let answers_path = temp.path().join("graph-answers.json");
    fs::write(
        &answers_path,
        serde_json::json!({
            "answers": {
                "agent_step.graph-decide.output": {
                    "approved": true,
                    "proof_ref": "receipt-proof:evil:step-output",
                    "receipt_id": "sha256:evil-step-output",
                    "result": {
                        "summary": "Graph fix authored."
                    }
                }
            }
        })
        .to_string(),
    )?;
    let resumed = run_skill(SkillRunRequest {
        skill_path: skill_dir,
        receipt_dir: Some(receipt_dir),
        run_id: Some(run_id.to_owned()),
        answers_path: Some(answers_path),
        inputs,
        env: BTreeMap::new(),
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    })?;

    let output = object(&resumed.output, "resumed graph skill run result")?;
    assert_eq!(string_field(output, "status"), Some("sealed"));
    let payload = object_field(output, "payload").ok_or("missing payload")?;
    assert!(!payload.contains_key("approved"));
    assert!(!payload.contains_key("proof_ref"));
    assert!(!payload.contains_key("receipt_id"));
    let decide_claim = step_claim(payload, "decide").ok_or("missing decide skill claim")?;
    let result = object_field(decide_claim, "result").ok_or("missing result")?;
    assert_eq!(string_field(result, "summary"), Some("Graph fix authored."));
    let step_outputs = object_field(payload, "step_outputs").ok_or("missing step_outputs")?;
    let decide = object_field(step_outputs, "decide").ok_or("missing decide step output")?;
    assert_eq!(string_field(decide, "status"), Some("success"));
    assert!(object_field(decide, "skill_claim").is_some());
    let declared_result = object_field(decide, "result").ok_or("missing declared result output")?;
    assert_eq!(
        string_field(declared_result, "summary"),
        Some("Graph fix authored.")
    );
    assert!(!decide.contains_key("approved"));
    assert!(!decide.contains_key("proof_ref"));
    assert!(!decide.contains_key("receipt_id"));

    Ok(())
}

#[test]
fn native_graph_transition_gate_rejects_skill_claim_as_fact()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let skill_dir = write_graph_gated_agent_step_skill(temp.path())?;
    let receipt_dir = temp.path().join("receipts");

    let initial = run_skill(SkillRunRequest {
        skill_path: skill_dir.clone(),
        receipt_dir: Some(receipt_dir.clone()),
        run_id: None,
        answers_path: None,
        inputs: BTreeMap::new(),
        env: BTreeMap::new(),
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    })?;
    let output = object(&initial.output, "gated graph result")?;
    let run_id = string_field(output, "run_id").ok_or("missing run_id")?;

    let answers_path = temp.path().join("gated-answers.json");
    fs::write(
        &answers_path,
        serde_json::json!({
            "answers": {
                "agent_step.gated-decide.output": {
                    "approved": true
                }
            }
        })
        .to_string(),
    )?;

    let error = match run_skill(SkillRunRequest {
        skill_path: skill_dir,
        receipt_dir: Some(receipt_dir.clone()),
        run_id: Some(run_id.to_owned()),
        answers_path: Some(answers_path),
        inputs: BTreeMap::new(),
        env: BTreeMap::new(),
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    }) {
        Ok(_) => return Err("skill claim unexpectedly satisfied transition gate".into()),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("transition gate 'decide.approved' is unresolved"),
        "unexpected error: {error}"
    );

    Ok(())
}

#[test]
fn native_graph_skill_run_pauses_and_resumes_nested_agent_skill()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let skill_dir = write_graph_nested_agent_skill(temp.path(), "agent")?;
    let receipt_dir = temp.path().join("receipts");
    let inputs = [(
        "thread_title".to_owned(),
        JsonValue::String("Nested agent bug".to_owned()),
    )]
    .into_iter()
    .collect::<BTreeMap<_, _>>();

    let initial = run_skill(SkillRunRequest {
        skill_path: skill_dir.clone(),
        receipt_dir: Some(receipt_dir.clone()),
        run_id: None,
        answers_path: None,
        inputs: inputs.clone(),
        env: BTreeMap::new(),
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    })?;

    let output = object(&initial.output, "nested agent graph result")?;
    assert_eq!(string_field(output, "status"), Some("needs_agent"));
    let run_id = string_field(output, "run_id").ok_or("missing run_id")?;
    let requests = array_field(output, "requests").ok_or("missing requests")?;
    assert_eq!(requests.len(), 1);
    let request = object(&requests[0], "request")?;
    assert_eq!(
        string_field(request, "id"),
        Some("agent.child-agent.output")
    );
    let invocation = object_field(request, "invocation").ok_or("missing invocation")?;
    assert_eq!(string_field(invocation, "source_type"), Some("agent"));

    let answers_path = temp.path().join("nested-agent-answers.json");
    fs::write(
        &answers_path,
        serde_json::json!({
            "answers": {
                "agent.child-agent.output": {
                    "result": {
                        "summary": "Nested agent fix authored."
                    }
                }
            }
        })
        .to_string(),
    )?;
    let resumed = run_skill(SkillRunRequest {
        skill_path: skill_dir,
        receipt_dir: Some(receipt_dir),
        run_id: Some(run_id.to_owned()),
        answers_path: Some(answers_path),
        inputs,
        env: BTreeMap::new(),
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    })?;

    let output = object(&resumed.output, "resumed nested agent graph result")?;
    assert_eq!(string_field(output, "status"), Some("sealed"));
    let payload = object_field(output, "payload").ok_or("missing payload")?;
    let nested_claim = step_claim(payload, "nested").ok_or("missing nested skill claim")?;
    let result = object_field(nested_claim, "result").ok_or("missing result")?;
    assert_eq!(
        string_field(result, "summary"),
        Some("Nested agent fix authored.")
    );
    let step_outputs = object_field(payload, "step_outputs").ok_or("missing step_outputs")?;
    assert!(object_field(step_outputs, "nested").is_some());

    Ok(())
}

#[test]
fn native_graph_skill_run_pauses_and_resumes_nested_agent_step_skill()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let skill_dir = write_graph_nested_agent_skill(temp.path(), "agent-step")?;
    let receipt_dir = temp.path().join("receipts");

    let initial = run_skill(SkillRunRequest {
        skill_path: skill_dir.clone(),
        receipt_dir: Some(receipt_dir.clone()),
        run_id: None,
        answers_path: None,
        inputs: BTreeMap::new(),
        env: BTreeMap::new(),
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    })?;

    let output = object(&initial.output, "nested agent-step graph result")?;
    assert_eq!(string_field(output, "status"), Some("needs_agent"));
    let run_id = string_field(output, "run_id").ok_or("missing run_id")?;
    let requests = array_field(output, "requests").ok_or("missing requests")?;
    assert_eq!(requests.len(), 1);
    let request = object(&requests[0], "request")?;
    assert_eq!(
        string_field(request, "id"),
        Some("agent_step.child-agent-step.output")
    );
    let invocation = object_field(request, "invocation").ok_or("missing invocation")?;
    assert_eq!(string_field(invocation, "source_type"), Some("agent-step"));

    let answers_path = temp.path().join("nested-agent-step-answers.json");
    fs::write(
        &answers_path,
        serde_json::json!({
            "answers": {
                "agent_step.child-agent-step.output": {
                    "result": {
                        "summary": "Nested agent-step fix authored."
                    }
                }
            }
        })
        .to_string(),
    )?;
    let resumed = run_skill(SkillRunRequest {
        skill_path: skill_dir,
        receipt_dir: Some(receipt_dir),
        run_id: Some(run_id.to_owned()),
        answers_path: Some(answers_path),
        inputs: BTreeMap::new(),
        env: BTreeMap::new(),
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    })?;

    let output = object(&resumed.output, "resumed nested agent-step graph result")?;
    assert_eq!(string_field(output, "status"), Some("sealed"));
    let payload = object_field(output, "payload").ok_or("missing payload")?;
    let nested_claim = step_claim(payload, "nested").ok_or("missing nested skill claim")?;
    let result = object_field(nested_claim, "result").ok_or("missing result")?;
    assert_eq!(
        string_field(result, "summary"),
        Some("Nested agent-step fix authored.")
    );

    Ok(())
}

#[cfg(feature = "catalog")]
#[test]
fn native_graph_skill_run_executes_local_tool_step() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let skill_dir = write_graph_tool_skill(temp.path())?;
    write_echo_tool(temp.path())?;
    let receipt_dir = temp.path().join("receipts");
    let tool_root = temp.path().join("tools");
    let inputs = [(
        "thread_title".to_owned(),
        JsonValue::String("Graph tool bug".to_owned()),
    )]
    .into_iter()
    .collect::<BTreeMap<_, _>>();
    let env = [(
        "RUNX_TOOL_ROOTS".to_owned(),
        tool_root.to_string_lossy().into_owned(),
    )]
    .into_iter()
    .collect::<BTreeMap<_, _>>();

    let result = run_skill(SkillRunRequest {
        skill_path: skill_dir,
        receipt_dir: Some(receipt_dir),
        run_id: None,
        answers_path: None,
        inputs,
        env,
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    })?;

    let output = object(&result.output, "graph tool result")?;
    assert_eq!(string_field(output, "status"), Some("sealed"));
    let payload = object_field(output, "payload").ok_or("missing payload")?;
    let echo_claim = step_claim(payload, "echo").ok_or("missing echo skill claim")?;
    let echo = object_field(echo_claim, "echo").ok_or("missing echo")?;
    assert_eq!(string_field(echo, "message"), Some("Graph tool bug"));

    Ok(())
}

#[cfg(feature = "catalog")]
#[test]
fn native_graph_skill_run_resolves_agent_step_named_emit_context()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let skill_dir = write_graph_agent_artifact_context_skill(temp.path())?;
    write_echo_tool(temp.path())?;
    let receipt_dir = temp.path().join("receipts");
    let tool_root = temp.path().join("tools");
    let answers_path = temp.path().join("answers.json");
    fs::write(
        &answers_path,
        serde_json::json!({
            "answers": {
                "agent_step.graph-author.output": {
                    "fix_bundle": {
                        "message": "Graph tool bug"
                    }
                }
            }
        })
        .to_string(),
    )?;
    let env = [(
        "RUNX_TOOL_ROOTS".to_owned(),
        tool_root.to_string_lossy().into_owned(),
    )]
    .into_iter()
    .collect::<BTreeMap<_, _>>();
    let pending = run_skill(SkillRunRequest {
        skill_path: skill_dir.clone(),
        receipt_dir: Some(receipt_dir.clone()),
        run_id: None,
        answers_path: None,
        inputs: BTreeMap::new(),
        env: env.clone(),
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    })?;
    let pending_output = object(&pending.output, "pending graph agent artifact result")?;
    assert_eq!(string_field(pending_output, "status"), Some("needs_agent"));
    let run_id = string_field(pending_output, "run_id")
        .ok_or("pending graph agent artifact result missing run_id")?
        .to_owned();

    let result = run_skill(SkillRunRequest {
        skill_path: skill_dir,
        receipt_dir: Some(receipt_dir),
        run_id: Some(run_id),
        answers_path: Some(answers_path),
        inputs: BTreeMap::new(),
        env,
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    })?;

    let output = object(&result.output, "graph agent artifact result")?;
    assert_eq!(string_field(output, "status"), Some("sealed"));
    let payload = object_field(output, "payload").ok_or("missing payload")?;
    let echo_claim = step_claim(payload, "echo").ok_or("missing echo skill claim")?;
    let echo = object_field(echo_claim, "echo").ok_or("missing echo")?;
    assert_eq!(string_field(echo, "message"), Some("Graph tool bug"));

    Ok(())
}

#[cfg(feature = "catalog")]
#[test]
fn native_graph_skill_run_omits_missing_optional_graph_input_references()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let skill_dir = write_graph_optional_json_tool_skill(temp.path())?;
    write_optional_json_tool(temp.path())?;
    let receipt_dir = temp.path().join("receipts");
    let tool_root = temp.path().join("tools");
    let inputs = [(
        "thread_title".to_owned(),
        JsonValue::String("Graph optional JSON bug".to_owned()),
    )]
    .into_iter()
    .collect::<BTreeMap<_, _>>();
    let env = [(
        "RUNX_TOOL_ROOTS".to_owned(),
        tool_root.to_string_lossy().into_owned(),
    )]
    .into_iter()
    .collect::<BTreeMap<_, _>>();

    let result = run_skill(SkillRunRequest {
        skill_path: skill_dir,
        receipt_dir: Some(receipt_dir),
        run_id: None,
        answers_path: None,
        inputs,
        env,
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    })?;

    let output = object(&result.output, "graph optional JSON tool result")?;
    assert_eq!(string_field(output, "status"), Some("sealed"));
    let payload = object_field(output, "payload").ok_or("missing payload")?;
    let echo_claim = step_claim(payload, "echo").ok_or("missing echo skill claim")?;
    let echo = object_field(echo_claim, "echo").ok_or("missing echo")?;
    assert_eq!(
        string_field(echo, "message"),
        Some("Graph optional JSON bug")
    );

    Ok(())
}

#[cfg(feature = "catalog")]
#[test]
fn native_graph_skill_run_prefers_built_cli_tool_root() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let skill_dir = write_graph_tool_skill_under_skills(temp.path())?;
    write_echo_tool_at(&temp.path().join("tools/test/echo"), "root tools")?;
    write_echo_tool_at(
        &temp.path().join("packages/cli/tools/test/echo"),
        "package cli tools",
    )?;
    let receipt_dir = temp.path().join("receipts");
    let inputs = [(
        "thread_title".to_owned(),
        JsonValue::String("Graph tool bug".to_owned()),
    )]
    .into_iter()
    .collect::<BTreeMap<_, _>>();

    let result = run_skill(SkillRunRequest {
        skill_path: skill_dir,
        receipt_dir: Some(receipt_dir),
        run_id: None,
        answers_path: None,
        inputs,
        env: BTreeMap::new(),
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    })?;

    let output = object(&result.output, "graph tool result")?;
    assert_eq!(string_field(output, "status"), Some("sealed"));
    let payload = object_field(output, "payload").ok_or("missing payload")?;
    let echo_claim = step_claim(payload, "echo").ok_or("missing echo skill claim")?;
    let echo = object_field(echo_claim, "echo").ok_or("missing echo")?;
    assert_eq!(string_field(echo, "message"), Some("package cli tools"));

    Ok(())
}

#[cfg(feature = "cli-tool")]
#[test]
fn native_graph_skill_run_executes_nested_cli_tool_skill() -> Result<(), Box<dyn std::error::Error>>
{
    let temp = tempdir()?;
    let skill_dir = write_graph_nested_cli_skill(temp.path())?;
    let receipt_dir = temp.path().join("receipts");
    let inputs = [(
        "thread_title".to_owned(),
        JsonValue::String("Nested graph bug".to_owned()),
    )]
    .into_iter()
    .collect::<BTreeMap<_, _>>();

    let result = run_skill(SkillRunRequest {
        skill_path: skill_dir,
        receipt_dir: Some(receipt_dir),
        run_id: None,
        answers_path: None,
        inputs,
        env: BTreeMap::new(),
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    })?;

    let output = object(&result.output, "nested graph skill result")?;
    assert_eq!(string_field(output, "status"), Some("sealed"));
    let payload = object_field(output, "payload").ok_or("missing payload")?;
    let nested_claim = step_claim(payload, "nested").ok_or("missing nested skill claim")?;
    let nested = object_field(nested_claim, "nested").ok_or("missing nested output")?;
    assert_eq!(string_field(nested, "message"), Some("Nested graph bug"));
    let step_outputs = object_field(payload, "step_outputs").ok_or("missing step outputs")?;
    let nested_step = object_field(step_outputs, "nested").ok_or("missing nested step output")?;
    let declared_nested = object_field(nested_step, "nested").ok_or("missing exposed nested output")?;
    assert_eq!(
        string_field(declared_nested, "message"),
        Some("Nested graph bug")
    );

    Ok(())
}

#[cfg(feature = "cli-tool")]
#[test]
fn native_graph_skill_run_executes_nested_x_yaml_runner_skill()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let skill_dir = write_graph_nested_x_yaml_cli_skill(temp.path())?;
    let receipt_dir = temp.path().join("receipts");
    let inputs = [(
        "thread_title".to_owned(),
        JsonValue::String("Runner manifest bug".to_owned()),
    )]
    .into_iter()
    .collect::<BTreeMap<_, _>>();

    let result = run_skill(SkillRunRequest {
        skill_path: skill_dir,
        receipt_dir: Some(receipt_dir),
        run_id: None,
        answers_path: None,
        inputs,
        env: BTreeMap::new(),
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    })?;

    let output = object(&result.output, "nested X.yaml graph skill result")?;
    assert_eq!(string_field(output, "status"), Some("sealed"));
    let payload = object_field(output, "payload").ok_or("missing payload")?;
    let nested_claim = step_claim(payload, "nested").ok_or("missing nested skill claim")?;
    let nested = object_field(nested_claim, "nested").ok_or("missing nested output")?;
    assert_eq!(string_field(nested, "message"), Some("Runner manifest bug"));

    Ok(())
}

#[test]
fn native_skill_run_rejects_partial_continuation_shape() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let skill_dir = write_agent_step_skill(temp.path())?;

    let run_id_only = match run_skill(SkillRunRequest {
        skill_path: skill_dir.clone(),
        receipt_dir: None,
        run_id: Some("issue-intake-run".to_owned()),
        answers_path: None,
        inputs: BTreeMap::new(),
        env: BTreeMap::new(),
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    }) {
        Ok(_) => return Err("run-id without answers should fail".into()),
        Err(error) => error,
    };
    assert!(
        run_id_only
            .to_string()
            .contains("runx skill --run-id requires --answers")
    );

    let answers_only = match run_skill(SkillRunRequest {
        skill_path: skill_dir,
        receipt_dir: None,
        run_id: None,
        answers_path: Some(temp.path().join("answers.json")),
        inputs: BTreeMap::new(),
        env: BTreeMap::new(),
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    }) {
        Ok(_) => return Err("answers without run-id should fail".into()),
        Err(error) => error,
    };
    assert!(
        answers_only
            .to_string()
            .contains("runx skill --answers requires --run-id")
    );

    Ok(())
}

fn run_skill(request: SkillRunRequest) -> Result<RunResult, Box<dyn std::error::Error>> {
    LocalOrchestrator
        .run_skill(&request)
        .map_err(|error| error.into())
}

fn write_agent_step_skill(root: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let skill_dir = root.join("issue-intake");
    fs::create_dir_all(&skill_dir)?;
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: issue-intake\n---\n# Issue Intake\n",
    )?;
    fs::write(
        skill_dir.join("X.yaml"),
        r#"
skill: issue-intake
runners:
  intake:
    default: true
    type: agent-step
    agent: builder
    task: issue-intake
    outputs:
      intake_report: object
    inputs:
      thread_title:
        type: string
        required: false
"#,
    )?;
    Ok(skill_dir.to_path_buf())
}

fn write_graph_agent_step_skill(root: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let skill_dir = root.join("graph-issue-to-pr");
    fs::create_dir_all(&skill_dir)?;
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: graph-issue-to-pr\n---\n# Graph Issue To PR\n",
    )?;
    fs::write(
        skill_dir.join("X.yaml"),
        r#"
skill: graph-issue-to-pr
runners:
  graph:
    default: true
    type: graph
    graph:
      name: graph-issue-to-pr
      steps:
        - id: decide
          run:
            type: agent-step
            agent: builder
            task: graph-decide
            outputs:
              result: object
          instructions: Use the full issue context.
"#,
    )?;
    Ok(skill_dir.to_path_buf())
}

fn write_graph_gated_agent_step_skill(root: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let skill_dir = root.join("graph-gated-agent-step");
    fs::create_dir_all(&skill_dir)?;
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: graph-gated-agent-step\n---\n# Graph Gated Agent Step\n",
    )?;
    fs::write(
        skill_dir.join("X.yaml"),
        r#"
skill: graph-gated-agent-step
runners:
  graph:
    default: true
    type: graph
    graph:
      name: graph-gated-agent-step
      steps:
        - id: decide
          run:
            type: agent-step
            agent: builder
            task: gated-decide
            outputs:
              approved: boolean
        - id: gated
          run:
            type: agent-step
            agent: builder
            task: gated-followup
            outputs:
              result: object
      policy:
        transitions:
          - to: gated
            field: decide.approved
            equals: true
"#,
    )?;
    Ok(skill_dir.to_path_buf())
}

fn write_graph_nested_agent_skill(
    root: &Path,
    source_type: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let child_name = match source_type {
        "agent" => "child-agent",
        "agent-step" => "child-agent-step",
        _ => return Err(format!("unsupported nested agent source type {source_type}").into()),
    };
    let child_dir = root.join(child_name);
    fs::create_dir_all(&child_dir)?;
    let source = if source_type == "agent-step" {
        r#"
source:
  type: agent-step
  agent: builder
  task: child-agent-step
  outputs:
    result: object
"#
    } else {
        r#"
source:
  type: agent
"#
    };
    fs::write(
        child_dir.join("SKILL.md"),
        format!(
            r#"---
name: {child_name}{source}---
# {child_name}
"#
        ),
    )?;

    let skill_dir = root.join(format!("graph-nested-{source_type}"));
    fs::create_dir_all(&skill_dir)?;
    fs::write(
        skill_dir.join("SKILL.md"),
        format!("---\nname: graph-nested-{source_type}\n---\n# Graph Nested {source_type}\n"),
    )?;
    fs::write(
        skill_dir.join("X.yaml"),
        format!(
            r#"
skill: graph-nested-{source_type}
runners:
  graph:
    default: true
    type: graph
    graph:
      name: graph-nested-{source_type}
      steps:
        - id: nested
          skill: ../{child_name}
          inputs:
            thread_title: $input.thread_title
"#
        ),
    )?;
    Ok(skill_dir)
}

#[cfg(feature = "catalog")]
fn write_graph_tool_skill(root: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let skill_dir = root.join("graph-tool");
    write_graph_tool_skill_at(&skill_dir)
}

#[cfg(feature = "catalog")]
fn write_graph_tool_skill_under_skills(root: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let skill_dir = root.join("skills/graph-tool");
    write_graph_tool_skill_at(&skill_dir)
}

#[cfg(feature = "catalog")]
fn write_graph_tool_skill_at(skill_dir: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    fs::create_dir_all(skill_dir)?;
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: graph-tool\n---\n# Graph Tool\n",
    )?;
    fs::write(
        skill_dir.join("X.yaml"),
        r#"
skill: graph-tool
runners:
  graph:
    default: true
    type: graph
    graph:
      name: graph-tool
      steps:
        - id: echo
          tool: test.echo
          inputs:
            message: $input.thread_title
"#,
    )?;
    Ok(skill_dir.to_path_buf())
}

#[cfg(feature = "catalog")]
fn write_graph_agent_artifact_context_skill(
    root: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let skill_dir = root.join("graph-agent-artifact-context");
    fs::create_dir_all(&skill_dir)?;
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: graph-agent-artifact-context\n---\n# Graph Agent Artifact Context\n",
    )?;
    fs::write(
        skill_dir.join("X.yaml"),
        r#"
skill: graph-agent-artifact-context
runners:
  graph:
    default: true
    type: graph
    graph:
      name: graph-agent-artifact-context
      steps:
        - id: author
          run:
            type: agent-step
            agent: builder
            task: graph-author
            outputs:
              fix_bundle: object
          artifacts:
            named_emits:
              fix_bundle: fix_bundle
        - id: echo
          tool: test.echo
          context:
            message: author.fix_bundle.data.message
"#,
    )?;
    Ok(skill_dir)
}

#[cfg(feature = "catalog")]
fn write_graph_optional_json_tool_skill(
    root: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let skill_dir = root.join("graph-optional-json-tool");
    fs::create_dir_all(&skill_dir)?;
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: graph-optional-json-tool\n---\n# Graph Optional JSON Tool\n",
    )?;
    fs::write(
        skill_dir.join("X.yaml"),
        r#"
skill: graph-optional-json-tool
runners:
  graph:
    default: true
    type: graph
    graph:
      name: graph-optional-json-tool
      steps:
        - id: echo
          tool: test.optional-json
          inputs:
            message: $input.thread_title
            harness: $input.harness
"#,
    )?;
    Ok(skill_dir)
}

#[cfg(feature = "catalog")]
fn write_echo_tool(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    write_echo_tool_at(&root.join("tools/test/echo"), "Graph tool bug")
}

#[cfg(feature = "catalog")]
fn write_echo_tool_at(tool_dir: &Path, message: &str) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(tool_dir)?;
    fs::write(
        tool_dir.join("manifest.json"),
        r#"{
  "schema": "runx.tool.manifest.v1",
  "name": "test.echo",
  "source": {
    "type": "cli-tool",
    "command": "/bin/sh",
    "args": ["./run.sh"],
    "input_mode": "stdin"
  },
  "inputs": {
    "message": { "type": "string", "required": true }
  },
  "scopes": ["test.echo"]
}
"#,
    )?;
    fs::write(
        tool_dir.join("run.sh"),
        format!(
            r#"raw="$(cat)"
case "$raw" in
  *"Graph tool bug"*) printf '%s\n' '{{"echo":{{"message":"{}"}}}}' ;;
  *) printf '%s\n' '{{"echo":{{"message":"unexpected"}}}}' ;;
esac
"#,
            message
        ),
    )?;
    Ok(())
}

#[cfg(feature = "catalog")]
fn write_optional_json_tool(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let tool_dir = root.join("tools/test/optional-json");
    fs::create_dir_all(&tool_dir)?;
    fs::write(
        tool_dir.join("manifest.json"),
        r#"{
  "schema": "runx.tool.manifest.v1",
  "name": "test.optional-json",
  "source": {
    "type": "cli-tool",
    "command": "/bin/sh",
    "args": ["./run.sh"],
    "input_mode": "stdin"
  },
  "inputs": {
    "message": { "type": "string", "required": true },
    "harness": { "type": "json", "required": false }
  },
  "scopes": ["test.optional-json"]
}
"#,
    )?;
    fs::write(
        tool_dir.join("run.sh"),
        r#"raw="$(cat)"
case "$raw" in
  *'$input.harness'*)
    printf '%s\n' '{"error":"unresolved harness reference reached tool input"}'
    exit 2
    ;;
  *'"harness"'*)
    printf '%s\n' '{"error":"optional harness should be omitted when absent"}'
    exit 3
    ;;
  *"Graph optional JSON bug"*)
    printf '%s\n' '{"echo":{"message":"Graph optional JSON bug"}}'
    ;;
  *)
    printf '%s\n' '{"echo":{"message":"unexpected"}}'
    ;;
esac
"#,
    )?;
    Ok(())
}

#[cfg(feature = "cli-tool")]
fn write_graph_nested_cli_skill(root: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let child_dir = root.join("child-echo");
    fs::create_dir_all(&child_dir)?;
    fs::write(
        child_dir.join("SKILL.md"),
        r#"---
name: child-echo
source:
  type: cli-tool
  command: node
  args:
    - run.mjs
  input_mode: stdin
---
# Child Echo
"#,
    )?;
    fs::write(
        child_dir.join("run.mjs"),
        r#"import fs from "node:fs";
const raw = fs.readFileSync(0, "utf8");
const input = raw.trim() ? JSON.parse(raw) : {};
console.log(JSON.stringify({ nested: { message: input.message } }));
"#,
    )?;

    let skill_dir = root.join("graph-nested-cli");
    fs::create_dir_all(&skill_dir)?;
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: graph-nested-cli\n---\n# Graph Nested CLI\n",
    )?;
    fs::write(
        skill_dir.join("X.yaml"),
        r#"
skill: graph-nested-cli
runners:
  graph:
    default: true
    type: graph
    graph:
      name: graph-nested-cli
      steps:
        - id: nested
          skill: ../child-echo
          inputs:
            message: $input.thread_title
"#,
    )?;
    Ok(skill_dir)
}

#[cfg(feature = "cli-tool")]
fn write_graph_nested_x_yaml_cli_skill(root: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let child_dir = root.join("child-x-cli");
    fs::create_dir_all(&child_dir)?;
    fs::write(
        child_dir.join("SKILL.md"),
        "---\nname: child-x-cli\n---\n# Child X CLI\n",
    )?;
    fs::write(
        child_dir.join("X.yaml"),
        r#"
skill: child-x-cli
runners:
  child-cli:
    default: true
    type: cli-tool
    command: node
    args:
      - run.mjs
    input_mode: stdin
"#,
    )?;
    fs::write(
        child_dir.join("run.mjs"),
        r#"import fs from "node:fs";
const raw = fs.readFileSync(0, "utf8");
const input = raw.trim() ? JSON.parse(raw) : {};
console.log(JSON.stringify({ nested: { message: input.message } }));
"#,
    )?;

    let skill_dir = root.join("graph-nested-x-yaml-cli");
    fs::create_dir_all(&skill_dir)?;
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: graph-nested-x-yaml-cli\n---\n# Graph Nested X YAML CLI\n",
    )?;
    fs::write(
        skill_dir.join("X.yaml"),
        r#"
skill: graph-nested-x-yaml-cli
runners:
  graph:
    default: true
    type: graph
    graph:
      name: graph-nested-x-yaml-cli
      steps:
        - id: nested
          skill: ../child-x-cli
          inputs:
            message: $input.thread_title
"#,
    )?;
    Ok(skill_dir)
}

fn object<'a>(
    value: &'a JsonValue,
    label: &str,
) -> Result<&'a runx_contracts::JsonObject, Box<dyn std::error::Error>> {
    match value {
        JsonValue::Object(object) => Ok(object),
        _ => Err(format!("{label} was not an object").into()),
    }
}

fn object_mut<'a>(
    value: &'a mut JsonValue,
    label: &str,
) -> Result<&'a mut runx_contracts::JsonObject, Box<dyn std::error::Error>> {
    match value {
        JsonValue::Object(object) => Ok(object),
        _ => Err(format!("{label} was not an object").into()),
    }
}

fn object_field<'a>(
    object: &'a runx_contracts::JsonObject,
    field: &str,
) -> Option<&'a runx_contracts::JsonObject> {
    match object.get(field) {
        Some(JsonValue::Object(value)) => Some(value),
        _ => None,
    }
}

fn step_claim<'a>(
    payload: &'a runx_contracts::JsonObject,
    step_id: &str,
) -> Option<&'a runx_contracts::JsonObject> {
    object_field(payload, "step_outputs")
        .and_then(|steps| object_field(steps, step_id))
        .and_then(|step| object_field(step, "skill_claim"))
}

fn array_field<'a>(
    object: &'a runx_contracts::JsonObject,
    field: &str,
) -> Option<&'a Vec<JsonValue>> {
    match object.get(field) {
        Some(JsonValue::Array(value)) => Some(value),
        _ => None,
    }
}

fn string_field<'a>(object: &'a runx_contracts::JsonObject, field: &str) -> Option<&'a str> {
    match object.get(field) {
        Some(JsonValue::String(value)) => Some(value),
        _ => None,
    }
}
