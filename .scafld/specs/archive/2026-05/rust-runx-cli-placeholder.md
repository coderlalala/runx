---
spec_version: '2.0'
task_id: rust-runx-cli-placeholder
created: '2026-05-15T13:05:00Z'
updated: '2026-05-19T01:29:51Z'
status: completed
harden_status: not_run
size: small
risk_level: low
---

# Rust runx-cli placeholder crate

## Current State

Status: completed
Current phase: final
Next: done
Reason: task completed
Blockers: none
Allowed follow-up command: `none`
Latest runner update: 2026-05-19T01:29:51Z
Review gate: pass

## Summary

Add the initial Cargo package for runx CLI distribution. The crates.io package
name is `runx-cli` because `runx` is already taken by an unrelated crate, but
the installed binary must be named `runx`.

This is a placeholder/distribution crate, not a rewrite of the CLI runtime. It
delegates to the authoritative npm CLI by default and provides a local JS
entrypoint override for development.

## Context

CWD: `.`

Packages:
- `crates/runx-cli`
- `@runxhq/cli`

Files impacted:
- `crates/Cargo.toml`
- `crates/README.md`
- `crates/rustfmt.toml`
- `crates/runx-cli/Cargo.toml`
- `crates/runx-cli/README.md`
- `crates/runx-cli/src/lib.rs`
- `crates/runx-cli/src/launcher.rs`
- `crates/runx-cli/src/main.rs`
- `.gitignore`
- `.github/workflows/ci.yml`

Invariants:
- The npm package `@runxhq/cli` remains the authoritative implementation.
- The Cargo package name is `runx-cli`; the binary name is `runx`.
- The launcher must not implement runx runtime behavior, parse skill contracts,
  execute MCP, write receipts, or duplicate TypeScript CLI semantics.
- Default delegation uses the latest npm CLI unless explicitly pinned.
- Development delegation through `RUNX_JS_BIN` must execute a local JS entrypoint
  through Node without shell interpolation.

Related docs:
- `docs/trusted-kernel-package-truth.md`
- `plans/runx.md`
- `crates/README.md`
- `rust-cli-feature-parity-matrix`

## Objectives

- Create a modern Rust 2024 Cargo workspace under `crates/`.
- Create a publishable `runx-cli` package that installs a `runx` binary.
- Delegate by default to `npm exec --yes --package @runxhq/cli@latest -- runx`.
- Support `RUNX_NPM_PACKAGE` for pinned npm CLI versions.
- Support `RUNX_JS_BIN` for local checkout development.
- Add unit coverage for launcher planning without spawning npm/node.
- Add CI checks for formatting, clippy, tests, and packaging if not already
  present.

## Scope

In scope:
- Cargo workspace metadata.
- Thin launcher crate.
- Dependency-free or near dependency-free launcher implementation.
- Basic launcher tests.
- Cargo package metadata and README.
- Rust CI check wiring.

Out of scope:
- Rust implementation of `runx` command semantics.
- Kernel parity, policy, state-machine, runtime-local, MCP, A2A, receipt, or
  provider adapter ports.
- Any claim that the Cargo binary is feature-equivalent to the npm CLI.
- Publishing to crates.io.
- Replacing npm distribution.

## Dependencies

- crates.io package name `runx-cli` is available at planning time.
- The exact `runx` crate name is unavailable, so this spec intentionally avoids
  claiming it.
- Rust toolchain is not installed in the current local environment; final
  validation may need CI or a machine with Rust installed.

## Assumptions

- Users who install through Cargo accept requiring Node.js/npm until the Rust
  runtime becomes real.
- `@runxhq/cli@latest` is the desired default for the placeholder crate.
- Pinning remains possible through `RUNX_NPM_PACKAGE` for reproducibility.
- Keeping the launcher dependency-free is preferable for supply-chain and
  package-review simplicity.

## Touchpoints

- Cargo package metadata.
- Binary name collision and PATH behavior.
- npm package invocation.
- local JS development override.
- CI Rust setup.
- `.gitignore` for Cargo `target/`.

## Risks

- Medium: users may assume `cargo install runx-cli` gives a self-contained
  native implementation. README and shim help must be explicit that Node/npm are
  still required.
- Medium: defaulting to latest npm CLI favors freshness over reproducibility.
  Pinning through `RUNX_NPM_PACKAGE` must be documented.
- Low: Cargo package checks cannot be locally verified without Rust installed.

## Acceptance

Profile: standard

Validation:
- [x] `v1` command - Cargo package metadata is valid.
  - Command: `cargo metadata --manifest-path crates/Cargo.toml --format-version 1 --no-deps`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-39
- [x] `v2` command - Rust launcher checks pass.
  - Command: `cargo fmt --manifest-path crates/Cargo.toml --all --check && cargo clippy --manifest-path crates/Cargo.toml --workspace --all-targets -- -D warnings && cargo test --manifest-path crates/Cargo.toml --workspace && cargo package --manifest-path crates/Cargo.toml -p runx-cli`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-40
- [x] `v3` command - default latest npm package is visible in source and docs.
  - Command: `rg -n '@runxhq/cli@latest' crates/runx-cli`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-41
- [x] `v4` command - binary name is `runx`.
  - Command: `rg -n 'name = "runx"' crates/runx-cli/Cargo.toml`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-42
- [x] `v5` command - TypeScript fast verification remains green.
  - Command: `pnpm verify:fast`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-43
- [x] `v6` command - placeholder docs avoid feature-parity claims.
  - Command: `! rg -n 'self-contained|native implementation|feature.?equivalent|drop-in replacement' crates/runx-cli/README.md crates/README.md`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-44

## Phase 1: Cargo workspace and launcher

Status: completed
Dependencies: none

Objective: Complete this phase.

Changes:
- `crates/Cargo.toml` (all, exclusive) - Define Rust 2024 workspace, resolver, MSRV, package metadata, and shared lints.
- `crates/rustfmt.toml` (all, exclusive) - Set formatting defaults.
- `crates/runx-cli/Cargo.toml` (all, exclusive) - Define package metadata and binary named `runx`.
- `crates/runx-cli/src/main.rs` (all, exclusive) - Implement process execution boundary only.
- `crates/runx-cli/src/lib.rs` (all, exclusive) - Export testable launcher planning code.
- `crates/runx-cli/src/launcher.rs` (all, exclusive) - Plan delegation to npm or local JS entrypoint and test the decision logic.

Acceptance:
- [x] `ac1_1` command - package installs a `runx` binary by metadata.
  - Command: `rg -n 'name = "runx"' crates/runx-cli/Cargo.toml`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-6
- [x] `ac1_2` command - launcher has no third-party dependencies.
  - Command: `! rg -n '^\\[dependencies\\]|clap|anyhow|tokio|reqwest|rmcp' crates/runx-cli/Cargo.toml crates/runx-cli/src`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-7
- [x] `ac1_3` command - launcher defaults to latest npm CLI.
  - Command: `rg -n '@runxhq/cli@latest' crates/runx-cli/src crates/runx-cli/README.md`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-8

## Phase 2: Documentation and CI

Status: completed
Dependencies: Phase 1

Objective: Complete this phase.

Changes:
- `crates/README.md` (all, exclusive) - Document Rust workspace commands and current placeholder status.
- `crates/runx-cli/README.md` (partial, exclusive) - Document Cargo install, Node/npm requirement, `RUNX_NPM_PACKAGE`, `RUNX_JS_BIN`, and shim flags.
- `.gitignore` (partial, shared) - Ignore Cargo `target/`.
- `.github/workflows/ci.yml` (partial, shared) - Add Rust check steps if this repo wants CI coverage immediately.

Acceptance:
- [x] `ac2_1` command - README states Node/npm requirement.
  - Command: `rg -n 'Node\\.js|npm|@runxhq/cli@latest|RUNX_NPM_PACKAGE|RUNX_JS_BIN' crates/runx-cli/README.md`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-23
- [x] `ac2_2` command - Cargo target is ignored.
  - Command: `rg -n 'target/' .gitignore`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-24
- [x] `ac2_3` command - CI contains Rust checks or docs state local-only.
  - Command: `rg -n 'cargo fmt|cargo clippy|cargo test|runx-cli|local-only' .github/workflows/ci.yml crates/README.md`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-25

## Rollback

Strategy: per_phase

Commands:
- Remove `crates/runx-cli`.
- Remove `crates/Cargo.toml`, `crates/README.md`, and `crates/rustfmt.toml` if
  no other Rust crates remain.
- Revert `.gitignore` and CI workflow changes introduced by this spec.

## Review

Status: completed
Verdict: pass
Mode: discover
Provider: command
Output: command.stdout
Summary: Rust runx-cli placeholder lifecycle is complete. The crate is a thin npm launcher, installs a runx binary, has unit coverage, and final validation passed with the crates manifest path.

Attack log:
- `launcher scope`: Confirmed task remains a launcher placeholder and does not implement runtime behavior -> clean (No new code changes in this pass; existing crate delegates to npm/local JS.)
- `acceptance`: Checked scafld acceptance status -> clean (Cargo metadata, fmt, clippy, tests, package, pnpm verify:fast, and docs checks passed.)
- `docs`: Checked README requirements and no feature parity claim -> clean (Docs state Node/npm requirement and avoid native parity claims.)

Findings:
- none

## Self Eval

Status: not_started
Completeness: none
Architecture fidelity: none
Spec alignment: none
Validation depth: none
Total: none
Second pass performed: none

Notes:
none

Improvements:
- none

## Deviations

- none

## Metadata

Estimated effort hours: 2
Actual effort hours: none
AI model: none
React cycles: none

Tags:
- rust
- cargo
- cli
- placeholder

## Origin

Source:
- user requested the `runx-cli` placeholder crate.

Repo:
- runxhq/runx

Git:
- none

Sync:
- none

Supersession:
- precedes: rust-kernel-parity-fixtures
- precedes: rust-cli-feature-parity-matrix

## Harden Rounds

- none

## Planning Log

- 2026-05-15T13:05:00Z: Drafted as Cargo placeholder crate plan.
