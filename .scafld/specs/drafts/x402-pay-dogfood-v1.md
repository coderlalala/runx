---
spec_version: '2.0'
task_id: x402-pay-dogfood-v1
created: '2026-05-21T00:00:00Z'
updated: '2026-05-21T00:00:00Z'
status: draft
harden_status: not_run
size: large
risk_level: high
---

# x402-pay dogfood v1

## Current State

Status: draft
Current phase: planning
Next: design review
Reason: First-draft dogfooding plan for the future `x402-pay` graph and the
current payment execution skills. Awaiting operator review before scafld
runner processing.

## Summary

Drive the current payment execution skills and the future `x402-pay` graph
through every governance and recovery invariant by using the current
`runx skill`, `runx harness`, and `runx history` surfaces against controlled
fixtures and a controlled paid surface. Iterate fixes against an append-only
hardening punch list until each phase passes without manual workarounds. This
spec defines the eventualities, the rails in scope, the loop, and what "done"
looks like before the previous `payment-execution-skills-v1` spec is allowed
to harden.

## Current Codebase Alignment

- There is no native `runx x402-pay ...` command today. Current native CLI
  entrypoints are `runx skill`, `runx harness`, and `runx history`.
- There are no current `runx receipts` or `runx ledger` CLI surfaces. Receipt
  state is observed through `runx history`, `runx skill inspect`, and explicit
  receipt or ledger projection files written by the harness under test.
- Current implemented payment skill directories are `payment-execute`,
  `payment-quote`, `payment-reserve`, `payment-rail-mock`,
  `payment-fulfill-rail`, and `payment-recover`.
- There is no concrete `x402-pay`, `mock-pay`, `stripe-pay`, or `mpp-pay`
  skill directory yet. Those names remain product intent and are deliverables
  if this dogfood spec decides to introduce them.
- `payment-fulfill-rail` uses rail ids `mock`, `x402`, `mpp`, and
  `stripe-spt`. It does not use `mock-pay` or `stripe-pay` rail ids.
- `paid-echo` and composer interception are future dogfood deliverables, not
  current product behavior.

## Why Dogfood Before Harden

The previous spec ships skeletons and X.yaml profiles. None of it has been
exercised under failure conditions. Hardening with synthetic tests would
encode our assumptions, not our blind spots. Dogfooding the CLI exposes the
governed surface to real timing, real ambiguity, and real operator ergonomics.
Findings feed back into core invariants, settlement marquees, plumbing skills,
and the CLI before any of it claims production behavior.

Dogfooding here means exercising the payment flow through the current CLI:
`runx harness <fixture|skill-dir|SKILL.md>` for fixture-backed cases and
`runx skill <skill-dir|SKILL.md>` for skill execution cases, then observing
closure through `runx history`, `runx skill inspect`, and receipt files.
Constructing the flow by hand or stubbing past the CLI does not count.

A native `runx x402-pay ...` command, `runx receipts`, or `runx ledger` would
be a new CLI surface. If any of those surfaces are created, they need their own
implementation, help text, and acceptance proof before a dogfood scenario may
depend on them.

## Phases

Phase 1: current mock rail through `payment-execute` and `payment-fulfill-rail`.
: Deterministic local settlement. Fastest iteration. Proves every Core-Owned
Rule and Skill-Owned Rule from `payment-execution-skills-v1` without external
rail variability. Phase 1 must be green before Phase 2 starts.

Phase 2: add `payment-fulfill-rail` with `stripe-spt` in Stripe test mode.
: Real rail behavior through the current rail id: timing, webhook ordering,
declines, rate limits, restarts mid-settlement. A `stripe-pay` graph marquee
may be introduced as a deliverable, but it is not a current skill directory.
Stripe live mode is explicitly out of scope.

Phase 3: deferred.
: Any `crypto-pay` or live-money graph stays hidden. Live-money rails,
production agent loops, and internal paid surfaces beyond the local dogfood
paid surface are not in scope for v1.

## Paid Surface

Phase 1 starts with the existing payment harness fixtures and may add a
minimal local `paid-echo` MCP server as a dogfood deliverable. `paid-echo`
issues a `payment_required` signal on a known tool name, accepts a fulfilled
credential, and echoes its input. Setup, invocation, and teardown live in the
dogfood loop, not in product code. No coupling to internal paid surfaces in
v1; internal surfaces only enter once a real paid tool exists and the loop is
already green.

## Eventualities

Each entry below is one runnable scenario. Each scenario is exercised by an
explicit current CLI invocation. Fixture cases use `runx harness`; skill
execution cases use `runx skill`. Expected state is visible through
`runx history`, `runx skill inspect`, explicit receipt files, and any ledger
projection files the scenario creates. Pass means: the expected closure was
produced without manual workarounds, escapes, or stack-trace leakage to the
operator.

### Phase 1 (current mock rail)

P1.1 Happy path
: Challenge issued, quote produced, reserve granted within policy, mock
settlement succeeds, receipt sealed with proof, paid tool result returned to
caller unchanged.

P1.2 Unsupported challenge shape
: Quote rejects a malformed challenge with a governed error. No reserve. No
settlement. No partial ledger entry.

P1.3 Reserve declines: cap exceeded
: Policy cap below required bound. Reserve refuses without contacting any
rail. Ledger records the refused intent, not a spend.

P1.4 Reserve declines: ambiguous bounds
: Challenge offers a range or undefined currency. Reserve refuses with a
governed reason; no rail call.

P1.5 Approval gate: approved
: Policy requires human approval at this amount. Operator approves through
the configured surface. Flow resumes; receipt sealed.

P1.6 Approval gate: denied
: Operator denies. Clean halt. No settlement attempt. Receipt records the
denial as a terminal decision.

P1.7 Idempotency replay
: Same idempotency key submitted twice. Second call returns the recovered
receipt without a second mock spend.

P1.8 Authority subset violation
: A crafted settlement step attempts an `AuthorityTerm` broader than the
reserved child term. Core rejects before mock execution.

P1.9 Single-use spend cap reuse
: A second use of the same spend capability ref is rejected by core.

P1.10 Receipt-before-success
: Mock settlement succeeds but receipt persistence is delayed. Caller does
not see a success result until the receipt is durably stored.

P1.11 Mock crash mid-settle
: Mock rail aborts after a partial state mutation. Recover queries by
idempotency key, classifies the state, and either seals or escalates.

P1.12 Settlement proof missing
: Mock rail returns success without the required proof fields. Core refuses
to seal the child receipt as success.

P1.13 Concurrent reserves
: Two paid tool calls reserve under the same policy at the same time. Budget
arithmetic is atomic; neither call sees stale bounds.

P1.14 Quote drift
: Bounds reserved at T1, mock attempts a spend above the reserved bound at
T2. Core rejects the spend before mock executes.

P1.15 CLI: invocation
: `runx harness <payment fixture>` and, where the skill can run directly,
`runx skill ./skills/payment-execute ...` run end to end without operator
intervention beyond the approval gate when configured. No stack traces.

P1.16 CLI: receipt observation
: `runx history` lists the sealed receipt after P1.1 within one operator
action, and `runx skill inspect <receipt-id>` or the receipt file shows
settlement family, proof ref, idempotency key, and sealed timestamp.

P1.17 Ledger projection observation
: The explicit ledger projection file, if present for the scenario, shows the
accrual for P1.1 and the refused entries from P1.3 and P1.4 distinctly. A
future `runx ledger` command is a separate CLI deliverable, not assumed here.

P1.18 Composer flow deliverable
: If this spec adds composer paid-tool interception, an outer skill that
invokes a paid tool transparently triggers the payment graph. The composer
sees a governed result or a governed error; never a raw rail artifact. This is
future work created by this dogfood spec, not current behavior.

### Phase 2 (`stripe-spt` in test mode)

P2.1 Happy path with test card
: Stripe test card settles through the `stripe-spt` rail id, webhook arrives,
receipt sealed, result returned.

P2.2 Settlement timeout
: Stripe slow. Recover distinguishes pending from failed without escalating
early or sealing prematurely.

P2.3 Crash mid-settlement
: Process killed after Stripe call returns but before receipt persisted. On
restart, recover queries Stripe state and reaches a terminal decision.

P2.4 Webhook ordering
: Webhook arrives before the foreground call would have persisted. Receipt
ordering is preserved; no double-seal.

P2.5 Test decline
: Stripe declines a known-decline card. Flow halts with a governed error,
no partial receipt.

P2.6 Rate limit
: Stripe returns a rate-limit error. Recover backs off, retries once,
escalates if still failing. Idempotency preserved across retries.

P2.7 Network partition
: Stripe call attempted while offline. Recover queries Stripe state on
reconnect, reaches a terminal decision without operator intervention beyond
the configured surfaces.

P2.8 Refund and reversal
: Deferred to a follow-up spec.

## Dogfood Loop

Each iteration:

1. Pick the next unmet eventuality.
2. Set the precondition state on the local fixture or dogfood `paid-echo`
   server, policy file, and (Phase 2) Stripe test account.
3. Run the current CLI entrypoint with the scenario inputs:
   `runx harness <fixture|skill-dir|SKILL.md>` for harness cases or
   `runx skill <skill-dir|SKILL.md>` for skill cases.
4. Observe closure through `runx history`, `runx skill inspect`, explicit
   receipt or ledger files, structured logs, and the CLI exit. Classify pass,
   fail, or ambiguous.
5. If fail or ambiguous, append a punch list entry. Land a fix as its own
   commit. Re-run the scenario from step 3 until pass.
6. Move to the next eventuality.

Phase 1 advances to Phase 2 only when every Phase 1 scenario is pass and the
punch list is empty.

## Hardening Punch List

Findings accrue in `.scafld/specs/drafts/x402-pay-dogfood-punchlist.md`. Each
entry records:

- Scenario id (e.g. P1.11).
- Observed behavior.
- Expected behavior.
- Root cause sketch.
- Fix commit reference once landed.
- Closed timestamp.

Closed entries remain in the file for audit. Entries are never edited or
deleted; only superseded by a follow-up entry that references the prior id.

## Out of Scope

- Live-money settlement on any rail.
- `crypto-pay` activation or exercise.
- Provider-side skills (`x402-charge` family). Deferred per the prior spec.
- Refund and reversal flows (P2.8).
- Multi-tenant policy and approval routing.
- UI affordances beyond what the CLI already exposes.
- Treating `runx x402-pay`, `runx receipts`, or `runx ledger` as current CLI
  surfaces without separately implementing and accepting them.

## Acceptance Criteria

- Every Phase 1 eventuality is pass with no manual workaround.
- Every Phase 2 eventuality (excluding P2.8) is pass with no manual workaround.
- Punch list is at zero open entries at the end of each phase before
  promotion.
- Each fix commit references the scenario id it closes.
- No scenario surfaces a raw stack trace, raw rail payload, or undocumented
  exit code to the operator.
- `runx history`, `runx skill inspect`, and explicit receipt or ledger files
  are the source of truth for every scenario closure; structured logs are
  diagnostic, not authoritative.
- The dogfood loop runs entirely through current `runx harness` and
  `runx skill` invocations unless this spec explicitly delivers and accepts a
  new native CLI command. No hand-orchestrated payment composition counts
  toward acceptance.
