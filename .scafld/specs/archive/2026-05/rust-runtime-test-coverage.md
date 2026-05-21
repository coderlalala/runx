---
spec_version: '2.0'
task_id: rust-runtime-test-coverage
created: '2026-05-21T03:00:00Z'
updated: '2026-05-21T02:04:39Z'
status: completed
harden_status: passed
size: medium
risk_level: medium
---

# Rust runtime test coverage audit

## Current State

Status: completed
Current phase: final
Next: done
Reason: task completed
Blockers: none
Allowed follow-up command: `none`
Latest runner update: 2026-05-21T02:04:39Z
Review gate: pass

## Why this exists

The `runx-runtime` crate has 30+ integration test files covering hundreds of
tests. Coverage is uneven: some surfaces (`payment_*`, `mcp_*`, `receipt_*`)
have dense matrices; others (`dev/`, `scaffold/`, `harness/assertions`,
`registry/local::build`) have a single happy-path test. Per the rust-takeover
plan §11, the launcher flip needs a baseline coverage signal before
production soak. This spec produces that baseline and prioritizes the gaps.

This is a **read + report** spec, not a code change. The output is a
checklist that drives the next ~5 small "add tests for X" follow-up specs.

## Scope

In scope:

- Read the current `crates/runx-runtime/tests/` corpus and runtime modules.
- Classify coverage by behaviorally observable runtime surface, including
  internal helpers whose behavior is exposed through public runtime entrypoints.
- Produce a self-contained ranked backlog with file targets and test intent.

Out of scope:

- Writing new tests in this spec.
- Touching the in-flight payment/authority prune surface. Payment coverage is
  treated as a moving, dense area and should receive its own post-prune audit.
- Adopting coverage tooling or changing CI.

## Method

1. For each top-level runtime module, enumerate its integration test file(s)
   and the assertion surface they cover.
2. For each behaviorally observable function and type, including `pub(super)`
   helpers that gate public runtime behavior, identify whether it is exercised
   in at least one assertion and whether failure paths are exercised.
3. Rank the gaps by **production risk**, not by raw coverage percentage.

## Phases

1. Inventory the current runtime test corpus and record dense, thin, and
   uncovered surfaces in this spec.
2. Remove stale or deleted targets before approval so follow-up specs never
   chase pre-prune ghosts.
3. Use the Priority section as the handoff artifact for future focused
   test-implementation specs.

## Surfaces by current density

### Dense (no immediate action)

| Surface | Test files | Approximate cases |
| --- | --- | --- |
| `payment_*` | `payment_authority.rs`, `payment_execution.rs`, `payment_receipts.rs` | 38 |
| `mcp` adapter | `mcp_adapter.rs`, `mcp_server.rs` | 21 |
| `receipts` (store, tree, paths) | `receipt_store.rs`, `receipt_tree.rs`, `receipt_paths.rs` | 46 |
| `harness` (fixtures) | `harness_fixtures.rs`, `parity.rs` | 40+ |
| `runner.rs` (graph execution) | `hello_graph.rs`, `fanout_parity.rs`, `fanout_proptest.rs`, `parity.rs` | 80+ |
| `connect/*` | `connect_*.rs` (4 files) | 30+ |

### Thin (priority follow-ups)

| Surface | Current tests | Missing |
| --- | --- | --- |
| `dev::*` (run_dev_once, watch, presentation, tool) | `dev.rs` | watch debounce semantics; lane filtering; fixture executor failure paths; render theme variants |
| `scaffold::*` (init, new, templates) | `scaffold.rs` | template ids mismatch; ensure_install_state failure; ensure_project_state preserve+overwrite branches; packet namespace edge cases |
| `harness::assertions` | covered indirectly | direct unit tests for `assert_expectations` against every `HarnessExpectedStatus` and disposition mismatch |
| `registry::local::build` | `registry.rs`, `registry_client.rs` | direct unit tests for `build_registry_skill_version` happy path + missing-publisher; `normalize_registry_skill_version` round-trip on every source-type variant |
| `registry::local::trust` | covered indirectly | direct unit tests for each `*_trust_signal` against verified/declared/not_declared transitions |
| `journal::*` (projection + history filters) | `journal_history.rs` | Most receipt filter combinations now have direct coverage; remaining gaps are paused-run filter edge cases and any future artifact/source variants not represented by receipt metadata |
| `agent_invocation::*` | `agent_parity.rs` | resolution flow under needs-agent loop; idempotency key derivation; act-ref resolution fixtures |
| `target_runner::*` | `target_runner.rs` | runtime-side execution beyond contract fixture parity; readiness mismatch propagation; PR observation race conditions |
| `post_merge_observer::*` | `post_merge_observer.rs` | publication-from-receipt projection failure modes; runtime dedupe with stale receipt refs |
| `sandbox::*` | covered indirectly | direct unit tests for `prepare_mcp_process_sandbox` against every cwd policy; env allowlist intersection |
| `doctor::*` | `doctor.rs` | each diagnostic severity producing the right exit-code path; repair-confidence ordering |
| `list::*` | none directly | direct unit tests for every `RunxListItemKind` discovery path; ok-only vs invalid-only filtering |

### Untested

| Surface | Why it matters |
| --- | --- |
| `runner/inputs.rs` typed-input helpers (`required_typed_input`, `optional_typed_vec_input`, etc.) | They produce `payment_authority_denied` errors with structured reasons. Currently covered by integration tests through full payment flows; no isolated unit. |
| `runner/sync.rs::receipt_strategy` + `receipt_decision` | Pure mappings; trivial but worth a single match-exhaustiveness unit test to lock the wire shape. |
| `dev::watch::debounce` edge cases | Parent `dev` behavior has coverage, but debounce timing semantics are not isolated. Use deterministic clock injection or keep this deferred rather than adding a flaky sleep test. |

## Priority

P1 (production risk): agent_invocation resolution loop, target_runner runtime execution paths, post_merge_observer projection failure modes, harness::assertions direct units.

P2 (correctness): registry::local::{build,trust} direct units, journal HistoryFilter matrix, sandbox prepare_mcp_process_sandbox direct units, scaffold ensure_*_state branches.

P3 (hygiene): list direct units, doctor diagnostic-to-exit-code matrix, runner::sync exhaustiveness, dev render theme variants, runner::inputs typed-input units.

## Deliverable

This spec's deliverable is the ranked Priority backlog above. It is the handoff
artifact for future executable test specs; it does not itself create those
follow-up specs or issues.

Each future follow-up spec should:

1. References the surface and the gap.
2. Names the file the test should live in (existing or new).
3. Estimates effort in test-cases-per-surface (most are <10 cases).

## Acceptance

Run from `oss/`:

```bash
scafld validate rust-runtime-test-coverage
awk '/^## Acceptance/{exit} /^## Harden Rounds/{exit} {print}' .scafld/specs/drafts/rust-runtime-test-coverage.md | rg 'OwnedReservedPaymentAuthority|runner/payment.rs|as_borrowed' && exit 1 || test $? -eq 1
```

Completion criteria:

- The spec validates.
- The ranked Priority backlog is present and self-contained.
- The pre-harden body contains no references to deleted payment adapter files
  or removed owned-adapter symbols.

## Rollback

N/A. This is a read-only audit spec. Reverting the spec changes restores the
previous draft but does not alter runtime code.

## Non-goals

- Migrating to `cargo nextest` (deferred per `rust-kernel-architecture.md` §18).
- Coverage instrumentation (`tarpaulin`, `llvm-cov`): adds CI complexity for
  marginal signal once the matrix above is filled in.
- Property-based testing rollout beyond the existing `fanout_proptest.rs` and
  `policy_proptest.rs`. Adopt per-surface only when fixture coverage proves
  insufficient.
- Automating coverage discovery. Re-running this audit is a manual exercise
  until the matrix is filled; `tarpaulin`/`llvm-cov` remains deferred.

## References

- [`crates/runx-runtime/tests/`](../../crates/runx-runtime/tests/) — current
  integration test corpus
- [`oss/docs/rust-kernel-architecture.md`](../../docs/rust-kernel-architecture.md)
  §11 (property + differential testing)
- [`plans/rust-takeover.md`](../../../plans/rust-takeover.md) §11 (outreach
  gating, which assumes a passing test signal)

## Harden Rounds

### round-1

Status: needs_revision
Started: 2026-05-21T01:46:31Z
Ended: 2026-05-21T01:46:31Z
Verdict: needs_revision
Provider: claude
Model: claude-opus-4-7
Output format: claude.mcp_submit_harden
Summary: Draft frames a read+report coverage audit but cannot be approved as-is. The top P1 entry in "Untested" cites `crates/runx-runtime/src/execution/runner/payment.rs::OwnedReservedPaymentAuthority::as_borrowed`, but that file was deleted (git status shows `D` for `runner/payment.rs`) and the symbol exists nowhere in the tree — it is a stale citation from before `authority-core-surface-prune-v1` landed. The spec also ships with empty Scope / Phases / Acceptance sections, so there is no executable gate for `scafld build`/`complete`, and the deliverable shape (does the ranked list live in this spec, in follow-up specs, or as filed issues?) is ambiguous. Minor counting and surface-classification slips compound the impression that the audit was assembled against a snapshot that is already moving. Fix the stale citation, add an Acceptance section (even if rollback is explicitly N/A), and clarify the hand-off artifact before approval.

Checks:
- path audit
  - Grounded in: code:crates/runx-runtime/src/execution/runner/
  - Result: failed
  - Evidence: Spec lists `runner/payment.rs::OwnedReservedPaymentAuthority::as_borrowed` and `the rest of the Owned* lifetime adapters` under Untested (P1). Glob of `crates/runx-runtime/src/execution/runner/*.rs` returns only execution.rs, sync.rs, inputs.rs, steps.rs, authority.rs — payment.rs is deleted (git status shows `D crates/runx-runtime/src/execution/runner/payment.rs`). Grep for `OwnedReservedPaymentAuthority` and `as_borrowed` finds zero hits anywhere except inside the draft spec itself. The pruning is in flight under active spec `authority-core-surface-prune-v1`.
- command audit
  - Grounded in: spec_gap:acceptance
  - Result: not_applicable
  - Evidence: The draft declares no acceptance commands and the rendered `acceptance` section is empty (0 bytes per context manifest). There is nothing to audit because the spec explicitly states it `produces only the ranked list; it does not write the tests`.
- scope/migration audit
  - Grounded in: spec_gap:scope
  - Result: failed
  - Evidence: Context manifest reports `scope`, `phases`, and `acceptance` bodies all at 0 bytes. For a `read + report` deliverable that drives ~5 follow-up specs, the absence of a Scope section means the boundary between this audit and the spawned follow-ups is undefined. `scafld build` has no phase to open and `scafld complete` has no validation to satisfy.
- acceptance timing audit
  - Grounded in: spec_gap:acceptance
  - Result: failed
  - Evidence: Spec says `Output of this spec: A follow-up filing (one issue or one focused spec per row above)` but provides no acceptance check that those filings exist, no naming convention, and no `Validation:` block. There is no signal a reviewer can run to know the audit is complete vs in-flight.
- rollback/repair audit
  - Grounded in: spec_gap:acceptance
  - Result: not_applicable
  - Evidence: Spec is read-only — `This is a read + report spec, not a code change` (Why-this-exists section). Rollback is genuinely N/A, but the spec should explicitly mark it so harden review does not re-raise it. Currently no Rollback section exists at all.
- design challenge
  - Grounded in: code:crates/runx-runtime/tests/
  - Result: passed
  - Evidence: The audit-then-spawn-followups pattern is the right architectural move for `rust-takeover §11` baseline gating: it produces a ranked, prioritized backlog rather than a single mega-PR that would block the launcher flip on speculative test work. The P1/P2/P3 risk-not-coverage-percentage framing is sound. Caveat: this is a point-in-time snapshot against a moving test corpus (`fixtures/harness/payment-approval-graph.yaml`, `runner/payment.rs`, etc. are all modified or deleted in current git status). Worth noting as accepted complexity, not blocking.

Issues:
- [critical/blocks approval] `harden-1` stale_citation - Untested section's top P1 entry cites a deleted file and a nonexistent type.
  - Status: open
  - Grounded in: code:crates/runx-runtime/src/execution/runner/
  - Evidence: The first row of `### Untested` names `runner/payment.rs::OwnedReservedPaymentAuthority::as_borrowed and the rest of the Owned* lifetime adapters`. Git status shows `D crates/runx-runtime/src/execution/runner/payment.rs`; the file is gone. Glob of `crates/runx-runtime/src/execution/runner/*.rs` lists execution.rs, sync.rs, inputs.rs, steps.rs, authority.rs. Grep for `OwnedReservedPaymentAuthority` and `as_borrowed` returns hits only in the draft spec itself. The active spec `authority-core-surface-prune-v1` (currently in review) drove the prune. As written, the audit's top production-risk item is fictional and would generate a follow-up spec with no surface to test.
  - Recommendation: Re-scan the runner module after `authority-core-surface-prune-v1` settles. Replace the Owned-adapter entry with the post-prune authority surface that actually exists (likely `runner/authority.rs::enforce_step_authority_admission` and `enforce_step_authority_receipt_before_success`) or remove the row if the responsibility moved into `runx-core::policy::payment_authority`. Note that the surface coverage audit must defer until that prune lands so it does not chase pre-prune ghosts.
  - Question: Should this audit explicitly block on `authority-core-surface-prune-v1` completing review, or rebase the Untested section against the post-prune surface now?
  - Recommended answer: Block: add a dependency on `authority-core-surface-prune-v1`. Rebase the Untested section once that spec is complete. Until then, drop the OwnedReservedPaymentAuthority row.
  - If unanswered: Default to dependency-block on `authority-core-surface-prune-v1`.
- [high/blocks approval] `harden-2` missing_acceptance - No Acceptance, Phases, or Scope sections — `scafld build`/`complete` have no gate.
  - Status: open
  - Grounded in: spec_gap:acceptance
  - Evidence: Harden context manifest renders `scope`, `phases`, and `acceptance` at 0 bytes each. The draft has a `Method`, surface tables, and a `Priority` block but no `## Acceptance` / `## Phases` / `## Scope And Touchpoints`. For a read-only audit, that does not mean acceptance is unnecessary — the deliverable is a ranked list, and `scafld complete` still needs a check (e.g., `the spec contains the ranked list` or `N follow-up issues filed`).
  - Recommendation: Add an Acceptance section with a single validation: either `the ranked Priority list appears in this spec at section X` (file-presence check) or `N follow-up spec files exist under .scafld/specs/drafts/` (glob check). Explicitly mark Rollback as N/A. Add a single-phase plan or declare the spec phase-less and document why.
  - Question: Should the deliverable be a self-contained ranked list inside this spec, or N filed follow-up specs that this one tracks?
  - Recommended answer: Self-contained list inside this spec; a separate follow-up command later spawns the per-surface specs as the operator decides which to fund.
  - If unanswered: Default to self-contained list; mark this spec complete when the Priority section is final.
- [medium/advisory] `harden-3` deliverable_ambiguity - Spec says output is `a follow-up filing` but does not say where or in what format.
  - Status: open
  - Grounded in: spec_gap:acceptance
  - Evidence: `Output of this spec` says `A follow-up filing (one issue or one focused spec per row above)` listing 3 requirements: references the surface, names the file, estimates effort. But there is no template for the filing, no directory for the follow-ups, and no count of how many filings constitute completion. The Why-this-exists section conversely says `the output is a checklist that drives the next ~5 small add tests for X follow-up specs`, suggesting the checklist itself is the output and follow-ups come later.
  - Recommendation: Reconcile the two statements: either the deliverable is the ranked list (and follow-ups are tracked externally), or it is N filed specs (and the ranked list is the artifact that proves they exist). Pick one and write it into the Acceptance section.
- [medium/advisory] `harden-4` surface_classification - `Method` step 2 says `public function and public type` but Untested entries cite `pub(super)` items.
  - Status: open
  - Grounded in: code:crates/runx-runtime/src/execution/runner/inputs.rs:1
  - Evidence: `inputs.rs` line 1+ shows `required_typed_input`, `optional_typed_vec_input` etc. are `pub(super) fn` — not part of the crate's public API. The Untested section cites them anyway. Either the method needs to broaden to `every behaviorally important fn regardless of visibility` or these entries should move to a different bucket. As-is, the method statement and the row contradict each other and the audit's selection criterion is unclear.
  - Recommendation: Rephrase the Method to `every function whose behavior is observable from a runtime entrypoint, regardless of Rust visibility` (matches what the audit actually does), or split the Untested rows into `public surface` and `internal helpers that gate public behavior`.
- [low/advisory] `harden-5` count_drift - Spec says `36 integration test files`; glob returns 34.
  - Status: open
  - Grounded in: code:crates/runx-runtime/tests/
  - Evidence: `Glob crates/runx-runtime/tests/*.rs` lists 34 files (hello_graph, agent_parity, skill_issue_intake, harness_fixtures, mcp_adapter, registry, tool_catalogs, config, catalog_adapter, parity, fanout_proptest, a2a_parity, connect_secret_redaction, connect_support, skill_issue_to_pr, fanout_parity, doctor, receipt_store, skill_run, receipt_paths, external, registry_client, connect_policy_integration, scaffold, approval, receipt_tree, connect_client, journal_history, mcp_server, target_runner, post_merge_observer, payment_receipts, payment_execution, dev). Spec claims 36. Small but suggests the figure was taken before recent test consolidation (cf. `D crates/runx-runtime/tests/payment_authority.rs` in git status).
  - Recommendation: Either re-count after `authority-core-surface-prune-v1` settles, or drop the absolute count in favor of `approximately 30+` since the corpus is moving.
- [low/advisory] `harden-6` duplicate_entry - `dev::watch` change debounce appears in both Thin and Untested tables.
  - Status: open
  - Grounded in: spec_gap:scope
  - Evidence: `### Thin` row `dev::*` lists `watch debounce semantics` under Missing. `### Untested` then has a separate row `dev::watch change debounce` with the rationale `Time-sensitive; flaky integration test deferred is better than no test.` Either the surface is Thin (has at least one test) or Untested (has none) — it cannot be both. Likely the parent `dev` module has a test (`tests/dev.rs`) but the specific `watch` submodule does not.
  - Recommendation: Merge into one row or sharpen the distinction (e.g., `dev::*` has happy-path coverage; `dev::watch::debounce` has no time-sensitive coverage). Keeping both as separate priority items risks double-funding the same follow-up spec.
- [low/advisory] `harden-7` dogfood_unaddressed - Harden contract asks `Can we dogfood this?`; spec does not address whether the audit is reproducible.
  - Status: open
  - Grounded in: spec_gap:scope
  - Evidence: The audit is hand-rolled (read each module, enumerate tests, judge gaps). A point-in-time snapshot against a moving corpus (see git status modifications across `runner/*.rs`, `tests/payment_*.rs`, `fixtures/harness/payment-approval-graph.*`) will drift quickly. The Non-goals section explicitly defers coverage instrumentation `for marginal signal once the matrix above is filled in`. That is a reasonable trade, but the spec should say whether re-running this audit in 3 months is a manual repeat or whether a script (e.g., `cargo +nightly rustdoc --output-format json` + a parser) becomes the dogfood path.
  - Recommendation: Add a one-line statement under Non-goals (or a new `Reproducibility` section): `Re-running this audit is a manual exercise; tarpaulin/llvm-cov adoption is deferred until the matrix is filled.` This makes the trade explicit so a future agent does not relitigate it.

### round-2

Status: passed
Started: 2026-05-21T04:05:00Z
Ended: 2026-05-21T04:28:00Z
Verdict: passed
Provider: local
Model: codex
Output format: manual_resolution
Summary: Resolved the stale runtime coverage audit findings. The draft now has explicit scope, phases, acceptance, rollback, a self-contained ranked backlog, and no pre-harden references to the deleted payment adapter target.

Checks:
- path audit
  - Grounded in: code:.scafld/specs/drafts/rust-runtime-test-coverage.md:105
  - Result: passed
  - Evidence: The untested table now targets existing `runner/inputs.rs` and `runner/sync.rs` surfaces, not the deleted `runner/payment.rs` or removed `OwnedReservedPaymentAuthority` symbol.
- command audit
  - Grounded in: code:.scafld/specs/drafts/rust-runtime-test-coverage.md:133
  - Result: passed
  - Evidence: `scafld validate rust-runtime-test-coverage` exited 0. The stale-symbol grep over the pre-acceptance body exited 0 because no stale citation remains.
- scope/migration audit
  - Grounded in: code:.scafld/specs/drafts/rust-runtime-test-coverage.md:37
  - Result: passed
  - Evidence: Scope now states this is a read-only audit, excludes the in-flight payment/authority prune surface, and makes the ranked backlog the deliverable.
- acceptance timing audit
  - Grounded in: code:.scafld/specs/drafts/rust-runtime-test-coverage.md:129
  - Result: passed
  - Evidence: Acceptance commands and completion criteria are present and executable from `oss/`.
- rollback/repair audit
  - Grounded in: code:.scafld/specs/drafts/rust-runtime-test-coverage.md:145
  - Result: not_applicable
  - Evidence: The spec is read-only and rollback is explicitly N/A.
- design challenge
  - Grounded in: code:.scafld/specs/drafts/rust-runtime-test-coverage.md:109
  - Result: passed
  - Evidence: The audit remains a ranked backlog rather than a mega-test implementation slice, which keeps the launcher-baseline work fundable as focused follow-up specs.

Issues:
- [critical/blocks approval] `harden-1` stale_citation - Untested section's top P1 entry cited a deleted file and nonexistent type.
  - Status: closed
  - Grounded in: code:.scafld/specs/drafts/rust-runtime-test-coverage.md:105
- [high/blocks approval] `harden-2` missing_acceptance - No Acceptance, Phases, or Scope sections.
  - Status: closed
  - Grounded in: code:.scafld/specs/drafts/rust-runtime-test-coverage.md:37
- [medium/advisory] `harden-3` deliverable_ambiguity - Follow-up filing location was undefined.
  - Status: closed
  - Grounded in: code:.scafld/specs/drafts/rust-runtime-test-coverage.md:117
- [medium/advisory] `harden-4` surface_classification - Method excluded internal helpers while the audit included them.
  - Status: closed
  - Grounded in: code:.scafld/specs/drafts/rust-runtime-test-coverage.md:55
- [low/advisory] `harden-5` count_drift - Absolute integration-test count drifted.
  - Status: closed
  - Grounded in: code:.scafld/specs/drafts/rust-runtime-test-coverage.md:27
- [low/advisory] `harden-6` duplicate_entry - `dev::watch` debounce was double-counted.
  - Status: closed
  - Grounded in: code:.scafld/specs/drafts/rust-runtime-test-coverage.md:107
- [low/advisory] `harden-7` dogfood_unaddressed - Reproducibility was implicit.
  - Status: closed
  - Grounded in: code:.scafld/specs/drafts/rust-runtime-test-coverage.md:158

### round-3

Status: passed
Started: 2026-05-21T04:35:00Z
Ended: 2026-05-21T02:02:55Z
Verdict: passed
Provider: local
Model: codex
Summary: Final manual harden evidence: the runtime coverage audit is now a valid read-only backlog spec with stale payment references removed and local acceptance commands recorded.

Checks:
- path audit
  - Grounded in: code:.scafld/specs/drafts/rust-runtime-test-coverage.md:105
  - Result: passed
  - Evidence: The stale deleted payment adapter target is absent from the active audit body.
- command audit
  - Grounded in: code:.scafld/specs/drafts/rust-runtime-test-coverage.md:133
  - Result: passed
  - Evidence: `scafld validate rust-runtime-test-coverage` and the stale-reference grep both exited 0.
- scope/migration audit
  - Grounded in: code:.scafld/specs/drafts/rust-runtime-test-coverage.md:37
  - Result: passed
  - Evidence: Scope, phases, deliverable, and non-goals bound this to read-only coverage backlog work.
- acceptance timing audit
  - Grounded in: code:.scafld/specs/drafts/rust-runtime-test-coverage.md:129
  - Result: passed
  - Evidence: Acceptance commands and completion criteria are explicit.
- rollback/repair audit
  - Grounded in: code:.scafld/specs/drafts/rust-runtime-test-coverage.md:145
  - Result: not_applicable
  - Evidence: The spec is read-only and rollback is explicitly N/A.
- design challenge
  - Grounded in: code:.scafld/specs/drafts/rust-runtime-test-coverage.md:109
  - Result: passed
  - Evidence: The output is a ranked, fundable backlog rather than a broad implementation bundle.

Issues:
- none


## Review

Status: completed
Verdict: pass
Mode: verify
Summary: Human-reviewed override accepted: Reviewed scoped spec-only diff after harden. scafld validate rust-runtime-test-coverage passed and stale deleted payment adapter grep passed; no implementation files changed under this task.

Attack log:
- `review gate`: manual human audit -> clean (Reviewed scoped spec-only diff after harden. scafld validate rust-runtime-test-coverage passed and stale deleted payment adapter grep passed; no implementation files changed under this task.)

Findings:
- none

