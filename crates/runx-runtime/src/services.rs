use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use runx_contracts::Receipt;
#[cfg(feature = "mcp")]
use runx_parser::SkillMcpServer;
#[cfg(any(feature = "cli-tool", feature = "mcp"))]
use runx_parser::SkillSource;

#[cfg(any(feature = "cli-tool", feature = "mcp"))]
use crate::RuntimeError;
use crate::receipts::paths::{
    RUNX_CWD_ENV, RUNX_PROJECT_DIR_ENV, ReceiptPathInputs, ResolvedReceiptPath,
    RuntimeReceiptConfig, resolve_receipt_path,
};
use crate::receipts::store::{LocalReceiptStore, ReceiptStoreError};
use crate::receipts::{RuntimeReceiptSignatureConfig, RuntimeReceiptSigningError};
#[cfg(any(feature = "cli-tool", feature = "mcp"))]
use crate::sandbox::SandboxPlan;
#[cfg(feature = "mcp")]
use crate::sandbox::prepare_mcp_process_sandbox;
#[cfg(feature = "cli-tool")]
use crate::sandbox::prepare_process_sandbox;

const RUNX_TOOL_ROOTS_ENV: &str = "RUNX_TOOL_ROOTS";
const PROCESS_ENV_KEYS: [&str; 3] = ["PATH", "SystemRoot", "PATHEXT"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkspaceEnv {
    env: BTreeMap<String, String>,
    cwd: PathBuf,
}

impl WorkspaceEnv {
    pub(crate) fn new(env: BTreeMap<String, String>, cwd: PathBuf) -> Self {
        Self { env, cwd }
    }

    pub(crate) fn env(&self) -> &BTreeMap<String, String> {
        &self.env
    }

    pub(crate) fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub(crate) fn graph_env_for_skill(&self, skill_dir: &Path) -> BTreeMap<String, String> {
        let mut env = self.env.clone();
        for key in PROCESS_ENV_KEYS {
            if !env.contains_key(key) {
                if let Ok(value) = std::env::var(key) {
                    env.insert(key.to_owned(), value);
                }
            }
        }
        let cwd = self.cwd.to_string_lossy().into_owned();
        env.entry(RUNX_CWD_ENV.to_owned())
            .or_insert_with(|| cwd.clone());
        env.entry(RUNX_PROJECT_DIR_ENV.to_owned()).or_insert(cwd);
        if !env.contains_key(RUNX_TOOL_ROOTS_ENV) {
            if let Some(joined) = inferred_tool_roots(skill_dir) {
                env.insert(RUNX_TOOL_ROOTS_ENV.to_owned(), joined);
            }
        }
        env
    }
}

pub(crate) fn process_env_value(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

pub(crate) fn process_env_snapshot() -> BTreeMap<String, String> {
    std::env::vars().collect()
}

#[derive(Clone, Debug)]
pub(crate) struct ReceiptServices {
    signature_config: RuntimeReceiptSignatureConfig,
}

impl ReceiptServices {
    pub(crate) fn from_env(
        env: &BTreeMap<String, String>,
    ) -> Result<Self, RuntimeReceiptSigningError> {
        Ok(Self {
            signature_config: RuntimeReceiptSignatureConfig::from_env(env)?,
        })
    }

    pub(crate) fn from_signature_config(signature_config: RuntimeReceiptSignatureConfig) -> Self {
        Self { signature_config }
    }

    pub(crate) fn signature_config(&self) -> &RuntimeReceiptSignatureConfig {
        &self.signature_config
    }

    pub(crate) fn resolve_path(
        &self,
        workspace: &WorkspaceEnv,
        explicit_dir: Option<&Path>,
        runtime_config: Option<&RuntimeReceiptConfig>,
    ) -> ResolvedReceiptPath {
        let _ = self;
        resolve_receipt_path(ReceiptPathInputs {
            explicit_dir,
            runtime_config,
            env: workspace.env(),
            cwd: workspace.cwd(),
        })
    }

    pub(crate) fn write_local_receipt(
        &self,
        receipt: &Receipt,
        path: &ResolvedReceiptPath,
    ) -> Result<(), ReceiptStoreError> {
        LocalReceiptStore::new(&path.path)
            .write_receipt_with_policy(receipt, self.signature_config.signature_policy())
    }

    #[cfg(feature = "mcp")]
    pub(crate) fn write_local_receipt_dir(
        &self,
        receipt: &Receipt,
        receipt_dir: &Path,
    ) -> Result<(), ReceiptStoreError> {
        LocalReceiptStore::new(receipt_dir)
            .write_receipt_with_policy(receipt, self.signature_config.signature_policy())
    }
}

#[cfg(any(feature = "cli-tool", feature = "mcp"))]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SandboxServices;

#[cfg(any(feature = "cli-tool", feature = "mcp"))]
impl SandboxServices {
    #[cfg(feature = "cli-tool")]
    pub(crate) fn process_plan(
        self,
        source: &SkillSource,
        skill_directory: &Path,
        inputs: &runx_contracts::JsonObject,
        base_env: &BTreeMap<String, String>,
    ) -> Result<SandboxPlan, RuntimeError> {
        prepare_process_sandbox(source, skill_directory, inputs, base_env)
    }

    #[cfg(feature = "mcp")]
    pub(crate) fn mcp_process_plan(
        self,
        source: &SkillSource,
        server: &SkillMcpServer,
        skill_directory: &Path,
        base_env: &BTreeMap<String, String>,
    ) -> Result<SandboxPlan, RuntimeError> {
        prepare_mcp_process_sandbox(source, server, skill_directory, base_env)
    }
}

fn inferred_tool_roots(skill_dir: &Path) -> Option<String> {
    let root = skill_dir
        .parent()
        .filter(|parent| parent.file_name().and_then(|name| name.to_str()) == Some("skills"))
        .and_then(Path::parent)?;
    let tools_root = root.join("tools");
    if !tools_root.is_dir() {
        return None;
    }
    std::env::join_paths([tools_root])
        .ok()
        .map(|value| value.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_env_injects_workspace_and_project_paths() {
        let workspace = WorkspaceEnv::new(BTreeMap::new(), PathBuf::from("/tmp/runx-work"));

        let env = workspace.graph_env_for_skill(Path::new("/tmp/runx-work/skills/demo"));

        assert_eq!(env.get(RUNX_CWD_ENV), Some(&"/tmp/runx-work".to_owned()));
        assert_eq!(
            env.get(RUNX_PROJECT_DIR_ENV),
            Some(&"/tmp/runx-work".to_owned())
        );
    }

    #[test]
    fn receipt_services_resolve_paths_from_workspace_env() {
        let env = BTreeMap::from([(RUNX_PROJECT_DIR_ENV.to_owned(), ".runx-custom".to_owned())]);
        let workspace = WorkspaceEnv::new(env, PathBuf::from("/tmp/runx-work"));
        let receipts = ReceiptServices::from_signature_config(
            RuntimeReceiptSignatureConfig::local_development(),
        );

        let resolved = receipts.resolve_path(&workspace, None, None);

        assert_eq!(
            resolved.path,
            PathBuf::from("/tmp/runx-work/.runx-custom/receipts")
        );
    }
}
