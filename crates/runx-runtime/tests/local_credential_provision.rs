//! Local, no-network per-run credential provision.
//!
//! Proves the OSS run path can supply a credential to a skill without any
//! network or hosted dependency: the descriptor on `SkillRunRequest` is turned
//! into a `CredentialDelivery` in-memory, the secret reaches the skill process,
//! and the secret value is redacted from the captured output, the sealed
//! receipt, and the run metadata. There is no brokerage, no persistence, and no
//! outbound call: the only process spawned is the cli-tool skill itself.

#![cfg(feature = "cli-tool")]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use runx_runtime::orchestrator::LocalCredentialDescriptor;
use runx_runtime::{LocalOrchestrator, RunResult, SkillRunRequest};
use tempfile::tempdir;

const SECRET: &str = "ghs_local_provision_secret_value";
const REDACTED: &str = "[redacted-credential]";

#[test]
fn local_credential_is_delivered_and_redacted() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let skill_dir = write_echo_token_skill(temp.path())?;
    let receipt_dir = temp.path().join("receipts");

    let request = SkillRunRequest {
        skill_path: skill_dir,
        receipt_dir: Some(receipt_dir),
        run_id: None,
        answers_path: None,
        inputs: BTreeMap::new(),
        env: BTreeMap::new(),
        cwd: temp.path().to_path_buf(),
        local_credential: Some(LocalCredentialDescriptor {
            provider: "github".to_owned(),
            auth_mode: "bearer".to_owned(),
            env_var: "GITHUB_TOKEN".to_owned(),
            material_ref: "local://github/main".to_owned(),
            scopes: vec!["repo".to_owned()],
            secret: SECRET.to_owned(),
        }),
    };

    let result = run_skill(request)?;

    // (1) The credential reached the skill process: the echo skill emitted the
    // delivered env var, redacted on the way back out.
    let serialized = serde_json::to_string(&result.output)?;
    assert!(
        serialized.contains(REDACTED),
        "expected the delivered credential to be redacted in the output, got: {serialized}",
    );

    // (2) The secret value never appears anywhere in the output, receipt, or
    // metadata that the run produced.
    assert!(
        !serialized.contains(SECRET),
        "raw secret leaked into the run output/receipt/metadata",
    );

    Ok(())
}

#[test]
fn run_without_descriptor_delivers_no_credential() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let skill_dir = write_echo_token_skill(temp.path())?;

    let request = SkillRunRequest {
        skill_path: skill_dir,
        receipt_dir: Some(temp.path().join("receipts")),
        run_id: None,
        answers_path: None,
        inputs: BTreeMap::new(),
        env: BTreeMap::new(),
        cwd: temp.path().to_path_buf(),
        local_credential: None,
    };

    let result = run_skill(request)?;
    let serialized = serde_json::to_string(&result.output)?;
    assert!(
        !serialized.contains(SECRET),
        "no credential was provided, the secret must not appear anywhere",
    );
    Ok(())
}

fn run_skill(request: SkillRunRequest) -> Result<RunResult, Box<dyn std::error::Error>> {
    LocalOrchestrator.run_skill(&request).map_err(Into::into)
}

/// A cli-tool skill that echoes the delivered `$GITHUB_TOKEN`. The command is a
/// local shell process: no network, no hosted dependency.
fn write_echo_token_skill(root: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let skill_dir = root.join("echo-token");
    fs::create_dir_all(&skill_dir)?;
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: echo-token\n---\n# Echo Token\n",
    )?;
    fs::write(
        skill_dir.join("X.yaml"),
        r#"
skill: echo-token
runners:
  echo:
    default: true
    type: cli-tool
    command: sh
    args:
      - "-c"
      - "printf '%s' \"$GITHUB_TOKEN\""
    sandbox:
      profile: readonly
"#,
    )?;
    Ok(skill_dir)
}
