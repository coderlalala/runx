---
spec_version: '2.0'
task_id: x402-pay-phase1-mock-scenario-punchlist
created: '2026-05-21T00:46:25Z'
updated: '2026-05-21T09:46:38Z'
status: completed
harden_status: not_run
size: small
risk_level: low
---

# x402-pay Phase 1 mock scenario punch list

This file is append-only. Rows stay here after closure with their status and
closing evidence updated by a follow-up spec.

Native Rust CLI coverage added 2026-05-21:

- P1.1 happy path remains covered by `fixtures/harness/x402-pay-approval.yaml`.
- P1.5 approval denial is now covered by
  `fixtures/harness/x402-pay-approval-denied.yaml`.
- `cargo test --manifest-path crates/Cargo.toml -p runx-cli --test
  x402_native_dogfood` proves both fixtures through the native Rust CLI with no
  Node, pnpm, tsx, or TypeScript package dependency.

Naming boundary: every row in this punch list belongs to canonical `x402-pay`
mock payment coverage. Charge and refund names are separate profile/flow
families, not `x402-pay` aliases and not Phase 1 executable payment surfaces.

| Scenario | Status | Observed Behavior | Expected Behavior | Concrete Blocker | Follow-up |
| --- | --- | --- | --- | --- | --- |
| P1.2 | Open | `pay-quote` has a happy-path inline harness only. | Malformed challenge is rejected with a governed error and no reserve, settlement, or ledger spend entry. | Missing negative quote fixture and CLI/harness assertion for governed refusal. | Add malformed challenge fixture for `pay-quote` and a no-settlement assertion. |
| P1.3 | Open | `pay-reserve` has a happy-path selected reservation only. | Cap-exceeded reserve declines before any rail call and records a refused intent. | Missing reserve decline fixture with policy cap below challenge amount. | Add cap-exceeded `pay-reserve` harness case plus ledger/refusal assertion. |
| P1.4 | Open | `pay-reserve` has no ambiguous-bounds refusal fixture. | Undefined currency or range bounds produce governed refusal and no rail call. | Missing ambiguous quote/reserve fixture and refusal packet shape. | Add ambiguous-bounds fixture after refusal packet schema is pinned. |
| P1.7 | Open | Current harness uses deterministic receipt ids but does not replay the same idempotency key through a second settlement attempt. | Replay returns the recovered receipt without issuing a second mock spend. | Missing mock rail spend counter or persisted idempotency lookup exposed through CLI/harness. | Add idempotency replay fixture once mock rail state is observable. |
| P1.8 | Open | Rust runtime graph governance admits scopes; payment authority subset enforcement is covered in Rust runtime tests, not this CLI harness matrix. | Broader child `AuthorityTerm` is rejected before mock execution. | Missing CLI/harness route that invokes core payment authority admission for crafted graph fixtures. | Add core-backed payment authority fixture or expose the Rust runtime graph runner to harness. |
| P1.9 | Open | No current CLI fixture consumes and then reuses the same spend capability ref. | Second use of a single-use spend capability is rejected by core. | Missing spend-capability consumption ledger visible to current harness. | Add single-use capability replay fixture after capability state is persisted. |
| P1.10 | Open | Happy-path receipts are persisted before test observation, but no fixture can delay receipt persistence after rail success. | Caller cannot observe success until the rail proof receipt is durably sealed. | Missing delayed receipt-store fault injection in CLI/harness. | Add receipt-store delay/failure fixture at runtime boundary. |
| P1.11 | Open | `pay-recover` has a happy-path recovery inspection harness, but no current CLI fixture simulates a mock rail crash after partial mutation. | Recovery queries by idempotency key after crash and classifies the state as sealed or escalated. | Missing crash/partial-mutation fixture and persisted rail state to recover against. | Add mock rail crash fixture once idempotency state is observable. |
| P1.12 | Open | Mock rail fixture always returns a proof ref. | Rail success without required proof fields is refused as non-sealable. | Missing proofless mock rail fixture and seal refusal assertion. | Add proofless mock rail fixture once rail proof schema validation is executable. |
| P1.13 | Open | Current reserve fixture is single-run and single-threaded. | Concurrent reserves under one policy use atomic budget arithmetic. | Missing policy budget store and concurrent CLI/harness driver. | Add budget store fixture and concurrent reserve test. |
| P1.14 | Open | Current graph uses matching quote/reserve/spend bounds. | Spend above reserved bounds is rejected before mock execution. | Missing quote-drift graph fixture wired into core authority comparison at CLI level. | Add drifted child authority fixture once P1.8 core-backed harness path exists. |
| P1.17 | Open | The graph ledger records run events and receipt links, not payment accrual/refusal projections. | Ledger projection distinguishes P1.1 accrual from P1.3/P1.4 refused entries. | Missing payment-specific ledger projection file and refusal scenarios. | Add payment ledger projection after refusal fixtures exist. |

## Current State

Status: completed
Current phase: final
Next: done
Reason: task completed
Blockers: none
Allowed follow-up command: `none`
Latest runner update: 2026-05-21T09:46:38Z
Review gate: pass

## Review

Status: completed
Verdict: pass
Mode: verify
Summary: Human-reviewed override accepted: implemented and validated in commit 2ceb010; x402-pay mock approval fixtures and punch-list coverage assertions passed

Attack log:
- `review gate`: manual human audit -> clean (implemented and validated in commit 2ceb010; x402-pay mock approval fixtures and punch-list coverage assertions passed)

Findings:
- none

