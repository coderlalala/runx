use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use runx_runtime::export::RunxExportLoadError;
use serde::Serialize;

use crate::cli_args::{os_arg, split_flag};

mod managed;
mod report;
mod shim;

const CLAUDE_MARKER: &str = "runx-export:claude";
const CODEX_MARKER: &str = "runx-export:codex";
const CODEX_RULE_START: &str = "# >>> runx-export start (managed) >>>";
const CODEX_RULE_END: &str = "# <<< runx-export end <<<";
const CODEX_RULE: &str = "prefix_rule(pattern = [\"runx\", \"skill\"], decision = \"allow\", justification = \"runx skill invocations are trusted\")";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportPlan {
    pub target: Target,
    pub refs: Vec<String>,
    pub project: bool,
    pub json: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    Claude,
    Codex,
}

impl Target {
    fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    fn marker(self) -> &'static str {
        match self {
            Self::Claude => CLAUDE_MARKER,
            Self::Codex => CODEX_MARKER,
        }
    }
}

#[derive(Clone, Debug)]
struct GeneratedFile {
    path: PathBuf,
    contents: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExportReport {
    pub target: String,
    pub scope: String,
    pub exported: Vec<ExportedFile>,
    pub pruned: Vec<String>,
    pub rules_file: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExportedFile {
    pub skill: String,
    pub path: String,
}

#[derive(Debug)]
pub enum ExportError {
    InvalidArgs(String),
    Io {
        context: String,
        source: std::io::Error,
    },
    Parse(String),
    Unsupported(String),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidArgs(message) | Self::Parse(message) | Self::Unsupported(message) => {
                formatter.write_str(message)
            }
            Self::Io { context, source } => write!(formatter, "{context}: {source}"),
        }
    }
}

impl std::error::Error for ExportError {}

impl From<RunxExportLoadError> for ExportError {
    fn from(error: RunxExportLoadError) -> Self {
        match error {
            RunxExportLoadError::InvalidArgs(message) => Self::InvalidArgs(message),
            RunxExportLoadError::Io { context, source } => Self::Io { context, source },
            RunxExportLoadError::Parse(message) => Self::Parse(message),
        }
    }
}

// rust-style-allow: long-function because this parser mirrors the flat native
// launcher grammar and keeps all export flags in one auditable pass.
pub fn parse_export_plan(args: &[OsString]) -> Result<ExportPlan, String> {
    let target = match os_arg(args, 1, "export")? {
        "claude" => Target::Claude,
        "codex" => Target::Codex,
        value => {
            return Err(format!(
                "runx export target must be claude or codex, got {value}"
            ));
        }
    };
    let mut refs = Vec::new();
    let mut project = false;
    let mut json = false;
    let mut index = 2;

    while index < args.len() {
        let token = os_arg(args, index, "export")?;
        if !token.starts_with("--") {
            refs.push(token.to_owned());
            index += 1;
            continue;
        }

        let (flag, inline_value) = split_flag(token);
        match flag {
            "--json" => {
                if inline_value.is_some() {
                    return Err("--json does not take a value".to_owned());
                }
                json = true;
            }
            "--project" => {
                if inline_value.is_some() {
                    return Err("--project does not take a value".to_owned());
                }
                project = true;
            }
            _ => return Err(format!("unknown export flag {flag}")),
        }
        index += 1;
    }

    Ok(ExportPlan {
        target,
        refs,
        project,
        json,
    })
}

pub fn run_native_export(plan: ExportPlan) -> ExitCode {
    let env = std::env::vars().collect::<BTreeMap<_, _>>();
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => {
            let _ignored = writeln!(std::io::stderr(), "runx: failed to resolve cwd: {error}");
            return ExitCode::from(1);
        }
    };

    match run_export_command(&plan, &cwd, &env) {
        Ok(report) => report::write_report(&report, plan.json),
        Err(ExportError::InvalidArgs(message)) => {
            let _ignored = writeln!(std::io::stderr(), "runx: {message}");
            ExitCode::from(64)
        }
        Err(error) => {
            let _ignored = writeln!(std::io::stderr(), "runx: {error}");
            ExitCode::from(1)
        }
    }
}

pub fn run_export_command(
    plan: &ExportPlan,
    cwd: &Path,
    env: &BTreeMap<String, String>,
) -> Result<ExportReport, ExportError> {
    validate_export_plan(plan)?;
    let root = canonicalize(cwd, "canonicalizing export root")?;
    if let Some(report) = codex_missing_home_report(plan, cwd, env) {
        return Ok(report);
    }

    let skills = runx_runtime::export::load_export_skills(&root, &plan.refs)?;
    let skill_dir = target_skill_dir(plan.target, plan.project, cwd, env);
    let files = shim::plan_files(plan.target, plan.project, &skills, &skill_dir);
    let pruned = managed::prune_managed_files(plan.target, &skill_dir, &files)?;
    managed::write_files(&files)?;
    let rules_file = if plan.target == Target::Codex && !plan.project {
        Some(managed::merge_codex_rules(&codex_rules_file(env, cwd))?)
    } else {
        None
    };

    Ok(export_report(plan, &files, pruned, rules_file))
}

fn validate_export_plan(plan: &ExportPlan) -> Result<(), ExportError> {
    if plan.target == Target::Codex && plan.project {
        return Err(ExportError::Unsupported(
            "runx export codex --project is not supported until Codex project skill and rules paths are verified".to_owned(),
        ));
    }
    Ok(())
}

fn codex_missing_home_report(
    plan: &ExportPlan,
    cwd: &Path,
    env: &BTreeMap<String, String>,
) -> Option<ExportReport> {
    if plan.target != Target::Codex || plan.project {
        return None;
    }
    let codex_home = home_dir(env, cwd).join(".codex");
    if codex_home.exists() {
        return None;
    }
    Some(ExportReport {
        target: plan.target.as_str().to_owned(),
        scope: scope_name(plan.project).to_owned(),
        exported: Vec::new(),
        pruned: Vec::new(),
        rules_file: None,
        warnings: vec![format!(
            "Codex home {} was not found; no files written",
            display_path(&codex_home)
        )],
    })
}

fn export_report(
    plan: &ExportPlan,
    files: &[GeneratedFile],
    pruned: Vec<String>,
    rules_file: Option<PathBuf>,
) -> ExportReport {
    ExportReport {
        target: plan.target.as_str().to_owned(),
        scope: scope_name(plan.project).to_owned(),
        exported: files
            .iter()
            .map(|file| ExportedFile {
                skill: file
                    .path
                    .parent()
                    .and_then(Path::file_name)
                    .and_then(|name| name.to_str())
                    .unwrap_or("unknown")
                    .to_owned(),
                path: display_path(&file.path),
            })
            .collect(),
        pruned,
        rules_file: rules_file.map(|path| display_path(&path)),
        warnings: Vec::new(),
    }
}

fn target_skill_dir(
    target: Target,
    project: bool,
    cwd: &Path,
    env: &BTreeMap<String, String>,
) -> PathBuf {
    if project {
        return match target {
            Target::Claude => cwd.join(".claude").join("skills"),
            Target::Codex => cwd.join(".codex").join("skills"),
        };
    }
    let home = home_dir(env, cwd);
    match target {
        Target::Claude => home.join(".claude").join("skills"),
        Target::Codex => home.join(".codex").join("skills"),
    }
}

fn codex_rules_file(env: &BTreeMap<String, String>, cwd: &Path) -> PathBuf {
    home_dir(env, cwd)
        .join(".codex")
        .join("rules")
        .join("default.rules")
}

fn canonicalize(path: &Path, context: &str) -> Result<PathBuf, ExportError> {
    fs::canonicalize(path).map_err(|source| ExportError::Io {
        context: format!("{context} {}", display_path(path)),
        source,
    })
}

fn home_dir(env: &BTreeMap<String, String>, cwd: &Path) -> PathBuf {
    env.get("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| cwd.to_path_buf())
}

fn scope_name(project: bool) -> &'static str {
    if project { "project" } else { "global" }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
