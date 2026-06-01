use std::env;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use runx_cli::launcher::{HarnessPlan, LauncherAction, help_text};

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
        LauncherAction::RunHarness(plan) => run_native_harness(plan),
        LauncherAction::RunKernel(plan) => runx_cli::kernel::run_native_kernel(plan),
        LauncherAction::RunPayment(plan) => runx_cli::payment::run_native_payment(plan),
        LauncherAction::RunParser(plan) => runx_cli::parser::run_native_parser(plan),
        LauncherAction::RunConfig(plan) => run_native_config(plan),
        LauncherAction::RunPolicy(plan) => runx_cli::policy::run_native_policy(plan),
        LauncherAction::RunRegistry(plan) => runx_cli::registry::run_native_registry(plan),
        LauncherAction::RunSkill(plan) => runx_cli::skill::run_native_skill(plan),
        LauncherAction::RunDoctor(plan) => runx_cli::doctor::run_native_doctor(plan),
        LauncherAction::RunDev(plan) => runx_cli::dev::run_native_dev(plan),
        LauncherAction::RunTool(plan) => runx_cli::tool::run_native_tool(plan),
        LauncherAction::RunUrlAdd(plan) => runx_cli::url_add::run_native_url_add(plan),
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

fn run_native_harness(plan: HarnessPlan) -> ExitCode {
    if contains_skill_package(&plan.fixture_paths) {
        let [target] = plan.fixture_paths.as_slice() else {
            let _ignored = write_stderr_line(
                "runx harness accepts one skill package, or one or more fixture files, not a mix",
            );
            return ExitCode::from(64);
        };
        return run_inline_harness(Path::new(target), plan.receipt_dir.as_ref());
    }
    run_standalone_harness(plan.fixture_paths)
}

fn run_standalone_harness(fixture_paths: Vec<OsString>) -> ExitCode {
    let mut outputs = Vec::new();
    let orchestrator = runx_cli::runtime::local_orchestrator();
    for fixture_path in fixture_paths {
        let request = runx_runtime::HarnessRunRequest {
            fixture_path: PathBuf::from(fixture_path),
        };
        match orchestrator.run_harness_fixture(&request) {
            Ok(output) => {
                if let Err(error) = runx_cli::runtime::persist_payment_ledger_projection(&output) {
                    let _ignored = write_stderr_line(&format!(
                        "runx: payment ledger projection failed: {error}"
                    ));
                    return ExitCode::from(1);
                }
                outputs.push(
                    match serde_json::to_value(&output.receipt)
                        .and_then(serde_json::from_value::<runx_contracts::JsonValue>)
                    {
                        Ok(value) => value,
                        Err(error) => {
                            let _ignored = write_stderr_line(&format!(
                                "runx: failed to serialize receipt: {error}"
                            ));
                            return ExitCode::from(1);
                        }
                    },
                );
            }
            Err(error) => {
                let _ignored = write_stderr_line(&format!(
                    "runx: native harness replay failed for {}: {error}",
                    request.fixture_path.display()
                ));
                return ExitCode::from(1);
            }
        }
    }
    write_harness_receipts(outputs)
}

fn write_harness_receipts(mut outputs: Vec<runx_contracts::JsonValue>) -> ExitCode {
    let output = if outputs.len() == 1 {
        outputs.pop().unwrap_or(runx_contracts::JsonValue::Null)
    } else {
        runx_contracts::JsonValue::Array(outputs)
    };
    match serde_json::to_string_pretty(&output) {
        Ok(json) => write_stdout_line(&json),
        Err(error) => {
            let _ignored =
                write_stderr_line(&format!("runx: failed to serialize receipt: {error}"));
            ExitCode::from(1)
        }
    }
}

// A skill package (directory or SKILL.md) runs its declared inline
// `harness.cases`; standalone fixture `.yaml` files replay as receipts.
fn contains_skill_package(paths: &[OsString]) -> bool {
    paths.iter().any(|path| is_skill_package(Path::new(path)))
}

// A harness target is a skill package (run its declared inline harness) when it
// is a SKILL.md file, or a directory that actually holds a skill package
// (a SKILL.md or X.yaml). A plain directory of fixture `.yaml` files is NOT a
// skill package and falls through to standalone fixture replay.
fn is_skill_package(path: &Path) -> bool {
    if path.is_dir() {
        return path.join("SKILL.md").exists() || path.join("X.yaml").exists();
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("SKILL.md"))
}

fn run_inline_harness(skill_path: &Path, receipt_dir: Option<&OsString>) -> ExitCode {
    let request = runx_runtime::InlineHarnessRequest {
        skill_path: skill_path.to_path_buf(),
        receipt_dir: receipt_dir.map(PathBuf::from),
    };
    let report = match runx_cli::runtime::local_orchestrator().run_inline_harness(&request) {
        Ok(report) => report,
        Err(error) => {
            let _ignored = write_stderr_line(&format!(
                "runx: inline harness failed for {}: {error}",
                skill_path.display()
            ));
            return ExitCode::from(1);
        }
    };
    let json = match serde_json::to_string_pretty(&report) {
        Ok(json) => json,
        Err(error) => {
            let _ignored = write_stderr_line(&format!(
                "runx: failed to serialize harness summary: {error}"
            ));
            return ExitCode::from(1);
        }
    };
    // The summary (including a `failed` one) is the artifact a caller parses, so
    // always emit it; a `failed` suite still exits non-zero for shell use.
    let _ignored = write_stdout_line(&json);
    if report.status == "failed" {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
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
