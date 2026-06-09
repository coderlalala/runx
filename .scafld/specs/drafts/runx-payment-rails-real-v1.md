---
spec_version: '2.0'
task_id: runx-payment-rails-real-v1
created: '2026-06-04T06:20:35Z'
updated: '2026-06-04T06:20:35Z'
status: draft
harden_status: not_run
size: medium
risk_level: medium
---

# runx-payment-rails-real-v1

## Current State

Status: draft
Current phase: none
Next: approve
Reason: draft created
Blockers: none
Allowed follow-up command: `scafld approve runx-payment-rails-real-v1`
Latest runner update: none
Review gate: not_started

## Summary

Make the magnet real: an agent buys a real resource over x402 (and Stripe SPT)
within a hard authority cap, sealing a verifiable spend receipt. The governance is
already real (authority bound, governed refusal, `finalize_output` refuses to seal
a spend without supervisor-verified proof) and the Phase-0 cloud-to-kernel bridge
is SHIPPED (signed admission token, release-on-denied, proof-gated seal,
intent-before-call gap-A, money_movement_id gap-B, per-run cap gap-E, offline
verify gap-6, finality webhook + reconciler tooling). What still runs in the CLI is
`DeterministicPaymentFinalitySupervisor` (a fixture, zero network); NO real rail is
wired. This spec lands the real-rail legs behind the deterministic supervisor seam:
x402 on Base Sepolia (Phase 1), Stripe SPT test-mode (Phase 2), the launch demo
with the agent tool-use loop, and the skill-catalog reshape. MPP is a later spec.

## Objectives

- A real x402 settlement on Base Sepolia: an external wallet signs the runx-bound
  EIP-712 `transferWithAuthorization`, the facilitator `/verify`+`/settle` runs, a
  real `0x` tx lands, runx never holds the key, the receipt resolves the tx and
  verifies offline.
- A real Stripe SPT test-mode charge through the same authority + governed path
  (`spt_…`/`ch_…`), runx never holding the card.
- The launch demo: one authority file, three acts (over-cap refusal, x402 buy,
  Stripe buy), all sealed + offline-verifiable, driven by the agent tool-use loop.
- Collapse payment internals to shared spend/charge/refund engines while keeping
  market-facing branded catalog facades where users recognize the provider:
  `x402-pay` and `stripe-pay` delegate to canonical `spend`; `mpp-pay` promotes
  only when MPP has a real adoption surface.

## Scope

In scope:
- Phase 1 x402 Base Sepolia: replace the env-driven `x402-testnet-settle.mjs`
  `--inspect` with a real external-signer (the bound EIP-712 template bytes) +
  facilitator `/verify`+`/settle` on the runtime-supervised external lane, behind
  `DeterministicPaymentFinalitySupervisor`'s seam; the `pay-fulfill-rail` x402 arm +
  `pay-recover` reconciler against real `/settle` latency.
- Phase 2 Stripe SPT: wire the built-but-unwired `cloud/packages/stripe-executor`
  into the governed spend path (`executeGovernedPayment` / `payment-spend.ts`).
- The agent tool-use loop (gap 10) in `cloud/packages/agent-runner`.
- Skill reshape (gap 9): move the step nodes toward owner-local graph stages under
  `skills/<name>/graph/<stage>/`; keep `mock-*` fixture-only; keep `x402-pay` and
  `stripe-pay` as branded catalog facades over canonical `spend`; promote
  `mpp-pay` later only if it becomes a recognizable user-facing surface.
- The HN/launch demo per payment-rails-demo.md (refuse-first, x402, Stripe, offline
  verify with `verify.mjs`).

Out of scope:
- Phase 4 MPP rails (mpp-fiat / mpp-tempo) + remaining async-finality gaps — later spec.
- Mainnet / real money (x402 stays Base Sepolia testnet; Stripe stays test mode).
- Any custodial handling. `deny.toml` bans reqwest/tokio outside runx-runtime, so the
  rail network legs MUST ride the runtime-supervised external lane, never pure crates.

## Dependencies

- SHIPPED: Phase-0 bridge (admission token, release-on-denied, proof-gated seal,
  gaps A/B/E/5/6), the authority runtime, the deterministic supervisor seam, the
  mcp-hosted payment tool, the finality webhook + `payment:reconcile-finality` /
  `payment:live-readiness` tooling.
- The external-signer EIP-712 bound-template bytes (amount/counterparty/idempotency
  layout the wallet signs + the kernel binds admission to).
- The agent tool-use loop (gap 10) for the published demo.

## Assumptions

- The supervisor seam is rail-agnostic and proven for two rails on fixtures; Phase 1/2
  swap fixture/env-driven clients for real facilitator/Stripe calls behind it.
- Testnet/test-mode is an honest demo posture: the chain, signature, and non-custody
  are real for x402; Stripe governance/receipt are identical, only issuance is test.

## Touchpoints

- `crates/runx-pay/src/{runtime.rs:107,supervisor.rs}` (the supervisor seam + proof gate).
- `oss/scripts/x402-testnet-settle.mjs` (real facilitator leg), `payments-demo.mjs`.
- `cloud/packages/{stripe-executor,agent-runner,billing/src/payment-spend.ts}`.
- The payment skill family + `oss/packages/cli/src/{official-skills.lock.json,skill-refs.ts}`.
- `oss/examples/governed-spend` + `verify.mjs` (the offline-verify centerpiece).

## Risks

- **Partial-failure at real latency (gap A).** A rail can settle while the proof write
  crashes. Mitigation: exercise the shipped intent-before-call + `pay-recover`
  reconciler against real `/settle` latency, not zero-latency fixtures.
- **External-signer template / preview-string drift.** Pin EIP-712 bytes, SPT preview
  version (2026-04-22), and the x402 header name against live docs at build time.
- **Skill-reshape churn on locked skills.** Collapse additively (faces keep ids),
  behavior-preserving, fixtures + the official lock regenerated.

## Acceptance

Profile: strict

Validation:
- An agent buys over x402 on Base Sepolia; a real `0x` tx lands on the public
  explorer; the sealed receipt resolves it and `verify.mjs` passes offline; runx
  never saw the key.
- The same authority buys over Stripe SPT test mode (`spt_`/`ch_` in the dashboard);
  the receipt verifies offline.
- An over-cap act is refused at admission (no mint/sign/settle); the denial receipt
  verifies offline.
- `pnpm verify:fast`, `pnpm fixtures:harness:check`,
  `cargo nextest run --workspace --all-features` green; payment skills still
  locked/maturity-tiered after the reshape.

## Phase 1: x402 on Base Sepolia (real settlement behind the supervisor seam)

Status: pending
Dependencies: Phase-0 bridge (shipped), external-signer template bytes

Objective: a real on-chain x402 settlement under the kernel authority, sealed +
offline-verifiable, runx non-custodial.

Changes:
- Real external-signer + bound EIP-712 template; facilitator `/verify`+`/settle`
  behind the deterministic supervisor; x402 `pay-fulfill-rail` arm + `pay-recover`.

Acceptance:
- [ ] `ac1` command - x402 settlement seals + verifies offline
  - Command: `runx harness examples/governed-spend/<x402-testnet-case>.yaml --json && node examples/governed-spend/verify.mjs <receipt>`
  - Expected kind: `exit_code_zero`
  - Status: pending

## Phase 2: Stripe SPT test-mode charge on the governed path

Status: pending
Dependencies: Phase 1

Objective: a real test-mode Stripe charge under the same authority + governed path.

Changes:
- Wire `stripe-executor` into `executeGovernedPayment`; the `stripe-pay` branded
  facade seals a canonical spend receipt resolving the `ch_`.

Acceptance:
- [ ] `ac2` command - stripe SPT charge seals + verifies offline
  - Command: `pnpm --dir ../cloud test && runx harness examples/governed-spend/<stripe-case>.yaml --json`
  - Expected kind: `exit_code_zero`
  - Status: pending

## Phase 3: Launch demo + skill reshape

Status: pending
Dependencies: Phase 1, Phase 2, agent tool-use loop (gap 10)

Objective: the three-act launch demo driven by the agent loop; payment skills
collapsed to engine + thin faces.

Changes:
- Land the agent tool-use loop; assemble the demo (refuse / x402 / stripe, one
  authority, offline verify); reshape skills (gap 9) additively.

Acceptance:
- [ ] `ac3` command - launch demo runs all three acts + offline verify
  - Command: `examples/governed-spend/run.sh && node examples/governed-spend/verify.mjs <each-receipt>`
  - Expected kind: `exit_code_zero`
  - Status: pending

## Rollback

- Each phase is additive behind the supervisor seam; reverting a rail restores the
  fixture supervisor + deletes that rail's client/tests, no contract churn. The
  skill reshape is additive (faces keep ids); revert restores the forked graphs.

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

## Origin

Created by: scafld
Source: plan

## Harden Rounds

- none

## Planning Log

- none
