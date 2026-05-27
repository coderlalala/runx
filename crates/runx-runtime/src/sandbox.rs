// rust-style-allow: large-file because sandbox planning owns both process and
// MCP environment/cwd policy plus the audit metadata emitted with each plan.
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use runx_contracts::{JsonObject, JsonValue};
use runx_core::policy::{CwdPolicy, SandboxProfile};
use runx_parser::{SkillMcpServer, SkillSandbox, SkillSource};

use crate::RuntimeError;
use crate::receipts::paths::{INIT_CWD_ENV, RUNX_CWD_ENV};

const DEFAULT_ENV_ALLOWLIST: [&str; 9] = [
    "PATH",
    "HOME",
    "TMPDIR",
    "TMP",
    "TEMP",
    "SystemRoot",
    "WINDIR",
    "COMSPEC",
    "PATHEXT",
];
const MAX_INLINE_INPUTS_BYTES: usize = 48 * 1024;
const MAX_INLINE_INPUT_VALUE_BYTES: usize = 8 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct SandboxPlan {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: BTreeMap<String, String>,
    pub metadata: JsonObject,
    pub cleanup_paths: Vec<PathBuf>,
}

impl Drop for SandboxPlan {
    fn drop(&mut self) {
        cleanup_paths_quietly(&self.cleanup_paths);
    }
}

pub fn prepare_process_sandbox(
    source: &SkillSource,
    skill_directory: &Path,
    inputs: &JsonObject,
    base_env: &BTreeMap<String, String>,
) -> Result<SandboxPlan, RuntimeError> {
    let command = source.command.clone().ok_or(RuntimeError::MissingCommand)?;
    let sandbox = source.sandbox.as_ref();
    validate_sandbox(sandbox)?;
    let workspace_cwd = workspace_cwd(base_env)?;
    let cwd = resolve_cwd(source, sandbox, skill_directory, workspace_cwd.as_deref())?;
    let args = source
        .args
        .iter()
        .map(|arg| resolve_template(arg, inputs, base_env))
        .collect();
    let writable_paths = resolved_writable_paths(sandbox, inputs, base_env);
    validate_writable_paths(sandbox, &writable_paths, &cwd, workspace_cwd.as_deref())?;
    let runtime = resolve_sandbox_runtime(sandbox, base_env)?;
    let mut cleanup_paths = Vec::new();
    let mut sandbox_base_env = base_env.clone();
    prepare_enforced_env(&runtime, &mut sandbox_base_env, &mut cleanup_paths)?;
    let env = match child_env(sandbox, &sandbox_base_env, inputs, &mut cleanup_paths) {
        Ok(env) => env,
        Err(error) => {
            cleanup_paths_quietly(&cleanup_paths);
            return Err(error);
        }
    };
    let (command, args) = sandbox_spawn_command(SandboxSpawnCommand {
        runtime: runtime.as_ref(),
        command,
        args,
        cwd: &cwd,
        skill_directory,
        workspace_cwd: workspace_cwd.as_deref(),
        writable_paths: &writable_paths,
        network: sandbox_network_enabled(sandbox),
        private_tmp: cleanup_paths.first().map(PathBuf::as_path),
    });
    Ok(SandboxPlan {
        command,
        args,
        cwd,
        env,
        metadata: sandbox_metadata_with_runtime(sandbox, &writable_paths, runtime.as_ref()),
        cleanup_paths,
    })
}

pub fn prepare_mcp_process_sandbox(
    source: &SkillSource,
    server: &SkillMcpServer,
    skill_directory: &Path,
    base_env: &BTreeMap<String, String>,
) -> Result<SandboxPlan, RuntimeError> {
    let sandbox = source.sandbox.as_ref();
    validate_sandbox(sandbox)?;
    let workspace_cwd = workspace_cwd(base_env)?;
    let cwd = resolve_cwd_value(
        server.cwd.as_deref(),
        sandbox,
        skill_directory,
        workspace_cwd.as_deref(),
    )?;
    let writable_paths = resolved_writable_paths(sandbox, &JsonObject::new(), base_env);
    validate_writable_paths(sandbox, &writable_paths, &cwd, workspace_cwd.as_deref())?;
    let runtime = resolve_sandbox_runtime(sandbox, base_env)?;
    let mut cleanup_paths = Vec::new();
    let mut sandbox_base_env = base_env.clone();
    prepare_enforced_env(&runtime, &mut sandbox_base_env, &mut cleanup_paths)?;
    let env = match child_base_env(sandbox, &sandbox_base_env) {
        Ok(env) => env,
        Err(error) => {
            cleanup_paths_quietly(&cleanup_paths);
            return Err(error);
        }
    };
    let (command, args) = sandbox_spawn_command(SandboxSpawnCommand {
        runtime: runtime.as_ref(),
        command: server.command.clone(),
        args: server.args.clone(),
        cwd: &cwd,
        skill_directory,
        workspace_cwd: workspace_cwd.as_deref(),
        writable_paths: &writable_paths,
        network: sandbox_network_enabled(sandbox),
        private_tmp: cleanup_paths.first().map(PathBuf::as_path),
    });
    Ok(SandboxPlan {
        command,
        args,
        cwd,
        env,
        metadata: sandbox_metadata_with_runtime(sandbox, &writable_paths, runtime.as_ref()),
        cleanup_paths,
    })
}

fn resolve_cwd(
    source: &SkillSource,
    sandbox: Option<&SkillSandbox>,
    skill_directory: &Path,
    workspace_cwd: Option<&Path>,
) -> Result<PathBuf, RuntimeError> {
    resolve_cwd_value(
        source.cwd.as_deref(),
        sandbox,
        skill_directory,
        workspace_cwd,
    )
}

fn resolve_cwd_value(
    source_cwd: Option<&str>,
    sandbox: Option<&SkillSandbox>,
    skill_directory: &Path,
    workspace_cwd: Option<&Path>,
) -> Result<PathBuf, RuntimeError> {
    let policy = sandbox
        .and_then(|sandbox| sandbox.cwd_policy.as_ref().map(CwdPolicy::as_str))
        .unwrap_or("skill-directory");
    let profile = sandbox
        .map(|sandbox| sandbox.profile.as_str())
        .unwrap_or("readonly");
    let cwd = match (policy, source_cwd) {
        ("custom", Some(cwd)) => Ok(resolve_path(skill_directory, cwd)),
        ("workspace", Some(cwd)) => Ok(resolve_path(skill_directory, cwd)),
        ("workspace", None) => workspace_cwd.map(Path::to_path_buf).map_or_else(
            || {
                std::env::current_dir()
                    .map_err(|source| RuntimeError::io("resolving workspace cwd", source))
            },
            Ok,
        ),
        (_, Some(cwd)) => Ok(resolve_path(skill_directory, cwd)),
        _ => Ok(skill_directory.to_path_buf()),
    }?;
    validate_cwd_policy(policy, profile, &cwd, skill_directory, workspace_cwd)?;
    Ok(normalize_path(&cwd))
}

fn workspace_cwd(env: &BTreeMap<String, String>) -> Result<Option<PathBuf>, RuntimeError> {
    let Some(path) = env.get(RUNX_CWD_ENV).or_else(|| env.get(INIT_CWD_ENV)) else {
        return Ok(None);
    };
    let path = PathBuf::from(path);
    if path.is_absolute() {
        Ok(Some(path))
    } else {
        std::env::current_dir()
            .map(|cwd| Some(cwd.join(path)))
            .map_err(|source| RuntimeError::io("resolving relative workspace cwd", source))
    }
}

fn resolve_path(base: &Path, path: &str) -> PathBuf {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        candidate
    } else {
        base.join(candidate)
    }
}

fn validate_cwd_policy(
    policy: &str,
    profile: &str,
    cwd: &Path,
    skill_directory: &Path,
    workspace_cwd: Option<&Path>,
) -> Result<(), RuntimeError> {
    if profile == "unrestricted-local-dev" {
        return Ok(());
    }
    let cwd = containment_path(cwd, "resolving sandbox cwd")?;
    let skill_directory = containment_path(skill_directory, "resolving sandbox skill directory")?;
    let workspace_root = match workspace_cwd {
        Some(workspace_cwd) => containment_path(workspace_cwd, "resolving sandbox workspace")?,
        None => containment_path(
            &std::env::current_dir().map_err(|source| {
                RuntimeError::io("resolving workspace cwd for sandbox policy", source)
            })?,
            "resolving sandbox workspace",
        )?,
    };
    match policy {
        "unrestricted-local-dev" => Ok(()),
        "custom"
            if is_within_path(&cwd, &skill_directory) || is_within_path(&cwd, &workspace_root) =>
        {
            Ok(())
        }
        "custom" => Err(sandbox_violation(format!(
            "sandbox custom cwd '{}' is outside skill directory '{}' and workspace '{}'",
            cwd.display(),
            skill_directory.display(),
            workspace_root.display()
        ))),
        "skill-directory" if is_within_path(&cwd, &skill_directory) => Ok(()),
        "skill-directory" => Err(sandbox_violation(format!(
            "sandbox cwd '{}' is outside skill directory '{}'",
            cwd.display(),
            skill_directory.display()
        ))),
        "workspace" if is_within_path(&cwd, &workspace_root) => Ok(()),
        "workspace" => Err(sandbox_violation(format!(
            "sandbox cwd '{}' is outside workspace '{}'",
            cwd.display(),
            workspace_root.display()
        ))),
        _ => Ok(()),
    }
}

fn is_within_path(candidate: &Path, root: &Path) -> bool {
    candidate == root || candidate.starts_with(root)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir => {
                if normalized.as_os_str().is_empty()
                    || normalized
                        .components()
                        .next_back()
                        .is_some_and(|component| component == Component::ParentDir)
                {
                    normalized.push("..");
                } else {
                    normalized.pop();
                }
            }
        }
    }
    normalized
}

fn child_env(
    sandbox: Option<&SkillSandbox>,
    base_env: &BTreeMap<String, String>,
    inputs: &JsonObject,
    cleanup_paths: &mut Vec<PathBuf>,
) -> Result<BTreeMap<String, String>, RuntimeError> {
    let mut env = child_base_env(sandbox, base_env)?;
    let serialized = serde_json::to_string(inputs)
        .map_err(|source| RuntimeError::json("serializing runtime inputs", source))?;
    if serialized.len() > MAX_INLINE_INPUTS_BYTES {
        let (inputs_path, cleanup_path) = write_inputs_file(base_env, &serialized)?;
        env.insert("RUNX_INPUTS_PATH".to_owned(), inputs_path);
        cleanup_paths.push(cleanup_path);
    } else {
        env.insert("RUNX_INPUTS_JSON".to_owned(), serialized);
    }
    let mut input_env_names = BTreeMap::new();
    for (key, value) in inputs {
        let serialized = json_value_env(value)?;
        if serialized.len() <= MAX_INLINE_INPUT_VALUE_BYTES {
            let env_name = input_env_name(key);
            if let Some(prior_key) = input_env_names.insert(env_name.clone(), key) {
                return Err(sandbox_violation(format!(
                    "input keys {prior_key:?} and {key:?} collide on environment variable {env_name}"
                )));
            }
            env.insert(env_name, serialized);
        }
    }
    Ok(env)
}

fn child_base_env(
    sandbox: Option<&SkillSandbox>,
    base_env: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, RuntimeError> {
    let mut env = allowed_base_env(sandbox, base_env);
    env.insert(
        RUNX_CWD_ENV.to_owned(),
        workspace_root(base_env)?.to_string_lossy().into_owned(),
    );
    Ok(env)
}

fn workspace_root(base_env: &BTreeMap<String, String>) -> Result<PathBuf, RuntimeError> {
    workspace_cwd(base_env)?.map_or_else(
        || {
            std::env::current_dir()
                .map_err(|source| RuntimeError::io("resolving workspace cwd", source))
        },
        Ok,
    )
}

fn write_inputs_file(
    base_env: &BTreeMap<String, String>,
    serialized: &str,
) -> Result<(String, PathBuf), RuntimeError> {
    let temp_root = base_env
        .get("TMPDIR")
        .or_else(|| base_env.get("TMP"))
        .or_else(|| base_env.get("TEMP"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let dir = temp_root.join(format!("runx-cli-inputs-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir)
        .map_err(|source| RuntimeError::io("creating inputs temp dir", source))?;
    let path = dir.join("inputs.json");
    let mut file = fs::File::create(&path)
        .map_err(|source| RuntimeError::io("creating inputs temp file", source))?;
    file.write_all(serialized.as_bytes())
        .map_err(|source| RuntimeError::io("writing inputs temp file", source))?;
    Ok((path.to_string_lossy().into_owned(), dir))
}

fn allowed_base_env(
    sandbox: Option<&SkillSandbox>,
    base_env: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut allowed = DEFAULT_ENV_ALLOWLIST
        .iter()
        .filter_map(|key| {
            base_env
                .get(*key)
                .cloned()
                .map(|value| ((*key).to_owned(), value))
        })
        .collect::<BTreeMap<_, _>>();
    if let Some(env_allowlist) = sandbox.and_then(|sandbox| sandbox.env_allowlist.as_ref()) {
        for key in env_allowlist {
            if let Some(value) = base_env.get(key) {
                allowed.insert(key.clone(), value.clone());
            }
        }
    }
    allowed
}

fn validate_sandbox(sandbox: Option<&SkillSandbox>) -> Result<(), RuntimeError> {
    let Some(sandbox) = sandbox else {
        return Ok(());
    };
    match sandbox.profile.as_str() {
        "readonly" => validate_readonly_sandbox(sandbox),
        "workspace-write" | "network" => Ok(()),
        "unrestricted-local-dev" => validate_unrestricted_sandbox(sandbox),
        profile => Err(sandbox_violation(format!(
            "unsupported sandbox profile '{profile}'"
        ))),
    }
}

fn resolved_writable_paths(
    sandbox: Option<&SkillSandbox>,
    inputs: &JsonObject,
    base_env: &BTreeMap<String, String>,
) -> Vec<String> {
    sandbox.map_or_else(Vec::new, |sandbox| {
        sandbox
            .writable_paths
            .iter()
            .map(|path| resolve_template(path, inputs, base_env))
            .filter(|path| !path.trim().is_empty() && !has_unresolved_template(path))
            .collect()
    })
}

fn validate_writable_paths(
    sandbox: Option<&SkillSandbox>,
    writable_paths: &[String],
    cwd: &Path,
    workspace_cwd: Option<&Path>,
) -> Result<(), RuntimeError> {
    let Some(sandbox) = sandbox else {
        return Ok(());
    };
    if sandbox.profile != SandboxProfile::WorkspaceWrite {
        return Ok(());
    }
    let workspace_root = match workspace_cwd {
        Some(workspace_cwd) => {
            containment_path(workspace_cwd, "resolving sandbox writable workspace")?
        }
        None => containment_path(
            &std::env::current_dir().map_err(|source| {
                RuntimeError::io("resolving workspace cwd for sandbox writable paths", source)
            })?,
            "resolving sandbox writable workspace",
        )?,
    };
    let escaped = writable_paths
        .iter()
        .map(|path| containment_path(&resolve_path(cwd, path), "resolving sandbox writable path"))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|path| !is_within_path(path, &workspace_root))
        .collect::<Vec<_>>();
    if escaped.is_empty() {
        return Ok(());
    }
    Err(sandbox_violation(format!(
        "workspace-write sandbox has writable path(s) outside workspace: {}",
        escaped
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    )))
}

fn containment_path(path: &Path, operation: &'static str) -> Result<PathBuf, RuntimeError> {
    if path.exists() {
        return fs::canonicalize(path).map_err(|source| RuntimeError::io(operation, source));
    }
    let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return Ok(normalize_path(path));
    };
    let canonical_parent =
        fs::canonicalize(parent).map_err(|source| RuntimeError::io(operation, source))?;
    Ok(path
        .file_name()
        .map(|file_name| canonical_parent.join(file_name))
        .unwrap_or(canonical_parent))
}

fn validate_readonly_sandbox(sandbox: &SkillSandbox) -> Result<(), RuntimeError> {
    if sandbox.network == Some(true) {
        return Err(sandbox_violation("readonly sandbox cannot request network"));
    }
    if !sandbox.writable_paths.is_empty() {
        return Err(sandbox_violation(
            "readonly sandbox cannot declare writable paths",
        ));
    }
    Ok(())
}

fn validate_unrestricted_sandbox(sandbox: &SkillSandbox) -> Result<(), RuntimeError> {
    if sandbox.approved_escalation == Some(true) {
        Ok(())
    } else {
        Err(sandbox_violation(
            "unrestricted-local-dev requires approved escalation",
        ))
    }
}

fn sandbox_violation(message: impl Into<String>) -> RuntimeError {
    RuntimeError::SandboxViolation {
        message: message.into(),
    }
}

#[derive(Clone, Debug, PartialEq)]
enum SandboxRuntime {
    Direct,
    DeclaredPolicyOnly {
        reason: String,
    },
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Bubblewrap {
        path: PathBuf,
    },
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    SandboxExec {
        path: PathBuf,
    },
}

impl SandboxRuntime {
    fn enforces(&self) -> bool {
        matches!(self, Self::Bubblewrap { .. } | Self::SandboxExec { .. })
    }
}

fn resolve_sandbox_runtime(
    sandbox: Option<&SkillSandbox>,
    _base_env: &BTreeMap<String, String>,
) -> Result<Option<SandboxRuntime>, RuntimeError> {
    let Some(sandbox) = sandbox else {
        return Ok(None);
    };
    if sandbox.profile == SandboxProfile::UnrestrictedLocalDev {
        if sandbox.require_enforcement == Some(true) {
            return Err(sandbox_violation(
                "unrestricted-local-dev cannot satisfy required sandbox enforcement",
            ));
        }
        return Ok(Some(SandboxRuntime::Direct));
    }

    let runtime = platform_sandbox_runtime(sandbox.profile.as_str());
    if runtime.enforces() {
        return Ok(Some(runtime));
    }
    if sandbox.require_enforcement != Some(true) {
        return Ok(Some(runtime));
    }
    let reason = match runtime {
        SandboxRuntime::DeclaredPolicyOnly { reason } => reason,
        SandboxRuntime::Direct => {
            "direct execution does not enforce sandbox declarations".to_owned()
        }
        SandboxRuntime::Bubblewrap { .. } | SandboxRuntime::SandboxExec { .. } => {
            return Ok(Some(runtime));
        }
    };
    Err(sandbox_violation(reason))
}

fn platform_sandbox_runtime(profile: &str) -> SandboxRuntime {
    #[cfg(target_os = "linux")]
    {
        if let Some(path) = find_trusted_executable("bwrap") {
            SandboxRuntime::Bubblewrap { path }
        } else {
            SandboxRuntime::DeclaredPolicyOnly {
                reason: missing_sandbox_backend_reason(profile),
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(path) = find_usable_sandbox_exec() {
            return SandboxRuntime::SandboxExec { path };
        }
        SandboxRuntime::DeclaredPolicyOnly {
            reason: missing_sandbox_backend_reason(profile),
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        SandboxRuntime::DeclaredPolicyOnly {
            reason: missing_sandbox_backend_reason(profile),
        }
    }
}

fn missing_sandbox_backend_reason(profile: &str) -> String {
    format!(
        "local sandbox profile '{profile}' requires Linux bubblewrap or macOS sandbox-exec for filesystem and network enforcement"
    )
}

#[cfg(target_os = "macos")]
fn find_usable_sandbox_exec() -> Option<PathBuf> {
    let path = find_trusted_executable("sandbox-exec")?;
    let status = std::process::Command::new(&path)
        .args(["-p", "(version 1)\n(allow default)", "/usr/bin/true"])
        .status()
        .ok()?;
    status.success().then_some(path)
}

fn find_trusted_executable(command: &str) -> Option<PathBuf> {
    default_executable_search_paths(command)
        .into_iter()
        .map(|dir| dir.join(command))
        .find(|candidate| candidate.is_file())
}

fn default_executable_search_paths(command: &str) -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin")];
    if command == "sandbox-exec" {
        paths.push(PathBuf::from("/usr/sbin"));
        paths.push(PathBuf::from("/sbin"));
    }
    paths
}

fn prepare_enforced_env(
    runtime: &Option<SandboxRuntime>,
    env: &mut BTreeMap<String, String>,
    cleanup_paths: &mut Vec<PathBuf>,
) -> Result<(), RuntimeError> {
    if !matches!(
        runtime,
        Some(SandboxRuntime::Bubblewrap { .. } | SandboxRuntime::SandboxExec { .. })
    ) {
        return Ok(());
    }
    let private_tmp = create_private_tmp()?;
    let private_tmp_str = private_tmp.to_string_lossy().into_owned();
    env.insert("TMPDIR".to_owned(), private_tmp_str.clone());
    env.insert("TMP".to_owned(), private_tmp_str.clone());
    env.insert("TEMP".to_owned(), private_tmp_str);
    cleanup_paths.push(private_tmp);
    Ok(())
}

fn create_private_tmp() -> Result<PathBuf, RuntimeError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let path =
        std::env::temp_dir().join(format!("runx-local-sandbox-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&path)
        .map_err(|source| RuntimeError::io("creating sandbox private temp dir", source))?;
    Ok(path)
}

struct SandboxSpawnCommand<'a> {
    runtime: Option<&'a SandboxRuntime>,
    command: String,
    args: Vec<String>,
    cwd: &'a Path,
    skill_directory: &'a Path,
    workspace_cwd: Option<&'a Path>,
    writable_paths: &'a [String],
    network: bool,
    private_tmp: Option<&'a Path>,
}

fn sandbox_spawn_command(input: SandboxSpawnCommand<'_>) -> (String, Vec<String>) {
    match input.runtime {
        Some(SandboxRuntime::Bubblewrap { path }) => (
            path.to_string_lossy().into_owned(),
            bubblewrap_args(BubblewrapCommand {
                command: input.command,
                command_args: input.args,
                cwd: input.cwd,
                skill_directory: input.skill_directory,
                workspace_cwd: input.workspace_cwd,
                writable_paths: input.writable_paths,
                network: input.network,
                private_tmp: input.private_tmp,
            }),
        ),
        Some(SandboxRuntime::SandboxExec { path }) => (
            path.to_string_lossy().into_owned(),
            sandbox_exec_args(
                input.command,
                input.args,
                input.cwd,
                input.writable_paths,
                input.network,
                input.private_tmp,
            ),
        ),
        Some(SandboxRuntime::Direct | SandboxRuntime::DeclaredPolicyOnly { .. }) | None => {
            (input.command, input.args)
        }
    }
}

fn sandbox_network_enabled(sandbox: Option<&SkillSandbox>) -> bool {
    sandbox.is_some_and(|sandbox| {
        sandbox
            .network
            .unwrap_or(sandbox.profile == SandboxProfile::Network)
    })
}

struct BubblewrapCommand<'a> {
    command: String,
    command_args: Vec<String>,
    cwd: &'a Path,
    skill_directory: &'a Path,
    workspace_cwd: Option<&'a Path>,
    writable_paths: &'a [String],
    network: bool,
    private_tmp: Option<&'a Path>,
}

fn bubblewrap_args(input: BubblewrapCommand<'_>) -> Vec<String> {
    let BubblewrapCommand {
        command,
        command_args,
        cwd,
        skill_directory,
        workspace_cwd,
        writable_paths,
        network,
        private_tmp,
    } = input;
    let workspace_root = workspace_cwd.map(normalize_path).or_else(|| {
        std::env::current_dir()
            .ok()
            .map(|path| normalize_path(&path))
    });
    let mut args = vec!["--unshare-all".to_owned()];
    if network {
        args.push("--share-net".to_owned());
    }
    args.extend([
        "--die-with-parent".to_owned(),
        "--proc".to_owned(),
        "/proc".to_owned(),
        "--dev".to_owned(),
        "/dev".to_owned(),
        "--tmpfs".to_owned(),
        "/tmp".to_owned(),
    ]);
    for mount_path in readonly_mounts(skill_directory, workspace_root.as_deref(), cwd) {
        args.extend([
            "--ro-bind-try".to_owned(),
            path_string(&mount_path),
            path_string(&mount_path),
        ]);
    }
    if let Some(private_tmp) = private_tmp {
        args.extend([
            "--bind".to_owned(),
            path_string(private_tmp),
            path_string(private_tmp),
        ]);
    }
    for mount in writable_mounts(writable_paths, cwd) {
        args.extend([
            "--bind".to_owned(),
            path_string(&mount),
            path_string(&mount),
        ]);
    }
    args.extend([
        "--chdir".to_owned(),
        path_string(cwd),
        "--".to_owned(),
        command,
    ]);
    args.extend(command_args);
    args
}

fn readonly_mounts(
    skill_directory: &Path,
    workspace_root: Option<&Path>,
    cwd: &Path,
) -> Vec<PathBuf> {
    unique_paths(
        system_readonly_mounts()
            .into_iter()
            .chain(find_package_root(skill_directory))
            .chain([normalize_existing_path(skill_directory)])
            .chain(workspace_root.map(Path::to_path_buf))
            .chain([normalize_existing_path(cwd)])
            .collect(),
    )
}

fn system_readonly_mounts() -> Vec<PathBuf> {
    [
        "/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc", "/opt", "/nix", "/snap",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

fn writable_mounts(writable_paths: &[String], cwd: &Path) -> Vec<PathBuf> {
    unique_paths(
        writable_paths
            .iter()
            .map(|path| writable_mount_path(&resolve_path(cwd, path)))
            .collect(),
    )
}

fn writable_mount_path(path: &Path) -> PathBuf {
    if path.exists() {
        return normalize_existing_path(path);
    }
    path.parent()
        .map(normalize_existing_path)
        .unwrap_or_else(|| normalize_path(path))
}

fn sandbox_exec_args(
    command: String,
    command_args: Vec<String>,
    cwd: &Path,
    writable_paths: &[String],
    network: bool,
    private_tmp: Option<&Path>,
) -> Vec<String> {
    let mut args = vec![
        "-p".to_owned(),
        sandbox_exec_profile(cwd, writable_paths, network, private_tmp),
    ];
    args.push(command);
    args.extend(command_args);
    args
}

fn sandbox_exec_profile(
    cwd: &Path,
    writable_paths: &[String],
    network: bool,
    private_tmp: Option<&Path>,
) -> String {
    let mut profile = [
        "(version 1)",
        "(deny default)",
        "(allow process*)",
        "(allow sysctl*)",
        "(allow file-read*)",
        "(allow file-write* (literal \"/dev/null\"))",
    ]
    .join("\n");
    if network {
        profile.push_str("\n(allow network*)");
        profile.push_str("\n(allow mach-lookup)");
    }
    for writable_path in writable_paths {
        let declared = resolve_path(cwd, writable_path);
        let path = sandbox_exec_path_filter_path(&declared);
        if declared.is_dir() {
            profile.push_str(&format!(
                "\n(allow file-write* (literal \"{}\") (subpath \"{}\"))",
                sandbox_profile_string(&path),
                sandbox_profile_string(&path)
            ));
        } else {
            profile.push_str(&format!(
                "\n(allow file-write* (literal \"{}\"))",
                sandbox_profile_string(&path)
            ));
        }
    }
    if let Some(private_tmp) = private_tmp {
        let path = sandbox_exec_path_filter_path(private_tmp);
        profile.push_str(&format!(
            "\n(allow file-write* (literal \"{}\") (subpath \"{}\"))",
            sandbox_profile_string(&path),
            sandbox_profile_string(&path)
        ));
    }
    profile
}

fn sandbox_exec_path_filter_path(path: &Path) -> PathBuf {
    if path.exists() {
        return normalize_existing_path(path);
    }
    let parent = path.parent().map(normalize_existing_path);
    parent
        .map(|parent| {
            path.file_name()
                .map(|name| parent.join(name))
                .unwrap_or(parent)
        })
        .unwrap_or_else(|| path.to_path_buf())
}

fn sandbox_profile_string(path: &Path) -> String {
    path_string(path).replace('\\', "\\\\").replace('"', "\\\"")
}

fn find_package_root(start: &Path) -> Option<PathBuf> {
    let mut current = normalize_existing_path(start);
    let mut found = None;
    loop {
        if current.join("package.json").exists() || current.join("pnpm-workspace.yaml").exists() {
            found = Some(current.clone());
        }
        let Some(parent) = current.parent() else {
            return found;
        };
        if parent == current {
            return found;
        }
        current = parent.to_path_buf();
    }
}

fn normalize_existing_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| normalize_path(path))
}

fn unique_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut unique = Vec::new();
    for path in paths {
        if !unique.iter().any(|prior| prior == &path) {
            unique.push(path);
        }
    }
    unique
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn cleanup_paths_quietly(paths: &[PathBuf]) {
    for path in paths {
        let _ = fs::remove_dir_all(path);
    }
}

fn input_env_name(key: &str) -> String {
    let mut suffix = String::new();
    let mut pending_separator = false;
    for ch in key.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_separator && !suffix.is_empty() {
                suffix.push('_');
            }
            suffix.push(ch.to_ascii_uppercase());
            pending_separator = false;
        } else {
            pending_separator = true;
        }
    }
    format!("RUNX_INPUT_{suffix}")
}

fn json_value_env(value: &JsonValue) -> Result<String, RuntimeError> {
    match value {
        JsonValue::Null => Ok(String::new()),
        JsonValue::Bool(value) => Ok(value.to_string()),
        JsonValue::Number(value) => serde_json::to_string(value)
            .map_err(|source| RuntimeError::json("serializing input number", source)),
        JsonValue::String(value) => Ok(value.clone()),
        JsonValue::Array(_) | JsonValue::Object(_) => serde_json::to_string(value)
            .map_err(|source| RuntimeError::json("serializing structured input", source)),
    }
}

fn resolve_template(
    template: &str,
    inputs: &JsonObject,
    base_env: &BTreeMap<String, String>,
) -> String {
    let mut resolved = template.to_owned();
    for (key, value) in inputs {
        if let Ok(value) = json_value_env(value) {
            resolved = resolved.replace(&format!("{{{{{key}}}}}"), &value);
            resolved = resolved.replace(&format!("{{{{ {key} }}}}"), &value);
        }
    }
    for (key, value) in base_env {
        resolved = resolved.replace(&format!("{{{{env.{key}}}}}"), value);
        resolved = resolved.replace(&format!("{{{{ env.{key} }}}}"), value);
    }
    resolved
}

fn has_unresolved_template(value: &str) -> bool {
    value.contains("{{") && value.contains("}}")
}

pub fn sandbox_metadata(sandbox: Option<&SkillSandbox>) -> JsonObject {
    let writable_paths = sandbox
        .map(|sandbox| sandbox.writable_paths.clone())
        .unwrap_or_default();
    sandbox_metadata_with_runtime(sandbox, &writable_paths, None)
}

fn sandbox_metadata_with_runtime(
    sandbox: Option<&SkillSandbox>,
    writable_paths: &[String],
    runtime: Option<&SandboxRuntime>,
) -> JsonObject {
    let mut metadata = JsonObject::new();
    if let Some(sandbox) = sandbox {
        metadata.insert(
            "profile".to_owned(),
            JsonValue::String(sandbox.profile.as_str().to_owned()),
        );
        if let Some(cwd_policy) = &sandbox.cwd_policy {
            metadata.insert(
                "cwd_policy".to_owned(),
                JsonValue::String(cwd_policy.as_str().to_owned()),
            );
        }
        metadata.insert(
            "env".to_owned(),
            JsonValue::Object(sandbox_env_metadata(sandbox)),
        );
        insert_network_metadata(&mut metadata, sandbox, runtime);
        insert_writable_paths_metadata(&mut metadata, writable_paths);
        metadata.insert(
            "require_enforcement".to_owned(),
            JsonValue::Bool(sandbox.require_enforcement.unwrap_or(false)),
        );
        insert_filesystem_metadata(&mut metadata, sandbox, runtime);
        insert_approval_metadata(&mut metadata, sandbox);
        insert_runtime_metadata(&mut metadata, sandbox, runtime);
    }
    metadata
}

fn sandbox_env_metadata(sandbox: &SkillSandbox) -> JsonObject {
    let allowlist = sandbox.env_allowlist.clone().unwrap_or_else(|| {
        DEFAULT_ENV_ALLOWLIST
            .into_iter()
            .map(str::to_owned)
            .collect()
    });
    [
        (
            "mode".to_owned(),
            JsonValue::String(if sandbox.env_allowlist.is_some() {
                "allowlist".to_owned()
            } else {
                "default-allowlist".to_owned()
            }),
        ),
        (
            "allowlist".to_owned(),
            JsonValue::Array(allowlist.into_iter().map(JsonValue::String).collect()),
        ),
    ]
    .into()
}

fn insert_network_metadata(
    metadata: &mut JsonObject,
    sandbox: &SkillSandbox,
    runtime: Option<&SandboxRuntime>,
) {
    metadata.insert(
        "network".to_owned(),
        JsonValue::Object(
            [
                (
                    "declared".to_owned(),
                    JsonValue::Bool(sandbox_network_enabled(Some(sandbox))),
                ),
                (
                    "enforcement".to_owned(),
                    JsonValue::String(network_enforcement(sandbox, runtime).to_owned()),
                ),
            ]
            .into(),
        ),
    );
}

fn insert_writable_paths_metadata(metadata: &mut JsonObject, writable_paths: &[String]) {
    metadata.insert(
        "writable_paths".to_owned(),
        JsonValue::Array(
            writable_paths
                .iter()
                .cloned()
                .map(JsonValue::String)
                .collect(),
        ),
    );
}

fn insert_filesystem_metadata(
    metadata: &mut JsonObject,
    sandbox: &SkillSandbox,
    runtime: Option<&SandboxRuntime>,
) {
    metadata.insert(
        "filesystem".to_owned(),
        JsonValue::Object(
            [
                (
                    "enforcement".to_owned(),
                    JsonValue::String(filesystem_enforcement(sandbox, runtime).to_owned()),
                ),
                (
                    "readonly_paths".to_owned(),
                    JsonValue::Bool(sandbox.profile != SandboxProfile::UnrestrictedLocalDev),
                ),
                (
                    "writable_paths_enforced".to_owned(),
                    JsonValue::Bool(
                        runtime.is_some_and(SandboxRuntime::enforces)
                            && sandbox.profile == SandboxProfile::WorkspaceWrite,
                    ),
                ),
                (
                    "private_tmp".to_owned(),
                    JsonValue::Bool(matches!(runtime, Some(SandboxRuntime::Bubblewrap { .. }))),
                ),
            ]
            .into(),
        ),
    );
}

fn insert_approval_metadata(metadata: &mut JsonObject, sandbox: &SkillSandbox) {
    metadata.insert(
        "approval".to_owned(),
        JsonValue::Object(
            [
                (
                    "required".to_owned(),
                    JsonValue::Bool(sandbox.profile == SandboxProfile::UnrestrictedLocalDev),
                ),
                (
                    "approved".to_owned(),
                    JsonValue::Bool(sandbox.approved_escalation.unwrap_or(false)),
                ),
            ]
            .into(),
        ),
    );
}

fn insert_runtime_metadata(
    metadata: &mut JsonObject,
    sandbox: &SkillSandbox,
    runtime: Option<&SandboxRuntime>,
) {
    metadata.insert(
        "runtime".to_owned(),
        JsonValue::Object(runtime_metadata(sandbox, runtime)),
    );
}

fn network_enforcement(sandbox: &SkillSandbox, runtime: Option<&SandboxRuntime>) -> &'static str {
    match runtime {
        Some(SandboxRuntime::Bubblewrap { .. } | SandboxRuntime::SandboxExec { .. }) => {
            if sandbox_network_enabled(Some(sandbox)) {
                "host-network-shared"
            } else {
                "isolated-namespace"
            }
        }
        Some(SandboxRuntime::Direct) if sandbox.profile == SandboxProfile::UnrestrictedLocalDev => {
            "host-ambient"
        }
        Some(SandboxRuntime::DeclaredPolicyOnly { .. }) | None => "not-enforced-local",
        Some(SandboxRuntime::Direct) => "host-ambient",
    }
}

fn filesystem_enforcement(
    sandbox: &SkillSandbox,
    runtime: Option<&SandboxRuntime>,
) -> &'static str {
    match runtime {
        Some(SandboxRuntime::Bubblewrap { .. }) => "bubblewrap-mount-namespace",
        Some(SandboxRuntime::SandboxExec { .. }) => "sandbox-exec-seatbelt",
        Some(SandboxRuntime::Direct) if sandbox.profile == SandboxProfile::UnrestrictedLocalDev => {
            "host-ambient"
        }
        Some(SandboxRuntime::DeclaredPolicyOnly { .. }) | None => "not-enforced-local",
        Some(SandboxRuntime::Direct) => "host-ambient",
    }
}

fn runtime_metadata(sandbox: &SkillSandbox, runtime: Option<&SandboxRuntime>) -> JsonObject {
    match runtime {
        Some(SandboxRuntime::Bubblewrap { path }) => [
            (
                "enforcer".to_owned(),
                JsonValue::String("bubblewrap".to_owned()),
            ),
            (
                "command".to_owned(),
                JsonValue::String(path.to_string_lossy().into_owned()),
            ),
        ]
        .into(),
        Some(SandboxRuntime::SandboxExec { path }) => [
            (
                "enforcer".to_owned(),
                JsonValue::String("sandbox-exec".to_owned()),
            ),
            (
                "command".to_owned(),
                JsonValue::String(path.to_string_lossy().into_owned()),
            ),
        ]
        .into(),
        Some(SandboxRuntime::Direct) => [(
            "enforcer".to_owned(),
            JsonValue::String("direct".to_owned()),
        )]
        .into(),
        Some(SandboxRuntime::DeclaredPolicyOnly { reason }) => [
            (
                "enforcer".to_owned(),
                JsonValue::String("declared-policy-only".to_owned()),
            ),
            ("reason".to_owned(), JsonValue::String(reason.clone())),
        ]
        .into(),
        None => [
            (
                "enforcer".to_owned(),
                JsonValue::String("declared-policy-only".to_owned()),
            ),
            (
                "reason".to_owned(),
                JsonValue::String(format!(
                    "local sandbox profile '{}' requires Linux bubblewrap or macOS sandbox-exec for filesystem and network enforcement",
                    sandbox.profile.as_str()
                )),
            ),
        ]
        .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writable_paths_omit_unresolved_optional_templates() {
        let sandbox = SkillSandbox {
            profile: SandboxProfile::WorkspaceWrite,
            cwd_policy: None,
            env_allowlist: None,
            network: None,
            writable_paths: vec![
                "{{workspace_path}}".to_owned(),
                "{{ fixture }}".to_owned(),
                "{{ env.RUNX_RAIL_COUNT_PATH }}".to_owned(),
                "logs".to_owned(),
            ],
            require_enforcement: None,
            approved_escalation: None,
            raw: JsonObject::new(),
        };
        let inputs = [(
            "fixture".to_owned(),
            JsonValue::String("/tmp/runx-fixture".to_owned()),
        )]
        .into_iter()
        .collect();
        let env = [(
            "RUNX_RAIL_COUNT_PATH".to_owned(),
            "/tmp/runx-rail-count.txt".to_owned(),
        )]
        .into_iter()
        .collect();

        assert_eq!(
            resolved_writable_paths(Some(&sandbox), &inputs, &env),
            vec![
                "/tmp/runx-fixture".to_owned(),
                "/tmp/runx-rail-count.txt".to_owned(),
                "logs".to_owned()
            ]
        );
    }

    #[test]
    fn trusted_enforcer_lookup_ignores_caller_path() {
        let trusted = find_trusted_executable("runx-test-enforcer-that-should-not-exist");
        assert!(trusted.is_none());
    }

    #[test]
    fn sandbox_exec_runtime_gets_private_writable_tmp_env() -> Result<(), String> {
        let runtime = Some(SandboxRuntime::SandboxExec {
            path: PathBuf::from("/usr/bin/sandbox-exec"),
        });
        let mut env = BTreeMap::new();
        let mut cleanup_paths = Vec::new();
        prepare_enforced_env(&runtime, &mut env, &mut cleanup_paths)
            .map_err(|source| source.to_string())?;

        let tmpdir = env
            .get("TMPDIR")
            .ok_or_else(|| "TMPDIR was not set".to_owned())?;
        assert_eq!(env.get("TMP"), Some(tmpdir));
        assert_eq!(env.get("TEMP"), Some(tmpdir));
        assert_eq!(cleanup_paths, vec![PathBuf::from(tmpdir)]);
        assert!(Path::new(tmpdir).is_dir());

        let profile =
            sandbox_exec_profile(Path::new("/workspace"), &[], true, Some(Path::new(tmpdir)));
        assert!(profile.contains("(allow file-write* (literal \"/dev/null\"))"));
        assert!(profile.contains("(allow mach-lookup)"));
        let tmp_filter_path = sandbox_exec_path_filter_path(Path::new(tmpdir));
        assert!(profile.contains(&format!(
            "(subpath \"{}\")",
            sandbox_profile_string(&tmp_filter_path)
        )));
        cleanup_paths_quietly(&cleanup_paths);
        Ok(())
    }

    #[test]
    #[cfg(unix)]
    fn workspace_write_rejects_symlink_escape() -> Result<(), String> {
        let temp = tempfile::tempdir().map_err(|source| source.to_string())?;
        let workspace = temp.path().join("workspace");
        let outside = temp.path().join("outside");
        fs::create_dir_all(&workspace).map_err(|source| source.to_string())?;
        fs::create_dir_all(&outside).map_err(|source| source.to_string())?;
        std::os::unix::fs::symlink(&outside, workspace.join("link"))
            .map_err(|source| source.to_string())?;
        let sandbox = SkillSandbox {
            profile: SandboxProfile::WorkspaceWrite,
            cwd_policy: None,
            env_allowlist: None,
            network: None,
            writable_paths: Vec::new(),
            require_enforcement: None,
            approved_escalation: None,
            raw: JsonObject::new(),
        };

        let error = validate_writable_paths(
            Some(&sandbox),
            &["link/escape.txt".to_owned()],
            &workspace,
            Some(&workspace),
        )
        .err()
        .ok_or_else(|| "symlink escape unexpectedly passed".to_owned())?;

        assert!(
            error.to_string().contains("outside workspace"),
            "unexpected error: {error}"
        );
        Ok(())
    }
}
