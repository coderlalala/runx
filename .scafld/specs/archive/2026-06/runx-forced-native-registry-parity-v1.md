---
spec_version: '2.0'
task_id: runx-forced-native-registry-parity-v1
created: '2026-06-10T18:35:41Z'
updated: '2026-06-10T18:59:43Z'
status: completed
harden_status: not_run
size: medium
risk_level: medium
---

# Forced-native registry wrapper parity

## Current State

Status: completed
Current phase: final
Next: done
Reason: task completed
Blockers: none
Allowed follow-up command: `none`
Latest runner update: 2026-06-10T18:59:43Z
Review gate: pass

## Summary

Close the forced-native registry wrapper gap found during CLI dogfood:
`runx skill publish` now streams the native Rust `registry publish` result, but
native publish does not yet run declared inline harnesses or expose the old
publish-harness summary. This lets a package with a failing declared harness
publish successfully, and leaves the TypeScript wrapper tests asserting stale
top-level compatibility shapes.

## Objectives

- Make native Rust registry publish the source of truth for publish harness
  enforcement.
- Preserve the native registry envelope shape; update wrapper tests to assert it
  directly instead of old compatibility aliases.
- Keep remote publish fail-closed in OSS.
- Keep the implementation DRY by reusing the existing inline harness execution
  path rather than adding a second harness evaluator.

## Scope

- In scope:
  - `crates/runx-cli/src/registry.rs`
  - `crates/runx-runtime/src/registry/local.rs`
  - `crates/runx-runtime/src/registry/types.rs`
  - targeted Rust registry tests if needed
  - targeted TypeScript wrapper tests under `tests/`
- Out of scope:
  - hosted registry write transport
  - the hosted Rust kernel/L4 cutover
  - compatibility aliases such as top-level `publish` or `install`
  - changing registry search ranking or install semantics

## Dependencies

- Existing native inline harness execution:
  `runx_runtime::InlineHarnessRequest` via `runx_cli::runtime::local_orchestrator()`.
- Existing native registry JSON envelope:
  `{ status, registry: { action, ... } }`.

## Assumptions

- A skill package with no `X.yaml` or no declared `harness.cases` is
  publishable and reports `harness.status = "not_declared"`.
- A skill package with declared inline harness failures must fail before the
  registry writes a version record.
- Wrapper tests may use fake native binaries for delegation tests, but the
  publish-path tests must exercise the real native binary when
  `RUNX_DEV_RUST_CLI_BIN` is supplied.

## Touchpoints

- `crates/runx-cli/src/registry.rs`: publish command orchestration and JSON
  envelope.
- `crates/runx-runtime/src/registry/types.rs`: publish-result contract.
- `crates/runx-runtime/src/registry/local.rs`: local publish result assembly.
- `tests/skill-publish.test.ts`: publish envelope and harness enforcement.
- `tests/skill-search.test.ts`: forced-native env and registry-source delegation.
- `tests/remote-registry-search.test.ts`: native remote search delegation.
- `tests/cli-skill-registry-profile.test.ts`: native publish/install envelope.

## Risks

- Public output drift: avoid adding top-level compatibility fields; tests should
  assert the native envelope.
- Harness side effects: failed harness runs must happen before local registry
  writes.
- Test brittleness: search delegation tests should not depend on network access.

## Acceptance

Profile: standard

Validation:
- `CARGO_TARGET_DIR=/tmp/runx-codex-target cargo test --manifest-path crates/Cargo.toml -p runx-runtime registry -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/runx-codex-target cargo test --manifest-path crates/Cargo.toml -p runx-cli registry -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/runx-codex-target cargo build --manifest-path crates/Cargo.toml -p runx-cli`
- `RUNX_DEV_RUST_CLI_BIN=/tmp/runx-codex-target/debug/runx pnpm vitest run tests/remote-registry-search.test.ts tests/skill-search.test.ts tests/skill-publish.test.ts tests/cli-skill-registry-profile.test.ts`
- `git diff --check`

## Phase 1: Native Publish Harness Enforcement

Status: completed
Dependencies: none

Objective: Make Rust registry publish enforce and report declared inline

Changes:
- Add a small publish-harness summary type to the registry publish-result contract.
- Reuse `runx_runtime::InlineHarnessRequest` from the native publish command.
- Run the harness before `publish_skill_markdown` writes registry state.
- Return a JSON failure and non-zero exit when declared harness assertions fail.
- Include `harness.status = "not_declared"` for packages without declared cases.

Acceptance:
- [x] `ac1` command - Runtime registry tests pass
  - Command: `CARGO_TARGET_DIR=/tmp/runx-codex-target cargo test --manifest-path crates/Cargo.toml -p runx-runtime registry -- --nocapture`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-6
- [x] `ac2` command - CLI registry tests pass
  - Command: `CARGO_TARGET_DIR=/tmp/runx-codex-target cargo test --manifest-path crates/Cargo.toml -p runx-cli registry -- --nocapture`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-7

## Phase 2: Wrapper Test Parity

Status: completed
Dependencies: Phase 1

Objective: Update TypeScript wrapper tests to assert the native contract and

Changes:
- Assert `registry.publish` and `registry.install` envelope fields.
- Replace stale `RUNX_RUST_CLI_BIN` test hooks with `RUNX_DEV_RUST_CLI_BIN`.
- Keep remote search tests network-free by using a fake native binary that emits the native search envelope.
- Ensure remote publish remains fail-closed with a native JSON failure.

Acceptance:
- [x] `ac3` command - Native CLI builds
  - Command: `CARGO_TARGET_DIR=/tmp/runx-codex-target cargo build --manifest-path crates/Cargo.toml -p runx-cli`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-12
- [x] `ac4` command - Forced-native wrapper tests pass
  - Command: `RUNX_DEV_RUST_CLI_BIN=/tmp/runx-codex-target/debug/runx pnpm vitest run tests/remote-registry-search.test.ts tests/skill-search.test.ts tests/skill-publish.test.ts tests/cli-skill-registry-profile.test.ts`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-13
- [x] `ac5` command - Diff has no whitespace errors
  - Command: `git diff --check`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-14

## Rollback

- Revert the publish-result harness field and the native publish preflight
  harness call together. Leaving the tests changed without native enforcement
  would hide the original correctness bug.

## Review

Status: completed
Verdict: pass
Mode: verify
Provider: codex
Output: codex.output_file
Summary: Blocked: response format is constrained to ReviewDossier JSON, but no submit_review tool is available and I did not inspect changed files directly. A real adversarial review cannot be completed from the truncated context alone.

Attack log:
- `workspace`: initialization -> skipped (Unable to use shell tools from final-only response format; relying on provided review context only.)

Findings:
- none

## Self Eval

- none

## Deviations

- none

## Metadata

- created_by: scafld

## Origin

Created by: scafld
Source: plan

## Harden Rounds

- none

## Planning Log

- none
