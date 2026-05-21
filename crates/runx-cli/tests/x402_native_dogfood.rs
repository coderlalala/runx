use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

#[test]
fn native_x402_mock_dogfood_fixtures_run_without_typescript()
-> Result<(), Box<dyn std::error::Error>> {
    let approved = run_harness_fixture(
        "fixtures/harness/x402-pay-approval.yaml",
        &[
            "credential_envelope",
            "rail_session_material",
            "rail-session-material:mock:payment-execution-001",
        ],
    )?;
    assert_eq!(approved["schema"], "runx.harness_receipt.v1");
    assert_eq!(approved["harness"]["state"], "sealed");
    assert_eq!(approved["seal"]["disposition"], "closed");
    assert_eq!(
        child_receipt_uris(&approved),
        vec![
            "runx:harness_receipt:hrn_rcpt_x402-pay-approval_approve-spend",
            "runx:harness_receipt:hrn_rcpt_x402-pay-approval_fulfill",
        ]
    );

    let denied = run_harness_fixture(
        "fixtures/harness/x402-pay-approval-denied.yaml",
        &[
            "credential_envelope",
            "rail_session_material",
            "rail-session-material:mock:payment-execution-001",
        ],
    )?;
    assert_eq!(denied["schema"], "runx.harness_receipt.v1");
    assert_eq!(denied["harness"]["state"], "sealed");
    assert_eq!(denied["seal"]["disposition"], "blocked");
    assert_eq!(denied["seal"]["reason_code"], "graph_blocked");
    assert_eq!(
        child_receipt_uris(&denied),
        vec!["runx:harness_receipt:hrn_rcpt_x402-pay-approval_approve-spend",]
    );

    Ok(())
}

#[test]
fn native_x402_paid_echo_fixture_passes_only_refs_downstream()
-> Result<(), Box<dyn std::error::Error>> {
    let receipt = run_harness_fixture(
        "fixtures/harness/x402-pay-paid-echo.yaml",
        &[
            "credential_envelope",
            "rail_session_material",
            "rail-session-material:mock:paid-echo-001",
        ],
    )?;

    assert_eq!(receipt["schema"], "runx.harness_receipt.v1");
    assert_eq!(receipt["harness"]["state"], "sealed");
    assert_eq!(receipt["seal"]["disposition"], "closed");
    assert_eq!(
        child_receipt_uris(&receipt),
        vec![
            "runx:harness_receipt:hrn_rcpt_x402-pay-paid-echo_quote",
            "runx:harness_receipt:hrn_rcpt_x402-pay-paid-echo_reserve",
            "runx:harness_receipt:hrn_rcpt_x402-pay-paid-echo_approve-spend",
            "runx:harness_receipt:hrn_rcpt_x402-pay-paid-echo_fulfill",
            "runx:harness_receipt:hrn_rcpt_x402-pay-paid-echo_echo",
        ]
    );

    Ok(())
}

#[test]
fn native_x402_stripe_spt_happy_path_runs_without_typescript()
-> Result<(), Box<dyn std::error::Error>> {
    let receipt = run_harness_fixture(
        "fixtures/harness/stripe-spt-payment.yaml",
        &[
            "credential_envelope",
            "rail_session_material",
            "rail-session-material:stripe-spt:demo-search-001",
            "client_secret",
            "webhook_secret",
            "card_number",
        ],
    )?;

    assert_eq!(receipt["schema"], "runx.harness_receipt.v1");
    assert_eq!(receipt["harness"]["state"], "sealed");
    assert_eq!(receipt["seal"]["disposition"], "closed");
    assert_eq!(
        child_receipt_uris(&receipt),
        vec![
            "runx:harness_receipt:hrn_rcpt_stripe-spt-payment_quote",
            "runx:harness_receipt:hrn_rcpt_stripe-spt-payment_reserve",
            "runx:harness_receipt:hrn_rcpt_stripe-spt-payment_approve-spend",
            "runx:harness_receipt:hrn_rcpt_stripe-spt-payment_fulfill",
        ]
    );

    Ok(())
}

fn run_harness_fixture(
    fixture: &str,
    denied_fragments: &[&str],
) -> Result<Value, Box<dyn std::error::Error>> {
    let output = native_command()?
        .args(["harness", fixture, "--json"])
        .output()?;
    assert_success(&output)?;
    let stdout = String::from_utf8(output.stdout)?;
    for denied in denied_fragments {
        assert!(
            !stdout.contains(denied),
            "native CLI receipt output must not expose raw payment material: {denied}"
        );
    }
    Ok(serde_json::from_str(&stdout)?)
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

fn assert_success(output: &Output) -> Result<(), Box<dyn std::error::Error>> {
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

fn child_receipt_uris(receipt: &Value) -> Vec<String> {
    receipt["harness"]["child_harness_receipt_refs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|reference| reference["uri"].as_str().map(str::to_owned))
        .collect()
}

fn repo_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?)
}
