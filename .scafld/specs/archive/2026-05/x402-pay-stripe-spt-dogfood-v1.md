---
spec_version: '2.0'
task_id: x402-pay-stripe-spt-dogfood-v1
created: '2026-05-21T00:46:25Z'
updated: '2026-05-21T09:46:42Z'
status: completed
harden_status: not_run
size: large
risk_level: high
---

# x402-pay Stripe SPT dogfood v1

## Current State

Status: completed
Current phase: final
Next: done
Reason: task completed
Blockers: none
Allowed follow-up command: `none`
Latest runner update: 2026-05-21T09:46:42Z
Review gate: pass

## Summary

Dogfood the canonical `x402-pay` path with the `stripe-spt` rail profile
through the native Rust runtime first. The current slice is offline and
deterministic: success with a scoped Stripe SPT proof, terminal decline, and
timeout preserving the reservation idempotency key. Existing `stripe-pay`
profile files are evidence carriers for this rail family, not aliases for a
native command or an alternate x402 surface.

Live Stripe test-mode execution remains a later gated layer. It must refuse
live keys and must not become the source of truth for payment authority,
receipt-before-forward, or raw-provider-material redaction.

## Scope And Touchpoints

In scope:

- `crates/runx-runtime/tests/stripe_spt_payment.rs`
- `scripts/dogfood-core-skills.mjs`
- `skills/stripe-pay/SKILL.md`
- `skills/stripe-pay/X.yaml`
- `skills/pay-fulfill-rail/SKILL.md`
- `skills/pay-fulfill-rail/X.yaml`
- Existing payment profile validation tests if fixture metadata changes.

Out of scope:

- Stripe live mode.
- Persisting real card data, API keys, webhook secrets, or raw credentials.
- Additional payment skill renames or alias compatibility paths.
- Refund, reversal, and dispute flows.
- `x402-charge`, `x402-refund`, or provider-specific charge/refund aliases.
- Native `runx x402-pay`, `runx receipts`, or `runx ledger` commands.
- TypeScript dogfood files as the primary proof path. They can wrap the Rust
  proof later, but the core invariant is native.

## Planned Phases

Phase 1: offline Rust Stripe SPT fixtures.
: Add deterministic native runtime fixtures for success, terminal decline, and
timeout/idempotency using sanitized provider-shaped references with no secrets.

Phase 2: gated Stripe test-mode dogfood.
: Add a script that runs only when explicit Stripe test-mode env vars are
present and refuses live keys.

Phase 3: recovery eventualities.
: Prove crash/recover and reconnect behavior against the same idempotency key.

## Acceptance

Profile: strict

Validation:
- [x] `v1` test - Rust Stripe SPT payment runtime tests pass.
  - Command: `cargo test --quiet --manifest-path crates/Cargo.toml -p runx-runtime --test stripe_spt_payment`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-11
- [x] `v1b` feature parity - Rust Stripe SPT payment runtime tests pass with
  - Command: `cargo test --quiet --manifest-path crates/Cargo.toml -p runx-runtime --features cli-tool --test stripe_spt_payment`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-12
- [x] `v2` dogfood - Core dogfood includes the Rust Stripe SPT payment runtime.
  - Command: `node scripts/dogfood-core-skills.mjs`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-13
- [x] `v4` dogfood - Native CLI Stripe SPT fixture passes.
  - Command: `cargo test --quiet --manifest-path crates/Cargo.toml -p runx-cli --test x402_native_dogfood`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-14

## Rollback

Strategy: per_phase

Commands:
- `git checkout HEAD -- crates/runx-runtime/tests/stripe_spt_payment.rs scripts/dogfood-core-skills.mjs skills/stripe-pay skills/pay-fulfill-rail tests/payment-skill-profile-validation.test.ts`

## Harden Rounds

- none

## Planning Log

- 2026-05-21T00:46:25Z: Filed from deferred Phase 2 `stripe-spt` scenarios in
  the completed mock-only dogfood spec.
- 2026-05-21T04:15:55Z: Recut to Rust-first offline proof. P2.1/P2.2/P2.5 are
  now represented as native runtime tests; the provider recovery eventualities
  remain pending.
- 2026-05-21T04:15:55Z: Core dogfood passed with the Rust Stripe SPT payment
  runtime test as an explicit queue step.
- 2026-05-21T05:18:00Z: Boundary recut kept the Stripe SPT proof Rust-first and
  identified CLI fixture promotion as the next required layer before any
  TypeScript wrapper or live test-mode script can count as dogfood evidence.
- 2026-05-21T07:47:10Z: Naming boundary clarified: Stripe SPT is a rail
  profile under canonical `x402-pay`; charge/refund names are not x402-pay
  aliases.
- 2026-05-21T08:15:00Z: CLI fixture promotion completed with
  `fixtures/harness/stripe-spt-payment.yaml` and native `runx-cli`
  `x402_native_dogfood` coverage.

## Review

Status: completed
Verdict: pass
Mode: verify
Summary: Human-reviewed override accepted: implemented and validated in commit 2ceb010; native Stripe SPT dogfood, CLI fixtures, cargo tests, vitest, scafld validate, and dogfood lanes passed

Attack log:
- `review gate`: manual human audit -> clean (implemented and validated in commit 2ceb010; native Stripe SPT dogfood, CLI fixtures, cargo tests, vitest, scafld validate, and dogfood lanes passed)

Findings:
- none

