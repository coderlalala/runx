use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use runx_contracts::JsonValue;
use runx_runtime::orchestrator::LocalCredentialDescriptor;
use runx_runtime::{LocalOrchestrator, SkillRunRequest};

#[derive(Debug, PartialEq)]
pub struct SkillPlan {
    pub skill_path: PathBuf,
    pub receipt_dir: Option<PathBuf>,
    pub run_id: Option<String>,
    pub answers: Option<PathBuf>,
    pub json: bool,
    pub inputs: BTreeMap<String, JsonValue>,
    /// One-shot, per-run local credential supplied via `--credential` and
    /// `--secret-env`. Never persisted; redacted by the runtime.
    pub local_credential: Option<LocalCredentialDescriptor>,
}

pub fn parse_skill_plan(args: &[OsString]) -> Result<SkillPlan, String> {
    let mut state = SkillParseState::default();
    let mut index = 1;

    while index < args.len() {
        index = parse_skill_arg(args, index, &mut state)?;
        index += 1;
    }

    let local_credential = finalize_local_credential(&state)?;

    let Some(skill_path) = state.skill_path else {
        return Err("runx skill requires a skill package path".to_owned());
    };
    if state.answers.is_some() && state.run_id.is_none() {
        return Err("runx skill --answers requires --run-id".to_owned());
    }
    if state.run_id.is_some() && state.answers.is_none() {
        return Err("runx skill --run-id requires --answers".to_owned());
    }

    Ok(SkillPlan {
        skill_path,
        receipt_dir: state.receipt_dir,
        run_id: state.run_id,
        answers: state.answers,
        json: state.json,
        inputs: state.inputs,
        local_credential,
    })
}

#[derive(Default)]
struct SkillParseState {
    skill_path: Option<PathBuf>,
    receipt_dir: Option<PathBuf>,
    run_id: Option<String>,
    answers: Option<PathBuf>,
    json: bool,
    inputs: BTreeMap<String, JsonValue>,
    credential: Option<CredentialBinding>,
    secret_env: Option<(String, String)>,
}

/// Non-secret binding metadata parsed from `--credential`.
struct CredentialBinding {
    provider: String,
    auth_mode: String,
    material_ref: String,
    scopes: Vec<String>,
}

/// Parse `--credential <provider>:<auth_mode>:<material_ref>[:<scope,scope>]`.
fn parse_credential_binding(value: &str) -> Result<CredentialBinding, String> {
    let mut parts = value.splitn(4, ':');
    let provider = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(|| {
            "runx skill --credential requires <provider>:<auth_mode>:<material_ref>".to_owned()
        })?;
    let auth_mode = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(|| {
            "runx skill --credential requires <provider>:<auth_mode>:<material_ref>".to_owned()
        })?;
    let material_ref = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(|| {
            "runx skill --credential requires <provider>:<auth_mode>:<material_ref>".to_owned()
        })?;
    let scopes = parts
        .next()
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|scope| !scope.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();
    Ok(CredentialBinding {
        provider: provider.to_owned(),
        auth_mode: auth_mode.to_owned(),
        material_ref: material_ref.to_owned(),
        scopes,
    })
}

/// Parse `--secret-env <ENV=VALUE>` into an env var name and secret value.
fn parse_secret_env(value: &str) -> Result<(String, String), String> {
    let (name, secret) = value
        .split_once('=')
        .ok_or_else(|| "runx skill --secret-env requires <ENV_VAR>=<value>".to_owned())?;
    if name.is_empty() {
        return Err("runx skill --secret-env requires a non-empty env var name".to_owned());
    }
    Ok((name.to_owned(), secret.to_owned()))
}

/// Build the per-run local credential descriptor from the parsed flags.
///
/// `--secret-env` is required to provision a credential (it carries the env var
/// and the secret); `--credential` supplies the non-secret binding metadata.
fn finalize_local_credential(
    state: &SkillParseState,
) -> Result<Option<LocalCredentialDescriptor>, String> {
    match (&state.credential, &state.secret_env) {
        (None, None) => Ok(None),
        (Some(_), None) => {
            Err("runx skill --credential requires --secret-env <ENV>=<value>".to_owned())
        }
        (binding, Some((env_var, secret))) => {
            let binding = binding.as_ref().ok_or_else(|| {
                "runx skill --secret-env requires --credential <provider>:<auth_mode>:<material_ref>"
                    .to_owned()
            })?;
            Ok(Some(LocalCredentialDescriptor {
                provider: binding.provider.clone(),
                auth_mode: binding.auth_mode.clone(),
                env_var: env_var.clone(),
                material_ref: binding.material_ref.clone(),
                scopes: binding.scopes.clone(),
                secret: secret.clone(),
            }))
        }
    }
}

fn parse_skill_arg(
    args: &[OsString],
    mut index: usize,
    state: &mut SkillParseState,
) -> Result<usize, String> {
    let token = string_arg(args, index)?;
    if is_retired_skill_option(&token) {
        return Err(
            "retired runx skill receipt option is not supported; use --receipt-dir".to_owned(),
        );
    }
    match token.as_str() {
        value if value.starts_with("--receipt-dir=") => {
            state.receipt_dir = Some(PathBuf::from(value.trim_start_matches("--receipt-dir=")));
        }
        "--receipt-dir" => {
            index += 1;
            state.receipt_dir = Some(PathBuf::from(string_arg(args, index)?));
        }
        value if value.starts_with("--run-id=") => {
            state.run_id = Some(value.trim_start_matches("--run-id=").to_owned());
        }
        "--run-id" => {
            index += 1;
            state.run_id = Some(string_arg(args, index)?);
        }
        value if value.starts_with("--answers=") => {
            state.answers = Some(PathBuf::from(value.trim_start_matches("--answers=")));
        }
        "--answers" => {
            index += 1;
            state.answers = Some(PathBuf::from(string_arg(args, index)?));
        }
        value if value.starts_with("--credential=") => {
            state.credential = Some(parse_credential_binding(
                value.trim_start_matches("--credential="),
            )?);
        }
        "--credential" => {
            index += 1;
            state.credential = Some(parse_credential_binding(&string_arg(args, index)?)?);
        }
        value if value.starts_with("--secret-env=") => {
            state.secret_env = Some(parse_secret_env(value.trim_start_matches("--secret-env="))?);
        }
        "--secret-env" => {
            index += 1;
            state.secret_env = Some(parse_secret_env(&string_arg(args, index)?)?);
        }
        "--json" => state.json = true,
        "--non-interactive" => {}
        value if value.starts_with("--") => {
            index = parse_skill_input_arg(args, index, value, &mut state.inputs)?;
        }
        value => {
            if state.skill_path.is_some() {
                return Err(format!("unexpected runx skill argument {value}"));
            }
            state.skill_path = Some(PathBuf::from(value));
        }
    }
    Ok(index)
}

fn parse_skill_input_arg(
    args: &[OsString],
    mut index: usize,
    token: &str,
    inputs: &mut BTreeMap<String, JsonValue>,
) -> Result<usize, String> {
    if token.contains('=') {
        let (key, value) = token.split_once('=').ok_or_else(|| {
            "runx skill argument must use --name value or --name=value".to_owned()
        })?;
        inputs.insert(
            key.trim_start_matches("--").replace('-', "_"),
            parse_cli_value(value),
        );
    } else {
        let key = token.trim_start_matches("--").replace('-', "_");
        index += 1;
        inputs.insert(key, parse_cli_value(&string_arg(args, index)?));
    }
    Ok(index)
}

fn is_retired_skill_option(token: &str) -> bool {
    let Some(flag) = token.strip_prefix("--") else {
        return false;
    };
    let name = flag.split_once('=').map_or(flag, |(name, _value)| name);
    name == "receipt" || name == retired_receipt_dir_option_name()
}

fn retired_receipt_dir_option_name() -> String {
    ["receipt", "Dir"].concat()
}

pub fn run_native_skill(plan: SkillPlan) -> ExitCode {
    let request = SkillRunRequest {
        skill_path: plan.skill_path,
        receipt_dir: plan.receipt_dir,
        run_id: plan.run_id,
        answers_path: plan.answers,
        inputs: plan.inputs,
        env: env::vars().collect(),
        cwd: env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        local_credential: plan.local_credential,
    };
    match LocalOrchestrator.run_skill(&request) {
        Ok(result) => {
            let exit_code = skill_result_exit_code(&result.output);
            write_json_with_exit(&result.output, exit_code)
        }
        Err(error) => {
            let _ignored = writeln!(io::stderr(), "runx: {error}");
            ExitCode::from(1)
        }
    }
}

fn string_arg(args: &[OsString], index: usize) -> Result<String, String> {
    let value = args
        .get(index)
        .ok_or_else(|| "missing value for runx skill argument".to_owned())?;
    value
        .to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| "runx skill arguments must be UTF-8".to_owned())
}

fn parse_cli_value(raw: &str) -> JsonValue {
    serde_json::from_str(raw).unwrap_or_else(|_| JsonValue::String(raw.to_owned()))
}

fn write_json_with_exit(value: &JsonValue, exit_code: ExitCode) -> ExitCode {
    match serde_json::to_string_pretty(value) {
        Ok(json) => {
            let mut stdout = io::stdout().lock();
            let result = stdout
                .write_all(json.as_bytes())
                .and_then(|_| stdout.write_all(b"\n"));
            match result {
                Ok(()) => exit_code,
                Err(_) => ExitCode::from(1),
            }
        }
        Err(error) => {
            let _ignored = writeln!(
                io::stderr(),
                "runx: failed to serialize skill result: {error}"
            );
            ExitCode::from(1)
        }
    }
}

fn skill_result_exit_code(value: &JsonValue) -> ExitCode {
    match value {
        JsonValue::Object(object) => match object.get("status") {
            Some(JsonValue::String(status)) if status == "needs_agent" => ExitCode::from(2),
            _ => ExitCode::SUCCESS,
        },
        _ => ExitCode::SUCCESS,
    }
}
