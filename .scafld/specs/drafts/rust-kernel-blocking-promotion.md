---
spec_version: '2.0'
task_id: rust-kernel-blocking-promotion
created: '2026-05-17T00:30:00Z'
updated: '2026-05-20T00:43:00Z'
status: draft
harden_status: not_run
size: medium
risk_level: high
---

# Rust kernel blocking promotion

## Current State

Status: draft
Current phase: evidence script landed; soak evidence pending
Next: record advisory-start evidence, then run the clean-kernel counter against
live GitHub metadata or an audited operator fixture
Reason: CI still marks Rust kernel parity advisory. The obsolete umbrella
orchestration spec has been superseded by narrow slices, and clean-PR counter
semantics are now locked by `rust-kernel-clean-pr-counter-semantics`.
Advisory-start evidence and five qualifying post-advisory PRs are still
missing.
Blockers: advisory start timestamp recorded; 5 clean kernel-touching PRs landed
while Rust kernel parity checks are advisory
Allowed follow-up command: run the evidence script against audited evidence; do
not run `scafld harden rust-kernel-blocking-promotion`.
Latest runner update: 2026-05-20 clean-kernel counter semantics slice landed
Review gate: not_started

## Summary

Promote Rust kernel parity from advisory CI signal to blocking CI gate after
the advisory soak proves the dual-tree workflow is sustainable. This spec is
deliberately separate from `rust-parity-ci-governance` because scafld specs are
completed once; the advisory-to-blocking flip is a later operational decision,
not a phase that can be reopened inside the completed governance spec.

The trigger is not calendar time. The trigger is 5 clean kernel-touching PRs
merged after advisory CI lands. A clean kernel-touching PR touches
`packages/core/src/state-machine/` or `packages/core/src/policy/`, runs the
Rust parity checks, and either passes them directly or includes an intentional
fixture refresh that makes both TypeScript and Rust pass.

Refresh note, 2026-05-20: current CI still contains
`continue-on-error: true` on the `Advisory Rust kernel parity` step, so Phase A
is still advisory. `scripts/count-clean-kernel-prs.ts`, fixture data, tests,
and counter semantics are present and pass against local fixtures, but the
Evidence section below is still intentionally unfilled for live post-advisory
PRs. This draft must not be treated as ready for CI promotion.

## Context

CWD: `.`

Packages:
- `@runxhq/core`
- `crates/runx-core`
- `crates/runx-contracts`
- `crates/runx-parser`

Files impacted:
- `.github/workflows/ci.yml`
- `scripts/count-clean-kernel-prs.ts`
- `docs/rust-kernel-architecture.md`
- `docs/trusted-kernel-package-truth.md`
- `CONTRIBUTING.md`

Invariants:
- TypeScript remains authoritative until a later cutover spec changes a
  consumer.
- Rust kernel parity checks do not become blocking until 5 clean
  kernel-touching PRs are evidenced.
- The promotion only affects Rust kernel parity checks added by
  `rust-parity-ci-governance`. It must not remove existing Rust launcher CI
  checks or change npm CLI release behavior.
- The CLI feature-parity matrix remains required for any runtime or CLI
  cutover. Blocking kernel parity is not a runtime cutover.
- If the 5-PR evidence is missing or ambiguous, this spec blocks.
- Promotion does not make Rust authoritative. TypeScript remains the source of
  truth until a separate cutover spec changes consumers.
- Parser-only PRs are not counted toward the original 5-PR trigger unless this
  spec is explicitly broadened and hardened again. The current trigger is
  state-machine/policy soak evidence.

Related docs:
- `docs/rust-kernel-architecture.md`
- `docs/trusted-kernel-package-truth.md`
- `.scafld/specs/drafts/rust-kernel-port-orchestration.md`
- `.scafld/specs/drafts/rust-kernel-clean-pr-counter-semantics.md`
- `.scafld/specs/drafts/rust-parity-ci-governance.md`

## Objectives

- Add a deterministic way to count clean kernel-touching PRs.
- Verify at least 5 qualifying PRs landed while Rust kernel checks were
  advisory.
- Remove `continue-on-error: true` from the Rust kernel parity checks added by
  `rust-parity-ci-governance`.
- Update docs to state Phase B is active and kernel parity is blocking.
- Keep runtime, MCP, adapter, parser, receipt, and CLI cutover work out of
  scope.

## Scope

In scope:
- Evidence collection for the 5 clean kernel-touching PR trigger.
- CI promotion from advisory to blocking for Rust kernel parity checks.
- Documentation updates that mark Phase B active.

Out of scope:
- Any Rust implementation changes.
- Any TypeScript kernel behavior changes.
- Any parser, receipt, runtime-local, MCP, adapter, or CLI cutover.
- Publishing Cargo crates.

## Dependencies

- `rust-kernel-port-orchestration` marked obsolete as written and superseded
  by fresh executable slices.
- `rust-kernel-clean-pr-counter-semantics` completed locally.
- `rust-parity-ci-governance` completed with advisory CI checks present.
- GitHub CLI or equivalent CI metadata access is available for counting merged
  PRs. If not available, the operator must provide audited PR evidence and the
  script must support a checked-in fixture mode.

## Assumptions

- The advisory start timestamp should be recorded in a replacement audited
  evidence block. The obsolete `rust-kernel-port-orchestration` file has no
  valid Phase Receipts and must not be backfilled. If audited evidence does not
  exist, the script must refuse to infer it from file timestamps or git history
  alone.
- The CI workflow names for Rust kernel parity checks are stable enough for
  `scripts/count-clean-kernel-prs.ts` to identify them.
- Some qualifying PRs may include fixture refreshes. That is acceptable only if
  the PR also shows both TypeScript and Rust parity passing after refresh.

## Risks

- High: promoting too early can block normal TypeScript kernel work. Mitigated
  by the 5 clean PR evidence gate.
- High: PR counting can be wrong if CI check names change. Mitigated by a
  script with explicit check-name configuration and fixture tests.
- Medium: advisory checks may have been bypassed manually. Mitigated by
  requiring PR-level evidence, not just current branch green status.
- Medium: CI promotion can accidentally affect existing launcher checks.
  Mitigated by limiting the workflow diff to the parity checks added by
  `rust-parity-ci-governance`.

## 2026-05-20 Refresh Findings

Observed current state:
- `.github/workflows/ci.yml` has a blocking `Rust checks` step for cargo
  fmt/clippy/test/package, followed by `Advisory Rust kernel parity` with
  `continue-on-error: true`.
- `package.json` maps `pnpm rust:check` to
  `node scripts/check-rust-kernel-parity.mjs`.
- `scripts/check-rust-kernel-parity.mjs` runs Cargo fmt, clippy, workspace
  tests, crate-graph/style guards, cargo-deny, and the `runx-core` public API
  snapshot unless `--api-only` is used.
- `docs/trusted-kernel-package-truth.md` still says CI is advisory during
  Phase A and becomes blocking only through this spec after five clean
  kernel-touching PRs.
- `scripts/count-clean-kernel-prs.ts` exists with fixture-mode tests and
  fails closed without advisory-start evidence.
- Clean PR evidence remains `<to be filled at exec time>`.

## Gate Classification

Blocking before this spec may promote CI:
- The advisory start point must be recorded as explicit evidence.
- `scripts/count-clean-kernel-prs.ts` must verify at least five qualifying
  post-advisory PRs from live metadata or audited evidence.
- `node scripts/check-rust-kernel-parity.mjs` must pass locally before the CI
  `continue-on-error` line is removed.

Advisory until those blockers clear:
- Existing CI `Advisory Rust kernel parity`.
- Cargo-deny and public API snapshot enforcement inside CI, because they are
  currently inside the advisory parity step.
- Rust-only maintenance PRs that do not exercise TypeScript kernel drift.

Non-goals for this promotion:
- Runtime, parser, receipt, adapter, MCP, SDK, or CLI cutover.
- Making Rust policy authoritative for runtime-local execution.
- Changing runtime payment code or payment rails.

## Clean PR Evidence Rules

The counting script must fail closed:
- Qualifying PRs must merge after the recorded advisory start.
- A qualifying PR must touch the authoritative TypeScript state-machine or
  policy surface, or a deliberate kernel fixture/oracle refresh tied to that
  surface.
- A Rust-only `crates/runx-core` maintenance PR can be recorded as advisory
  evidence, but it does not count toward the five-PR promotion trigger unless
  this spec is explicitly broadened.
- Fixture refresh PRs qualify only when both the TypeScript oracle and Rust
  parity pass after refresh.
- Missing, renamed, skipped, or failed parity checks make the PR non-qualifying
  unless audited evidence is checked into the fixture mode.
- Parser-only PRs remain out of the five-PR count for this draft, even though
  parser is a pure TypeScript core domain and `runx-parser` now exists.

## Next Executable Slices

Execute these in order:
- Advisory-start evidence slice: record the exact advisory start point from an
  audited source before live PR counting is allowed.
- Soak verification slice: run the script against live GitHub metadata or an
  operator-provided audited fixture and fill the Evidence section.
- CI flip slice: remove only the `continue-on-error: true` on the Rust kernel
  parity step after all blockers pass.
- Docs coherence slice: update docs to say Phase B is active only after the CI
  flip is made. Until then, docs must keep Phase A/advisory language.

## Acceptance

Profile: strict
Self-eval threshold: 8
Review provider: external Claude; local review does not satisfy
complete.
Harden required before approve: yes

Definition of done:
- [x] `dod1` `scripts/count-clean-kernel-prs.ts` exists and has fixture tests.
- [ ] `dod2` The script verifies at least 5 clean kernel-touching PRs.
- [ ] `dod3` Rust kernel parity checks are blocking in CI.
- [ ] `dod4` Docs state Phase B is active and TS remains authoritative until
  cutover.
- [ ] `dod5` Runtime/CLI cutover language still points to the CLI
  feature-parity matrix.

Validation:
- [ ] `v1` command - Rust kernel parity still passes.
  - Command: `node scripts/check-rust-kernel-parity.mjs`
  - Expected kind: `exit_code_zero`
  - Status: pending
- [ ] `v2` command - 5 clean kernel-touching PRs are evidenced.
  - Command: `pnpm exec tsx scripts/count-clean-kernel-prs.ts --min 5`
  - Expected kind: `exit_code_zero`
  - Timeout seconds: 60
  - Status: pending
- [ ] `v3` command - Rust kernel parity checks are no longer advisory.
  - Command: `! rg -n 'continue-on-error: true' .github/workflows/ci.yml | rg -qE 'cargo-deny|cargo public-api|check-rust-kernel-parity'`
  - Expected kind: `exit_code_zero`
  - Status: pending
- [ ] `v4` command - docs state blocking phase is active.
  - Command: `rg -n 'Phase B.*active|blocking.*kernel parity|kernel parity.*blocking' docs CONTRIBUTING.md`
  - Expected kind: `exit_code_zero`
  - Status: pending

## Phase 1: Evidence script

Goal: Add a script that identifies clean kernel-touching PRs and can be tested
without live GitHub access.

Status: completed
Dependencies: none

Changes:
- `scripts/count-clean-kernel-prs.ts` (all, exclusive) - Count merged PRs that
  touch `packages/core/src/state-machine/` or `packages/core/src/policy/` and
  have passing Rust kernel parity checks.
- `tests/count-clean-kernel-prs.test.ts` (all, exclusive) - Fixture tests for
  clean PRs, non-kernel PRs, missing CI checks, failed CI checks, and fixture
  refresh PRs.

Acceptance:
- [x] `ac1_1` command - script tests pass.
  - Command: `pnpm exec vitest run --config vitest.config.ts tests/count-clean-kernel-prs.test.ts`
  - Expected kind: `exit_code_zero`
  - Status: passed 2026-05-20
- [x] `ac1_2` command - script exposes fixture mode.
  - Command: `pnpm exec tsx scripts/count-clean-kernel-prs.ts --fixture tests/fixtures/clean-kernel-prs.json --min 1`
  - Expected kind: `exit_code_zero`
  - Status: passed 2026-05-20 by stricter local fixture run with `--min 3`

## Phase 2: Soak verification

Goal: Verify 5 clean kernel-touching PRs using live metadata or audited
operator-provided fixture evidence.

Status: pending
Dependencies: Phase 1

Acceptance:
- [ ] `ac2_1` command - 5 qualifying PRs found.
  - Command: `pnpm exec tsx scripts/count-clean-kernel-prs.ts --min 5`
  - Expected kind: `exit_code_zero`
  - Timeout seconds: 60
  - Status: pending
- [ ] `ac2_2` command - evidence is recorded in this spec.
  - Command: `rg -n 'Clean PR evidence: filled' .scafld/specs/drafts/rust-kernel-blocking-promotion.md`
  - Expected kind: `exit_code_zero`
  - Status: pending

## Phase 3: CI promotion

Goal: Remove advisory markers from the Rust kernel parity checks only.

Status: pending
Dependencies: Phase 2

Changes:
- `.github/workflows/ci.yml` (partial, shared) - Remove `continue-on-error:
  true` from Rust kernel parity checks added by `rust-parity-ci-governance`.
  Do not alter pre-existing Rust launcher checks.

Acceptance:
- [ ] `ac3_1` command - parity checks are blocking.
  - Command: `! rg -n 'continue-on-error: true' .github/workflows/ci.yml | rg -qE 'cargo-deny|cargo public-api|check-rust-kernel-parity'`
  - Expected kind: `exit_code_zero`
  - Status: pending
- [ ] `ac3_2` command - parity wrapper still passes.
  - Command: `node scripts/check-rust-kernel-parity.mjs`
  - Expected kind: `exit_code_zero`
  - Status: pending

## Phase 4: Docs

Goal: Mark Phase B active while preserving the TypeScript source-of-truth and
CLI feature-parity cutover rules.

Status: pending
Dependencies: Phase 3

Changes:
- `docs/rust-kernel-architecture.md` (partial, shared) - Mark Phase B active.
- `docs/trusted-kernel-package-truth.md` (partial, shared) - State that Rust
  kernel parity blocks kernel changes but does not authorize runtime/CLI
  cutover.
- `CONTRIBUTING.md` (partial, shared) - Document the blocking local check.

Acceptance:
- [ ] `ac4_1` command - docs mention Phase B active.
  - Command: `rg -n 'Phase B.*active|blocking.*kernel parity|kernel parity.*blocking' docs CONTRIBUTING.md`
  - Expected kind: `exit_code_zero`
  - Status: pending
- [ ] `ac4_2` command - docs still preserve CLI cutover gate.
  - Command: `rg -n 'cli-feature-parity|feature-parity matrix|runtime.*cutover|CLI.*cutover' docs CONTRIBUTING.md`
  - Expected kind: `exit_code_zero`
  - Status: pending

## Rollback

Strategy: per_phase

Commands:
- Phase 1: remove `scripts/count-clean-kernel-prs.ts` and its tests.
- Phase 2: clear the recorded PR evidence from this spec.
- Phase 3: re-add `continue-on-error: true` to the Rust kernel parity checks.
- Phase 4: revert docs from Phase B active back to advisory language.

## Review

Status: not_started
Verdict: none
Timestamp: none
Review rounds: none
Reviewer mode: none
Reviewer session: none
Round status: none
Override applied: none
Override reason: none
Override confirmed at: none
Reviewed head: none
Reviewed dirty: none
Reviewed diff: none

Findings:
- none

Passes:
- none

## Self Eval

Status: not_started
Completeness: none
Architecture fidelity: none
Spec alignment: none
Validation depth: none
Total: none
Second pass performed: none

## Deviations

- none

## Evidence

Clean PR evidence: <to be filled at exec time>

## Metadata

Estimated effort hours: 6
Actual effort hours: none
AI model: none
React cycles: none

Tags:
- rust
- trusted-kernel
- ci
- governance

## Origin

Source:
- Split from `rust-parity-ci-governance` after validating the installed scafld
  CLI and finding no supported command for reopening a completed spec.

Repo:
- runxhq/runx

Git:
- none

Sync:
- none

Supersession:
- follows: rust-kernel-port-orchestration
- follows: rust-parity-ci-governance

## Harden Rounds

- none

## Planning Log

- 2026-05-17T00:30:00Z: Drafted as the separate blocking-promotion spec. This
  keeps `rust-parity-ci-governance` executable as an advisory CI spec and
  avoids the invalid assumption that a completed scafld spec can be reopened
  with a `deviate` command.
