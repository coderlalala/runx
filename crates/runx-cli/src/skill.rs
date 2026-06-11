use std::collections::BTreeMap;
use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use runx_contracts::{JsonObject, JsonValue};
use runx_runtime::SkillRunRequest;
use runx_runtime::orchestrator::LocalCredentialDescriptor;

mod inputs;
mod output;
mod parser;
mod resolver;

use output::{SkillOutputResume, skill_result_exit_code, write_skill_output};
pub use parser::parse_skill_plan;
use resolver::{RegistryTrustState, ResolvedSkillRef, resolve_skill_ref_details};

#[derive(Debug, PartialEq)]
pub struct SkillPlan {
    pub skill_path: PathBuf,
    pub runner: Option<String>,
    pub receipt_dir: Option<PathBuf>,
    pub run_id: Option<String>,
    pub answers: Option<PathBuf>,
    pub registry: Option<String>,
    pub expected_digest: Option<String>,
    pub json: bool,
    pub inputs: BTreeMap<String, JsonValue>,
    /// One-shot, per-run local credential descriptor supplied via
    /// `--credential` and `--secret-env`. The secret is read from the named
    /// process environment variable so raw secret material never appears on
    /// argv. Runner-specific execution validates whether that delivery channel
    /// is supported before any child process starts.
    pub local_credential: Option<LocalCredentialDescriptor>,
}

pub fn run_native_skill(plan: SkillPlan) -> ExitCode {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let env = env::vars().collect();
    let resume_skill_ref = plan.skill_path.to_string_lossy().into_owned();
    let resolved = match resolve_skill_ref_details(
        &plan.skill_path,
        &cwd,
        resolver::SkillResolverOptions {
            env: &env,
            registry: plan.registry.as_deref(),
            expected_digest: plan.expected_digest.as_deref(),
        },
    ) {
        Ok(skill_path) => skill_path,
        Err(error) => {
            return write_skill_failure(&error.to_string(), plan.json, "skill_error", 1, None);
        }
    };
    let skill_path = resolved.runnable_path.clone();
    let resume = SkillOutputResume {
        skill_ref: Some(&resume_skill_ref),
        selected_runner: plan.runner.as_deref(),
        receipt_dir: plan.receipt_dir.as_deref(),
        answers_path: plan.answers.as_deref(),
    };
    let request = SkillRunRequest {
        skill_path,
        receipt_dir: plan.receipt_dir.clone(),
        run_id: plan.run_id.clone(),
        answers_path: plan.answers.clone(),
        inputs: plan.inputs,
        env,
        cwd,
        local_credential: plan.local_credential,
    };
    let orchestrator = crate::runtime::local_orchestrator();
    let result = match plan.runner.as_deref() {
        Some(runner) => orchestrator.run_skill_with_runner(&request, runner),
        None => orchestrator.run_skill(&request),
    };
    match result {
        Ok(mut result) => {
            attach_registry_provenance(&mut result.output, &resolved);
            let exit_code = skill_result_exit_code(&result.output);
            write_skill_output(&result.output, plan.json, exit_code, resume)
        }
        Err(error) => write_skill_failure(
            &error.to_string(),
            plan.json,
            "skill_error",
            1,
            registry_provenance(&resolved),
        ),
    }
}

fn attach_registry_provenance(output: &mut JsonValue, resolved: &ResolvedSkillRef) {
    let Some(provenance) = registry_provenance(resolved) else {
        return;
    };
    let JsonValue::Object(object) = output else {
        return;
    };
    object.insert(
        "registry_provenance".to_owned(),
        JsonValue::Object(provenance),
    );
}

fn registry_provenance(resolved: &ResolvedSkillRef) -> Option<JsonObject> {
    let skill_id = resolved.skill_id.as_ref()?;
    let mut provenance = JsonObject::new();
    provenance.insert("skill_id".to_owned(), JsonValue::String(skill_id.clone()));
    insert_optional(&mut provenance, "version", resolved.version.as_ref());
    insert_optional(&mut provenance, "digest", resolved.digest.as_ref());
    insert_optional(
        &mut provenance,
        "profile_digest",
        resolved.profile_digest.as_ref(),
    );
    insert_optional(
        &mut provenance,
        "registry_source",
        resolved.registry_source.as_ref(),
    );
    insert_optional(
        &mut provenance,
        "registry_source_fingerprint",
        resolved.registry_source_fingerprint.as_ref(),
    );
    insert_optional(&mut provenance, "trust_tier", resolved.trust_tier.as_ref());
    insert_optional(
        &mut provenance,
        "registry_key_id",
        resolved.registry_key_id.as_ref(),
    );
    if matches!(
        resolved.trust_state.as_ref(),
        Some(RegistryTrustState::Trusted)
    ) {
        provenance.insert(
            "trust_state".to_owned(),
            JsonValue::String("trusted".to_owned()),
        );
    }
    Some(provenance)
}

fn insert_optional(object: &mut JsonObject, key: &str, value: Option<&String>) {
    if let Some(value) = value {
        object.insert(key.to_owned(), JsonValue::String(value.clone()));
    }
}

fn write_skill_failure(
    message: &str,
    json: bool,
    code: &str,
    exit_code: u8,
    provenance: Option<JsonObject>,
) -> ExitCode {
    if json {
        let output = skill_json_failure_output(message, code, provenance);
        return crate::cli_io::write_stdout_code(&output, exit_code);
    }
    let _ignored = writeln!(io::stderr(), "runx: {message}");
    ExitCode::from(exit_code)
}

fn skill_json_failure_output(message: &str, code: &str, provenance: Option<JsonObject>) -> String {
    let mut error = JsonObject::new();
    error.insert("message".to_owned(), JsonValue::String(message.to_owned()));
    error.insert("code".to_owned(), JsonValue::String(code.to_owned()));
    let mut output = JsonObject::new();
    output.insert("status".to_owned(), JsonValue::String("failure".to_owned()));
    output.insert("error".to_owned(), JsonValue::Object(error));
    if let Some(provenance) = provenance {
        output.insert(
            "registry_provenance".to_owned(),
            JsonValue::Object(provenance),
        );
    }
    serde_json::to_string_pretty(&JsonValue::Object(output))
        .map(|json| format!("{json}\n"))
        .unwrap_or_else(|_| crate::launcher::json_failure_output(message, code))
}
