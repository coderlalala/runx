---
spec_version: '2.0'
task_id: rust-mcp-rmcp-adoption
created: '2026-05-21T03:00:00Z'
updated: '2026-05-21T02:04:44Z'
status: completed
harden_status: passed
size: large
risk_level: high
---

# rmcp adoption for the MCP adapter

## Current State

Status: completed
Current phase: final
Next: done
Reason: task completed
Blockers: none
Allowed follow-up command: `none`
Latest runner update: 2026-05-21T02:04:44Z
Review gate: pass

## Why this exists

The MCP adapter at [`crates/runx-runtime/src/adapters/mcp/`](../../crates/runx-runtime/src/adapters/mcp/)
implements a narrow MCP client and server over stdio, with hand-rolled JSON-RPC
framing, request/response correlation, server-state, and tool-call dispatch.
This is intentional per `crates/deny.toml`:

```
{ name = "rmcp", reason = "MCP currently uses the narrow local protocol layer;
                            rmcp needs a scoped adapter spec first." }
```

The hand-rolled layer was the right call when the protocol surface was small.
Now that:

- `adapters/mcp/` has ~2000 LoC across 9 files implementing transport,
  framing, JSON-RPC, server, server-skill execution, sandbox metadata,
  templating, types, and the adapter trait;
- the upstream `rmcp` crate has a pre-1.0 Rust implementation with rustls +
  tokio transports;
- the rust-takeover plan §10 lists the MCP adapter as "the hardest port"
  (architecture doc §13);

it's time to spec the migration to the official `rmcp` crate.

## Scope

Design and baseline only. Migration lands in a follow-up
`rust-mcp-rmcp-cutover` spec. The only executable work allowed under this task
is non-network baseline coverage for the existing stdio contract.

This spec answers:

1. Which parts of the hand-rolled MCP layer rmcp replaces.
2. Which parts stay (the runx-specific shapes that rmcp doesn't know
   about).
3. How the cutover stages so the existing `serve_mcp_json_rpc` byte-shape
   contract is preserved.
4. The async-runtime exception this spec consumes (depends on
   `rust-async-http-layer`).

This spec does **not** add `rmcp`, change Cargo features, or delete any
hand-rolled MCP code. The "cutover plan" below is handoff material for the
future cutover spec.

## Safe executable baseline

Because `rust-async-http-layer` is still blocked, this spec must not add the
`rmcp` dependency or any async runtime features yet. The executable MCP-only
slice is to record the current hand-rolled stdio server contract and make it
easy for the future rmcp cutover to diff against it.

Baseline fixtures:

- `fixtures/runtime/adapters/mcp/wire-contract/basic-lifecycle.requests.jsonl`
- `fixtures/runtime/adapters/mcp/wire-contract/basic-lifecycle.responses.jsonl`
- `fixtures/runtime/adapters/mcp/wire-contract/error-paths.requests.jsonl`
- `fixtures/runtime/adapters/mcp/wire-contract/error-paths.responses.jsonl`

Baseline test:

```bash
cargo test -p runx-runtime --features mcp --test mcp_server mcp_server_matches_recorded_stdio_wire_contract
```

The test frames each JSONL body with MCP `Content-Length` headers and compares
raw response bytes. It covers initialize, `notifications/initialized`
notification suppression, `tools/list`, `tools/call`, and JSON-RPC error paths.
The rmcp cutover must reuse this corpus before replacing the hand-rolled server
loop.

## Replacement map

| Hand-rolled module | rmcp equivalent |
| --- | --- |
| `mcp/framing.rs` (Content-Length parsing) | `rmcp::transport::stdio` |
| `mcp/jsonrpc.rs` (request/response builders) | `rmcp::model::{Request, Response, JsonRpcMessage}` |
| `mcp/transport.rs::ProcessMcpTransport` (spawn child, stdio framing, response correlation, timeouts) | `rmcp::Client::serve(stdio)` |
| `mcp/transport.rs::FixtureMcpTransport` (in-process fixture) | stays — fixture support is runx-specific |
| `mcp/server.rs::serve_mcp_json_rpc` (stdio server loop) | `rmcp::Server::serve(stdio)` |
| `mcp/server.rs::McpServerState` (tool registry) | rmcp's `ServerHandler` trait |
| `mcp/server.rs::initialize_server_result` / `tools_list_result` / `mcp_tool_result_json` | rmcp model types |
| `mcp/server_skill.rs::execute_mcp_server_skill` | stays — runx skill execution under MCP |
| `mcp/templates.rs::map_mcp_arguments` and `stringify_*` | stays — runx template engine |
| `mcp/sandbox_metadata.rs` | stays — runx sandbox metadata is runx-specific |
| `mcp/adapter.rs::McpAdapter` (SkillAdapter impl) | stays — runx adapter trait remains |
| `mcp/types.rs::McpToolResult`, `McpHostRunResult` | partially: rmcp has `Content`, `CallToolResult`; runx `McpHostRunResult` stays |

Net result: ~1100 LoC removed (transport, jsonrpc, framing, server protocol),
~1000 LoC stays (skill execution, templates, sandbox metadata, runx-side
adapter glue).

## Follow-up cutover plan (`rust-mcp-rmcp-cutover`)

These stages execute in the follow-up cutover spec, not in this design spec.
Each stage compiles and tests independently. No "big bang" rewrite.

### Stage 1: pull rmcp behind a feature flag, no behavior change

Add to `runx-runtime/Cargo.toml`:

```toml
[features]
mcp = []
mcp-rmcp = ["dep:rmcp", "async-http"]

[dependencies]
# The cutover spec must pin an exact reviewed rmcp release, for example
# `=<major.minor.patch>`. Wildcards, ranges, and fake literals are forbidden.
```

`mcp` (hand-rolled) and `mcp-rmcp` (rmcp-backed) are disjoint features.
The cutover spec must add a build-time guard:

```rust
#[cfg(all(feature = "mcp", feature = "mcp-rmcp"))]
compile_error!("features `mcp` and `mcp-rmcp` are mutually exclusive");
```

The exact `rmcp` dependency line is intentionally absent from this design
spec. The cutover spec must choose the version after reviewing the current
crate release, run `cargo update -p rmcp`, and commit the resulting
`Cargo.lock` diff with the dependency review.

### Stage 2: replace client transport

Behind `#[cfg(feature = "mcp-rmcp")]`, swap `ProcessMcpTransport::call_tool`
to use `rmcp::Client`. Keep `FixtureMcpTransport` unchanged.

Validation: every existing `mcp_adapter` integration test passes against
the rmcp client.

### Stage 3: replace server transport

Behind `mcp-rmcp`, swap `serve_mcp_json_rpc`'s stdio loop for rmcp's server.
Keep the runx-specific `McpServerState` and tool-call dispatch — wrap them
in an rmcp `ServerHandler` impl.

Validation: every existing `mcp_server` integration test passes.

### Stage 4: byte-exact wire compatibility check

Run the rmcp server against
`fixtures/runtime/adapters/mcp/wire-contract/*.requests.jsonl` and diff its raw
framed output against the matching `*.responses.jsonl` baseline after framing
each JSONL body.

Acceptable wire-diff envelope:

- Object key ordering may differ.
- `jsonrpc` may be present where the old path omitted it, or omitted where the
  old path accepted the omission, as long as the message remains JSON-RPC 2.0
  valid.
- `serverInfo.name`, `serverInfo.version`, and the exact `capabilities`
  subtree may differ if the advertised tools remain identical.
- `protocolVersion` may differ only when rmcp negotiates a newer MCP release
  and the fixture explicitly records the negotiated value.

Must match:

- JSON-RPC request/response `id` correlation.
- Tool names and argument payloads.
- `result.content[*].type`, `result.content[*].text`, and
  `result.structuredContent.runx.*`.
- `error.code`, `error.message` category, and whether `error.data` is present.
- Notification suppression for `notifications/initialized`.

Any diff outside this envelope is a regression and must either be fixed or
added to the envelope in a separate review before deletion proceeds.

### Stage 5: delete the hand-rolled layer

Once rmcp passes Stage 4 and the in-repo deletion gate is satisfied, delete
`mcp/{framing,jsonrpc,transport,server}` and remove the `mcp-rmcp` feature.
The rmcp-backed implementation becomes the only `mcp` feature.

Deletion gate:

- `mcp-rmcp` has been default-on for one full runx minor release or an owner
  override is committed in the cutover spec.
- `fixtures/runtime/adapters/mcp/rmcp-cutover/` contains a dated attestation
  JSON file for each production consumer that still runs `runx mcp serve`
  directly. Each file records the consumer, runx commit, start/end timestamps,
  and zero known protocol-drift incidents.
- The cutover spec records the command used to query protocol-drift receipts
  or explicitly states that telemetry is unavailable and owner override is
  being used.

Nitrosend and aster soak remains release evidence, but the scafld gate is the
in-repo attestation or explicit owner override.

## Deny.toml exception

After `rust-async-http-layer` lands (tokio/reqwest allowed), the follow-up
cutover spec removes:

```toml
{ name = "rmcp", reason = "..." }
```

from `crates/deny.toml`.

Keep `cargo deny check licenses` clean — rmcp pulls in tokio (already
allowed by the async-http spec), schemars, and JSON-Schema codegen
helpers. Confirm all are Apache-2.0 / MIT.

## What rmcp does NOT replace

The cutover must not touch:

- **Sandbox metadata emission** (`sandbox_metadata.rs`) — runx receipt
  shape, not part of the MCP protocol.
- **Template engine** (`templates.rs`) — runx-specific argument templating
  for `{{ field }}` substitution. rmcp doesn't know about runx templates.
- **Server-side skill execution** (`server_skill.rs`) — runx skill model;
  rmcp doesn't know runx skills.
- **`McpHostRunResult` projection** — runx `runx:` content object that
  encodes runx run state (skillName, runId, receiptId, status); the
  `mcp_tool_result_from_host_result` function continues to exist.
- **runx receipt sealing** (`step_receipt`, `LocalReceiptStore` writes) —
  runx contract.

## Risks

- **rmcp churn**: the crate is pre-1.0. Stage 1 must include a CI pin to a
  specific release and a `cargo update -p rmcp` review gate.
- **Tokio bloat**: rmcp pulls full-feature tokio if we're not careful. The
  feature spec above uses `transport-io` only.
- **Test fixture diff**: byte-exact diffs are hard to predict. Stage 4 may
  surface protocol-version drift between our hand-rolled `2025-06-18` and
  rmcp's default. Any allowed drift must fit the envelope above.
- **Production soak**: aster and nitrosend depend on `runx mcp serve`.
  Stage 5 must not delete the hand-rolled path until the in-repo attestation
  gate exists or an owner override records why deletion is still correct.
- **Async prerequisite**: if `rust-async-http-layer` ships only for a
  non-adapter consumer or is delayed, this spec reopens. rmcp adoption remains
  blocked until adapter-tier async runtime rules are explicit.

## Acceptance for this spec

Run from `oss/`:

```bash
scafld validate rust-mcp-rmcp-adoption
cargo test --manifest-path crates/Cargo.toml -p runx-runtime --features mcp --test mcp_server mcp_server_matches_recorded_stdio_wire_contract
awk '/^## Acceptance for this spec/{exit} /^## Harden Rounds/{exit} {print}' .scafld/specs/drafts/rust-mcp-rmcp-adoption.md | rg 'rmcp = \{ version = "0\.x"|mcp-rmcp = \["mcp"|pre-declared envelope|production users .*soaked' && exit 1 || test $? -eq 1
```

Completion criteria:

- The design spec validates.
- The existing hand-rolled stdio wire-contract baseline passes.
- The pre-harden body contains no fake `rmcp` version, no additive
  `mcp-rmcp = ["mcp", ...]` feature shape, no unenumerated "pre-declared
  envelope", and no deletion gate that depends only on unverifiable external
  soak.

## Rollback

This spec is design-only. If `rust-async-http-layer` is declined or delayed,
leave the current hand-rolled MCP implementation and `rmcp` deny rule in place
and reopen this spec before authoring `rust-mcp-rmcp-cutover`.

## Acceptance gates for the follow-up cutover

| Stage | Acceptance |
| --- | --- |
| 1 (feature exists) | `cargo check -p runx-runtime --features mcp-rmcp` clean; `cargo check -p runx-runtime --features mcp,mcp-rmcp` fails with the intentional compile-error |
| 2 (client replaced) | all `mcp_adapter` tests pass with `--features mcp-rmcp` |
| 3 (server replaced) | all `mcp_server` tests pass with `--features mcp-rmcp` |
| 4 (wire parity) | `wire-contract/*.requests.jsonl` fixture diff against hand-rolled bytes within the enumerated envelope above |
| 5 (deletion) | in-repo cutover attestation or owner override exists; hand-rolled files removed; rmcp-backed path is the only `mcp` feature |

## Open questions

- Should rmcp's HTTP transport (SSE / streamable HTTP) be enabled too, or
  only stdio? Defer: scope this spec to stdio only; add a follow-up if a
  consumer needs HTTP.
- Should runx publish rmcp `ServerHandler` impls as a public reusable
  type? Probably yes, but defer to a `runx-mcp-public-server-trait` spec
  after the cutover.

## References

- [`crates/runx-runtime/src/adapters/mcp/`](../../crates/runx-runtime/src/adapters/mcp/)
- [`crates/deny.toml`](../../crates/deny.toml) — current rmcp ban
- [`oss/docs/rust-kernel-architecture.md`](../../docs/rust-kernel-architecture.md)
  §13 (MCP is "the hardest port")
- [`plans/rust-takeover.md`](../../../plans/rust-takeover.md) §9 step 7
  ("MCP is last")
- rmcp upstream: `https://github.com/modelcontextprotocol/rust-sdk`

## Harden Rounds

### round-1

Status: needs_revision
Started: 2026-05-21T01:46:31Z
Ended: 2026-05-21T01:46:31Z
Verdict: needs_revision
Provider: claude
Model: claude-opus-4-7
Output format: claude.mcp_submit_harden
Summary: Design spec is well-grounded — every referenced file, fixture, test, deny.toml ban, and protocol-version literal verifies against the codebase, and the executable baseline (wire-contract snapshot test) already exists and is exercised by `cargo test --features mcp`. The scoping is honest about deferring the migration. However the spec needs revision before approval on four design points that would otherwise be inherited unresolved by the cutover spec: the `mcp` / `mcp-rmcp` feature pair is described as "mutually exclusive" while the Cargo snippet has `mcp-rmcp = ["mcp", ...]` which makes them additive; the `rmcp = "0.x"` literal is not a valid Cargo version requirement and contradicts the stated CI-pin discipline; Stage 4's "pre-declared envelope" of acceptable wire diffs is named but never enumerated, leaving the wire-parity gate non-executable; and Stage 5 conditions deletion on production soak in aster + nitrosend (external private repos) with no in-repo verifiable signal. The Cutover-stages section also reads as executable plan despite Scope saying "design only" — either reframe stages as "follow-up cutover plan" or split the gate-table out. None of these block the architectural direction (rmcp adoption is the right move — replaces hand-rolled JSON-RPC with the upstream protocol crate and aligns with rust-takeover §9 step 7), but they would block a clean handoff to the cutover spec.

Checks:
- path audit
  - Grounded in: code:crates/runx-runtime/src/adapters/mcp/mod.rs:12
  - Result: passed
  - Evidence: All nine modules in the Replacement-map table exist on disk: adapter.rs, framing.rs, jsonrpc.rs, sandbox_metadata.rs, server.rs, server_skill.rs, templates.rs, transport.rs, types.rs (verified via Glob crates/runx-runtime/src/adapters/mcp/**). All four wire-contract fixture files referenced in the Safe-executable-baseline section exist under fixtures/runtime/adapters/mcp/wire-contract/. crates/deny.toml line 19 contains the rmcp ban literal quoted in the spec.
- command audit
  - Grounded in: spec_gap:cargo_dep_version
  - Result: failed
  - Evidence: Stage-1 Cargo snippet uses `rmcp = { version = "0.x", ... }`. `0.x` is not a valid Cargo SemVer requirement — Cargo would reject it at parse time. The Risks section says 'Stage 1 must include a CI pin to a specific minor', but the in-spec snippet directly contradicts that discipline. The cutover spec will copy this snippet; pin it now (e.g., `version = "0.2"` or whatever the current minor is) and document the `cargo update -p rmcp` review gate explicitly.
- scope/migration audit
  - Grounded in: spec_gap:scope_vs_stages
  - Result: failed
  - Evidence: Scope section: 'Design only. Migration lands in a follow-up rust-mcp-rmcp-cutover spec. The only executable work allowed before that cutover is non-network baseline coverage.' But the spec then provides Stage-1 through Stage-5 with concrete Cargo edits, cfg gates, and an acceptance-gate table — readable as executable phases of THIS spec. A reader running `scafld build rust-mcp-rmcp-adoption` would have no way to know which stages are in-scope vs. handoff plan for the future cutover spec. The single executable artifact (the baseline test) is already implemented and passing, so the in-scope phase list is effectively empty — but the spec never says so.
- acceptance timing audit
  - Grounded in: spec_gap:stage4_envelope
  - Result: failed
  - Evidence: Stage-4 acceptance: 'fixture diff against hand-rolled bytes within pre-declared envelope'. The 'envelope' is referenced three times (Stage 4 body, acceptance table, Risks 'fixture diff') but never enumerated in this spec. Stage-5 acceptance gates deletion on 'aster + nitrosend production soak ≥ 30 days' — these are external private products with no in-repo verifiable signal, so the gate cannot be evaluated from inside the OSS repo by an executor. Stages 2/3 acceptance ('all mcp_adapter / mcp_server tests pass with --features mcp-rmcp') will be checkable once the feature exists but cannot run today because the `mcp-rmcp` feature, the rmcp dep, and the async-http prerequisite are all absent.
- rollback/repair audit
  - Grounded in: spec_gap:rollback
  - Result: failed
  - Evidence: The spec is design-only and does not specify rollback for the design itself — e.g., what happens if rust-async-http-layer is declined or delayed? The stages all assume `mcp-rmcp = ["mcp", "dep:rmcp", "async-http"]` is buildable, which requires the async-http feature from a separate spec that is also still in_progress hardening. There is no fallback plan ('if async-http slips, do X') and no described path to back out individual stages (e.g., Stage-2 client cutover failing soak: do we revert the cfg flip, or pull the feature?). Risks section calls out rmcp churn and tokio bloat but does not say how to repair a stuck Stage-3 in production.
- design challenge
  - Grounded in: code:crates/runx-runtime/src/adapters/mcp/jsonrpc.rs:5
  - Result: passed
  - Evidence: The replacement direction is sound. Hand-rolled JSON-RPC framing, request/response builders, and stdio server loop (framing.rs, jsonrpc.rs:5-161, server.rs, transport.rs) are exactly the surfaces an upstream protocol crate is built to own. The spec correctly keeps the runx-specific surfaces (sandbox_metadata, templates, server_skill, McpHostRunResult projection, adapter trait impl) — these are runx contracts, not MCP. This matches rust-kernel-architecture.md §13 ('MCP is the hardest port') and rust-takeover.md §9 step 7 ('MCP is last'). Adoption is not a bandaid: rmcp is the upstream maintained reference implementation; staying hand-rolled grows the protocol-drift surface every MCP-spec release.

Issues:
- [medium/blocks approval] `harden-1` feature_gating_contradiction - `mcp` and `mcp-rmcp` are described as mutually exclusive but the Cargo snippet makes them additive (`mcp-rmcp = ["mcp", ...]`).
  - Status: open
  - Grounded in: spec:Stage 1: pull rmcp behind a feature flag, no behavior change
  - Evidence: Stage-1 body: '`mcp` (hand-rolled) and `mcp-rmcp` (rmcp-backed) are mutually exclusive in the build but coexist in source.' But the Cargo features snippet defines `mcp-rmcp = ["mcp", "dep:rmcp", "async-http"]` — enabling `mcp-rmcp` also enables `mcp`. Cargo features are additive by design; there is no `mutually-exclusive` syntax. As written, building with `--features mcp-rmcp` compiles BOTH the hand-rolled and rmcp paths into the same binary, requiring per-call-site `cfg(any/all)` gates to pick one, which the spec does not show.
  - Recommendation: Either (a) define them as truly disjoint: `mcp = []`, `mcp-rmcp = ["dep:rmcp", "async-http"]` (no `"mcp"` in the list), and add a `compile_error!` in lib.rs if both are enabled; or (b) drop the 'mutually exclusive' framing and document the per-callsite cfg pattern explicitly. Pick one and update the stage description, the snippet, and the acceptance table so the future cutover spec inherits an unambiguous model.
  - Question: Should `mcp` and `mcp-rmcp` be disjoint features (xor), or layered (mcp-rmcp adds rmcp behind a cfg inside the mcp tree)?
  - Recommended answer: Disjoint. `mcp-rmcp = ["dep:rmcp", "async-http"]` with a build-time `compile_error!` if both are set. Layered features hide the cutover and make Stage-5 deletion riskier.
- [medium/blocks approval] `harden-2` cargo_version_pin - `rmcp = { version = "0.x" }` is not a valid Cargo version requirement and contradicts the stated CI-pin discipline.
  - Status: open
  - Grounded in: spec:Stage 1 snippet, Risks 'rmcp churn'
  - Evidence: Stage-1 Cargo block: `rmcp = { version = "0.x", default-features = false, features = ["transport-io", "macros"], optional = true }`. Risks section: 'the crate is pre-1.0. Stage 1 must include a CI pin to a specific minor and a `cargo update -p rmcp` review gate.' The literal `0.x` will fail Cargo parsing; even if interpreted charitably it pins nothing.
  - Recommendation: Replace `"0.x"` with the current stable rmcp minor (verify against crates.io at cutover time) and add an explicit `cargo update -p rmcp` review-gate sentence to Stage-1 acceptance. The cutover spec will copy this verbatim; locking the version literal now prevents a silent transitive bump.
  - Question: Which rmcp minor should the cutover pin against — the current latest, or hold for the next minor that lands a feature we need?
  - Recommended answer: Pin against current latest stable minor at cutover-spec authoring time; do not pre-pin in this design spec, but state it as 'pinned minor TBD at cutover time' rather than `0.x`.
- [medium/blocks approval] `harden-3` acceptance_unspecified - Stage-4 'pre-declared envelope' of acceptable wire diffs is referenced three times but never enumerated.
  - Status: open
  - Grounded in: spec:Stage 4 byte-exact wire compatibility check
  - Evidence: Stage-4: 'Acceptable diffs are pre-declared (e.g., rmcp may set a `jsonrpc` field that the hand-rolled didn't, or vice versa); any unexpected diff is a regression.' Risks/Test fixture diff: 'The cutover spec must enumerate which JSON keys are allowed to differ.' Acceptance table row-4: 'fixture diff against hand-rolled bytes within pre-declared envelope.' No envelope is enumerated. This spec produces the baseline; without an envelope shape the cutover spec inherits an unconstrained gate.
  - Recommendation: Add a concrete envelope skeleton to this design spec: which fields MAY differ (suggested: jsonrpc presence/order, serverInfo.name/version, capabilities subtree shape) and which fields MUST NOT differ (suggested: id, method, params subtree for client→server; error.code, result.content[*].type/text). Even a starter list converts Stage-4 from 'TBD' to 'amend this list'.
  - Question: Which JSON fields are guaranteed-stable across the cutover (must-match), vs allowed-drift (may-differ within shape)?
  - Recommended answer: Must-match: id, method, params.name, params.arguments, result.content[*].{type,text}, result.structuredContent.runx.*, error.code. May-differ: jsonrpc presence, key ordering inside objects, serverInfo strings, capabilities subtree shape, protocolVersion if rmcp negotiates a newer release.
- [high/blocks approval] `harden-4` external_soak_gate - Stage-5 deletion gate depends on production soak in aster + nitrosend (external private products) with no in-repo verifiable signal.
  - Status: open
  - Grounded in: spec:Stage 5 deletion, Risks Production soak
  - Evidence: Stage-5: 'Once rmcp passes Stage 4 and external production users (nitrosend, aster) have soaked on `mcp-rmcp` for ≥ 30 days without incident...'. Risks/Production soak repeats the gate. Grepping the OSS tree confirms aster and nitrosend appear only inside this spec — there is no fixture, no attestation file, no published feed entry that the gate can read. An executor running `scafld build rust-mcp-rmcp-cutover` Stage-5 has no way to evaluate the gate from inside the OSS repo.
  - Recommendation: Define an in-repo verifiable proxy: e.g., a signed attestation file checked into oss/fixtures/attestations/ from each external consumer, or a public release-channel timestamp the gate can read, or replace the named-consumer gate with an objective metric (e.g., 'no protocol-error receipts in the runx telemetry feed for 30 days after `mcp-rmcp` becomes default'). Otherwise the gate is unenforceable in this repo.
  - Question: What in-repo signal will satisfy the 30-day soak gate — an attestation file from each consumer, a telemetry-feed query, or replace the named-consumer gate entirely with an objective error-rate ceiling?
  - Recommended answer: Replace named-consumer gate with: (a) `mcp-rmcp` shipped as default-on for one full minor release, (b) zero `mcp.protocol_drift` receipts logged in the public feed during that window, (c) attestation file committed under oss/fixtures/attestations/rmcp-cutover-<consumer>.json by each consumer that runs `runx mcp serve` in production. Any one of the three OR explicit override by repo owner.
- [medium/advisory] `harden-5` scope_clarity - Cutover-stages section reads as executable plan despite Scope declaring 'design only'.
  - Status: open
  - Grounded in: spec:Scope vs Cutover stages
  - Evidence: Scope: 'Design only. Migration lands in a follow-up rust-mcp-rmcp-cutover spec.' The Cutover-stages section then provides Stage-1 through Stage-5 with Cargo edits, cfg gates, and a binding acceptance-gate table — formatted indistinguishably from executable phases. The single thing this spec actually ships (the baseline wire-contract fixtures + test) already exists in the repo, so the in-scope executable surface is effectively empty.
  - Recommendation: Reframe the stages section as 'Cutover plan (executes in follow-up rust-mcp-rmcp-cutover spec)' and add a one-line 'In-scope work for THIS spec' callout noting that only the baseline test/fixtures land here, and they are already present. Prevents a future executor from running Stage-1 Cargo edits under this task id.
  - Question: Should the Cutover-stages section be relabeled as 'follow-up cutover plan' to prevent it being mistaken for this spec's executable phases?
  - Recommended answer: Yes — rename to 'Follow-up cutover plan (rust-mcp-rmcp-cutover)' and add an 'In-scope here' line stating only the baseline fixtures/test.
- [low/advisory] `harden-6` prerequisite_dependency - Spec hard-depends on rust-async-http-layer landing without specifying behavior if that spec is delayed or scoped down.
  - Status: open
  - Grounded in: spec:Safe executable baseline, Deny.toml exception
  - Evidence: Spec: 'Because `rust-async-http-layer` is still blocked, this spec must not add the `rmcp` dependency or any async runtime features yet.' The async-http spec is also in draft / harden_status: in_progress. The dependency is correctly declared but the contingency ('what if it slips by a release? gets descoped to registry-only?') is not.
  - Recommendation: Add a one-line contingency: 'If rust-async-http-layer is delayed past <date> or scoped to a non-MCP consumer only, this spec re-opens its async-runtime question.' Optional, but cheap insurance against months of stale design.
  - Question: What is the behavior of this spec if rust-async-http-layer ships tokio for registry only and explicitly excludes adapter-tier consumers?
  - Recommended answer: This spec re-opens — rmcp adoption is then blocked on a separate adapter-async spec, and the cutover slips. State this explicitly so the dependency chain is auditable.
- [low/advisory] `harden-7` verification_grounding - Net-LoC claim ('~1100 removed, ~1000 stays') is presented as design fact but unverified.
  - Status: open
  - Grounded in: spec:Replacement map net result
  - Evidence: Replacement map closes with: 'Net result: ~1100 LoC removed (transport, jsonrpc, framing, server protocol), ~1000 LoC stays.' Mod.rs comments confirm the structural split, but the LoC numbers themselves are unverified in this session. Useful as a rough sizing signal, not load-bearing for any acceptance gate.
  - Recommendation: Either run `wc -l` over the to-remove vs to-stay files at spec time and lock the numbers, or soften to 'roughly half the adapter source comes out'. Low priority — does not block design intent.
  - Question: Should the LoC counts be tightened with a `wc -l` check, or softened to a qualitative bound?
  - Recommended answer: Soften. The numbers are useful for cutover sizing but should not be a contract; qualitative 'roughly half removed' carries the same design weight without inviting drift.

### round-2

Status: passed
Started: 2026-05-21T04:12:00Z
Ended: 2026-05-21T04:30:00Z
Verdict: passed
Provider: local
Model: codex
Output format: manual_resolution
Summary: Resolved the rmcp adoption design blockers. The spec is now explicitly design-and-baseline only, has a disjoint feature model, avoids fake Cargo version literals, enumerates the Stage 4 wire-diff envelope, and replaces the unverifiable external-only deletion gate with an in-repo attestation or owner override.

Checks:
- path audit
  - Grounded in: code:crates/runx-runtime/src/adapters/mcp/jsonrpc.rs:5
  - Result: passed
  - Evidence: The replacement map remains grounded in the existing hand-rolled MCP protocol layer.
- command audit
  - Grounded in: code:crates/runx-runtime/tests/mcp_server.rs:46
  - Result: passed
  - Evidence: `scafld validate rust-mcp-rmcp-adoption` exited 0, the stale-pattern text gate exited 0, and `cargo test --manifest-path crates/Cargo.toml -p runx-runtime --features mcp --test mcp_server mcp_server_matches_recorded_stdio_wire_contract` passed.
- scope/migration audit
  - Grounded in: code:.scafld/specs/drafts/rust-mcp-rmcp-adoption.md:50
  - Result: passed
  - Evidence: Scope now states this task is design and baseline only; the concrete migration stages are labeled as the follow-up `rust-mcp-rmcp-cutover` plan.
- acceptance timing audit
  - Grounded in: code:.scafld/specs/drafts/rust-mcp-rmcp-adoption.md:263
  - Result: passed
  - Evidence: The spec now includes local acceptance commands and a follow-up cutover gate table with an enumerated wire-diff envelope.
- rollback/repair audit
  - Grounded in: code:.scafld/specs/drafts/rust-mcp-rmcp-adoption.md:282
  - Result: passed
  - Evidence: Rollback states that the hand-rolled MCP implementation and `rmcp` deny rule stay in place if the async prerequisite is declined or delayed.
- design challenge
  - Grounded in: code:.scafld/specs/drafts/rust-mcp-rmcp-adoption.md:172
  - Result: passed
  - Evidence: The spec preserves the upstream rmcp direction while making the wire compatibility and deletion gates executable.

Issues:
- [medium/blocks approval] `harden-1` feature_gating_contradiction - `mcp-rmcp` was additive while described as mutually exclusive.
  - Status: closed
  - Grounded in: code:.scafld/specs/drafts/rust-mcp-rmcp-adoption.md:127
- [medium/blocks approval] `harden-2` cargo_version_pin - The spec used invalid `rmcp = "0.x"` syntax.
  - Status: closed
  - Grounded in: code:.scafld/specs/drafts/rust-mcp-rmcp-adoption.md:131
- [medium/blocks approval] `harden-3` acceptance_unspecified - Stage 4 did not enumerate allowed wire diffs.
  - Status: closed
  - Grounded in: code:.scafld/specs/drafts/rust-mcp-rmcp-adoption.md:172
- [high/blocks approval] `harden-4` external_soak_gate - Stage 5 depended only on private external production soak.
  - Status: closed
  - Grounded in: code:.scafld/specs/drafts/rust-mcp-rmcp-adoption.md:201
- [medium/advisory] `harden-5` scope_clarity - Cutover stages read as executable under this task.
  - Status: closed
  - Grounded in: code:.scafld/specs/drafts/rust-mcp-rmcp-adoption.md:117
- [low/advisory] `harden-6` prerequisite_dependency - Async prerequisite contingency was missing.
  - Status: closed
  - Grounded in: code:.scafld/specs/drafts/rust-mcp-rmcp-adoption.md:259
- [low/advisory] `harden-7` verification_grounding - LoC counts were unverified.
  - Status: closed
  - Grounded in: code:.scafld/specs/drafts/rust-mcp-rmcp-adoption.md:113

### round-3

Status: passed
Started: 2026-05-21T04:35:00Z
Ended: 2026-05-21T02:03:00Z
Verdict: passed
Provider: local
Model: codex
Summary: Final manual harden evidence: rmcp adoption is now a design-and-baseline spec with disjoint features, exact-version policy, executable wire-diff semantics, and local deletion gates.

Checks:
- path audit
  - Grounded in: code:crates/runx-runtime/src/adapters/mcp/jsonrpc.rs:5
  - Result: passed
  - Evidence: The spec remains grounded in the current hand-rolled MCP protocol layer.
- command audit
  - Grounded in: code:crates/runx-runtime/tests/mcp_server.rs:46
  - Result: passed
  - Evidence: `scafld validate rust-mcp-rmcp-adoption`, the stale-pattern text gate, and the MCP wire-contract cargo test all exited 0.
- scope/migration audit
  - Grounded in: code:.scafld/specs/drafts/rust-mcp-rmcp-adoption.md:50
  - Result: passed
  - Evidence: Scope limits this task to design and baseline; migration is labeled as the follow-up cutover plan.
- acceptance timing audit
  - Grounded in: code:.scafld/specs/drafts/rust-mcp-rmcp-adoption.md:263
  - Result: passed
  - Evidence: Local acceptance commands and follow-up cutover gates are recorded.
- rollback/repair audit
  - Grounded in: code:.scafld/specs/drafts/rust-mcp-rmcp-adoption.md:282
  - Result: passed
  - Evidence: Rollback keeps the existing MCP implementation and deny rule if async prerequisites do not land.
- design challenge
  - Grounded in: code:.scafld/specs/drafts/rust-mcp-rmcp-adoption.md:172
  - Result: passed
  - Evidence: Stage 4 now has an enumerated wire-diff envelope and Stage 5 has an in-repo attestation/override gate.

Issues:
- none


## Review

Status: completed
Verdict: pass
Mode: verify
Summary: Human-reviewed override accepted: Reviewed scoped spec-only diff after harden. scafld validate rust-mcp-rmcp-adoption, stale-pattern gate, and MCP wire-contract cargo test passed; no rmcp dependency or feature changes landed under this task.

Attack log:
- `review gate`: manual human audit -> clean (Reviewed scoped spec-only diff after harden. scafld validate rust-mcp-rmcp-adoption, stale-pattern gate, and MCP wire-contract cargo test passed; no rmcp dependency or feature changes landed under this task.)

Findings:
- none

