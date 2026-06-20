use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use runx_contracts::JsonValue;
use runx_runtime::orchestrator::LocalCredentialDescriptor;
use runx_runtime::{resolve_path_from_user_input, resolve_runx_home_dir};
use serde::Deserialize;

use super::SkillPlan;
use super::inputs::{parse_direct_input_arg, parse_input_arg};

pub fn parse_skill_plan(args: &[OsString]) -> Result<SkillPlan, String> {
    let mut state = SkillParseState::default();
    let mut index = 1;

    while index < args.len() {
        index = parse_skill_arg(args, index, &mut state)?;
        index += 1;
    }

    let env = env::vars().collect();
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let local_credential = finalize_local_credential(&state, &env, &cwd)?;

    let Some(skill_path) = state.skill_path.as_ref() else {
        return Err("runx skill requires a skill package path".to_owned());
    };
    reject_resolver_flags_for_skill_management_action(skill_path, &state)?;
    let skill_path = skill_path.clone();
    if state.answers.is_some() && state.run_id.is_none() {
        return Err("runx skill --answers requires --run-id".to_owned());
    }
    if state.run_id.is_some() && state.answers.is_none() {
        return Err("runx skill --run-id requires --answers".to_owned());
    }

    Ok(SkillPlan {
        skill_path,
        runner: state.runner,
        receipt_dir: state.receipt_dir,
        run_id: state.run_id,
        answers: state.answers,
        registry: state.registry,
        expected_digest: state.expected_digest,
        json: state.json,
        inputs: state.inputs,
        local_credential,
    })
}

#[derive(Default)]
struct SkillParseState {
    skill_path: Option<PathBuf>,
    runner: Option<String>,
    receipt_dir: Option<PathBuf>,
    run_id: Option<String>,
    answers: Option<PathBuf>,
    registry: Option<String>,
    expected_digest: Option<String>,
    json: bool,
    inputs: BTreeMap<String, JsonValue>,
    credential: Option<CredentialBinding>,
    credential_profile: Option<String>,
    secret_env: Option<(String, String)>,
}

struct CredentialBinding {
    provider: String,
    auth_mode: String,
    material_ref: String,
    scopes: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialProfilesFile {
    profiles: BTreeMap<String, CredentialProfile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialProfile {
    credential: String,
    secret_env: String,
}

fn parse_credential_binding(value: &str) -> Result<CredentialBinding, String> {
    let (provider, rest) = value.split_once(':').ok_or_else(credential_usage_error)?;
    let provider = non_empty_credential_part(provider)?;
    let (auth_mode, rest) = rest.split_once(':').ok_or_else(credential_usage_error)?;
    let auth_mode = non_empty_credential_part(auth_mode)?;
    let (material_ref, scopes) = split_material_ref_and_scopes(rest)?;
    Ok(CredentialBinding {
        provider: provider.to_owned(),
        auth_mode: auth_mode.to_owned(),
        material_ref,
        scopes,
    })
}

fn split_material_ref_and_scopes(value: &str) -> Result<(String, Vec<String>), String> {
    let value = non_empty_credential_part(value)?;
    let Some(index) = value.rfind(':') else {
        return Ok((value.to_owned(), Vec::new()));
    };
    if value[index..].starts_with("://") {
        return Ok((value.to_owned(), Vec::new()));
    }
    let material_ref = non_empty_credential_part(&value[..index])?.to_owned();
    let scopes = value[index + 1..]
        .split(',')
        .map(str::trim)
        .filter(|scope| !scope.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    Ok((material_ref, scopes))
}

fn non_empty_credential_part(value: &str) -> Result<&str, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(credential_usage_error());
    }
    Ok(value)
}

fn credential_usage_error() -> String {
    "runx skill --credential requires <provider>:<auth_mode>:<material_ref>".to_owned()
}

fn parse_secret_env(value: &str) -> Result<(String, String), String> {
    parse_secret_env_from(value, |name| env::var(name).ok())
}

fn parse_secret_env_from(
    value: &str,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<(String, String), String> {
    if value.contains('=') {
        return Err(
            "runx skill --secret-env accepts an environment variable name, not an inline value"
                .to_owned(),
        );
    }
    let name = value.trim();
    if name.is_empty() {
        return Err("runx skill --secret-env requires a non-empty env var name".to_owned());
    }
    let secret = lookup(name)
        .ok_or_else(|| format!("runx skill --secret-env env var '{name}' is not set"))?;
    if secret.trim().is_empty() {
        return Err("runx skill --secret-env requires a non-empty secret value".to_owned());
    }
    Ok((name.to_owned(), secret.to_owned()))
}

fn finalize_local_credential(
    state: &SkillParseState,
    env: &BTreeMap<String, String>,
    cwd: &Path,
) -> Result<Option<LocalCredentialDescriptor>, String> {
    if let Some(profile) = state.credential_profile.as_ref() {
        if state.credential.is_some() || state.secret_env.is_some() {
            return Err(
                "runx skill --credential-profile cannot be combined with --credential or --secret-env"
                    .to_owned(),
            );
        }
        let profile = load_credential_profile(profile, env, cwd)?;
        let credential = parse_credential_binding(&profile.credential)?;
        let secret_env = parse_secret_env_from(&profile.secret_env, |name| env.get(name).cloned())?;
        return Ok(Some(local_credential_descriptor(&credential, &secret_env)));
    }
    match (&state.credential, &state.secret_env) {
        (None, None) => Ok(None),
        (Some(_), None) => {
            Err("runx skill --credential requires --secret-env <ENV_VAR>".to_owned())
        }
        (binding, Some((env_var, secret))) => {
            let binding = binding.as_ref().ok_or_else(|| {
                "runx skill --secret-env requires --credential <provider>:<auth_mode>:<material_ref>"
                    .to_owned()
            })?;
            Ok(Some(local_credential_descriptor(
                binding,
                &(env_var.clone(), secret.clone()),
            )))
        }
    }
}

fn local_credential_descriptor(
    binding: &CredentialBinding,
    secret_env: &(String, String),
) -> LocalCredentialDescriptor {
    LocalCredentialDescriptor {
        provider: binding.provider.clone(),
        auth_mode: binding.auth_mode.clone(),
        env_var: secret_env.0.clone(),
        material_ref: binding.material_ref.clone(),
        scopes: binding.scopes.clone(),
        secret: secret_env.1.clone(),
    }
}

fn load_credential_profile(
    profile_name: &str,
    env: &BTreeMap<String, String>,
    cwd: &Path,
) -> Result<CredentialProfile, String> {
    let profile_name = profile_name.trim();
    if profile_name.is_empty() {
        return Err("runx skill --credential-profile requires a non-empty name".to_owned());
    }
    let paths = credential_profile_paths(env, cwd);
    for path in &paths {
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "runx skill could not read credential profile file {}: {error}",
                    path.display()
                ));
            }
        };
        let parsed: CredentialProfilesFile = serde_json::from_str(&contents).map_err(|error| {
            format!(
                "runx skill credential profile file {} is invalid JSON: {error}",
                path.display()
            )
        })?;
        if let Some(profile) = parsed.profiles.into_iter().find_map(|(name, profile)| {
            if name == profile_name {
                Some(profile)
            } else {
                None
            }
        }) {
            return Ok(profile);
        }
    }
    let searched = paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!(
        "runx skill credential profile '{profile_name}' was not found; searched {searched}"
    ))
}

fn credential_profile_paths(env: &BTreeMap<String, String>, cwd: &Path) -> Vec<PathBuf> {
    if let Some(path) = env
        .get("RUNX_CREDENTIAL_PROFILES")
        .filter(|value| !value.trim().is_empty())
    {
        return vec![resolve_path_from_user_input(path, env, cwd, true)];
    }
    let mut paths = Vec::new();
    if let Some(project_dir) = env
        .get("RUNX_PROJECT_DIR")
        .filter(|value| !value.trim().is_empty())
        .map(|value| resolve_path_from_user_input(value, env, cwd, true))
        .or_else(|| nearest_project_runx_dir(cwd))
    {
        paths.push(project_dir.join("credentials.json"));
    }
    paths.push(resolve_runx_home_dir(env, cwd).join("credentials.json"));
    paths.dedup();
    paths
}

fn nearest_project_runx_dir(cwd: &Path) -> Option<PathBuf> {
    cwd.ancestors()
        .map(|ancestor| ancestor.join(".runx"))
        .find(|candidate| candidate.is_dir())
}

fn reject_resolver_flags_for_skill_management_action(
    skill_path: &Path,
    state: &SkillParseState,
) -> Result<(), String> {
    if state.registry.is_none() && state.expected_digest.is_none() {
        return Ok(());
    }
    if !is_skill_management_action(skill_path) {
        return Ok(());
    }
    Err("runx skill --registry and --digest are only supported when running a skill ref".to_owned())
}

fn is_skill_management_action(skill_path: &Path) -> bool {
    if skill_path.components().count() != 1 {
        return false;
    }
    matches!(
        skill_path.to_str(),
        Some("add" | "inspect" | "publish" | "search" | "validate")
    )
}

// rust-style-allow: long-function because this is the single skill-flag dispatch
// match (--receipt-dir/--run-id/--answers/--json/--credential and positionals);
// splitting the arms would scatter the CLI parse contract.
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
        value if value.starts_with("-R=") => {
            state.receipt_dir = Some(PathBuf::from(value.trim_start_matches("-R=")));
        }
        value if value.starts_with("--receipts=") => {
            state.receipt_dir = Some(PathBuf::from(value.trim_start_matches("--receipts=")));
        }
        "--receipt-dir" => {
            index += 1;
            state.receipt_dir = Some(PathBuf::from(string_arg(args, index)?));
        }
        "--receipts" => {
            index += 1;
            state.receipt_dir = Some(PathBuf::from(string_arg(args, index)?));
        }
        "-R" => {
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
        value if value.starts_with("--runner=") => {
            state.runner = Some(non_empty_flag_value(
                "--runner",
                value.trim_start_matches("--runner="),
            )?);
        }
        "--runner" => {
            index += 1;
            state.runner = Some(non_empty_flag_value("--runner", &string_arg(args, index)?)?);
        }
        value if value.starts_with("--registry=") => {
            state.registry = Some(non_empty_flag_value(
                "--registry",
                value.trim_start_matches("--registry="),
            )?);
        }
        "--registry" => {
            index += 1;
            state.registry = Some(non_empty_flag_value(
                "--registry",
                &string_arg(args, index)?,
            )?);
        }
        value if value.starts_with("--digest=") => {
            state.expected_digest = Some(non_empty_flag_value(
                "--digest",
                value.trim_start_matches("--digest="),
            )?);
        }
        "--digest" => {
            index += 1;
            state.expected_digest =
                Some(non_empty_flag_value("--digest", &string_arg(args, index)?)?);
        }
        value if value.starts_with("--input=") => {
            index = parse_input_arg(
                args,
                index,
                Some(value.trim_start_matches("--input=")),
                &mut state.inputs,
            )?;
        }
        value if value.starts_with("-i=") => {
            index = parse_input_arg(
                args,
                index,
                Some(value.trim_start_matches("-i=")),
                &mut state.inputs,
            )?;
        }
        "--input" => index = parse_input_arg(args, index, None, &mut state.inputs)?,
        "-i" => index = parse_input_arg(args, index, None, &mut state.inputs)?,
        value if value.starts_with("--credential=") => {
            state.credential = Some(parse_credential_binding(
                value.trim_start_matches("--credential="),
            )?);
        }
        "--credential" => {
            index += 1;
            state.credential = Some(parse_credential_binding(&string_arg(args, index)?)?);
        }
        value if value.starts_with("--credential-profile=") => {
            state.credential_profile = Some(non_empty_flag_value(
                "--credential-profile",
                value.trim_start_matches("--credential-profile="),
            )?);
        }
        value if value.starts_with("--profile=") => {
            state.credential_profile = Some(non_empty_flag_value(
                "--profile",
                value.trim_start_matches("--profile="),
            )?);
        }
        "--credential-profile" => {
            index += 1;
            state.credential_profile = Some(non_empty_flag_value(
                "--credential-profile",
                &string_arg(args, index)?,
            )?);
        }
        "--profile" | "-p" => {
            index += 1;
            state.credential_profile = Some(non_empty_flag_value(
                "--profile",
                &string_arg(args, index)?,
            )?);
        }
        value if value.starts_with("--secret-env=") => {
            state.secret_env = Some(parse_secret_env(value.trim_start_matches("--secret-env="))?);
        }
        "--secret-env" => {
            index += 1;
            state.secret_env = Some(parse_secret_env(&string_arg(args, index)?)?);
        }
        "--json" | "-j" => state.json = true,
        "--non-interactive" => {}
        value if value.starts_with("--") => {
            index = parse_direct_input_arg(args, index, value, &mut state.inputs)?;
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

fn non_empty_flag_value(flag: &str, value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("runx skill {flag} requires a non-empty value"));
    }
    Ok(value.to_owned())
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

fn string_arg(args: &[OsString], index: usize) -> Result<String, String> {
    let value = args
        .get(index)
        .ok_or_else(|| "missing value for runx skill argument".to_owned())?;
    value
        .to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| "runx skill arguments must be UTF-8".to_owned())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{SkillParseState, finalize_local_credential};

    #[test]
    fn credential_profile_resolves_project_descriptor_and_env_secret() -> Result<(), String> {
        let root = unique_temp_dir("runx-credential-profile")?;
        let runx_dir = root.join(".runx");
        fs::create_dir_all(&runx_dir).map_err(|error| error.to_string())?;
        fs::write(
            runx_dir.join("credentials.json"),
            r#"{
  "profiles": {
    "frantic": {
      "credential": "frantic:bearer:local://frantic/internal:frantic.review",
      "secret_env": "INTERNAL_SYNC_SECRET"
    }
  }
}
"#,
        )
        .map_err(|error| error.to_string())?;

        let mut state = SkillParseState::default();
        state.credential_profile = Some("frantic".to_owned());
        let env = [
            (
                "RUNX_PROJECT_DIR".to_owned(),
                runx_dir.to_string_lossy().into_owned(),
            ),
            ("INTERNAL_SYNC_SECRET".to_owned(), "secret-value".to_owned()),
        ]
        .into_iter()
        .collect();
        let credential = finalize_local_credential(&state, &env, &root)?
            .ok_or_else(|| "credential profile did not resolve".to_owned())?;

        assert_eq!(credential.provider, "frantic");
        assert_eq!(credential.auth_mode, "bearer");
        assert_eq!(credential.material_ref, "local://frantic/internal");
        assert_eq!(credential.env_var, "INTERNAL_SYNC_SECRET");
        assert_eq!(credential.secret, "secret-value");
        assert_eq!(credential.scopes, vec!["frantic.review"]);

        fs::remove_dir_all(root).map_err(|error| error.to_string())?;
        Ok(())
    }

    #[test]
    fn credential_profile_rejects_manual_credential_flags() -> Result<(), String> {
        let mut state = SkillParseState::default();
        state.credential_profile = Some("frantic".to_owned());
        state.secret_env = Some(("TOKEN".to_owned(), "secret".to_owned()));
        let error = finalize_local_credential(&state, &Default::default(), &std::env::temp_dir())
            .err()
            .ok_or_else(|| "profile unexpectedly combined with manual flags".to_owned())?;
        assert!(error.contains("cannot be combined"));
        Ok(())
    }

    #[test]
    fn short_human_flags_parse_for_skill_runs() -> Result<(), String> {
        let args = [
            "skill",
            "-p",
            "operator",
            "-i",
            "claim=abc",
            "-R",
            "receipts",
            "-j",
        ]
        .into_iter()
        .map(std::ffi::OsString::from)
        .collect::<Vec<_>>();
        let mut state = SkillParseState::default();
        let mut index = 1;
        while index < args.len() {
            index = super::parse_skill_arg(&args, index, &mut state)?;
            index += 1;
        }
        assert_eq!(state.credential_profile.as_deref(), Some("operator"));
        assert_eq!(
            state.inputs.get("claim"),
            Some(&runx_contracts::JsonValue::String("abc".to_owned()))
        );
        assert_eq!(
            state.receipt_dir.as_deref(),
            Some(std::path::Path::new("receipts"))
        );
        assert!(state.json);
        Ok(())
    }

    #[test]
    fn credential_parser_keeps_uri_material_ref_intact() -> Result<(), String> {
        let binding = super::parse_credential_binding(
            "frantic:bearer:local://frantic/internal:frantic.review,frantic.write",
        )?;
        assert_eq!(binding.provider, "frantic");
        assert_eq!(binding.auth_mode, "bearer");
        assert_eq!(binding.material_ref, "local://frantic/internal");
        assert_eq!(binding.scopes, vec!["frantic.review", "frantic.write"]);
        Ok(())
    }

    fn unique_temp_dir(name: &str) -> Result<std::path::PathBuf, String> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{name}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).map_err(|error| error.to_string())?;
        Ok(path)
    }
}
