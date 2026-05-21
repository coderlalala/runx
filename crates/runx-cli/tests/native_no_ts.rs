use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn native_cli_smoke_runs_without_node_or_typescript_env() -> Result<(), Box<dyn std::error::Error>>
{
    let doctor = native_command()?
        .args([
            "doctor",
            "fixtures/doctor/empty-success/workspace",
            "--json",
        ])
        .output()?;
    assert_success(&doctor)?;
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&doctor.stdout)?["status"],
        "success"
    );

    let list = native_command()?
        .args(["list", "skills", "--json"])
        .output()?;
    assert_success(&list)?;
    let list_json = serde_json::from_slice::<serde_json::Value>(&list.stdout)?;
    assert_eq!(list_json["schema"], "runx.list.v1");

    let temp = temp_root("runx-native-no-ts");
    fs::create_dir_all(&temp)?;
    let receipt_dir = temp.join("receipts");

    let history = native_command()?
        .args([
            "history",
            "--receipt-dir",
            receipt_dir.to_str().ok_or("non-utf8 receipt dir")?,
            "--json",
        ])
        .output()?;
    assert_success(&history)?;
    let history_json = serde_json::from_slice::<serde_json::Value>(&history.stdout)?;
    assert_eq!(
        history_json["projector_id"],
        "runx-runtime.local-history.v1"
    );

    let skill_dir = write_agent_step_skill(&temp)?;
    let skill = native_command()?
        .args([
            "skill",
            skill_dir.to_str().ok_or("non-utf8 skill dir")?,
            "--receipt-dir",
            receipt_dir.to_str().ok_or("non-utf8 receipt dir")?,
            "--json",
            "--non-interactive",
        ])
        .output()?;
    assert_eq!(
        skill.status.code(),
        Some(2),
        "stderr={}\nstdout={}",
        String::from_utf8_lossy(&skill.stderr),
        String::from_utf8_lossy(&skill.stdout)
    );
    assert_eq!(String::from_utf8(skill.stderr.clone())?, "");
    let skill_json = serde_json::from_slice::<serde_json::Value>(&skill.stdout)?;
    assert_eq!(skill_json["status"], "needs_agent");
    assert_eq!(skill_json["requests"][0]["kind"], "agent_act");

    let harness = native_command()?
        .args([
            "harness",
            "fixtures/harness/payment-approval-graph.yaml",
            "--json",
        ])
        .output()?;
    assert_success(&harness)?;
    let receipt = serde_json::from_slice::<serde_json::Value>(&harness.stdout)?;
    assert_eq!(receipt["schema"], "runx.harness_receipt.v1");
    assert_eq!(receipt["harness"]["state"], "sealed");

    Ok(())
}

fn native_command() -> Result<Command, Box<dyn std::error::Error>> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_runx"));
    command.current_dir(repo_root()?);
    command.env_clear();
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    command.env("NO_COLOR", "1");
    Ok(command)
}

fn assert_success(output: &std::process::Output) -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        output.status.success(),
        "status={:?}\nstderr={}\nstdout={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(String::from_utf8(output.stderr.clone())?, "");
    Ok(())
}

fn repo_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?)
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
"#,
    )?;
    Ok(skill_dir)
}

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let root = std::env::temp_dir().join(format!("{name}-{}-{nanos}", std::process::id()));
    if root.exists() {
        let _ignored = fs::remove_dir_all(&root);
    }
    root
}
