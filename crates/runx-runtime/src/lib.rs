//! Native Rust runtime skeleton for runx execution.
//!
//! The runtime owns impure boundaries: filesystem reads, subprocess execution,
//! sandbox preparation, caller reporting, and harness receipt emission. Pure
//! parser/core/receipt crates stay upstream of this crate.

pub mod adapter;
mod agent_invocation;
pub mod approval;
pub mod caller;
pub mod config;
pub mod connect;
pub mod dev;
pub mod doctor;
pub mod error;
pub mod execution;
mod hosted_http;
pub mod journal;
pub mod list;
pub mod post_merge_observer;
pub mod receipts;
pub mod registry;
pub mod sandbox;
pub mod scaffold;
pub mod tool_catalogs;

pub use execution::harness;
pub use execution::runner;
pub use execution::skill_run;
pub use execution::target_runner;
pub use receipts::paths as receipt_paths;
pub use receipts::store as receipt_store;
pub use receipts::tree as receipt_tree;

#[cfg(any(
    feature = "cli-tool",
    feature = "catalog",
    feature = "mcp",
    feature = "a2a",
    feature = "agent"
))]
pub mod adapters;

pub use adapter::{InvocationStatus, SkillAdapter, SkillInvocation, SkillOutput};
pub use approval::{
    ApprovalError, ApprovalResolution, LocalApprovalGateResolver, approval_idempotency_key,
    request_approval,
};
pub use caller::{Caller, NoopCaller};
pub use config::{
    ConfigError, ConfigKey, LocalProfileSource, ManagedAgentConfig, ManagedAgentProvider,
    ResolvedLocalProfile, RunxAgentConfig, RunxConfigFile, load_local_agent_api_key,
    load_managed_agent_config, load_runx_config_file, lookup_runx_config_value,
    mask_runx_config_file, parse_config_key, resolve_local_skill_profile,
    resolve_path_from_user_input, resolve_runx_global_home_dir, resolve_runx_home_dir,
    update_runx_config_value, write_runx_config_file,
};
pub use connect::{
    ConnectClient, ConnectClientOptions, ConnectError, ConnectOpener, ConnectResult,
    HttpConnectGrant, HttpConnectListResponse, HttpConnectPreprovisionRequest,
    HttpConnectReadyResponse, HttpConnectRevokeResponse, ProcessConnectOpener,
    load_connect_options_from_env,
};
pub use dev::{
    DEFAULT_DEV_WATCH_DEBOUNCE_MS, DevError, DevFixtureAssertion, DevFixtureAssertionKind,
    DevFixtureExecutionRoots, DevFixtureExecutor, DevFixtureResult, DevFixtureStatus, DevLane,
    DevLoopOptions, DevRenderTheme, DevReport, DevReportStatus, DevWatchError, DevWatchEvent,
    DevWatchEventKind, DevWatchOptions, DevWatchSnapshot, DevWatchTrigger, LocalDevFixtureExecutor,
    ParsedDevFixture, PollingDevWatcher, PreparedDevFixtureWorkspace, collect_watch_snapshot,
    dev_receipt_metadata, discover_fixture_paths, render_dev_result, render_dev_result_with_theme,
    run_dev_once, run_dev_once_with_executor, should_ignore_dev_watch_path,
};
pub use doctor::{DoctorOptions, default_doctor_options, run_doctor};
pub use error::RuntimeError;
pub use harness::{
    HarnessExpectedStatus, HarnessFixture, HarnessFixtureError, HarnessFixtureKind,
    HarnessReceiptExpectation, HarnessReplayError, HarnessReplayOutput, HarnessReplayReceipt,
    load_harness_fixture, parse_harness_fixture, run_harness_fixture,
    run_harness_fixture_with_adapter,
};
pub use journal::{ExecutionJournal, JournalEntry};
pub use list::{
    RunxListEmit, RunxListItem, RunxListItemKind, RunxListOptions, RunxListReport,
    RunxListRequestedKind, RunxListSource, RunxListStatus, default_list_options,
    list_authoring_primitives,
};
pub use receipts::paths::{
    INIT_CWD_ENV, RUNTIME_RECEIPTS_DIR_CONFIG_KEY, RUNX_CWD_ENV, RUNX_PROJECT_DIR_ENV,
    RUNX_RECEIPT_DIR_ENV, ReceiptPathInputs, ReceiptPathSource, ReceiptStoreLabel,
    ResolvedReceiptPath, RuntimeReceiptConfig, resolve_project_runx_dir, resolve_receipt_path,
    resolve_workspace_base, safe_receipt_store_label,
};
pub use receipts::store::{
    LocalReceiptStore, ReceiptStoreError, ReceiptStoreIndex, ReceiptStoreIndexEntry,
};
pub use receipts::tree::{
    RuntimeReceiptResolver, validate_runtime_receipt_tree, verify_runtime_receipt_tree,
};
pub use registry::{RegistryInstallMetadataInput, registry_install_receipt_metadata};
#[cfg(feature = "cli-tool")]
pub use runner::run_graph_file;
pub use runner::{GraphCheckpoint, GraphRun, Runtime, RuntimeOptions, StepRun};
pub use runx_core::kernel_eval;
pub use runx_core::policy::{
    PaymentAuthorityError, PaymentRailAdmission, PaymentRailAdmissionDecision,
    PaymentRailAuthorization, PaymentRailAuthorizationDecision, PaymentSpendCapabilityBinding,
    admit_payment_rail, authorize_payment_rail, payment_authority_requires_receipt_before_success,
    payment_authority_spends,
};
pub use scaffold::{
    InitAction, InitGeneratedValues, RunxInitOptions, RunxInitResult, RunxInstallState,
    RunxNewOptions, RunxNewResult, RunxProjectState, ScaffoldError, ensure_runx_install_state,
    ensure_runx_project_state, packet_namespace_for_name, runx_init, sanitize_runx_package_name,
    scaffold_runx_package,
};
pub use skill_run::{SkillRunError, SkillRunRequest, execute_skill_run};
pub use tool_catalogs::{
    ToolBuildOptions, ToolCatalogError, ToolInspectOptions, ToolSearchOptions, build_tool_catalogs,
    inspect_tool, search_tools,
};

pub const PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");
