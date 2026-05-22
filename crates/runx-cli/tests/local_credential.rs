//! End-to-end proof that the OSS CLI can provision a credential for a single
//! run with no network and no hosted dependency.
//!
//! Drives the real `runx skill` binary with `--credential` and `--secret-env`,
//! the only local credential-provision surface in the MIT CLI. The echo skill
//! is a local shell process; the only thing spawned is the skill itself. The
//! secret must reach the skill (proving the flags parse into a descriptor and
//! the runtime delivers it) and must be redacted from the captured output and
//! the sealed receipt (proving the boundary holds end to end).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const SECRET: &str = "ghs_cli_local_provision_secret_value";
const REDACTED: &str = "[redacted-credential]";

#[test]
fn cli_provisions_local_credential_and_redacts_secret() -> Result<(), Box<dyn std::error::Error>> {
    let temp = temp_root("runx-cli-local-credential");
    fs::create_dir_all(&temp)?;
    let skill_dir = write_echo_token_skill(&temp)?;
    let receipt_dir = temp.join("receipts");

    let output = native_command()?
        .arg("skill")
        .arg(&skill_dir)
        .arg("--receipt-dir")
        .arg(&receipt_dir)
        .arg("--credential")
        .arg("github:bearer:local://github/main:repo")
        .arg("--secret-env")
        .arg(format!("GITHUB_TOKEN={SECRET}"))
        .arg("--json")
        .output()?;

    assert!(
        output.status.success(),
        "status={:?}\nstderr={}\nstdout={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(String::from_utf8(output.stderr.clone())?, "");

    let stdout = String::from_utf8(output.stdout)?;
    // The credential reached the skill process: the echo skill emitted the
    // delivered env var, redacted on the way back out.
    assert!(
        stdout.contains(REDACTED),
        "expected the delivered credential to be redacted in the result, got: {stdout}"
    );
    // The raw secret never appears anywhere the run produced.
    assert!(
        !stdout.contains(SECRET),
        "raw secret leaked into the CLI result"
    );

    // The sealed receipt on disk must not carry the raw secret either.
    for receipt in receipt_files(&receipt_dir)? {
        let body = fs::read_to_string(&receipt)?;
        assert!(
            !body.contains(SECRET),
            "raw secret leaked into receipt {}",
            receipt.display()
        );
    }

    Ok(())
}

#[test]
fn cli_rejects_secret_env_without_credential() -> Result<(), Box<dyn std::error::Error>> {
    let temp = temp_root("runx-cli-local-credential-bad");
    fs::create_dir_all(&temp)?;
    let skill_dir = write_echo_token_skill(&temp)?;

    let output = native_command()?
        .arg("skill")
        .arg(&skill_dir)
        .arg("--secret-env")
        .arg(format!("GITHUB_TOKEN={SECRET}"))
        .arg("--json")
        .output()?;

    assert!(
        !output.status.success(),
        "expected provisioning without --credential to fail"
    );
    let stderr = String::from_utf8(output.stderr)?;
    assert!(
        stderr.contains("--credential"),
        "expected an error pointing at --credential, got: {stderr}"
    );
    assert!(
        !stderr.contains(SECRET),
        "raw secret leaked into the error output"
    );

    Ok(())
}

fn native_command() -> Result<Command, Box<dyn std::error::Error>> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_runx"));
    command.env_clear();
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    command.env("NO_COLOR", "1");
    Ok(command)
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

fn receipt_files(receipt_dir: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    collect_files(receipt_dir, &mut files)?;
    Ok(files)
}

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_files(&path, files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}

fn temp_root(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("{prefix}-{nanos}"))
}
