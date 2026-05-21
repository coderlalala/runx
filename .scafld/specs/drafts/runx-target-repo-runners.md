---
spec_version: '2.0'
task_id: runx-target-repo-runners
created: '2026-05-19T02:08:02Z'
updated: '2026-05-21T09:45:53Z'
status: draft
harden_status: in_progress
size: large
risk_level: high
---

# Runx target repo runners

## Current State

Status: draft
Current phase: none
Next: resolve live target execution blockers before marking harden passed
Reason: hardening round in progress; the Rust target-runner path is
fixture-executable for policy admission, same-repo/cross-repo planning,
readiness gating, provider dedupe observations, PR create/reuse receipt
metadata, and source-publication receipt metadata, but it is not live-target
executable.
Blockers: target checkout/git mutation, pull-request create/update, outbox
pushers for source issue/thread publication, and Aster scheduling/readback are
not implemented in this target-runner path. Live GitHub provider API lookup now
has a transport-backed search client, but live end-to-end target mutation is
still blocked.
Allowed follow-up command: `scafld harden runx-target-repo-runners --mark-passed`
only after the live execution blockers are resolved or explicitly descoped.
Latest runner update: 2026-05-21 added a transport-backed GitHub Search API
client for provider dedupe lookup. It sends the concrete open-PR search command,
redacts authorization headers in request debug output, projects only GitHub pull
request search items for the target repo, and only marks markers/source refs
present when they are echoed in returned PR text so reuse remains fail-closed.
Remaining target-runner work is checkout/git mutation, PR create/update,
source-publication pushers, and Aster scheduling/readback rather than contract
drift.
Review gate: not_started

## Summary

Add first-class target-repo runner support to runx issue-to-PR flows. A source
issue or Slack/Sentry intake can select a target repo, run the right governed
runner in that repo, create or update the PR there, and publish all milestone
links back to the original source issue/thread.

This preserves the security posture: automation can produce a PR, but human
review remains the merge gate unless a future policy explicitly enables a
separate merge workflow.

## Context

CWD: `.` (runx OSS workspace)

Production shape to generalize:
- Source thread: Slack/GitHub/Sentry-originated issue intake.
- Source issue: a GitHub issue used as durable runx source thread.
- Target repo: the actual codebase receiving the PR.
- Runner: a governed scafld/runx worker inside the target repo, executed as a
  child harness with attenuated authority.
- Follow-up publishing: source Slack thread and source GitHub issue receive the
  issue link, triage result, PR link, review gate, and closure/proof
  projection.

Candidate touchpoints:
- `skills/issue-intake/**`
- `skills/issue-to-pr/**`
- `skills/work-plan/**`
- `packages/cli/**`
- `packages/cli/tools/outbox/**`
- `crates/runx-runtime/**`
- Aster runner scheduling/readback

Invariants:
- Target repo selection is policy-driven and fail-closed.
- Runner availability is explicit; no hidden fallback to the source repo.
- A target repo must be scafld-ready before mutating issue-to-PR work runs.
- Target execution runs inside a governed harness; the PR-producing act uses
  `form: "revision"` and is proved only by the sealing harness receipt.
- Dedupe runs before creating a new PR.
- PR packaging records `metadata.dedupe.strategy`, `metadata.dedupe.key`, and
  `metadata.dedupe.result` so retries are auditable and provider pushers can
  reuse an existing PR path.
- Source issue/thread ids are carried through every milestone act receipt.
- Public comments use repo names and URLs, not operator-local checkout paths.

## Objectives

- Define target repo runner model and source-to-target harness context.
- Add policy-backed target repo selection and runner availability checks.
- Add PR dedupe before creating a new branch/PR.
- Preserve source issue/thread metadata through runner execution and outbox
  publishing.
- Add fixtures for same-repo, cross-repo, no-runner, not-allowed, not-scafld,
  and duplicate-PR cases.

## Scope

In scope:
- Core source-to-target harness runner contract for product issue-to-pr flows.
- Policy-backed repository and runner selection.
- Dedupe query and reuse behavior.
- Source-thread/source-issue carry-through.
- Tests and fixtures.

Out of scope:
- Post-merge deployment observation and source closure publication; owned by
  `runx-post-merge-closure-observer`, which now seals closure/proof as harness
  receipts rather than a peer terminal packet.
- Slack-specific event listener implementation.
- Auto-merge behavior.
- Nitrosend-only wrapper script details except as fixture input.

## Dependencies

- `runx-operational-policy-config`.
- `rust-runtime-skeleton` for Rust runtime execution path, or current TS runner
  during staged migration.
- `rust-runtime-skill-execution` for Rust skill execution parity.
- `rust-nitrosend-dogfood` consumes this as the production proof point.

## Assumptions

- GitHub is the first provider target; the contract should not preclude other
  repository providers later.
- Target checkouts may be local worktrees, CI-provided paths, or Aster-managed
  sandboxes, but the runner contract must hide those details from public output.

## Touchpoints

- Issue intake product payload.
- Source-to-target harness runner context.
- GitHub adapter/outbox act/receipt builders.
- Policy config loader.
- Dedupe provider query.
- Aster runner scheduling.

## Risks

- Running in the wrong repo is a high-impact failure.
- Dedupe bugs can spam reviewers with duplicate PRs.
- Losing source-thread metadata recreates noisy Slack root-channel posts.
- Hidden fallback behavior can bypass policy.

## Acceptance

Profile: strict

Validation:
- `pnpm test`
- `cargo test --manifest-path crates/Cargo.toml`
- target-runner fixture command
- `git diff --check`

Required behavior:
- [x] Same-repo issue-to-PR still works through the target runner contract.
- [ ] Cross-repo issue-to-PR creates the PR in the configured target repo.
- [x] Unknown target repo fails before checkout or mutation.
- [x] Missing runner fails before checkout or mutation.
- [x] Target repo without scafld readiness fails before mutation.
- [x] Provider lookup planning for the dedupe key is deterministic and carries
  source issue/thread references before mutation.
- [x] Existing open PR for the dedupe key is reused and linked instead of
  creating another PR through provider lookup execution.
- [x] Runtime fixture boundary chooses create versus reuse from provider dedupe
  observations without live network/git calls.
- [x] Local pull-request receipt metadata records whether the PR path was
  created or reused for the dedupe key using canonical `metadata.dedupe.result`
  values.
- [ ] Pull-request outbox metadata and the sealed pull-request harness receipt
  record whether the PR path was created or reused for the dedupe key. Local
  contract receipt metadata is present; live outbox/sealed-harness integration
  remains.
- [ ] Source GitHub issue receives the target PR link.
- [ ] Source Slack thread metadata survives through all outbox receipt nodes.
- [x] Runtime source-publication adapter requests include source issue and
  source-thread commands with the target PR link, and readback is sealed as a
  reply receipt carrying dedupe metadata.
- [x] Runtime live adapter receives fail-closed typed checkout and GitHub PR
  dedupe search commands before provider readback, with no local path leakage.
- [x] Public output excludes local checkout paths and env-secret values.

## Phase 1: Contract

Status: completed
Dependencies: `runx-operational-policy-config`

Objective: Define the target runner context and fail-closed selection rules.

Changes:
- Add source/target context types.
- Add policy-backed target selection.
- Add runner availability checks and a mutation-free provider dedupe lookup
  plan. Scafld readiness probing is still pending runtime work.

Acceptance:
- [x] Fixtures cover allowed and denied target planning.
- [x] Provider dedupe lookup carries target-scoped keys and source-thread refs.
- [x] Missing runner and not-scafld target fixtures deny before a runner plan
  materializes.

## Phase 2: Runner Execution

Status: in_progress
Dependencies: Phase 1

Objective: Execute product issue-to-pr work through the selected target harness.

Changes:
- Thread target repo context into runner invocation as role-named References.
- Carry source issue/thread metadata through execution.
- Ensure public output uses URLs and repo names.
- Seal target runner execution as a harness receipt containing a `revision`
  act for branch/PR creation or reuse.

Acceptance:
- [x] Contract execution plan denies non-scafld-ready targets before checkout
  mutation and exposes only public repo/source references.
- [x] Runtime fixture execution rechecks the execution plan against readiness
  observations before PR mutation and fails closed on stale/not-ready readiness.
- [x] Cross-repo fixture produces target PR receipt and source issue/thread
  reply receipt.
- [x] Live-adapter fixture publishes source issue/thread commands after the
  sealed PR receipt and fails closed when source issue readback is missing.
- [x] Live-adapter checkout receives a typed command that carries only public
  repo identity, runner identity, base branch, scafld requirements, and mutation
  admission, and rejects malformed target repo ids before adapter calls.

## Phase 3: Dedupe

Status: in_progress
Dependencies: Phase 2

Objective: Avoid duplicate PRs for the same issue/fix.

Changes:
- Add dedupe key generation and provider lookup.
- Reuse/link existing PR when policy says so.
- Preserve the dedupe decision in pull-request outbox metadata and in the
  sealed harness receipt proof path.

Acceptance:
- [x] Provider lookup execution reuses an open PR only when dedupe markers and
  source issue/thread refs match.
- [x] PR receipt metadata records `metadata.dedupe.strategy`,
  `metadata.dedupe.key`, canonical `metadata.dedupe.result` (`created` or
  `reused`), disposition, target PR URL, and source-thread URI.
- [x] Runtime fixture execution chooses create when provider lookup has no
  matching open PR and reuse when provider lookup returns a matching PR.
- [x] Duplicate fixture reuses the existing PR and produces no new branch.
- [x] Runtime provider lookup emits a concrete GitHub open-PR search command
  containing repo qualifiers, dedupe markers, source issue/thread refs, and a
  bounded result limit before projecting deterministic readback.
- [x] Runtime GitHub provider lookup can execute the open-PR search command
  through an HTTP transport and project readback into the dedupe observation
  without leaking authorization values.

## Rollback

- Keep current same-repo path available only until target runner parity is green.
  Remove duplicate old path during cutover; do not introduce legacy aliases or
  compatibility shims.

## Review

Status: not_started
Verdict: none

Findings:
- none

## Self Eval

- Target score: 9.5. Passing means cross-repo issue-to-PR is a core runx
  capability with explicit policy and reviewable evidence.

## Deviations

- none

## Metadata

- created_by: scafld
- planning_reason: move reusable target repo execution out of adopter scripts

## Origin

Created by: scafld
Source: plan

## Harden Rounds

### round-1

Status: in_progress
Started: 2026-05-20T10:26:59Z
Ended: none

Checks:
- `cargo test --manifest-path crates/Cargo.toml -p runx-contracts --test target_runner`
- `cargo test --manifest-path crates/Cargo.toml -p runx-runtime --test target_runner`

Issues:
- none


## Planning Log

- 2026-05-19: Expanded placeholder into target-repo runner contract after
  Nitrosend source-thread dogfood review.
- 2026-05-19: Locked PR dedupe to outbox metadata plus the sealed pull-request
  harness receipt node so the Rust runner and provider pushers keep retry
  behavior observable.
- 2026-05-20: Added local missing-runner and not-scafld negative fixture
  coverage for fail-closed target-runner admission before mutation.
- 2026-05-20: Added Rust request-admission coverage for the Nitrosend-like
  workspace/API/app targets. Remaining target-runner work is still execution
  and provider mutation, not policy admission.
- 2026-05-20: Added Rust target-runner planning and provider dedupe lookup
  contracts. Remaining target-runner work is now target checkout/readiness,
  provider lookup execution, PR create/update, source-thread publication, Aster
  scheduling, and receipt/outbox integration.
- 2026-05-20: Added Rust target checkout/readiness execution contract,
  provider dedupe lookup execution, PR create/reuse receipt metadata, and
  source publication receipt metadata. Remaining work is live provider/git
  mutation, outbox pushers, Aster scheduling/readback, and fixture replay.
- 2026-05-20: Added explicit same-repo target-runner contract coverage using
  the minimal single-repo policy fixture. Remaining work is not contract drift;
  it is live provider/git mutation, source publication outbox integration, and
  Aster scheduling/readback.
- 2026-05-21: Added runtime source-publication command generation, adapter
  readback validation, sealed reply receipt projection, and negative readback
  coverage for missing source issue publication. Remaining work is the concrete
  provider/git/outbox/Aster adapters.
- 2026-05-21: Added typed runtime checkout commands plus concrete GitHub
  provider dedupe search commands and readback projection. Remaining work is
  executing those commands through provider API transport and real target git/PR
  mutation, plus source-publication pushers and Aster scheduling/readback.
- 2026-05-21: Added a transport-backed GitHub provider lookup client for the
  target-runner open-PR dedupe search command, with readback projection and
  HTTP error coverage. Remaining work is real target checkout/git mutation, PR
  create/update, source-publication pushers, and Aster scheduling/readback.
