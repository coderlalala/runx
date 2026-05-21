---
spec_version: '2.0'
task_id: x402-pay-ledger-projection-v1
created: '2026-05-21T00:00:00Z'
updated: '2026-05-21T00:00:00Z'
status: active
harden_status: not_run
size: medium
risk_level: medium
---

# x402-pay ledger projection v1

## Current State

Status: active
Current phase: native ledger integration complete
Next: broaden dogfood coverage to refusal projection artifacts and history
readback
Reason: Rust-native projection builder, deterministic artifact writer, native
harness completion hook, and `payment_ledger_projected` JSONL event emission
are implemented and covered without TypeScript.
Blockers: none for happy-path native event emission
Allowed follow-up command: `implement`
Latest runner update: 2026-05-21T00:00:00Z
Review gate: not_run

## Summary

Add the P1.17 dogfood lane for x402 payment ledger projection evidence. The
projection must distinguish happy settlement accrual from governed refusal:

- happy settlement accrues exactly one bounded spend when a sealed rail proof is
  present;
- governed refusal records the refusal reason and zero accrued spend when
  authority, bounds, or proof validation blocks settlement.

This lane is evidence/projection only. It does not add live x402 rails, ledger
mutation commands, money movement, or public aliases.

## Current Surface Gap

The current native surface can read local run ledgers and produce journal/history
projections in Rust, while the old TypeScript `runtime-local` reflect projection
path remains a sunset surface. Payment ledger projection must therefore land in
`runx-runtime` and be exposed through the native CLI; adding new trusted
orchestration to `packages/runtime-local` would violate `docs/ts-interop-boundary.md`.
The existing ledger can record emitted artifacts, receipt links, step events,
and generic run history, but it does not expose a payment-specific projection
contract that folds x402 payment packets into an accrual/refusal view.

Boundary status:

- Rust API: `runx_runtime::payment_ledger::build_payment_ledger_projection`,
  accepting sealed graph/harness receipts plus typed child receipt evidence.
- Native file output: `write_payment_ledger_projection_artifact` writes a
  deterministic projection artifact under
  `<receipt-dir>/artifacts/payment-ledger/x402-pay/<receipt-id>.json`, keyed by
  `x402-pay:<source-receipt-id>`, without requiring TypeScript knowledge-store
  code.
- Event payload: the writer returns a `payment_ledger_projected` payload with
  the projection artifact id, projection artifact path, source receipt id,
  scenario id, profile, and disposition.
- Native ledger event: native harness completion appends a deterministic
  `payment_ledger_projected` run event after writing the projection artifact
  when `RUNX_RECEIPT_DIR` is configured.
- Contract: `runx.payment_ledger_projection.v1` has stable JSON fields for
  `scenario_id`, `disposition`, `accrual`, `refusal`, `evidence_refs`, and
  `source_receipt_id`.

The implementation should live beside the existing Rust journal/history
projection boundary, not in core policy and not in `packages/runtime-local`.
Suggested target paths:

- `crates/runx-runtime/src/payment_ledger.rs`
- `crates/runx-runtime/tests/payment_ledger_projection.rs`
- `crates/runx-cli/src/history.rs` or the native harness completion path for
  projection artifact discovery, whichever is the narrower integration point
- `crates/runx-cli/tests/x402_native_dogfood.rs` for TS-free dogfood assertions
- optional generated schema path:
  `schemas/payment-ledger-projection.schema.json`

## Scope And Touchpoints

In scope:

- `.scafld/specs/active/x402-pay-ledger-projection-v1.md`
- `fixtures/ledger-projections/x402-pay-ledger-*.json`
- A future Rust runtime projection module and tests under
  `crates/runx-runtime/`
- Existing x402 harness fixtures as read-only inputs:
  `fixtures/harness/x402-pay-paid-echo.yaml`,
  `fixtures/harness/x402-pay-negative-cap-exceeded.yaml`,
  `fixtures/harness/x402-pay-negative-ambiguous-bounds.yaml`, and
  `fixtures/harness/x402-pay-negative-proofless-rail.yaml`

Out of scope:

- `.scafld/specs/drafts/rust-nitrosend-dogfood.md`
- Shared x402 test rewrites unrelated to P1.17.
- Live x402, Stripe, MPP, refunds, disputes, or real money rails.
- New public `runx ledger`, `runx x402-pay`, `x402-charge`, or `x402-refund`
  commands.
- Changing the existing ledger chain schema.

## Projection Contract

The v1 projection is a derived evidence document:

- `schema_version`: `runx.payment_ledger_projection.v1`
- `payment_profile`: `x402-pay`
- `source_receipt_id`: graph receipt id used for the projection
- `disposition`: `settled` or `refused`
- `scenario_id`: P1 scenario id when known
- `accrual`: settled amount, currency, rail, idempotency key, and proof refs;
  zero amount and empty proof refs for refusal
- `refusal`: null for settled, otherwise reason code, refused stage, and
  `ledger_spend_recorded: false`
- `evidence_refs`: receipt, harness, and artifact refs sufficient to rerun or
  audit the projection

Required P1.17 distinctions:

- P1.1/P1.5 happy settlement: `disposition: settled`,
  `accrual.amount_minor > 0`, `rail_proof_refs` non-empty, and refusal null.
- P1.3 cap exceeded: `disposition: refused`, `reason_code: cap_exceeded`,
  `accrual.amount_minor: 0`, and no rail proof refs.
- P1.4 ambiguous bounds: `disposition: refused`,
  `reason_code: ambiguous_bounds`, `accrual.amount_minor: 0`, and no rail call.
- P1.12 proofless rail: `disposition: refused`,
  `reason_code: missing_rail_proof`, `accrual.amount_minor: 0`, and no paid echo.

## Planned Phases

Phase 1: projection boundary.
: Add the Rust runtime projection builder, contract type, and unit tests that
fold existing x402 ledger artifact envelopes into the v1 projection.

Phase 2: persistence boundary.
: Persist the native projection artifact under the configured receipt directory
and return the `payment_ledger_projected` payload.

Phase 2b: native ledger integration.
: Invoke the projection artifact writer from the native harness completion path
and append `payment_ledger_projected` to the run ledger.

Phase 3: dogfood evidence.
: Run the happy and refusal fixtures, assert projection differences against the
`fixtures/ledger-projections/x402-pay-ledger-*.json` examples, and update the
Phase 1 punch-list only after the executable projection evidence exists.

## Acceptance

Profile: strict

Validation:
- [x] `v1` scafld - Spec validates.
  - Command: `scafld validate x402-pay-ledger-projection-v1 --json`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
- [x] `v2` fixture shape - Golden projection examples are valid JSON.
  - Command: `node -e "const fs=require('node:fs'); for (const f of fs.readdirSync('fixtures/ledger-projections').filter(f => f.startsWith('x402-pay-ledger-') && f.endsWith('.json'))) JSON.parse(fs.readFileSync('fixtures/ledger-projections/'+f,'utf8'))"`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
- [x] `v3` projection tests - Rust runtime projection distinguishes settlement from
  governed refusal.
  - Command: `cargo test --manifest-path crates/Cargo.toml -p runx-runtime --test payment_ledger_projection`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0; happy settlement and governed refusal projections
    matched `fixtures/ledger-projections/x402-pay-ledger-*.json`, and the
    artifact writer persisted/read back a projection under a receipt dir while
    returning `payment_ledger_projected`
- [x] `v4` x402 dogfood - Native x402 fixture lane remains green.
  - Command: `cargo test --manifest-path crates/Cargo.toml -p runx-cli --test x402_native_dogfood native_x402_negative_fixtures_refuse_without_settlement`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
- [x] `v5` projection persistence writer - Runtime writer persists the
  payment-ledger projection artifact and returns the event payload without
  TypeScript.
  - Command: `cargo test --manifest-path crates/Cargo.toml -p runx-runtime --test payment_ledger_projection x402_projection_artifact_writer_persists_under_receipt_dir_and_returns_event_payload`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
- [x] `v6` projection ledger integration - Native CLI harness run writes the
  payment-ledger projection artifact and ledger event without TypeScript.
  - Command: `cargo test --manifest-path crates/Cargo.toml -p runx-cli --test x402_native_dogfood native_x402_ledger_projection`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0; native `runx harness` honored
    `RUNX_RECEIPT_DIR`, wrote
    `artifacts/payment-ledger/x402-pay/hrn_rcpt_x402-pay-paid-echo.json`, and
    appended one `payment_ledger_projected` JSONL run event.

## Remaining Native Integration

The minimal Rust-native happy-path persistence and event emission are complete.
The remaining work is expansion, not a blocker for this boundary:

- add a governed-refusal CLI assertion that writes a refusal projection artifact
  without any settlement accrual;
- add history/readback projection coverage if `payment_ledger_projected` should
  be visible through `runx history`;
- archive this spec after the full native x402 dogfood lane runs clean with the
  ledger assertion included.

Validation commands:

- `cargo test --manifest-path crates/Cargo.toml -p runx-runtime --test payment_ledger_projection`
- `cargo test --manifest-path crates/Cargo.toml -p runx-cli --test x402_native_dogfood native_x402_negative_fixtures_refuse_without_settlement`
- `cargo test --manifest-path crates/Cargo.toml -p runx-cli --test x402_native_dogfood native_x402_ledger_projection`
- `scafld validate x402-pay-ledger-projection-v1 --json`

## Rollback

Strategy: per_phase

Commands:
- `rm -f .scafld/specs/active/x402-pay-ledger-projection-v1.md`
- `rm -rf fixtures/ledger-projections`
- Future implementation rollback:
  `git checkout HEAD -- crates/runx-runtime/src/payment_ledger.rs crates/runx-runtime/tests/payment_ledger_projection.rs crates/runx-cli/src/history.rs crates/runx-cli/tests/x402_native_dogfood.rs schemas/payment-ledger-projection.schema.json`

## Harden Rounds

- none

## Planning Log

- 2026-05-21T00:00:00Z: Filed after P1.3, P1.4, and P1.12 refusal fixtures
  existed. The current repo has no Rust-native payment-specific ledger
  projection API, so this spec recorded the missing boundary and
  executable acceptance commands.
- 2026-05-21T00:00:00Z: Added the Rust-native pure projection builder and
  focused runtime tests for the happy settlement and governed refusal golden
  fixtures. Persistence remains a follow-up boundary.
- 2026-05-21T00:00:00Z: Added the narrow Rust-native projection artifact writer
  and runtime persistence test. Full native ledger event append remains blocked
  on wiring the writer into harness completion and the run ledger writer.
- 2026-05-21T00:00:00Z: Wired native harness completion to write the projection
  artifact and append `payment_ledger_projected` when `RUNX_RECEIPT_DIR` is
  configured. Added native CLI dogfood coverage proving the artifact and event
  are produced without TypeScript.

## Review

Status: not_run
Verdict: pending
Mode: discover
Findings:
- none
