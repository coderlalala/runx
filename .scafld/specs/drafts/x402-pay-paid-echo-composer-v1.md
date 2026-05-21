---
spec_version: '2.0'
task_id: x402-pay-paid-echo-composer-v1
created: '2026-05-21T00:46:25Z'
updated: '2026-05-21T04:15:55Z'
status: draft
harden_status: not_run
size: large
risk_level: high
---

# x402-pay paid-echo Rust runtime dogfood v1

## Current State

Status: draft
Current phase: implementation
Next: full dogfood
Reason: Cut over from the stale TS-composer framing to the Rust runtime core
where payment authority, proof sealing, and graph forwarding now live.
Blockers: none
Allowed follow-up command: `scafld harden x402-pay-paid-echo-composer-v1`
Latest runner update: Rust payment execution test now covers paid echo success,
approval denial, and missing rail proof.
Review gate: not_started

## Summary

Introduce a local-only `paid-echo` dogfood surface in the native Rust runtime
and prove the core sequence without a TypeScript composer dependency:
`payment_required` signal, quote, reserve, approval, mock rail fulfillment,
typed sealed payment proof, and only then the returned echo result.

This spec intentionally does not add `x402-charge` or `x402-pay` aliases. The
payment category remains a clean cutover to the scoped x402-pay path; any
provider-facing charge surface is a later graph/skill layer over the same Rust
authority invariant.

## Scope And Touchpoints

In scope:

- `crates/runx-runtime/src/execution/graph.rs`
- `crates/runx-runtime/tests/payment_execution.rs`
- `scripts/dogfood-core-skills.mjs`
- Native Rust graph context forwarding for structured payment packets.
- Rust payment authority admission and typed rail proof before paid action
  forwarding.

Out of scope:

- Live-money rails and Stripe test mode.
- Internal paid surfaces.
- Additional payment skill renames or alias compatibility paths.
- Native `runx x402-pay`, `runx receipts`, or `runx ledger` commands.
- TypeScript composer interception. That may be a thin wrapper after the Rust
  invariant is stable, but it is not the core proof.
- Provider-side charge forwarding.

## Planned Phases

Phase 1: Rust paid-echo graph fixture.
: Add an in-memory Rust fixture that emits a `payment_required` signal for one
tool and accepts only a fulfilled credential/proof for that same tool.

Phase 2: core forwarding.
: Route the local signal through quote, reserve, approval, mock rail settlement,
and return the paid tool result only after the receipt is sealed.

Phase 3: negative paths.
: Prove denied approval, missing rail proof, and raw rail artifact suppression.

## Acceptance

Profile: strict

Definition of done:
- [x] `dod1` Local paid-echo success returns the echo result only after a sealed
  payment receipt exists.
- [x] `dod2` The paid echo receives only scoped credential/proof refs; no raw
  rail payload is forwarded.
- [x] `dod3` Negative paths cover approval denial and missing rail proof before
  echo invocation.

Validation:
- [x] `v1` test - Rust payment execution test passes.
  - Command: `cargo test --quiet --manifest-path crates/Cargo.toml -p runx-runtime --test payment_execution`
  - Expected kind: `exit_code_zero`
  - Timeout seconds: none
  - Result: 15 passed; 0 failed
  - Status: passed
  - Evidence: `x402_paid_echo_returns_echo_only_after_sealed_payment_proof`,
    `x402_paid_echo_denied_approval_never_invokes_payment_or_echo`, and
    `x402_paid_echo_missing_rail_proof_never_invokes_echo`.
  - Source event: none
  - Last attempt: local command
  - Checked at: 2026-05-21T04:07:27Z
- [x] `v1b` feature parity - Rust payment execution test passes with
  `cli-tool` enabled.
  - Command: `cargo test --quiet --manifest-path crates/Cargo.toml -p runx-runtime --features cli-tool --test payment_execution`
  - Expected kind: `exit_code_zero`
  - Timeout seconds: none
  - Result: 15 passed; 0 failed
  - Status: passed
  - Evidence: structured graph output forwarding is identical in the CLI-backed
    runtime build.
  - Source event: none
  - Last attempt: local command
  - Checked at: 2026-05-21T04:07:27Z
- [x] `v2` dogfood - Core dogfood includes the Rust payment runtime.
  - Command: `node scripts/dogfood-core-skills.mjs`
  - Expected kind: `exit_code_zero`
  - Timeout seconds: none
  - Result: Rust payment runtime 15 passed; Rust Stripe SPT payment runtime 3
    passed; x402 mock fixtures 4 passed; payment profiles 4 passed; canonical
    payment graph harnesses 4 passed; official skills 25 passed.
  - Status: passed
  - Evidence: core dogfood queue now runs `cargo test --quiet --manifest-path
    crates/Cargo.toml -p runx-runtime --test payment_execution` before the
    workspace package/dogfood checks.
  - Source event: none
  - Last attempt: local command
  - Checked at: 2026-05-21T04:15:55Z

## Rollback

Strategy: per_phase

Commands:
- `git checkout HEAD -- crates/runx-runtime/src/execution/graph.rs crates/runx-runtime/tests/payment_execution.rs scripts/dogfood-core-skills.mjs`

## Harden Rounds

- none

## Planning Log

- 2026-05-21T00:46:25Z: Filed from the paid-echo and composer deferrals in the
  completed mock-only dogfood spec.
- 2026-05-21T01:34:00Z: Recut to Rust-first after review: TS composer dogfood is
  stale until the native runtime authority and forwarding behavior is proven.
- 2026-05-21T04:07:27Z: Core dogfood passed with the Rust payment execution
  test as an explicit queue step.
- 2026-05-21T04:07:27Z: Re-ran payment execution with the `cli-tool` feature
  enabled to prove the generic structured output parser matches the CLI-backed
  runtime build.
- 2026-05-21T04:15:55Z: Core dogfood passed again after adding the Rust Stripe
  SPT payment runtime queue step.
