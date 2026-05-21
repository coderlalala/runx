# runx-runtime

Native Rust runtime for governed runx execution.

This crate owns the canonical local orchestration path for Rust-backed runx:
skill execution, graph execution, harness replay, caller reporting, sandbox
preparation, receipts, history projection, adapters, and payment/authority
runtime policy. Pure parser, core, contract, and receipt crates remain
upstream.

Current slice:

- parses a local graph with `runx-parser`
- plans sequential/fanout transitions with `runx-core`
- runs `cli-tool` skills behind the `cli-tool` feature
- emits harness receipts and validates the parent receipt tree with
  `runx-receipts`
- exposes native skill, doctor, list, history, MCP, registry, config, policy,
  tool, and dev command support through `runx-cli`

Adapter families remain feature gated:

- `cli-tool`
- `mcp`
- `a2a`
- `agent`
- `catalog`

## Doctor

The native Rust doctor API is wired into `runx-cli` for the read-only
diagnostic surface. It must not shell out to npm or TypeScript for canonical
local behavior.

This crate currently ports the read-only fixture-backed diagnostics:

- `runx.tool.manifest.removed_format`
- `runx.tool.fixture.missing`
- `runx.skill.fixture.missing`
- `runx.structure.file_budget.exceeded`
- `runx.structure.cross_package_reach_in`

Deferred doctor families remain owned by follow-up slices:

- `runx doctor --fix` repair writes
- diagnostic catalog, `--list-diagnostics`, and `--explain`
- official skills lock freshness
- tool manifest stale source and schema hashes
- packet index diagnostics
- graph packet path validation
- receipt proof health
- policy health
