---
spec_version: '2.0'
task_id: x402-pay-paid-echo-composer-v1
created: '2026-05-21T00:46:25Z'
updated: '2026-05-21T09:46:36Z'
status: completed
harden_status: not_run
size: large
risk_level: high
---

# x402-pay paid-echo Rust runtime dogfood v1

## Current State

Status: completed
Current phase: final
Next: done
Reason: task completed
Blockers: none
Allowed follow-up command: `none`
Latest runner update: 2026-05-21T09:46:36Z
Review gate: pass

## Summary

Introduce a local-only `paid-echo` dogfood surface in the native Rust runtime
and prove the core sequence without a TypeScript composer dependency:
`payment_required` signal, quote, reserve, approval, mock rail fulfillment,
typed sealed payment proof, and only then the returned echo result.

This spec intentionally does not add `x402-charge`, `x402-refund`, or any
alias for `x402-pay`. The payment category remains a clean cutover to the
scoped `x402-pay` path. Provider-facing charge and refund surfaces remain
profile/flow families over the same Rust authority invariant, not competing
runtime skill names in this dogfood.

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
- Charge/refund profile cleanup beyond documenting that those names are not
  canonical x402-pay aliases.

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

Validation:
- [x] `v1` test - Rust payment execution test passes.
  - Command: `cargo test --quiet --manifest-path crates/Cargo.toml -p runx-runtime --test payment_execution`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-11
- [x] `v1b` feature parity - Rust payment execution test passes with
  - Command: `cargo test --quiet --manifest-path crates/Cargo.toml -p runx-runtime --features cli-tool --test payment_execution`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-12
- [x] `v2` dogfood - Core dogfood includes the Rust payment runtime.
  - Command: `node scripts/dogfood-core-skills.mjs`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-13
- [x] `v3` dogfood - Native CLI paid-echo fixture passes.
  - Command: `cargo test --quiet --manifest-path crates/Cargo.toml -p runx-cli --test x402_native_dogfood`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-14

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
- 2026-05-21T05:18:00Z: Native x402 mock payment dogfood moved into
  `crates/runx-cli/tests/x402_native_dogfood.rs`; paid-echo remains
  Rust-runtime-proven and still needs CLI fixture promotion.
- 2026-05-21T07:47:10Z: Naming boundary clarified: `x402-pay` is canonical;
  charge/refund names are profile flows only and not aliases or competing
  runtime skills for this cutover.
- 2026-05-21T08:15:00Z: CLI fixture promotion completed with
  `fixtures/harness/x402-pay-paid-echo.yaml` and native `runx-cli`
  `x402_native_dogfood` coverage.

## Review

Status: completed
Verdict: pass
Mode: verify
Summary: Human-reviewed override accepted: implemented and validated in commit 2ceb010; native x402 paid-echo dogfood, CLI fixtures, vitest, scafld validate, and dogfood lanes passed

Attack log:
- `review gate`: manual human audit -> clean (implemented and validated in commit 2ceb010; native x402 paid-echo dogfood, CLI fixtures, vitest, scafld validate, and dogfood lanes passed)

Findings:
- none

