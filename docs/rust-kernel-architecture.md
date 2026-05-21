# Rust kernel architecture

Status: draft, prerequisite for `rust-runx-cli-placeholder`,
`rust-cli-feature-parity-matrix`, `rust-kernel-parity-fixtures`,
`rust-state-machine-parity`, `rust-policy-parity`, and
`rust-parity-ci-governance`.

This document captures the architectural decisions that the Rust parity
specs depend on. The goal is to make the choices explicit once, so each spec
can reference them rather than rederive them.

## 1. Position

The Rust kernel started as conformance evidence for the TypeScript trusted
kernel, but the local runtime cutover is now underway. For local execution,
Rust is the canonical owner once a command is advertised by the native CLI.
TypeScript remains a client, package, product UX, docs, and compatibility-test
surface unless a separate spec gives it ownership of a cloud or authoring
boundary. Rust crates exist to:

- Prove behavioral parity through a shared fixture suite while a domain is
  still in the dual-tree window.
- Make TypeScript kernel drift explicit (intentional fixture refresh required).
- Provide the native local CLI/runtime path for skill, graph, harness, receipt,
  history, policy, authority, payment, and adapter orchestration.

Rust is not a second source of truth for cut-over surfaces; it is the source of
truth. If Rust and TypeScript disagree on a fixture before a surface cuts over,
the active cutover spec decides whether the fixture is still a TypeScript
oracle or a Rust contract fixture.

## 2. Pure kernel scope

"Pure kernel" in this document means exactly what the existing boundary check
enforces, not what the trusted-kernel doc lists. `oss/scripts/check-boundaries.mjs`
defines `pureCoreDomains = ["parser", "policy", "state-machine"]` and forbids
those domains from importing `fs`, `child_process`, `http`, `net`, and the
other node IO modules.

By line count, the trusted-kernel surface under `packages/core/src/` is
~11,300 lines across ten domains. Of those, only five are currently
node-import-free:

- `executor` (369 lines)
- `marketplaces` (245 lines)
- `parser` (1658 lines)
- `state-machine` (667 lines)
- `policy` (1150 lines, after the `node:path` removal in
  `rust-kernel-parity-fixtures`)

Total pure surface: ~4,090 lines, or roughly 36% of trusted-kernel-by-LOC.
This plan ports state-machine + policy (~1,820 lines), which is 100% of
what `pureCoreDomains` enforces today. The remaining pure-by-imports domains
(`executor`, `marketplaces`, `parser`) are candidates for follow-up parity
specs; their boundary status would need to be added to `pureCoreDomains`
before a Rust port is meaningful.

The other five domains (`artifacts`, `config`, `knowledge`, `receipts`,
`registry`) use node modules and would need TS-side purification or a
clean split into pure-decision + impure-IO halves before they could move
to Rust.

So the scope of this plan is narrow but defensible: it covers exactly what
the existing repo defines as pure. Future scope expansion is a sequence of
explicit follow-up specs, not a vague "port the kernel" promise.

## 3. Target crate graph

The future workspace under `oss/crates/`:

```
runx-contracts    pure public contracts: CLI JSON, host protocol, receipts,
                    registry/tool records, act assignment
                    deps: serde, sha2
                    deferred deps: serde_json/thiserror only when concrete code
                                   needs them outside tests

runx-core         pure decisions: state-machine, policy, scope, sandbox normalization
                    deps: runx-contracts as needed, serde, thiserror as needed,
                          serde_json for private deterministic JSON canonicalization
                    posture: std default; no_std deferred to a follow-up spec

runx-parser       pure: YAML -> AST -> intermediate representation
                    deps: runx-contracts, runx-core, serde, serde_norway,
                          serde_json, regex, thiserror
                    posture: public raw object subtrees use
                             `runx_contracts::JsonValue`; execution
                             semantics use `runx_contracts::execution`;
                             sandbox normalization uses `runx_core::policy`

runx-receipts     pure: receipt model, hashing helpers, verification rules
                    deps: runx-contracts, serde, sha2

runx-sdk          library: blocking CLI-backed SDK v0; future async path
                    deps: runx-contracts only in v0
                    explicit non-dep: runx-core in v0

runx-runtime      impure: filesystem, subprocess, network, adapters, MCP,
                  sandbox enforcement
                    default features: none
                    opt-in features: cli-tool, mcp, a2a, agent, catalog
                    deps: runx-contracts, runx-core, runx-parser,
                          runx-receipts; async/network dependencies require
                          explicit adapter-tier exception specs

runx-cli          binary: argument parsing, presentation, exit codes
                    includes: skill authoring subcommands until a separate
                              authoring library use case exists
                    deps: runx-runtime (long-term)
                    current: native local command surface for advertised Rust
                             commands; npm package is selector/client wrapper
```

Pure crates (`runx-contracts`, `runx-core`, `runx-parser`, `runx-receipts`,
and the CLI-backed v0 `runx-sdk`) depend only on each other when that coupling
is needed plus parsing, hashing, and serde-style support crates. `runx-parser`
depends on `runx-contracts` for JSON and execution-semantic boundary types and
on `runx-core` for sandbox normalization, so parser parity exercises the same
typed Rust surfaces the future runtime will consume.
`runx-sdk` is special: it is pure library code plus a blocking CLI client, but
it is not part of the trusted kernel and must not depend on `runx-core` in v0.
Crates below the runtime line own all side effects. The boundary between pure
and impure is enforced both by dependency direction and by `cargo-deny`/lint
rules (see section 10).

Parser parity uses `serde_norway` as the YAML backend. Raw object subtrees use
`runx_contracts::JsonValue`, and execution semantics are validated into the
`runx_contracts::execution` types so parser fixtures detect drift against the
contracts crate instead of carrying a duplicate local model.

Order of operations is committed, not loose. Pure crates ship first:

1. `rust-contracts-bootstrap`: crate graph, placeholder reservation versioning, and
   `runx-contracts` placeholder guardrails. This is the pre-kernel execution
   gate.
2. `runx-core` (this plan): state-machine + policy parity.
3. `runx-contracts`: public JSON/host/receipt contract parity for SDK/runtime.
   Follow-up spec.
4. `runx-parser`: pure YAML/AST/IR parity. Implemented through
   `rust-parser-parity` for graphs, skills, runner manifests, tool manifests,
   and skill installs.
5. `runx-receipts`: pure receipt model + verification rules. Follow-up spec.

Only after the initial pure set passes parity does any impure crate begin:

6. `runx-sdk` CLI-backed v0 can ship once its consumed `runx-contracts`
   subset and CLI JSON cases are fixture-backed.
7. `runx-runtime` skeleton with one impure adapter ported as a runtime feature
   (cli-tool first; MCP last because rmcp + tokio + sandbox + spawn semantics
   are the hardest cross-language surface).
8. `runx-cli` native binary, gated by `rust-cli-feature-parity-matrix`.
9. `runx-sdk` native-runtime feature, gated by the same runtime and CLI
   feature-parity evidence.

Each step is its own design pass. MCP cannot jump the queue.

`runx-sdk` has a special early path: a CLI-backed Rust SDK can ship before
`runx-runtime` exists, as long as it calls the authoritative `runx` binary and
only consumes documented JSON output from `runx-contracts`. That early SDK is
a blocking client wrapper and host-protocol type layer, not a native runtime.
Once `runx-runtime` exists, a separate spec may add the async SDK path. The
likely shape is `runx-sdk` exposing async APIs by default with a `blocking`
facade feature or sibling facade module, but that is not part of v0. SDK v0 is
allowed to block because it is only a subprocess-backed bridge to the current
CLI.

contracts-first-ordering: `runx-contracts` owns host protocol, capability
execution, idempotency hashes, and consumed JSON contract types before SDK
Phase 2. `runx-sdk` may depend on `runx-contracts` in CLI-backed v0, but it
must not duplicate contract-owned types or hash helpers.

There is no `runx-authoring` crate in the initial Rust shape. Skill authoring
helpers live in `runx-cli` subcommands or `runx-sdk` modules until there is a
clear library caller who needs authoring without either surface. The TypeScript
package split is useful history, not a forcing function for Cargo crates.

There is also no `runx-adapters` crate in the initial Rust shape. Adapter
families live under `runx-runtime` feature flags (`cli-tool`, `mcp`, `a2a`,
`agent`, `catalog`) until an adapter family has an independent publishing
story.

There is no umbrella `runx` crate in the initial Rust shape. The `runx` crate
name is already taken by an unrelated crate, and the installable user-facing
surface is `runx-cli` with a binary named `runx`. If the name ever becomes
available or transferred, an umbrella crate can be proposed in a separate spec;
until then consumers depend on the specific crate they need.

## 4. `runx-core` public API stance

`runx-core` is library-only and not published in this phase. Its public API is
shaped for two consumers:

- Fixture-runner tests (internal to this monorepo).
- Future internal consumers (`runx-parser`, `runx-receipts`,
  `runx-runtime`, `runx-cli`).

Stability rules during the parity phase:

- The public surface is unstable. No SemVer guarantee. The crate version
  stays `0.0.x`.
- Every module is `pub mod` only if a fixture or sibling crate consumes it.
  Internal helpers stay private.
- Re-exports at crate root follow the TypeScript export shape: one module per
  TS sub-module (`state_machine`, `policy`, `policy::sandbox`,
  `policy::authority_proof`, `policy::public_work`, `policy::scope`).
- Naming preserves runx vocabulary. `admit_local_skill` matches
  `admitLocalSkill`. No invented aliases.

Publication to crates.io follows section 14.

## 5. Error and decision model

TypeScript policy returns discriminated decision objects, for example
`{ status: "approved", grant }` or `{ status: "rejected", reason }`.

Rust mirrors this shape with enums, not `Result`:

```rust
pub enum AdmissionDecision {
    Approved(LocalAdmissionGrant),
    Rejected { reason: AdmissionRejectionReason },
}
```

Rationale:

- `Result<Grant, Reason>` would imply rejection is exceptional. In policy code
  it is a normal, expected outcome.
- Fixture JSON encodes both arms uniformly via the discriminator field.
- Callers can `match` exhaustively; new variants are breaking changes that
  surface at compile time.

Rejection reasons are typed enums (`AdmissionRejectionReason`,
`SandboxRejectionReason`, etc.), not free-form strings. The reason value in
fixtures uses the serde-renamed enum variant name.

Panics are forbidden in `runx-core`. The workspace `Cargo.toml` already denies
`clippy::panic` and `clippy::unwrap_used` for the launcher; the same lints
apply to all pure crates.

## 6. Serde conventions

Fixture JSON is the cross-language contract. Conventions:

- All public types derive `serde::Serialize` and `serde::Deserialize`.
- Struct fields use `#[serde(rename_all = "camelCase")]` to match TypeScript
  emit. This is the default for the whole crate.
- Tagged unions use `#[serde(tag = "status")]` or `#[serde(tag = "kind")]`
  matching the discriminator field name from TypeScript. Per-union choice is
  documented next to the type.
- Enum variants without payloads use `#[serde(rename_all = "kebab-case")]` to
  match TS string union values such as `"in-progress"`, `"on-failure"`.
- Optional fields use `Option<T>` with `#[serde(skip_serializing_if = "Option::is_none")]`
  so fixture JSON matches TypeScript's omitted-key behavior.
- No `#[serde(default)]` on required fields. Missing fields are errors, not
  silently zero-valued.
- Deduplicated arrays preserve first-seen insertion order from the TypeScript
  oracle. Rust ports use `Vec` plus insertion-preserving deduplication
  (`IndexSet` or equivalent) for serialized arrays such as `requestedScopes`
  and `grantedScopes`, not `HashSet` or `BTreeSet`.

A single `crates/runx-core/src/serde_conventions.rs` module documents these
rules in code comments and exposes a tiny test that round-trips a few golden
values, so the rules are not just prose.

## 7. Platform-sensitive behavior

The TypeScript policy module imports `node:path` for `path.basename()` in
`normalizeExecutableName` ([packages/core/src/policy/index.ts:3](../packages/core/src/policy/index.ts#L3)).
Node's path module is OS-aware: on Windows it treats `\` as a separator, on
POSIX it does not.

Decision: fixtures use POSIX semantics only. Executable names that contain
backslashes are normalized as if the separator were `/`, regardless of host
OS. Rationale:

- Fixtures are deterministic if and only if path semantics are platform-free.
- Cross-platform `node:path` behavior produces different results for the same
  input string between Windows and POSIX runners; this would make fixtures
  host-dependent.
- The Rust port implements its own `posix_basename` helper rather than using
  `std::path::Path`, which is platform-aware.

If a real runtime consumer ever needs Windows-aware path handling, that lives
in `runx-runtime`, not in `runx-core`.

This also makes strict CLI-tool inline-code admission deterministic across
hosts. A command such as `C:\Tools\node.exe` normalizes to `node` everywhere,
so inline `-e`/`--eval` style invocations are denied consistently instead of
being bypassed on POSIX runners because backslashes were treated as ordinary
filename characters.

A TypeScript-side change is in scope for the fixtures spec: replace the
`node:path` import with a small `posixBasename` helper. This makes the kernel
truly side-effect-free (it currently imports a node-only module) and aligns
both languages on the same semantics. Flag for review.

## 8. Standard library posture

`runx-core` uses `std` by default and does not gate `no_std` from day one.

Rationale:

- No concrete consumer needs `no_std` today.
- `serde_json` with `no_std` requires `alloc` and a specific feature dance
  that adds friction to every dependency add. The kernel is small (<2,000
  lines once ported); retrofitting `no_std` later if a real embedded or
  embedded-WASM consumer materializes is a half-day of work.
- WASM works fine with `std`. The hypothetical "in-browser preview" path
  does not require `no_std`.

If a kernel-adjacent crate (`runx-receipts` for signing helpers in embedded
contexts, for example) ever ships a real `no_std` requirement, that decision
is revisited as a follow-up spec; it does not constrain this plan.

## 9. MSRV and edition

- Edition: 2024.
- Resolver: 3.
- MSRV: 1.85.0 (the first Rust release with edition 2024 support).
- MSRV pinned in `crates/Cargo.toml` workspace `[workspace.package]` block and
  enforced in CI via `rust-toolchain.toml` or an explicit toolchain in the
  workflow.
- MSRV bumps are spec-level changes, not silent dependency updates.

## 10. Rust-side boundary enforcement

The TypeScript boundary script ([oss/scripts/check-boundaries.mjs](../scripts/check-boundaries.mjs))
forbids node APIs from `policy` and `state-machine` packages. The Rust side
needs the same discipline. Layered enforcement:

1. **Dependency direction**: `runx-core/Cargo.toml` lists only
   `runx-contracts`, `serde`, `serde_json`, and narrow test-only tools.
   `serde_json` is allowed for private deterministic JSON canonicalization,
   but public APIs expose `runx_contracts::JsonValue`, not
   `serde_json::Value`. `runx-parser` also exposes parser raw object subtrees
   as `runx_contracts::JsonValue` and validates execution metadata into the
   `runx_contracts::execution` types. No `tokio`, `reqwest`, `hyper`, `clap`,
   `rmcp`.
   Enforced by dependency direction and by the workspace `cargo-deny`
   default ban. The ban is intentionally stricter than "pure crates only"
   because `cargo-deny` is workspace-scoped in this track; adapter/runtime-tier
   exceptions must be introduced by an explicit spec before the dependency is
   removed from `deny.toml`.

2. **API surface lint**: `cargo-public-api` snapshots the public API. A diff
   against the snapshot in CI flags accidental surface growth.

3. **Forbidden imports**: a lightweight build-time check (a `build.rs` is
   overkill; a CI step running `cargo +nightly rustdoc -Z unstable-options`
   or a simple grep over the crate's compiled deps tree) ensures
   `std::process`, `std::fs`, `std::net`, `std::time::SystemTime`,
   `std::env` are not referenced from `runx-core/src`. The grep is fragile
   but acceptable as a defense-in-depth signal alongside cargo-deny.

4. **Boundary check in TS**: continues to apply. The existing
   `pnpm boundary:check` is the source of truth for TS-side enforcement; the
   Rust checks above mirror it on the Rust side.

`cargo-deny` configuration lives at `oss/crates/deny.toml` and is referenced
from CI.

## 11. Property and differential testing

Fixture-only testing covers known cases. For state-machine logic with several
enums and fanout sync semantics, the long tail is large.

Plan:

- Phase 1 (fixtures spec): checked-in fixtures only. Enough to prove the
  contract works.
- Phase 2 (state-machine spec): add `proptest` strategies for graph state
  transitions. Same generated inputs run through both languages via a
  TypeScript subprocess invoked from a Rust integration test, or vice versa.
  Differential failures pin a counterexample as a new fixture.
- Phase 3 (policy spec): proptest strategies for admission and scope
  narrowing inputs.

Differential testing is optional in early phases but is the only realistic
way to catch parity drift on combinatorial state spaces. Each parity spec
declares whether it adopts it.

## 12. Dual-tree maintenance policy

Once parity exists, the maintenance cost is real. The policy is staged:

- **Phase A (advisory)**: Rust parity runs in CI through
  `scripts/check-rust-kernel-parity.mjs`, but failure is a warning.
  TypeScript developers can break fixtures and regenerate them. A failing
  Rust check produces a CI annotation but does not block merge. The local
  command is `pnpm rust:check`.
- **Phase B (blocking)**: After 5 clean kernel-touching PRs land green in
  Phase A, Rust parity blocks merge. Every PR that touches
  `packages/core/src/state-machine/` or `packages/core/src/policy/` must
  either pass Rust parity or include an intentional fixture refresh
  (regenerated via the parity script).

Calendar time is not the trigger. If the kernel doesn't churn for weeks, the
soak proves nothing; if it churns daily, calendar time is too coarse. PR
count maps to actual exposure.

Expected cost after promotion: roughly +4 to +8 hours per kernel-touching
PR for the dual-tree update (TS change, fixture regen, Rust port, Rust
clippy/proptest update, public-API snapshot bump). Budget this explicitly;
do not pretend it is free.

The transition between phases is a deliberate decision in the follow-up
`rust-kernel-blocking-promotion` spec, not automatic.

### TS sunset trigger

The dual tree is not the destination. The gating oracle for retiring
TypeScript implementations is `rust-cli-feature-parity-matrix`. Once that
matrix passes against a Rust runtime candidate, the cutover is triggered.

Sunset order (each step is its own cutover spec, not implicit):

1. Replace TS state-machine consumers with `runx-core::state_machine`.
   Delete `packages/core/src/state-machine/`.
2. Replace TS policy consumers with `runx-core::policy`.
   Delete `packages/core/src/policy/`.
3. Port and delete `parser`, `executor`, `marketplaces` (pure-by-imports
   trusted-kernel domains).
4. Port impure trusted-kernel domains (`artifacts`, `config`, `knowledge`,
   `receipts`, `registry`) and runtime-local. Each requires TS-side
   purification or pure/impure split first.
5. Cut npm `@runxhq/cli` over to Rust binary entry. Delete the
   Node-launcher path from `crates/runx-cli`.
6. Move `runx-sdk` from CLI-backed mode to `native-runtime` once the runtime
   cutover is complete. Until then, the SDK remains a Rust client over the
   authoritative CLI and shared `runx-contracts` types.

Until step 1 is approved as its own spec, the dual tree is the operating
state, and the cost in section 12 applies.

## 13. CLI cutover position

`crates/runx-cli` is now the native local command surface for the Rust-backed
commands it advertises. Help text is a contract: if a form appears in
`runx --help`, it must either execute through Rust or fail closed before any
hidden TypeScript fallback.

The npm `@runxhq/cli` package remains a platform-aware launcher/client wrapper,
but local execution semantics move through `runx-runtime`, not through
`@runxhq/runtime-local`.

New native command forms still require parity evidence:

- `fixtures/cli-parity` exists and covers every current command, subcommand,
  flag, exit code, JSON output shape, human-output promise, receipt behavior,
  sandbox metadata path, adapter path, and documented workflow.
- `runx-core`, `runx-parser`, and `runx-receipts` exist and pass parity.
- A `runx-runtime` crate exists with at least one impure adapter ported.
- A command-specific cutover or hardening spec proposes the move.

Kernel parity is not CLI parity. A Rust state-machine or policy port can prove
that pure decisions match TypeScript, but it does not prove that the executable
CLI is a drop-in replacement. Native CLI candidates must run against the
fixture matrix and pass one-to-one feature parity before npm wrappers may
present them as canonical.

The one-to-one CLI matrix belongs in `fixtures/cli-parity/` and is governed by
the `rust-cli-feature-parity-matrix` spec. The matrix is intentionally broader
than kernel parity. It includes `skill`, `evolve`, `resume`, `replay`, `diff`,
`search`, `add`, `inspect`, `history`, `export-receipts`, `knowledge show`,
`connect`, `config`, `new`, `init`, `harness`, `list`, `doctor`, `dev`,
`mcp serve`, `tool search`, `tool inspect`, and `tool build`, plus aliases and
JSON/non-JSON modes.

## 14. Placeholder publishing strategy

Cargo placeholders are also a crates.io name reservation strategy. The policy
is explicit:

- `runx-cli` publishes as the launcher package because it installs a useful
  `runx` binary today. It is live at `0.1.0`.
- Placeholder crates publish as explicit reservation releases at `0.0.1`.
  `runx-contracts`, `runx-receipts`, `runx-runtime`, and `runx-sdk` are live
  at `0.0.1`.
- `runx-core` was reserved at `0.0.1` and now contains the first real Rust
  kernel surfaces: state-machine parity and policy parity. runx-core policy parity is not runtime-authoritative; it remains conformance evidence only
  until a cutover spec replaces TypeScript consumers.
- `runx-parser` was reserved at `0.0.1` and now contains parser parity for the
  public TypeScript parser surfaces listed in its README. It remains
  conformance evidence until a TypeScript parser sunset spec replaces current
  consumers. It is marked `publish = false`, depends on the local
  `runx-contracts` and `runx-core` crates for shared boundary types, and its
  package check verifies those three crates together so Cargo does not resolve
  stale placeholder reservations from crates.io.
- Placeholder README and crate docs must clearly say they are placeholders and
  do not provide native feature parity.
- Placeholder publishing is governed by `rust-placeholder-crates-publish`.
- The publish order must follow dependency direction: `runx-contracts`,
  `runx-parser`, `runx-receipts`, `runx-runtime`, `runx-sdk`, with
  `runx-core` versioned independently as parity lands and `runx-cli`
  independent as the usable launcher package.
- The first non-placeholder release of each crate requires its own
  fixture-backed parity spec.

## 15. Path-inconsistency note

The repo-root `docs/trusted-kernel-package-truth.md` remains the broad package
authority document for the full runx repository. The OSS workspace also keeps
`oss/docs/trusted-kernel-package-truth.md` as a Rust-parity addendum so scafld
specs executed from `oss/` have a stable local docs path.

## 16. Open questions intentionally deferred

- Whether to adopt `bon` or another builder crate for the larger value types.
- Whether to expose a C ABI for non-Rust hosts (likely no, but not decided).
- Whether the Rust port targets a single big crate or splits state-machine
  and policy into their own crates from day one. Current decision: single
  crate. Split is a follow-up if the public surface grows too large.
- Whether a `runx-macros` crate is ever justified. There is no macros crate
  placeholder now. Procedural macros need a separate spec because they add
  build complexity and are easy to overuse.

## 17. Current Rust Kernel Status

`crates/runx-core` now implements state-machine parity against the checked-in
TypeScript oracle fixtures. This is conformance evidence only: TypeScript is
still authoritative and no TypeScript consumer has been replaced.
`crates/runx-core` also implements policy parity for the current fixture-backed
policy surface. `crates/runx-contracts` now carries typed act-assignment
and host-protocol contracts with TypeScript-generated parity fixtures. Parser,
receipts, registry, tools, runtime, SDK, and native CLI cutover remain follow-up
spec tracks.

## 18. Rust implementation quality bar

The Rust port must read like Rust, not like TypeScript mechanically translated
into Rust syntax. The source of truth for behavior is TypeScript; the source
of truth for Rust shape is Rust's own API and style conventions.
Fixture and wire parity are mandatory; internal names, module boundaries, and
helper structure should actively improve when the TypeScript shape is awkward
or less idiomatic in Rust.

Code shape rules:

- Use Rust 2024 idioms: `let else`, `matches!`, `is_some_and`,
  `Option::then_some`, exhaustive `match`, and iterator combinators where they
  simplify control flow. Do not write clever iterator pipelines when a short
  loop is clearer.
- Follow Rust API Guidelines naming: modules, functions, and values use
  `snake_case`; types, traits, and enum variants use `UpperCamelCase`; error
  types use verb-object-error naming when an error type is needed.
- Public APIs preserve runx vocabulary but not TypeScript casing:
  `admitLocalSkill` becomes `admit_local_skill`, not a generated alias.
- Prefer small value types, enums, and `match` over stringly typed records.
  `serde_json::Value` is allowed in fixture tests, but not in the public
  `runx-core` API.
- Use `BTreeMap` for serialized maps whose key order reaches fixture JSON.
  Do not use `HashMap` in `runx-core` unless the value never crosses a
  serialization boundary and the spec explains why.
- Keep helpers private by default. A function becomes `pub` only when a
  fixture runner or sibling crate needs it. Do not use `pub use *`.
- Avoid macro-heavy abstractions. `derive` is fine; bespoke macros require a
  spec-level justification.
- Avoid builder crates and fluent builders in `runx-core` unless a type has a
  real optional-field explosion. Plain structs and constructors are preferred.
- Avoid clone-driven design. Small enums can derive `Copy`; larger values are
  borrowed by slice/reference where straightforward. Cloning at fixture or
  serde boundaries is acceptable.
- Keep modules scoped. A Rust source file above roughly 350 lines or a
  function above roughly 60 logical lines needs a short comment in the spec
  receipt explaining why it is still the clearest shape.

Error and failure rules:

- No `unsafe`, `panic!`, `todo!`, `unimplemented!`, `dbg!`, `unwrap`, or
  `expect` in `runx-core` production code.
- No `anyhow`, `eyre`, `Box<dyn Error>`, or dynamic error erasure in
  `runx-core` public APIs. Policy decisions are normal enum values, not
  errors. Actual validation errors use concrete enums or structs, with
  `thiserror` preferred when deriving `Display` and `std::error::Error` keeps
  the implementation smaller and clearer.
- Tests may return `Result` and use `?`; fixture loaders should not rely on
  `unwrap` or `expect`.

Async and blocking rules:

- `runx-contracts`, `runx-core`, `runx-parser`, and `runx-receipts` do not
  depend on `tokio`, `async-trait`, HTTP clients, or subprocess libraries.
- `runx-runtime` owns process management, network IO, MCP, sandbox
  enforcement, and adapter concurrency. It may own an async runtime or MCP/HTTP
  protocol crate only after a spec records the security rationale and updates
  `crates/deny.toml`; until then the workspace ban is deliberate.
- `runx-runtime` defaults to no adapter features. Adapter families are opt-in:
  `cli-tool`, `mcp`, `a2a`, `agent`, and `catalog`.
- `runx-sdk` v0 is explicitly a blocking CLI-backed client and depends on
  `runx-contracts`, not `runx-core` or `runx-runtime`. A future async SDK path
  requires its own spec and contract fixtures.
- `runx-cli` may bridge into the async runtime once it is native, but must not
  bypass `runx-runtime` by calling pure crates directly for runtime behavior.

Workspace policy:

- Commit the single workspace lockfile at `crates/Cargo.lock`. This workspace
  contains the `runx-cli` binary plus publishable library crates, so the lock
  file is part of reproducible CI.
- `cargo-nextest` is not required for placeholder crates. It becomes a good
  follow-up once the Rust workspace has enough tests for nextest to materially
  improve CI feedback.

Enforcement:

- `cargo fmt --all --check` is required.
- `cargo clippy -p runx-core --all-targets -- -D warnings` is required.
- `scripts/check-rust-core-style.mjs` checks repository-specific shape rules
  that Clippy does not know: no public `serde_json::Value`, no `HashMap` in
  `runx-core/src`, no wildcard re-exports, no dynamic error erasure, no macro
  definitions, and line-count warnings for oversized files/functions.
- `scripts/check-rust-crate-graph.mjs` checks crate membership, placeholder
  reservation versioning, and dependency direction. Dependency relaxation is a
  spec-level change.
- `cargo-public-api` snapshots ensure "just make it pub" does not become the
  easy escape hatch.

This bar deliberately avoids `clippy::pedantic` as a global deny. High-signal
lints are required; style churn is not.

## References

- [docs/trusted-kernel-package-truth.md](../../docs/trusted-kernel-package-truth.md)
  (repo-root docs)
- [oss/scripts/check-boundaries.mjs](../scripts/check-boundaries.mjs)
- [oss/packages/core/src/state-machine/index.ts](../packages/core/src/state-machine/index.ts)
- [oss/packages/core/src/policy/index.ts](../packages/core/src/policy/index.ts)
- [oss/crates/runx-cli/src/main.rs](../crates/runx-cli/src/main.rs)
