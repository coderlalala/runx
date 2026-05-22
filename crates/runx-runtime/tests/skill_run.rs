use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use runx_contracts::JsonValue;
use runx_runtime::{
    LocalOrchestrator, LocalReceiptStore, RUNX_RECEIPT_DIR_ENV, RunResult, SkillRunRequest,
};
use tempfile::tempdir;

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
fn native_skill_run_resumes_and_seals_harness_receipt() -> Result<(), Box<dyn std::error::Error>> {
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
    assert!(receipt_id.starts_with("hrn_rcpt_issue-intake-run_"));
    assert!(receipt_dir.join(format!("{receipt_id}.json")).exists());

    let receipt = LocalReceiptStore::new(&receipt_dir).read_exact(receipt_id)?;
    assert_eq!(
        serde_json::to_value(&receipt.schema)?,
        serde_json::json!("runx.harness_receipt.v1")
    );
    assert_eq!(serde_json::to_value(&receipt.seal.disposition)?, "declined");
    assert_eq!(receipt.harness.acts.len(), 1);
    assert_eq!(receipt.harness.decisions.len(), 1);
    assert_eq!(
        serde_json::to_value(&receipt.harness.acts[0].closure.disposition)?,
        "declined"
    );

    let payload = object_field(output, "payload").ok_or("missing payload")?;
    assert!(object_field(payload, "intake_report").is_some());

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

fn object_field<'a>(
    object: &'a runx_contracts::JsonObject,
    field: &str,
) -> Option<&'a runx_contracts::JsonObject> {
    match object.get(field) {
        Some(JsonValue::Object(value)) => Some(value),
        _ => None,
    }
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
