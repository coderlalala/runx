use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

use runx_runtime::journal::{
    HistoryFilter, JournalProjectionError, list_local_history, list_local_history_with_policy,
};
use runx_runtime::{
    Ed25519ReceiptVerifier, LocalReceiptStore, ReceiptPathInputs, RuntimeReceiptConfig,
    RuntimeReceiptSignaturePolicy, resolve_receipt_path,
};

// rust-style-allow: large-file because the native history CLI slice keeps
// parsing, rendering, and CLI parity tests together until the rest of the Rust
// command routing settles.
#[derive(Debug)]
pub enum HistoryCliError {
    InvalidArgs(String),
    InvalidReceiptVerifier(String),
    Projection(JournalProjectionError),
    Serialize(serde_json::Error),
}

impl fmt::Display for HistoryCliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidArgs(message) => formatter.write_str(message),
            Self::InvalidReceiptVerifier(message) => formatter.write_str(message),
            Self::Projection(error) => write!(formatter, "{error}"),
            Self::Serialize(error) => write!(formatter, "failed to serialize history: {error}"),
        }
    }
}

impl std::error::Error for HistoryCliError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryCliResult {
    pub output: String,
    pub error_is_usage: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ParsedHistoryArgs {
    receipt_dir: Option<PathBuf>,
    query: Option<String>,
    filter: HistoryFilter,
    json: bool,
}

pub fn run_history_command(
    args: &[OsString],
    env: &BTreeMap<String, String>,
    cwd: &Path,
) -> Result<HistoryCliResult, HistoryCliError> {
    let parsed = parse_history_args(args)?;
    let receipt_config = RuntimeReceiptConfig::default();
    let resolved = resolve_receipt_path(ReceiptPathInputs {
        explicit_dir: parsed.receipt_dir.as_deref(),
        runtime_config: Some(&receipt_config),
        env,
        cwd,
    });
    let store = LocalReceiptStore::new(&resolved.path);
    let verifier = history_production_verifier(env)?;
    let history = if let Some(verifier) = verifier.as_ref() {
        list_local_history_with_policy(
            &store,
            &resolved.workspace_base,
            &resolved.project_runx_dir,
            &parsed.filter,
            RuntimeReceiptSignaturePolicy::production(verifier),
        )
    } else {
        list_local_history(
            &store,
            &resolved.workspace_base,
            &resolved.project_runx_dir,
            &parsed.filter,
        )
    }
    .map_err(HistoryCliError::Projection)?;
    let output = if parsed.json {
        format!(
            "{}\n",
            serde_json::to_string_pretty(&history).map_err(HistoryCliError::Serialize)?
        )
    } else {
        render_history(&history, parsed.query.as_deref())
    };
    Ok(HistoryCliResult {
        output,
        error_is_usage: false,
    })
}

const RUNX_RECEIPT_VERIFY_KID_ENV: &str = "RUNX_RECEIPT_VERIFY_KID";
const RUNX_RECEIPT_VERIFY_ED25519_PUBLIC_KEY_BASE64_ENV: &str =
    "RUNX_RECEIPT_VERIFY_ED25519_PUBLIC_KEY_BASE64";

fn history_production_verifier(
    env: &BTreeMap<String, String>,
) -> Result<Option<Ed25519ReceiptVerifier>, HistoryCliError> {
    let kid = non_empty_env(env, RUNX_RECEIPT_VERIFY_KID_ENV);
    let public_key = non_empty_env(env, RUNX_RECEIPT_VERIFY_ED25519_PUBLIC_KEY_BASE64_ENV);
    match (kid, public_key) {
        (None, None) => Ok(None),
        (Some(kid), Some(public_key)) => Ed25519ReceiptVerifier::from_public_key_base64(
            kid.to_owned(),
            public_key,
        )
        .map(Some)
        .map_err(|_| {
            HistoryCliError::InvalidReceiptVerifier(format!(
                "{RUNX_RECEIPT_VERIFY_ED25519_PUBLIC_KEY_BASE64_ENV} is not valid Ed25519 public key material"
            ))
        }),
        _ => Err(HistoryCliError::InvalidReceiptVerifier(format!(
            "{RUNX_RECEIPT_VERIFY_KID_ENV} and {RUNX_RECEIPT_VERIFY_ED25519_PUBLIC_KEY_BASE64_ENV} must be set together"
        ))),
    }
}

fn non_empty_env<'a>(env: &'a BTreeMap<String, String>, key: &str) -> Option<&'a str> {
    env.get(key)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub fn env_map() -> BTreeMap<String, String> {
    env::vars().collect()
}

// rust-style-allow: long-function because this mirrors the public history CLI
// flag grammar in one parser during the hard cutover.
fn parse_history_args(args: &[OsString]) -> Result<ParsedHistoryArgs, HistoryCliError> {
    if args.first().and_then(|arg| arg.to_str()) != Some("history") {
        return Err(HistoryCliError::InvalidArgs(
            "internal error: history dispatcher received non-history command".to_owned(),
        ));
    }
    let mut parsed = ParsedHistoryArgs::default();
    let mut positionals = Vec::new();
    let mut index = 1;
    while index < args.len() {
        let token = os_arg(args, index)?;
        if !token.starts_with("--") {
            positionals.push(token.to_owned());
            index += 1;
            continue;
        }
        let (flag, inline_value) = split_flag(token);
        match flag {
            "--json" => {
                if inline_value.is_some() {
                    return Err(invalid_args("--json does not take a value"));
                }
                parsed.json = true;
                index += 1;
            }
            "--receipt-dir" => {
                let (value, next_index) = flag_value(args, index, flag, inline_value)?;
                parsed.receipt_dir = Some(PathBuf::from(value));
                index = next_index;
            }
            "--skill" => {
                let (value, next_index) = flag_value(args, index, flag, inline_value)?;
                parsed.filter.skill = Some(value);
                index = next_index;
            }
            "--status" => {
                let (value, next_index) = flag_value(args, index, flag, inline_value)?;
                parsed.filter.status = Some(value);
                index = next_index;
            }
            "--source" => {
                let (value, next_index) = flag_value(args, index, flag, inline_value)?;
                parsed.filter.source = Some(value);
                index = next_index;
            }
            "--actor" => {
                let (value, next_index) = flag_value(args, index, flag, inline_value)?;
                parsed.filter.actor = Some(value);
                index = next_index;
            }
            "--artifact-type" | "--artifact_type" | "--artifactType" => {
                let (value, next_index) = flag_value(args, index, flag, inline_value)?;
                parsed.filter.artifact_type = Some(value);
                index = next_index;
            }
            "--since" => {
                let (value, next_index) = flag_value(args, index, flag, inline_value)?;
                parsed.filter.since = Some(value);
                index = next_index;
            }
            "--until" => {
                let (value, next_index) = flag_value(args, index, flag, inline_value)?;
                parsed.filter.until = Some(value);
                index = next_index;
            }
            "--limit" => {
                let (value, next_index) = flag_value(args, index, flag, inline_value)?;
                parsed.filter.limit = Some(value.parse().map_err(|_| {
                    HistoryCliError::InvalidArgs(format!("invalid --limit value '{value}'"))
                })?);
                index = next_index;
            }
            _ => {
                return Err(HistoryCliError::InvalidArgs(format!(
                    "unknown history flag {flag}"
                )));
            }
        }
    }
    parsed.query = (!positionals.is_empty()).then(|| positionals.join(" "));
    parsed.filter.query = parsed.query.clone();
    Ok(parsed)
}

fn render_history(
    history: &runx_runtime::journal::LocalHistoryProjection,
    query: Option<&str>,
) -> String {
    let total = history.receipts.len() + history.pending_runs.len();
    if total == 0 {
        if let Some(query) = query {
            return format!(
                "\n  No receipts matched {query}.\n  Try runx history to see every local run.\n\n"
            );
        }
        return "\n  No receipts yet. Try a run first:\n  runx skill <skill-dir> --json\n  runx harness <fixture.yaml> --json\n\n"
            .to_owned();
    }
    let mut lines = Vec::new();
    lines.push(String::new());
    if history.pending_runs.is_empty() {
        lines.push(format!("  history  {} receipt(s)", history.receipts.len()));
    } else {
        lines.push(format!(
            "  history  {} receipt(s), {} needs_agent",
            history.receipts.len(),
            history.pending_runs.len()
        ));
    }
    lines.push(String::new());
    for pending in &history.pending_runs {
        let step = pending
            .step_labels
            .first()
            .or_else(|| pending.step_ids.first())
            .map_or("", String::as_str);
        lines.push(format!(
            "  *  {}  needs_agent  {}  {}",
            pending.name,
            step,
            short_id(&pending.id)
        ));
    }
    for receipt in &history.receipts {
        lines.push(format!(
            "  {}  {}  {}  {}",
            receipt.status,
            receipt.name,
            receipt.verification.status,
            short_id(&receipt.id)
        ));
    }
    lines.push(String::new());
    if history.pending_runs.is_empty() {
        lines.push("  next  runx history <receipt-id> --json".to_owned());
    } else {
        lines.push(
            "  next  rerun the same runx skill <path> with --run-id and --answers".to_owned(),
        );
    }
    lines.push(String::new());
    lines.join("\n")
}

fn short_id(value: &str) -> &str {
    value.get(..12).unwrap_or(value)
}

fn os_arg(args: &[OsString], index: usize) -> Result<&str, HistoryCliError> {
    args.get(index)
        .and_then(|arg| arg.to_str())
        .ok_or_else(|| HistoryCliError::InvalidArgs("history arguments must be UTF-8".to_owned()))
}

fn split_flag(token: &str) -> (&str, Option<&str>) {
    token
        .split_once('=')
        .map_or((token, None), |(flag, value)| (flag, Some(value)))
}

fn flag_value(
    args: &[OsString],
    index: usize,
    flag: &str,
    inline_value: Option<&str>,
) -> Result<(String, usize), HistoryCliError> {
    if let Some(value) = inline_value {
        return Ok((value.to_owned(), index + 1));
    }
    let value = os_arg(args, index + 1)
        .map_err(|_| HistoryCliError::InvalidArgs(format!("{flag} requires a value")))?;
    if value.starts_with("--") {
        return Err(HistoryCliError::InvalidArgs(format!(
            "{flag} requires a value"
        )));
    }
    Ok((value.to_owned(), index + 2))
}

fn invalid_args(message: &str) -> HistoryCliError {
    HistoryCliError::InvalidArgs(message.to_owned())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;

    use super::*;

    #[test]
    fn parses_history_args_without_comparing_against_runtime_constants() -> Result<(), io::Error> {
        let parsed = parse_history_args(&[
            "history".into(),
            "sourcey".into(),
            "--skill".into(),
            "source".into(),
            "--status=needs_agent".into(),
            "--artifact-type".into(),
            "artifact".into(),
            "--json".into(),
        ])
        .map_err(|error| io::Error::other(error.to_string()))?;

        assert_eq!(parsed.query.as_deref(), Some("sourcey"));
        assert_eq!(parsed.filter.skill.as_deref(), Some("source"));
        assert_eq!(parsed.filter.status.as_deref(), Some("needs_agent"));
        assert_eq!(parsed.filter.artifact_type.as_deref(), Some("artifact"));
        assert!(parsed.json);
        Ok(())
    }

    #[test]
    // rust-style-allow: long-function because the CLI execute oracle test keeps
    // its ledger fixture, command invocation, and typed output assertions in
    // one place so the parity case remains readable.
    fn executes_history_json_against_cli_parity_oracle() -> Result<(), io::Error> {
        let temp = tempfile_dir()?;
        let receipt_dir = temp.join("receipts");
        fs::create_dir_all(receipt_dir.join("ledgers"))?;
        fs::write(
            receipt_dir
                .join("ledgers")
                .join("gx_needs_agent_oracle.jsonl"),
            format!(
                "{}\n{}\n",
                r#"{"entry":{"type":"run_event","version":"1","data":{"kind":"run_started","status":"started","step_id":null,"detail":{}},"meta":{"artifact_id":"ax_start","run_id":"gx_needs_agent_oracle","step_id":null,"producer":{"skill":"sourcey","runner":"graph"},"created_at":"2026-04-28T01:00:00.000Z","hash":"sha256:start","size_bytes":2,"parent_artifact_id":null,"receipt_id":null,"redacted":false}}}"#,
                r#"{"entry":{"type":"run_event","version":"1","data":{"kind":"step_waiting_resolution","status":"waiting","step_id":"discover","detail":{"request_ids":["agent_step.test-step.output"],"resolution_kinds":["agent_act"],"step_ids":["discover"],"step_labels":["inspect repo"],"inputs":{},"selected_runner":"agent-step"}},"meta":{"artifact_id":"ax_wait","run_id":"gx_needs_agent_oracle","step_id":"discover","producer":{"skill":"sourcey","runner":"graph"},"created_at":"2026-04-28T01:00:00.000Z","hash":"sha256:wait","size_bytes":2,"parent_artifact_id":null,"receipt_id":null,"redacted":false}}}"#
            ),
        )?;
        let oracle: CliParityOracle = serde_json::from_str(include_str!(
            "../../../fixtures/cli-parity/cases/oracle.json"
        ))
        .map_err(|error| io::Error::other(error.to_string()))?;
        let execute_case = oracle
            .cases
            .iter()
            .find(|case| case.id == "history.execute")
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "missing history.execute oracle case",
                )
            })?;

        let mut env = BTreeMap::new();
        env.insert("RUNX_CWD".to_owned(), temp.to_string_lossy().to_string());
        let result = run_history_command(
            &[
                "history".into(),
                "--receipt-dir".into(),
                receipt_dir.into_os_string(),
                "--json".into(),
            ],
            &env,
            &temp,
        )
        .map_err(|error| io::Error::other(error.to_string()))?;
        let output: HistoryOutput = serde_json::from_str(&result.output)
            .map_err(|error| io::Error::other(error.to_string()))?;
        let first_pending_run = output.pending_runs.first().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "history output has no pending run",
            )
        })?;

        assert_eq!(
            output.pending_runs.len(),
            execute_case.expect.pending_runs as usize
        );
        assert_eq!(
            first_pending_run.id,
            execute_case.expect.first_pending_run_id
        );
        assert_eq!(
            first_pending_run.status,
            execute_case.expect.first_pending_run_status
        );
        assert_eq!(
            first_pending_run.selected_runner,
            Some("agent-step".to_owned())
        );
        Ok(())
    }

    #[derive(serde::Deserialize)]
    struct CliParityOracle {
        cases: Vec<CliParityCase>,
    }

    #[derive(serde::Deserialize)]
    struct CliParityCase {
        id: String,
        #[serde(default)]
        expect: CliParityExpectation,
    }

    #[derive(Default, serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct CliParityExpectation {
        #[serde(default)]
        pending_runs: u64,
        #[serde(default)]
        first_pending_run_id: String,
        #[serde(default)]
        first_pending_run_status: String,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct HistoryOutput {
        pending_runs: Vec<HistoryPendingRun>,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct HistoryPendingRun {
        id: String,
        status: String,
        selected_runner: Option<String>,
    }

    fn tempfile_dir() -> Result<PathBuf, io::Error> {
        let path = std::env::temp_dir().join(format!(
            "runx-cli-history-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| io::Error::other(error.to_string()))?
                .as_nanos()
        ));
        fs::create_dir_all(&path)?;
        Ok(path)
    }
}
