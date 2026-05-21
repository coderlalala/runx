---
spec_version: '2.0'
task_id: payment-charge-packet-schema-doctor-v1
created: '2026-05-21T00:46:25Z'
updated: '2026-05-21T02:27:44Z'
status: completed
harden_status: passed
size: medium
risk_level: medium
---

# Charge graph packet metadata doctor cleanup v1

## Current State

Status: completed
Current phase: final
Next: done
Reason: task completed
Blockers: none
Allowed follow-up command: `none`
Latest runner update: 2026-05-21T02:27:44Z
Review gate: pass

## Summary

Resolve the 16 `runx.graph.context.schema_missing` warnings emitted by the
dogfood doctor run for `mock-charge`, `mpp-charge`, and `stripe-charge`. The
repair must make charge graph step outputs legible to the
doctor without adding runtime charge behavior, live settlement, or new payment
packet ids unless matching schemas and tests are in scope.

## Scope And Touchpoints

In scope:

- `skills/charge-price/X.yaml`
- `skills/charge-challenge/X.yaml`
- `skills/charge-verify/X.yaml`
- `skills/mock-charge/X.yaml`
- `skills/mpp-charge/X.yaml`
- `skills/stripe-charge/X.yaml`
- no public x402-specific charge skill directory, registry lock entry, or
  runtime alias
- `dist/packets/payment.charge-price.v1.schema.json`
- `dist/packets/payment.charge-challenge.v1.schema.json`
- `dist/packets/payment.charge-verification.v1.schema.json`
- `dist/packets/payment.charge-seal.v1.schema.json`
- `packages/cli/src/official-skills.lock.json`
- `tests/payment-skill-profile-validation.test.ts`
- `scripts/dogfood-core-skills.mjs` only if the dogfood assertion needs to
  check warning ids more precisely

Out of scope:

- Rust contracts/runtime changes.
- Stripe live mode or any real rail mutation.
- New `charge` resource-family contracts. This task may add `runx.payment.*`
  packet schemas for the existing payment charge payloads when no existing
  packet accurately models the emitted fields.
- Provider forwarding runtime behavior.

## Planned Phases

Phase 1: model current warning surface.
: Capture the exact `runx.graph.context.schema_missing` warnings for the four
charge graphs and identify whether each is best fixed by graph artifact
metadata, existing packet ids, or a new packet schema.

Phase 2: add charge graph packet metadata.
: Update the charge graph profiles so context references such as
`price.charge_price_packet.data`, `challenge.charge_challenge_packet.data`,
`verify.charge_verification_packet.data`, and `seal.charge_seal.data` have
doctor-verifiable producer metadata. Nested charge skill outputs own the
`charge_price_packet`, `charge_challenge_packet`, and
`charge_verification_packet` metadata; each charge graph owns its local
`charge_seal` metadata.

Phase 2a: add exact payment charge packet schemas.
: Add narrow `runx.payment.charge_price.v1`,
`runx.payment.charge_challenge.v1`, `runx.payment.charge_verification.v1`,
and `runx.payment.charge_seal.v1` schemas rather than reusing semantically
different consumer-side payment packets.

Phase 3: validate and dogfood.
: Re-run profile validation, doctor, and the core dogfood script.

## Acceptance

Profile: strict

Validation:
- [x] `v1` test - Payment profile validation passes.
  - Command: `pnpm exec vitest run tests/payment-skill-profile-validation.test.ts`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-10
- [x] `v2` test - Doctor has no charge graph context schema warnings.
  - Command: `pnpm exec tsx packages/cli/src/index.ts doctor --json > /tmp/runx-doctor-payment-charge.json || true; node -e 'const fs=require("node:fs"); const report=JSON.parse(fs.readFileSync("/tmp/runx-doctor-payment-charge.json","utf8")); const bad=(report.diagnostics||[]).filter((d)=>String(d.id||"").startsWith("runx.graph.context.") && [d.target?.ref,d.location?.path,d.message].some((value)=>String(value||"").includes("charge"))); if (bad.length) { console.error(JSON.stringify(bad,null,2)); process.exit(1); } console.log("charge graph context diagnostics: 0");'`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-11
- [x] `v3` dogfood - Core dogfood remains green.
  - Command: `node scripts/dogfood-core-skills.mjs`
  - Expected kind: `exit_code_zero`
  - Status: pass
  - Evidence: exit code was 0
  - Source event: entry-12

## Rollback

Strategy: per_phase

Commands:
- `git checkout HEAD -- skills/charge-price/X.yaml skills/charge-challenge/X.yaml skills/charge-verify/X.yaml skills/mock-charge/X.yaml skills/mpp-charge/X.yaml skills/stripe-charge/X.yaml packages/cli/src/official-skills.lock.json tests/payment-skill-profile-validation.test.ts scripts/dogfood-core-skills.mjs`
- `rm -f dist/packets/payment.charge-price.v1.schema.json dist/packets/payment.charge-challenge.v1.schema.json dist/packets/payment.charge-verification.v1.schema.json dist/packets/payment.charge-seal.v1.schema.json`

## Harden Rounds

### round-1

Status: passed
Started: 2026-05-21T01:30:54Z
Ended: 2026-05-21T01:33:32Z

Checks:
- path audit
  - Grounded in: code:packages/cli/src/commands/doctor.ts:547
  - Result: passed
  - Evidence: Scope now includes the nested charge skill profiles that doctor
- command audit
  - Grounded in: code:tests/payment-skill-profile-validation.test.ts:67
  - Result: passed
  - Evidence: Acceptance includes payment profile validation, doctor JSON
- scope/migration audit
  - Grounded in: code:dist/packets/payment.quote.v1.schema.json:5
  - Result: passed
  - Evidence: The spec rejects false reuse of consumer-side payment packets and
- acceptance timing audit
  - Grounded in: code:packages/cli/src/commands/doctor.ts:432
  - Result: passed
  - Evidence: Packet metadata must exist before doctor validates graph context
- rollback/repair audit
  - Grounded in: spec_gap:rollback
  - Result: passed
  - Evidence: Rollback now covers nested skill profiles, the four charge graph
- design challenge
  - Grounded in: code:packages/cli/src/commands/doctor.ts:638
  - Result: passed
  - Evidence: The lowest-effort fix would only silence `schema_missing` by
- Should charge graph metadata reuse existing consumer-side payment packets or add exact provider-charge packet schemas?
  - Grounded in: code:dist/packets/payment.quote.v1.schema.json:5
  - Result: passed
  - Evidence: Existing consumer payment packet schemas do not expose the exact
- Where should packet metadata be declared for graph steps that call nested charge skills?
  - Grounded in: code:packages/cli/src/commands/doctor.ts:547
  - Result: passed
  - Evidence: Doctor loads nested skill profiles for `skill:` graph steps and
- Who owns the `charge_seal` packet metadata?
  - Grounded in: code:packages/cli/src/commands/doctor.ts:558
  - Result: passed
  - Evidence: Local `run:` graph steps do not have nested skill profiles, so

Issues:
- none


## Planning Log

- 2026-05-21T00:46:25Z: Filed after `x402-pay-dogfood-v1` completed with
  doctor success but 16 charge graph context-schema warnings.

## Review

Status: completed
Verdict: pass
Mode: discover
Provider: claude:claude-opus-4-7
Output: claude.mcp_submit_review
Summary: In-scope deliverable is correct: the four charge graphs (`mock-`, `mpp-`, `stripe-`, `x402 charge flow`) and their nested skills (`charge-price`, `charge-challenge`, `charge-verify`) now declare `artifacts.wrap_as` + `artifacts.packet` metadata pointing to four new `dist/packets/payment.charge-*.v1.schema.json` schemas whose `properties` cover every field referenced by the graph `context:` blocks (`requested_payment_authority`, `idempotency`, `settlement_proof`, `receipt_ref`). This addresses all four warning patterns per graph × four graphs = the 16 `runx.graph.context.schema_missing` diagnostics. Lock file regenerates cleanly and the in-scope profile-validation test enforces the new metadata. Two non-blocking observations: (1) several Rust crate files were modified during the task even though the spec explicitly lists "Rust contracts/runtime changes" as out of scope; (2) `tests/x402-pay-dogfood-mock.test.ts` was added even though it is not in the declared touchpoints. Acceptance criteria (v1/v2/v3) are recorded as passed and the construction of the doctor JSON filter is tight enough to catch regressions of the original warnings.

Attack log:
- `dist/packets/payment.charge-*.v1.schema.json`: Verify each new schema declares x-runx-packet-id and covers every property referenced by graph context paths -> clean (charge-price.requested_payment_authority, charge-challenge.idempotency, charge-verification.settlement_proof, charge-seal.{receipt_ref,sealed} all present as schema properties.)
- `skills/charge-*/X.yaml nested-skill artifacts`: Trace doctor's loadStepOutputDeclarations -> outputDeclarationsFromArtifacts to confirm wrap_as -> packet resolution falls through artifactMetadata.packet when raw.outputs has no matching key -> clean (outputs.charge_price (and siblings) are scalar declarations, so outputDeclarationsFromArtifacts at packages/cli/src/commands/doctor.ts:574-585 correctly reads artifactMetadata.packet.)
- `skills/{mock,mpp,stripe,x402}-charge/X.yaml seal step`: Confirm local run-step artifacts metadata resolves to charge_seal packet without requiring a nested profile lookup -> clean (doctor.ts:558-564 handles step.artifacts for run-steps; payload-shape path validation accepts seal.charge_seal.data.{receipt_ref,sealed}.)
- `doctor.ts validateGraphContextReferences warning count`: Recompute the original 16 warning surface: 4 schema_missing keys per graph (price/challenge/verify/seal) × 4 graphs = 16, and confirm new metadata removes them -> clean (warnedMissingSchema dedupes per producer.emitName; every producer now has packet metadata so every key is short-circuited at doctor.ts:432.)
- `tests/payment-skill-profile-validation.test.ts expectedChargePacketMetadata + chargeGraphSkillNames`: Verify the test now asserts the new packet ids exist for every charge skill and that loadDeclaredPacketIds reads dist/packets payment schemas -> clean (expectedChargePacketMetadata covers all three nested skills; chargeGraphSkillNames covers all four charge graphs; loadDeclaredPacketIds globs dist/packets/payment.*.schema.json so the new schemas are picked up.)
- `packages/cli/src/official-skills.lock.json`: Confirm regenerated digests are unique and that all seven charge skills appear -> clean (scripts/generate-official-lock.mjs is fully deterministic on SKILL.md + X.yaml content; entries 3,8,13,78,93,213,238 cover the 7 charge skills.)
- `policy.transitions field references`: Check that mock/mpp/stripe/x402 graph transition fields (seal.charge_seal.data.sealed) validate via tests/payment-skill-profile-validation.test.ts validateGraphFieldReference -> clean (packetDataShape 'payload' + non-approval packet path => returns undefined; doctor does not validate transitions but the test does.)
- `Scope vs workspace task_changes`: Diff declared spec scope against task_changes manifest -> finding (Crates/ Rust files and tests/x402-pay-dogfood-mock.test.ts are not in declared touchpoints; recorded as two non-blocking scope-drift findings.)
- `Acceptance command v2 doctor JSON filter`: Validate the regression filter actually catches a recurrence of runx.graph.context.schema_missing for charge graphs -> clean (Filter at spec line 99 keys on id prefix runx.graph.context. and string 'charge' across target.ref/location.path/message; the warning at doctor.ts:438-444 carries all three so future regressions trip exit 1.)

Findings:
- [medium/non-blocking] `scope-drift-rust-crates` Rust crate files modified during task period despite spec marking 'Rust contracts/runtime changes' as out of scope
  - Location: `crates/runx-core/api-snapshot.txt`
  - Evidence: Task changes since approval baseline include: crates/runx-core/api-snapshot.txt (added), crates/runx-core/src/policy.rs (M->M new hash), crates/runx-core/src/policy/payment_authority.rs (M->M new hash), crates/runx-runtime/src/lib.rs (M->M new hash), crates/runx-runtime/tests/payment_authority.rs (M->D deleted). The spec at .scafld/specs/active/payment-charge-packet-schema-doctor-v1.md lines 55-60 explicitly lists 'Rust contracts/runtime changes' under Out of scope.
  - Impact: Workspace state mixes this task's deliverable with unrelated Rust authority changes. A reviewer attributing the deleted test or api-snapshot churn to this spec would be misled. If the Rust changes are from concurrent work, they should land under their own spec; if they were intentional here, the spec scope should have been amended via harden.
  - Validation: git diff HEAD -- crates/ should be empty after this task or covered by a separate approved spec.
- [low/non-blocking] `scope-drift-x402-mock-test` tests/x402-pay-dogfood-mock.test.ts added during task is not in the declared touchpoint list
  - Location: `tests/x402-pay-dogfood-mock.test.ts`
  - Evidence: Task changes list 'added tests/x402-pay-dogfood-mock.test.ts (?? 94c878a6...)' as untracked-new. The spec's Scope And Touchpoints (lines 35-51) does not include this file; the only test file in scope is tests/payment-skill-profile-validation.test.ts. The file is, however, referenced by scripts/dogfood-core-skills.mjs (line 37), which is conditionally in scope.
  - Impact: Either the dogfood acceptance command depends on a test file that this task quietly created (in which case the spec scope should have called it out alongside the dogfood script), or the file belongs to another active spec (x402-pay-phase1-mock-scenario-fixtures-v1.md references it) and was pulled in here by accident.
