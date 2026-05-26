---
spec_version: '2.0'
task_id: runx-rust-runtime-architecture-lift-v1
created: '2026-05-26T13:14:19Z'
updated: '2026-05-26T22:20:34Z'
status: completed
harden_status: passed
size: large
risk_level: high
---

# runx Rust runtime architecture lift v1

## Current State

Status: completed
Current phase: final
Next: done
Reason: task completed
Blockers: none
Allowed follow-up command: `none`
Latest runner update: 2026-05-26T22:20:34Z
Review gate: pass

## Summary

This spec turns the Rust architecture case-study review into an executable
runtime architecture lift. It is not a broad rewrite and not a new-crate
sprawl plan. The goal is to make the current Rust code read like runx's
product model:

- a harness runs under explicit runtime services,
- authority is attenuated through a checkable algebra,
- adapters plug into one governed invocation pipeline,
- receipts and lifecycle events are emitted through one seal path,
- replay/stress gates are isolated and machine-readable.

The lift is a clean-cutover plan. Do not add compatibility aliases, legacy
adapters, old vocabulary bridges, or a second implementation path beside the
new one.

## Current Evidence

Commands re-run on 2026-05-26 during phase 1:

```sh
rg --files crates --glob '*.rs' --glob '!**/target/**' | xargs wc -l | sort -nr | sed -n '1,80p'
find crates/runx-runtime/src -maxdepth 3 -type f -name '*.rs' | sort
node scripts/check-rust-crate-graph.mjs
cargo tree --manifest-path crates/Cargo.toml -p runx-runtime --depth 1 --features cli-tool,catalog,mcp,async-http
rg -n "BTreeMap<String, String>|RuntimeOptions|RuntimeReceiptSignatureConfig::from_env|LocalReceiptStore::new|prepare_process_sandbox|prepare_mcp_process_sandbox|SkillAdapter|fn invoke\\(|std::process::Command|Command::new|tokio::process|receipt_dir|RUNX_" crates/runx-runtime/src crates/runx-cli/src crates/runx-core/src --glob '*.rs' --glob '!**/target/**'
sed -n '1,220p' crates/runx-runtime/src/lib.rs
sed -n '1,220p' crates/runx-runtime/src/adapter.rs
sed -n '1,220p' crates/runx-runtime/src/execution.rs
git diff -- crates/runx-runtime/src/execution/runner/execution.rs
git diff -- docs/runtime-throughput.md
```

Observed results from the refreshed phase 1 pass:

- Crate graph direction is currently clean:
  `node scripts/check-rust-crate-graph.mjs` passed.
- `runx-runtime` direct dependencies currently include `reqwest`, `rmcp`,
  `rustls`, and `tokio`, alongside `runx-contracts`, `runx-core`,
  `runx-parser`, and `runx-receipts`. This is acceptable only because the
  side-effect tier owns adapter/runtime surfaces; pure crates must remain
  free of these dependencies.
- `cargo tree --manifest-path crates/Cargo.toml -p runx-runtime --depth 1
  --features cli-tool,catalog,mcp,async-http` reports `reqwest v0.13.3`,
  `rmcp v1.7.0`, `rustls v0.23.40`, and `tokio v1.52.3`.
- Runtime structure is broad but not yet internally settled. Current runtime
  modules include adapters, approval, config, credentials, dev, doctor,
  execution, host, journal, parser eval, payment, post-merge observer,
  receipts, registry, sandbox, scaffold, and tool catalogs.
- `crates/runx-runtime/src/lib.rs` re-exports many operational surfaces at
  crate root: adapter traits, config, credentials, dev loop helpers, doctor,
  harness, host, journal, list, orchestrator, parser eval, receipts, registry,
  runner, scaffold, and tool catalogs. The public root is convenient but
  does not yet communicate ownership layers.
- `SkillInvocation` in `crates/runx-runtime/src/adapter.rs` carries
  `skill_name`, `source`, inputs, resolved inputs, skill directory, raw
  `BTreeMap<String, String>` env, and credential delivery. That makes the
  adapter boundary concrete, but also forces env/service plumbing into every
  adapter.
- Repeated runtime-service plumbing is visible across runtime and CLI:
  raw env maps, receipt dirs, signer config, receipt stores, sandbox plans,
  process commands, and `RUNX_*` variables are passed through many modules.
- Process supervision is split by adapter surface today:
  `cli_tool.rs`, `external_adapter.rs`, and `adapters/mcp/transport.rs` each
  own parts of spawn/terminate/capture behavior.
- The largest surviving Rust implementation files are concentrated in runtime
  execution, sandbox, adapter, payment, receipt, and target-runner surfaces:
  `runx-contracts/tests/schema_wire_compat.rs` 4284 lines,
  `execution/target_runner.rs` 2338 lines,
  `tests/payment/execution.rs` 1886,
  `tests/skill_run.rs` 1825,
  `execution/harness/runner.rs` 1407,
  `post_merge_observer.rs` 1324,
  `sandbox.rs` 1279,
  `adapters/external_adapter.rs` 1259,
  `execution/runner/steps.rs` 1241,
  `runx-receipts/src/tree.rs` 1237,
  `runx-core/src/policy/payment_authority.rs` 1192,
  `execution/skill_run.rs` 1108.
- Current dirty files are outside phase 1 implementation ownership and must
  not be overwritten:
  `crates/runx-receipts/src/canonical.rs`,
  `crates/runx-receipts/src/verify/proof.rs`,
  `crates/runx-runtime/src/execution/runner/execution.rs`,
  `crates/runx-runtime/src/receipts/seal.rs`,
  `crates/runx-runtime/src/receipts/store.rs`,
  `crates/runx-runtime/src/receipts/tree.rs`, and
  `docs/runtime-throughput.md`.
- The active fanout diff extracts `execute_parallel_fanout_batch` and
  `join_parallel_fanout_handles`; this spec defers touching
  `execution/runner/execution.rs` until that work lands.
- The active throughput doc diff adds `RUNX_MAX_FANOUT_CONCURRENCY`
  documentation; this spec defers touching `docs/runtime-throughput.md` until
  that work lands.

## Case Study Inputs

The scope is informed by current code plus primary-source architecture cues:

- Deno runtime separates a configurable runtime worker from permission parsing
  and OS bindings. The useful lesson is not to copy Deno's worker model; it is
  to make runx runtime services explicit instead of ambient env/config access.
- Wasmtime/WASI uses crate and module boundaries to make capability boundaries
  enforceable. The useful lesson is that runx should split only where a
  dependency or capability direction becomes mechanically enforceable.
- Nushell separates protocol, engine, command, plugin protocol, plugin engine,
  and test support. The useful lesson is that runx adapter invocation protocol
  and adapter test support should become stable internal surfaces.
- Vector keeps sources, transforms, sinks, topology, config, secrets, and
  telemetry as explicit areas. The useful lesson is that runx needs explicit
  lifecycle/telemetry ownership, not receipt/event construction scattered
  through feature modules.
- Nextest separates build/list/run phases and runs tests as isolated
  processes. The useful lesson is to make harness replay, fixture listing,
  stress runs, and soak gates separate machine-readable phases instead of one
  broad `cargo test` concept.

These are inputs, not authority. If a pattern does not fit runx's harness,
authority, and receipt model, do not adopt it.

## Architecture Rules

- Crate count stays stable unless a split creates an enforceable dependency
  direction. Prefer internal modules over new crates.
- Pure crates remain pure. `runx-contracts`, `runx-core`, `runx-parser`,
  `runx-receipts`, and CLI-backed `runx-sdk` must not gain async runtime,
  HTTP, MCP, process, filesystem, or registry-network dependencies.
- The runtime owns side effects, but side effects must enter through named
  services. Raw env maps, receipt dirs, process spawns, and signer config
  should not leak through unrelated layers.
- Harness is the governed execution boundary. Architecture names should
  reinforce harness, authority, decision, act, closure, receipt, signal,
  adapter, and host. Do not reintroduce `work_item`, `engagement`,
  `operation` as object, `judgment`, `outcome`, or compatibility vocabulary.
- Adapter invocation has one lifecycle: resolve, admit, invoke, capture,
  redact/project, seal. Individual adapters may customize phases, but they
  should not each invent the lifecycle.
- Authority attenuation must be computed by typed values and property-tested.
  References and proof records may cite the comparison; they must not be the
  comparison.
- Receipt/lifecycle events are product evidence, not incidental logging. They
  need one internal owner and should be easy to project into local history,
  hosted ingestion, and test oracles.
- Testing is a surface. Fast build gates, semantic parity gates, receipt
  oracle gates, adapter stress gates, and soak gates must stay distinct.

## Objectives

- Introduce explicit runtime service/context ownership without changing public
  wire formats.
- Collapse duplicated adapter process and invocation lifecycle logic behind a
  single internal pipeline.
- Make authority attenuation a coherent algebraic module with property tests.
- Establish one receipt/lifecycle event stream used by runtime execution,
  harness replay, local history, and adapter surfaces.
- Split harness replay and adapter stress testing into deterministic,
  machine-readable phases.
- Use this lift to guide later decomposition, not to churn every large file
  immediately.

## Scope

In scope:

- `crates/runx-runtime/src/lib.rs` public/internal module shape.
- `crates/runx-runtime/src/adapter.rs` and built-in adapter modules.
- `crates/runx-runtime/src/execution/**`, excluding active dirty edits until
  their owner lands.
- `crates/runx-runtime/src/receipts/**`, `journal.rs`, `host.rs`, and
  lifecycle/event projection code.
- `crates/runx-runtime/src/sandbox.rs` as a service consumer and eventual
  service provider boundary.
- `crates/runx-runtime/src/config.rs`, `credentials.rs`, `registry/**`, and
  `runtime_http.rs` only where they participate in runtime-service plumbing.
- `crates/runx-core/src/policy/**` authority subset and attenuation logic.
- Runtime and adapter tests that prove behavior-preserving refactors.
- Docs that define the Rust runtime architecture and testing profiles.

Out of scope:

- Broad large-file cleanup owned by `monolith-decomposition-v1`.
- Release readiness work owned by `runx-rust-95-release-readiness`.
- Runtime-local TypeScript deletion owned by the `rust-ts-sunset-*` specs.
- Cloud worker architecture unless a runtime-service boundary must be consumed
  from cloud through an already-defined interface.
- New public contract names, schema IDs, or compatibility aliases.
- New Rust crates unless a harden/review pass proves the dependency boundary
  cannot be enforced as an internal module.

## Dependencies

- `monolith-decomposition-v1`: owns broad god-file decomposition. This spec
  may split files only when it is necessary to establish a named architecture
  boundary.
- `runx-oss-trust-boundary-cleanup-v1`: owns immediate security cleanup. This
  spec must build on those trust-boundary decisions, not weaken them.
- `rust-mcp-rmcp-cutover` and `rust-runtime-adapters-mcp`: archived MCP
  adapter history; this spec may refine MCP architecture but must not revert
  rmcp/tokio decisions without a new dependency-policy ruling.
- `rust-contract-pipeline-inversion`: archived contract source-of-truth work;
  this spec must not recreate hand-authored schema ownership in runtime.
- Current dirty fanout/throughput work: do not touch
  `execution/runner/execution.rs` or `docs/runtime-throughput.md` until those
  edits land or are explicitly released.

## Risks

- Refactoring could hide security regressions behind "architecture cleanup".
  Mitigation: every phase needs behavior-preserving tests before larger file
  movement.
- A context/services object could become a god object.
  Mitigation: define small service facets (`WorkspaceEnv`, `ReceiptServices`,
  `SandboxServices`, `AdapterServices`) and keep construction near runtime
  entrypoints. Do not add a catch-all `HarnessContext` in this phase.
- Adapter pipeline extraction could overfit one adapter.
  Mitigation: prove the pipeline with CLI tool, external adapter, and MCP
  before converting A2A/agent/catalog.
- Authority algebra could become payment-only.
  Mitigation: keep payment-specific rules in payment modules, but extract
  shared subset/attenuation primitives where non-payment authority can use
  them.
- Testing phases could slow the normal loop.
  Mitigation: fast gates remain first-class; stress/soak gates are explicit
  commands and not hidden inside every local build.

## Phase 1: Evidence Refresh And Boundary Map

Status: completed
Dependencies: none

Objective: refresh evidence immediately before implementation and produce a

Changes:
- Re-run the current evidence commands in this spec.
- Add or update a local architecture note that maps current runtime modules to these buckets: service construction, harness execution, adapter invocation, receipt/event projection, authority algebra, CLI presentation, dev/testing. The map lives in `docs/rust-kernel-architecture.md` under a "Runtime buckets" section so the architecture note has one stable home.
- Record any files currently dirty and exclude them until their owner lands.
- Decide whether this spec supersedes any part of `monolith-decomposition-v1`; default answer should be no.

Acceptance:
- `cargo test --manifest-path crates/Cargo.toml -p runx-runtime --features cli-tool,catalog,mcp --test cli_tool_contract`
- `cargo test --manifest-path crates/Cargo.toml -p runx-runtime --features external-adapter --test external_adapter`
- `cargo test --manifest-path crates/Cargo.toml -p runx-runtime --features cli-tool,catalog,mcp --test mcp_adapter`
- `cargo test --manifest-path crates/Cargo.toml -p runx-runtime --features cli-tool,catalog,mcp --test mcp_server`
- `cargo build --manifest-path crates/Cargo.toml -p runx-runtime --all-targets --features cli-tool,catalog,mcp`
- `rg -n "start_kill\\(|kill_direct_child_if_running|process_group\\(|Command::new\\(\\\"/bin/kill\\\"\\)|terminate_tokio_child" crates/runx-runtime/src/adapters --glob '*.rs'`

Phase 3 evidence, 2026-05-26:

- Added `crates/runx-runtime/src/adapter_pipeline.rs` with
  `AdapterInvocationPlan`, `AdapterExecutionContext`, `AdapterCapture`, and
  `AdapterProjection`; kept `SkillAdapter` as the stable trait.
- Wired CLI-tool and MCP adapters through the shared projection/timing context.
  External adapter now uses the shared invocation plan and duration helper.
- Updated MCP process transport to spawn MCP servers in their own process group
  on Unix and terminate with the same TERM/force process-group discipline used
  by CLI-tool and external adapters. The non-Unix path remains direct-child
  termination because process groups are not available there.
- Updated MCP server tests to supply production receipt-signing env and verify
  written receipts with the matching production signature policy.
- All listed phase 3 acceptance commands passed.

## Phase 2: Runtime Services

Status: completed
Dependencies: phase1

Objective: replace repeated raw runtime plumbing with explicit service

Changes:
- Introduce small internal service structs rather than one broad god object. Candidate names: dir, tool roots, registry/home paths. receipt verification config. sandbox planning. shared redaction/projection helpers.
- Construct services at `RuntimeOptions`/`LocalOrchestrator`/MCP server boundaries, not inside leaf adapters.
- Pass the individual service facets where they are needed. Do not introduce a bundle object unless a later implementation review proves the plumbing cost is worse than the extra abstraction.
- Keep `RuntimeOptions::local_development()` explicit. Do not add `Default`.
- Keep env serialization unchanged at process boundaries, but avoid passing raw env maps through unrelated internal APIs where a typed service can answer the question.

Acceptance:
- `cargo test --manifest-path crates/Cargo.toml -p runx-core policy`
- `cargo test --manifest-path crates/Cargo.toml -p runx-core --test policy_proptest`
- `cargo test --manifest-path crates/Cargo.toml -p runx-runtime --features cli-tool,catalog,mcp --test payment`

Phase 4 evidence, 2026-05-26:

- Added `runx_core::policy::authority_algebra` with shared pure subset
  primitives for reference address equality, item subset checks, parent
  requirement preservation, optional exact narrowing, and optional bounded
  narrowing.
- Rewired payment authority comparison to consume those shared primitives
  instead of owning generic subset helpers locally.
- Removed a duplicate cap-subset branch in the old payment-local helper by
  replacing it with `optional_bound_subset`.
- Added property coverage for reflexivity, transitivity, and denied widening
  in `crates/runx-core/tests/policy_proptest.rs`.
- All listed phase 4 acceptance commands passed.

## Phase 3: Adapter Invocation Pipeline

Status: completed
Dependencies: phase2

Objective: make adapter execution lifecycle shared and explicit without

Changes:
- Define internal pipeline types around existing `SkillInvocation` and `SkillOutput`, such as `AdapterInvocationPlan`, `AdapterExecutionContext`, `AdapterCapture`, and `AdapterProjection`.
- Move shared steps out of individual adapters where behavior is duplicated: cwd/tool-root resolution, sandbox admission, bounded process supervision, receipt-dir propagation, metadata hashing, output projection, and redaction.
- Convert CLI tool, external adapter, and MCP first. Convert A2A, agent, and catalog only after the shared pipeline is proven not to distort their shape.
- Keep `SkillAdapter` as the stable trait unless harden proves it is now the wrong abstraction. Do not create an adapter crate in this phase.
- Unify child termination semantics. Direct child-only termination must not remain in one adapter while another kills process groups.

Acceptance:
- none

## Phase 4: Authority Algebra

Status: completed
Dependencies: phase1

Objective: make attenuation checkable as algebra rather than scattered

Changes:
- Extract shared authority primitives in `runx-core::policy` for subset, equality, resource-family comparison, actor/principal narrowing, time bounds, approval requirements, and capability consumption.
- Keep payment-specific terms in payment authority modules, but have them use shared primitives where possible.
- Add property tests for reflexivity, transitivity where valid, denied widening, malformed resource rejection, and payment/non-payment examples.
- Ensure runtime receipt/proof code records the comparison evidence but does not treat an asserted proof result as authoritative.

Acceptance:
- none

## Phase 5: Receipt And Lifecycle Event Stream

Status: completed
Dependencies: phase2

Objective: establish one internal receipt/lifecycle event owner for harness,

Changes:
- Define internal lifecycle event types for harness opened, decision recorded, act started/closed, child harness linked, adapter invoked, receipt sealed, abnormal seal, verification recorded, and publication projected.
- Route receipt creation and local journal/history projection through one event/seal owner rather than hand-building adjacent metadata in multiple feature modules.
- Preserve existing receipt wire formats unless a separate contract spec changes them.
- Ensure abnormal terminal paths still emit a receipt or explicit failed seal evidence. If no existing test covers killed, timed out, sandbox-denied, and adapter-crash seal evidence after the event-stream rewrite, add a dedicated `abnormal_seal` integration test and fixture before closing this phase.

Acceptance:
- none

## Phase 6: Harness Replay And Stress Gates

Status: completed
Dependencies: phase3, phase5

Objective: make harness replay and adapter stress testing separate,

Changes:
- Extract the existing `runx-harness-fixture-oracles` fixture table into a runtime library function, for example `harness::fixtures::list_cases()`, so the oracle binary and future CLI readers share one source. Do not add a second fixture registry.
- Ensure replay runs can emit a machine-readable summary with per-case status, elapsed time, receipt id/digest, and failure classification.
- Keep fast semantic gates distinct from stress/soak gates. Do not hide MCP or fanout stress under normal `cargo test --workspace`.
- Add explicit stress commands for MCP stdio transport, CLI tool process supervision, external adapter cancellation, and fanout ordering/concurrency.
- Document which gates are local fast, CI fast, heavy, and soak.

Acceptance:
- none

## Phase 7: Public Root And Documentation Polish

Status: completed
Dependencies: phases2-6

Objective: make the final module/export surface human-authored and legible.

Changes:
- Review `crates/runx-runtime/src/lib.rs` re-exports. Keep public exports that are real user/internal-consumer surfaces; move helper exports behind modules.
- Add concise module docs that explain the runtime buckets without jargon.
- Update `docs/rust-kernel-architecture.md`, `docs/trusted-kernel-package-truth.md`, and `docs/how-we-test.md` with the final service/pipeline/event/testing shape.
- Remove stale `rust-style-allow: large-file` waivers only when the file is actually decomposed or the waiver is no longer needed.

Acceptance:
- none

## Definition Done

- Current evidence refreshed after active dirty files are resolved.
- No pure crate gains runtime/adapter/network dependencies.
- Runtime services are explicit, independently passed, and not bundled into a
  god object.
- At least three adapters use the shared invocation pipeline.
- Authority attenuation has shared typed primitives and property coverage.
- Receipt/lifecycle events have one internal owner.
- Harness replay and stress gates are documented and machine-readable.
- Public docs and module exports reflect the final shape.
- No compatibility aliases, legacy vocabulary bridges, or old runtime fallback
  surfaces are introduced.

## Rollback

- Revert phase commits independently; each phase must preserve public wire
  formats unless a separate contract spec says otherwise.
- If a service abstraction grows into a god object, stop and split by service
  facet before continuing.
- If adapter pipeline extraction distorts MCP/external adapter semantics,
  keep the shared process supervisor and defer broader adapter conversion.
- If authority algebra extraction risks payment behavior, preserve payment
  behavior and narrow the shared primitive set.

## Review Notes

- Claude harden must challenge whether this spec duplicates
  `monolith-decomposition-v1` or whether it owns a distinct architecture seam.
- Claude harden must challenge whether services become unnecessary ceremony.
  After round 1, this draft intentionally drops `HarnessContext` and keeps
  only individual service facets.
- Claude harden must challenge whether adapter pipeline extraction is too broad
  for one spec.
- Claude harden must challenge whether authority algebra belongs here or in a
  separate policy-only spec.
- Claude harden must challenge whether stress-gate work belongs here or in
  release-readiness.
- Review must probe for any active authority decision path where stored proof
  output replaces recomputation. Useful search:
  `rg -n "result:.*subset|\\\"subset\\\"|asserted.*authority|authority.*assert" crates/runx-core/src crates/runx-runtime/src crates/runx-contracts/src --glob '*.rs'`.

## Harden Rounds

### round-1

Status: needs_revision
Started: 2026-05-26T13:16:37Z
Ended: 2026-05-26T13:16:37Z
Verdict: needs_revision
Provider: claude
Model: claude-opus-4-7
Output format: claude.mcp_submit_harden
Summary: Architecturally the lift reads well: it owns a distinct seam (service/context, adapter pipeline, authority algebra, lifecycle event owner, replay/stress separation) that complements `monolith-decomposition-v1` (file size) and `runx-rust-95-release-readiness` (gates/dogfood). Scope, rollback, and dirty-file exclusions are credible. However, three acceptance commands are non-executable as written (a malformed `cargo test --test payment/execution` target, a `runtime-throughput.mjs check` missing `--baseline`, and a phase-7 retired-vocab rg that hits the guard test in `crates/runx-runtime/tests/target_runner.rs:1531-1546`). There are also unresolved seam questions: where the Phase 1 boundary map lives, whether the planned fixture listing duplicates the existing `runx-harness-fixture-oracles` binary, how the abnormal-seal invariant is gated, and whether `HarnessContext` is a useful facet or a soft god-object combining the other four services. These are all addressable as spec edits before approval.

Checks:
- path audit
  - Grounded in: code:crates/runx-runtime/src/lib.rs:1, code:crates/runx-runtime/src/adapter.rs:21, code:crates/runx-runtime/src/execution.rs:15, code:crates/runx-runtime/src/execution/runner.rs:42, code:scripts/runtime-throughput.mjs:63, code:scripts/runtime-adapter-oracle-checks.ts, code:scripts/check-rust-crate-graph.mjs, code:docs/rust-kernel-architecture.md, code:docs/trusted-kernel-package-truth.md, code:docs/how-we-test.md, code:crates/runx-core/src/policy.rs:1
  - Result: passed
  - Evidence: All in-scope source files (`crates/runx-runtime/src/lib.rs`, `adapter.rs`, `execution/**`, `receipts/**`, `journal.rs`, `host.rs`, `sandbox.rs`, `config.rs`, `credentials.rs`, `registry/**`, `runtime_http.rs`, `crates/runx-core/src/policy/**`), docs (`docs/rust-kernel-architecture.md`, `docs/trusted-kernel-package-truth.md`, `docs/how-we-test.md`), and supporting scripts (`scripts/runtime-throughput.mjs`, `scripts/runtime-adapter-oracle-checks.ts`, `scripts/check-rust-crate-graph.mjs`) exist. `package.json` defines `verify:fast` (line 58) and `boundary:check` (line 37). No fabricated paths.
- command audit
  - Grounded in: code:crates/runx-runtime/tests/payment.rs:1, code:scripts/runtime-throughput.mjs:63, code:crates/runx-runtime/tests/target_runner.rs:1531
  - Result: failed
  - Evidence: Three acceptance commands will not run as written: (1) Phase 4 — `cargo test --test payment/execution` is invalid; cargo `--test <name>` takes the integration-test binary name. `tests/payment.rs` exists as a single binary that mods in `payment/execution.rs`; the correct shape is `--test payment payment::execution::`. (2) Phase 6 — `node scripts/runtime-throughput.mjs check` exits with `perf:runtime:check requires --baseline <path>` (script line 64-66); a baseline JSON path must be supplied. (3) Phase 7 — `rg -n "work_item|engagement|judgment|..." crates packages docs scripts ...` will match `crates/runx-runtime/tests/target_runner.rs:1536-1539`, where these exact tokens appear inside an assertion guarding that retired vocabulary does NOT leak; the gate would fail on legitimate guard code.
- scope/migration audit
  - Grounded in: spec:.scafld/specs/drafts/runx-rust-runtime-architecture-lift-v1.md#scope, spec:.scafld/specs/drafts/monolith-decomposition-v1.md, spec:.scafld/specs/active/runx-rust-95-release-readiness.md:60
  - Result: passed
  - Evidence: Scope is distinct from `monolith-decomposition-v1` (file-size decomposition explicitly deferred) and from `runx-rust-95-release-readiness` (release gates explicitly deferred). Phase 3 process-supervisor unification and Phase 6 stress-gate work touch areas the 9.5 spec mentions (`Thread outbox provider process supervision`), but the lift cites that overlap and the 9.5 spec defers broad decomposition. No new crate is proposed; pure crates remain pure. Dirty files identified in git status (`execution/runner/execution.rs`, `docs/runtime-throughput.md`) are explicitly excluded.
- acceptance timing audit
  - Grounded in: spec:.scafld/specs/drafts/runx-rust-runtime-architecture-lift-v1.md#phases
  - Result: failed
  - Evidence: Most phase acceptance commands are runnable at the right phase (Phase 1 `scafld validate` + `check-rust-crate-graph.mjs`; Phase 2 service-related unit tests; Phase 3 adapter contract/external/mcp tests; Phase 5 receipt/journal/harness tests). However, Phase 5 lists no acceptance evidence for the explicit invariant `abnormal terminal paths still emit a receipt or explicit failed seal evidence` — no test target named to prove abnormal-seal coverage. Phase 4 includes a manual `rg` (`result:.*subset|...`) labeled as an acceptance check, but it's a hand-evaluated audit rather than a green/red signal. Phase 6 `pnpm verify:fast` runs full workspace fast-lane, which is heavier than a phase gate; acceptable but should be the last command, not interleaved.
- rollback/repair audit
  - Grounded in: spec:.scafld/specs/drafts/runx-rust-runtime-architecture-lift-v1.md#acceptance
  - Result: passed
  - Evidence: Rollback is credible: phase commits revert independently; per-phase rules cover the three named failure modes (service god-object, adapter-pipeline distortion, authority-algebra payment regression). Each rollback prescribes a concrete narrowing (split by facet, keep supervisor only, narrow primitive set) rather than open-ended retreat. Spec explicitly forbids compatibility aliases, so rollback cannot accidentally bifurcate the implementation.
- design challenge
  - Grounded in: spec:.scafld/specs/drafts/runx-rust-runtime-architecture-lift-v1.md#phase2, code:crates/runx-runtime/src/execution/runner.rs:44, code:crates/runx-runtime/src/adapter.rs:22
  - Result: passed
  - Evidence: The lift is the right architectural move, not bloat: today raw `BTreeMap<String,String>` env, `RUNX_*` env names, receipt dirs, sandbox plans, and process commands thread through unrelated layers (visible in `RuntimeOptions`, `SkillInvocation`, and ad-hoc helpers across `adapters/cli_tool.rs`, `adapters/external_adapter.rs`, `adapters/mcp/transport.rs`). Naming services (`WorkspaceEnv`, `ReceiptServices`, `SandboxServices`, `AdapterServices`) gives the existing plumbing typed owners without introducing a new crate. The remaining design risk is that `HarnessContext` composes all four other facets — that is exactly where the predicted god-object lives — so the spec should justify it as a thin bundle or drop it.

Issues:
- [high/blocks approval] `harden-1` command audit - Phase 4 acceptance command `cargo test --test payment/execution` is not a valid cargo target.
  - Status: open
  - Grounded in: code:crates/runx-runtime/tests/payment.rs:1
  - Evidence: `tests/payment.rs` is a single integration-test binary that declares `mod payment { mod execution; ... }`. Cargo's `--test <name>` accepts the integration-test file name, not a path. The correct form is `cargo test --manifest-path crates/Cargo.toml -p runx-runtime --features cli-tool,catalog,mcp --test payment payment::execution::`. As written the command exits with `no test target named 'payment/execution'`.
  - Recommendation: Rewrite Phase 4 acceptance to `cargo test --manifest-path crates/Cargo.toml -p runx-runtime --features cli-tool,catalog,mcp --test payment payment::execution::` (or scope the filter to whichever sub-test is the intended payment authority oracle).
  - Question: Is the intent to run the whole `payment` integration binary, or only the `execution` submodule? If only `execution`, the filter form above is the closest direct translation.
  - Recommended answer: Run the whole `payment` binary with `--test payment` and let the existing module structure exercise execution/state/receipts/ledger/stripe together.
  - If unanswered: Default to `--test payment` (full payment integration suite).
- [high/blocks approval] `harden-2` command audit - Phase 6 acceptance command `node scripts/runtime-throughput.mjs check` is missing the required `--baseline <path>` argument.
  - Status: open
  - Grounded in: code:scripts/runtime-throughput.mjs:63
  - Evidence: The script branches at line 63-66: `} else if (command === "check") { if (!options.baseline) { throw new Error("perf:runtime:check requires --baseline <path>."); } }`. Running the command as the spec writes it exits non-zero before doing any work.
  - Recommendation: Update the Phase 6 acceptance line to include a baseline path that exists in-repo (e.g., `node scripts/runtime-throughput.mjs check --baseline fixtures/throughput/baseline.json`) or add a `pnpm` alias that supplies the baseline, and confirm the baseline file is in-tree.
  - Question: Where does the throughput baseline live, and is regenerating it part of this spec or owned elsewhere (e.g., release-readiness)?
  - Recommended answer: Specify the exact baseline path (or owning spec) before approval; if the baseline file doesn't exist yet, gate the check behind a `runtime-throughput.mjs capture` step in Phase 6 changes.
  - If unanswered: Default to citing whichever baseline file `pnpm verify:fast` already uses, or drop the `check` from acceptance and rely on `capture`.
- [high/blocks approval] `harden-3` path audit - Phase 7 retired-vocabulary rg gate matches a guard test that legitimately names the retired tokens.
  - Status: open
  - Grounded in: code:crates/runx-runtime/tests/target_runner.rs:1531
  - Evidence: `crates/runx-runtime/tests/target_runner.rs:1531-1546` defines `assert_hard_cutover_vocabulary_only` which iterates `["work_item", "matter", "engagement", "judgment", "effect", "outcome"]` and asserts they do not leak. The Phase 7 acceptance `rg -n "work_item|engagement|judgment|operation as object|harness_receipt|compatibility shim|legacy adapter|RuntimeOptions::default\(" crates packages docs scripts ...` would flag this guard as a violation.
  - Recommendation: Either (a) tighten the rg to exclude test guard fixtures (`--glob '!**/tests/target_runner.rs'` or a more general `--glob '!**/tests/**'` if no production code is allowed to ship the tokens), or (b) move the retired-vocabulary string list into a fixture file or const that the rg pattern doesn't intersect.
  - Question: Should the Phase 7 gate exclude tests that intentionally name retired vocabulary, or should those guards relocate?
  - Recommended answer: Exclude `crates/**/tests/**` from the rg gate; the same tokens in production code remain forbidden and the guard test stays a positive backstop.
  - If unanswered: Default to adding `--glob '!**/tests/**'` to the Phase 7 rg.
- [medium/advisory] `harden-4` acceptance timing audit - Phase 5 names an abnormal-seal invariant but no acceptance test target proves it.
  - Status: open
  - Grounded in: spec:.scafld/specs/drafts/runx-rust-runtime-architecture-lift-v1.md#phase5
  - Evidence: Phase 5 Changes says `Ensure abnormal terminal paths still emit a receipt or explicit failed seal evidence`, but Phase 5 Acceptance lists only `receipt_signing`, `receipt_tree`, `journal_history`, `harness_fixtures`, and `hello_graph`. None of those names indicate they exercise abnormal terminal paths (panic, cancellation, sandbox denial, adapter crash) specifically.
  - Recommendation: Either name an existing test target that exercises abnormal seal paths (e.g., a cancellation/timeout fixture), or commit Phase 5 to adding one before its acceptance closes. Without it, the invariant is asserted prose, not gated.
  - Question: Which test target already covers abnormal terminal seal evidence today, and is its coverage sufficient for the unified event-stream rewrite?
  - Recommended answer: Add a named abnormal-seal fixture under `crates/runx-runtime/tests/` (e.g., `abnormal_seal`) and list it in Phase 5 acceptance.
  - If unanswered: Default to documenting the abnormal-seal invariant as a behavior-preserving claim only and call it out in Review Notes so the review gate adversarially probes it.
- [medium/advisory] `harden-5` scope/migration audit - Phase 6 fixture-listing command overlaps with the existing `runx-harness-fixture-oracles` binary; ownership and shape are unclear.
  - Status: open
  - Grounded in: code:crates/runx-runtime/src/bin/runx-harness-fixture-oracles.rs:26, spec:.scafld/specs/drafts/runx-rust-runtime-architecture-lift-v1.md#phase6
  - Evidence: A binary already exists at `crates/runx-runtime/src/bin/runx-harness-fixture-oracles.rs` with a hard-coded `FIXTURES: &[FixtureSpec]` table that enumerates harness cases. Phase 6 proposes 'Add a fixture listing command or library function that enumerates harness, adapter, receipt, and graph fixture cases without running them' — but doesn't say whether to extend the existing oracle binary, add a new CLI subcommand, or expose a library function.
  - Recommendation: Pick one of: (a) add a `--list-only` flag to `runx-harness-fixture-oracles`; (b) extract `FIXTURES` into a library function in `runx-runtime` and have both the oracle binary and a new listing command consume it; (c) add a `runx harness list` CLI subcommand. Name the choice in the spec so reviewers can verify the shape.
  - Question: Should the fixture lister extend the oracle binary, live in the runtime library, or surface as a CLI subcommand?
  - Recommended answer: Option (b): extract a `harness::fixtures::list_cases()` library function, have the oracle binary consume it, and expose it via the CLI as a thin reader.
  - If unanswered: Default to extracting the library function only; CLI surface can land in a follow-up.
- [medium/advisory] `harden-6` design challenge - `HarnessContext` risks being the very god-object the spec warns against.
  - Status: open
  - Grounded in: spec:.scafld/specs/drafts/runx-rust-runtime-architecture-lift-v1.md#phase2
  - Evidence: Phase 2 introduces four small facets (`WorkspaceEnv`, `ReceiptServices`, `SandboxServices`, `AdapterServices`) and then a fifth `HarnessContext` that 'combines the service facets for one harness execution'. A composer that holds the other four is the canonical god-object shape. The mitigation in the Risks section ('keep construction near runtime entrypoints') does not constrain HarnessContext's API surface.
  - Recommendation: Either drop `HarnessContext` and pass the individual facets where they are needed (cheap because they are small structs), or constrain HarnessContext to be a pure compositional newtype with no methods of its own and document that constraint as a rule.
  - Question: Does HarnessContext do anything beyond bundle the four facets — and if not, why not pass the facets directly?
  - Recommended answer: Drop HarnessContext for Phase 2 and revisit only if a measured plumbing-cost ratio shows the bundle pays for itself; default to passing facets directly.
  - If unanswered: Default to dropping HarnessContext from the Phase 2 changes list and re-introducing it only on demand.
- [low/advisory] `harden-7` spec_gap - Phase 1 boundary map has no named home; risk of orphan artifact.
  - Status: open
  - Grounded in: spec:.scafld/specs/drafts/runx-rust-runtime-architecture-lift-v1.md#phase1
  - Evidence: Phase 1 Changes says 'Add or update a local architecture note that maps current runtime modules to these buckets...' but does not point at a file path. Docs in scope (`docs/rust-kernel-architecture.md`, `docs/trusted-kernel-package-truth.md`, `docs/how-we-test.md`) already exist; the boundary map could either extend one of them or land as a new doc.
  - Recommendation: Name the file in Phase 1 (e.g., extend `docs/rust-kernel-architecture.md` with a 'Runtime buckets' section) so Phase 7 polish has a single, predictable artifact to refine.
  - Question: Should the boundary map extend `docs/rust-kernel-architecture.md` or land as a new doc under `docs/`?
  - Recommended answer: Extend `docs/rust-kernel-architecture.md`; Phase 7 already updates that file.
  - If unanswered: Default to extending `docs/rust-kernel-architecture.md`.
- [low/advisory] `harden-8` acceptance timing audit - Phase 4 rg-based acceptance for `asserted authority` is a manual audit, not a green/red signal.
  - Status: open
  - Grounded in: spec:.scafld/specs/drafts/runx-rust-runtime-architecture-lift-v1.md#phase4
  - Evidence: Phase 4 acceptance includes `rg -n "result:.*subset|\"subset\"|asserted.*authority|authority.*assert" ...` with the qualifier 'has no active authority decision path where a stored proof result replaces recomputation'. The pattern is loose and the judgment is human; this is not a runnable pass/fail check.
  - Recommendation: Either tighten the pattern to a structural check that can succeed cleanly (e.g., grep for a specific banned function name like `from_proof_result`), or move this audit to Review Notes as an adversarial focus rather than acceptance.
  - Question: Is there a specific function/symbol whose presence would prove the recomputation invariant is violated?
  - Recommended answer: Add the rg to Review Notes as adversarial probe; keep Phase 4 acceptance to the cargo-test commands.
  - If unanswered: Default to moving the rg into Review Notes.

### round-2

Status: passed
Started: 2026-05-26T13:28:55Z
Ended: 2026-05-26T13:28:55Z
Verdict: pass
Provider: claude
Model: claude-opus-4-7
Output format: claude.mcp_submit_harden
Summary: Round-2 verification of the revised draft. All four blocking issues from round-1 are addressed in the current spec: Phase 4 acceptance now uses the valid `--test payment` form (file confirmed at `crates/runx-runtime/tests/payment.rs`); Phase 6 supplies `--baseline ../.scafld/perf/oss-runtime-throughput-baseline.json` and the baseline file exists; Phase 7 retired-vocab rg now includes `--glob '!**/tests/**'` which excludes the legitimate guard at `crates/runx-runtime/tests/target_runner.rs:1531-1546`; Phase 5 names a dedicated `--test abnormal_seal` target and commits to adding the fixture as part of phase work. The HarnessContext god-object risk is resolved by explicitly dropping the bundle and passing facets directly. Boundary map has a stable home (`docs/rust-kernel-architecture.md` "Runtime buckets"). Fixture listing has a single source (`harness::fixtures::list_cases()`). Phase 4 manual rg moved to Review Notes as adversarial probe. Scope remains distinct from `monolith-decomposition-v1` and `runx-rust-95-release-readiness`; dirty files (`execution/runner/execution.rs`, `docs/runtime-throughput.md`) explicitly excluded. No new crates, pure crates stay pure. The architectural seams (services, adapter pipeline, authority algebra, lifecycle event owner, replay/stress separation) form a coherent lift that complements rather than duplicates the other active specs.

Checks:
- path audit
  - Grounded in: code:crates/runx-runtime/tests/payment.rs, code:crates/runx-runtime/src/bin/runx-harness-fixture-oracles.rs, code:.scafld/perf/oss-runtime-throughput-baseline.json, code:docs/rust-kernel-architecture.md, code:docs/how-we-test.md, code:docs/trusted-kernel-package-truth.md, code:scripts/runtime-throughput.mjs, code:crates/runx-core/tests/policy_proptest.rs
  - Result: passed
  - Evidence: Verified file existence for every in-scope path: tests (`payment.rs`, `receipt_signing.rs`, `receipt_tree.rs`, `journal_history.rs`, `harness_fixtures.rs`, `hello_graph.rs`, `cli_tool_contract.rs`, `external_adapter.rs`, `mcp_adapter.rs`, `mcp_server.rs`, `fanout_parity.rs`, `fanout_proptest.rs`, `policy_proptest.rs`), source files (`adapter.rs` with `SkillAdapter`/`invoke`, `sandbox.rs` with `mod tests` at line 1204, `credentials.rs` with `mod tests` at line 504, policy mod tree in `crates/runx-core/src/policy.rs`), scripts (`runtime-throughput.mjs`, `check-rust-crate-graph.mjs`, `runtime-adapter-oracle-checks.ts`), docs (`rust-kernel-architecture.md`, `how-we-test.md`, `trusted-kernel-package-truth.md`), and the throughput baseline at `/Users/kam/dev/runx/runx/.scafld/perf/oss-runtime-throughput-baseline.json`. The `abnormal_seal.rs` test does not exist yet — the spec explicitly commits to adding it as part of Phase 5 changes, which is the intentional future-file pattern.
- command audit
  - Grounded in: code:crates/runx-runtime/tests/payment.rs, code:scripts/runtime-throughput.mjs:63, code:crates/runx-runtime/tests/target_runner.rs:1531
  - Result: passed
  - Evidence: All round-1 command failures are fixed in the current draft. Phase 4 now uses `cargo test ... --test payment` (valid: `tests/payment.rs` is the integration-test binary that mods `payment::execution::*`). Phase 6 supplies `--baseline ../.scafld/perf/oss-runtime-throughput-baseline.json`; the script's `repoRoot = process.cwd()` resolution from `oss/` lands on the existing baseline file. Phase 7 rg includes `--glob '!**/tests/**'` which excludes the `assert_hard_cutover_vocabulary_only` guard at `crates/runx-runtime/tests/target_runner.rs:1531-1546`. Phase 2 acceptance commands target `sandbox::tests` and `credentials::tests` filters, both of which exist. Phase 4 includes a `--test policy_proptest` target that exists at `crates/runx-core/tests/policy_proptest.rs`. The Phase 2 zero-match rg for `RuntimeOptions::default(` currently has no matches in the runtime/CLI tree, so the negative-assertion gate is satisfiable.
- scope/migration audit
  - Grounded in: spec:.scafld/specs/drafts/runx-rust-runtime-architecture-lift-v1.md#scope, spec:.scafld/specs/drafts/runx-rust-runtime-architecture-lift-v1.md#dependencies
  - Result: passed
  - Evidence: Scope explicitly excludes broad large-file cleanup (owned by `monolith-decomposition-v1`), release readiness (owned by `runx-rust-95-release-readiness`), TS deletion (`rust-ts-sunset-*`), cloud worker architecture, new public contract names/schema IDs, and new Rust crates. Dirty files `execution/runner/execution.rs` and `docs/runtime-throughput.md` are explicitly excluded. No new crates; the spec defers any split until a dependency boundary is mechanically enforceable. Pure crates (`runx-contracts`, `runx-core`, `runx-parser`, `runx-receipts`, `runx-sdk`) are explicitly forbidden from gaining runtime/HTTP/MCP/process deps. The architecture rules require harness vocabulary and forbid reintroducing `work_item`/`engagement`/`judgment`/`operation as object`/`outcome` — consistent with the existing hard-cutover guard test at `tests/target_runner.rs:1531`. Cross-spec boundaries are respected throughout.
- acceptance timing audit
  - Grounded in: spec:.scafld/specs/drafts/runx-rust-runtime-architecture-lift-v1.md#phase5, spec:.scafld/specs/drafts/runx-rust-runtime-architecture-lift-v1.md#phase6
  - Result: passed
  - Evidence: Each phase's acceptance commands match its scope. Phase 1: `scafld validate` + `check-rust-crate-graph.mjs` + evidence summary (no compile required). Phase 2: receipt-signing, sandbox, credentials unit tests + targeted build + zero-match rg for `RuntimeOptions::default`. Phase 3: CLI tool/external/MCP adapter contract tests + supervisor rg gate. Phase 4: `runx-core policy` + `policy_proptest` + `runx-runtime payment` integration. Phase 5: receipts, receipt_tree, journal_history, harness_fixtures, hello_graph, abnormal_seal — the explicit `abnormal_seal` target now gates the abnormal-terminal-path invariant that round-1 flagged as ungated. Phase 6: fanout_parity, fanout_proptest, mcp_server, oracle script, throughput capture+check, verify:fast. Phase 7: fmt/clippy/build/typecheck/boundary/diff plus the retired-vocab rg. The abnormal_seal gating gap from round-1 is closed.
- rollback/repair audit
  - Grounded in: spec:.scafld/specs/drafts/runx-rust-runtime-architecture-lift-v1.md#rollback
  - Result: passed
  - Evidence: Rollback is credible: each phase reverts independently and each phase preserves public wire formats unless an external contract spec says otherwise. Three named failure modes have concrete narrowing rules: god-object service abstraction → split by service facet; adapter pipeline distortion → keep shared supervisor and defer broader conversion; authority-algebra payment regression → preserve payment behavior and narrow shared primitive set. The spec's `Do not add compatibility aliases` rule means rollback cannot accidentally bifurcate the implementation. Repair via incremental phase revert is feasible because Phase 1 builds an explicit boundary map under `docs/rust-kernel-architecture.md` that survives a phase revert.
- design challenge
  - Grounded in: code:crates/runx-runtime/src/adapter.rs:55, code:crates/runx-runtime/src/execution/runner.rs:54, spec:.scafld/specs/drafts/runx-rust-runtime-architecture-lift-v1.md#phase2
  - Result: passed
  - Evidence: The lift is the right architectural move, not bandaid or bloat. Today raw env, `RUNX_*` names, receipt dirs, sandbox plans, and process commands thread through unrelated layers (`SkillInvocation` carries `BTreeMap<String,String>` env and `skill_dir`/`source` jointly; cli_tool/external_adapter/mcp transport each own pieces of spawn/terminate/capture). Naming four small service facets (`WorkspaceEnv`, `ReceiptServices`, `SandboxServices`, `AdapterServices`) without introducing a new crate gives the existing plumbing typed owners and keeps the change inside the side-effect tier. Round-1's god-object risk is now addressed: the draft explicitly drops `HarnessContext` and the Review Notes call out this revision so reviewers can adversarially probe re-introduction attempts. The lift complements rather than duplicates `monolith-decomposition-v1` (file-size) and `runx-rust-95-release-readiness` (gates).

Issues:
- [low/advisory] `harden-1` acceptance timing audit - Phase 6 throughput capture+check writes and then reads the same baseline path, weakening regression-detection value.
  - Status: open
  - Grounded in: code:scripts/runtime-throughput.mjs:49, spec:.scafld/specs/drafts/runx-rust-runtime-architecture-lift-v1.md#phase6
  - Evidence: Phase 6 acceptance runs `runtime-throughput.mjs capture --output ../.scafld/perf/oss-runtime-throughput-baseline.json` immediately followed by `runtime-throughput.mjs check --baseline ../.scafld/perf/oss-runtime-throughput-baseline.json`. The script's `capture` overwrites the baseline file, and `check` then re-captures and compares against the just-written file — so the comparison is essentially against itself. This verifies the perf tooling runs but does not detect regressions versus a historical committed baseline.
  - Recommendation: Either (a) split the commands across phases (capture in release-readiness, check here against the committed baseline only), or (b) document explicitly in the spec that Phase 6's capture+check is a tooling smoke test and that real regression detection is owned by a separate gate. Minimum: add a note alongside the acceptance commands clarifying the intent.
  - Question: Is the intent of Phase 6's capture+check pair to smoke-test the perf tooling, or to detect regressions against the committed baseline?
  - Recommended answer: It is a tooling smoke test. Add a one-line comment in Phase 6 acceptance clarifying that real regression detection against a committed baseline lives in release-readiness.
  - If unanswered: Default to adding a comment that Phase 6 capture+check verifies the gate runs, and that committed-baseline regression detection is owned elsewhere.
- [low/advisory] `harden-2` spec_gap - Phase 4 names `runx-core::policy` as the home for shared authority primitives but does not pick a specific module (new vs. existing).
  - Status: open
  - Grounded in: spec:.scafld/specs/drafts/runx-rust-runtime-architecture-lift-v1.md#phase4, code:crates/runx-core/src/policy.rs:1
  - Evidence: Phase 4 says 'Extract shared authority primitives in `runx-core::policy` for subset, equality, resource-family comparison, actor/principal narrowing, time bounds, approval requirements, and capability consumption.' The existing `policy.rs` declares `authority_proof`, `credential_grant`, `graph_scope`, `interpreter`, `local`, `maturity`, `payment_authority`, `public_work`, `retry`, `rfc3339`, `sandbox`, `scope`, `types`. None of these obviously owns the shared subset/attenuation primitives; the new code could land in `authority_proof`, a new `authority_algebra` module, or `types`. Reviewers cannot verify boundary cleanliness without the chosen home.
  - Recommendation: Name the target module(s) in Phase 4 (e.g., extend `authority_proof` or add a new `policy::authority_algebra` module). The choice does not need to be final at harden time, but should narrow to one or two named candidates.
  - Question: Should the shared authority algebra primitives extend `policy::authority_proof`, land in a new `policy::authority_algebra` module, or live in `policy::types`?
  - Recommended answer: Add a new `policy::authority_algebra` module so the primitives have a focused home and the algebra/property tests have a discoverable surface.
  - If unanswered: Default to introducing `policy::authority_algebra` as the canonical home; refactor in a follow-up if the module shape proves wrong.

## Review

Status: completed
Verdict: pass
Mode: verify
Summary: Human-reviewed override accepted: Claude review diagnostic passed with no completion blockers; provider failed only because it did not submit the structured dossier after a stale text-only directive.

Attack log:
- `review gate`: manual human audit -> clean (Claude review diagnostic passed with no completion blockers; provider failed only because it did not submit the structured dossier after a stale text-only directive.)

Findings:
- none

