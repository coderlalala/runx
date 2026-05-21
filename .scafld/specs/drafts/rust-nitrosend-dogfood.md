---
spec_version: '2.0'
task_id: rust-nitrosend-dogfood
created: '2026-05-18T00:00:00Z'
updated: '2026-05-21T19:44:33+10:00'
status: draft
harden_status: not_run
size: medium
risk_level: medium
---

# Rust nitrosend dogfood

## Current State

Status: draft
Current phase: actual Nitrosend dogfood wrapper consumes the Rust contract;
external replay pending
Next: add Rust runtime replay only after target execution and observer runtime
gates are ready
Reason: refreshed against the current local OSS checkout and the actual
Nitrosend repo. OSS fixture and policy contract validation is still green, and
Nitrosend wrapper harness tests still replay through the Rust binary, but the
actual Nitrosend `config/runx-issue-flow.json` now contains a top-level
`post_merge` block that the current Rust operational-policy schema rejects.
Blockers: `runx-target-repo-runners` and
`runx-post-merge-closure-observer` are still draft for live external replay.
A sanitized external-shaped fixture contract exists and Nitrosend local wrapper
fixtures replay through the Rust binary, but no live target-runner/observer
external replay has been added. The actual Nitrosend policy/schema drift must
also be resolved before this dogfood spec can advance.
Allowed follow-up command: none during this refresh; do not run
`scafld harden rust-nitrosend-dogfood`.
Latest runner update: 2026-05-21 closed the Segment dogfood evidence gap:
`segment-from-prose` now rejects non-`runx.harness_receipt.v1` evidence and
returns the sealed `receipt_id` to callers. 2026-05-20 added Rust contract
request-admission coverage for the Nitrosend-like policy. The Rust API admits
`nitrosend/nitrosend`, `nitrosend/api`, and `nitrosend/app` through the
policy-backed source, target, runner, owner, dedupe, and closure/proof surface,
and denies unknown target repos and missing source-thread routing before
mutation.
Runtime skill fixtures are present; `fixtures/external/nitrosend/issue-intake`
now contains the sanitized `api-source-thread.json` fixture; the Nitrosend-like
policy fixture covers workspace, API, and app target routing; contract policy
validation passed; after refreshing ignored `dist` output with `pnpm build`,
CLI policy lint/inspect also pass.
Review gate: not_started

## Summary

Use Nitrosend as the external-shaped dogfood for the Rust runtime cutover, but
keep the plan honest about what exists in this OSS checkout today.

Current local facts:

- Generic runtime fixtures exist for the upstream `issue-intake` and
  `issue-to-pr` skills under `fixtures/runtime/skills/issue-intake/` and
  `fixtures/runtime/skills/issue-to-pr/`.
- A reusable operational policy contract exists as
  `runx.operational_policy.v1`, with CLI support in
  `packages/cli/src/commands/policy.ts` and schemas/fixtures under
  `schemas/operational-policy.schema.json` and `fixtures/operational-policy/`.
- `fixtures/operational-policy/nitrosend-like.json` is present, but it is only
  Nitrosend-like. It now includes `nitrosend/nitrosend`, `nitrosend/api`, and
  `nitrosend/app` so policy lint/inspect cover the real target set.
- `fixtures/external/nitrosend/issue-intake/api-source-thread.json` exists as
  the first sanitized external-shaped fixture contract. It cites the generic
  runtime fixtures, the Nitrosend-like policy, the target-runner planning and
  dedupe lookup contracts, and the post-merge harness receipt fixture.
- The current reusable follow-on work is not complete: target-repo runner
  live execution and the post-merge closure observer runtime are both draft
  specs.

The dogfood goal is preservation, not a new adopter flow. The real Nitrosend
workflow may keep using the human/product names `issue-intake` and
`issue-to-pr`, but cutover execution artifacts must be canonical harness,
decision, act, proof, and sealed `runx.harness_receipt.v1` objects only.

This spec must not invent live Nitrosend traffic capture. Initial validation is
fixture parity against checked-in, sanitized, deterministic external-shaped
fixtures. Production soak is a later confirmation step after fixture, policy,
target-runner, and post-merge observer gates are green.

## Context

CWD: `.` (runx OSS workspace)

Relevant existing local surfaces:

- `skills/issue-intake/SKILL.md`
- `skills/issue-intake/X.yaml`
- `skills/issue-to-pr/SKILL.md`
- `skills/issue-to-pr/X.yaml`
- `fixtures/runtime/skills/issue-intake/cases/*.yaml`
- `fixtures/runtime/skills/issue-to-pr/cases/*.yaml`
- `fixtures/operational-policy/nitrosend-like.json`
- `packages/contracts/src/schemas/operational-policy.ts`
- `schemas/operational-policy.schema.json`
- `packages/cli/src/commands/policy.ts`
- `.scafld/specs/drafts/runx-target-repo-runners.md`
- `.scafld/specs/drafts/runx-post-merge-closure-observer.md`

Missing local surfaces this plan must create or wait for:

- `fixtures/external/nitrosend/issue-intake/**`
- `crates/runx-runtime/tests/external/nitrosend_issue_intake.rs`
- Target PR creation through the reusable target-repo runner contract.
- Post-merge/provider observation through sealed closure/proof receipts.

## Invariants

- No compatibility reader or writer translates retired outcome/effect/report
  artifacts into harness receipts at runtime.
- Product copy, skill names, slash-command text, and source-thread comments may
  say `issue-intake` or `issue-to-pr`; persisted execution contract ids,
  fixture artifact kinds, outbox contract payload ids, and Rust structs must not
  use those names as legacy execution artifact identifiers.
- External fixtures are deterministic and sanitized. They are not captured live
  during CI, and they do not depend on private Nitrosend repositories being
  checked out.
- `runx.operational_policy.v1` is the policy boundary. Nitrosend-specific JSON
  can be an input to a one-time conversion, but core runtime behavior must not
  dual-read adopter policy formats.
- Missing source-thread metadata fails closed before any Slack/GitHub final
  publication.
- Target completion, failed verification, source issue closure, and final
  source-thread replies are modeled through contained act closure plus
  proof-bound verification on sealed harness receipts.

## Objectives

- Add an external-shaped Nitrosend fixture suite that layers the existing
  generic `issue-intake`/`issue-to-pr` runtime fixtures into a realistic
  multi-target policy scenario.
- Extend the Nitrosend-like operational policy fixture to include
  `nitrosend/api` and prove workspace, API, and app target routing.
- Validate `runx policy lint` and `runx policy inspect` against the
  Nitrosend-like policy without leaking raw provider locators.
- Coordinate with `runx-target-repo-runners` so target PR creation, dedupe, and
  source-thread metadata are reusable core behavior.
- Coordinate with `runx-post-merge-closure-observer` so merge/close/deploy
  observation, final publication, and source issue closure are sealed
  closure/proof receipts, not a Nitrosend-only observer path.
- Add Rust runtime replay coverage only after the external-shaped fixture and
  the reusable runner/observer contracts exist.

## Scope

In scope:

- Plan and fixture contract for Nitrosend-shaped issue intake.
- Policy fixture coverage for `nitrosend/api`.
- Runtime parity expectations for canonical harness receipts.
- Explicit dependency on target-repo runner and post-merge closure observer
  work.

Out of scope:

- Editing Nitrosend production repositories.
- Live external traffic capture.
- Scafld hardening in this refresh.
- Compatibility with retired peer terminal artifacts, including legacy
  outcome/effect packets and report-shaped verification payloads.

## Dependencies

- `runx-contract-spine-hard-cutover` for canonical harness, decision, act,
  proof, and receipt shapes.
- `rust-runtime-skeleton` and `rust-runtime-skill-execution`.
- `rust-receipt-proof-verification`, `rust-receipt-tree-resolution`, and
  `rust-runtime-receipt-path-discovery`.
- `runx-operational-policy-config` as the completed policy contract.
- `runx-target-repo-runners` before target dispatch parity can pass.
- `runx-post-merge-closure-observer` before final closure/proof parity can
  pass.

## Acceptance Criteria

- [ ] Existing generic `issue-intake` and `issue-to-pr` runtime fixtures remain
  green and are referenced by the external-shaped Nitrosend fixture plan.
- [x] `fixtures/external/nitrosend/issue-intake/**` exists and contains
  sanitized deterministic inputs, not live external capture.
- [x] The Nitrosend-like operational policy fixture covers
  `nitrosend/nitrosend`, `nitrosend/api`, and `nitrosend/app`.
- [x] `runx policy lint fixtures/operational-policy/nitrosend-like.json`
  accepts the policy, and `runx policy inspect` redacts raw provider locators.
- [ ] Target PR creation uses `runx-target-repo-runners`; dedupe is represented
  in pull-request outbox `metadata.dedupe` and in the sealed receipt proof path.
- [ ] Target completion and final publication use
  `runx-post-merge-closure-observer` only through closure/proof on sealed
  harness receipts.
- [ ] No fixture, schema id, persisted receipt, or replay expectation uses
  retired peer terminal artifacts, legacy outcome/effect packet fields, or
  report-shaped verification payloads; completion is represented as harness
  receipt closure with `proof.verification`.
- [ ] Replaying the same external-shaped fixture twice produces identical
  canonical receipt bytes after normalized fixture ids/timestamps and passes
  proof verification.

## Validation Commands

Current local discovery/guard commands:

```sh
find fixtures/runtime/skills -maxdepth 4 -type f | sort
test -f fixtures/external/nitrosend/issue-intake/api-source-thread.json
cargo test --manifest-path crates/Cargo.toml -p runx-contracts --test nitrosend_external_fixture -- --nocapture
./crates/target/debug/runx policy lint fixtures/operational-policy/nitrosend-like.json --json
./crates/target/debug/runx policy inspect fixtures/operational-policy/nitrosend-like.json --json
cargo test --manifest-path crates/Cargo.toml -p runx-contracts --test operational_policy -- --nocapture
cargo test --manifest-path crates/Cargo.toml -p runx-contracts
! rg -n "runx\\.issue_to_pr_outcome\\.v1|issue_to_pr_outcome|verification[_-]report|target[_-]?effect|\"effect\"\\s*:" fixtures/runtime fixtures/operational-policy fixtures/external skills crates/runx-runtime/src
scafld validate rust-nitrosend-dogfood --json
git diff --check -- .scafld/specs/drafts/rust-nitrosend-dogfood.md
```

2026-05-21 current refresh results:

- `find fixtures/runtime/skills -maxdepth 4 -type f | sort` passed and still
  lists the generic `issue-intake` and `issue-to-pr` runtime fixture cases.
- `test -f fixtures/external/nitrosend/issue-intake/api-source-thread.json`
  passed.
- `cargo test --manifest-path crates/Cargo.toml -p runx-contracts --test nitrosend_external_fixture -- --nocapture`
  passed: 2 tests.
- The documented `pnpm --filter @runxhq/cli run runx policy ...` commands are
  stale in the current checkout because `@runxhq/cli` exposes a `runx` binary
  but no `runx` npm script. The current safe invocation is the Rust binary
  directly.
- `./crates/target/debug/runx policy lint fixtures/operational-policy/nitrosend-like.json --json`
  passed with `status: "success"` and no findings.
- `./crates/target/debug/runx policy inspect fixtures/operational-policy/nitrosend-like.json --json`
  passed with `status: "success"` and no findings; source locators are redacted
  to counts.
- `cargo test --manifest-path crates/Cargo.toml -p runx-contracts --test operational_policy -- --nocapture`
  passed: 9 tests.
- `cargo test --manifest-path crates/Cargo.toml -p runx-contracts` passed. This
  includes contract-level target-runner and post-merge observer tests, but this
  is not live external replay evidence and does not clear the draft blockers.
- The original retired-artifact guard over `crates/runx-runtime` now false
  positives on `crates/runx-runtime/tests/external/aster_agent_step.rs`, where
  the retired names are rejection-test strings. The narrowed guard over
  `crates/runx-runtime/src` plus fixtures and skills passed.
- `scafld validate rust-nitrosend-dogfood --json` passed.
- In Nitrosend, `/Users/kam/dev/runx/runx/oss/crates/target/debug/runx policy lint config/runx-issue-flow.json --json`
  failed because `config/runx-issue-flow.json` contains unknown top-level field
  `post_merge`; the current Rust schema expects `outcomes` for that surface.
- In Nitrosend, `/Users/kam/dev/runx/runx/oss/crates/target/debug/runx policy inspect config/runx-issue-flow.json --json`
  failed for the same `post_merge` schema mismatch.
- In Nitrosend,
  `RUNX_BIN=/Users/kam/dev/runx/runx/oss/crates/target/debug/runx node --test scripts/onboarding.test.mjs scripts/segment-from-prose.test.mjs scripts/issue-intake.test.mjs scripts/github-issue-thread.test.mjs scripts/post-issue-intake-comments.test.mjs scripts/runx-target-outcome.test.mjs scripts/scafld-command-review.test.mjs scripts/runx-harness.test.mjs`
  passed: 122 tests, 0 skipped.
- In Nitrosend, the retired-artifact and fallback-knob guard passed:
  `! rg -n "needs_resolution|runx\\.issue_to_pr_outcome\\.v1|issue_to_pr_outcome|verification[_-]report|target[_-]?effect|\"effect\"\\s*:|RUNX_JS_BIN|RUNX_NPM_PACKAGE|target_repositories|allowed_repositories|route_hints" scripts config fixtures/runx .github/workflows/issue-intake.yml .github/workflows/wrapper-ci.yml`.
- `git diff --check` passed in Nitrosend before this spec update.

2026-05-20 local refresh results:

- `find fixtures/runtime/skills -maxdepth 4 -type f | sort` passed and listed
  the existing generic `issue-intake` and `issue-to-pr` runtime fixtures.
- `test -f fixtures/external/nitrosend/issue-intake/api-source-thread.json`
  passed; a sanitized external-shaped Nitrosend fixture contract now exists.
- `cargo test --manifest-path crates/Cargo.toml -p runx-contracts --test nitrosend_external_fixture -- --nocapture`
  passed: 2 tests, deriving a target runner plan and provider dedupe lookup
  from the fixture and validating the cited post-merge harness receipt.
- A direct contract-level policy check against
  `fixtures/operational-policy/nitrosend-like.json` passed with no findings and
  projected a readback that redacts raw Slack/Sentry source locators.
- `pnpm vitest run packages/contracts/src/schemas/operational-policy.test.ts`
  passed: 23 tests.
- Static target coverage check passed for `nitrosend/nitrosend`,
  `nitrosend/api`, and `nitrosend/app` across targets, runner target repos, and
  owner routes.
- The retired-artifact guard passed:
  `! rg -n "runx\\.issue_to_pr_outcome\\.v1|issue_to_pr_outcome|verification[_-]report|target[_-]?effect|\"effect\"\\s*:" fixtures/runtime fixtures/operational-policy skills crates/runx-runtime`.
- `git diff --check -- .scafld/specs/drafts/rust-nitrosend-dogfood.md fixtures/operational-policy/nitrosend-like.json packages/contracts/src/schemas/operational-policy.test.ts packages/cli/src/index.test.ts packages/cli/src/commands/policy.ts`
  passed before this evidence note was added.
- `pnpm build` passed and refreshed ignored workspace `dist` output after the
  receipts sunset removed `@runxhq/core/receipts`.
- `pnpm --filter @runxhq/cli run runx policy lint fixtures/operational-policy/nitrosend-like.json --json`
  passed with `status: "success"` and no findings.
- `pnpm --filter @runxhq/cli run runx policy inspect fixtures/operational-policy/nitrosend-like.json --json`
  passed with `status: "success"` and no findings.
- `cargo test --manifest-path crates/Cargo.toml -p runx-contracts --test operational_policy -- --nocapture`
  passed: 9 tests, including Rust request-admission coverage for all three
  Nitrosend-like target repos plus unknown-target and missing-source-thread
  fail-closed cases.
- `cargo test --manifest-path crates/Cargo.toml -p runx-contracts` passed.

2026-05-20 actual Nitrosend dogfood integration verification:

- Nitrosend `config/runx-issue-flow.json` now uses
  `runx.operational_policy.v1` directly for `nitrosend/nitrosend`,
  `nitrosend/api`, and `nitrosend/app`.
- Nitrosend issue-intake derives target workspaces from the operational policy
  `targets` list instead of the retired repo-local target-repository shape.
- Nitrosend harness fixtures now expect sealed `runx.harness_receipt.v1`
  receipts from the Rust binary.
- The actual Rust binary replayed the Nitrosend onboarding, segment, and
  issue-intake harness fixtures through `RUNX_BIN=.../crates/target/debug/runx`
  with no TypeScript fallback knobs.
- The generic runx `issue-intake` runtime fixtures were refreshed to the
  current Rust receipt closure reason (`process_closed`) and replay green.

Validation:

- `/Users/kam/dev/runx/runx/oss/crates/target/debug/runx policy lint config/runx-issue-flow.json --json`
  passed in the Nitrosend repo with no findings.
- `/Users/kam/dev/runx/runx/oss/crates/target/debug/runx policy inspect config/runx-issue-flow.json --json`
  passed in the Nitrosend repo and redacted raw locators via counts.
- `RUNX_BIN=/Users/kam/dev/runx/runx/oss/crates/target/debug/runx node --test scripts/onboarding.test.mjs scripts/segment-from-prose.test.mjs scripts/issue-intake.test.mjs scripts/github-issue-thread.test.mjs scripts/post-issue-intake-comments.test.mjs scripts/runx-target-outcome.test.mjs scripts/scafld-command-review.test.mjs scripts/runx-harness.test.mjs`
  passed in the Nitrosend repo: 128 tests, 0 skipped.
- `cargo test --manifest-path crates/Cargo.toml -p runx-contracts` passed in
  runx OSS.
- `! rg -n "needs_resolution|runx\\.issue_to_pr_outcome\\.v1|issue_to_pr_outcome|verification[_-]report|target[_-]?effect|\"effect\"\\s*:|RUNX_JS_BIN|RUNX_NPM_PACKAGE|target_repositories|allowed_repositories|route_hints" scripts config fixtures/runx .github/workflows/issue-intake.yml .github/workflows/wrapper-ci.yml`
  passed in the Nitrosend repo.
- `! rg -n "runx\\.issue_to_pr_outcome\\.v1|issue_to_pr_outcome|verification[_-]report|target[_-]?effect|\"effect\"\\s*:" fixtures/runtime fixtures/operational-policy fixtures/external skills crates/runx-runtime`
  passed in runx OSS.
- `git diff --check` passed in Nitrosend; `git diff --check -- fixtures/runtime/skills/issue-intake/cases .scafld/specs/drafts/rust-nitrosend-dogfood.md`
  passed in runx OSS.

Remaining blocker:

- Live target PR creation and final post-merge/provider observation still depend
  on the reusable `runx-target-repo-runners` and
  `runx-post-merge-closure-observer` work. No external replay was added here.

2026-05-21 Segment dogfood evidence verification:

- `scripts/segment-from-prose.mjs` now validates the returned sealed Runx
  result contains an embedded `runx.harness_receipt.v1` receipt and returns
  `receipt_id` to success and rejection callers.
- `scripts/segment-from-prose.test.mjs` now asserts the surfaced `receipt_id`
  and rejects non-harness receipt evidence.
- `RUNX_BIN=/Users/kam/dev/runx/runx/oss/crates/target/debug/runx node --test scripts/segment-from-prose.test.mjs scripts/runx-harness.test.mjs`
  passed in the Nitrosend repo: 12 tests, 0 skipped.

2026-05-20 narrow policy-fixture slice verification:

- The current safe slice is limited to verifying the Nitrosend-like policy and
  external dogfood fixture cover `nitrosend/api` alongside
  `nitrosend/nitrosend` and `nitrosend/app`.
- Static fixture coverage passed for all three repos across policy targets,
  runner `target_repos`, and owner-route `target_repos`.
- Focused Rust contract tests passed for operational policy admission,
  Nitrosend external fixture derivation, and target-runner planning/dedupe.
- TypeScript operational-policy fixture tests passed.
- CLI policy lint/inspect passed with no findings and redacted source
  locators.
- `scafld validate rust-nitrosend-dogfood --json` passed. `scafld build` and
  `scafld complete` were not run because this parent dogfood spec remains a
  draft with runtime replay acceptance still intentionally pending.

Future validation once the missing external fixture and runner/observer specs
land:

```sh
cargo test --manifest-path crates/Cargo.toml -p runx-runtime nitrosend_issue_intake
cargo test --manifest-path crates/Cargo.toml -p runx-receipts
```

## Rollback And Repair

- Before any Nitrosend launcher flip, rollback is to keep the existing
  production path unchanged and disable Rust side-by-side validation.
- If the Nitrosend-like policy fixture later omits a real target such as
  `nitrosend/api`, repair the policy fixture and policy admission tests rather
  than adding special-case runtime routing.
- If an external-shaped fixture is accidentally generated from live traffic,
  replace it with a sanitized deterministic fixture before merging.
- If retired artifact fields appear in fixtures or replay output, repair the
  producer and expected receipts. Do not whitelist retired fields.

## Open Questions

- Exact Rust binary pinning mechanism for Nitrosend CI.
- Whether Nitrosend wrapper behavior remains repo-local or is fully absorbed by
  reusable runx target-runner and post-merge observer code.
- Soak duration after fixture parity is green.
