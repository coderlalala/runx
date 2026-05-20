---
spec_version: '2.0'
task_id: payment-refund-skills-v1
created: '2026-05-21T00:00:00Z'
updated: '2026-05-20T16:15:16Z'
status: review
harden_status: passed
size: medium
risk_level: high
---

# Payment refund skills v1

## Current State

Status: review
Current phase: final
Next: repair
Reason: review gate fail: 2 finding(s), 2 completion blocker(s)
Blockers: none
Allowed follow-up command: `scafld handoff payment-refund-skills-v1`
Latest runner update: 2026-05-20T16:19:03Z
Review gate: fail

## Summary

Refund execution in runx is a proposed governed graph profile that operates
over an existing sealed payment receipt. This v1 is intentionally
registry/profile-only. The first-party skills created by this spec make
after-the-fact money movement legible to humans and registry tooling, but they
do not add runtime refund execution, new CLI commands, new contract enums, or
live-money behavior.

The future runtime owner of this flow is still core: core must own the link to
the original receipt, refundable amount bounds, idempotency across reattempts,
the receipt-before-success invariant for the refund act itself, same-family
refund checks, open-dispute refusal, and authority subset proof against the
original charge before any profile here is treated as executable refund
movement.

## Current Codebase Alignment

- Current implemented payment skill directories are consumer-side:
  `payment-execute`, `payment-quote`, `payment-reserve`,
  `payment-rail-mock`, `payment-fulfill-rail`, and `payment-recover`.
- There are no concrete `x402-refund`, `stripe-refund`, `mpp-refund`,
  `mock-refund`, or `dispute-respond` skill directories yet. This spec
  creates those refund-side packages if accepted.
- Current native CLI entrypoints are `runx skill`, `runx harness`, and
  `runx history`. No refund-specific native CLI surface is assumed.
- `payment-fulfill-rail` uses rail ids `mock`, `x402`, `mpp`, and
  `stripe-spt`; refund-side settlement family names in this spec are
  proposed skill package names, not current rail ids.

## Product Rationale

Refund, reversal, and dispute response are separate from consumer-side
payment execution and provider-side charge because their authority points back
to a sealed receipt graph. A refund is bounded by what was already charged, a
reversal is system-derived recovery for a sealed-charge/no-forward condition,
and a dispute is initiated by the counterparty. Keeping the catalog family
separate prevents after-the-fact receipt repair from being hidden inside a
normal spend or charge graph.

## Scope And Touchpoints

In scope for this v1:

- Add `skills/refund-quote/SKILL.md` and `skills/refund-quote/X.yaml`.
- Add `skills/refund-reserve/SKILL.md` and `skills/refund-reserve/X.yaml`.
- Add `skills/refund-recover/SKILL.md` and `skills/refund-recover/X.yaml`.
- Add `skills/mock-refund/SKILL.md` and `skills/mock-refund/X.yaml`.
- Add `skills/stripe-refund/SKILL.md` and `skills/stripe-refund/X.yaml`.
- Add `skills/mpp-refund/SKILL.md` and `skills/mpp-refund/X.yaml`.
- Add `skills/x402-refund/SKILL.md` and `skills/x402-refund/X.yaml`.
- Add `skills/dispute-respond/SKILL.md` and
  `skills/dispute-respond/X.yaml`.
- Update `tests/payment-skill-profile-validation.test.ts` only as needed so
  the explicit refund and dispute skill names above are parsed,
  package-ingested, graph-reference checked, and raw merchant secret fields are
  rejected.
- Regenerate `packages/cli/src/official-skills.lock.json` with
  `node scripts/generate-official-lock.mjs` after adding the first-party skill
  directories.

Out of scope for this v1:

- No `skills/crypto-refund` directory or registry-installable placeholder.
- No changes to `crates/runx-*`, CLI commands, runtime graph execution,
  payment authority contracts, receipt enforcement, or durable refund indexes.
- No `packages/*` changes except the generated
  `packages/cli/src/official-skills.lock.json` lockfile required for
  first-party skill discovery.
- No new public packet schemas under `dist/packets` unless a future scoped
  spec adds schemas and contract tests.
- No live Stripe, MPP, x402, or on-chain refund.

## Authority And Receipt Model

Refund v1 reuses the existing `payment` `AuthorityTerm` model from
`payment-authority-term-v1`, including the existing `refund` verb. It does not
introduce a `refund` resource family, a separate `refund` bounds object, or
new authority enum values.

Profiles may carry refund-specific payload fields such as
`original_receipt_ref`, `prior_refund_receipt_refs`,
`refund_idempotency_key`, `refundable_bounds`, `settlement_family`,
`dispute_state`, and `refund_reason`. These fields are profile payload
conventions in v1, not first-class core enforcement fields. Inline authority
examples must remain valid `resource_family: payment` terms using existing
payment verbs and `bounds.payment` fields.

V1 profile examples support exactly one full refund or up to two partial
refund receipts per original receipt. A third refund attempt is modeled as
`unsupported_partial_refund_depth` even when amount remains. Broader
partial-refund accounting is deferred.

## Artifact Packet Boundary

Refund v1 does not introduce new public packet ids. Profiles may reuse existing
payment packet ids only where the packet semantics are already correct:

- `refund-quote` may use `runx.payment.quote.v1`.
- `refund-reserve` may use `runx.payment.reservation.v1`.
- `mock-refund`, `stripe-refund`, and `mpp-refund` may use
  `runx.payment.rail.v1` for the settlement proof artifact.
- `refund-recover` may use `runx.payment.recovery.v1`.
- `dispute-respond` must use profile-local artifact names in v1, such as
  `dispute_response` or `dispute_evidence`, with no `runx.payment.*` packet id
  unless a follow-up packet-schema spec adds one.

The refund profiles must not reference new ids such as
`runx.payment.refund.*` or `runx.payment.dispute.*` in this iteration.

## Planned Phases

1. Scaffold non-settlement refund and dispute profiles:
   create `refund-quote`, `refund-reserve`, `refund-recover`, and
   `dispute-respond` with human-readable `SKILL.md` files plus `X.yaml`
   profiles. These profiles may rely on profile payload fields for original
   receipt links, refundable bounds, recovery closure states, and dispute posture.
2. Scaffold settlement marquees after charge approval:
   create `mock-refund`, `stripe-refund`, `mpp-refund`, and `x402-refund`
   only after `payment-charge-skills-v1` is approved. Graph profiles must show
   quote -> reserve -> optional approval -> settlement while carrying the
   original receipt link at every stage. `x402-refund` stays static profile
   metadata; no dynamic dispatch is introduced.
3. Validate profile coverage:
   update the payment profile validation test to discover the declared refund
   and dispute skill names explicitly and run
   `pnpm exec vitest run tests/payment-skill-profile-validation.test.ts`.
   Then run `node scripts/generate-official-lock.mjs` and include the
   refreshed `packages/cli/src/official-skills.lock.json`. No runtime, CLI,
   contract, durable index, or packet-schema changes are part of this phase.

## Runtime Boundary

`x402-refund`, `stripe-refund`, `mpp-refund`, and `mock-refund` are graph
profile contracts in this v1. They may show the desired refund sequence,
same-family refusal, open-dispute refusal, receipt-before-success invariant,
and recovery closure states, but they must not claim that `runx` can execute a
refund rail mutation, repair an ambiguous refund, or enforce refund admission
at runtime. A later runtime spec must own link-before-quote,
receipt-before-success for `payment:refund`, same-family enforcement, and
crash-safe recovery before these profiles become operational behavior.

## Lifecycle Command Note

This checkout is the runx repository, not the scafld source repository. It
does not contain `./bin/scafld` or `cmd/scafld`. Lifecycle commands for this
draft use the configured `scafld` binary on `PATH`; in this environment that is
`/opt/homebrew/bin/scafld`. The source-checkout warning in `AGENTS.md` applies
when working inside the scafld source repository itself.

Three flows live in this family and are intentionally separated:

- Refund: provider-initiated return of funds against a sealed charge receipt.
- Reversal: system-initiated correction when a sealed receipt should not
  have been issued (e.g. the upstream tool call failed after the charge
  sealed). Driven by recovery, not by an operator action.
- Dispute: payer-initiated claim that a sealed charge was incorrect.
  Resolved through the provider's dispute response, not by silent refund.

## Skill Set

`refund-quote`
: Inspects a sealed charge receipt and computes the refundable bounds
(remaining amount, time-to-refund window, settlement-family constraints).
Non-mutating; receives no rail secrets.

`refund-reserve`
: Selects or declines the refund intent. The output is a Decision-shaped
reservation packet containing the linked receipt id, refundable bounds,
idempotency key, approval status, and the child authority term that may be
passed to a refund settlement step.

`refund-recover`
: Reconciles a refund idempotency key after crash, timeout, retry, or
ambiguous settlement state. Must query by the linked receipt id and refund
idempotency key before any repeat mutation.

`stripe-refund`, `mpp-refund`, `mock-refund`
: Proposed settlement-pinned refund marquees. Each composes refund-quote,
refund-reserve, optional approval, and the named settlement family's refund
verb (e.g. Stripe refund, MPP reverse). Each returns a refund proof
payload/ref suitable for sealing into a child harness receipt that links the
original receipt.

`crypto-refund`
: Reserved placeholder for on-chain refund. Documented for naming
continuity. Not exposed in the registry; no SKILL.md, no X.yaml profile, and
no harness case in this iteration.

`x402-refund`
: The proposed unpinned graph marquee. Same composition as the
settlement-pinned marquees, but the settlement family is selected from the
linked original receipt in the future runtime contract rather than baked into
the skill. In this v1 it is a static profile that carries desired dispatch
metadata and refusal examples; dynamic runtime dispatch is deferred. The
graph's authority is bounded by the original receipt's settlement family;
cross-family refunds are refused in profile examples.

`dispute-respond`
: Receives a provider-initiated dispute event (e.g. Stripe chargeback),
attaches the linked sealed charge receipt and any prior refund receipts, and
emits a governed profile-local response artifact. Does not settle. The future
dispute closure is itself a separate sealed receipt produced by the underlying
rail.

## Concept Boundaries

- A refund is always linked to exactly one prior sealed charge receipt.
- A reversal is not its own skill; it is what `payment-recover` and
  `charge-verify` produce together when a charge sealed but the upstream
  operation could not be forwarded. The reversal manifests as a refund
  receipt with `reason: reversal` and a system-derived authority term.
- A dispute is a counterparty event, not an operator-initiated refund.
  Silent refund of a disputed charge is refused; the operator must use
  `dispute-respond`.

## Spine Mapping

- Input is a sealed charge receipt reference (the link target).
- Quote output is refundable bounds, not refund.
- Reservation is a selected `Decision` and authority subset proof against
  the linked receipt.
- Refund settlement is a child `Harness` with one terminal `Act` whose
  receipt links the original.
- Dispute response is a sibling `Harness` whose receipt links both the
  original charge and any prior refund receipts.
- Ledger/reporting is a projection over the receipt graph (charge -> refund
  -> dispute), not over individual receipts.

## Future Core-Owned Rules

These rules are the target runtime invariants for a later executable refund
flow. They are not implemented by this profile-only v1.

- Core resolves the linked original receipt before any refund quote runs.
  Refunds against missing or already-fully-refunded receipts are refused
  before any rail is asked.
- Core enforces refundable bounds as the sum of prior refund receipts is
  always less than or equal to the original charge amount.
- Core compares child and parent `AuthorityTerm` values with the refund
  authority partial order, where the parent term is the original charge's
  sealed authority, not the operator's session authority.
- Core deduplicates refund attempts by the (original receipt id, refund
  idempotency key) pair.
- Core refuses success until the child refund receipt carries the settlement
  proof and the link to the original receipt.
- Core refuses a refund whose settlement family does not match the original
  receipt's settlement family.
- Core refuses a silent refund of a charge that has an open dispute on it;
  the operator must drive `dispute-respond` first.

## Skill-Owned Rules

- A refund-quote skill may classify the original receipt and recommend
  bounds, but it cannot authorize refund.
- A refund-reserve skill may present the human-readable refund decision
  record, but it cannot mint authority beyond what core admits from the
  original receipt.
- A refund settlement marquee may invoke one rail's refund verb, but it
  cannot set caps, read raw merchant secrets, or override the original
  settlement family.
- `dispute-respond` may attach evidence and select a response posture, but
  it cannot itself settle the dispute closure; only the rail can.

## Settlement Family Coverage

- `mock-refund`: deterministic local refund for harnesses, demos, and
  contract tests.
- `stripe-refund`: Stripe refund verb on sealed Stripe charges.
- `mpp-refund`: multi-party payment protocol reversal verb.
- `crypto-refund`: on-chain refund. Reserved placeholder, not exposed or
  harnessed in this iteration.

`x402-refund` is the proposed unpinned graph profile that documents how the
future runtime should resolve to one of the above based on the linked original
receipt's settlement family.

## Dependencies

- `payment-execution-skills-v1` supplies the consumer-side credential,
  idempotency, and authority-term wire shape.
- `payment-charge-skills-v1` supplies the provider-side charge receipt and
  settlement-family vocabulary this spec references. Phase 2 settlement
  marquee work (`mock-refund`, `stripe-refund`, `mpp-refund`, and
  `x402-refund`) must not start until `payment-charge-skills-v1` is approved.
  Phase 1 non-settlement refund/dispute profiles may be drafted before charge
  approval because they carry receipt linkage as profile payload conventions,
  not executable settlement-family behavior.
- `payment-authority-term-v1` supplies the existing `payment` authority
  contract reused by refund profile examples.
- `x402-pay-dogfood-v1` is not a prerequisite for profile scaffolding in this
  spec. It is a prerequisite only for a later executable refund dogfood/runtime
  claim against the same paid surface.
- A separate dogfooding spec will follow this one to exercise refund and
  dispute eventualities (refund of partial vs full charge, refund after
  expiry window, refund across restart, dispute won, dispute lost, refund
  during open dispute, double refund attempt).

## Out of Scope

- Live-money refund on any rail.
- `crypto-refund` activation or exercise.
- Multi-currency refund (refund settles in the same currency the original
  charge settled in).
- Partial-refund accounting across more than two settlement steps per
  original receipt; deferred to a v2 if real demand emerges.
- Cross-rail refund (e.g. charge on Stripe, refund on MPP). Explicitly
  refused by core.

## Acceptance Criteria

- Each first-party refund skill except the `crypto-refund` placeholder has
  a human-readable `SKILL.md`.
- Each first-party refund skill except the `crypto-refund` placeholder has
  an `X.yaml` profile with concrete inputs, outputs, artifacts, and harness
  cases.
- Graph profiles (`x402-refund`, `stripe-refund`, `mpp-refund`,
  `mock-refund`) make the authority transition visible: quote -> reserve ->
  optional approval -> settlement, with the link to the original receipt
  visible at every stage.
- Settlement profiles declare refund authority metadata under `runx` and
  never declare raw merchant secrets as inputs.
- `tests/payment-skill-profile-validation.test.ts` explicitly discovers and
  validates all eight non-crypto refund/dispute X.yaml files.
- `pnpm exec vitest run tests/payment-skill-profile-validation.test.ts` passes.
- `node scripts/generate-official-lock.mjs` refreshes
  `packages/cli/src/official-skills.lock.json`, and a second run leaves the
  lockfile unchanged.
- Refund profiles reference only the existing packet ids listed in Artifact
  Packet Boundary, or profile-local artifact names with no `packet` field.
- Scope audit passes with no task changes outside:
  `skills/refund-quote/`, `skills/refund-reserve/`,
  `skills/refund-recover/`, `skills/mock-refund/`,
  `skills/stripe-refund/`, `skills/mpp-refund/`,
  `skills/x402-refund/`, `skills/dispute-respond/`,
  `tests/payment-skill-profile-validation.test.ts`,
  `packages/cli/src/official-skills.lock.json`, and this spec file.
- `dispute-respond` has a SKILL.md and an X.yaml profile, but no settlement
  step of its own; its output is a profile-local evidence artifact plus a
  posture.
- The `crypto-refund` slot is documented but neither installable nor
  harnessed in this iteration.
- No new refund authority schema, public packet schema, runtime behavior,
  durable refund index, or CLI refund command is introduced or claimed.
- Runtime repair for ambiguous refund attempts is deferred. `refund-recover`
  may report modeled closure states such as `seal_recovered_proof`,
  `retry_same_key`, `decline`, or `escalate`, but it does not repair durable
  receipt state in v1.

## Acceptance And Rollback

Build rollback is mechanical: remove the eight non-crypto refund/dispute skill
directories, revert any changes to
`tests/payment-skill-profile-validation.test.ts`, and regenerate or revert
`packages/cli/src/official-skills.lock.json` back to the pre-refund skill set.
Since this v1 is profile-only and non-mutating, there is no data migration,
rail rollback, or durable receipt repair.

Operational repair for future ambiguous refund state is out of scope.
`refund-recover` is a skill/profile that reports the recommended terminal
action for a linked original receipt and refund idempotency key; runtime
automatic repair requires a follow-up runtime recovery spec.

## Harden Rounds

### round-1

Status: needs_revision
Started: 2026-05-20T15:34:49Z
Ended: 2026-05-20T15:34:49Z
Verdict: needs_revision
Provider: codex
Output format: codex.output_file
Summary: Harden verdict: needs revision. The product direction is coherent, but the draft is not approval-ready: dependencies are not satisfied, concrete scope/phases are missing, packet/schema work is undeclared, runtime/core claims exceed current enforcement, refund authority data has no first-class contract shape, and recovery/rollback is not operationally defined.

Checks:
- scope/migration audit
  - Grounded in: spec_gap:scope
  - Result: failed
  - Evidence: The harden packet rendered `scope`, `phases`, and `acceptance` sections as empty, and the draft itself has no explicit Scope And Touchpoints, Planned Phases, or Acceptance And Rollback sections. The draft instead has narrative sections and acceptance bullets at `.scafld/specs/drafts/payment-refund-skills-v1.md:177`.
- path audit
  - Grounded in: code:.scafld/specs/drafts/payment-refund-skills-v1.md:44
  - Result: failed
  - Evidence: The draft names future skills at `.scafld/specs/drafts/payment-refund-skills-v1.md:44` but does not declare concrete target paths for `skills/refund-quote`, `skills/refund-reserve`, `skills/refund-recover`, `skills/{stripe,mpp,mock,x402}-refund`, `skills/dispute-respond`, packet schemas, tests, or core/runtime files.
- command audit
  - Grounded in: command:scafld status payment-refund-skills-v1 --json
  - Result: failed
  - Evidence: `./bin/scafld status payment-refund-skills-v1 --json` failed because `./bin/scafld` is absent. `command -v scafld` found `/opt/homebrew/bin/scafld`, and `scafld status payment-refund-skills-v1 --json` reported draft status with `session_ok:false`.
- acceptance timing audit
  - Grounded in: code:tests/payment-skill-profile-validation.test.ts:90
  - Result: failed
  - Evidence: Existing profile validation discovers payment skills by directory name containing `payment` or profile text containing `resource_family: payment` / `payment[.:_-]`, and rejects unknown `runx.payment.*` packet refs against `dist/packets/payment.*.schema.json`. Refund directories such as `refund-quote` can be missed unless their profiles contain payment markers; new packet refs such as refund/dispute packets will fail unless packet schemas are added or existing packet ids are reused intentionally.
- rollback/repair audit
  - Grounded in: code:.scafld/specs/drafts/payment-refund-skills-v1.md:57
  - Result: failed
  - Evidence: The draft introduces `refund-recover` and says it reconciles by linked receipt id plus refund idempotency key, but it does not define the operator recovery command, the durable ledger/index repaired, or the terminal action when a rail mutation succeeds but the child refund receipt was not sealed.
- design challenge
  - Grounded in: code:crates/runx-contracts/src/authority.rs:62
  - Result: failed
  - Evidence: Current Rust/TS contracts expose payment authority, `refund` as an authority verb, and generic payment bounds, but no first-class original receipt ref, refundable remaining amount, settlement family, dispute state, or refund idempotency tuple in `PaymentAuthorityBounds`. Runtime enforcement currently keys receipt-before-success to `payment:spend`, not `payment:refund`.

Issues:
- [high/blocks approval] `HARDEN-1` scope_gap - The draft is not executable because it does not declare concrete scope or phases.
  - Status: open
  - Grounded in: spec_gap:scope
  - Evidence: The harden packet reports empty `scope`, `phases`, and `acceptance` sections. In the draft, acceptance bullets exist at `.scafld/specs/drafts/payment-refund-skills-v1.md:177`, but there are no explicit target paths or phases for a medium/high-risk task that creates multiple skills and possibly packet/test/runtime support.
  - Recommendation: Add Scope And Touchpoints and Planned Phases with exact future paths, plus an explicit non-goal list for core/runtime behavior if this is skill-profile-only.
  - Question: Which concrete files and directories are in scope for this v1: only `skills/*/SKILL.md` and `skills/*/X.yaml`, or also packet schemas, profile-validation tests, registry fixtures, and core/runtime authority code?
  - Recommended answer: Keep v1 skill-profile-only plus validation coverage: add `skills/refund-quote`, `skills/refund-reserve`, `skills/refund-recover`, `skills/mock-refund`, `skills/stripe-refund`, `skills/mpp-refund`, `skills/x402-refund`, and `skills/dispute-respond`; do not add `skills/crypto-refund`; update profile validation/packet artifacts only as needed; no core/runtime behavior changes in this spec.
  - If unanswered: Default to a skill-profile-only v1: create only non-crypto refund/dispute skill directories under `skills/`, add/update only profile validation and packet artifacts needed for those profiles, and explicitly exclude core/runtime enforcement until a follow-up spec.
- [high/blocks approval] `HARDEN-2` dependency_gap - The draft's own dependencies are not currently satisfied.
  - Status: open
  - Grounded in: code:.scafld/specs/drafts/payment-refund-skills-v1.md:155
  - Evidence: The draft depends on `payment-charge-skills-v1` being at acceptance, but that spec is still `status: draft`, `harden_status: needs_revision`, and records 6 approval-blocking issues at `.scafld/specs/drafts/payment-charge-skills-v1.md:16` and `.scafld/specs/drafts/payment-charge-skills-v1.md:20`. The draft also requires `x402-pay-dogfood-v1` Phase 1 green, but that spec is still `status: draft` at `.scafld/specs/drafts/x402-pay-dogfood-v1.md:16`.
  - Recommendation: Make dependencies phase-specific and enforceable: Phase 1 can scaffold non-mutating refund/dispute docs if desired; settlement graph profiles and harness cases must wait for charge acceptance and dogfood Phase 1 evidence.
  - Question: Should approval of this refund spec be blocked until `payment-charge-skills-v1` is accepted and `x402-pay-dogfood-v1` Phase 1 is green, or should the spec split into an early docs/profile phase and a later settlement phase gated on those dependencies?
  - Recommended answer: Split the plan: Phase 1 creates non-mutating `refund-quote`, `refund-reserve`, `refund-recover`, and `dispute-respond` profiles; Phase 2 creates settlement marquees only after `payment-charge-skills-v1` is accepted and `x402-pay-dogfood-v1` Phase 1 is green.
  - If unanswered: Default to blocking build of refund settlement profiles until charge is accepted and x402-pay dogfood Phase 1 is green; allow only non-settlement documentation/profile scaffolding if explicitly phased.
- [critical/blocks approval] `HARDEN-3` contract_gap - The spec relies on refund invariants that current payment authority contracts do not expose.
  - Status: open
  - Grounded in: code:crates/runx-contracts/src/authority.rs:62
  - Evidence: Rust contracts have `AuthorityVerb::Refund` but `PaymentAuthorityBounds` only includes generic currency/cap/rails/idempotency/recovery/receipt-before-success/single-use-spend fields at `crates/runx-contracts/src/authority.rs:62`; TS mirrors this at `packages/contracts/src/schemas/spine.ts:372`. There is no modeled original receipt link, refundable remaining amount, settlement family, dispute-open state, or `(original receipt id, refund idempotency key)` tuple.
  - Recommendation: Either add contract/schema work to scope with tests, or demote these to declared skill-profile payloads and state that core enforcement is deferred.
  - Question: Where will original receipt linkage, refundable bounds, settlement family matching, dispute-open refusal, and refund idempotency tuple live in v1: new contract/schema fields, profile payload conventions, or deferred runtime design?
  - Recommended answer: For v1, use explicit profile payload fields and packet schemas for `original_receipt_ref`, `refund_idempotency_key`, `refundable_bounds`, `settlement_family`, and `prior_refund_receipt_refs`; do not claim core enforcement until a follow-up runtime contract spec adds first-class fields.
  - If unanswered: Default to treating original receipt linkage, refundable bounds, settlement family, and dispute state as profile payload fields only, not enforceable core invariants, and remove claims that core already owns them from this spec.
- [critical/blocks approval] `HARDEN-4` runtime_claim_gap - The draft claims core refund enforcement that current runtime only implements for payment spend.
  - Status: open
  - Grounded in: code:crates/runx-runtime/src/execution/runner.rs:802
  - Evidence: The draft says core refuses success until the child refund receipt carries proof and original link at `.scafld/specs/drafts/payment-refund-skills-v1.md:123`, but current runtime receipt-before-success enforcement only applies to graph steps with `payment:spend` scope at `crates/runx-runtime/src/execution/runner.rs:802` and `crates/runx-runtime/src/execution/runner.rs:824`.
  - Recommendation: If runtime enforcement is in scope, add concrete runtime files, tests, and rollback. If not, rewrite Core-Owned Rules as future invariants and make acceptance prove only profile shape.
  - Question: Is this spec supposed to implement runtime admission/enforcement for `payment:refund`, or only profile the future governed flow without claiming runtime behavior?
  - Recommended answer: Profile-only in v1. Keep runtime enforcement out of scope, and change acceptance to require that profiles visibly carry the original receipt link and proof refs without claiming that core enforces them yet.
  - If unanswered: Default to no runtime claim: refund profiles may show desired receipt/proof fields, but acceptance cannot assert core enforcement of `payment:refund` receipt-before-success.
- [high/blocks approval] `HARDEN-5` acceptance_gap - Acceptance can fail because new refund/dispute packet refs are not declared anywhere.
  - Status: open
  - Grounded in: code:tests/payment-skill-profile-validation.test.ts:193
  - Evidence: Existing validation treats `runx.payment.*` packet refs as declared only when found in `dist/packets/payment.*.schema.json` at `tests/payment-skill-profile-validation.test.ts:193`. Current repo references known packet ids like `runx.payment.quote.v1`, `runx.payment.reservation.v1`, `runx.payment.approval.v1`, `runx.payment.rail.v1`, and `runx.payment.recovery.v1`, but no refund or dispute packet refs were found.
  - Recommendation: Declare packet ids in the spec and add packet schema/test work to scope if new ids are used.
  - Question: What packet ids should the new refund and dispute profiles emit, and are their schemas in scope for this task?
  - Recommended answer: Add explicit packet artifacts for `runx.payment.refund.quote.v1`, `runx.payment.refund.reservation.v1`, `runx.payment.refund.rail.v1`, `runx.payment.refund.recovery.v1`, and `runx.payment.dispute.response.v1`, or choose shorter names, but make schemas and validation updates part of Phase 1.
  - If unanswered: Default to adding packet schema artifacts for any new `runx.payment.refund.*` or `runx.payment.dispute.*` ids before profile acceptance, or reuse existing packet ids only when semantically accurate.
- [high/blocks approval] `HARDEN-6` rollback_repair_gap - Refund recovery is named but not operationally executable.
  - Status: open
  - Grounded in: code:.scafld/specs/drafts/payment-refund-skills-v1.md:57
  - Evidence: `refund-recover` is described at `.scafld/specs/drafts/payment-refund-skills-v1.md:57`, and core dedupe is described at `.scafld/specs/drafts/payment-refund-skills-v1.md:121`, but the spec does not define a durable store/index, repair command, or post-crash sequence for already-mutated rail state without a sealed refund receipt.
  - Recommendation: Add Acceptance And Rollback with concrete recovery command, expected inputs, terminal states, and what is repaired versus only reported.
  - Question: What exact recovery surface repairs an ambiguous refund attempt: `runx skill ./skills/refund-recover`, `runx harness`, a future `runx receipts` command, or runtime automatic recovery?
  - Recommended answer: For v1, `refund-recover` is a skill/profile that reports `seal_recovered_proof`, `retry_same_key`, `decline`, or `escalate`; it does not itself repair durable receipt state. Runtime automatic repair is deferred.
  - If unanswered: Default to a profile-only recovery packet and no operational repair guarantee; require a follow-up runtime recovery spec before claiming crash-safe refunds.
- [medium/blocks approval] `HARDEN-7` invariant_conflict - Refund amount invariant conflicts with the stated partial-refund accounting limit.
  - Status: open
  - Grounded in: code:.scafld/specs/drafts/payment-refund-skills-v1.md:116
  - Evidence: The draft says core enforces prior refund sum `<=` original charge at `.scafld/specs/drafts/payment-refund-skills-v1.md:116`, but out of scope excludes partial-refund accounting across more than two settlement steps at `.scafld/specs/drafts/payment-refund-skills-v1.md:172`. These conflict for any third refund attempt, including the double-refund scenario named in dependencies.
  - Recommendation: Make the partial-refund cardinality explicit and align core/refund-profile acceptance with it.
  - Question: What is the v1 rule for a third refund receipt against the same original charge: refused as unsupported, allowed if within bounds, or deferred entirely?
  - Recommended answer: V1 supports full refund or up to two partial refund receipts per original charge in mock/profile cases; a third refund attempt is refused as `unsupported_partial_refund_depth` even if amount remains.
  - If unanswered: Default to supporting exactly one full refund or up to two partial refund receipts per original receipt in profiles, with later attempts refused as unsupported in v1.
- [medium/advisory] `HARDEN-8` design_advisory - The unpinned refund graph implies runtime dispatch not shown by current graph profile patterns.
  - Status: open
  - Grounded in: code:skills/payment-execute/X.yaml:212
  - Evidence: `x402-refund` is described as selecting settlement family from the linked original receipt at runtime at `.scafld/specs/drafts/payment-refund-skills-v1.md:74`. Existing graph examples use static skill references, e.g. `payment-execute` has a static `payment-rail-mock` step at `skills/payment-execute/X.yaml:212`.
  - Recommendation: Avoid hidden dynamic dispatch in this spec unless runtime graph dispatch work is explicitly scoped and tested.
  - Question: Should `x402-refund` be a static graph profile with explicit mocked family branches, or does this task introduce dynamic settlement-family dispatch in graph execution?
  - Recommended answer: Make `x402-refund` a static profile that carries the desired dispatch metadata and refuses cross-family examples; dynamic runtime dispatch belongs in a later graph-runtime spec.
  - If unanswered: Default to a static graph profile with explicit transitions/refusals, not dynamic skill loading.

### round-2

Status: error
Started: 2026-05-20T15:53:45Z
Ended: 2026-05-20T15:53:45Z
Summary: provider error: provider failed: process idle timeout: ... s supported path=/Users/kam/.codex/.tmp/plugins/plugins/twilio-developer-kit/.codex-plugin/plugin.json

Checks:
- none

Issues:
- none

### round-3

Status: needs_revision
Started: 2026-05-20T15:56:54Z
Ended: 2026-05-20T15:57:20Z
Verdict: needs_revision
Provider: manual
Summary: Manual hardening confirms the content gaps from round 1 have been

Checks:
- scope/migration audit
  - Grounded in: code:.scafld/specs/drafts/payment-refund-skills-v1.md:65
  - Result: passed
  - Evidence: Scope now declares all refund/dispute skill directories, the
- path audit
  - Grounded in: code:.scafld/specs/drafts/payment-refund-skills-v1.md:69
  - Result: passed
  - Evidence: The future `SKILL.md` and `X.yaml` paths are explicit for
- command audit
  - Grounded in: code:.scafld/specs/drafts/payment-refund-skills-v1.md:158
  - Result: passed
  - Evidence: The draft documents that this runx checkout uses the configured
- acceptance timing audit
  - Grounded in: code:.scafld/specs/drafts/payment-refund-skills-v1.md:142
  - Result: failed
  - Evidence: Phase 2 settlement marquees are explicitly gated on
- rollback/repair audit
  - Grounded in: code:.scafld/specs/drafts/payment-refund-skills-v1.md:366
  - Result: passed
  - Evidence: Rollback is mechanical deletion of the added skill directories,
- design challenge
  - Grounded in: code:.scafld/specs/drafts/payment-refund-skills-v1.md:56
  - Result: passed
  - Evidence: The spec distinguishes refund, reversal, and dispute response as

Issues:
- [high/blocks approval] `HARDEN-9` dependency_timing_gap - Refund settlement
  - Status: open
  - Grounded in: code:.scafld/specs/drafts/payment-refund-skills-v1.md:142
  - Evidence: The refund draft now gates `mock-refund`, `stripe-refund`,
  - Recommendation: Approve `payment-charge-skills-v1` first, then rerun
  - Question: Should refund wait for charge approval, or should settlement
  - Recommended answer: Wait for charge approval, preserving the operator's
  - If unanswered: Default to blocking refund approval until charge approval.

### round-4

Status: passed
Started: 2026-05-20T16:09:23Z
Ended: 2026-05-20T16:10:26Z
Verdict: pass
Provider: manual
Summary: Manual pass recorded from the latest Codex harden dossier after

Checks:
- scope/migration audit
  - Grounded in: code:.scafld/specs/drafts/payment-refund-skills-v1.md:65
  - Result: passed
  - Evidence: Scope declares exact added skill files, the validation test, and
- path audit
  - Grounded in: code:.scafld/specs/drafts/payment-refund-skills-v1.md:69
  - Result: passed
  - Evidence: Refund/dispute target paths are explicit future files; charge
- command audit
  - Grounded in: code:.scafld/specs/drafts/payment-refund-skills-v1.md:167
  - Result: passed
  - Evidence: `scafld validate payment-refund-skills-v1` passes from the
- acceptance timing audit
  - Grounded in: code:tests/payment-skill-profile-validation.test.ts:18
  - Result: passed
  - Evidence: The profile validation test explicitly includes refund,
- rollback/repair audit
  - Grounded in: code:.scafld/specs/drafts/payment-refund-skills-v1.md:374
  - Result: passed
  - Evidence: Rollback is profile-only deletion/revert/regenerate and makes
- design challenge
  - Grounded in: code:.scafld/specs/drafts/payment-refund-skills-v1.md:156
  - Result: passed
  - Evidence: The spec keeps refund v1 registry/profile-only, defers runtime

Issues:
- [low/advisory] `HARDEN-10` stale_context - Current alignment text still
  - Status: open
  - Grounded in: code:.scafld/specs/drafts/payment-refund-skills-v1.md:43
  - Evidence: The workspace now contains provider-side charge skill
  - Recommendation: Treat charge skills as present for refund build, while the


## Review

Status: completed
Verdict: fail
Mode: verify
Provider: codex
Output: codex.output_file
Summary: Verification failed. The two known refund blockers are repaired: approval steps now carry the original receipt link and x402-refund no longer encodes dynamic dispatch. However, the raw-secret validation still misses common merchant/provider secret field names, so the refund acceptance criterion for rejecting raw merchant secret inputs is not actually enforced. Workspace changed during review; review failed closed.

Attack log:
- `skills/{mock-refund,stripe-refund,mpp-refund,x402-refund}/X.yaml`: Known blocker verification: approval-stage receipt propagation -> clean (Checked mock-refund, stripe-refund, mpp-refund, and x402-refund graph approval steps. Each approve-refund context now includes original_receipt_ref and refund_idempotency before settlement.)
- `skills/x402-refund/X.yaml`: Known blocker verification: x402 dynamic dispatch removal -> clean (Checked x402-refund for the prior original_receipt.settlement_family template and dispatch_source metadata. Both are gone; settlement is a static profile example and runtime_refund_enabled remains false.)
- `skills/{refund-quote,refund-reserve,refund-recover,dispute-respond,mock-refund,stripe-refund,mpp-refund,x402-refund}`: Scope coverage -> clean (Confirmed all eight in-scope non-crypto refund/dispute directories have SKILL.md and X.yaml, and skills/crypto-refund is absent.)
- `skills/*refund and skills/dispute-respond`: Packet boundary -> clean (Searched refund/dispute profiles for new runx.payment.refund.* or runx.payment.dispute.* packet ids. Profiles use existing payment packet ids or profile-local dispute artifacts without a packet field.)
- `packages/cli/src/official-skills.lock.json`: Official skill lock coverage -> clean (Inspected official-skills.lock.json and confirmed entries exist for all eight new refund/dispute skills.)
- `tests/payment-skill-profile-validation.test.ts`: Validation discovery coverage -> clean (Read tests/payment-skill-profile-validation.test.ts. The explicit skill-name set includes all eight refund/dispute skill names, so existing directories are discovered and parsed.)
- `tests/payment-skill-profile-validation.test.ts`: Raw merchant secret validation adversarial check -> finding (Tested the validation regex against common merchant/provider secret input names. It misses merchant_secret, stripe_api_key, client_secret, access_token, and api_key.)
- `skills/*refund, skills/dispute-respond, packages/cli/src/official-skills.lock.json`: Runtime/scope drift check -> clean (Read the refund profiles for runtime refund execution claims, CLI/runtime changes, durable index changes, and new public packet schema claims. No runtime execution surface was introduced in the inspected task files.)
- `workspace mutation guard`: compare pre-review and post-review workspace snapshots -> finding (added skills/refund-reserve/SKILL.md (M 4ba27e09b2a89c97921e9e09cc525187722fda002b5a81b2b57c47653280d201), added skills/refund-reserve/X.yaml (M 6e245163bc2eacb8de9c70eda02ff8e435480b6343f4fa25fd6da6cf6b57c01b))

Findings:
- [high/blocks completion] `REVIEW-1` Payment profile validation does not reject common raw merchant secret field names.
  - Location: `tests/payment-skill-profile-validation.test.ts:16`
  - Evidence: The acceptance contract requires settlement profiles to never declare raw merchant secrets as inputs and the validation test to reject raw merchant secret fields. The only guard is paymentSecretKeyPattern at tests/payment-skill-profile-validation.test.ts:16, used by findPaymentSecretFields. That regex catches secret_key and private_key, but not common raw merchant/provider secret names: merchant_secret, stripe_api_key, client_secret, access_token, or api_key. A read-only Node check against the exact regex returned false for those names and true for secret_key/private_key.
  - Impact: A refund settlement profile could declare common raw merchant secret inputs and still pass the payment profile validation test. That leaves the explicit raw-secret acceptance criterion unenforced for first-party refund skills.
  - Validation: Read tests/payment-skill-profile-validation.test.ts and ran a read-only Node regex check against representative raw merchant secret input names. Build/test commands were not run because the review packet requires read-only review and treats recorded acceptance evidence as already executed.
- [critical/blocks completion] `workspace_mutation` Workspace changed during review.
  - Location: `skills/refund-reserve/SKILL.md (M 4ba27e09b2a89c97921e9e09cc525187722fda002b5a81b2b57c47653280d201)`
  - Evidence: workspace changed during review: added skills/refund-reserve/SKILL.md (M 4ba27e09b2a89c97921e9e09cc525187722fda002b5a81b2b57c47653280d201), added skills/refund-reserve/X.yaml (M 6e245163bc2eacb8de9c70eda02ff8e435480b6343f4fa25fd6da6cf6b57c01b)
  - Impact: The review provider changed the workspace while acting as a read-only reviewer, so its verdict is not trustworthy.
  - Validation: Restore the workspace to the expected state, ensure the provider is read-only, then rerun scafld review.

