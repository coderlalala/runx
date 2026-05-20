---
spec_version: '2.0'
task_id: payment-charge-skills-v1
created: '2026-05-21T00:00:00Z'
updated: '2026-05-20T16:06:55Z'
status: review
harden_status: passed
size: medium
risk_level: high
---

# Payment charge skills v1

## Current State

Status: review
Current phase: final
Next: repair
Reason: review gate fail: 2 finding(s), 2 completion blocker(s)
Blockers: none
Allowed follow-up command: `scafld handoff payment-charge-skills-v1`
Latest runner update: 2026-05-20T16:16:52Z
Review gate: fail

## Summary

Provider-side payment in runx is a proposed governed graph profile that prices
an inbound tool call, issues a payment challenge, verifies the returned
credential, and models the requirement that a receipt seal before the upstream
tool runs. This v1 is intentionally registry/profile-only. The first-party
skills created by this spec make the flow legible to humans and registry
tooling, but they do not add runtime charge execution, new CLI commands, new
contract enums, or live-money behavior.

The future runtime owner of this flow is still core: core must own pricing
bounds, challenge issuance, credential verification gates, the
receipt-before-forward invariant, and authority subset proof on the provider
charge side before any profile here is treated as executable money movement.
Settlement marquees adapt one protocol or provider family.

## Current Codebase Alignment

- Current implemented payment skill directories are consumer-side:
  `payment-execute`, `payment-quote`, `payment-reserve`,
  `payment-rail-mock`, `payment-fulfill-rail`, and `payment-recover`.
- There are no concrete `x402-charge`, `stripe-charge`, `mpp-charge`, or
  `mock-charge` skill directories yet. This spec creates those provider-side
  packages if accepted.
- Current native CLI entrypoints are `runx skill`, `runx harness`, and
  `runx history`. No charge-specific native CLI surface is assumed.
- `payment-fulfill-rail` uses rail ids `mock`, `x402`, `mpp`, and
  `stripe-spt`; charge-side settlement family names in this spec are proposed
  skill package names, not current rail ids.

## Product Rationale

Provider-side charge is intentionally separate from the consumer-side
`pay-*` family. Consumer-side payment starts from a challenge and spends
against it. Provider-side charge starts from an inbound operation, prices it,
emits the challenge, verifies the returned credential, and only then allows
the upstream operation to be forwarded. Keeping the catalog families separate
prevents one overloaded payment graph from hiding who holds authority at each
step.

## Scope And Touchpoints

In scope for this v1:

- Add `skills/charge-price/SKILL.md` and `skills/charge-price/X.yaml`.
- Add `skills/charge-challenge/SKILL.md` and
  `skills/charge-challenge/X.yaml`.
- Add `skills/charge-verify/SKILL.md` and `skills/charge-verify/X.yaml`.
- Add `skills/mock-charge/SKILL.md` and `skills/mock-charge/X.yaml`.
- Add `skills/stripe-charge/SKILL.md` and `skills/stripe-charge/X.yaml`.
- Add `skills/mpp-charge/SKILL.md` and `skills/mpp-charge/X.yaml`.
- Add `skills/x402-charge/SKILL.md` and `skills/x402-charge/X.yaml`.
- Update `tests/payment-skill-profile-validation.test.ts` only as needed so
  the explicit charge skill names above are parsed, package-ingested,
  graph-reference checked, and raw merchant secret fields are rejected.
- Regenerate `packages/cli/src/official-skills.lock.json` with
  `node scripts/generate-official-lock.mjs` after adding the first-party skill
  directories.

Out of scope for this v1:

- No `skills/crypto-charge` directory or registry-installable placeholder.
- No changes to `crates/runx-*`, CLI commands, runtime graph execution,
  payment authority contracts, or receipt enforcement.
- No `packages/*` changes except the generated
  `packages/cli/src/official-skills.lock.json` lockfile required for
  first-party skill discovery.
- No new public packet schemas under `dist/packets` unless a future scoped
  spec adds schemas and contract tests.
- No live Stripe, MPP, x402, or on-chain settlement.

## Authority Model

Charge v1 reuses the existing `payment` `AuthorityTerm` model from
`payment-authority-term-v1`. It does not introduce a `charge` resource family,
`charge` bounds object, or new verify-credential capability enum.

Profiles may carry charge-specific metadata under `runx.payment_authority`
using keys such as `direction: provider_charge`, `phase`, `settlement_family`,
and `receipt_before_forward_required`. Inline authority examples must remain
valid `resource_family: payment` terms using existing payment verbs and
`bounds.payment` fields. New schema fields are deferred to a runtime contract
spec if provider-side charge enforcement needs first-class contract support.

## Planned Phases

1. Scaffold charge skill profiles:
   create the seven non-crypto skill directories and author human-readable
   `SKILL.md` files plus `X.yaml` profiles with concrete inputs, outputs,
   artifacts, and harness cases. Graph profiles must show
   price -> challenge -> verify -> seal -> forward as declared steps while
   making clear that forward is modeled, not runtime-enabled.
2. Validate profile coverage:
   update the payment profile validation test to discover the declared charge
   skill names explicitly and run
   `pnpm exec vitest run tests/payment-skill-profile-validation.test.ts`.
   Then run `node scripts/generate-official-lock.mjs` and include the
   refreshed `packages/cli/src/official-skills.lock.json`. No runtime, CLI,
   contract, or packet-schema changes are part of this phase.

## Runtime Boundary

`x402-charge`, `stripe-charge`, `mpp-charge`, and `mock-charge` are graph
profile contracts in this v1. They may show the desired charge sequence and the
receipt-before-forward invariant, but they must not claim that `runx` can
execute a paid provider call, forward the upstream operation, or repair a
sealed-charge/no-forward split state. A later runtime spec must own
price-before-challenge, verify-before-seal, receipt-before-forward, and
provider-side recovery before those profiles become operational behavior.

## Skill Set

`charge-price`
: Turns an inbound MCP operation and policy context into a price packet plus
requested charge authority bounds. Settlement-agnostic; non-mutating;
receives no rail secrets.

`charge-challenge`
: Emits a `payment_required` signal at the priced bounds with an idempotency
key, the accepted settlement families, and any operator metadata. Settlement-
agnostic; does not verify credentials.

`charge-verify`
: Accepts a returned credential, dispatches to the matching settlement
verifier, and produces a sealed receipt with proof. Receives the already-
priced authority term and a single-use verify capability ref. Does not
forward the upstream tool call by itself.

`stripe-charge`, `mpp-charge`, `mock-charge`
: Proposed settlement-pinned graph marquees. Each composes price, challenge,
verify, and optional dispute pre-arm under the named settlement family. Each
receives an already-priced child authority term and a single-use verify
capability ref. Each returns a verified receipt payload/ref suitable for
sealing into the child harness receipt before the upstream operation
executes.

`crypto-charge`
: Reserved placeholder for on-chain settlement verification. Documented for
naming continuity. Not exposed in the registry; no SKILL.md, no X.yaml
profile, and no harness case in this iteration.

`x402-charge`
: The proposed unpinned graph marquee. Same composition as the
settlement-pinned marquees, but the verifier is selected from the inbound
credential family and provider policy in the future runtime contract. In this
v1 it is a static profile that makes the intended "tool-provider charges
agent" authority sequence visible without claiming executable forwarding.

## Spine Mapping

- Operation input is an MCP tool-call request paired with provider policy.
- Price output is evidence and requested authority, not collection.
- Challenge is a typed `payment_required` signal carrying idempotency.
- Verification is a child `Harness` with one terminal `Act`.
- Verified receipt is a receipt payload/reference with sensitive fields
  redacted.
- Forwarding the upstream tool call is a sibling `Act` gated by the sealed
  charge receipt.
- Ledger/reporting is a projection over sealed charge receipts.

## Future Core-Owned Rules

These rules are the target runtime invariants for a later executable provider
charge flow. They are not implemented by this profile-only v1.

- Core compares child and parent `AuthorityTerm` values with the charge
  authority partial order before verification starts. In v1, that charge
  partial order is expressed as provider-side use of existing payment
  authority terms.
- Core prices the operation under provider policy before any challenge is
  emitted.
- Core derives or validates the single-use verify capability for
  `verify_credential`.
- Core deduplicates credentials by idempotency key before reissuing a
  challenge or sealing a duplicate receipt.
- Core refuses to forward the upstream tool call until the child harness
  receipt carries the verified settlement proof.

## Skill-Owned Rules

- A price skill may classify the operation, recommend bounds, and surface
  policy metadata, but it cannot collect or commit charge.
- A challenge skill may format the `payment_required` signal and attach
  provider hints, but it cannot widen authority beyond what core admits.
- A settlement marquee may verify one payment protocol/provider family, but
  it cannot set prices, read raw merchant secrets, or decide forwarding.
- The graph marquee may select among settlement families based on inbound
  credential type and policy, but it cannot override pricing or skip
  verification.

## Initial Settlement Families

- `mock-charge`: deterministic local verification for harnesses, demos, and
  contract tests.
- `stripe-charge`: Stripe session/payment token credential verification.
- `mpp-charge`: multi-party payment protocol credential verification.
- `crypto-charge`: on-chain credential verification. Reserved placeholder,
  not exposed or harnessed in this iteration.

`x402-charge` is the proposed unpinned graph profile that records how the
future runtime should select one of the above from the inbound credential and
provider policy.

These names are first-party skill packages, not hardcoded core concepts. Core
only sees charge authority terms, idempotency keys, child harnesses, and
receipt proof refs.

## Dependencies

- `payment-execution-skills-v1` supplies the consumer-side credential,
  idempotency, and authority-term wire shape. It must remain the source of
  compatibility for charge profile examples.
- `payment-authority-term-v1` supplies the existing `payment` authority
  contract reused by this provider-side profile family.
- `x402-pay-dogfood-v1` is not a prerequisite for profile scaffolding in this
  spec. It is a prerequisite only for a later provider-side dogfood/runtime
  claim.

## Out of Scope

- Live-money settlement on any rail.
- `crypto-charge` activation or exercise.
- Refund, reversal, and dispute response. Covered in
  `payment-refund-skills-v1`.
- Multi-tenant provider policy and approval routing.
- Webhook delivery guarantees and replay protection beyond what the
  underlying rail adapter already provides.

## Acceptance Criteria

- Each first-party charge skill except the `crypto-charge` placeholder has a
  human-readable `SKILL.md`.
- Each first-party charge skill except the `crypto-charge` placeholder has
  an `X.yaml` profile with concrete inputs, outputs, artifacts, and harness
  cases.
- Graph profiles (`x402-charge`, `stripe-charge`, `mpp-charge`,
  `mock-charge`) make the authority transition visible:
  price -> challenge -> verify -> seal -> forward.
- Settlement profiles declare charge authority metadata under `runx` and
  never declare raw merchant secrets as inputs.
- `tests/payment-skill-profile-validation.test.ts` explicitly discovers and
  validates all seven non-crypto charge X.yaml files.
- `pnpm exec vitest run tests/payment-skill-profile-validation.test.ts` passes.
- `node scripts/generate-official-lock.mjs` refreshes
  `packages/cli/src/official-skills.lock.json`, and a second run leaves the
  lockfile unchanged.
- The `crypto-charge` slot is documented but neither installable nor
  harnessed in this iteration.
- No new `charge` authority schema, public packet schema, runtime behavior, or
  CLI charge command is introduced or claimed.
- Runtime repair for a future verified-charge/no-forward split state is
  deferred. Profiles may surface a `reversal_required` or recovery hint only as
  modeled metadata.

## Acceptance And Rollback

Build rollback is mechanical: remove the seven non-crypto charge skill
directories, revert any changes to
`tests/payment-skill-profile-validation.test.ts`, and regenerate or revert
`packages/cli/src/official-skills.lock.json` back to the pre-charge skill set.
Since this v1 is profile-only and non-mutating, there is no data migration or
runtime repair.

Operational repair for a future state where verification seals but upstream
forwarding fails is out of scope. Charge profiles should expose that eventuality
as a `reversal_required` or recovery hint, and the refund/reversal spec owns
the later executable recovery design.

## Harden Rounds

### round-1

Status: needs_revision
Started: 2026-05-20T15:31:55Z
Ended: 2026-05-20T15:31:55Z
Verdict: needs_revision
Provider: codex
Output format: codex.output_file
Summary: Harden verdict: needs revision. The draft has a coherent high-level story, but approval would be unsafe because key executable contract pieces are unresolved: concrete scope/paths, charge-vs-payment authority semantics, runtime behavior claims, validation coverage for charge profiles, and repair/rollback for verified-charge/no-forward cases.

Checks:
- scope/migration audit
  - Grounded in: spec_gap:scope
  - Result: failed
  - Evidence: The draft has acceptance criteria at `.scafld/specs/drafts/payment-charge-skills-v1.md:145`, but no explicit `Scope And Touchpoints`, `Planned Phases`, or rollback section. The packet also rendered those sections as empty.
- path audit
  - Grounded in: code:README.md:238
  - Result: failed
  - Evidence: Expected future files are implied as `skills/<skill>/SKILL.md` and `skills/<skill>/X.yaml` by README lines 238-241 and existing payment profiles. The draft does not declare concrete paths for the seven charge packages or whether any packet/schema files are in scope.
- command audit
  - Grounded in: code:tests/payment-skill-profile-validation.test.ts:19
  - Result: passed
  - Evidence: `scafld status payment-charge-skills-v1 --json` works via global scafld and confirms draft/harden gate. The likely profile validation command failed only because read-only sandbox blocked Vite temp-file creation, not because the test file is absent.
- acceptance timing audit
  - Grounded in: code:tests/payment-skill-profile-validation.test.ts:90
  - Result: failed
  - Evidence: Existing validation discovers payment skill dirs by directory name containing `payment` or profile text matching `resource_family: payment` / `payment[.:_-]`. Charge packages named `charge-*`, `stripe-charge`, etc. can avoid that discovery unless they reuse payment markers or the test is extended.
- rollback/repair audit
  - Grounded in: code:.scafld/specs/drafts/payment-refund-skills-v1.md:88
  - Result: failed
  - Evidence: No rollback/repair section states how to recover when charge verification succeeds but upstream forwarding fails. The refund draft says reversal is produced by `payment-recover` and `charge-verify` together, but this charge draft excludes refund/reversal/dispute without defining the interim repair command.
- design challenge
  - Grounded in: code:.scafld/specs/drafts/payment-charge-skills-v1.md:65
  - Result: failed
  - Evidence: The draft calls this a skill-only v1 and says no runtime/CLI charge behavior is claimed, but `x402-charge` is described as returning a forwarded result from one client call. Current core exposes payment authority primitives, not a distinct charge authority model.

Issues:
- [high/blocks approval] `HARDEN-1` scope_gap - The draft is not executable because it does not define the files and code surfaces the builder may touch.
  - Status: open
  - Grounded in: spec_gap:scope
  - Evidence: The draft lacks concrete scope/touchpoints and phases. It does not list target paths like `skills/charge-price/SKILL.md`, `skills/x402-charge/X.yaml`, packet schema paths, or tests to edit, despite acceptance requiring multiple new skill packages and profile parsing.
  - Recommendation: Add a Scope And Touchpoints section with exact future paths and an explicit non-goal list for core/runtime changes.
  - Question: Which concrete files and directories are in scope for this v1: only `skills/*/SKILL.md` and `skills/*/X.yaml`, or also packet schemas, registry fixtures, parser tests, and core authority code?
  - Recommended answer: Keep v1 skill-profile-only plus validation coverage: create `skills/charge-price`, `skills/charge-challenge`, `skills/charge-verify`, `skills/mock-charge`, `skills/stripe-charge`, `skills/mpp-charge`, and `skills/x402-charge`; do not add `skills/crypto-charge`; update profile validation so these are parsed and secret-scanned.
  - If unanswered: Default to a skill-profile-only scope: add SKILL.md and X.yaml files under `skills/` for all non-crypto charge skills; update only profile validation/tests needed to cover them; no runtime/core behavior changes unless explicitly declared in a follow-up spec.
- [critical/blocks approval] `HARDEN-2` contract_gap - The spec relies on charge authority terms that current contracts do not expose.
  - Status: open
  - Grounded in: code:crates/runx-contracts/src/authority.rs:7
  - Evidence: Rust authority terms have `AuthorityResourceFamily::Payment`, verbs including `Quote`, `Reserve`, `Spend`, `Refund`, `Verify`, capability `PaymentSingleUseSpend`, and `bounds.payment`; there is no charge resource family, charge bounds, or verify-credential capability. TS schemas mirror payment-only bounds and capabilities.
  - Recommendation: Resolve this before approval; otherwise implementers can either invent unsupported `charge` schema fields or silently overload payment semantics.
  - Question: Is `charge authority` a new contract surface, or is it the provider-side use of the existing `payment` authority term and bounds?
  - Recommended answer: Use the existing `payment` authority model for v1. Charge profiles should declare provider-side phases under `runx.payment_authority` or a clearly named profile metadata key, while core schema changes for a distinct charge authority are deferred.
  - If unanswered: Default to reusing `resource_family: payment` and existing `bounds.payment` with charge-specific phases expressed under `runx.payment_authority.phase`, and prohibit new charge authority schema or enum changes in this spec.
- [high/blocks approval] `HARDEN-3` invariant_conflict - The draft mixes skill-profile deliverables with runtime behavior claims.
  - Status: open
  - Grounded in: code:.scafld/specs/drafts/payment-charge-skills-v1.md:65
  - Evidence: The draft says `x402-charge` is a one-call surface that returns either a sealed charge receipt plus forwarded result or refusal, but acceptance later says no runtime or CLI charge behavior is claimed until runtime harness enforcement exists.
  - Recommendation: Make the product goal explicit and remove contradictory runtime claims from either the skill description or acceptance criteria.
  - Question: Should v1 actually implement forwarding behavior, or only document/profile the governed graph shape for future runtime enforcement?
  - Recommended answer: V1 is a registry/tooling legibility release only. It creates human-readable skills and X.yaml graph profiles that make the future charge flow visible, but it does not make `runx x402-charge` executable or forward upstream tool calls.
  - If unanswered: Default to describing `x402-charge` as a graph profile contract only; it may show the intended price/challenge/verify/seal/forward sequence but must not claim executable forwarding behavior in v1.
- [high/blocks approval] `HARDEN-4` acceptance_gap - The current acceptance claim can pass without validating the new charge profiles.
  - Status: open
  - Grounded in: code:tests/payment-skill-profile-validation.test.ts:90
  - Evidence: Existing profile validation discovers dirs by `entry.name.includes("payment")` or profile text matching `resource_family: payment` / `payment[.:_-]`. New packages named `charge-price`, `x402-charge`, etc. may not be covered if their metadata uses `charge_authority` or avoids payment markers.
  - Recommendation: Add a concrete acceptance command and require either explicit charge discovery or a shared payment/charge validation helper.
  - Question: What exact validation command proves all new charge X.yaml files are parsed and secret-scanned, and should the existing payment profile test be extended to include charge packages explicitly?
  - Recommended answer: Use `pnpm exec vitest run tests/payment-skill-profile-validation.test.ts` after extending discovery to include the declared charge skill names and charge metadata markers.
  - If unanswered: Default to extending the validation test to explicitly discover the charge skill names from this spec and validate their SKILL.md/X.yaml pairs.
- [high/blocks approval] `HARDEN-5` rollback_gap - Receipt-before-forward has no repair path when the second half of the flow fails.
  - Status: open
  - Grounded in: code:.scafld/specs/drafts/payment-refund-skills-v1.md:88
  - Evidence: No rollback/repair section exists. The refund draft says reversal is what `payment-recover` and `charge-verify` produce together when a charge sealed but upstream operation could not be forwarded, but this charge draft excludes refund/reversal/dispute and does not define the v1 repair story.
  - Recommendation: Add a rollback and operational repair section. If v1 is profile-only, say so plainly and avoid claiming repair behavior.
  - Question: For v1, what is the human recovery path if a charge verifies and seals but forwarding the upstream tool call fails?
  - Recommended answer: In v1 this is only a modeled eventuality, not executable behavior. The profile should surface a `reversal_required` or recovery hint, and actual repair is deferred to the refund/reversal spec; build rollback removes the new charge skill packages and validation entries.
  - If unanswered: Default to no live-money or mutating repair in v1; rollback is deleting the added skill/profile files and validation changes before release. Runtime repair for sealed-charge/no-forward is deferred to refund/reversal specs and must not be claimed by charge v1.
- [medium/advisory] `HARDEN-6` dependency_clarity - Dependency language can be read as blocking this spec on an unfinished dogfood draft.
  - Status: open
  - Grounded in: code:.scafld/specs/drafts/x402-pay-dogfood-v1.md:216
  - Evidence: `payment-execution-skills-v1` is completed, satisfying the charge draft’s dependency, but `x402-pay-dogfood-v1` is still draft and says provider-side charge skills are deferred. Charge draft says provider-side dogfooding is filed separately once consumer-side Phase 1 is green.
  - Recommendation: Clarify dependency timing so build agents do not wait on a draft dogfood spec for a profile-only task.
  - Question: Is consumer-side dogfood Phase 1 a prerequisite for building charge profiles, or only for claiming runtime/provider dogfood behavior later?
  - Recommended answer: Not a prerequisite for building these profiles. It is a prerequisite only for a later provider-side dogfood/runtime claim.
  - If unanswered: Default to making dogfood a non-blocking follow-up dependency and prohibit using dogfood pass criteria as acceptance for this spec.
- [medium/advisory] `HARDEN-7` artifact_contract_gap - The artifact/packet contract for charge outputs is underspecified.
  - Status: open
  - Grounded in: code:dist/packets/payment.quote.v1.schema.json:4
  - Evidence: `dist/packets` currently includes payment quote/reservation/approval/rail/recovery packets but no charge-specific packet ids. Existing graph profiles declare packet ids in `artifacts.packet`, and tests verify unknown `runx.payment.*` packet refs.
  - Recommendation: Decide now because packet ids are public-ish registry artifacts and will affect parser validation and future compatibility.
  - Question: Should charge profiles introduce new packet ids such as `runx.payment.charge.verify.v1`, or avoid new packet schemas in this iteration?
  - Recommended answer: Avoid new public packet schemas in v1 unless a separate scope item adds schemas and tests. Use artifact names and structured outputs in X.yaml, with packet schema work deferred.
  - If unanswered: Default to reusing existing payment packet ids only where they are semantically correct, and otherwise using profile-local artifact names without `runx.payment.charge.*` packet ids until packet schemas are explicitly scoped.
- [high/blocks approval] `HARDEN-8` design_challenge - The draft does not justify the new catalog surface against existing payment skills.
  - Status: open
  - Grounded in: spec_gap:product_goal
  - Evidence: The harden prompt explicitly asks whether the plan is a bandaid, future bloat, or right architectural move. The draft creates seven first-party charge names while existing payment skills already include quote, reserve, rail fulfillment, recover, and graph execution; it does not explain why provider-side charge requires separate skills rather than parameterized payment profiles.
  - Recommendation: Add a short product-goal/rationale section that distinguishes provider-side charge from consumer-side pay in authority direction, receipt invariant, and operator ergonomics.
  - Question: Is the separate charge skill family the right architectural move, or future catalog bloat over the existing payment execution profiles?
  - Recommended answer: It is the right move if kept profile-only: consumer-side `pay-*` spends against a challenge; provider-side `charge-*` prices, challenges, verifies credentials, and gates forwarding. The split prevents a single overloaded payment graph from hiding who holds authority at each step.
  - If unanswered: Default to keeping separate charge skills only if the spec adds a short rationale: provider-side challenge/verify/forward has different authority direction and receipts than consumer-side reserve/spend; otherwise collapse or rename before approval.

### round-2

Status: needs_revision
Started: 2026-05-20T15:40:46Z
Ended: 2026-05-20T15:40:46Z
Verdict: needs_revision
Provider: codex
Output format: codex.output_file
Summary: Most round-1 harden concerns are resolved: scope, authority reuse, runtime boundary, validation target, rollback, and product rationale are now clear. One approval blocker remains: the spec adds first-party skill directories that will require official-skill lock regeneration, while the current scope excludes `packages/*` and acceptance does not check the lock.

Checks:
- scope/migration audit
  - Grounded in: code:packages/cli/src/commands/doctor-structure.ts:45
  - Result: failed
  - Evidence: The spec declares exact skill paths and excludes core/runtime/schema/live-money work, but adding seven first-party skill dirs with `SKILL.md` and `X.yaml` necessarily affects the official-skill lock generated under `packages/cli/src`, which the spec excludes.
- path audit
  - Grounded in: code:skills/payment-execute/X.yaml:1
  - Result: passed
  - Evidence: Declared charge paths are explicit future files. Existing payment profiles confirm the same `SKILL.md` + `X.yaml` shape.
- command audit
  - Grounded in: code:tests/payment-skill-profile-validation.test.ts:1
  - Result: failed
  - Evidence: `pnpm` is installed and the acceptance test exists, but the command cannot execute in this read-only sandbox because Vite attempts to write `node_modules/.vite-temp/vitest.config...mjs`. `go run ./cmd/scafld` also cannot execute because Go cannot create a temp build directory.
- acceptance timing audit
  - Grounded in: code:tests/payment-skill-profile-validation.test.ts:90
  - Result: failed
  - Evidence: The draft requires explicit discovery/validation of all seven charge profiles in `tests/payment-skill-profile-validation.test.ts`, which addresses the current heuristic that only discovers payment-named dirs or profiles containing payment markers. It does not include an acceptance check for the official-skill lock drift that these new first-party skill dirs will cause.
- rollback/repair audit
  - Grounded in: code:.scafld/specs/drafts/payment-charge-skills-v1.md:268
  - Result: passed
  - Evidence: Build rollback is mechanical deletion of the seven new skill dirs plus reverting the validation test; runtime repair for sealed-charge/no-forward is explicitly deferred and limited to modeled metadata.
- design challenge
  - Grounded in: code:.scafld/specs/drafts/payment-charge-skills-v1.md:55
  - Result: passed
  - Evidence: The product rationale distinguishes provider-side charge from consumer-side pay by authority direction and receipt-before-forward gating, while keeping this v1 profile-only.

Issues:
- [high/blocks approval] `HARDEN-9` scope_gap - Adding seven first-party skill packages will stale the official-skill lock, but the spec excludes the required `packages/cli` lockfile update.
  - Status: open
  - Grounded in: code:packages/cli/src/commands/doctor-structure.ts:45
  - Evidence: The draft scope allows adding seven `skills/*/SKILL.md` + `X.yaml` packages and explicitly says no `packages/*` changes. However `discoverOfficialSkillsLockDoctorDiagnostics` renders `packages/cli/src/official-skills.lock.json` from every directory under `skills/` that has both files, and reports `runx.skill.lock.stale` when the checked-in lock differs. The stale-lock behavior is covered by `packages/cli/src/index.test.ts:1666`. The repair command is `node scripts/generate-official-lock.mjs`, which writes `packages/cli/src/official-skills.lock.json`.
  - Recommendation: Revise Scope And Touchpoints and Acceptance Criteria to include regenerating `packages/cli/src/official-skills.lock.json` with `node scripts/generate-official-lock.mjs`, or explicitly change the implementation shape so these profiles do not participate in official-skill discovery. As written, approval would authorize a build that makes `runx doctor` report a stale official skills lock while forbidding the required `packages/*` repair.
  - Question: Should this spec include official-skill lock regeneration for the seven new first-party charge skills, or are these charge profiles intentionally excluded from the official skills lock despite living under `skills/` with `SKILL.md` and `X.yaml`?
  - Recommended answer: Include official-skill lock regeneration in this v1. These are first-party skill packages under `skills/`, so registry tooling should see them consistently; run `node scripts/generate-official-lock.mjs` after adding the profiles and include the resulting lockfile change in scope.
  - If unanswered: Default to adding `packages/cli/src/official-skills.lock.json` to scope and acceptance, generated with `node scripts/generate-official-lock.mjs`; include dist lock syncing only if the build convention requires it for checked-in artifacts.
- [low/advisory] `HARDEN-10` command_audit - The documented local `./bin/scafld` path is missing in this checkout; source fallback is available but blocked by the read-only sandbox.
  - Status: open
  - Grounded in: code:AGENTS.md:37
  - Evidence: `./bin/scafld status payment-charge-skills-v1 --json` fails because `./bin/scafld` is absent. AGENTS.md permits the source checkout fallback `go run ./cmd/scafld`, but this read-only sandbox prevents Go from creating its temp build directory.
  - Recommendation: Use `go run ./cmd/scafld` in a writable environment for lifecycle operations, or restore `./bin/scafld` if local workflows expect it.

### round-3

Status: passed
Started: 2026-05-20T15:44:19Z
Ended: 2026-05-20T15:44:19Z
Verdict: pass
Provider: codex
Output format: codex.output_file
Summary: Round 3 resolves the prior blockers. The spec is now executable as a profile-only, first-party skill catalog task: paths are declared, authority reuse is bounded to existing payment terms, runtime claims are deferred, validation and lockfile acceptance are concrete, and rollback is mechanical. No blocking harden issues remain.

Checks:
- scope/migration audit
  - Grounded in: code:.scafld/specs/drafts/payment-charge-skills-v1.md:65
  - Result: passed
  - Evidence: Scope declares all seven non-crypto charge `SKILL.md` and `X.yaml` paths, the validation test path, and `packages/cli/src/official-skills.lock.json`; out-of-scope excludes `crates/runx-*`, CLI/runtime/contract changes, packet schemas, and live settlement.
- path audit
  - Grounded in: code:skills/payment-execute/X.yaml:1
  - Result: passed
  - Evidence: Declared charge skill files are intentional future files. Existing payment skills use the same `SKILL.md` + `X.yaml` package shape, for example `skills/payment-execute/X.yaml`.
- command audit
  - Grounded in: code:tests/payment-skill-profile-validation.test.ts:19
  - Result: passed
  - Evidence: `pnpm`, `node`, global `scafld`, `tests/payment-skill-profile-validation.test.ts`, and `scripts/generate-official-lock.mjs` are present. `pnpm exec vitest run tests/payment-skill-profile-validation.test.ts` fails here only because the read-only sandbox blocks Vite writing `node_modules/.vite-temp/...`; `node scripts/generate-official-lock.mjs` fails here only because the read-only sandbox blocks writing `packages/cli/src/official-skills.lock.json`.
- acceptance timing audit
  - Grounded in: code:tests/payment-skill-profile-validation.test.ts:90
  - Result: passed
  - Evidence: Acceptance now requires explicit discovery and validation of all seven non-crypto charge `X.yaml` files, plus lockfile refresh and a second generator run leaving the lock unchanged. This addresses the current heuristic in `discoverPaymentSkillDirs`, which otherwise discovers by payment-named dirs or payment markers.
- rollback/repair audit
  - Grounded in: code:.scafld/specs/drafts/payment-charge-skills-v1.md:279
  - Result: passed
  - Evidence: Rollback says to remove the seven charge directories, revert the validation test, and regenerate or revert the official skills lockfile. Runtime repair is explicitly out of scope and limited to modeled `reversal_required` or recovery hints.
- design challenge
  - Grounded in: code:.scafld/specs/drafts/payment-charge-skills-v1.md:55
  - Result: passed
  - Evidence: The spec distinguishes provider-side charge from consumer-side payment by pricing, challenge issuance, credential verification, and forwarding gate authority direction. Runtime boundary states graph profiles must not claim executable provider calls, forwarding, or repair.

Issues:
- [low/advisory] `HARDEN-11` command_audit - Local scafld source entrypoints named in AGENTS.md are absent; global scafld works.
  - Status: open
  - Grounded in: code:AGENTS.md:37
  - Evidence: `AGENTS.md` says to use `./bin/scafld` or `go run ./cmd/scafld` inside the scafld repo. In this checkout, `./bin/scafld` and `cmd/scafld/main.go` are absent, while `/opt/homebrew/bin/scafld status payment-charge-skills-v1 --json` works and reports the task in draft/harden state.
  - Recommendation: For lifecycle commands in this checkout, either restore the source-local scafld entrypoint expected by AGENTS.md or document that this workspace intentionally uses the installed `scafld` binary. This does not block this spec because the task status command succeeds and the build acceptance commands are project-local.

## Review

Status: completed
Verdict: fail
Mode: discover
Provider: codex
Output: codex.output_file
Summary: Failing review: the implemented profiles and lockfile are broadly in scope, but the acceptance test does not actually reject common raw merchant secret input names, so the raw-secret acceptance criterion is not satisfied. Workspace changed during review; review failed closed.

Attack log:
- `git status / review packet classification`: Workspace classification -> clean (Checked `git status --short`; only unrelated `.scafld/specs/active/payment-refund-skills-v1.md` is currently modified, so I treated the packet's stale task-change list as context and reviewed source files directly.)
- `skills/charge-price, charge-challenge, charge-verify, mock-charge, stripe-charge, mpp-charge, x402-charge`: Scope and file presence -> clean (Verified all seven non-crypto charge skill packages have `SKILL.md` and `X.yaml`, and no `skills/crypto-charge` package is present.)
- `packages/cli/src/official-skills.lock.json`: Official skill lock coverage -> clean (Inspected `packages/cli/src/official-skills.lock.json`; it includes `runx/charge-*`, `runx/mock-charge`, `runx/stripe-charge`, `runx/mpp-charge`, and `runx/x402-charge`.)
- `skills/*-charge/X.yaml and SKILL.md`: Graph runtime boundary -> clean (Read the charge graph profiles and SKILL files; graph profiles model `forward`, but each declares `runtime_forwarding_enabled: false` and docs state no live settlement or upstream forwarding.)
- `skills/*-charge/X.yaml, packages/core/src/parser/graph.ts, packages/cli/src/commands/doctor.ts`: Graph reference and nested runner shape -> clean (Compared charge graph context/artifact references against existing payment/refund graph patterns and parser/doctor expectations; no missing nested runner references found in the inspected profile structure.)
- `skills/charge-price/X.yaml, skills/charge-verify/X.yaml, charge graph runx.payment_authority metadata`: Authority model drift -> clean (Checked inline authority examples in charge profiles; they reuse `resource_family: payment` and existing payment bounds instead of adding a new charge authority schema.)
- `tests/payment-skill-profile-validation.test.ts`: Secret-field rejection -> finding (Attacked the validation helper against merchant/provider secret field names required by the spec's acceptance criteria; found it misses common raw merchant secret names.)
- `workspace mutation guard`: compare pre-review and post-review workspace snapshots -> finding (added packages/cli/src/official-skills.lock.json (M 1b3d46af51da85515d3a365a1d35afaa7f0d10e7f80934363c0ee8ba712db60d), added skills/mock-refund/X.yaml (M ab932dddc22875cd2ebb063b635b7d5aaa1da1f88dbce0e0bec51b926530ce03), added skills/mpp-refund/X.yaml (M c2a2e7906ea68a18604795d4dbc87f2bb5161511d45fb2a5d18a5c8cd6416643), added skills/stripe-refund/X.yaml (M a880d56cf9f9b2c35926ec79678a708270fbf9151b74f4ee0f739b64a9dd36a5), added skills/x402-refund/SKILL.md (M 02efda3c54594418300fc4482b02c12a938129d71c425fb07802410af347901a), added skills/x402-refund/X.yaml (M 6045e286ba954ecf1b3e5921d1cfa324eac0ed268927a91ead6d73af14b5440f))

Findings:
- [high/blocks completion] `REVIEW-1` Payment skill profile validation does not reject common raw merchant secret field names.
  - Location: `tests/payment-skill-profile-validation.test.ts:16`
  - Evidence: The spec requires settlement profiles to never declare raw merchant secrets as inputs (`.scafld/specs/active/payment-charge-skills-v1.md:263`). The only validation guard is `paymentSecretKeyPattern` at `tests/payment-skill-profile-validation.test.ts:16`, applied by `findPaymentSecretFields` at lines 139-150. That regex catches `secret_key` and `private_key`, but not `merchant_secret`, `stripe_api_key`, `client_secret`, or `access_token`. I validated the exact regex with Node: those four names return `false`, while `secret_key` and `private_key` return `true`.
  - Impact: The charge profile acceptance test can pass even if a future or edited settlement profile declares raw merchant/provider secret inputs under common names. That fails the explicit raw-merchant-secret acceptance criterion and weakens the no-hardcoded-secrets safety boundary for first-party payment skills.
  - Validation: Read `tests/payment-skill-profile-validation.test.ts` and ran a read-only Node regex check against representative field names.
- [critical/blocks completion] `workspace_mutation` Workspace changed during review.
  - Location: `packages/cli/src/official-skills.lock.json (M 1b3d46af51da85515d3a365a1d35afaa7f0d10e7f80934363c0ee8ba712db60d)`
  - Evidence: workspace changed during review: added packages/cli/src/official-skills.lock.json (M 1b3d46af51da85515d3a365a1d35afaa7f0d10e7f80934363c0ee8ba712db60d), added skills/mock-refund/X.yaml (M ab932dddc22875cd2ebb063b635b7d5aaa1da1f88dbce0e0bec51b926530ce03), added skills/mpp-refund/X.yaml (M c2a2e7906ea68a18604795d4dbc87f2bb5161511d45fb2a5d18a5c8cd6416643), added skills/stripe-refund/X.yaml (M a880d56cf9f9b2c35926ec79678a708270fbf9151b74f4ee0f739b64a9dd36a5), added skills/x402-refund/SKILL.md (M 02efda3c54594418300fc4482b02c12a938129d71c425fb07802410af347901a), added skills/x402-refund/X.yaml (M 6045e286ba954ecf1b3e5921d1cfa324eac0ed268927a91ead6d73af14b5440f)
  - Impact: The review provider changed the workspace while acting as a read-only reviewer, so its verdict is not trustworthy.
  - Validation: Restore the workspace to the expected state, ensure the provider is read-only, then rerun scafld review.
