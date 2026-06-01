//! Single integration-test binary for runx-runtime.
//!
//! Each module below is one integration test file. They are compiled and
//! linked once as a single binary instead of one binary per file; see
//! .scafld/specs/active/test-surface-build-consolidation.md. `autotests = false`
//! in Cargo.toml keeps Cargo from building each file as its own binary.

mod a2a_parity;
mod abnormal_seal;
mod agent_parity;
mod approval;
mod catalog_adapter;
mod cli_tool_contract;
mod config;
mod credential_delivery;
mod credential_grant_policy;
mod dev;
mod doctor;
mod external;
mod external_adapter;
mod fanout_parity;
mod fanout_proptest;
mod harness_fixtures;
mod hello_graph;
mod journal_history;
mod license_boundary;
mod local_credential_provision;
mod mcp_adapter;
mod mcp_server;
mod parity;
#[cfg(feature = "payment-rails")]
mod payment_finality;
mod post_merge_observer;
mod receipt_paths;
mod receipt_refs;
mod receipt_signing;
mod receipt_store;
mod receipt_tree;
mod registry;
mod registry_client;
mod registry_install;
mod scaffold;
mod sensitive_text_redaction;
mod skill_author_runtime_fixtures;
mod skill_issue_intake;
mod skill_issue_to_pr;
mod skill_run;
mod support;
mod target_runner;
mod thread_outbox_provider;
mod tool_catalogs;
