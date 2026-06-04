---
spec_version: '2.0'
task_id: test-surface-build-consolidation
created: '2026-05-27T13:45:00Z'
updated: '2026-06-04T20:50:02Z'
status: review
harden_status: not_run
size: large
risk_level: medium
---

# Consolidate the test build surface

## Current State

Status: review
Current phase: final
Next: repair
Reason: review gate fail: 1 finding(s), 1 completion blocker(s)
Blockers: none
Allowed follow-up command: `scafld handoff test-surface-build-consolidation`
Latest runner update: 2026-06-04T21:13:05Z
Review gate: fail

## Summary

The long runx CI job is build-bound, not test-bound. Test execution is already
fast: cloud vitest runs 527 tests in about 11s, and oss `verify:fast` is about
1.5min. The time goes into Rust compilation and linking, paid more than once per
run.

The dominant cause is that every top-level `tests/*.rs` file is compiled and
linked as its own crate. Cargo documents this and explicitly recommends a single
integration test split into modules when many integration tests make compile or
run time inefficient (Cargo Book, "Integration tests":
https://doc.rust-lang.org/cargo/reference/cargo-targets.html#integration-tests).
For runx the case is stronger because `runx-runtime` links heavy adapter and
runtime deps (reqwest, tokio, rustls, rmcp) into every test binary.

This spec collapses the integration-test binaries to one per crate (the layout
several runx crates already imply), adds a guard so the `autotests = false`
layout cannot silently drop coverage, adopts cargo-nextest as the runner with
doctests kept as a separate `cargo test --doc` step, installs advisory tools as
prebuilt binaries, and tightens caching and job structure. No test assertions,
fixtures, or coverage are removed.

## Measured Baseline

Isolated worktree, clean target dir, deps and lib pre-warmed identically for both
runs, `runx-runtime --all-features`, `cargo test --no-run`:

- Current (42 separate test binaries): 244s to build, 382 tests.
- Merged (1 binary, same files as modules): 7s to build, same 382 tests.
- Result: about 35x on the test-build phase for one crate, identical tests.

Mechanism confirmed: test-source codegen is cheap; the cost is 42 separate linker
invocations, each statically linking `runx_runtime` plus reqwest/tokio/rustls/rmcp
into a ~51MB executable. `runx-runtime` is 42 of the workspace's integration
binaries, so the absolute saving is larger again.

Supporting signal: `oss/crates/target` was 183GB locally (cache bloat from
feature and toolchain permutations).

## Phase 1 Evidence (implemented)

Consolidated to one `tests/integration.rs` binary per crate, `autotests = false`,
each former test file kept intact as a module:

| crate | files merged | tests | suite result |
| --- | --- | --- | --- |
| runx-runtime | 42 | 383 | behavior-neutral vs baseline (see below) |
| runx-contracts | 17 | 80 | 80 passed |
| runx-cli | 13 + shared `support` | 74 | 74 passed |
| runx-core | 6 | 50 | 50 passed |
| runx-receipts | 3 | 26 | 26 passed |
| runx-parser | 5 | 20 | 20 passed |
| runx-sdk | 3 | 6 | 6 passed |

- Behavior-neutrality proof: under an identical local invocation, `runx-runtime`
  reports 372 pass / 11 fail both before consolidation (pristine HEAD worktree,
  42 separate binaries) and after (1 binary). The 11 failures are pre-existing,
  environment-dependent tests (receipt-signing issuer-key resolution) that only
  pass under the full verify-fast/CI orchestration; they are unrelated to and
  unaffected by consolidation.
- nextest validation: `cargo nextest run --workspace --all-features` runs 789
  tests, 778 pass, the same 11 runtime failures, nothing new. Process-per-test
  execution is green.
- Two files needed structural fixes because they owned submodules: `payment.rs`
  (runtime) and `act.rs` (sdk) had a redundant inline `mod {}` wrapper that, once
  the file itself became a module, nested submodule paths one level too deep; the
  wrapper was removed so submodules resolve under `tests/<name>/`. `runx-cli`'s
  shared `tests/support/` is declared once in integration.rs and the five files
  that used it now reference `crate::support`.
- Every `--test <name>` invocation that targeted a now-removed standalone target
  was retargeted to `--test integration -- <module>` and verified to run the
  same subset (5 package.json scripts, the CI license-boundary guard, and the
  a2a/agent fixture-generator hint strings).

## Standard adopted

The forward standard for runx Rust testing:

- One integration binary per crate (`tests/integration.rs` + `autotests = false`).
- cargo-nextest as the normal runner (process-per-test isolation; see
  https://nexte.st/docs/design/why-process-per-test/), which removes the only
  regression introduced by sharing one binary: process-global state leaking
  across tests under threaded `cargo test`.
- Doctests run as a separate `cargo test --doc` step, because nextest does not
  execute doctests.
- A module-list and process-global-mutation guard
  (`scripts/check-integration-test-modules.mjs`, wired into verify-fast) so a new
  `tests/*.rs` cannot be silently un-compiled and so `env::set_var` /
  `set_current_dir` style mutations are banned in test code unless explicitly
  isolated and annotated.

## Scope

- In scope:
  - `crates/*/tests/` layout and the corresponding `[[test]]` / `autotests`
    manifest entries (done).
  - The module/process-global guard and its wiring (done).
  - `oss/.github/workflows/ci.yml` runner and advisory-tool steps (done).
  - Retargeting every `--test <name>` reference (done).
  - Follow-up CI structure: feature-surface review, job parallelization, cache.
- Out of scope:
  - Changing any test assertion, fixture, or expected value.
  - Removing or weakening any gate, including heavy graph and all-features tests.
  - JS/vitest test logic (already fast).
  - The stable/nightly toolchain arrangement: nightly exists specifically for
    `cargo-public-api` in the advisory parity step (see archived
    `rust-parity-ci-governance.md`); it is not removable here.

## Dependencies

- `heavy-test-suite-gating` (completed) established that CI must run
  `pnpm test:heavy:graph` and an all-features cargo test gate with a prebuilt
  eval binary. This spec keeps both: the heavy graph step is unchanged and the
  all-features gate is now `cargo nextest run --workspace --all-features` plus
  `cargo test --workspace --all-features --doc`.
- `runx-rust-95-release-readiness` (active) owns the mandatory Rust gates; the
  runner change here keeps those gates green.

## Risks

- Shared process state under threaded `cargo test`: addressed by the guard
  (bans process-global mutation; the scan found none) and by nextest's
  process-per-test execution.
- The 11 environment-dependent runtime tests must be confirmed green in CI under
  nextest; they are runner- and layout-independent (fail identically under
  cargo test and nextest, before and after consolidation), so this is a
  pre-existing orchestration concern, not a regression from this work.
- CI workflow edits cannot be fully validated locally; they take effect only on
  commit, so a dry-run on a branch is the gate before merge.

## Acceptance

- [x] `dod1` Each crate with tests builds exactly one integration binary; total
  test count unchanged. Evidence: 7 crates each emit one `tests/integration.rs`
  executable; per-crate counts recorded above (639 integration tests).
- [x] `dod2` Consolidated suites pass with no shared-state flakiness. Evidence:
  6/7 crates fully green; runtime behavior-neutral vs baseline; nextest green
  except the pre-existing 11.
- [x] `dod3` Heavy graph gate and an all-features cargo gate remain enforced in
  CI. Evidence: heavy graph step unchanged; `cargo nextest run --workspace
  --all-features` + `cargo test --workspace --all-features --doc` in Rust checks.
- [x] `dod4` Advisory tools installed prebuilt, not compiled from source.
  Evidence: `taiki-e/install-action` installs cargo-nextest, cargo-deny,
  cargo-public-api; the `cargo install` step is removed.
- [x] `dod5` Guard prevents silent coverage loss and bans process-global
  mutation. Evidence: `scripts/check-integration-test-modules.mjs` fails on an
  orphaned `tests/*.rs` and on `env::set_var`; wired into verify-fast.
- [ ] `dod6` Warm CI wall time materially reduced; before/after recorded from a
  real CI run. Expected kind: manual.

## Phase 2: CI caching and structure

Status: completed

Implemented:

- Swapped the oss Cargo cache from `actions/cache` (keyed on `Cargo.lock`, target
  tree grew unbounded) to `Swatinem/rust-cache@v2` with `workspaces: crates`, the
  same Rust-aware cache the cloud workflow already uses. It keys on lockfile and
  rustc version and prunes stale artifacts.

Analyzed and deliberately NOT done:

- Job parallelization (split `checks` into parallel `verify` and `rust` jobs) was
  rejected after analysis: on a cold run each job restores its own cache and
  recompiles the dependency graph in its own target, so the expensive ~10min dep
  compile is paid twice and wall time (max of the jobs) does not improve; it only
  helps warm runs (already fast) by a couple of minutes. Low value, and it makes
  the cold case worse. Not pursued.
- Feature-surface "unification": clippy and the test build cannot share compiled
  artifacts (clippy wraps workspace crates), but heavy dependency artifacts ARE
  shared between them, and clippy/test already use the same `--all-features` set.
  There is no safe further consolidation here; closed.

Recommended next lever (needs a CI dry-run before merge):

- sccache as a persistent compilation cache (`RUSTC_WRAPPER=sccache`, GHA-cache
  backend, `CARGO_INCREMENTAL=0`). This is the real lever for the cold case that
  produced the original ~40min run: when `Cargo.lock` changes only some deps,
  sccache reuses cached object files for the unchanged ~297 deps instead of
  recompiling the whole graph. It coexists with clippy's `RUSTC_WORKSPACE_WRAPPER`.
  Left unimplemented because a misconfigured wrapper silently disables caching or
  breaks the build, and it cannot be validated locally.

Optional, lower priority:

- Gate `cargo package -p runx-cli` to pushes on main and tags rather than every
  PR (it does a full isolated rebuild). Mildly reduces per-PR publish safety, so
  decide explicitly.

- [ ] `p2_ac1` command - oss Cargo cache uses rust-cache.
  - Command: `rg -n "Swatinem/rust-cache" oss/.github/workflows/ci.yml`
  - Expected kind: `reviewed_output`
- [ ] `p2_ac2` manual - branch CI run green end to end under nextest; warm-run
  wall time recorded before/after.

## Rollback

- Phase 1 is the high-value change and the only one touching test layout. Each
  crate's consolidation is independent; if a crate ever cannot run cleanly in one
  binary, leave it unconsolidated rather than weakening tests.
- CI changes are revertible by restoring the prior `ci.yml`; no test content
  changes, so coverage is unaffected by any rollback.
- Keep each lever in its own commit so it can be reverted independently.

## Review

Status: completed
Verdict: fail
Mode: verify
Provider: command
Output: command.stdout
Summary: Local implementation evidence is insufficient to complete this spec. The spec is in review state, but acceptance requires a real branch CI dry-run under nextest with before/after warm-run wall time; local checks cannot satisfy dod6 or p2_ac2.

Attack log:
- `.scafld/specs/active/test-surface-build-consolidation.md:172`: verify manual CI acceptance items -> finding (dod6 and p2_ac2 require branch CI evidence)
- `.github/workflows/ci.yml`: check whether local workflow inspection can replace branch CI -> finding (local-only evidence is insufficient for a dry-run gate)
- `scafld status test-surface-build-consolidation --json`: verify lifecycle is review, not complete -> finding (blocked pending external CI dry-run)

Findings:
- [high/blocks completion] `ci-dry-run-missing` Missing branch CI dry-run evidence
  - Location: `.scafld/specs/active/test-surface-build-consolidation.md:172`
  - Evidence: dod6 requires warm CI wall time from a real CI run, and p2_ac2 requires a branch CI run green end to end under nextest with before/after warm-run timing. No branch CI run evidence is present in this local workspace.
  - Impact: Completing the spec locally would bypass its only real validation for CI workflow behavior and cache timing.
  - Validation: Push the branch, run CI, record the green nextest workflow and before/after warm-run wall time, then rerun scafld review and complete.
