//! Execution cluster.
//!
//! - `runner`: the `Runtime` graph engine and step orchestrator.
//! - `graph`: graph loading and step lookup helpers.
//! - `fanout`: fanout policy helpers shared across runner and harness.
//! - `harness`: harness fixture replay and assertion engine.
//! - `orchestrator`: canonical entrypoint for local skill, graph, and harness
//!   execution.
//! - `skill_run`: top-level skill-run orchestration.
//! - `target_runner`: target-repo runner dispatch helpers.

pub(crate) mod fanout;
pub(crate) mod graph;
pub mod harness;
pub mod orchestrator;
pub mod runner;
pub mod skill_run;
pub mod target_runner;
