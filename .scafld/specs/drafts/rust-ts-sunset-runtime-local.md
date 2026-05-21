---
spec_version: '2.0'
task_id: rust-ts-sunset-runtime-local
created: '2026-05-18T00:00:00Z'
updated: '2026-05-21T05:52:03Z'
status: draft
harden_status: not_run
size: large
risk_level: very_high
---

# TS sunset: runtime-local

## Current State

Status: draft
Current phase: none
Next: smaller importer specs, not package deletion
Reason: refreshed after the CLI importer completion and a small non-CLI
contract fixture generator cleanup. This remains a deletion/cutover spec, not
a compatibility-bridge spec. A 2026-05-21 exact-package census finds 117
active files outside `.scafld/specs/**` and `dist/**` with
runtime-local/adapters package references, imports, direct source paths, docs,
fixtures, or package-resolution entries. Of those, 93 are outside the two
packages being deleted and 66 are actual package import files outside those
packages. `packages/cli/src/**` has zero exact runtime-local/adapters package
references in this tree. Blockers: not currently executable. `rust-harness`,
`rust-runtime-skill-execution`, `rust-runtime-adapters-agent`,
`rust-runtime-adapters-a2a`, `rust-runtime-adapters-catalog`,
`rust-runtime-adapters-mcp`, and `rust-mcp-server-harness-receipt-seal` are
completed or archived completed. The remaining blockers are importer/routing
blockers: IDE core, langchain, package manifests, path aliases, vitest aliases,
oracle scripts, active docs/fixtures, Rust parity/doctor references, and many
tests still reference `@runxhq/runtime-local`, `@runxhq/adapters`, or their
package paths. Host-adapters and CLI source no longer import
runtime-local/adapters in the current tree. All surviving local callers must be
Rust-routed or explicitly sunset before deletion starts.
Allowed follow-up command: `none`
Latest runner update: 2026-05-21T15:52:10+10:00 importer census refreshed;
CLI source package references are zero; contract fixture generation no longer
imports `packages/runtime-local/src/sdk/act-assignment.js`; deletion remains
blocked by live non-CLI importers, tests, scripts, and package-resolution
entries.
Review gate: not_started

## Summary

Delete `packages/runtime-local/` and `packages/adapters/` in one coordinated
end-state sunset. This is not currently executable: the workspace still has
active package dependencies, TS path aliases, source imports, and test imports
for both packages. The package dependency direction is mostly
`@runxhq/adapters` -> `@runxhq/runtime-local`: the adapters package imports
runtime-local SDK, sandbox, MCP, harness, and tool-catalog helpers. The
remaining tests and TS package importers consume both packages together by
constructing adapters and passing them into runtime-local execution. Both
retire only after adapter behavior lands and is routed through
`runx-runtime::adapters::{cli_tool, agent, catalog, a2a, mcp}` and surviving
callers use stable Rust/contract/CLI boundaries.

The replacement execution model is Rust runtime plus the ratified harness
spine. Skill execution is expressed as a harness run: decisions and acts are
contained in the harness node, receipts are sealed harness nodes, and graph
execution is represented by parent/child harness receipt references. This spec
does not preserve a TypeScript compatibility bridge, a `runtime-local` shim, or
legacy receipt/object vocabularies.

This is the last big rip. After it lands, the Rust takeover is complete
for OSS purposes; cloud-side hosted surfaces (`agent-runner`, `worker`,
`api`, `auth`) remain TS unless and until separate cloud cutover specs
target them. The disposition of every remaining TS package is documented
in `rust-ts-interop-boundary`.

Current reality as of this refresh:
- Completed prerequisites: `rust-harness`, `rust-runtime-skill-execution`,
  `rust-runtime-adapters-agent`, `rust-runtime-adapters-a2a`, archived
  `rust-runtime-adapters-catalog`, archived `rust-runtime-adapters-mcp`, and
  archived `rust-mcp-server-harness-receipt-seal`.
- The former MCP blocker is closed: `rust-mcp-server-harness-receipt-seal`
  is archived completed with review gate pass and acceptance evidence for
  `cargo test --manifest-path crates/Cargo.toml -p runx-runtime mcp_server
  --features mcp -- --nocapture`, harness tests, receipt proof tests, and MCP
  clippy. Runtime-local deletion is now blocked by importer/routing work, not
  by the MCP server single-skill receipt proof.
- Rust adapter files exist behind feature gates under
  `crates/runx-runtime/src/adapters/{cli_tool,agent,a2a,catalog,mcp}.rs`, but
  feature-gated files alone are not deletion evidence; routing and importer
  removal are required.
- Root package metadata, IDE core, langchain, scripts, Rust parity/doctor
  references, docs/API surface, active fixtures, package path aliases, vitest
  aliases, the runtime-local/adapters packages themselves, and many tests still
  import or refer to `@runxhq/runtime-local`, `@runxhq/adapters`, or their
  package paths.
- Update: the CLI source package-import blocker is closed in the current tree.
  `packages/cli/src/**` has zero exact `@runxhq/runtime-local`,
  `@runxhq/adapters`, `packages/runtime-local`, or `packages/adapters`
  references. Do not use this spec to take the parent-owned CLI dead-command
  cleanup.
- Update: `scripts/generate-rust-contract-fixtures.ts` no longer imports the
  runtime-local act-assignment SDK helper; it now uses `@runxhq/contracts` for
  validation and `@runxhq/core/util` for stable hashing.
- `packages/host-adapters/**` no longer has a runtime-local/adapters
  dependency or import in the current tree.
- The refreshed 2026-05-21 exact-package census finds 117 active files outside
  `.scafld/specs/**` and `dist/**` with runtime-local/adapters references; 93
  of those are outside `packages/runtime-local/**` and `packages/adapters/**`.
- Existing TS oracle generators remain useful before deletion. They are not
  a post-sunset execution path.

## Context

CWD: `.`

Packages:
- `@runxhq/runtime-local`
- `@runxhq/adapters`
- `crates/runx-runtime`
- Every TS importer of `@runxhq/runtime-local` or `@runxhq/adapters`

Current TypeScript sources:
- `packages/runtime-local/**` (to be deleted)
- `packages/adapters/**` (to be deleted)

Files impacted:
- `packages/runtime-local/` (deleted)
- `packages/adapters/` (deleted)
- `pnpm-workspace.yaml` (remove workspace members)
- Every TS file currently importing from `@runxhq/runtime-local` or
  `@runxhq/adapters`
- Fixture-generation scripts retired by this sunset:
  `scripts/generate-rust-fanout-fixtures.ts`,
  `scripts/generate-rust-skill-fixtures.ts`, adapter oracle generators whose
  Rust specs no longer need the TS oracle, and any runtime-local-only
  approval/harness oracle scripts that have no surviving Rust consumer

Current importer and blocker inventory:
- Census command used for this refresh, from repo root:

  ```sh
  rg -l "@runxhq/(runtime-local|adapters)|@runxhq/runtime-local|@runxhq/adapters|packages/(runtime-local|adapters)" --glob '!node_modules/**' --glob '!dist/**' --glob '!.scafld/specs/**' | sort
  ```

- 2026-05-21 refreshed exact-package active reference totals:
  - 117 active files outside `.scafld/specs/**` and `dist/**`.
  - 93 active files outside the two packages being deleted.
  - 81 files with actual package import statements under
    `packages/**`, `plugins/**`, `tests/**`, and `scripts/**`.
  - 66 actual package import files outside `packages/runtime-local/**` and
    `packages/adapters/**`.
- 2026-05-21 refreshed exact-package prefix census:
  - `README.md`: 1 file.
  - `crates/runx-runtime`: 2 files.
  - `docs`: 3 files.
  - `fixtures/cli-parity`: 1 file.
  - `fixtures/rust-cli-cutover-negative`: 1 file.
  - root `package.json`: 1 file.
  - `packages/adapters`: 12 files.
  - `packages/langchain`: 3 files.
  - `packages/runtime-local`: 12 files.
  - `plugins/ide-core`: 2 files.
  - `pnpm-lock.yaml`: 1 file.
  - `scripts`: 9 files.
  - `skills/write-harness`: 1 file.
  - `tests`: 66 files.
  - `tsconfig.base.json`: 1 file.
  - `vitest.workspace-aliases.ts`: 1 file.
- Package manifests and path aliases:
  - Root `package.json` devDependencies on `@runxhq/adapters` and
    `@runxhq/runtime-local`.
  - `packages/cli/package.json` no longer declares either dependency, and
    `packages/cli/src/**` has zero exact runtime-local/adapters references in
    the current tree.
  - `plugins/ide-core/package.json` depends on `@runxhq/adapters`.
  - `packages/langchain/package.json` depends on `@runxhq/runtime-local`.
  - `packages/host-adapters/package.json` no longer references either package.
  - `packages/adapters/package.json` depends on `@runxhq/runtime-local`.
  - `tsconfig.base.json` aliases `@runxhq/adapters`, subpaths,
    `@runxhq/runtime-local`, and subpaths.
  - `vitest.workspace-aliases.ts` mirrors the same aliases for tests.
  - `pnpm-lock.yaml` still records the root, adapters, langchain, runtime-local,
    and ide-core links.
  - `pnpm-workspace.yaml` does not name the packages directly, but
    `packages/*` still includes both package directories until deletion.
- Surviving package source imports:
  - `packages/cli/src/**` is no longer a package-import blocker; the CLI
    importer routing slice is archived completed. Leave parent-owned CLI
    dead-command cleanup out of this spec.
  - `plugins/ide-core/src/actions.ts` imports adapters and runtime-local
    harness/SDK surfaces.
  - `packages/langchain/src/**` imports runtime-local SDK, tool-catalogs, and
    result types and must move to CLI JSON or another boundary ratified by
    `rust-ts-interop-boundary`.
  - `packages/adapters/src/**` imports runtime-local MCP, sandbox, harness,
    SDK, and tool-catalog helpers; these imports confirm adapters depend on
    runtime-local rather than runtime-local depending on adapters.
  - `packages/runtime-local/src/**` still has self-referential public package
    exports and subpath imports; these are deleted with the package, not routed.
- Rust-side direct package-path references:
  - `crates/runx-runtime/src/adapters/catalog.rs` pins the hash of
    `packages/runtime-local/src/harness/mcp-fixture.ts` for catalog/MCP oracle
    metadata.
  - `crates/runx-runtime/src/doctor.rs` has a file-budget probe for
    `packages/runtime-local/src/runner-local/index.ts`.
- Test importers:
  - 66 files under `tests/**` still import package APIs or direct package
    source paths. They cover graph runner/fanout/governance/registry refs,
    local skill runner, sourcey, scafld, issue-to-pr, issue-intake, MCP, A2A,
    approval, auth, history, receipt inspection, package resolution/profile,
    CLI tool policy/sandbox, host protocol, SDK imported tools, and framework
    bridge coverage.
  - SDK/host tests include host protocol, imported tools, framework bridge,
    caller approval boundary, and IDE action coverage.
  - Adapter tests under `packages/adapters/**` remain with the package until
    deleted or converted to durable Rust fixture coverage.
- Active docs, fixtures, and skills references:
  - `README.md`, `docs/api-surface.md`, `docs/rust-kernel-architecture.md`, and
    `docs/ts-interop-boundary.md` still describe or expose runtime-local or
    adapters surfaces.
  - `fixtures/cli-parity/runtime-surfaces.json`,
    `fixtures/rust-cli-cutover-negative/legacy-v2-package-candidate/package.json`,
    and `skills/write-harness/SKILL.md` still name package paths or package
    dependencies.
- Fixture/oracle scripts currently present:
  - `scripts/generate-rust-harness-fixtures.ts` has a surviving Rust harness
    consumer and is not automatically deleted by this sunset.
  - `scripts/generate-rust-skill-fixtures.ts` is tied to completed product
    skill execution evidence and is retired only when fixtures no longer need
    TS oracle regeneration.
  - `scripts/generate-a2a-adapter-fixtures.ts`,
    `scripts/generate-agent-adapter-fixtures.ts`, and
    `scripts/generate-runtime-catalog-adapter-oracles.ts` are retained until
    their completed adapter specs declare the checked-in Rust fixtures durable.
  - `scripts/generate-runtime-mcp-oracles.ts` exists and imports the TS MCP
    adapter; after the MCP receipt-seal completion it remains an oracle
    ownership decision, not a receipt-proof blocker.
  - `scripts/generate-rust-contract-fixtures.ts` has moved off the
    runtime-local act-assignment SDK helper and is no longer a blocker for this
    sunset.
  - `scripts/dogfood-github-issue-to-pr.mjs` still imports adapters and
    runtime-local directly and must route through the Rust CLI or be sunset.
  - `scripts/generate-cli-feature-parity.ts`, `scripts/check-boundaries.mjs`,
    `scripts/check-rust-cli-cutover-negative.mjs`, and
    `scripts/check-rust-cli-release-artifacts.ts` still encode package names or
    direct package path expectations and must be updated as part of the final
    package-boundary cleanup.

Invariants:
- Every importer either re-routed to Rust (via CLI subprocess, in-process
  binding, or `runx-runtime-service` daemon) or is itself sunset.
- The cloud `agent-runner` package has a stable boundary against the Rust
  runtime (settled in `rust-aster-runtime-cutover`).
- No compatibility package, shim, alias, or v2 surface remains for
  `@runxhq/runtime-local` or `@runxhq/adapters` inside this workspace.
- No active runtime-local replacement code, fixture, receipt assertion, schema
  projection, or docs page introduced by this spec uses retired peer
  execution-object keys.
- Skill names remain canonical. Do not introduce aliases such as
  `issue-control`, `runtime-local-v2`, or any v2 package name to mask the
  sunset. `issue-to-pr` and `issue-intake` are legitimate product skill names
  and must not be flagged or renamed by vocabulary checks. Existing product
  packet names such as `runx.issue_to_pr_outcome.v1` are not runtime-local
  compatibility aliases.
- The only accepted runtime-local replacement for skill execution is a Rust
  harness run that emits and verifies canonical sealed harness receipts with
  contained decision payloads, contained act payloads, child harness receipt
  refs, proof status, and verification checks.
- Historical archived specs may mention old terms, but active specs, fixtures,
  package manifests, and runtime-local replacement docs must not depend on
  them.

## Objectives

- Enumerate every importer and classify it as Rust-routed, sunset, or surviving
  stable boundary.
- Verify migration for each importer without falling back to runtime-local.
- Prove real product skill execution through Rust harness runs, not a TS oracle
  or compatibility object.
- Delete `packages/adapters/`.
- Delete `packages/runtime-local/`.
- Retire runtime-local-only fixture generators and TS unit tests after their
  durable fixture coverage is owned by Rust.
- Update workspace config.
- Update package manifests, internal docs, and public package disposition notes
  so no workspace import points at `@runxhq/runtime-local` or
  `@runxhq/adapters`.

## Scope

In scope:
- TS runtime-local deletion.
- TS adapters deletion bundled with runtime-local.
- Removing workspace package members, package references, tsconfig paths,
  exports, tests, scripts, and internal docs that keep runtime-local alive.
- Re-routing surviving local callers through Rust runtime, Rust CLI JSON, Rust
  harness execution, or another stable boundary already ratified in
  `rust-ts-interop-boundary`.
- Retiring runtime-local-owned TS fixture generators after their fixture output
  has either moved to durable `fixtures/**` coverage or become obsolete.
- Enforcing the ratified harness spine as the skill-execution contract.

Out of scope:
- Any runtime feature change.
- Cloud-side TS package deletions (their own specs).
- Creating a new `@runxhq/runtime-local` compatibility package, v2 package,
  import alias, adapter facade, or work-item/object bridge.
- Preserving legacy pre-cutover execution objects for skill runs, graph runs,
  or retired peer terminal artifacts.
- Adding new skills or changing product skill `SKILL.md`/`X.yaml` content to
  make the sunset pass.

## Dependencies

- `runx-contract-spine-hard-cutover` complete; it is the canonical source for
  harness, act, decision, signal, authority, and harness receipt shapes.
- `rust-harness` complete and default-ready for every harness mode needed by
  surviving local callers. It must reject retired fixture receipt fields rather
  than translate them.
- `rust-runtime-skill-execution` complete; checked-in product skill harnesses
  execute in Rust and verify sealed harness receipt trees.
- `rust-ts-sunset-marketplaces` complete.
- Every runtime adapter path complete and routed:
  `rust-runtime-adapters-agent`, `rust-runtime-adapters-a2a`, and archived
  `rust-runtime-adapters-catalog` and `rust-runtime-adapters-mcp` are
  completed; `rust-mcp-server-harness-receipt-seal` is also archived completed
  and closes the former MCP server single-skill receipt proof gap. The
  `cli_tool` runtime path is already consumed by skill execution but does not
  cover MCP, agent, A2A, catalog, CLI package source routing, or TS package
  import removal by implication.
- MCP server, dev, journal-local, connect, scaffold, tool-catalogs, doctor,
  registry, receipt path, and every CLI surface that imports runtime-local
  consumed by Rust or explicitly sunset.
- `rust-ts-interop-boundary` remains the package disposition source of truth.

## Sequencing

1. `runx-contract-spine-hard-cutover` ratifies the harness spine and receipt
   vocabulary. This spec must not create new schema aliases, v2 shapes, or
   transitional runtime-local object models.
2. `rust-harness` ports replay execution to Rust and upgrades active harness
   fixtures to canonical harness receipt assertions. This sunset must wait for
   those fixtures to reject retired fields such as `skill_execution`,
   `graph_execution`, `skill_name`, `source_type`, `graph_name`, and `owner`
   under `expect.receipt`.
3. `rust-runtime-skill-execution` proves checked-in product skills execute as
   Rust harness runs with contained decisions, contained acts, child harness
   receipt refs, proof validation, and unsupported-source fail-closed evidence.
4. Adapter specs move live adapter behavior into `runx-runtime::adapters::*`.
   This sunset may not start deleting packages until importer audit shows no
   runtime-local caller still needs TS adapters for production execution.
5. Runtime-local deletion removes the TS package only after the above evidence
   is checked in. The deletion PR is not a place to repair Rust parity except
   for narrow import cleanup caused by removing the TS package.

## Build Decisions

- `@runxhq/runtime-local` and `@runxhq/adapters` are removed from the workspace
  rather than replaced with empty packages.
- Public external-consumer migration is handled by release notes, deprecation
  metadata, and the surviving CLI/contracts boundaries. It is not handled by a
  local compatibility bridge.
- Product skill execution evidence is a Rust harness run. A passing assertion
  must cite canonical harness receipt schema/id, harness id, seal status,
  contained decisions, contained acts, child receipt refs, proof status, and
  verification checks.
- The Rust CLI launcher or SDK may spawn Rust runtime or call Rust libraries,
  but it must not import a TS runtime-local facade after this sunset.
- TS tests that only exercised runtime-local internals are deleted with the
  package. Durable end-to-end coverage lives under `fixtures/**`,
  `tests/cli/**`, or Rust crate tests.
- Any surviving TypeScript package may import stable TS contracts, shell the
  Rust CLI, or speak cloud HTTP. It may not import deleted runtime-local or
  adapters package paths.

## Deletion Classification

Can be deleted or rerouted now:
- `scripts/generate-rust-contract-fixtures.ts` direct runtime-local SDK source
  import: completed in this slice by moving act-assignment fixture generation
  onto `@runxhq/contracts` validation plus `@runxhq/core/util` stable hashing.

Needs a fresh smaller spec before deletion:
- `plugins/ide-core/**`: decide whether the private IDE core is sunset or
  routed to Rust CLI/contract boundaries; do not replace the runtime-local SDK
  with a compatibility facade.
- `packages/langchain/**`: decide whether the optional package is sunset or
  rebuilt around a stable Rust CLI/contract boundary. Local type redefinitions
  that preserve runtime-local behavior are not acceptable shims.
- Adapter oracle scripts:
  `scripts/generate-a2a-adapter-fixtures.ts`,
  `scripts/generate-agent-adapter-fixtures.ts`,
  `scripts/generate-runtime-catalog-adapter-oracles.ts`, and
  `scripts/generate-runtime-mcp-oracles.ts` need explicit durable-fixture
  ownership or deletion.
- Root `tests/**`: triage into Rust parity/CLI JSON coverage, package-internal
  runtime-local/adapters tests deleted with the packages, or obsolete tests.
  Do not touch payment tests in this sunset slice.
- Rust package-path probes in `crates/runx-runtime/src/adapters/catalog.rs`
  and `crates/runx-runtime/src/doctor.rs` need a Rust-owned replacement or
  deletion with targeted Rust checks.
- Root package metadata, lockfile entries, TS path aliases, vitest aliases,
  generated API docs, active fixtures, and the package directories themselves
  are final package-boundary cleanup after importer gates are zero.

Do not take in this spec slice:
- `packages/cli/src/**`: exact runtime-local/adapters references are already
  zero; the parent owns the remaining small CLI dead-command cleanup.
- Payment/x402 work.

## Next Executable Slices

This draft is still not executable as a package deletion. The next executable
work must be narrower than this sunset and should land in separate specs:

1. Non-CLI package boundary routing:
   - Decide whether `plugins/ide-core` is sunset or routed to Rust harness/SDK
     contracts.
   - Route or sunset `packages/langchain` so it no longer depends on
     runtime-local SDK/tool-catalog/result types.
   - Acceptance gate: no runtime-local/adapters package imports or manifest
     deps under `plugins/ide-core/**` or `packages/langchain/**`.
2. Oracle and fixture ownership cleanup:
   - For `generate-a2a-adapter-fixtures`, `generate-agent-adapter-fixtures`,
     `generate-runtime-catalog-adapter-oracles`, and
     `generate-runtime-mcp-oracles`, either declare the checked-in Rust
     fixtures durable and retire the TS generator, or keep a named pre-sunset
     owner that still runs before deletion.
   - `generate-rust-contract-fixtures.ts` is no longer in scope for this
     cleanup; it has moved off runtime-local SDK helpers.
   - Acceptance gate: every remaining oracle generator has a Rust owner or is
     deleted before package deletion.
3. Test-suite triage:
   - Classify the 66 `tests/**` reference files as Rust parity coverage, CLI JSON
     coverage, package-internal coverage deleted with runtime-local/adapters,
     or obsolete tests.
   - Exclude payment/x402 files from this sunset slice unless a separate
     payment owner explicitly scopes them.
   - Acceptance gate: no root `tests/**` file imports runtime-local/adapters or
     direct package source paths.
4. Rust path-probe cleanup:
   - Replace or delete runtime-local package-path expectations in
     `crates/runx-runtime/src/adapters/catalog.rs` and
     `crates/runx-runtime/src/doctor.rs` through Rust-owned invariants.
   - Acceptance gate: targeted `runx-runtime` catalog/doctor tests pass and no
     Rust source file names `packages/runtime-local` or `packages/adapters`.
5. Package-boundary deletion cleanup:
   - Remove root devDependencies, TS path aliases, vitest aliases, pnpm lock
     links, docs/API-surface entries, active fixture references, and package
     directories only after the above importer gates are zero.
   - Acceptance gate: the negative import check in this spec passes outside
     archived scafld specs.

## Planned Phases

Phase 1: importer and fixture inventory.
- Enumerate all imports, package deps, tsconfig paths, workspace scripts, docs,
  and fixture generators that reference `@runxhq/runtime-local`,
  `@runxhq/adapters`, `packages/runtime-local`, or `packages/adapters`.
- Start from the current inventory in this draft and refresh it immediately
  before execution; other workers may have added or removed importers.
- Classify each importer as Rust-routed, sunset with runtime-local, or
  surviving stable-boundary package.
- Enumerate runtime-local-only fixture generators and identify the Rust spec or
  durable fixture set that now owns each behavior.

Phase 2: evidence gate.
- Verify `rust-harness` acceptance evidence is checked in and active fixtures
  use canonical harness receipt assertions.
- Verify `rust-runtime-skill-execution` acceptance evidence is checked in for
  `issue-intake` and `issue-to-pr` without modifying product skill files.
- Verify adapter specs cover every source type reachable from surviving
  callers, and unsupported production source types fail closed with receipt
  evidence.
- Treat MCP adapter/client and MCP server receipt sealing as completed
  prerequisites, then verify no surviving TS caller still reaches the TS MCP
  adapter/runtime-local path in production execution.

Phase 3: route surviving callers.
- Remove runtime-local/adapters package dependencies from surviving packages.
- Route local execution through Rust CLI JSON, Rust runtime APIs, or the stable
  TS contracts boundary.
- Keep cloud-side TS packages on their existing cloud HTTP boundary; do not
  delete cloud packages here.

Phase 4: delete packages and scaffolding.
- Remove `packages/runtime-local/` and `packages/adapters/`.
- Remove workspace members, exports, package references, test targets, TS path
  aliases, and scripts that only existed for those packages.
- Remove or archive runtime-local-only oracle generators once their durable
  fixtures are owned by Rust.

Phase 5: validation and review.
- Run the negative import/object-vocabulary checks and positive Rust harness,
  runtime, receipt, workspace, and CLI validation commands.
- Record any intentionally surviving mention as archived documentation only.
- Hand off with explicit evidence that no compatibility bridge remains.

## Acceptance Criteria

- No active workspace file outside archived specs imports, depends on, exports,
  or path-aliases `@runxhq/runtime-local`, `@runxhq/adapters`,
  `packages/runtime-local`, or `packages/adapters`.
- `packages/runtime-local/` and `packages/adapters/` are gone and are not
  replaced by empty packages, shim packages, aliases, or v2 package names.
- All surviving skill execution acceptance evidence is expressed as Rust
  harness runs. Receipts are sealed harness nodes with contained decisions,
  contained acts, child harness receipt refs where graph execution is involved,
  proof status, and verification checks.
- No active replacement code, fixture, docs page, package manifest, or spec
  added by this sunset uses retired peer terminal artifact keys as execution
  objects.
- No active replacement fixture or receipt expectation accepts retired
  `skill_execution` or `graph_execution` receipt objects.
- `issue-intake` and `issue-to-pr` run through Rust runtime skill execution and
  their emitted harness receipts validate through `runx-receipts`.
- `runx harness` and the runtime skill execution tests pass without invoking
  `packages/runtime-local` or `packages/adapters`.
- Runtime-local-only fixture generator scripts are either deleted or retained
  only when another active spec explicitly names them as still required before
  this sunset can complete.
- The deletion does not change product skill files (`skills/**/SKILL.md` or
  `skills/**/X.yaml`) except through separately approved skill specs.

## Validation Commands

```sh
test ! -d packages/runtime-local
test ! -d packages/adapters
! rg -n "@runxhq/(runtime-local|adapters)|packages/(runtime-local|adapters)" . --glob '!.scafld/specs/**' --glob '!**/archive/**'
! rg -n '"(work[_]item|engagement|matter|operation)"\\s*:' crates/runx-runtime crates/runx-contracts fixtures/runtime fixtures/harness tests --glob '!**/archive/**'
! rg -n '"(skill_execution|graph_execution)"\\s*:' fixtures/runtime fixtures/harness crates/runx-runtime tests --glob '!**/archive/**'
! rg -n "\\b(issue-control|runtime-local-v2|adapters-v2)\\b" packages crates fixtures tests docs --glob '!**/archive/**'
! rg -n "(^|[^A-Za-z0-9_])issue_to_pr([^A-Za-z0-9_]|$)" crates/runx-runtime fixtures/runtime tests --glob '!**/archive/**' --glob '!fixtures/runtime/skills/issue-to-pr/**'
pnpm install --frozen-lockfile
pnpm test
pnpm build
cargo test --manifest-path crates/Cargo.toml -p runx-runtime harness
cargo test --manifest-path crates/Cargo.toml -p runx-runtime --features cli-tool --test skill_issue_intake
cargo test --manifest-path crates/Cargo.toml -p runx-runtime --features cli-tool --test skill_issue_to_pr
cargo test --manifest-path crates/Cargo.toml -p runx-receipts
cargo clippy --manifest-path crates/Cargo.toml -p runx-runtime -p runx-receipts --all-targets --features cli-tool -- -D warnings
cargo fmt --manifest-path crates/Cargo.toml --all --check
node scripts/check-rust-core-style.mjs
```

## Explicit Blockers

- Regression in a completed prerequisite: `rust-harness`,
  `rust-runtime-skill-execution`, any Rust adapter spec, or
  `rust-mcp-server-harness-receipt-seal` losing sealed harness receipt proof or
  reintroducing TS runtime-local dispatch.
- The 2026-05-21 importer census is nonzero: 117 active exact-package
  reference files remain outside `.scafld/specs/**` and `dist/**`, including
  93 outside the two packages to delete and 66 actual package import files
  outside those packages.
- Any surviving local caller still importing `@runxhq/runtime-local` or
  `@runxhq/adapters`.
- `packages/cli/src/**` must stay at zero exact runtime-local/adapters
  references while parent-owned CLI cleanup continues out of band.
- `plugins/ide-core/**` and `packages/langchain/**` still depending on or
  importing runtime-local/adapters instead of a stable Rust/contract/CLI
  boundary or being explicitly sunset.
- Root package metadata, pnpm lock entries, `tsconfig.base.json`, or
  `vitest.workspace-aliases.ts` still resolving runtime-local/adapters after
  callers are routed.
- Runtime-local/adapters oracle generators still importing deleted package
  sources without a named durable Rust fixture owner.
- Root `tests/**` still importing runtime-local/adapters package APIs or direct
  package source paths after their behavior has a Rust/CLI owner.
- Any adapter source type reachable from surviving local execution still lacks
  a Rust adapter or explicit fail-closed receipt evidence.
- Any active replacement object model still centered on retired peer terminal
  artifacts, `skill_execution`, or `graph_execution`.
- Any proposal to keep a workspace shim, v2 package, path alias, or bridge
  package for runtime-local/adapters.
- Cloud-side hosted cutover ambiguity that would make local deletion break a
  published stable boundary already retained by `rust-ts-interop-boundary`.

## Rollback And Repair

- Before package deletion, rollback is to leave runtime-local/adapters in place
  and keep this spec blocked; do not add a bridge.
- After deletion, rollback must restore only the deleted package files from the
  deletion commit if the Rust runtime path is proven incomplete. Do not repair
  by adding aliases, v2 packages, or legacy object translation.
- If validation finds an importer, route that importer through Rust or sunset
  it. Do not reintroduce runtime-local as a dependency.
- If a fixture still needs old TS receipt fields, fix the upstream Rust harness
  or runtime skill execution evidence. Do not whitelist the old field here.

## Open Questions

- Whether npm deprecation metadata for the deleted packages is published in the
  same release train or a separate release-management spec. Default: publish
  deprecation metadata outside this deletion PR; do not keep local shim
  packages.
