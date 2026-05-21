// rust-style-allow: large-file - launcher argument parity is centralized for CLI routing tests.
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use crate::config::ConfigPlan;
use crate::connect::ConnectPlan;
use crate::kernel::{KernelInputSource, KernelPlan};
use crate::mcp::McpPlan;
use crate::policy::{PolicyAction, PolicyPlan};
use crate::registry::{RegistryAction, RegistryPlan};
use crate::skill::SkillPlan;

#[derive(Debug, PartialEq)]
pub enum LauncherAction {
    Error(String),
    RunDev(DevPlan),
    RunDoctor(DoctorPlan),
    RunInit(InitPlan),
    RunList(ListPlan),
    RunMcp(McpPlan),
    RunNew(NewPlan),
    RunHistory(HistoryPlan),
    RunHarness(HarnessPlan),
    RunKernel(KernelPlan),
    RunConnect(ConnectPlan),
    RunConfig(ConfigPlan),
    RunPolicy(PolicyPlan),
    RunRegistry(RegistryPlan),
    RunSkill(SkillPlan),
    RunTool(ToolPlan),
    PrintHelp,
    PrintVersion,
}

#[derive(Debug, Eq, PartialEq)]
pub struct DevPlan {
    pub root: Option<PathBuf>,
    pub lane: Option<String>,
    pub json: bool,
}

#[derive(Debug, Eq, PartialEq)]
pub struct HarnessPlan {
    pub fixture_path: OsString,
}

#[derive(Debug, Eq, PartialEq)]
pub struct HistoryPlan {
    pub args: Vec<OsString>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct DoctorPlan {
    pub path: Option<PathBuf>,
    pub json: bool,
}

#[derive(Debug, Eq, PartialEq)]
pub struct ListPlan {
    pub kind: ListKind,
    pub filter: FilterMode,
    pub json: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FilterMode {
    #[default]
    All,
    OkOnly,
    InvalidOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ListKind {
    All,
    Tools,
    Skills,
    Graphs,
    Packets,
    Overlays,
}

#[derive(Debug, Eq, PartialEq)]
pub struct NewPlan {
    pub name: String,
    pub directory: Option<PathBuf>,
    pub json: bool,
}

#[derive(Debug, Eq, PartialEq)]
pub struct InitPlan {
    pub global: bool,
    pub prefetch_official: bool,
    pub json: bool,
}

#[derive(Debug, Eq, PartialEq)]
pub struct ToolPlan {
    pub action: ToolAction,
    pub path: Option<PathBuf>,
    pub ref_or_query: Option<String>,
    pub all: bool,
    pub source: Option<String>,
    pub json: bool,
}

#[derive(Debug, Eq, PartialEq)]
pub enum ToolAction {
    Build,
    Search,
    Inspect,
}

// rust-style-allow: long-function because launcher routing is the cutover gate:
// every native command branch and fail-closed decision is reviewed here.
pub fn plan_launcher(args: Vec<OsString>) -> LauncherAction {
    if args.is_empty() || single_arg_is(&args, "--help") || single_arg_is(&args, "-h") {
        return LauncherAction::PrintHelp;
    }

    if single_arg_is(&args, "--version") || single_arg_is(&args, "-V") {
        return LauncherAction::PrintVersion;
    }

    if first_arg_is(&args, "harness") {
        return native_harness_plan(&args);
    }

    if first_arg_is(&args, "connect") {
        return crate::connect::parse_connect_plan(&args)
            .map_or_else(LauncherAction::Error, LauncherAction::RunConnect);
    }

    if first_arg_is(&args, "config") {
        return crate::config::parse_config_plan(&args)
            .map_or_else(LauncherAction::Error, LauncherAction::RunConfig);
    }

    if first_arg_is(&args, "policy") {
        return parse_policy_plan(&args)
            .map_or_else(LauncherAction::Error, LauncherAction::RunPolicy);
    }

    if first_arg_is(&args, "kernel") {
        return parse_kernel_plan(&args)
            .map_or_else(LauncherAction::Error, LauncherAction::RunKernel);
    }

    if first_arg_is(&args, "doctor") {
        return parse_doctor_plan(&args)
            .map_or_else(LauncherAction::Error, LauncherAction::RunDoctor);
    }

    if first_arg_is(&args, "dev") {
        return parse_dev_plan(&args).map_or_else(LauncherAction::Error, LauncherAction::RunDev);
    }

    if first_arg_is(&args, "list") {
        return parse_list_plan(&args).map_or_else(LauncherAction::Error, LauncherAction::RunList);
    }

    if first_arg_is(&args, "new") {
        return parse_new_plan(&args).map_or_else(LauncherAction::Error, LauncherAction::RunNew);
    }

    if first_arg_is(&args, "init") {
        return parse_init_plan(&args).map_or_else(LauncherAction::Error, LauncherAction::RunInit);
    }

    if first_arg_is(&args, "history") {
        return LauncherAction::RunHistory(HistoryPlan { args });
    }

    if first_arg_is(&args, "mcp") {
        if mcp_runner_before_serve(&args) {
            return LauncherAction::Error(
                "runx mcp --runner must follow the serve subcommand".to_owned(),
            );
        }
        return crate::mcp::parse_mcp_plan(&args)
            .map_or_else(LauncherAction::Error, LauncherAction::RunMcp);
    }

    if first_arg_is(&args, "tool") {
        return parse_tool_plan(&args).map_or_else(LauncherAction::Error, LauncherAction::RunTool);
    }

    if first_arg_is(&args, "registry") {
        return parse_registry_plan(&args)
            .map_or_else(LauncherAction::Error, LauncherAction::RunRegistry);
    }

    if first_arg_is(&args, "skill") {
        return crate::skill::parse_skill_plan(&args)
            .map_or_else(LauncherAction::Error, LauncherAction::RunSkill);
    }

    LauncherAction::Error(format!(
        "unknown command {}",
        args.first()
            .and_then(|arg| arg.to_str())
            .unwrap_or("<non-utf8>")
    ))
}

pub fn help_text() -> String {
    "\
runx

Usage:
  runx <command> [args]
  runx --help
  runx --version

Commands:
  runx new <name> [--directory dir] [--json]
  runx init [-g|--global] [--prefetch official] [--json]
  runx history [query] [--skill s] [--status s] [--source s] [--actor a] [--artifact-type t] [--since iso] [--until iso] [--receipt-dir dir] [--json]
  runx list [tools|skills|graphs|packets|overlays] [--ok-only|--invalid-only] [--json]
  runx connect list|revoke <grant-id>|<provider> [--scope scope] [--scope-family family] [--authority-kind read_only|constructive|destructive] [--target-repo owner/repo] [--target-locator locator] [--json]
  runx config set|get|list [agent.provider|agent.model|agent.api_key] [value] [--json]
  runx policy inspect|lint <policy.json> [--json]
  runx kernel eval --input <file|-> --json
  runx doctor [path] [--json]
  runx dev [root] [--lane lane] [--json]
  runx mcp serve <skill-ref...> [--receipt-dir dir]
  runx skill <skill-ref|skill-dir|SKILL.md> [--input k=v] [--receipt-dir dir] [--run-id id] [--answers file] [--json]
  runx harness <fixture.yaml> [--json]
  runx tool build <tool-dir>|--all [--json]
  runx tool search <query> [--source source] [--json]
  runx tool inspect <ref> [--source source] [--json]
  runx registry search|read|resolve|install|publish ... --json
"
    .to_owned()
}

fn single_arg_is(args: &[OsString], expected: &str) -> bool {
    args.len() == 1 && first_arg_is(args, expected)
}

fn first_arg_is(args: &[OsString], expected: &str) -> bool {
    args.first().is_some_and(|arg| arg == OsStr::new(expected))
}

fn mcp_runner_before_serve(args: &[OsString]) -> bool {
    args.iter()
        .skip(1)
        .take_while(|arg| arg.as_os_str() != OsStr::new("serve"))
        .any(|arg| {
            arg.to_str()
                .is_some_and(|token| token == "--runner" || token.starts_with("--runner="))
        })
}

fn native_harness_plan(args: &[OsString]) -> LauncherAction {
    let mut fixture_path = None;
    let mut index = 1;

    while index < args.len() {
        let Some(token) = args.get(index).and_then(|arg| arg.to_str()) else {
            return LauncherAction::Error("harness arguments must be UTF-8".to_owned());
        };

        if !token.starts_with("--") {
            if fixture_path.is_some() {
                return LauncherAction::Error(
                    "runx harness requires exactly one fixture path".to_owned(),
                );
            }
            fixture_path = Some(args[index].clone());
            index += 1;
            continue;
        }

        let (flag, inline_value) = split_flag(token);
        match flag {
            "--json" => {
                if inline_value.is_some() {
                    return LauncherAction::Error("--json does not take a value".to_owned());
                }
                index += 1;
            }
            _ => return LauncherAction::Error(format!("unknown harness flag {flag}")),
        }
    }

    let Some(fixture_path) = fixture_path else {
        return LauncherAction::Error("runx harness requires exactly one fixture path".to_owned());
    };

    LauncherAction::RunHarness(HarnessPlan { fixture_path })
}

fn parse_new_plan(args: &[OsString]) -> Result<NewPlan, String> {
    let mut name = None;
    let mut directory = None;
    let mut json = false;
    let mut positional_directory = None;
    let mut extra_positionals = Vec::new();
    let mut index = 1;

    while index < args.len() {
        let token = os_arg(args, index, "new")?;
        if !token.starts_with("--") {
            if name.is_none() {
                name = Some(token.to_owned());
            } else if positional_directory.is_none() {
                positional_directory = Some(PathBuf::from(token));
            } else {
                extra_positionals.push(token.to_owned());
            }
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
                index += 1;
            }
            "--directory" | "--dir" => {
                let (value, next_index) = flag_value(args, index, flag, inline_value, "new")?;
                directory = Some(PathBuf::from(value));
                index = next_index;
            }
            _ => return Err(format!("unknown new flag {flag}")),
        }
    }

    if !extra_positionals.is_empty() {
        return Err("runx new accepts at most one directory argument".to_owned());
    }

    Ok(NewPlan {
        name: name.ok_or_else(|| "runx new requires a package name".to_owned())?,
        directory: directory.or(positional_directory),
        json,
    })
}

fn parse_init_plan(args: &[OsString]) -> Result<InitPlan, String> {
    let mut global = false;
    let mut prefetch_official = false;
    let mut json = false;
    let mut index = 1;

    while index < args.len() {
        let token = os_arg(args, index, "init")?;
        if token == "-g" {
            global = true;
            index += 1;
            continue;
        }
        if !token.starts_with("--") {
            return Err(format!("unexpected init argument {token}"));
        }

        let (flag, inline_value) = split_flag(token);
        match flag {
            "--json" => {
                if inline_value.is_some() {
                    return Err("--json does not take a value".to_owned());
                }
                json = true;
                index += 1;
            }
            "--global" => {
                if inline_value.is_some() {
                    return Err("--global does not take a value".to_owned());
                }
                global = true;
                index += 1;
            }
            "--prefetch" | "--prefetchOfficial" | "--prefetch-official" => {
                if matches!(inline_value, Some("false") | Some("0")) {
                    prefetch_official = false;
                    index += 1;
                    continue;
                }
                let (value, next_index) = optional_flag_value(args, index, inline_value, "init")?;
                prefetch_official = value.is_none_or(|value| truthy(&value));
                index = next_index;
            }
            _ => return Err(format!("unknown init flag {flag}")),
        }
    }

    Ok(InitPlan {
        global,
        prefetch_official,
        json,
    })
}

fn parse_dev_plan(args: &[OsString]) -> Result<DevPlan, String> {
    let mut root = None;
    let mut lane = None;
    let mut json = false;
    let mut index = 1;

    while index < args.len() {
        let token = os_arg(args, index, "dev")?;
        if !token.starts_with("--") {
            if root.is_some() {
                return Err("runx dev accepts at most one root path".to_owned());
            }
            root = Some(PathBuf::from(token));
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
                index += 1;
            }
            "--lane" => {
                let (value, next_index) = flag_value(args, index, flag, inline_value, "dev")?;
                if value.is_empty() {
                    return Err("--lane must not be empty".to_owned());
                }
                lane = Some(value);
                index = next_index;
            }
            _ => return Err(format!("unknown dev flag {flag}")),
        }
    }

    Ok(DevPlan { root, lane, json })
}

fn parse_doctor_plan(args: &[OsString]) -> Result<DoctorPlan, String> {
    let mut path = None;
    let mut json = false;
    let mut index = 1;

    while index < args.len() {
        let token = os_arg(args, index, "doctor")?;
        if !token.starts_with("--") {
            if path.is_some() {
                return Err("runx doctor accepts at most one path".to_owned());
            }
            path = Some(PathBuf::from(token));
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
                index += 1;
            }
            _ => return Err(format!("unknown doctor flag {flag}")),
        }
    }

    Ok(DoctorPlan { path, json })
}

fn parse_list_plan(args: &[OsString]) -> Result<ListPlan, String> {
    let mut kind = ListKind::All;
    let mut filter = FilterMode::All;
    let mut json = false;
    let mut saw_kind = false;
    let mut index = 1;

    while index < args.len() {
        let token = os_arg(args, index, "list")?;
        if !token.starts_with("--") {
            if saw_kind {
                return Err("runx list accepts at most one kind".to_owned());
            }
            kind = parse_list_kind(token).ok_or_else(|| {
                "runx list kind must be tools, skills, graphs, packets, or overlays".to_owned()
            })?;
            saw_kind = true;
            index += 1;
            continue;
        }

        let (flag, inline_value) = split_flag(token);
        if inline_value.is_some() {
            return Err(format!("{flag} does not take a value"));
        }
        let requested = match flag {
            "--json" => {
                json = true;
                index += 1;
                continue;
            }
            "--ok-only" | "--okOnly" => FilterMode::OkOnly,
            "--invalid-only" | "--invalidOnly" => FilterMode::InvalidOnly,
            _ => return Err(format!("unknown list flag {flag}")),
        };
        if filter != FilterMode::All && filter != requested {
            return Err("runx list accepts either --ok-only or --invalid-only".to_owned());
        }
        filter = requested;
        index += 1;
    }

    Ok(ListPlan { kind, filter, json })
}

fn parse_list_kind(value: &str) -> Option<ListKind> {
    match value {
        "tools" => Some(ListKind::Tools),
        "skills" => Some(ListKind::Skills),
        "graphs" => Some(ListKind::Graphs),
        "packets" => Some(ListKind::Packets),
        "overlays" => Some(ListKind::Overlays),
        _ => None,
    }
}

fn parse_kernel_plan(args: &[OsString]) -> Result<KernelPlan, String> {
    let subcommand = os_arg(args, 1, "kernel")?;
    if subcommand != "eval" {
        return Err(format!("unknown kernel subcommand {subcommand}"));
    }

    let mut input = None;
    let mut json = false;
    let mut index = 2;

    while index < args.len() {
        let token = os_arg(args, index, "kernel")?;
        if !token.starts_with("--") {
            return Err(format!("unexpected kernel eval argument {token}"));
        }

        let (flag, inline_value) = split_flag(token);
        match flag {
            "--json" => {
                if inline_value.is_some() {
                    return Err("--json does not take a value".to_owned());
                }
                json = true;
                index += 1;
            }
            "--input" => {
                if input.is_some() {
                    return Err("runx kernel eval accepts exactly one --input".to_owned());
                }
                let (value, next_index) = flag_value(args, index, flag, inline_value, "kernel")?;
                input = Some(if value == "-" {
                    KernelInputSource::Stdin
                } else {
                    KernelInputSource::Path(PathBuf::from(value))
                });
                index = next_index;
            }
            _ => return Err(format!("unknown kernel eval flag {flag}")),
        }
    }

    if !json {
        return Err("runx kernel eval requires --json".to_owned());
    }

    Ok(KernelPlan {
        input: input.ok_or_else(|| "runx kernel eval requires --input <file|->".to_owned())?,
        json,
    })
}

fn parse_policy_plan(args: &[OsString]) -> Result<PolicyPlan, String> {
    let subcommand = os_arg(args, 1, "policy")?;
    let action = match subcommand {
        "inspect" => PolicyAction::Inspect,
        "lint" => PolicyAction::Lint,
        _ => return Err(format!("unknown policy subcommand {subcommand}")),
    };
    let mut json = false;
    let mut positionals = Vec::new();
    let mut index = 2;

    while index < args.len() {
        let token = os_arg(args, index, "policy")?;
        if !token.starts_with("--") {
            positionals.push(PathBuf::from(token));
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
                index += 1;
            }
            _ => return Err(format!("unknown policy flag {flag}")),
        }
    }

    let [path] = positionals.as_slice() else {
        return Err("runx policy inspect|lint requires exactly one policy path".to_owned());
    };
    Ok(PolicyPlan {
        action,
        path: path.clone(),
        json,
    })
}

fn parse_tool_plan(args: &[OsString]) -> Result<ToolPlan, String> {
    let subcommand = os_arg(args, 1, "tool")?;
    let action = match subcommand {
        "build" => ToolAction::Build,
        "search" => ToolAction::Search,
        "inspect" => ToolAction::Inspect,
        _ => return Err(format!("unknown tool subcommand {subcommand}")),
    };
    let mut json = false;
    let mut all = false;
    let mut source = None;
    let mut positionals = Vec::new();
    let mut index = 2;

    while index < args.len() {
        let token = os_arg(args, index, "tool")?;
        if !token.starts_with("--") {
            positionals.push(token.to_owned());
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
                index += 1;
            }
            "--all" => {
                if inline_value.is_some() {
                    return Err("--all does not take a value".to_owned());
                }
                all = true;
                index += 1;
            }
            "--source" => {
                let (value, next_index) = flag_value(args, index, flag, inline_value, "tool")?;
                source = Some(value);
                index = next_index;
            }
            _ => return Err(format!("unknown tool flag {flag}")),
        }
    }

    match action {
        ToolAction::Build => build_tool_plan(positionals, all, source, json),
        ToolAction::Search => search_tool_plan(positionals, all, source, json),
        ToolAction::Inspect => inspect_tool_plan(positionals, all, source, json),
    }
}

fn build_tool_plan(
    positionals: Vec<String>,
    all: bool,
    source: Option<String>,
    json: bool,
) -> Result<ToolPlan, String> {
    if source.is_some() {
        return Err("runx tool build does not accept --source".to_owned());
    }
    if all && !positionals.is_empty() {
        return Err("runx tool build accepts either --all or one tool directory".to_owned());
    }
    if !all && positionals.len() != 1 {
        return Err("runx tool build requires a tool directory or --all".to_owned());
    }

    Ok(ToolPlan {
        action: ToolAction::Build,
        path: positionals.first().map(PathBuf::from),
        ref_or_query: None,
        all,
        source: None,
        json,
    })
}

fn search_tool_plan(
    positionals: Vec<String>,
    all: bool,
    source: Option<String>,
    json: bool,
) -> Result<ToolPlan, String> {
    if all {
        return Err("runx tool search does not accept --all".to_owned());
    }
    let query = positionals.join(" ");
    if query.is_empty() {
        return Err("runx tool search requires a query".to_owned());
    }

    Ok(ToolPlan {
        action: ToolAction::Search,
        path: None,
        ref_or_query: Some(query),
        all: false,
        source,
        json,
    })
}

fn inspect_tool_plan(
    positionals: Vec<String>,
    all: bool,
    source: Option<String>,
    json: bool,
) -> Result<ToolPlan, String> {
    if all {
        return Err("runx tool inspect does not accept --all".to_owned());
    }
    let tool_ref = positionals.join(" ");
    if tool_ref.is_empty() {
        return Err("runx tool inspect requires a tool reference".to_owned());
    }

    Ok(ToolPlan {
        action: ToolAction::Inspect,
        path: None,
        ref_or_query: Some(tool_ref),
        all: false,
        source,
        json,
    })
}

fn parse_registry_plan(args: &[OsString]) -> Result<RegistryPlan, String> {
    let subcommand = os_arg(args, 1, "registry")?;
    let action = parse_registry_action(subcommand)?;
    let mut state = RegistryParseState::default();
    parse_registry_args(args, &mut state)?;
    let subject = registry_subject(&action, subcommand, &mut state.positionals)?;

    Ok(RegistryPlan {
        action,
        subject,
        registry: state.registry,
        registry_dir: state.registry_dir,
        version: state.version,
        expected_digest: state.expected_digest,
        destination: state.destination,
        installation_id: state.installation_id,
        owner: state.owner,
        profile: state.profile,
        limit: state.limit,
        upsert: state.upsert,
        json: state.json,
    })
}

#[derive(Default)]
struct RegistryParseState {
    json: bool,
    upsert: bool,
    registry: Option<String>,
    registry_dir: Option<PathBuf>,
    version: Option<String>,
    expected_digest: Option<String>,
    destination: Option<PathBuf>,
    installation_id: Option<String>,
    owner: Option<String>,
    profile: Option<PathBuf>,
    limit: Option<usize>,
    positionals: Vec<String>,
}

fn parse_registry_action(subcommand: &str) -> Result<RegistryAction, String> {
    match subcommand {
        "search" => Ok(RegistryAction::Search),
        "read" => Ok(RegistryAction::Read),
        "resolve" => Ok(RegistryAction::Resolve),
        "install" => Ok(RegistryAction::Install),
        "publish" => Ok(RegistryAction::Publish),
        _ => Err(format!("unknown registry subcommand {subcommand}")),
    }
}

fn parse_registry_args(args: &[OsString], state: &mut RegistryParseState) -> Result<(), String> {
    let mut index = 2;
    while index < args.len() {
        let token = os_arg(args, index, "registry")?;
        if !token.starts_with("--") {
            state.positionals.push(token.to_owned());
            index += 1;
            continue;
        }

        let (flag, inline_value) = split_flag(token);
        index = parse_registry_flag(args, index, flag, inline_value, state)?;
    }
    Ok(())
}

fn parse_registry_flag(
    args: &[OsString],
    index: usize,
    flag: &str,
    inline_value: Option<&str>,
    state: &mut RegistryParseState,
) -> Result<usize, String> {
    match flag {
        "--json" => set_registry_bool_flag(flag, inline_value, &mut state.json, index),
        "--upsert" => set_registry_bool_flag(flag, inline_value, &mut state.upsert, index),
        "--registry" => {
            set_registry_string_flag(args, index, flag, inline_value, &mut state.registry)
        }
        "--registry-dir" | "--registryDir" => {
            set_registry_path_flag(args, index, flag, inline_value, &mut state.registry_dir)
        }
        "--version" => {
            set_registry_string_flag(args, index, flag, inline_value, &mut state.version)
        }
        "--digest" => {
            set_registry_string_flag(args, index, flag, inline_value, &mut state.expected_digest)
        }
        "--to" | "--destination" => {
            set_registry_path_flag(args, index, flag, inline_value, &mut state.destination)
        }
        "--installation-id" | "--installationId" => {
            set_registry_string_flag(args, index, flag, inline_value, &mut state.installation_id)
        }
        "--owner" => set_registry_string_flag(args, index, flag, inline_value, &mut state.owner),
        "--profile" => set_registry_path_flag(args, index, flag, inline_value, &mut state.profile),
        "--limit" => set_registry_limit_flag(args, index, flag, inline_value, state),
        _ => Err(format!("unknown registry flag {flag}")),
    }
}

fn set_registry_bool_flag(
    flag: &str,
    inline_value: Option<&str>,
    target: &mut bool,
    index: usize,
) -> Result<usize, String> {
    if inline_value.is_some() {
        return Err(format!("{flag} does not take a value"));
    }
    *target = true;
    Ok(index + 1)
}

fn set_registry_string_flag(
    args: &[OsString],
    index: usize,
    flag: &str,
    inline_value: Option<&str>,
    target: &mut Option<String>,
) -> Result<usize, String> {
    let (value, next_index) = flag_value(args, index, flag, inline_value, "registry")?;
    *target = Some(value);
    Ok(next_index)
}

fn set_registry_path_flag(
    args: &[OsString],
    index: usize,
    flag: &str,
    inline_value: Option<&str>,
    target: &mut Option<PathBuf>,
) -> Result<usize, String> {
    let (value, next_index) = flag_value(args, index, flag, inline_value, "registry")?;
    *target = Some(PathBuf::from(value));
    Ok(next_index)
}

fn set_registry_limit_flag(
    args: &[OsString],
    index: usize,
    flag: &str,
    inline_value: Option<&str>,
    state: &mut RegistryParseState,
) -> Result<usize, String> {
    let (value, next_index) = flag_value(args, index, flag, inline_value, "registry")?;
    state.limit = Some(
        value
            .parse::<usize>()
            .map_err(|_| "--limit must be a positive integer".to_owned())?,
    );
    Ok(next_index)
}

fn registry_subject(
    action: &RegistryAction,
    subcommand: &str,
    positionals: &mut Vec<String>,
) -> Result<String, String> {
    match action {
        RegistryAction::Search => {
            if positionals.is_empty() {
                return Err("runx registry search requires a query".to_owned());
            }
            Ok(positionals.join(" "))
        }
        RegistryAction::Read | RegistryAction::Resolve | RegistryAction::Install => {
            if positionals.len() != 1 {
                return Err(format!(
                    "runx registry {subcommand} requires exactly one ref"
                ));
            }
            Ok(positionals.remove(0))
        }
        RegistryAction::Publish => {
            if positionals.len() != 1 {
                return Err(
                    "runx registry publish requires exactly one skill markdown path".to_owned(),
                );
            }
            Ok(positionals.remove(0))
        }
    }
}

fn os_arg<'a>(args: &'a [OsString], index: usize, command: &str) -> Result<&'a str, String> {
    args.get(index)
        .and_then(|arg| arg.to_str())
        .ok_or_else(|| format!("{command} arguments must be UTF-8"))
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
    command: &str,
) -> Result<(String, usize), String> {
    if let Some(value) = inline_value {
        return Ok((value.to_owned(), index + 1));
    }
    let value = os_arg(args, index + 1, command).map_err(|_| format!("{flag} requires a value"))?;
    if value.starts_with("--") {
        return Err(format!("{flag} requires a value"));
    }
    Ok((value.to_owned(), index + 2))
}

fn optional_flag_value(
    args: &[OsString],
    index: usize,
    inline_value: Option<&str>,
    command: &str,
) -> Result<(Option<String>, usize), String> {
    if let Some(value) = inline_value {
        return Ok((Some(value.to_owned()), index + 1));
    }
    let Some(value) = args.get(index + 1).and_then(|arg| arg.to_str()) else {
        return Ok((None, index + 1));
    };
    if value.starts_with('-') {
        return Ok((None, index + 1));
    }
    os_arg(args, index + 1, command)?;
    Ok((Some(value.to_owned()), index + 2))
}

fn truthy(value: &str) -> bool {
    matches!(value, "true" | "1" | "yes" | "official")
}
