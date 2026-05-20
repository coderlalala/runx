---
spec_version: '2.0'
task_id: rust-kernel-payment-authority-fixture-parity
created: '2026-05-20T00:00:00Z'
updated: '2026-05-20T00:43:00Z'
status: draft
harden_status: not_run
size: small
risk_level: high
---

# Rust kernel payment-authority fixture parity

## Current State

Status: draft
Current phase: local fixture parity implementation complete
Next: review/approve if this slice is promoted through the spec lifecycle
Reason: the kernel fixture generator, policy fixture schema, fixture set, and
Rust policy fixture runner now carry TypeScript-generated oracle fixtures for
payment-authority subset decisions.
Blockers: external review/spec lifecycle only. This slice intentionally did not
make Rust authoritative and did not implement payment rails or adapters.
Allowed follow-up command: run the validation commands below; do not run
`scafld harden rust-kernel-payment-authority-fixture-parity`.
Latest runner update: 2026-05-20 local implementation passed fixture, key-order,
and Rust policy parity validation.
Review gate: not_started

## Summary

Add fixture parity for the existing pure payment-authority subset comparator.
The target operation is a kernel fixture input such as
`policy.isPaymentAuthoritySubset` with `child` and `parent` authority terms and
a boolean expected output.

This slice does not make Rust authoritative. TypeScript remains the oracle, and
fixtures remain the cross-language conformance surface.

## Context

Grounded current facts:
- `crates/runx-core/src/policy/payment_authority.rs` exports
  `is_payment_authority_subset`.
- `crates/runx-core/tests/policy_proptest.rs` covers payment-authority subset
  behavior directly in Rust.
- `crates/runx-contracts/src/authority.rs` defines `AuthorityTerm`,
  `PaymentAuthorityBounds`, payment verbs, payment resource family, and
  `PaymentSingleUseSpend`.
- `fixtures/kernel/README.md` says payment-authority subset logic is covered by
  Rust unit/proptest coverage today and fixture parity remains a separate
  executable slice.
- `fixtures/kernel/schema/policy.schema.json`,
  `scripts/generate-kernel-parity-fixtures.ts`, and
  `crates/runx-core/tests/policy_fixtures.rs` now expose
  `policy.isPaymentAuthoritySubset`.

## Scope

In scope:
- Add or expose a TypeScript oracle for the pure payment-authority subset
  decision.
- Add payment-authority fixture cases under `fixtures/kernel/policy/`.
- Extend the policy fixture schema and generator/check mode for the new input
  kind.
- Extend the Rust policy fixture runner to dispatch the new input kind to
  `is_payment_authority_subset`.
- Preserve existing Rust proptests and unit coverage.

Out of scope:
- Runtime payment execution.
- Rail providers, wallets, ledger projections, payment receipts, or adapters.
- Changing payment skill behavior.
- Making Rust policy runtime-authoritative.
- CI promotion from advisory to blocking.

## Fixture Cases

Minimum fixture coverage:
- allows a child with narrower amount bounds, same currency, subset rails,
  preserved required conditions, preserved approvals, and compatible expiry.
- allows reserve/quote behavior without single-use spend capability when the
  child does not request `spend`.
- denies currency widening.
- denies rail widening.
- denies dropping a required payment boolean such as `receipt_before_success`.
- denies omitting a parent-required realm, counterparty, operation, or period.
- denies `spend` without `PaymentSingleUseSpend` and
  `single_use_spend`/`credential_form` evidence.
- denies resource-family or resource-ref mismatch.

## Acceptance

Profile: strict
Self-eval threshold: 8
Review provider: external Claude; local review does not satisfy complete.
Harden required before approve: yes

Definition of done:
- [x] `dod1` TypeScript generator/check mode emits deterministic
  payment-authority fixtures from the oracle.
- [x] `dod2` Policy fixture schema accepts only the new payment-authority input
  shape needed by this slice.
- [x] `dod3` Rust policy fixture runner compares
  `is_payment_authority_subset` against the TypeScript-generated expected
  boolean.
- [x] `dod4` Runtime payment execution and CI promotion are untouched by this
  fixture-parity slice. Parallel integration may include separate runtime
  payment proof work.
- [ ] `dod5` Review gate passes with the TypeScript-authoritative/advisory-CI
  posture intact.

Validation:
- [x] `v1` command - new fixture kind is wired on both sides.
  - Command: `rg -n 'policy\\.isPaymentAuthoritySubset' scripts/generate-kernel-parity-fixtures.ts fixtures/kernel/schema/policy.schema.json crates/runx-core/tests/policy_fixtures.rs`
  - Expected kind: `exit_code_zero`
  - Status: passed 2026-05-20.
- [x] `v2` command - payment-authority fixtures exist.
  - Command: `test -f fixtures/kernel/policy/payment-authority-allows-narrower-child.json && test -f fixtures/kernel/policy/payment-authority-denies-currency-widening.json && test -f fixtures/kernel/policy/payment-authority-denies-single-use-spend-without-capability.json`
  - Expected kind: `exit_code_zero`
  - Status: passed 2026-05-20.
- [x] `v3` command - fixture generator, validator, and key order are clean.
  - Command: `pnpm fixtures:kernel:check && pnpm fixtures:kernel:validate && pnpm fixtures:kernel:keys`
  - Expected kind: `exit_code_zero`
  - Timeout seconds: 120
  - Status: passed 2026-05-20.
- [x] `v4` command - Rust policy fixture and proptest coverage pass.
  - Command: `cargo test --manifest-path crates/Cargo.toml -p runx-core --test policy_fixtures && cargo test --manifest-path crates/Cargo.toml -p runx-core --test policy_proptest payment_authority`
  - Expected kind: `exit_code_zero`
  - Timeout seconds: 180
  - Status: passed 2026-05-20.
- [x] `v5` command - runtime payment execution was not touched by this slice.
  - Command: ownership review of changed files for this slice.
  - Expected kind: `manual_review`
  - Status: passed 2026-05-20; runtime changes in the final worktree belong to
    the separate payment runtime proof slice, not this kernel fixture slice.

## Phase 1: Oracle And Fixture Generator

Goal: make the TypeScript side emit deterministic payment-authority subset
fixtures.

Status: complete
Dependencies: none

Expected changes:
- TypeScript oracle/helper for pure payment-authority subset comparison.
- `scripts/generate-kernel-parity-fixtures.ts` dispatch and cases.
- `fixtures/kernel/schema/policy.schema.json` input kind.
- New `fixtures/kernel/policy/payment-authority-*.json` files.

## Phase 2: Rust Fixture Runner

Goal: make Rust consume the generated fixtures through the shared policy
fixture runner.

Status: complete
Dependencies: Phase 1

Expected changes:
- `crates/runx-core/tests/policy_fixtures.rs` adds the new fixture files and
  dispatches to `is_payment_authority_subset`.
- Existing payment-authority proptests remain in place.

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
Blocking count: none
Non-blocking count: none

Findings:
- none

Passes:
- none

## Metadata

Estimated effort hours: 6
Actual effort hours: none
AI model: none
React cycles: none

Tags:
- rust
- trusted-kernel
- payment-authority
- fixtures
- parity

## Origin

Source:
- split from obsolete `rust-kernel-port-orchestration` after observing that the
  Rust helper exists but fixture parity remains explicitly separate.

Repo:
- runxhq/runx

Git:
- none

Sync:
- none

Supersession:
- follows: rust-kernel-port-orchestration
- related: payment-authority-term-v1

## Harden Rounds

- none
