---
spec_version: '2.0'
task_id: rust-runtime-adapters-mcp
created: '2026-05-18T00:00:00Z'
updated: '2026-05-20T00:38:00Z'
status: draft
harden_status: blocked
size: extra_large
risk_level: very_high
---

# Rust runtime MCP adapter

## Current State

Status: draft
Current phase: implementation gap review
Next: implement the missing MCP lifecycle slices before handoff or review
Reason: local Rust code contains a partial feature-gated MCP client adapter,
but the lifecycle required by this spec is not implemented or fixture-proven.
`crates/runx-runtime/src/adapters/mcp.rs` has `McpAdapter`,
`ProcessMcpTransport`, argument mapping, stdio framing, response size checks,
timeouts, sanitized transport errors, result stringification, and sandbox-plan
metadata. The client path initializes and calls a tool, but does not prove
`tools/list` parity. It does not implement the Rust MCP server lifecycle
required for `runx mcp serve`, `runx_resume`, duplicate tool-name rejection,
JSON-RPC server error parity, or approval/resume result conversion.
Implemented slice:
- Added Rust MCP adapter coverage for two existing client lifecycle gaps:
  timeout lower-bound propagation (`timeout_seconds: 0` clamps to 50 ms)
  with sanitized provider tool errors, and malformed stdio JSON responses
  through the real process transport returning the generic sanitized adapter
  failure without leaking fixture input secrets.

Blockers:
- No Rust CLI MCP command is present under `crates/runx-cli/src`; the checked
  `rg` query over `crates/runx-cli/src` and `crates/runx-cli/Cargo.toml`
  returned no matches.
- No MCP server test file exists; the checked test-file query finds only
  `crates/runx-runtime/tests/mcp_adapter.rs`.
- Current Rust MCP test coverage includes argument mapping, sanitized client
  tool errors with the 50 ms minimum timeout floor, malformed JSON response
  sanitization through stdio framing, and MCP tool-catalog search/inspect
  enablement. It still does not cover client echo, sandbox env enforcement,
  full timeout/child termination behavior, missing metadata, size bounds,
  server initialize/list/call, resume, duplicate tool names, or JSON-RPC
  server errors.
- `fixtures/runtime/adapters/mcp` does not exist, and no
  `scripts/generate-runtime-mcp-oracles.ts` or
  `scripts/check-runtime-mcp-oracles.sh` exists in this checkout.
- `crates/runx-runtime/Cargo.toml` has `default = []` and `mcp = []`, so the
  feature boundary exists, but there is no rmcp dependency or recorded library
  decision yet.
Allowed follow-up command: none until implementation and parity fixtures land
Latest runner update: 2026-05-20 local implementation slice. Checked
`cargo test --manifest-path crates/Cargo.toml -p runx-runtime mcp --features
mcp -- --nocapture`; it passed, exercising three MCP adapter tests and two MCP
tool-catalog tests matched by the filter. Also ran
`cargo fmt --manifest-path crates/Cargo.toml --all`; formatting was applied to
the new test file.
Review gate: blocked_missing_mcp_lifecycle_evidence

## Summary

Port the `mcp` adapter family to `runx-runtime` behind the
`features = ["mcp"]` flag. Covers MCP client (consume MCP servers as
tools) and MCP server (`runx mcp serve`) execution paths.

This is intentionally the LAST adapter ported. The rmcp ecosystem,
tokio integration, sandbox interaction, and spawn semantics combine more
moving parts than any other adapter. The earlier adapters validate the
runtime trait shape so MCP consumes the established shape.

This is a hard cutover spec for MCP runtime paths. Once the Rust MCP path is
routed, it must not dispatch to TypeScript at runtime. TypeScript remains the
oracle source for committed fixture bytes before routing changes.

## Context

CWD: `.`

Read-only TypeScript references:
- `packages/adapters/src/mcp/index.ts`
- `packages/adapters/src/mcp/index.test.ts`
- `packages/runtime-local/src/mcp/index.ts`
- `packages/runtime-local/src/harness/mcp-fixture.ts`
- `packages/cli/src/commands/mcp.ts`
- `packages/cli/src/commands/mcp.test.ts`
- `packages/runtime-local/src/sdk/**`
- `packages/runtime-local/src/runner-local/process-sandbox.ts`
- `packages/runtime-local/src/runner-local/index.ts`

Cloud reference only, not part of this implementation:
- `../cloud/packages/mcp-hosted/src/**`

Rust packages:
- `crates/runx-runtime`
- `crates/runx-contracts`
- `crates/runx-cli`

Implementation impact files:
- `crates/runx-runtime/src/adapters/mcp.rs` or
  `crates/runx-runtime/src/adapters/mcp/mod.rs`
- `crates/runx-runtime/src/adapters/mcp/client.rs`
- `crates/runx-runtime/src/adapters/mcp/protocol.rs`
- `crates/runx-runtime/src/adapters/mcp/server.rs`
- `crates/runx-runtime/src/adapters/mcp/sandbox.rs`
- `crates/runx-runtime/src/adapters/mod.rs`
- `crates/runx-runtime/src/lib.rs`
- `crates/runx-runtime/Cargo.toml`
- `crates/runx-cli/src/commands/mcp.rs` or the scaffolded Rust CLI command
  path if different
- `crates/runx-runtime/tests/mcp_adapter.rs`
- `crates/runx-runtime/tests/mcp_server.rs`
- `crates/runx-runtime/tests/support/mcp_oracles.rs` if shared helpers keep
  tests clearer
- `scripts/generate-runtime-mcp-oracles.ts`
- `scripts/check-runtime-mcp-oracles.sh`
- `fixtures/runtime/adapters/mcp/**`

Do not modify in this spec execution:
- `../cloud/packages/mcp-hosted/**`.
- TypeScript runtime files. TypeScript is the oracle source for fixture bytes
  before routing changes, not a runtime dispatch path.
- `crates/runx-runtime/src/adapters/catalog.rs`, except for shared adapter
  registry wiring if MCP needs to register beside it.

Invariants:
- MCP client uses rmcp (or chosen rust MCP library) per Phase 1 decision.
- Sandbox enforcement for MCP-spawned subprocesses matches the TS process
  sandbox semantics; no weakening.
- MCP server respects the same scope-admission and approval-gate behavior
  the TS server does.
- MCP client sanitizes remote tool errors the same way TypeScript does:
  provider details and user input secrets must not appear in stderr,
  error strings, receipt payloads, or metadata.
- MCP stdout stringification matches TypeScript: text content entries join
  with newlines, non-text content is JSON stringified per entry, and
  non-content results stringify as JSON unless already a string.
- JSON-RPC framing is deterministic and bounded. Client responses above
  1 MiB and server requests above 4 MiB are rejected.
- Timeouts use the same lower bound as TypeScript: requested timeouts below
  50 ms are raised to 50 ms.
- The built-in MCP resume tool remains named `runx_resume`.
- No live network or external MCP server hits in fixtures.
- The `mcp` runtime feature is opt-in and must not enable `agent`, `a2a`, or
  `catalog` implicitly.

## Objectives

- Pick the Rust MCP library in Phase 1 ingest and record the decision in the
  implementation notes. Default candidate: `rmcp`.
- Port MCP client (consume MCP tools as skill steps).
- Port MCP server (`runx mcp serve`) including listing, invocation,
  approval round-trips on MCP-driven mutations.
- Add fixture suite and oracle generator for client and server flows.
- Prove sandbox, timeout, malformed response, sanitized error, resume, and
  duplicate tool-name behavior.

## Scope

In scope:
- MCP client and server under the adapter trait.
- Runtime feature wiring for `features = ["mcp"]`.
- Rust CLI wiring for `runx mcp serve` only if the Rust CLI command tree is
  already the canonical dispatch point when this spec executes.
- Fixture-backed parity for JSON-RPC bytes, structured MCP responses, adapter
  output, and sanitized failures.

Out of scope:
- Cloud-hosted MCP server (`cloud/packages/mcp-hosted`) cutover; that's a
  separate cloud cutover spec.
- Marketplace publication or hosted registry changes.
- Provider-specific MCP services that require live credentials.

## Dependencies

- `rust-runtime-skeleton`.
- `rust-runtime-adapters-agent`.
- `rust-approval-gate-parity`.
- `runx-contract-spine-hard-cutover`.
- Post-cutover harness receipt proof and receipt-tree APIs.
- At least one additional adapter complete so the trait shape is stable.

## Implementation Contract

### MCP Client

Implement the local MCP client path with the chosen Rust MCP library or a
small local protocol layer if the chosen library cannot express the required
stdio framing without weakening semantics. The client must:
- Spawn the configured MCP server command under the same process sandbox
  policy used for `cli-tool`.
- Send `initialize`, then `notifications/initialized`, before listing or
  calling tools.
- Use protocol version `2025-06-18`.
- Support `tools/list` and `tools/call`.
- Enforce response size, timeout, stderr capture, and termination behavior.
- Terminate the child process after the call, escalating to a hard kill after
  the same grace window TypeScript uses.

Argument mapping must match TypeScript exactly:
- No argument template: merge `inputs` and `resolved_inputs`, with
  `resolved_inputs` taking precedence.
- Exact `{{ key }}` template: copy the input value without stringifying.
- Embedded `{{ key }}` template: stringify the input value and substitute.
- Non-string template values pass through unchanged.

Failure sanitization must match TypeScript:
- MCP error codes become `MCP tool returned error <code>.`.
- Sandbox denials expose the sandbox denial reason and sandbox metadata.
- Malformed JSON, early process exit, and unknown failures become
  `MCP adapter failed.` unless TypeScript exposes a more specific timeout
  message for the same fixture.

### MCP Server

Implement `runx mcp serve` through Rust only after the Rust CLI command tree
for this command is assigned to this spec. The server must:
- Require at least one served skill reference.
- Serve `initialize`, `ping`, `tools/list`, and `tools/call`.
- Expose each skill as an MCP tool with input schema generated from skill
  inputs.
- Add the built-in `runx_resume` tool with the same schema and behavior as
  TypeScript.
- Reject duplicate tool names with the same user-facing error text.
- Return JSON-RPC parse, invalid-params, method-not-found, and tool-call
  failure errors with the same codes and messages as TypeScript fixtures.
- Convert completed, paused, denied, escalated, and failed host results into
  MCP tool results with the same `content`, `structuredContent.runx`, and
  `isError` shape as TypeScript.

The server must route skill execution through the post-cutover Rust
runtime/harness path that seals `runx.harness_receipt.v1` nodes and links child
harness receipt refs. It must not call TypeScript to execute served skills once
routed.

### Feature Boundaries

`runx-runtime` must keep default features empty. Add `mcp` as an opt-in
feature if it is not already present. Keep MCP-specific async/process
dependencies behind that feature.

If `rmcp` is selected, pin the crate and feature set in `crates/runx-runtime`
only. Do not add MCP dependencies to `runx-contracts`, `runx-core`,
`runx-parser`, `runx-receipts`, or `runx-sdk`.

## Fixture and Oracle Contract

Create deterministic fixtures under `fixtures/runtime/adapters/mcp/`:
- `client-echo/`
- `client-error-sanitized/`
- `client-sandbox-env/`
- `client-timeout/`
- `client-malformed-json/`
- `client-missing-tool-metadata/`
- `server-list-and-call/`
- `server-paused-and-resume/`
- `server-duplicate-tool-name/`
- `server-json-rpc-errors/`

Add `scripts/generate-runtime-mcp-oracles.ts` to execute the TypeScript MCP
client/server paths against those fixtures and write oracle files:
- `fixtures/runtime/adapters/mcp/oracles/<case>.stdout`
- `fixtures/runtime/adapters/mcp/oracles/<case>.stderr`
- `fixtures/runtime/adapters/mcp/oracles/<case>.status`
- `fixtures/runtime/adapters/mcp/oracles/<case>.json`
- `fixtures/runtime/adapters/mcp/oracles/<case>.frames.json` for server
  request/response transcript cases

The generator must:
- Run from the OSS workspace root.
- Keep `RUNX_HOME`, receipt directories, and cache directories in temporary
  paths.
- Normalize run ids, receipt ids, temp paths, measured durations, and
  timestamps only in oracle comparison helpers.
- Include a fixture input containing a synthetic secret and assert that the
  oracle outputs do not contain it.
- Record non-success statuses for expected failure cases.
- Avoid external network access and live MCP services.

Add `scripts/check-runtime-mcp-oracles.sh` to run the generator in check mode.

## Tests

Add Rust tests for:
- MCP client echo success.
- MCP client tool error sanitization.
- MCP sandbox env allowlist enforcement and metadata.
- MCP timeout handling and child termination.
- Malformed JSON response handling.
- Missing server/tool metadata failure.
- MCP server initialize, tools/list, and tools/call.
- Paused run exposure and resume through `runx_resume`.
- Duplicate tool-name rejection.
- JSON-RPC parse, invalid params, unknown method, and tool-not-found errors.
- Feature-gated build with `mcp` enabled and unrelated adapter features
  disabled.

Test names should include `mcp_adapter`, `mcp_client`, or `mcp_server` and the
behavior under test. Normalize only dynamic fields before byte comparison.

## Acceptance Commands

Run these after implementation:

```sh
pnpm install --frozen-lockfile
node scripts/test-workspace.mjs packages/adapters/src/mcp/index.test.ts packages/cli/src/commands/mcp.test.ts packages/runtime-local/src/tool-catalogs/index.test.ts
pnpm exec tsx scripts/generate-runtime-mcp-oracles.ts --check
scripts/check-runtime-mcp-oracles.sh
cargo fmt --manifest-path crates/Cargo.toml --all --check
cargo test --manifest-path crates/Cargo.toml -p runx-runtime mcp --features mcp -- --nocapture
cargo test --manifest-path crates/Cargo.toml -p runx-cli mcp --features mcp -- --nocapture
cargo test --manifest-path crates/Cargo.toml -p runx-runtime receipt_tree -- --nocapture
cargo test --manifest-path crates/Cargo.toml -p runx-receipts proof -- --nocapture
cargo clippy --manifest-path crates/Cargo.toml -p runx-runtime -p runx-cli --all-targets --features mcp -- -D warnings
node scripts/check-rust-core-style.mjs
! rg -n "runx\\.issue_to_pr_outcome\\.v1|issue_to_pr_outcome|verification[_-]report|target[_-]?effect|\"effect\"\\s*:" fixtures/runtime/adapters/mcp crates/runx-runtime/src/adapters/mcp crates/runx-cli/src
```

If the workspace-level Rust command is stable when this spec runs, also run:

```sh
cargo test --manifest-path crates/Cargo.toml --workspace
```

## Completion Criteria

- Rust MCP client and server fixture outputs match TypeScript oracle bytes
  after dynamic-field normalization.
- The Rust MCP adapter emits the same success, failure, metadata, and
  sanitized-error shapes as TypeScript for committed cases.
- `runx mcp serve` exposes the same tool list, resume tool, structured
  content, and JSON-RPC error behavior as TypeScript for committed cases.
- Sandbox, timeout, size-limit, and process-termination behavior are tested.
- No live network or external MCP server is required by tests.
- `mcp` remains opt-in and does not pull MCP dependencies into pure crates.
- Acceptance commands pass, or any skipped command is recorded with the exact
  blocker and owner.

## Open Questions

- rmcp library maturity and licensing at port time. Default: use rmcp if it
  can satisfy stdio framing, timeout, and sandbox requirements without
  weakening semantics; otherwise implement the narrow local protocol layer and
  record why.
- Whether MCP server spawns under the same process-sandbox primitives as
  cli-tool, or needs an MCP-specific isolation profile.
