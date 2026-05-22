---
spec_version: '2.0'
task_id: oss-local-credential-provision-v1
created: '2026-05-22T11:05:00+10:00'
updated: '2026-05-22T02:25:01Z'
status: approved
harden_status: needs_revision
size: medium
risk_level: high
---

# Restore local credential provision in the OSS runtime

## Current State

Status: approved
Current phase: planning
Next: build
Reason: hardening found draft contract issues
Blockers: check needs revision: path audit; check needs revision: scope/migration audit; check needs revision: rollback/repair audit; check needs revision: design challenge; 6 approval-blocking issue(s)
Allowed follow-up command: `scafld build oss-local-credential-provision-v1`
Latest runner update: 2026-05-22T11:05:00+10:00 drafted from an over-cut
Review gate: not_started

## Summary

The license-boundary refactor cut one notch too deep. Removing the hosted OAuth
brokerage from OSS was right; removing the local credential *establishment* path
was not. Today the MIT CLI cannot supply a credential to a skill: `runx connect`
refuses and points at the private distribution, the runtime's scoped env is a
non-secret allowlist, the CLI run path never constructs a `MaterialResolver`,
and there is no token-intake verb. So the open runtime cannot do authenticated
work standalone, which breaks the doctrine's offline/zero-dependency promise and
north-star's "BYO credential delivery unlocks the portfolio" order, for no moat
gain (the brokerage secrets were always in `cloud/packages/auth`).

This spec restores a local, no-network credential-provision path: a one-shot,
per-run structured credential descriptor that the runtime turns into a
`CredentialDelivery` through the existing opaque `MaterialResolver`, with the
secret redacted. v1 persists no secret state and does not change the
credential-delivery contract schema, so labeling the receipt `grant_type: local`
is deferred to a coordinated contract migration. It does not reopen OAuth
brokerage, Nango, hosted connect, or secret custody.

## Context

- `crates/runx-cli/src/main.rs:44` returns "runx connect is not available in the
  MIT OSS CLI; use the hosted/private CLI distribution".
- `crates/runx-runtime/src/execution/runner.rs:48` defines `safe_default_env()`,
  a strict allowlist (`PATH`, `SystemRoot`, `PATHEXT`, `RUNX_RECEIPT_DIR`,
  `RUNX_PROJECT_DIR`, `RUNX_CWD`) with no secret passthrough.
- `crates/runx-runtime/src/credentials.rs:106` defines `MaterialResolver` and
  `InMemoryMaterialResolver`, populated only programmatically; the CLI run path
  does not construct one (`rg MaterialResolver crates/runx-cli/src` is empty).
- The doctrine ("runs stay local, zero-dependency") and `plans/runx.md` "Offline
  mode: `runx connect --token`, no browser, `grant_type: local`" both assume a
  local establishment path that no longer exists in OSS.
- `connect-auth-mit-boundary-v1` (archived) banned `NangoConnection`, `oauth_*`,
  `RUNX_CONNECT_*` and kept the opaque `MaterialResolver`. That stays.

## Objectives

- Add a one-shot, per-run local credential-provision surface to the OSS CLI: a
  structured descriptor (`provider`, `auth_mode`, `env_var`,
  `material_ref`/`grant_id`, `scopes`, and the secret value) supplied at run
  invocation (for example `runx run ... --secret-env <ENV=...>` /
  `--credential <descriptor>`). No persisted secret state, no local config file.
- Carry the descriptor on `SkillRunRequest`; the runtime, not the CLI, derives the
  `CredentialEnvelope`, `CredentialDeliveryProfile`, and a local allow decision for
  that descriptor and constructs `CredentialDelivery`, keeping policy and redaction
  centralized.
- Deliver the secret to the adapter through the existing `CredentialDelivery`
  channel, redacted across receipts, output, and metadata.
- Defer `grant_type: local` receipt labeling: v1 records local provision through
  the existing `CredentialDeliveryObservation` metadata only and adds no contract
  schema field (that is a coordinated migration, see Scope).
- Keep the boundary intact: no OAuth brokerage, Nango, hosted calls, or custody.
  Add only the local-provision identifiers to the boundary manifest allowlist;
  reintroduce none of the banned brokerage identifiers.

## Scope

In scope:
- `crates/runx-cli/src` (the per-run `--secret-env`/`--credential` provision flags;
  forward the descriptor to the runtime).
- `crates/runx-runtime/src` (carry the descriptor on `SkillRunRequest`; derive the
  envelope/profile/allow-decision and construct `CredentialDelivery` in the run
  path; reuse the existing `MaterialResolver` and redaction).
- `docs/license-boundary.manifest.json` allowlist update.
- Tests: an offline run that consumes a per-run provided credential; redaction
  across receipts/output/metadata; a no-network assertion (sibling to `locality.rs`).

Out of scope:
- OAuth brokerage, hosted connect, Nango, or secret custody (stay private).
- Any credential-delivery contract schema change, including a `grant_type` field
  (deferred; coordinate with `credential-envelope-opaque-reference-v1`).
- Persistent local secret storage of any kind (v1 is per-run, in-memory only).
- Browser loopback/PKCE establishment (a possible later add, not v1).
- Any cloud change.

## Acceptance

- [ ] `dod1` The OSS CLI can provide a credential for a single run via the
  structured per-run descriptor, with no network, no hosted dependency, and no
  secret persisted to disk.
- [ ] `dod2` A skill consuming that credential runs, and the secret is redacted
  from receipts, captured output, and metadata via `CredentialDelivery`.
- [ ] `dod3` The license-boundary guard passes; only local-provision identifiers
  are added to the allowlist and no banned brokerage identifier is reintroduced.
- [ ] `dod4` A no-network test proves the provision + run path makes no outbound
  calls.
- [ ] `dod5` No credential-delivery contract schema field is added; local
  provision is observable only through existing non-secret observation metadata.

Validation:
- [ ] `v1` Scafld validates this spec.
  - Command: `scafld validate oss-local-credential-provision-v1 --json`
  - Expected kind: `exit_code_zero`
  - Status: pending
- [ ] `v2` Offline credential-provision run + redaction tests pass.
  - Command: `cargo test --manifest-path crates/Cargo.toml -p runx-cli -p runx-runtime local_credential`
  - Expected kind: `exit_code_zero`
  - Status: pending
- [ ] `v3` The license-boundary guard passes on the changed tree.
  - Command: `cargo test --manifest-path crates/Cargo.toml -p runx-runtime --test license_boundary`
  - Expected kind: `exit_code_zero`
  - Status: pending
- [ ] `v4` The CLI locality guard still passes.
  - Command: `cargo test --manifest-path crates/Cargo.toml -p runx-cli --test locality`
  - Expected kind: `exit_code_zero`
  - Status: pending

## Phase 1: Provision Surface And Semantics

Pin the per-run descriptor contract (exact flags, descriptor fields, precedence,
validation failures) and the runtime derivation rules (descriptor to
envelope/profile/allow-decision), plus the redaction guarantees. No source
remediation, no persisted state.

## Phase 2: Implementation

Implement the per-run CLI flags, carry the descriptor on `SkillRunRequest`, derive
the envelope/profile/allow-decision and construct `CredentialDelivery` in the
runtime run path, and apply redaction through the existing channel. No persisted
secret state.

## Phase 3: Boundary And Tests

Add the local-provision identifiers to the manifest allowlist, add the offline
run + redaction + no-network tests, and confirm the boundary guard and locality
guard both pass with no brokerage reintroduced.

## Rollback

Revert the CLI surface and run-path wiring; remove the allowlist additions. The
offline path returns to its current absent state with no secret material
persisted. The boundary guard must still pass after rollback.

## Origin

Conversation on 2026-05-22: grounded code review showed the MIT CLI cannot
provide a credential at all after the boundary refactor, contradicting the
project's own doctrine and north-star order. The fix is additive (restore local
establishment), not an unwind of the boundary.

## Harden Rounds

### round-1

Status: needs_revision
Started: 2026-05-22T02:10:29Z
Ended: 2026-05-22T02:10:29Z
Verdict: needs_revision
Provider: codex
Output format: codex.output_file
Summary: Verdict: needs revision. The direction is architecturally right, but approval is unsafe until the spec pins the v1 CLI surface, the local descriptor-to-delivery derivation, the CLI/runtime ownership boundary, and the sealed location for `grant_type: local`. Also fix the manifest path and make rollback match the persistence choice.

Checks:
- path audit
  - Grounded in: rg --files from /Users/kam/dev/runx/runx/oss; docs/license-boundary.manifest.json:1
  - Result: failed
  - Evidence: Declared paths exist for `crates/runx-cli/src`, `crates/runx-runtime/src`, and `crates/runx-cli/tests/locality.rs`; the actual manifest is `docs/license-boundary.manifest.json`, while the draft scope says `oss/docs/license-boundary.manifest.json`.
- command audit
  - Grounded in: command output
  - Result: passed
  - Evidence: `command -v scafld` returned `/opt/homebrew/bin/scafld`; `scafld status oss-local-credential-provision-v1 --json` returned ok/status draft/gate harden; `scafld validate oss-local-credential-provision-v1 --json` returned valid=true. `./bin/scafld` is absent, but the draft validation commands use `scafld`, not `./bin/scafld`.
- scope/migration audit
  - Grounded in: code:crates/runx-contracts/src/credential_delivery.rs:118; code:crates/runx-runtime/src/credentials.rs:222
  - Result: failed
  - Evidence: `CredentialDeliveryObservation` has no `grant_type` field, and the draft does not include `crates/runx-contracts` or `schemas/credential-delivery-*.json` in scope. `CredentialDelivery::from_allowed_binding` requires a `CredentialBindingDecision`, `CredentialEnvelope`, `CredentialDeliveryProfile`, and `MaterialResolver`, but the draft does not specify how local provision creates or derives the non-secret envelope/profile/binding inputs.
- acceptance timing audit
  - Grounded in: command output; code:crates/runx-cli/Cargo.toml:20; code:crates/runx-runtime/tests/credential_delivery.rs:1
  - Result: passed
  - Evidence: `scafld validate ... --json` passes. The cargo acceptance command could not be executed in this read-only sandbox because Cargo failed to open `crates/target/debug/.cargo-lock`; source review shows `runx-cli` enables runtime `cli-tool` and `mcp` features, so the planned package combination can compile feature-gated credential tests once a writable target dir is available.
- rollback/repair audit
  - Grounded in: spec_gap:Rollback
  - Result: failed
  - Evidence: Rollback says no secret material is persisted, but the objective explicitly leaves local config file as an acceptable implementation option. If the build chooses a persistent file, rollback must include deletion/repair of that local material or the no-persistence statement becomes false.
- design challenge
  - Grounded in: code:crates/runx-runtime/src/adapters/cli_tool.rs:58; code:crates/runx-runtime/src/adapters/external_adapter.rs:787; code:crates/runx-runtime/src/execution/skill_run.rs:192; code:crates/runx-runtime/src/execution/runner/steps.rs:74
  - Result: failed
  - Evidence: Existing adapter delivery/redaction is the right seam: `CliToolAdapter` injects `credential_delivery.secret_env()` after `env_clear()` and redacts captured output; external adapter process env does the same. However, the native skill and graph execution paths currently set `CredentialDelivery::none()`, so the spec must define exactly where local provision is converted into delivery.

Issues:
- [medium/blocks approval] `H1` path audit - Boundary manifest path is wrong relative to the checkout.
  - Status: open
  - Grounded in: rg --files from /Users/kam/dev/runx/runx/oss
  - Evidence: The draft scope says `oss/docs/license-boundary.manifest.json`, but from the task checkout `/Users/kam/dev/runx/runx/oss` the existing file is `docs/license-boundary.manifest.json`.
  - Recommendation: Correct the spec scope to `docs/license-boundary.manifest.json` so execution does not target a nonexistent nested `oss/docs` path.
  - Question: Which boundary manifest path is authoritative for this task?
  - Recommended answer: Use `docs/license-boundary.manifest.json` relative to the OSS repo root.
  - If unanswered: Default to `docs/license-boundary.manifest.json` relative to `/Users/kam/dev/runx/runx/oss`.
- [high/blocks approval] `H2` scope/migration audit - `grant_type: local` is required but has no specified sealed schema location.
  - Status: open
  - Grounded in: code:crates/runx-contracts/src/credential_delivery.rs:118
  - Evidence: `CredentialDeliveryObservation` contains schema/status/refs/profile/provider/purpose/delivery_mode/material_ref_hash/roles/redaction refs/observed_at, but no `grant_type` field. Receipt sealing builds acts/seals from `SkillOutput`, and the draft does not name where `grant_type: local` belongs.
  - Recommendation: Define the wire/receipt location. If it requires schema changes, add `crates/runx-contracts`, schema JSON, and fixture updates to scope and call out the public API change.
  - Question: Where exactly should `grant_type: local` be sealed, and is changing the credential-delivery contract schema in scope?
  - Recommended answer: Seal it through an explicit credential-delivery observation/schema update and expand scope to `crates/runx-contracts` plus `schemas/credential-delivery-*.json` fixtures.
  - If unanswered: Default to adding no schema field until a public contract migration is explicitly approved; record local grant type only in a named existing non-secret metadata field if that is accepted.
- [high/blocks approval] `H3` scope/migration audit - The spec does not define the non-secret credential metadata required by the existing delivery contract.
  - Status: open
  - Grounded in: code:crates/runx-runtime/src/credentials.rs:222
  - Evidence: `CredentialDelivery::from_allowed_binding` requires `CredentialBindingDecision`, `CredentialEnvelope`, `CredentialDeliveryProfile`, and a resolver. The draft only says to supply a token/secret and populate `MaterialResolver`; it does not specify how local provision creates or selects the envelope, profile, provider, auth mode, env binding, scopes, or allow decision.
  - Recommendation: Add the concrete input shape and derivation rules before approval; a raw `--secret` alone is insufficient for the existing delivery API.
  - Question: What is the exact local credential descriptor that turns a raw secret into a `CredentialEnvelope`, delivery profile, and allowed binding decision?
  - Recommended answer: Use a structured per-run descriptor: provider, auth_mode, env_var, grant_id/material_ref, scopes, and secret value; runtime declares an allow decision only for that local descriptor and profile match.
  - If unanswered: Default to requiring a structured CLI provision payload containing provider, auth_mode, env_var, material_ref/grant_id, and scopes, with runtime deriving the local allow decision.
- [high/blocks approval] `H4` scope/migration audit - Credential delivery ownership at the CLI/runtime boundary is undefined.
  - Status: open
  - Grounded in: code:crates/runx-runtime/src/execution/orchestrator.rs:18; code:crates/runx-runtime/src/execution/skill_run.rs:192; code:crates/runx-cli/src/skill.rs:11
  - Evidence: `SkillRunRequest` only carries skill_path, receipt_dir, run_id, answers_path, inputs, env, and cwd. `runner_invocation` always sets `credential_delivery: CredentialDelivery::none()`. The CLI `SkillPlan` has no credential fields and forwards only env/cwd/inputs.
  - Recommendation: Choose the ownership boundary. Runtime-side construction better preserves policy/redaction ownership; CLI-side construction makes the CLI own credential semantics.
  - Question: Should local credential provision be part of `SkillRunRequest`, or should the CLI precompute a `CredentialDelivery` before invoking `LocalOrchestrator`?
  - Recommended answer: Make `SkillRunRequest` carry a structured local credential provision request; runtime constructs `CredentialDelivery` and keeps policy/redaction centralized.
  - If unanswered: Default to adding structured local credential provision to `SkillRunRequest` and constructing `CredentialDelivery` inside runtime before adapter invocation.
- [high/blocks approval] `H5` design challenge - The provision UX is still a design menu, not an executable contract.
  - Status: open
  - Grounded in: spec_gap:Objectives
  - Evidence: The draft objective lists multiple possible UX surfaces (`runx grant`/`--secret`, local config file, env allowlist), and Phase 1 defers exact UX/wire design to build time. That leaves approved execution free to choose incompatible persistence, parsing, and rollback behavior.
  - Recommendation: Pick exactly one v1 surface in the spec and define its flags/file shape, precedence, validation failures, and redaction expectations.
  - Question: What is the v1 CLI surface: one-shot per-run secret, a `runx grant` command, a local config file, or env allowlist?
  - Recommended answer: Use one-shot per-run credential provision only; do not add persistent local config in v1.
  - If unanswered: Default to a per-run `runx skill ... --credential <descriptor>`/`--secret-env <ENV=...>` style surface with no persisted secret state.
- [medium/blocks approval] `H6` rollback/repair audit - Rollback is incompatible with the draft's local config file option.
  - Status: open
  - Grounded in: spec_gap:Rollback
  - Evidence: Rollback says reverting the CLI surface/wiring leaves no secret material persisted, but the objectives allow a local config file. A persistent implementation would need an explicit repair command/path cleanup.
  - Recommendation: Either forbid persistence for v1 or add exact storage path, file permissions, cleanup command, and rollback verification.
  - Question: Is v1 allowed to persist secret material locally, and if so what command/path removes it during rollback or repair?
  - Recommended answer: Forbid secret persistence in v1; accept only per-run in-memory material sourced from CLI/env.
  - If unanswered: Default to no persistent local secret storage in v1; rollback only removes code and allowlist additions.
- [low/advisory] `H7` acceptance timing audit - Validation feature activation is implicit but probably workable.
  - Status: open
  - Grounded in: code:crates/runx-cli/Cargo.toml:20; code:crates/runx-runtime/Cargo.toml:18; code:crates/runx-runtime/tests/credential_delivery.rs:1
  - Evidence: `runx-cli` enables runtime features `cli-tool` and `mcp`, while `runx-runtime` default features are empty and credential-delivery tests are gated with `cfg(all(feature = "cli-tool", feature = "mcp"))`. The planned package command likely enables the right runtime features through `runx-cli`, but the spec should avoid relying on implicit feature unification if the test moves to runtime-only invocation.
  - Recommendation: Clarify the feature expectation in validation notes; this is advisory because the current package pair appears to unify the needed features.
  - Question: Should the local credential test command explicitly name runtime features, or intentionally rely on `runx-cli` enabling them?
  - Recommended answer: Keep `-p runx-cli -p runx-runtime` and note that `runx-cli` enables runtime `cli-tool,mcp`; add explicit features if the command is changed.
  - If unanswered: Keep the current package pair or add explicit `--features runx-runtime/cli-tool,runx-runtime/mcp` if the command is narrowed.


