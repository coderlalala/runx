---
spec_version: '2.0'
task_id: x402-pay-stripe-spt-dogfood-v1
created: '2026-05-21T00:46:25Z'
updated: '2026-05-21T04:15:55Z'
status: draft
harden_status: not_run
size: large
risk_level: high
---

# x402-pay Stripe SPT dogfood v1

## Current State

Status: draft
Current phase: implementation
Next: full dogfood
Reason: Recut to Rust-first offline Stripe SPT runtime proof before any
TypeScript wrapper or live test-mode dogfood.
Blockers: none
Allowed follow-up command: `scafld harden x402-pay-stripe-spt-dogfood-v1`
Latest runner update: Rust runtime now proves Stripe SPT happy path, terminal
decline, and timeout/idempotency preservation.
Review gate: not_started

## Summary

Dogfood `stripe-pay` and its `pay-fulfill-rail` `stripe-spt` rail id through
the native Rust runtime first. The current slice is offline and deterministic:
success with a scoped Stripe SPT proof, terminal decline, and timeout preserving
the reservation idempotency key.

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

Definition of done:
- [x] `dod1` Offline Rust fixtures cover P2.1 success, P2.2 timeout, and P2.5
  decline from `x402-pay-dogfood-v1`.
- [x] `dod2` Offline proofs use provider-shaped references only and never commit
  secret material.
- [ ] `dod3` Recovery uses idempotency-preserving queries and never issues a
  second spend with a new key for P2.3, P2.4, P2.6, and P2.7.

Validation:
- [x] `v1` test - Rust Stripe SPT payment runtime tests pass.
  - Command: `cargo test --quiet --manifest-path crates/Cargo.toml -p runx-runtime --test stripe_spt_payment`
  - Expected kind: `exit_code_zero`
  - Timeout seconds: none
  - Result: 3 passed; 0 failed
  - Status: passed
  - Evidence: `stripe_spt_payment_seals_happy_path_with_scoped_proof`,
    `stripe_spt_payment_decline_returns_governed_error_without_sealing_success`,
    and `stripe_spt_payment_timeout_preserves_idempotency_for_recovery`.
  - Source event: none
  - Last attempt: local command
  - Checked at: 2026-05-21T04:15:55Z
- [x] `v1b` feature parity - Rust Stripe SPT payment runtime tests pass with
  `cli-tool` enabled.
  - Command: `cargo test --quiet --manifest-path crates/Cargo.toml -p runx-runtime --features cli-tool --test stripe_spt_payment`
  - Expected kind: `exit_code_zero`
  - Timeout seconds: none
  - Result: 3 passed; 0 failed
  - Status: passed
  - Evidence: Stripe SPT scenario behavior is identical in the CLI-backed
    runtime build.
  - Source event: none
  - Last attempt: local command
  - Checked at: 2026-05-21T04:15:55Z
- [x] `v2` dogfood - Core dogfood includes the Rust Stripe SPT payment runtime.
  - Command: `node scripts/dogfood-core-skills.mjs`
  - Expected kind: `exit_code_zero`
  - Timeout seconds: none
  - Result: Rust payment runtime 15 passed; Rust Stripe SPT payment runtime 3
    passed; x402 mock fixtures 4 passed; payment profiles 4 passed; canonical
    payment graph harnesses 4 passed; official skills 25 passed.
  - Status: passed
  - Evidence: core dogfood queue now runs `cargo test --quiet --manifest-path
    crates/Cargo.toml -p runx-runtime --test stripe_spt_payment`.
  - Source event: none
  - Last attempt: local command
  - Checked at: 2026-05-21T04:15:55Z
- [ ] `v3` gated test-mode - Stripe test-mode script passes when test env is
  present.
  - Command: `node scripts/dogfood-stripe-spt.mjs`
  - Expected kind: `exit_code_zero`
  - Timeout seconds: none
  - Result: none
  - Status: pending
  - Evidence: If env is absent, this validation is intentionally incomplete;
    skip is not a pass.
  - Source event: none
  - Last attempt: none
  - Checked at: none

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
