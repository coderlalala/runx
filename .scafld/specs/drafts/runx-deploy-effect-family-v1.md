---
spec_version: '2.0'
task_id: runx-deploy-effect-family-v1
created: '2026-06-09T03:24:53Z'
updated: '2026-06-09T03:24:53Z'
status: draft
harden_status: not_run
size: medium
risk_level: high
---

# runx-deploy-effect-family-v1

## Current State

Status: draft
Current phase: none
Next: harden
Reason: `plans/skill-catalog.md` identifies the deploy effect family as the
smallest proof that runx's payment governance model generalizes beyond money.
The runtime already has a generic `RuntimeEffect` trait and effect registry, but
the only consequential finality proof in production is payment-shaped.
Blockers: none
Allowed follow-up command: `scafld harden runx-deploy-effect-family-v1`
Latest runner update: 2026-06-09T03:24:53Z
Review gate: not_started

## Summary

Build the first non-payment consequential effect family: deterministic deploy
finality. This is not a real infrastructure deploy. It is a local proof path
that exercises the same `admit -> prepare_output -> finalize_output -> persist`
shape payment already uses, but with deployment evidence instead of settlement
evidence.

The point is conceptual and architectural: runx should prove a production action
the same way it proves a spend, without reusing payment-specific proof types.

## Objectives

- Add a `DEPLOY_EFFECT_FAMILY` runtime effect that proves a deploy-like action
  using deterministic local evidence.
- Bind deploy proof data to the sealed receipt: effect family, operation,
  environment, artifact digest, rollback handle, deployment ref,
  idempotency key, step id, receipt id/digest, and evidence digest.
- Keep payment proof types payment-specific. Reuse `RuntimeEffect` and generic
  `ProofKind::EffectEvidence` / `EffectFinality` seams, not
  `PaymentSupervisorProof`.
- Add one fixture skill or graph that emits valid deploy evidence and one
  negative case that fails before success when evidence is missing or mismatched.

## Scope

In scope:
- A deterministic local deploy effect implementation in OSS runtime code.
- Contract additions only where needed for generic deploy evidence/proof.
- Harness fixtures proving success and refusal.
- Tests that verify the deploy proof appears in receipt verification refs and is
  bound to the sealed receipt digest.

Out of scope:
- Real cloud/Kubernetes/Vercel/Fly/SSH deployment.
- Cloud-to-kernel hosted sealing bridge.
- Vector-of-proofs / multiparty atomic finality.
- Secret rotation, regulated export, incident break-glass, or scheduled loops.
- Payment rail refactors.

## Dependencies

- Generic effect runtime:
  - `crates/runx-runtime/src/effects/types.rs`
  - `crates/runx-runtime/src/effects/registry.rs`
  - `crates/runx-runtime/src/execution/runner/steps.rs`
- Generic contracts:
  - `crates/runx-contracts/src/authority.rs`
  - `crates/runx-contracts/src/reference.rs`
  - `crates/runx-contracts/src/receipt.rs`
- Existing payment implementation as a pattern only:
  - `crates/runx-pay/src/runtime.rs`
  - `crates/runx-pay/src/supervisor.rs`

## Assumptions

- `RuntimeEffect` is the right abstraction boundary. The deploy effect should not
  require a trait redesign for the first slice.
- `PaymentSupervisorProof` remains payment-shaped. Deploy gets its own proof
  payload or a genuinely generic effect proof wrapper.
- Existing generic contract seams (`AuthorityResourceFamily::Deployment`,
  `AuthorityEffectGuard`, `ProofKind::EffectEvidence`, and
  `ProofKind::EffectFinality`) are sufficient or can be extended additively.

## Touchpoints

- Likely new module under `crates/runx-runtime/src/effects/`.
- `runx-cli` runtime registration.
- A deploy-effect fixture graph first; `skills/deploy` becomes a catalog skill
  only when the user-facing deploy procedure, authority gates, and receipt
  semantics are ready to publish.
- Runtime integration tests under `crates/runx-runtime/tests/`.
- Fixture/harness tests under `oss/tests/` only if needed for CLI parity.

## Risks

- **Payment-shaped leakage.** The deploy proof must not smuggle payment terms
  such as rail, currency, amount, or spend capability into generic runtime code.
- **Fake deployment theater.** This slice is deterministic local finality, not a
  fake prod deploy. Naming and docs must say that plainly.
- **Receipt churn.** Existing payment receipts and canonical fixtures must not
  change.
- **Authority ambiguity.** Deployment authority must be explicit enough to prove
  environment/operation bounds, even if richer realm policy lands later.

## Acceptance

Profile: strict

Validation:
- `cargo fmt --all --check`
- `cargo test --workspace --all-features deploy_effect`
- `cargo nextest run --workspace --all-features`
- `pnpm verify:fast`
- Existing payment tests continue to pass without canonical fixture churn.

## Phase 1: Deploy Effect Contract And Runtime

Status: pending
Dependencies: generic effect runtime

Objective: Add the first deploy effect family behind the generic runtime seam.

Changes:
- Add a deploy evidence/proof shape that is not payment-shaped.
- Implement `RuntimeEffect` for deterministic deploy evidence.
- Register the effect family in the CLI runtime.
- Ensure successful finality adds generic effect proof refs to the receipt.

Acceptance:
- [ ] `p1_ac1` command - deploy effect compiles and unit tests pass
  - Command: `cargo test --workspace --all-features deploy_effect`
  - Expected kind: `exit_code_zero`
  - Status: pending
- [ ] `p1_ac2` command - payment-specific proof terms do not enter deploy modules
  - Command: `rg -n "PaymentSupervisorProof|amount_minor|currency|rail|spend_capability" crates/runx-runtime/src/effects crates/runx-contracts/src`
  - Expected kind: `no_matches`
  - Status: pending

## Phase 2: Fixture Skill And Negative Case

Status: pending
Dependencies: Phase 1

Objective: Prove the deploy family through a real runx skill/harness path.

Changes:
- Add a deterministic fixture skill or private first-party `deploy` skill.
- Positive case emits deploy evidence and seals.
- Negative case omits or mismatches evidence and fails before success.

Acceptance:
- [ ] `p2_ac1` command - positive deploy fixture seals with deploy proof refs
  - Command: `runx harness <deploy-effect-positive-fixture> --json`
  - Expected kind: `exit_code_zero`
  - Status: pending
- [ ] `p2_ac2` command - negative deploy fixture fails before success
  - Command: `runx harness <deploy-effect-negative-fixture> --json`
  - Expected kind: `exit_code_nonzero`
  - Status: pending

## Phase 3: Gates And Close

Status: pending
Dependencies: Phase 2

Objective: Prove the tree remains clean after introducing non-payment effect
finality.

Acceptance:
- [ ] `p3_ac1` command - Rust and TS fast gates
  - Command: `cargo fmt --all --check && cargo nextest run --workspace --all-features && pnpm verify:fast`
  - Expected kind: `exit_code_zero`
  - Status: pending
- [ ] `p3_ac2` manual - existing payment receipts and fixture digests do not churn
  - Expected kind: `manual`
  - Status: pending

## Rollback

- Remove the deploy effect module, registration, fixtures, and additive contract
  fields. Existing payment paths should be unaffected because this spec must not
  mutate payment proof shapes.

## Review

Status: not_started
Verdict: none

Findings:
- none

## Self Eval

- none

## Deviations

- none

## Metadata

- created_by: scafld
- source_plan: plans/skill-catalog.md
- catalog_wave: Wave 2

## Origin

Created by: scafld
Source: plan

## Harden Rounds

- none

## Planning Log

- 2026-06-09: Grounded against runtime effect review. Use `RuntimeEffect`, not a
  genericized `PaymentSupervisorProof`.
