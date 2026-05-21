// rust-style-allow: large-file because the launcher binary currently owns
// native command IO, connect rendering, delegation, and exit-code mapping in
// one audited cutover surface.
use std::env;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use runx_cli::launcher::{LauncherAction, help_text};
use runx_runtime::connect::{ConnectGrantStatus, ConnectReadyStatus, ConnectRevokeStatus};
use runx_runtime::{
    HttpConnectGrant, HttpConnectListResponse, HttpConnectReadyResponse, HttpConnectRevokeResponse,
    LocalOrchestrator,
};

fn main() -> ExitCode {
    let args: Vec<OsString> = env::args_os().skip(1).collect();

    match runx_cli::launcher::plan_launcher(args) {
        LauncherAction::Error(message) => {
            let _ignored = write_stderr_line(&format!("runx: {message}"));
            ExitCode::from(64)
        }
        LauncherAction::PrintHelp => write_stdout(&help_text()),
        LauncherAction::PrintVersion => {
            write_stdout_line(&format!("runx-cli {}", env!("CARGO_PKG_VERSION")))
        }
        LauncherAction::RunInit(plan) => runx_cli::scaffold::run_native_init(plan),
        LauncherAction::RunNew(plan) => runx_cli::scaffold::run_native_new(plan),
        LauncherAction::RunHistory(plan) => run_native_history(plan.args),
        LauncherAction::RunList(plan) => run_native_list(plan),
        LauncherAction::RunMcp(plan) => runx_cli::mcp::run_native_mcp(plan),
        LauncherAction::RunHarness(plan) => run_native_harness(PathBuf::from(plan.fixture_path)),
        LauncherAction::RunKernel(plan) => runx_cli::kernel::run_native_kernel(plan),
        LauncherAction::RunConnect(plan) => run_native_connect(plan),
        LauncherAction::RunConfig(plan) => run_native_config(plan),
        LauncherAction::RunPolicy(plan) => runx_cli::policy::run_native_policy(plan),
        LauncherAction::RunRegistry(plan) => runx_cli::registry::run_native_registry(plan),
        LauncherAction::RunSkill(plan) => runx_cli::skill::run_native_skill(plan),
        LauncherAction::RunDoctor(plan) => runx_cli::doctor::run_native_doctor(plan),
        LauncherAction::RunDev(plan) => runx_cli::dev::run_native_dev(plan),
        LauncherAction::RunTool(plan) => runx_cli::tool::run_native_tool(plan),
    }
}

fn run_native_connect(plan: runx_cli::connect::ConnectPlan) -> ExitCode {
    let env_map = env::vars().collect::<std::collections::BTreeMap<_, _>>();
    let options = match runx_runtime::load_connect_options_from_env(&env_map) {
        Ok(options) => options,
        Err(error) => {
            let _ignored = write_stderr_line(&format!("runx: {error}"));
            return ExitCode::from(1);
        }
    };
    let client = match runx_runtime::ConnectClient::new(options, env_map) {
        Ok(client) => client,
        Err(error) => {
            let _ignored = write_stderr_line(&format!("runx: {error}"));
            return ExitCode::from(1);
        }
    };
    let result = match execute_connect_plan(&client, &plan) {
        Ok(result) => result,
        Err(error) => {
            let _ignored = write_stderr_line(&format!("runx: {error}"));
            return ExitCode::from(1);
        }
    };
    if plan.json {
        return write_connect_json(&result);
    }
    write_stdout(&render_connect_result(&plan, &result))
}

fn execute_connect_plan<T, O>(
    client: &runx_runtime::ConnectClient<T, O>,
    plan: &runx_cli::connect::ConnectPlan,
) -> Result<NativeConnectResult, runx_runtime::ConnectError>
where
    T: runx_runtime::connect::HostedTransport,
    O: runx_runtime::ConnectOpener,
{
    match plan.action {
        runx_cli::connect::ConnectAction::List => Ok(NativeConnectResult::List(client.list()?)),
        runx_cli::connect::ConnectAction::Revoke => {
            let grant_id = plan.grant_id.as_deref().unwrap_or_default();
            Ok(NativeConnectResult::Revoke(client.revoke(grant_id)?))
        }
        runx_cli::connect::ConnectAction::Preprovision => {
            let request = runx_runtime::HttpConnectPreprovisionRequest {
                provider: plan.provider.clone().unwrap_or_default(),
                scopes: plan.scopes.clone(),
                scope_family: plan.scope_family.clone(),
                authority_kind: plan.authority_kind.map(runtime_authority_kind),
                target_repo: plan.target_repo.clone(),
                target_locator: plan.target_locator.clone(),
            };
            Ok(NativeConnectResult::Ready(client.preprovision(&request)?))
        }
    }
}

#[derive(serde::Serialize)]
#[serde(untagged)]
enum NativeConnectResult {
    List(HttpConnectListResponse),
    Ready(HttpConnectReadyResponse),
    Revoke(HttpConnectRevokeResponse),
}

fn runtime_authority_kind(
    kind: runx_cli::connect::ConnectAuthorityKind,
) -> runx_runtime::connect::ConnectAuthorityKind {
    match kind {
        runx_cli::connect::ConnectAuthorityKind::ReadOnly => {
            runx_runtime::connect::ConnectAuthorityKind::ReadOnly
        }
        runx_cli::connect::ConnectAuthorityKind::Constructive => {
            runx_runtime::connect::ConnectAuthorityKind::Constructive
        }
        runx_cli::connect::ConnectAuthorityKind::Destructive => {
            runx_runtime::connect::ConnectAuthorityKind::Destructive
        }
    }
}

fn write_connect_json(result: &NativeConnectResult) -> ExitCode {
    match serde_json::to_string_pretty(&ConnectJsonEnvelope {
        status: "success",
        connect: result,
    }) {
        Ok(json) => write_stdout_line(&json),
        Err(error) => {
            let _ignored = write_stderr_line(&format!(
                "runx: failed to serialize connect result: {error}"
            ));
            ExitCode::from(1)
        }
    }
}

#[derive(serde::Serialize)]
struct ConnectJsonEnvelope<'a> {
    status: &'static str,
    connect: &'a NativeConnectResult,
}

fn render_connect_result(
    plan: &runx_cli::connect::ConnectPlan,
    result: &NativeConnectResult,
) -> String {
    match result {
        NativeConnectResult::List(response) => render_connect_list(response),
        NativeConnectResult::Ready(response) => {
            render_connect_grant(plan, &response.grant, ready_status(response.status))
        }
        NativeConnectResult::Revoke(response) => {
            render_connect_grant(plan, &response.grant, revoke_status(response.status))
        }
    }
}

fn render_connect_grant(
    plan: &runx_cli::connect::ConnectPlan,
    grant: &HttpConnectGrant,
    status: &'static str,
) -> String {
    let title = if plan.action == runx_cli::connect::ConnectAction::Revoke {
        "connection revoked"
    } else {
        "connection ready"
    };
    let next = if plan.action == runx_cli::connect::ConnectAction::Revoke {
        "runx connect github"
    } else {
        "runx connect list"
    };
    let mut rows = vec![
        ("provider", grant.provider.clone()),
        ("grant", grant.grant_id.clone()),
    ];
    let scopes = connect_scopes(grant);
    if !scopes.is_empty() {
        rows.push(("scopes", scopes));
    }
    push_optional_row(&mut rows, "family", grant.scope_family.as_deref());
    push_optional_row(
        &mut rows,
        "authority",
        grant.authority_kind.map(connect_authority_kind),
    );
    push_optional_row(&mut rows, "repo", grant.target_repo.as_deref());
    push_optional_row(&mut rows, "locator", grant.target_locator.as_deref());
    rows.push(("next", next.to_owned()));

    let mut lines = vec![String::new(), format!("  ✓  {title}  {status}")];
    lines.extend(render_key_value_rows(&rows));
    lines.push(String::new());
    lines.push(String::new());
    lines.join("\n")
}

fn render_connect_list(result: &HttpConnectListResponse) -> String {
    if result.grants.is_empty() {
        return "\n  No connections yet.\n  start  runx connect github\n\n".to_owned();
    }
    let mut lines = vec![
        String::new(),
        format!("  connections  {} grant(s)", result.grants.len()),
        String::new(),
    ];
    for grant in &result.grants {
        lines.push(format!(
            "  {}  {}  {}",
            connect_status_icon(grant),
            grant.provider,
            grant.grant_id
        ));
        let scopes = connect_scopes(grant);
        if !scopes.is_empty() {
            lines.push(format!("  scopes  {scopes}"));
        }
        push_optional_line(&mut lines, "family", grant.scope_family.as_deref());
        push_optional_line(
            &mut lines,
            "authority",
            grant.authority_kind.map(connect_authority_kind),
        );
        push_optional_line(&mut lines, "repo", grant.target_repo.as_deref());
        push_optional_line(&mut lines, "locator", grant.target_locator.as_deref());
        lines.push(String::new());
    }
    lines.join("\n")
}

fn render_key_value_rows(rows: &[(&str, String)]) -> Vec<String> {
    let width = rows
        .iter()
        .filter(|(_label, value)| !value.is_empty())
        .map(|(label, _value)| label.len())
        .max()
        .unwrap_or(0);
    rows.iter()
        .filter(|(_label, value)| !value.is_empty())
        .map(|(label, value)| format!("  {label:<width$}  {value}"))
        .collect()
}

fn push_optional_row(
    rows: &mut Vec<(&'static str, String)>,
    label: &'static str,
    value: Option<&str>,
) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        rows.push((label, value.to_owned()));
    }
}

fn push_optional_line(lines: &mut Vec<String>, label: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        lines.push(format!("  {label}  {value}"));
    }
}

fn connect_status_icon(grant: &HttpConnectGrant) -> &'static str {
    match grant.status {
        ConnectGrantStatus::Revoked => "✗",
        ConnectGrantStatus::Active => "✓",
    }
}

fn connect_scopes(grant: &HttpConnectGrant) -> String {
    grant.scopes.join(", ")
}

fn connect_authority_kind(kind: runx_runtime::connect::ConnectAuthorityKind) -> &'static str {
    match kind {
        runx_runtime::connect::ConnectAuthorityKind::ReadOnly => "read_only",
        runx_runtime::connect::ConnectAuthorityKind::Constructive => "constructive",
        runx_runtime::connect::ConnectAuthorityKind::Destructive => "destructive",
    }
}

fn ready_status(status: ConnectReadyStatus) -> &'static str {
    match status {
        ConnectReadyStatus::Created => "created",
        ConnectReadyStatus::Unchanged => "unchanged",
    }
}

fn revoke_status(status: ConnectRevokeStatus) -> &'static str {
    match status {
        ConnectRevokeStatus::Revoked => "revoked",
    }
}

fn run_native_history(args: Vec<OsString>) -> ExitCode {
    let cwd = match env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => {
            let _ignored = write_stderr_line(&format!("runx: failed to resolve cwd: {error}"));
            return ExitCode::from(1);
        }
    };
    match runx_cli::history::run_history_command(&args, &runx_cli::history::env_map(), &cwd) {
        Ok(output) => write_stdout(&output.output),
        Err(runx_cli::history::HistoryCliError::InvalidArgs(message)) => {
            let _ignored = write_stderr_line(&format!("runx: {message}"));
            ExitCode::from(64)
        }
        Err(error) => {
            let _ignored = write_stderr_line(&format!("runx: {error}"));
            ExitCode::from(1)
        }
    }
}

fn run_native_list(plan: runx_cli::launcher::ListPlan) -> ExitCode {
    let cwd = match env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => {
            let _ignored = write_stderr_line(&format!("runx: failed to resolve cwd: {error}"));
            return ExitCode::from(1);
        }
    };
    match runx_cli::list::run_list_command(&plan, &cwd) {
        Ok(output) => write_stdout(&output),
        Err(error) => {
            let _ignored = write_stderr_line(&format!("runx: {error}"));
            ExitCode::from(1)
        }
    }
}

fn run_native_config(plan: runx_cli::config::ConfigPlan) -> ExitCode {
    let cwd = match env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => {
            let _ignored = write_stderr_line(&format!("runx: failed to resolve cwd: {error}"));
            return ExitCode::from(1);
        }
    };
    match runx_cli::config::run_config_command(&plan, &runx_cli::history::env_map(), &cwd) {
        Ok(output) => write_stdout(&output),
        Err(runx_cli::config::ConfigCliError::InvalidArgs(message)) => {
            let _ignored = write_stderr_line(&format!("runx: {message}"));
            ExitCode::from(64)
        }
        Err(error) => {
            let _ignored = write_stderr_line(&format!("runx: {error}"));
            ExitCode::from(1)
        }
    }
}

fn run_native_harness(fixture_path: PathBuf) -> ExitCode {
    let request = runx_runtime::HarnessRunRequest { fixture_path };
    match LocalOrchestrator.run_harness(&request) {
        Ok(output) => match serde_json::to_string_pretty(&output.output) {
            Ok(json) => write_stdout_line(&json),
            Err(error) => {
                let _ignored = write_stderr_line(&format!(
                    "runx: failed to serialize harness receipt: {error}"
                ));
                ExitCode::from(1)
            }
        },
        Err(error) => {
            let _ignored = write_stderr_line(&format!(
                "runx: native harness replay failed for {}: {error}",
                request.fixture_path.display()
            ));
            ExitCode::from(1)
        }
    }
}

fn write_stdout(message: &str) -> ExitCode {
    let mut stdout = io::stdout().lock();
    if stdout.write_all(message.as_bytes()).is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn write_stdout_line(message: &str) -> ExitCode {
    let mut stdout = io::stdout().lock();
    if writeln!(stdout, "{message}").is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn write_stderr_line(message: &str) -> ExitCode {
    let mut stderr = io::stderr().lock();
    if writeln!(stderr, "{message}").is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
