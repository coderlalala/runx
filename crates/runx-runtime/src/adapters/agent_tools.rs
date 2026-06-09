//! Recursive tool executor for the managed-agent loop.
//!
//! When the model chooses a tool, the agent invokes it through the governed
//! runtime. This reuses the catalog adapter's single resolve-and-invoke path
//! (`resolve_and_invoke_local_tool`) so the agent's tool calls go through the same
//! resolution, sandbox, credential delivery, and receipt machinery as any other
//! local tool. There is no parallel execution route.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Instant;

use runx_contracts::JsonValue;

use super::agent_loop::ToolExecutor;
use super::catalog::{LocalToolRequest, resolve_and_invoke_local_tool};
use crate::RuntimeError;
use crate::adapter::SkillOutput;
use crate::credentials::CredentialDelivery;

const MANAGED_AGENT_SKILL: &str = "managed-agent";

/// Executes the agent's chosen tools through the governed runtime, carrying the
/// run context (env, skill directory, credential delivery) the resolver captured
/// from the agent invocation.
pub struct RuntimeToolExecutor {
    env: BTreeMap<String, String>,
    skill_directory: PathBuf,
    credential_delivery: CredentialDelivery,
    allowed_tools: BTreeSet<String>,
}

impl RuntimeToolExecutor {
    #[must_use]
    pub fn new(
        env: BTreeMap<String, String>,
        skill_directory: PathBuf,
        credential_delivery: CredentialDelivery,
        allowed_tools: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            env,
            skill_directory,
            credential_delivery,
            allowed_tools: allowed_tools.into_iter().collect(),
        }
    }
}

impl ToolExecutor for RuntimeToolExecutor {
    fn execute(&self, tool: &str, input: &JsonValue) -> Result<SkillOutput, RuntimeError> {
        if !self.allowed_tools.contains(tool) {
            return Err(RuntimeError::SkillFailed {
                skill_name: MANAGED_AGENT_SKILL.to_owned(),
                message: format!("managed agent tool '{tool}' is not in the run's allowed_tools"),
            });
        }
        // The model supplies the tool arguments already resolved, so pass them as
        // both inputs and resolved_inputs.
        let inputs = input.as_object().cloned().unwrap_or_default();
        let request = LocalToolRequest {
            tool_ref: tool,
            inputs: &inputs,
            resolved_inputs: &inputs,
            env: &self.env,
            skill_directory: &self.skill_directory,
            credential_delivery: &self.credential_delivery,
            skill_name: tool,
            allow_explicit_manifest_path: false,
        };
        match resolve_and_invoke_local_tool(&request, Instant::now())? {
            Some(output) => Ok(output),
            None => Err(RuntimeError::SkillFailed {
                skill_name: MANAGED_AGENT_SKILL.to_owned(),
                message: format!("managed agent tool '{tool}' did not resolve to a local tool"),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unresolved_tool_is_an_error() {
        // A non-object input (here Null) also exercises the coercion to empty args
        // on the way to a clean failure, so this covers that path too.
        let executor = RuntimeToolExecutor::new(
            BTreeMap::new(),
            PathBuf::from("."),
            CredentialDelivery::none(),
            ["definitely-not-a-real-tool".to_owned()],
        );
        let result = executor.execute("definitely-not-a-real-tool", &JsonValue::Null);
        assert!(
            matches!(&result, Err(RuntimeError::SkillFailed { .. })),
            "an unresolved tool must fail, not panic or succeed; got: {result:?}"
        );
    }

    #[test]
    fn tool_outside_allowed_tools_is_rejected_before_resolution() {
        let executor = RuntimeToolExecutor::new(
            BTreeMap::new(),
            PathBuf::from("."),
            CredentialDelivery::none(),
            ["fs.read".to_owned()],
        );
        let result = executor.execute("/tmp/manifest.json", &JsonValue::Null);
        assert!(
            matches!(&result, Err(RuntimeError::SkillFailed { message, .. }) if message.contains("not in the run's allowed_tools")),
            "a model-selected tool outside allowed_tools must fail before local resolution; got: {result:?}"
        );
    }
}
