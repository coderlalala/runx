---
spec_version: '2.0'
task_id: rust-ts-sunset-receipts
created: '2026-05-18T00:00:00Z'
updated: '2026-05-20T00:40:00Z'
status: draft
harden_status: not_run
size: medium
risk_level: high
---

# TS sunset: receipts

## Current State

Status: draft
Current phase: strict Rust receipt and tree proof acceptance landed; importer
audit guardrail landed; import and shape blockers remain
Next: migrate the remaining TS receipt importers and record the production
signature verifier decision
Reason: draft created under `plans/rust-takeover.md`. Fifth TS sunset,
refreshed after the harness receipt shape was ratified. The sunset is not yet
ready to delete TS receipt code because TS receipt importers remain live and
the runtime pseudo-signature path still needs a production verifier decision.
Blockers: `rust-ts-sunset-executor` complete,
`runx-contract-spine-hard-cutover` complete, `rust-receipts-parity` complete
against post-cutover harness receipts, production signature verifier decision
recorded, and TS receipt importer migration complete.
Allowed follow-up command: `pnpm receipts:importer-audit`; do not run `scafld
harden rust-ts-sunset-receipts`.
Latest runner update: 2026-05-20 strict store/journal/history and
parent/child tree proof acceptance landed; 2026-05-20 importer/shape audit
scanner added
Review gate: not_started

## Summary

Receipts are first-class sealed harness nodes. The live receipt contract after
the hard cutover is the canonical `runx.harness_receipt.v1` envelope, not the
retired TS `LocalSkillReceipt` or `LocalGraphReceipt` shape. A skill receipt is
a sealed leaf harness receipt. A graph receipt is a sealed parent harness
receipt that links child harness receipt refs and verifies recursive integrity
through `runx-receipts`.

This spec no longer proposes immediate deletion of `packages/core/src/receipts/`.
Deletion is unsafe until every live TS receipt importer has migrated and the
runtime pseudo-signature path has an explicit production-verifier decision. The
store, journal, history, parent/child tree proof acceptance, and importer audit
guardrail slices have landed; the next safe slice is importer migration and
production verifier policy.

No production caller may continue importing, emitting, reading, adapting, or
aliasing the old TS receipt model after the deletion phase. Verification and
public projections must come from `runx-receipts` over first-class sealed
harness nodes.

## Context

CWD: `.`

Packages:
- `@runxhq/core`
- `@runxhq/contracts`
- `crates/runx-receipts`
- `crates/runx-runtime`
- `cloud/packages/receipts-store` (must already consume post-cutover harness
  receipt contracts or be on its own sunset path before TS deletion starts)

Ratified harness receipt shape:
- `packages/contracts/src/schemas/spine.ts` defines
  `harnessReceiptEnvelopeSchema`.
- A receipt envelope has `schema: "runx.harness_receipt.v1"`, `id`,
  `created_at`, `issuer`, `signature`, `harness`, top-level `seal`, optional
  `sync_points`, and optional `metadata`.
- `signature.alg` is `Ed25519`; `signature.value` is required and must verify
  the canonical receipt body commitment during strict proof acceptance.
- `harness` is the sealed node body: `harness_id`, `parent_harness_ref`,
  `state`, `host_ref`, `harness_ref`, `authority`, `enforcement`,
  `idempotency`, `revision`, `signal_refs`, `decisions`, `acts`,
  `child_harness_receipt_refs`, `artifact_refs`, and `seal`.
- Top-level `seal` must mirror `harness.seal` for terminal harness states.
- Child links are harness receipt refs, not suffix lookups or local
  `rx_`/`gx_` file conventions.

Current Rust sources:
- `crates/runx-receipts/src/verify.rs` provides structural harness receipt
  verification.
- `crates/runx-receipts/src/verify/proof.rs` provides strict proof checks for
  body digest, signature verifier input, verification summary claims,
  authority proof presence, redaction refs, hash commitments, and external
  attestations.
- `crates/runx-receipts/src/tree.rs` exposes strict parent/child receipt tree
  proof verification APIs in addition to structural tree checks.
- `crates/runx-runtime/src/receipts.rs` constructs sealed harness receipts and
  validates them with strict proof using a runtime-local verifier.
- `crates/runx-runtime/src/receipt_tree.rs` verifies runtime receipt trees
  through strict parent/child proof acceptance.
- `crates/runx-runtime/src/receipt_store.rs` reads and lists harness receipt
  files by schema and serde shape, checks file-name/id integrity, and rejects
  proof-invalid receipts with `ReceiptProofInvalid`.
- `crates/runx-runtime/src/journal.rs` builds journal/history projections over
  `LocalReceiptStore` and derives verification from strict proof acceptance.

Current TypeScript sources:
- `packages/core/src/receipts/**` remains live.
- `packages/core/package.json` still exposes the `./receipts` subpath.
- `packages/contracts/src/schemas/local-receipt.ts` and
  `packages/contracts/src/index.ts` still expose retired local receipt
  contracts.
- Active OSS imports still reference `@runxhq/core/receipts` or
  `../receipts/index.js` in `packages/runtime-local/src/**`,
  `packages/cli/src/**`, `packages/core/src/parser/index.ts`,
  `packages/core/src/registry/ingest.ts`,
  `packages/core/src/marketplaces/fixture.ts`,
  `packages/core/src/knowledge/**`, and receipt-focused tests.
- Sibling cloud checkout audit also found active `@runxhq/core/receipts`
  imports in `../cloud/packages/api`, `../cloud/packages/receipts-store`,
  `../cloud/packages/worker`, and `../cloud/tests`.

Importer audit evidence, 2026-05-20:
- Added `scripts/check-receipt-importers.ts`, runnable through
  `pnpm receipts:importer-audit`, with deterministic classifications:
  `active_blocker`, `fixture_archive`, `generated_stale_artifact`,
  `false_positive`, and `migrated`.
- Latest audit scanned 1371 files including the sibling `../cloud` checkout
  and reported: `active_blocker=408`, `fixture_archive=11`,
  `generated_stale_artifact=16`, `migrated=274`, `false_positive=0`.
- Active blocker kinds: `retired_receipt_import=40`,
  `retired_receipt_shape=310`, `retired_receipt_type=29`,
  `runtime_pseudo_signature=21`, `retired_contract_export=4`,
  `legacy_receipt_id_prefix=3`, and `retired_core_receipts_export=1`.
- The OSS live retired-import blocker files are:
  `packages/cli/src/commands/dev.ts`, `packages/cli/src/trainable-receipts.ts`,
  `packages/runtime-local/src/sdk/index.ts`,
  `packages/runtime-local/src/sdk/trusted-host-outcome.ts`,
  `packages/runtime-local/src/runner-local/approval.ts`,
  `packages/runtime-local/src/runner-local/context.ts`,
  `packages/runtime-local/src/runner-local/graph-fanout-gates.ts`,
  `packages/runtime-local/src/runner-local/graph-hydration.ts`,
  `packages/runtime-local/src/runner-local/orchestrator/finalize.ts`,
  `packages/runtime-local/src/runner-local/orchestrator/prepare-run.ts`,
  `packages/runtime-local/src/runner-local/orchestrator/run-context.ts`,
  `packages/runtime-local/src/runner-local/reflect.ts`,
  `packages/runtime-local/src/runner-local/runner-helpers.ts`,
  `packages/runtime-local/src/runner-local/skill-install.ts`,
  `packages/core/src/knowledge/local-store.ts`,
  `packages/core/src/marketplaces/fixture.ts`,
  `packages/core/src/parser/index.ts`,
  `packages/core/src/registry/ingest.ts`, and focused tests that still exercise
  retired receipt APIs.
- Prefix-based legacy graph/receipt inference remains in
  `packages/runtime-local/src/runner-local/history.ts` at the `gx_`
  `startsWith` fallback sites.

Receipt model:
- Receipt nodes are first-class sealed harness receipts with the canonical
  post-cutover schema from `runx-contract-spine-hard-cutover`.
- A skill receipt is a sealed leaf harness: no child harness receipt refs, one
  governed skill execution boundary, contained acts/decisions/proofs, and a
  proof-verifiable body/full digest.
- A graph receipt is a sealed parent harness receipt: it links child harness
  receipt refs, binds child digest/proof expectations, and verifies recursive
  integrity through `rust-receipt-tree-resolution`.
- Public receipt references point at harness receipts and contained act or
  decision ids. They do not point at retired `rx_`/`gx_` local receipt files or
  suffix-based child lookup.

## Remaining Drift

- Runtime pseudo-signature: `crates/runx-runtime/src/receipts.rs` seals receipts
  as `sig:{digest}` with `LocalHarnessSignatureVerifier` and issuer
  `runtime-skeleton`. This proves the strict proof path is wired, but it is not
  an Ed25519-backed production signature acceptance model.
- The importer audit guard also detects pseudo-signature references in
  `crates/runx-runtime/src/receipts.rs`, `receipt_store.rs`, `receipt_tree.rs`,
  `journal.rs`, and `harness/assertions.rs`; these remain active blockers until
  the production verifier policy is recorded.
- TS receipt imports still live:
  active source still imports TS receipt helpers, types, hash utilities, and
  local receipt contracts from `packages/core/src/receipts/**` or the
  `@runxhq/core/receipts` subpath.

## Invariants

- Live governed paths use only post-cutover harness receipts once this sunset
  reaches deletion.
- TS receipt deletion must not proceed unless Rust runtime parent/child graph
  paths continue to reject receipt trees that fail strict proof acceptance.
- Existing pre-cutover receipts on disk are either migrated, archived, or read
  through an explicitly offline archival verifier. They are not accepted by
  live runtime, CLI, cloud, or receipt-store paths after this sunset.
- TypeScript receipt deletion cannot proceed while any live caller depends on
  old receipt field names, old digest semantics, suffix child lookup,
  repo-local receipt path discovery, `LocalSkillReceipt`, or
  `LocalGraphReceipt`.
- The Rust receipt stack owns canonical serialization, body/full digest
  computation, proof verification, redaction checks, parent/child tree
  resolution, and safe path discovery.
- Fixture history must preserve the reviewer/security oracle without keeping
  retired live reader behavior. Old fixtures can be archived as historical
  artifacts only if they are excluded from active parity and runtime
  acceptance.
- The final fixture catalogue covers at least denied approval, approval
  round-trip, leaf skill receipt, graph fanout parent receipt, replay, child
  harness, verification form, abnormal seal, digest tamper, signature tamper,
  redaction mismatch, missing child, malformed child ref, and external
  proof-missing cases.

## Sequencing

- `runx-contract-spine-hard-cutover` is the source of truth for the hard
  cutover receipt shape: sealed harness receipts, contained acts, contained
  decisions, authority, signals, refs, fingerprints, redaction refs, and proof
  bindings.
- `rust-receipts-parity` must target that post-cutover harness receipt shape.
  It must not use pre-cutover `LocalSkillReceipt` or `LocalGraphReceipt` bytes
  as the parity target unless a prior phase explicitly names an offline
  archival migration fixture and keeps it outside live acceptance.
- Byte-identical means byte-identical within one fixed post-cutover receipt
  shape: canonical harness receipt JSON, stable key order, normalized fixture
  ids/timestamps where applicable, deterministic child ordering where order is
  semantic, and exact hash inputs shared with `runx-receipts`.
- Byte-identical does not mean retired TS local receipt bytes equal Rust
  harness receipt bytes across the hard cutover. The cutover intentionally
  changes the contract shape.
- The next slice is importer audit plus production verifier policy. It is not
  broad deletion, not compatibility scaffolding, and not a revival of retired
  TS local receipt bytes.
- TS deletion starts only after every live producer and consumer has already
  moved to the post-cutover harness receipt contract and Rust persistence
  paths fail closed on proof failures. If any live caller still needs retired
  TS receipts, this spec pauses and that caller gets a migration or
  archival-read spec first.

## No-Legacy Rule

- Do not keep compatibility readers, write shims, field aliases, export
  aliases, deprecated wrappers, or dual emission for
  `LocalSkillReceipt`/`LocalGraphReceipt`.
- Do not accept old receipt field names such as `skill_execution`,
  `graph_execution`, `skill_name`, `graph_name`, `source_type`, `owner`,
  `childReceipts`, or repo-local path discovery fields in live receipt
  readers. Canonical harness receipt metadata may carry user-facing labels, but
  verifier acceptance must not depend on retired local receipt field names.
- Do not add `v2` schema ids, `runx.*.v2` compatibility contracts, or
  version-switched readers to bridge old and new receipt models. The hard
  cutover uses the canonical harness receipt contract and removes superseded
  names instead of aliasing them.
- Do not preserve public exports whose only purpose is to keep old TS import
  paths compiling. Callers must import the post-cutover contract or
  `runx-receipts` verifier APIs directly.

## Objectives

- Keep Rust receipt store read/list acceptance strict on proof, with fail-closed
  errors for invalid body digest, signature, redaction, hash commitment,
  external attestation, or authority summary claims.
- Keep journal/history projections derived from strict proof acceptance, not
  structural validation alone.
- Record the runtime pseudo-signature as a test/development bridge and block
  production acceptance on a real Ed25519-backed verifier or injected verifier
  policy.
- Keep parent/child tree proof acceptance as a non-regression gate for runtime
  graph projections.
- Enumerate every importer of `packages/core/src/receipts/**` and retire or
  migrate it before deletion.
- Prove live receipt producers emit sealed harness receipts for both leaf
  skills and parent graphs.
- Preserve fixture coverage for security/reviewer behavior without making old
  TS receipt shapes a live compatibility target.
- Delete the TS receipts implementation and remove the `@runxhq/core` export
  surface for retired receipt APIs only after strict Rust acceptance and import
  migration are complete.

## Scope

In scope for the next implementation slice:
- Auditing active TS receipt importers and classifying each as migrated,
  deletion blocker, archived fixture, generated stale artifact, or false
  positive.
- Documenting production signature verifier requirements and preventing the
  runtime pseudo-signature from being treated as the final acceptance model.

In scope after strict Rust acceptance lands:
- Deleting the TS receipts implementation from `@runxhq/core`.
- Removing retired receipt exports and package subpaths from
  `packages/core/src/index.ts` and `packages/core/package.json`.
- Removing retired local receipt contracts from the live `@runxhq/contracts`
  export surface after all imports have migrated.
- Updating tests/fixtures that still assert retired TS receipt fields so active
  assertions target sealed harness receipts.
- Adding static guards that prevent reintroducing live imports or aliases for
  retired receipt types.

Out of scope:
- Cloud receipts-store internal changes. Cloud must already be migrated or
  tracked by its own blocking sunset spec.
- Changing the canonical harness receipt contract.
- Adding an archival verifier for pre-cutover receipts. If required, that is a
  separate offline-only spec with explicit non-live routing.
- Renaming public harness receipt schemas or introducing a second receipt
  version.
- Running `scafld harden` as part of this refresh.

## Dependencies

- `rust-ts-sunset-executor` complete.
- `runx-contract-spine-hard-cutover` complete and reviewed; canonical harness
  receipt contract is frozen for this sunset.
- `rust-harness` complete enough to supply canonical post-cutover harness
  receipt replay fixtures for leaf skills and graph parents.
- `rust-receipts-parity` complete against post-cutover harness receipts, not
  retired TS local receipt bytes.
- `rust-receipt-proof-verification` complete for body/full digest checks,
  signature/proof verification, tamper failures, abnormal seals, redaction
  mismatches, and external proof-missing cases.
- `rust-receipt-tree-resolution` complete for parent graph harness receipts,
  child harness receipt refs, missing child failures, malformed refs, and
  recursive digest/proof recomputation.
- `rust-runtime-receipt-path-discovery` complete for runtime-owned receipt
  discovery and public projection paths, with no repo-local legacy lookup.
- `rust-approval-gate-parity` complete for approval request/decision receipt
  round-trip evidence.
- Cloud receipts-store migration or sunset decision recorded before TS
  deletion: either it consumes harness receipt contracts, or a blocking cloud
  spec owns the remaining retired receipt dependency.

## Planned Phases

Phase 1: strict Rust store/journal/history proof acceptance.
Status: completed 2026-05-20.
- `LocalReceiptStore` read/list/index paths reject proof-invalid receipts.
- Journal and history projections derive verification from strict proof
  acceptance, not `verify_harness_receipt` alone.
- Negative tests cover structural pass/proof fail cases including tampered body
  digest and signature paths.
- Remaining caveat: runtime pseudo-signature is still deterministic local
  verifier scaffolding, not production Ed25519 acceptance.

Phase 2: parent/child proof and fixture handoff.
Status: completed 2026-05-20 for strict runtime tree proof acceptance; fixture
catalogue handoff remains in later phases.
- Confirm leaf skill and graph parent harness receipt fixtures exist and
  validate through strict `runx-receipts` proof paths.
- Confirm approval round-trip, graph fanout, replay, child harness, abnormal
  seal, digest tamper, signature tamper, redaction mismatch, malformed child
  ref, and external proof-missing cases are covered by Rust validation.
- Runtime graph parent receipt verification now uses strict proof acceptance
  where graph parent receipts are accepted as verified.
- Move or rewrite any active TS fixture expectations that still assert retired
  receipt fields.

Phase 3: importer and shape audit.
Status: guardrail landed 2026-05-20; blockers remain.
- Search active source, tests, fixtures, scripts, generated surfaces, and cloud
  touchpoints for `packages/core/src/receipts`, `@runxhq/core/receipts`,
  `LocalSkillReceipt`, `LocalGraphReceipt`, retired receipt field names,
  `rx_`/`gx_` live lookup, and suffix child lookup.
- Classify each hit as migrated, active blocker, fixture archive, generated
  stale artifact, or false positive.
- Record the cloud receipts-store state and block deletion if any live cloud
  path still requires retired receipt types.
- Keep `pnpm receipts:importer-audit` passing in audit mode; deletion-phase
  work may use `tsx scripts/check-receipt-importers.ts
  --fail-on-active-blockers` once all blockers are expected to be gone.

Phase 4: delete TS receipt implementation.
- Delete `packages/core/src/receipts/**`.
- Remove retired exports from `packages/core/src/index.ts`,
  `packages/core/package.json`, and live `@runxhq/contracts` export surfaces
  after import migration is complete.
- Delete or update tests that exercised only retired TS receipt implementation
  details.
- Keep active tests focused on public harness receipt behavior and Rust
  verifier acceptance.

Phase 5: no-legacy guardrails.
- Add or extend static checks so live source cannot import retired receipt
  paths, reference retired receipt type names, or introduce `runx.*.v2`
  receipt schema ids.
- Ensure fixtures under active runtime/replay paths reject retired receipt
  fields instead of silently ignoring them.
- Verify generated package surfaces no longer expose retired receipt names.

Phase 6: final validation and review evidence.
- Run the full validation command set below.
- Attach strict proof acceptance evidence, importer audit evidence, fixture
  catalogue evidence, and no-legacy scan output to this spec's completion
  notes.
- Confirm all changed files are limited to this sunset's implementation scope.

## Acceptance Criteria

- Rust receipt store read/list paths reject structurally valid but
  proof-invalid harness receipts unless the caller explicitly chooses an
  offline/non-live archival mode outside this sunset.
- Journal/history `verification.status` is derived from strict receipt proof
  acceptance. Structural JSON validation alone cannot produce `verified`.
- Parent graph verification does not claim strict acceptance without resolving
  child harness receipt refs and checking child receipt proof requirements.
- The runtime pseudo-signature is either removed from production paths or
  explicitly gated as a deterministic fixture/test verifier.
- No active source imports `packages/core/src/receipts/**` or imports retired
  receipt names from `@runxhq/core` after the deletion phase.
- `packages/core/src/receipts/` is deleted only after strict Rust proof
  acceptance is in place and active TS imports are gone.
- Live skill execution emits and verifies a sealed leaf harness receipt.
- Live graph execution emits and verifies a sealed parent harness receipt that
  links child harness receipt refs and fails closed on missing, malformed, or
  digest/proof-mismatched children.
- Receipt equality tests compare canonical post-cutover harness receipt bytes
  within the fixed harness receipt shape. They do not compare Rust output to
  retired TS `LocalSkillReceipt` or `LocalGraphReceipt` bytes.
- `runx-receipts` proof verification, not structural JSON comparison alone,
  validates body digest, full digest, signatures/proofs, abnormal seals,
  redaction refs, and external proof requirements.
- Active fixtures contain no retired receipt expectation fields under receipt
  assertions.
- Pre-cutover receipt fixtures, if retained, live only in an explicitly named
  archive path and are not loaded by live runtime, CLI, cloud, or Rust parity
  validation.
- No compatibility readers, aliases, deprecated exports, dual-emission paths,
  or `v2` receipt schema ids are introduced.
- Cloud receipts-store has no live dependency on old TS receipt field names,
  old digest semantics, suffix child lookup, or repo-local path discovery
  before deletion lands.

## Validation Commands

Strict Rust proof acceptance slice:

```sh
pnpm receipts:importer-audit
cargo test --manifest-path crates/Cargo.toml -p runx-receipts
cargo test --manifest-path crates/Cargo.toml -p runx-runtime receipt_store
cargo test --manifest-path crates/Cargo.toml -p runx-runtime journal_history
cargo test --manifest-path crates/Cargo.toml -p runx-runtime --test receipt_tree
rg -n "verify_harness_receipt\\(" crates/runx-runtime/src/journal.rs crates/runx-runtime/src/receipt_store.rs && exit 1 || exit 0
rg -n "sig:\\{digest\\}|sig:pending|runtime-skeleton" crates/runx-runtime/src/receipts.rs
```

Deletion-phase validation after strict acceptance and import migration:

```sh
pnpm exec tsx scripts/check-receipt-importers.ts --fail-on-active-blockers
test ! -d packages/core/src/receipts
SCAN_ROOTS="apps bindings crates packages fixtures schemas scripts tests tools"
! rg -n "packages/core/src/receipts|@runxhq/core/receipts|LocalSkillReceipt|LocalGraphReceipt" $SCAN_ROOTS
! rg -n "skill_execution|graph_execution|skill_name|graph_name|source_type|childReceipts|receiptPath|receipt_path" $SCAN_ROOTS
! rg -n "runx\\.[a-z0-9_.-]+\\.v2|receipt.*compat|compat.*receipt|legacy.*receipt|alias.*receipt" $SCAN_ROOTS
rg -n "runx\\.harness_receipt\\.v1|HarnessReceipt|harness receipt" $SCAN_ROOTS
cargo test --manifest-path crates/Cargo.toml -p runx-receipts
cargo test --manifest-path crates/Cargo.toml -p runx-runtime receipt
cargo test --manifest-path crates/Cargo.toml -p runx-runtime harness
pnpm test -- packages/core
pnpm test -- packages/runtime-local/src/harness
pnpm exec tsx scripts/generate-rust-harness-fixtures.ts --check
pnpm boundary:check
pnpm rust:check
```

Cloud-specific validation is required before completion if the sibling cloud
checkout exists:

```sh
test ! -d ../cloud || ! rg -n "LocalSkillReceipt|LocalGraphReceipt|skill_execution|graph_execution|source_type|childReceipts|receipt_path|receiptPath" ../cloud
test ! -d ../cloud || pnpm --dir ../cloud test -- receipts harness
```

## Rollback And Repair

- Before Phase 1, rollback is to keep current structural Rust read/projection
  behavior and continue blocking TS deletion.
- After Phase 1, rollback is a normal revert of the strict proof acceptance
  slice only if a live verifier cannot be supplied. Do not mark structural-only
  receipts as verified to restore old behavior.
- Before Phase 4, rollback is to keep the TS receipt implementation in place
  and continue blocking this sunset on the importer or fixture that still
  requires it.
- After Phase 4, rollback is a normal revert of this sunset's deletion commit
  only if a live importer was missed. Do not add compatibility aliases or dual
  readers as rollback.
- If a fixture was upgraded incorrectly, repair the canonical harness receipt
  fixture and Rust verifier expectation together. Do not restore retired
  receipt fields to active fixtures.
- If cloud still needs retired receipt types, pause this spec and complete the
  cloud migration/sunset first.
- If byte comparison fails because a fixture crosses the hard cutover, remove
  that comparison from live acceptance or explicitly sequence it as an
  offline archival migration fixture. Do not redefine byte-identical to span
  two receipt contract shapes.

## Open Questions

- What production verifier should replace the runtime pseudo-signature path:
  local Ed25519 key material, hosted verifier injection, or a runtime verifier
  trait supplied by the caller?
- Whether the structural-only tree APIs should remain public after importer
  deletion, or become explicitly archival/debug-only.
- Whether cloud receipts-store gets its own Rust port before this sunset.
  If yes, that is an additional cloud spec; if no, the cloud-side must keep a
  contract-typed post-cutover harness receipt view via `runx-contracts`.
- Whether an offline archival verifier for pre-cutover local receipts is needed
  for customer support. Default: no live compatibility path; archival support,
  if required, is separate and cannot block deletion once live callers are
  migrated.
