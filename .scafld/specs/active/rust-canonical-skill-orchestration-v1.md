---
spec_version: '2.0'
task_id: rust-canonical-skill-orchestration-v1
created: '2026-05-21T04:35:09Z'
updated: '2026-05-21T04:35:09Z'
status: active
harden_status: not_run
size: large
risk_level: very_high
---

# Rust canonical skill orchestration v1

## Current State

Status: active
Current phase: phases 0-2 first slice
Next: inline harness expansion or TypeScript importer reroute
Reason: Skill orchestration is the critical cutover boundary. Rust now owns
parts of runtime execution, payment authority, receipts, history, doctor, MCP,
skill run, and harness replay, but TypeScript still owns or masks too much of
the local product orchestration through `@runxhq/runtime-local`,
`@runxhq/adapters`, package dogfood scripts, CLI wrappers, composer flows, and
many tests. The standalone Rust CLI must be useful on a machine with no Node,
pnpm, tsx, or TypeScript packages installed.
Blockers: CLI package importer reroute remains open; `packages/cli/src/**`
still imports `@runxhq/runtime-local` and `@runxhq/adapters` for local
execution commands. Native inline harness expansion from `skill-dir|SKILL.md`
is not implemented and must stay out of help until it is Rust-native.
Allowed follow-up command: `scafld harden rust-canonical-skill-orchestration-v1`
Latest runner update: 2026-05-21 - docs boundary repaired, Rust harness help
made truthful, TS-free native CLI smoke added, canonical Rust orchestrator
surface introduced, and official `fixtures/skills` Node/tsx dependencies moved
to checked-in shell or Python fixtures.
Review gate: not_started

## Summary

Make Rust the canonical local skill orchestrator and make TypeScript a client
surface over Rust, not an alternate trusted runtime.

The end state is:

`TS may request. Rust admits. Rust orchestrates. Rust seals. TS displays.`

That means the native Rust CLI must run the accepted local workflows without
TypeScript installed. TypeScript can continue to provide npm packaging, SDKs,
composer/product UX, marketplace tooling, and compatibility tests, but it must
not be the only place where skill execution, graph execution, approval
resolution, retry/fanout/idempotency, receipt sealing, payment authority,
payment recovery, or harness replay is implemented.

This is a boundary/specification plan, not an immediate deletion spec. It
defines the proper code shape and the acceptance gates that must be satisfied
before `rust-ts-sunset-runtime-local` can complete safely.

## Boundary Contract

### Rust Owns

- Canonical CLI command parsing, help, exit codes, and JSON envelopes for local
  `runx` commands.
- Skill package loading from `SKILL.md` plus `X.yaml`, runner selection, input
  hydration, environment policy, and continuation via answer files.
- Graph execution: step planning, context edges, transitions, approval gates,
  retries, fanout, sync points, idempotency keys, and failure classification.
- Harness replay for standalone fixture files and inline skill/graph harness
  cases if those forms are advertised by `runx --help`.
- Adapters that affect trusted execution: `cli-tool`, `agent`, `a2a`, `mcp`,
  catalog/tool resolution, and offline deterministic fixture adapters.
- Receipt store, receipt sealing, receipt proof verification, history
  projection, and all redaction/public-message guarantees.
- Authority algebra and policy admission, including payment charge/pay/refund
  verbs, subset proofs, proof kinds, receipt-before-forward, recovery, and
  same-family constraints.
- Payment rail recovery state machines and deterministic offline mock/x402/
  Stripe SPT eventuality simulators used for dogfood.
- Public Rust APIs consumed by SDKs and wrappers for the above.

### TypeScript Owns

- npm package launcher and native-binary selection.
- Developer-facing wrappers that spawn the Rust binary or call stable Rust
  host-protocol/JSON APIs.
- TypeScript SDK and product/composer UX that requests local runs.
- Marketplace/package authoring tooling, scafld workflows, docs generation,
  and compatibility tests.
- Published TypeScript contract views that mirror `runx-contracts`.
- Cloud-hosted product services unless a separate cloud cutover spec moves
  them.

### TypeScript Must Not Own

- Any trusted admission decision.
- Any payment authority, spend, refund, dispute, recovery, or proof-sealing
  invariant.
- Any canonical receipt or history projection.
- Any only-implementation of local skill/graph/harness execution.
- Any hidden fallback for Rust CLI commands after the command is advertised as
  native.

## Current Boundary Violations

These are known violations to repair or explicitly retire:

- `README.md` says `@runxhq/runtime-local` owns local orchestration, caller
  interaction, sandbox preparation, receipt-write orchestration, harness
  execution, runtime SDK entrypoints, MCP client behavior, and tool catalogs.
  That contradicts the desired Rust canonical boundary.
- `docs/rust-kernel-architecture.md` still contains conformance-first language
  saying TypeScript remains authoritative. That is no longer true for local
  orchestration once this spec is accepted.
- `docs/api-surface.md` still publishes runtime-local as a local orchestration
  package rather than a sunset client/wrapper surface.
- `runx --help` advertises `runx harness <fixture.yaml|skill-dir|SKILL.md>`,
  but the Rust binary currently accepts fixture files only; `runx harness
  skills/stripe-pay --json` fails by trying to read the directory as YAML.
- Some native harness fixtures still depend on `node -e`; those are not valid
  standalone Rust CLI dogfood fixtures.
- `scripts/dogfood-core-skills.mjs` is currently the top-level dogfood queue.
  It can remain as a wrapper, but it must not be the canonical proof that the
  Rust CLI works without TS.
- Many tests and package sources still import `@runxhq/runtime-local` or
  `@runxhq/adapters`; those importers block the runtime-local sunset and blur
  ownership.
- Recent payment/x402 Rust tests prove kernel behavior but do not by themselves
  expose every accepted payment dogfood scenario as a TS-free CLI workflow.

## Target Code Shape

### Rust Runtime

Create or consolidate a public orchestration boundary under
`crates/runx-runtime/src/execution/`:

- `orchestrator.rs`: canonical entrypoint for skill, graph, and harness
  execution.
- `skill_run.rs`: remains as the CLI skill surface but delegates execution
  semantics to the orchestrator rather than growing a second path.
- `runner/`: graph state machine integration, context forwarding, approval,
  retry, fanout, authority, and step execution.
- `harness/`: fixture loading, inline skill/graph harness case expansion,
  assertion, and receipt proof validation.
- `adapters/`: Rust implementations for `cli-tool`, `agent`, `a2a`, `mcp`,
  catalog/tool resolution, and deterministic fixture adapters.
- `payment/` or `policy/payment_runtime.rs`: provider-neutral payment rail
  contract and recovery eventuality model used by mock/x402/Stripe SPT offline
  scenarios.

The Rust orchestrator should expose stable request/result types:

- `SkillRunRequest`, `GraphRunRequest`, `HarnessRunRequest`.
- `RunContinuation` for answer-file/approval continuation.
- `RunResult` containing status, receipt refs, child receipt refs, pending
  requests, and public-safe diagnostics.
- `RunStatus` with canonical values aligned to CLI exit codes.

No TypeScript package type should be required to construct these values.

### Rust CLI

`crates/runx-cli` is the canonical local command surface:

- Help text must only advertise forms that are implemented and tested in Rust.
- `runx skill <skill-dir|SKILL.md>` must work without Node for agent-step
  request/answer flows and for Rust-native fixture adapters.
- `runx harness <fixture.yaml>` must remain Rust-native.
- If `runx harness <skill-dir|SKILL.md>` remains in help, it must expand the
  inline `X.yaml` harness cases natively. Otherwise remove that advertised form
  until implemented.
- `runx doctor`, `runx list`, `runx history`, `runx policy`, `runx kernel`,
  `runx config`, `runx tool`, `runx registry`, `runx mcp`, and `runx dev`
  must not shell out to Node for canonical local behavior.

### TypeScript Packages

TypeScript should route to Rust through stable boundaries:

- `packages/cli`: Node selector/package UX only; local execution commands
  delegate to the native binary or consume Rust JSON APIs.
- `packages/runtime-local`: no new features; slated for deletion after
  importers route through Rust.
- `packages/adapters`: no new trusted behavior; slated for deletion after Rust
  adapters cover callable surfaces.
- SDK packages: call stable Rust CLI/host-protocol contracts; do not re-host
  local execution semantics.
- Tests: compatibility tests may use TS, but every trusted local behavior must
  have a Rust test or a TS-free Rust CLI fixture.

## Scope And Touchpoints

In scope:

- `.scafld/specs/drafts/rust-canonical-skill-orchestration-v1.md`
- `.scafld/specs/drafts/rust-ts-sunset-runtime-local.md`
- `docs/ts-interop-boundary.md`
- `docs/rust-kernel-architecture.md`
- `docs/api-surface.md`
- `docs/cli-exit-codes.md`
- `README.md`
- `crates/runx-runtime/README.md`
- `crates/runx-runtime/src/execution/**`
- `crates/runx-runtime/src/adapters/**`
- `crates/runx-runtime/src/receipts/**`
- `crates/runx-runtime/src/journal/**`
- `crates/runx-cli/src/**`
- Rust CLI/runtime tests under `crates/runx-cli/tests/**` and
  `crates/runx-runtime/tests/**`
- Current payment/x402 specs that depend on dogfood proof:
  `x402-pay-paid-echo-composer-v1`,
  `x402-pay-stripe-spt-dogfood-v1`, and the mock-scenario punch list.
- TS wrapper tests only when they are changed to spawn/consume Rust, not to
  reimplement local orchestration.

Out of scope:

- Cloud-hosted service runtime migration.
- Adding legacy aliases or compatibility command names.
- Live-money Stripe or crypto behavior.
- Keeping a TypeScript runtime-local facade as a long-term compatibility layer.
- Introducing a second Rust execution path that bypasses the orchestrator.

## Planned Phases

Phase 0: boundary inventory and docs repair.
: Update docs to say Rust is canonical for local orchestration, TypeScript is a
client/wrapper, and `runtime-local`/`adapters` are sunset surfaces. Add an
importer census and command-surface truth table.

Phase 1: help truth and TS-free native smoke.
: Make `runx --help` match implemented Rust behavior exactly. Add TS-free smoke
tests for `runx doctor`, `runx list`, `runx history`, `runx skill`, and
`runx harness` using the native binary with no Node/pnpm/tsx dependency.

Phase 2: canonical Rust orchestrator surface.
: Consolidate skill/graph/harness execution behind one Rust orchestrator API.
Remove duplicated execution semantics between `skill_run`, graph runner, and
harness replay. Make answer-file continuation, approval, retries, and receipt
writing flow through this API.

Phase 3: inline harness and official fixture cutover.
: Either implement native `runx harness <skill-dir|SKILL.md>` for `X.yaml`
inline harness cases or remove the advertised form. Replace official local
dogfood fixtures that use `node -e` with Rust-native fixtures or checked-in
CLI-tool scripts that are not Node-dependent for core proof.

Phase 4: payment/x402 dogfood promotion.
: Promote payment mock, paid-echo, x402, and Stripe SPT offline eventualities
from in-memory Rust tests into CLI-runnable Rust fixtures. Keep TS composer and
dogfood scripts as wrappers only.

Phase 5: TypeScript importer reroute.
: Route `packages/cli`, IDE, langchain, and remaining tests through the Rust
binary/JSON/host-protocol boundary. Remove `@runxhq/runtime-local` and
`@runxhq/adapters` imports from all non-sunset package sources.

Phase 6: runtime-local/adapters deletion readiness.
: Re-run the `rust-ts-sunset-runtime-local` importer census. Delete the
packages only after Rust CLI/runtime acceptance is green and docs/API surfaces
no longer publish runtime-local as an execution owner.

## Acceptance

Profile: strict

Definition of done:
- [ ] `dod1` command - Docs state one clear boundary: Rust is canonical for local skill,
  graph, harness, receipt, history, policy, and payment orchestration; TS is a
  wrapper/client surface.
  - Command: `! rg -n '@runxhq/runtime-local.*owns local orchestration|runtime-local.*local runtime: orchestration|TypeScript remains authoritative' README.md docs crates/runx-runtime/README.md`
- [ ] `dod2` command - Native `runx --help` advertises only Rust-implemented command
  forms, with tests proving each advertised form.
  - Command: `crates/target/debug/runx --help && cargo test --quiet --manifest-path crates/Cargo.toml -p runx-cli --test launcher`
- [ ] `dod3` command - The Rust CLI has a TS-free smoke suite that runs without Node,
  pnpm, tsx, or workspace TypeScript packages.
  - Command: `env -i PATH="$PATH" crates/target/debug/runx doctor --json && env -i PATH="$PATH" crates/target/debug/runx list skills --json && env -i PATH="$PATH" crates/target/debug/runx harness fixtures/harness/payment-approval-graph.yaml --json`
- [ ] `dod4` command - All trusted local orchestration flows enter through the Rust
  orchestrator API.
  - Command: `rg -n 'pub (struct|enum) (SkillRunRequest|GraphRunRequest|HarnessRunRequest|RunContinuation|RunResult|RunStatus)' crates/runx-runtime/src/execution && cargo test --quiet --manifest-path crates/Cargo.toml -p runx-runtime --tests`
- [ ] `dod5` command - Official payment/x402 dogfood eventualities have Rust-native
  CLI-runnable fixtures, not just TS wrappers or in-memory unit tests.
  - Command: `cargo test --quiet --manifest-path crates/Cargo.toml -p runx-runtime --tests && ! rg -n 'command: node|node -e|tsx|pnpm' fixtures/harness fixtures/graphs fixtures/skills`
- [ ] `dod6` command - `packages/cli/src/**` has no `@runxhq/runtime-local` or
  `@runxhq/adapters` imports for local execution commands.
  - Command: `! rg -n '@runxhq/(runtime-local|adapters)' packages/cli/src --glob '!**/*.test.ts'`
- [ ] `dod7` command - `rust-ts-sunset-runtime-local` is unblocked by a refreshed importer
  census and no docs/API page still presents runtime-local as canonical local
  orchestration.
  - Command: `scafld validate rust-ts-sunset-runtime-local --json && ! rg -n '@runxhq/runtime-local.*owns local orchestration|runtime-local.*local runtime: orchestration|TypeScript remains authoritative' README.md docs crates/runx-runtime/README.md`

Validation:
- [ ] `v1` command - This spec validates.
  - Command: `scafld validate rust-canonical-skill-orchestration-v1 --json`
  - Expected kind: `exit_code_zero`
  - Timeout seconds: none
  - Result: none
  - Status: pending
  - Evidence: none
  - Source event: none
  - Last attempt: none
  - Checked at: none
- [ ] `v2` command - Native Rust CLI works without TS runtime.
  - Command: `env -i PATH="$PATH" crates/target/debug/runx doctor --json && env -i PATH="$PATH" crates/target/debug/runx list skills --json && env -i PATH="$PATH" crates/target/debug/runx harness fixtures/harness/payment-approval-graph.yaml --json`
  - Expected kind: `exit_code_zero`
  - Timeout seconds: none
  - Result: none
  - Status: pending
  - Evidence: none
  - Source event: none
  - Last attempt: none
  - Checked at: none
- [ ] `v3` command - Every advertised `runx harness` form works or is not
  advertised.
  - Command: `crates/target/debug/runx --help && cargo test --quiet --manifest-path crates/Cargo.toml -p runx-cli --test launcher`
  - Expected kind: `exit_code_zero`
  - Timeout seconds: none
  - Result: none
  - Status: pending
  - Evidence: none
  - Source event: none
  - Last attempt: none
  - Checked at: none
- [ ] `v4` command - Canonical local execution tests pass.
  - Command: `cargo test --quiet --manifest-path crates/Cargo.toml -p runx-runtime --tests`
  - Expected kind: `exit_code_zero`
  - Timeout seconds: none
  - Result: none
  - Status: pending
  - Evidence: none
  - Source event: none
  - Last attempt: none
  - Checked at: none
- [ ] `v5` command - CLI source no longer imports runtime
  execution from TS sunset packages.
  - Command: `! rg -n '@runxhq/(runtime-local|adapters)' packages/cli/src --glob '!**/*.test.ts'`
  - Expected kind: `exit_code_zero`
  - Timeout seconds: none
  - Result: none
  - Status: pending
  - Evidence: none
  - Source event: none
  - Last attempt: none
  - Checked at: none
- [ ] `v6` command - Trusted dogfood fixtures do not
  require Node for core proof.
  - Command: `! rg -n 'command: node|node -e|tsx|pnpm' fixtures/harness fixtures/graphs fixtures/skills`
  - Expected kind: `exit_code_zero`
  - Timeout seconds: none
  - Result: none
  - Status: pending
  - Evidence: none
  - Source event: none
  - Last attempt: none
  - Checked at: none
- [ ] `v7` command - Docs no longer name runtime-local as canonical
  local orchestration.
  - Command: `! rg -n '@runxhq/runtime-local.*owns local orchestration|runtime-local.*local runtime: orchestration|TypeScript remains authoritative' README.md docs crates/runx-runtime/README.md`
  - Expected kind: `exit_code_zero`
  - Timeout seconds: none
  - Result: none
  - Status: pending
  - Evidence: none
  - Source event: none
  - Last attempt: none
  - Checked at: none

## Sequencing Rules

- Do not delete `packages/runtime-local` or `packages/adapters` until Phases 0
  through 5 pass.
- Do not add new TypeScript execution behavior for local skills, graphs,
  harnesses, or payments. New TS code must call Rust or remain purely
  presentational.
- Do not add compatibility aliases. Clean cutover means one canonical command
  spelling and one trusted execution owner.
- Do not count a TS wrapper test as proof unless the same behavior is proven by
  Rust tests or a TS-free Rust CLI fixture.
- If a Rust command cannot support a form currently advertised in help, fix the
  command or remove the advertised form before claiming cutover progress.

## Rollback

Strategy: per_phase

Commands:
- Phase 0 docs rollback: `git checkout HEAD -- README.md docs/ts-interop-boundary.md docs/rust-kernel-architecture.md docs/api-surface.md docs/cli-exit-codes.md crates/runx-runtime/README.md`
- Runtime rollback: revert the specific `crates/runx-runtime/src/execution/**`,
  `crates/runx-runtime/src/adapters/**`, and `crates/runx-cli/src/**` files
  touched by the phase.
- TS routing rollback: restore the previous wrapper/importer files only if the
  Rust command is also removed from help and the runtime-local sunset spec is
  marked blocked again.

## Harden Rounds

- none

## Planning Log

- 2026-05-21T04:35:09Z: Filed after boundary review found that skill
  orchestration remains partially TypeScript-owned, docs still contradict the
  Rust canonical target, and the Rust CLI must be useful without TypeScript
  installed.
