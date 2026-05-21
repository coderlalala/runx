---
spec_version: '2.0'
task_id: rust-aplus-cleanup
created: '2026-05-21T03:50:00Z'
updated: '2026-05-21T04:18:10Z'
status: draft
harden_status: not_run
size: medium
risk_level: medium
---

# Rust A+ cleanup before cutover

## Why this exists

A full dark-pattern / code-quality sweep of the Rust workspace (all 7 crates,
~62k LoC) found a ranked list of issues to clear before the launcher flip.
Several are already being executed by in-flight work (the payment→authority
generalization, `source_type`→`SourceKind`, the `runx:` URI/RunxRef work, the
TS↔Rust contract-spine reshape). This spec captures the **remaining** items so
they are not lost, with file:line and a one-line fix each. Two items were
already executed out-of-band and are recorded as done.

This is a tracking + execution spec. Each section is independently shippable.
Items touching files under active parallel work are marked **[blocked-until:
<workstream>]** and should be picked up once that workstream lands.

## Done (executed)

- **CLI FilterMode** — `ListPlan { ok_only: bool, invalid_only: bool }` had a
  representable-but-invalid `(true, true)` state guarded only by a runtime
  check. Replaced with `FilterMode { All, OkOnly, InvalidOnly }` so the invalid
  state is unrepresentable (parse-don't-validate). `runx-cli/src/launcher.rs`,
  `list.rs`, `tests/launcher.rs`. Tests green.
- **SDK pass-through fields** — investigated `SkillSearchResult.source_type`,
  `ConnectionSummary.status`/`provider` being `String`. Verdict: NOT a defect.
  The SDK is a thin subprocess bridge that mirrors CLI JSON output and never
  matches these strings for control flow; typing them would duplicate
  vocabularies owned in other crates and make the bridge brittle to CLI
  additions. No change.
- **Local registry `split_skill_id` invariant** —
  `runx-runtime/src/registry/local.rs` no longer uses `unwrap_or_default()` to
  mask missing owner/name segments before validation. It now matches the two
  required non-empty segments explicitly and rejects extra segments before path
  safety checks.
- **Runtime time helpers** — removed the duplicate `now_iso8601`,
  `civil_from_unix_seconds`, and `civil_from_days` implementations from
  `runtime/scaffold/ids.rs` and `runtime/registry/local/util.rs`. Both now use
  one private `runtime/time.rs` helper.

## Tier 1 — correctness / drift risk

### A. Stringly discriminants (parse-time validated, stored as `String`, re-matched)

- **[blocked-until: source_type→SourceKind]** `SkillSource.source_type`
  (`runx-parser/src/skill.rs:55`) and `.input_mode`
  (`skill.rs:64`, matched at `runx-runtime/src/adapters/cli_tool.rs:71`).
  → `SourceKind` / `InputMode` enums, `#[serde(rename_all = "kebab-case")]`,
  parsed once. Dissolves the `RuntimeError::Unsupported*` variants (Class K).
- `CatalogMetadata.kind` / `.audience` / `.visibility`
  (`runx-parser/src/skill.rs:531-549`) — validated against `skill|graph`,
  `public|builder|operator`, `public|private`, stored as `String`.
  → `CatalogKind` / `CatalogAudience` / `CatalogVisibility` enums.
  *Same-file risk with source_type work; sequence after it.*
- `SkillSandbox.profile` / `.cwd_policy` (`runx-parser/src/skill.rs:104,106`).
  → reuse `runx_core::policy::sandbox::SandboxProfile` (already exists; do not
  duplicate) and a new `CwdPolicy` enum.
- **[blocked-until: RunxRef]** `journal.rs:901` `event.kind == "run_started"`;
  string status assignments `journal.rs:768,923,932,946,952`
  (`"paused"`/`"valid"`/`"invalid"`). → `JournalEventKind` / `JournalStatus`.
  *journal.rs also builds `runx:` URIs, so the RunxRef workstream will be in
  this file; coordinate.*

### B. Magic-string / label semantics

- **[blocked-until: payment→authority]** payment scope `"payment:spend"` +
  label `"payment rail proof"`. Already being generalized to `AuthorityVerb`
  dispatch via `admit_step_authority`.
- **[blocked-until: post_merge reshape]** `post_merge_observer/plan.rs:191`
  `criterion.criterion_id == "post_merge.close_policy_authorized"` and the
  whole `post_merge.*` criterion-id family. → `PostMergeCriterionKind` enum
  with serde rename. *`post_merge_observer/plan.rs` is under active reshape.*

### D. Hardcoded magic constants (latent bug)

- **[partially blocked: runner.rs active]** Two different fake "default"
  timestamps in four places: `"2026-05-20T00:00:00Z"`
  (`adapters/mcp/server_skill.rs:190,250`) and `"2026-05-18T00:00:00Z"`
  (`execution/runner.rs:39`, `execution/skill_run.rs:24` `DEFAULT_CREATED_AT`).
  The MCP server path invents its own date that **diverges** from the runner's
  default — a latent determinism bug. → one named const; the MCP server path
  should inject `created_at` from `RuntimeOptions` rather than hardcode.

### E. The `runx:` URI scheme is hand-built and hand-parsed

- **[blocked-until: RunxRef]** `format!("runx:act:{}")`,
  `format!("runx:decision:{}")`, `format!("runx:harness_receipt:{}")`,
  `strip_prefix("runx:harness_receipt:")`, `HARNESS_RECEIPT_REF_PREFIX` const —
  scattered across `journal.rs`, `receipts/seal.rs`, `receipts/tree.rs`,
  `post_merge_observer.rs`, `execution/target_runner.rs`.
  → one `RunxRef::{Act,Decision,HarnessReceipt,AgentAct}(id)` with `Display` +
  `FromStr`. *All these files are in the active reference/spine workstream.*

### H. `unwrap_or` masking invariants (not benign defaults)

- **[done]** `registry/local.rs:452` — `split_skill_id` uses explicit segment
  matching instead of `.unwrap_or_default()` plus `is_empty()`.
- **[blocked-until: RunxRef]** `journal.rs:988,995` —
  `value.as_str().unwrap_or("unknown")` hides a serialization failure behind
  the string `"unknown"`. → propagate the error.

## Tier 2 — mechanical, high-leverage, low-risk (pure deletion / dedup)

### C. Duplicated primitive helpers

- **`sha256_hex` / `sha256_prefixed` / `hex_lower` defined 8 times**:
  `contracts/act_assignment/hash.rs:87,91`,
  `contracts/post_merge_observer/plan.rs:858,862`,
  `contracts/target_runner/plan.rs:658`,
  `core/policy/authority_proof.rs:627,632`,
  `receipts/canonical.rs:9,42`, `runtime/doctor.rs:751`.
  → one canonical impl in `runx-contracts::fingerprint` (every crate depends on
  contracts). *Two of the contracts copies are in files under active reshape;
  do the core/receipts/runtime consolidation first, fold contracts in after.*
- `reference_type_name()` — **byte-identical** in `runtime/receipts/seal.rs:617`
  and `runtime/execution/target_runner.rs:901`. → one shared fn, or a
  `ReferenceType::as_str()` in contracts. *Both files active (RunxRef); after.*
- **[done]** `now_iso8601` + `civil_from_unix_seconds` + `civil_from_days` —
  consolidated into one private `runtime/time.rs` module.
- `reference()` URI builder duplicated `seal.rs:599` + `target_runner.rs:890`
  (pairs with `reference_type_name`).

### F. Boolean blindness (3+ adjacent bools)

- `OperationalPolicyAutomationPermissions { auto_merge, mutate_target_repo,
  require_human_merge_gate }` (`contracts/operational_policy.rs`). The validator
  already forces `auto_merge == false` and `require_human_merge_gate == true`,
  so two of three bools are effectively constants — the type is wrong.
  → `AutomationLevel` enum. *Contract spine active; coordinate.*
- `OperationalPolicyPostMergePolicy` bools — same file.
- **[done]** CLI `ListPlan` → `FilterMode` (above).
- CLI `InitPlan { global, prefetch_official, json }` — `runx-cli/launcher.rs`.
  → keep `json` separate, group the rest if a third mode appears. Low urgency.

### K. Domain-specific error variants

- **[blocked-until: source_type→SourceKind]** `RuntimeError::Unsupported{Adapter,
  RunStep,RunnerSelection} { ...: String }` (`error.rs`). Collapses once
  `SourceKind` parsing rejects unknown types at the boundary.

## Tier 3 — ergonomics / polish

### G. CLI dispatch + parse duplication

- 14× copy-pasted `if first_arg_is(&args, "...")` branches (`launcher.rs`).
  → static dispatch table. *Note: launcher.rs is the cutover parity oracle
  ("centralized for CLI routing tests"); keep all `tests/launcher.rs` +
  `fixtures/cli-parity` green. Defer until after the launcher flip soak to
  avoid perturbing the oracle.*
- `parse_*_plan` repeat the same flag-loop skeleton — shared helper.
- `Err(format!("unknown {cmd} flag {flag}"))` ×20 → `unknown_flag(cmd, flag)`.
- flag literals `"--json"` etc ×100 → `const FLAG_JSON`. The `truthy()` matcher
  (`launcher.rs:357`) mixes magic `"official"` into a boolean parser — document
  or split.

### I. Clone-driven design (runtime: 574 `.clone()`)

- `execution/target_runner.rs` `revision_details()` (~542-620) and
  `target_repo_runner_revision_receipt()` (~364-451) — 10+ clones each building
  static metadata in branch arms. → build references once / `Cow<Reference>`.
  *target_runner.rs active.*
- `registry/local/trust.rs` — highest clone density; audit trust-signal
  builders for clone-in-loop.

### J. Primitive obsession (35 `String` id fields)

- `step_id`, `run_id`, `receipt_id`, `act_id`, `policy_id`, `decision_id`,
  `term_id` threaded untyped across contracts + runtime; the compiler can't
  tell a `term_id` from a `step_id`. → newtypes for the 4-5 highest-traffic ids
  (`ReceiptId`, `StepId`, `ActId`, `RunId`). High churn; do last, as one pass,
  when the spine is stable.

## Class L — Naming & module structure (A+ infra naming vs the core model)

Audit of module names, filenames, and the layering they imply, judged against
the runx core vocabulary (act / authority / harness / decision / signal /
receipt / host / skill / graph / scope / sandbox). The crate layer itself is
clean (`contracts` → `core` → `parser`/`receipts` → `runtime` → `cli`/`sdk`);
findings are below that line.

- **L1. Module-file style is inconsistent across the workspace (mechanical,
  highest-leverage).** `runx-runtime` is the *only* crate using legacy
  `mod.rs` — 10 of them: `adapters/mod.rs`, `adapters/mcp/mod.rs`,
  `connect/mod.rs`, `dev/mod.rs`, `execution/mod.rs`,
  `execution/harness/mod.rs`, `receipts/mod.rs`, `registry/mod.rs`,
  `scaffold/mod.rs`, `tool_catalogs/mod.rs`. Every other crate uses the modern
  `foo.rs + foo/` style (`contracts/operational_policy.rs + operational_policy/`,
  `core/policy.rs + policy/`, etc.). → convert the runtime's 10 `mod.rs` to
  `foo.rs` siblings. Matches the workspace convention and the established
  preference (modern style was chosen when the contracts decomposition was
  corrected). Pure mechanical move; no logic change.

- **L2. `execution/orchestrator.rs` vs `execution/runner.rs` — synonym names
  hide a real layering.** `orchestrator.rs` is the canonical entrypoint facade
  (`LocalOrchestrator`, `RunRequest`/`RunResult`/`RunStatus`); `runner.rs` is
  the graph engine (`Runtime`, `GraphRun`, the step state machine). The names
  are near-synonyms, so a reader can't tell which is the front door. → rename
  for legibility, e.g. `runner.rs` → `engine.rs` (keep `orchestrator` as the
  entrypoint), or `orchestrator.rs` → `entry.rs`. Pick the pair that reads as
  facade-over-engine.

- **L3. `caller.rs::Caller` is off the core model.** It is the host callback
  interface (`report(event)`, `resolve(request)`) — exactly the boundary the
  contracts crate names `host` (`host_protocol`, `HostRunResult`,
  `HostRunState`, `ResolutionRequest`). `Caller` doesn't connect to that word.
  Verified: zero external consumers (CLI/SDK don't reference it), so a rename
  is safe and runtime-local. → consider `Host` / `HostBridge` /
  `HostInterface` so the runtime↔host boundary uses one vocabulary. Counter:
  `Caller` is a defensible inversion-of-control name (a test `NoopCaller`
  isn't a "host"); decision call, not a clear defect. Lower priority than L1/L2.

- **L4. `adapter.rs` (the `SkillAdapter` trait + invocation types) vs
  `adapters/` (the implementations) — distinguished only by singular/plural.**
  Fragile: a one-character difference carries the contract-vs-impl boundary.
  → after L1, fold the trait into `adapters.rs` (the modern parent file), or
  rename `adapter.rs` → `skill_adapter.rs` to make the trait's identity
  explicit and break the singular/plural collision.

- **L5. `post_merge_observer.rs` is an orphan at the runtime top level.** It is
  a runtime-side projector/observer sitting among unrelated peers (`adapter`,
  `approval`, `caller`, `config`, `doctor`). → cluster it under a named home
  (`observers/` or `signals/`), or move it into the closure flow under
  `execution/` if that is where it belongs. Structural tidiness, not a defect.

- **L6. Generic `types.rs` proliferation (7 instances): `core/policy/types.rs`,
  `core/state_machine/types.rs`, `parser/graph/types.rs`,
  `runtime/connect/types.rs`, `runtime/dev/types.rs`,
  `runtime/registry/types.rs`, `runtime/adapters/mcp/types.rs`** (plus
  `parser/graph/helpers.rs`, `registry/local/util.rs`). Defensible inside a
  multi-file feature module (the parent is just re-exports, no single "primary"
  file), but inconsistent with the contracts decision to fold types into the
  parent. → document one rule: "types live in the parent file unless the module
  has ≥3 sibling concern-files, in which case a `types.rs` sibling is allowed."
  Then apply it. Low priority; consistency rather than correctness.

- **L7. Verb-prefix near-synonyms in the pure-crate public surface (minor).**
  Validation-class operations split across `validate_*` (3), `lint_*` (1),
  `evaluate_*` (4); construction split across `build_*` (3), `create_*` (2),
  `derive_*` (3). → pick one verb per operation class (e.g. `validate` for
  fail-closed checks, `lint` for finding-collection, `build` for construction)
  and document it. Low priority polish.

- **L8. `runtime_http.rs` — generic name for the transitional curl-subprocess
  HTTP transport.** Acceptable as-is; it is replaced wholesale by
  `rust-async-http-layer`. Note only — rename naturally when that spec lands
  (`http.rs` once it is a real client, or delete).

Sequencing note: L1 (mod.rs → modern) should run *before* L4 (adapter fold) and
is independent of the Tier 1-3 logic work. L2/L3 are small, isolated renames.
L5-L8 are tidiness.

## Explicitly NOT findings (examined, kept)

- `Option<Vec<T>>` in `contracts/execution.rs` (`surface_refs`, `evidence_refs`)
  and `act_assignment.rs` (`scope_set`): mirror TypeScript
  `Type.Optional(Type.Array(...))` wire contracts where absent ≠ empty.
  Collapsing breaks cross-language parity. Verified against the TS schema.
- `ReferenceType::` / `AuthorityVerb` comparisons: already typed + exhaustive —
  the *correct* pattern the stringly ones should converge to.
- `serde_json::Value` in public APIs: zero instances.

## Sequencing

1. After payment→authority lands: confirm Class B(payment), K resolved.
2. After source_type→SourceKind lands: A(source_type/input_mode), then A
   (CatalogMetadata, SkillSandbox), then K collapse.
3. After RunxRef lands: E, A(journal), H(journal), C(reference_type_name).
4. Independent of the above (do anytime files are clean): C(time helpers — needs
   lib.rs settled), C(sha256 in core/receipts/runtime), F(operational_policy),
   D(timestamps), H(registry split_skill_id).
5. Post-cutover soak: G(CLI dispatch table), I(clone hotspots), J(newtypes).

## References

- Full scan findings: this spec's predecessor analysis in conversation.
- [`plans/rust-takeover.md`](../../../plans/rust-takeover.md) §9 (sunset order),
  §11 (outreach gating — assumes A+ baseline).
- [`oss/docs/rust-kernel-architecture.md`](../../docs/rust-kernel-architecture.md)
  §18 (Rust implementation quality bar).
