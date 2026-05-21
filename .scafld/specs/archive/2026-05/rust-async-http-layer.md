---
spec_version: '2.0'
task_id: rust-async-http-layer
created: '2026-05-21T03:00:00Z'
updated: '2026-05-21T02:04:47Z'
status: completed
harden_status: passed
size: medium
risk_level: high
---

# Rust async HTTP layer

## Current State

Status: completed
Current phase: final
Next: done
Reason: task completed
Blockers: none
Allowed follow-up command: `none`
Latest runner update: 2026-05-21T02:04:47Z
Review gate: pass

## Why this exists

`runx-runtime`'s registry and connect surfaces talk to remote HTTP endpoints
through `hosted_http.rs`, which is a thin wrapper around a `curl` subprocess.
This is deliberate per the architecture doc: the parity workspace has banned
`tokio`, `reqwest`, `hyper`, `ureq`, and `async-std` in `crates/deny.toml`
because no spec yet justifies introducing an async runtime.

The blocking-curl approach is fine for low-frequency calls (registry search,
single grant fetch). It will not scale to:

- The launcher flip workload, where every `runx skill` invocation may resolve
  a registry skill, fetch a profile, fetch attestations, and post a receipt.
- Adapter-tier consumers that need to dispatch multiple parallel HTTP calls
  (e.g., MCP servers, A2A peers, hosted agents).
- Connect-flow polling loops that would otherwise spawn N curl processes per
  poll cycle.

This spec defines the dependency policy and migration shape for the *single,
scoped* introduction of an async runtime and HTTP client to the runtime crate.
It does not land the dependencies or wire call sites by itself.

## Scope And Touchpoints

In scope:

- Design the adapter-tier exception policy for `tokio`/`reqwest`/TLS without
  weakening pure-kernel dependency boundaries.
- Define the fallible, panic-free runtime helper shape future cutover specs
  must use.
- Define feature-gating and migration sequencing for registry, connect, MCP,
  and other adapter-tier consumers.

Out of scope:

- Editing `crates/deny.toml`, `Cargo.toml`, or runtime source under this task.
- Replacing `hosted_http.rs` or removing curl.
- Enabling `async-http` from `cli-tool`, `mcp`, `a2a`, `agent`, or `catalog`.
- Choosing exact dependency versions before the implementation spec reviews
  the current crate graph.

Future cutovers land behind feature gates and per-call-site specs such as
`rust-async-http-cutover-registry`, `rust-async-http-cutover-connect`, and
`rust-async-http-cutover-hosted-http-removal`.

## Choices

### Async runtime: `tokio` (single-threaded current-thread by default)

Why `tokio`:

- Ecosystem default; reqwest pulls it transitively.
- `current_thread` flavor adds ~50 KB and no OS threads; the runtime crate
  remains predominantly synchronous code that hops into the runtime only at
  HTTP call sites.
- A private runtime helper can preserve the existing blocking public surface
  while future async entrypoints use native `async` calls.

Rejected:

- `async-std`: smaller ecosystem, ABI churn, reqwest incompatible.
- Pure futures executors (e.g., `futures::executor::block_on`): no I/O
  driver; can't drive reqwest.
- Roll-our-own `hyper` directly: TLS, retries, redirects, decompression are
  all reinventions the workspace will regret in 6 months.

### HTTP client: `reqwest` with `default-features = false`, opt-in features

Why `reqwest`:

- Drop-in replacement for the curl-subprocess surface. Synchronous-looking
  API exists via `reqwest::blocking::Client` if needed for migration
  staging.
- TLS via `rustls-tls` (no native dependency on macOS keychain or Windows
  CryptoAPI).
- Native gzip/brotli decompression, redirects, connection pooling.

Implementation cutovers must choose exact reviewed versions at the time they
land. The dependency lines must use exact pins (`=<major.minor.patch>`), not
wildcards or broad ranges. Required feature shape:

- `reqwest`: `default-features = false`, features `rustls-tls`, `json`, `gzip`.
- `tokio`: `default-features = false`, features `rt`, `net`, `time`.

No `default-features = true`. No `blocking` feature. No `cookies`. No
`stream` until a specific consumer needs it.

Rejected:

- `ureq`: blocking only; no async. Adopting it would require yet another
  migration when an async consumer appears.
- `surf` / `isahc`: smaller ecosystems; pull libcurl back in transitively.

### Feature gating

```toml
[features]
default = []
async-http = ["dep:reqwest", "dep:tokio"]
```

Adapter-tier consumers (`cli-tool`, `mcp`, `a2a`, `agent`, `catalog`) do not
enable `async-http` by default in this spec. `async-http` remains an
orthogonal leaf feature until a cutover spec proves a call site should require
it. Each consumer that needs it must enable it explicitly:

```toml
[features]
catalog = ["cli-tool", "async-http"]
```

This preserves the pure default build for kernel-parity testing and keeps the
decision about whether `cli-tool` implies `async-http` with the registry or
connect cutover that actually removes curl.

## `deny.toml` exceptions

Do not remove these bans under this design spec:

- `reqwest`
- `tokio`

The first implementation cutover that adds the dependencies must:

- keep pure crates (`runx-contracts`, `runx-core`, `runx-parser`,
  `runx-receipts`) free of `tokio`, `reqwest`, `hyper`, and raw network
  clients;
- remove or scope the `reqwest` and `tokio` ban only in the same commit that
  adds exact reviewed dependencies and a passing `cargo deny check`;
- prefer per-crate license exceptions over broadening the global license
  allowlist;
- keep `hyper` banned as a direct dependency. It may appear only transitively
  through the reviewed HTTP client graph.

Keep banned by default:

- `hyper` — only allowed transitively via reqwest, not as a direct dep
- `async-std`, `ureq` — explicitly not allowed
- `axum` — server framework, separate spec needed if ever required

## Runtime lifecycle

The runtime crate exposes two private surfaces behind `async-http`:

1. An async client path used by any code already running inside a tokio
   context, including future rmcp handlers.
2. A blocking bridge for today's synchronous registry/connect callers.

The blocking bridge must be panic-free and must not call `block_on` from
inside an active tokio runtime. Shape:

```rust
fn async_runtime() -> Result<Arc<tokio::runtime::Runtime>, RuntimeError> {
    static RUNTIME: OnceLock<Result<Arc<tokio::runtime::Runtime>, String>> =
        OnceLock::new();

    let initialized = RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .map(Arc::new)
            .map_err(|error| error.to_string())
    });

    initialized
        .as_ref()
        .map(Arc::clone)
        .map_err(|message| RuntimeError::AsyncRuntimeUnavailable {
            message: message.clone(),
        })
}

fn block_on_http<T>(
    future: impl Future<Output = Result<T, RuntimeError>>,
) -> Result<T, RuntimeError> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(RuntimeError::BlockingHttpInsideAsyncRuntime);
    }

    async_runtime()?.block_on(future)
}
```

No `unwrap`, `expect`, `panic`, or `println` is permitted in the helper. The
public API stays blocking until a cutover spec adds an async public surface.
Any code already inside a tokio runtime must call the async client directly;
it must not go through the blocking bridge.

## Migration plan

Each cutover lands as a separate, narrowly scoped spec. These IDs are reserved
names; create them before the first dependency exception lands:

1. `rust-async-http-cutover-registry` — replace `hosted_http`'s curl
   subprocess with reqwest for the registry crate's GET/PUT calls; keep
   the public registry API unchanged.
2. `rust-async-http-cutover-connect` — same for the connect client.
3. `rust-async-http-cutover-hosted-http-removal` — once all consumers are
   migrated, delete `hosted_http.rs` entirely.

Each cutover spec must include:

- Side-by-side fixture parity: existing curl-fixture tests pass with the
  reqwest implementation.
- A short performance comparison (latency, allocation count, parallelism
  ceiling). The point is to prove the migration is justified, not to chase
  microbenchmarks.
- A dependency proof: exact dependency versions, `Cargo.lock` diff, license
  exceptions, and `cargo deny check` output.
- A feature decision: whether the call site keeps curl as default, requires
  `async-http`, or makes a higher-level feature imply `async-http`.

## Phases

Phase 1, design hardening:

- Tighten this spec so it is executable as a dependency-policy handoff.
- Do not edit Cargo manifests or source code.

Phase 2, follow-up creation:

- Create the cutover specs named above before removing any ban or adding any
  async/network dependency.
- Each cutover owns its own code changes and acceptance gates.

Phase 3, implementation cutovers:

- Land one call-site migration at a time.
- Keep `cargo deny check`, feature-specific `cargo check`, and existing
  fixture parity green in every slice.

## Acceptance

Run from `oss/`:

```bash
scafld validate rust-async-http-layer
awk '/^## Acceptance/{exit} /^## Harden Rounds/{exit} {print}' .scafld/specs/drafts/rust-async-http-layer.md | rg 'expect\\(|panic!|unwrap\\(|Remove these from \\[bans\\] once this spec lands|reqwest = \\{ version = \"0\\.|tokio = \\{ version = \"1\"' && exit 1 || test $? -eq 1
```

Completion criteria:

- The spec validates.
- The pre-acceptance body contains no denied helper pattern (`expect`,
  `unwrap`, `panic!`) and no statement that this design spec removes
  `tokio`/`reqwest` bans.
- The feature story keeps `async-http` orthogonal until a cutover spec chooses
  a consumer.

## Rollback

This spec is design-only. If the dependency exception is rejected, keep curl
and the current deny rules. If a later implementation cutover fails, revert
that cutover's manifest/source diff and restore the exact `tokio`/`reqwest`
deny entries if no remaining approved cutover uses them. Then run:

```bash
cargo deny --manifest-path crates/Cargo.toml check
cargo check --manifest-path crates/Cargo.toml --workspace --all-targets
```

## Risks

- **Supply chain**: reqwest pulls ~25 transitive deps. The workspace `deny.toml`
  must add exact per-crate license exceptions or a reviewed allowlist change.
  Run `cargo deny check licenses` after the dep is added.
- **Cross-compile**: `rustls-tls` requires `ring` (or `aws-lc-rs`); both build
  cleanly on macOS, Linux, Windows. Confirm in CI before merging.
- **Binary size**: adds ~3 MB to the `runx` binary. Acceptable for the launcher
  flip given the perf headroom this unlocks.
- **No more curl**: some users may be relying on curl behavior (cert store,
  proxy env vars). reqwest respects `HTTP_PROXY`/`HTTPS_PROXY`/`NO_PROXY` env
  vars natively; document the cert-store difference in the registry cutover
  spec.
- **Nested runtime**: rmcp and other adapter-tier callers may already be inside
  tokio. Blocking bridges must detect that case and return a structured error;
  async callers use the native async client path.
- **Boundary erosion**: removing a workspace ban weakens defense-in-depth.
  Every implementation cutover must keep pure crates free of async/network deps
  with `cargo tree` or a crate-graph check in addition to `cargo deny`.
- **Trait migration**: `HostedTransport::send` is synchronous. Cutovers should
  first add a reqwest-backed implementation behind the existing trait using the
  panic-free blocking bridge, then introduce a parallel async trait only when a
  public async API spec needs it.

## Open questions deferred

- Should `runx-sdk` v1 expose an async surface that shares the same tokio
  runtime? Defer to a `runx-sdk-async-path` spec after this lands.
- Connection pooling tuning (max idle connections, keep-alive timeout) —
  defer until real workload data exists.
- Per-call-site retry policy. Defer to `rust-http-retry-policy` follow-up.

## References

- [`crates/deny.toml`](../../crates/deny.toml) — current bans
- [`crates/runx-runtime/src/hosted_http.rs`](../../crates/runx-runtime/src/hosted_http.rs)
- [`crates/runx-runtime/src/registry/http.rs`](../../crates/runx-runtime/src/registry/http.rs)
- [`crates/runx-runtime/src/connect/client.rs`](../../crates/runx-runtime/src/connect/client.rs)
- [`oss/docs/rust-kernel-architecture.md`](../../docs/rust-kernel-architecture.md) §10 (boundary enforcement)
- [`plans/rust-takeover.md`](../../../plans/rust-takeover.md) §3 (commitment shift)

## Harden Rounds

### round-1

Status: needs_revision
Started: 2026-05-21T01:46:31Z
Ended: 2026-05-21T01:46:31Z
Verdict: needs_revision
Provider: claude
Model: claude-opus-4-7
Output format: claude.mcp_submit_harden
Summary: Draft makes a coherent architectural case for a scoped tokio+reqwest exception, but it is not yet executable: phases/scope/acceptance are empty, the code snippet violates workspace clippy lints, the proposed license allowlist will fail on reqwest's transitive crates, and the feature-gating story for cli-tool/registry/connect callers contradicts the migration plan. Several risks (nested-runtime panics once rmcp adopts tokio, workspace-wide ban removal weakening defense-in-depth, sync-trait→async migration shape) are not addressed. Strong design direction; needs a tighter executable contract before approval.

Checks:
- path audit
  - Grounded in: spec:rust-async-http-layer.md#References
  - Result: passed
  - Evidence: All four referenced source files exist: crates/deny.toml, crates/runx-runtime/src/hosted_http.rs, crates/runx-runtime/src/registry/http.rs, crates/runx-runtime/src/connect/client.rs (Glob confirmed). docs/rust-kernel-architecture.md §10 verified (line 320) and plans/rust-takeover.md §3 'Commitment Shift' verified (line 133).
- command audit
  - Grounded in: spec:rust-async-http-layer.md#Risks
  - Result: not_applicable
  - Evidence: Spec only references `cargo deny check licenses` as a future gate. No phase commands are declared because Phases section is empty.
- scope/migration audit
  - Grounded in: spec:rust-async-http-layer.md#Migration plan
  - Result: failed
  - Evidence: Three follow-on specs are named (rust-async-http-cutover-registry/-connect/-hosted-http-removal) but none exist in .scafld/specs/{drafts,approved,archive}. Glob for rust-async-http-cutover-*.md returned empty. The 'single scoped introduction' claim is undermined because the deny.toml exception removes the workspace-wide ban for tokio+reqwest before any cutover spec lands.
- acceptance timing audit
  - Grounded in: spec:rust-async-http-layer.md context manifest
  - Result: failed
  - Evidence: Harden context shows scope=0, phases=0, acceptance=0 bytes. The spec has no `## Phases`, no `## Acceptance`, no `## Rollback`. For risk_level=high this is non-executable: `scafld build` will not be able to open a phase and `scafld review` will have no acceptance to grade against.
- rollback/repair audit
  - Grounded in: spec:rust-async-http-layer.md#`deny.toml` exceptions
  - Result: failed
  - Evidence: Spec proposes removing tokio+reqwest from [bans] in crates/deny.toml (lines 22, 18) but documents no rollback path. If the runtime feature is reverted, the bans must be restored verbatim including the original deny reasons; the spec must also state that no transitive `hyper` direct-dep regression has crept in. No rollback test or revert recipe is given.
- design challenge
  - Grounded in: spec:rust-async-http-layer.md#Runtime lifecycle
  - Result: failed
  - Evidence: Multiple architectural risks unaddressed: (1) `RUNTIME.get_or_init(...).build().expect("tokio runtime")` violates the workspace-pinned `clippy::expect_used = "deny"` and `clippy::panic = "deny"` lints (crates/Cargo.toml lines 38-44). (2) `current_thread.block_on` panics if invoked from inside a tokio context — once rust-mcp-rmcp-adoption lands and brings rmcp's tokio runtime, any code path that ends up nested in an rmcp handler will panic. (3) License allowlist (deny.toml lines 26-31) is restricted to Apache-2.0/BSD-3-Clause/MIT/Unicode-3.0; reqwest pulls in MPL-2.0 (encoding_rs) and ISC (rustls deps) transitively — `cargo deny check licenses` will fail without an update.

Issues:
- [critical/blocks approval] `harden-1` executable_contract - Spec has no Phases, Scope/Touchpoints, or Acceptance/Rollback sections.
  - Status: open
  - Grounded in: spec:rust-async-http-layer.md context manifest sections scope/phases/acceptance=0 bytes
  - Evidence: The harden context manifest reports scope=0, phases=0, acceptance=0 bytes; `## Phases` and `## Acceptance` do not appear in the spec body. Risk level is `high`, but `scafld build` cannot open a phase and `scafld review` cannot grade acceptance. The spec's stated work (edit deny.toml, add features to runx-runtime/Cargo.toml, add lazy runtime helper) is concrete enough to phase out.
  - Recommendation: Add at minimum: (1) Scope section listing crates/deny.toml, crates/runx-runtime/Cargo.toml, and the new private helper module path; (2) Phases section with `materialize` (deny.toml exception + Cargo.toml feature + helper module behind `async-http` feature, no consumer migration) and `verify` (cargo deny check licenses, cargo deny check bans, cargo check --features async-http, cargo check --no-default-features) phases; (3) Acceptance commands and Rollback recipe.
  - Question: Is the intent for this spec to ship code (the feature + helper) or to be a design-only spec whose only artifact is the deny.toml diff?
  - Recommended answer: Ship the feature + lazy runtime helper behind `async-http` with zero consumers wired up; defer all call-site migrations to the cutover specs. That gives a concrete acceptance: `cargo build --features async-http` succeeds and `cargo deny check` passes, without touching registry/connect.
  - If unanswered: Default to design-only: the build artifact is the deny.toml diff plus the feature flag declaration, and acceptance is `cargo deny check bans` + `cargo deny check licenses` green.
- [high/blocks approval] `harden-2` lint_violation - Proposed runtime helper uses `.expect(...)`, which is denied workspace-wide.
  - Status: open
  - Grounded in: code:crates/Cargo.toml:38-44
  - Evidence: Workspace lints set `expect_used = "deny"`, `panic = "deny"`, `unwrap_used = "deny"` (crates/Cargo.toml). The helper in `## Runtime lifecycle` ends in `.build().expect("tokio runtime")`. As written it will not compile under workspace lints; reviewers will be forced to either add an `#[allow]`, downgrade the workspace lint, or rewrite the helper.
  - Recommendation: Replace with a panic-free pattern: return `Result<Arc<Runtime>, BuildError>` from a fallible `async_runtime() -> &'static Result<Arc<Runtime>, RuntimeBuildError>` (cached in `OnceLock`), or use `match builder.build() { Ok(rt) => Arc::new(rt), Err(e) => { ... } }` with a structured `RuntimeError` variant. State the chosen pattern in the spec so the implementation does not silently introduce a `#[allow(clippy::expect_used)]`.
  - Question: Should the failed-to-build-runtime path be a hard panic (with allow-list) or a structured `RuntimeError` variant?
  - Recommended answer: Structured RuntimeError: tokio::runtime::Builder::build can fail (rare, but real). A runtime that cannot start should surface to the caller as `RuntimeError::AsyncRuntimeUnavailable` so the launcher can render a clean diagnostic rather than panic mid-CLI.
  - If unanswered: Default to structured error: introduce `RuntimeError::AsyncRuntimeUnavailable { source: std::io::Error }` and have `async_runtime()` return `Result`.
- [high/blocks approval] `harden-3` supply_chain - License allowlist will reject reqwest's transitive dependency tree.
  - Status: open
  - Grounded in: code:crates/deny.toml:26-31
  - Evidence: deny.toml `[licenses].allow` lists only Apache-2.0, BSD-3-Clause, MIT, Unicode-3.0 (lines 27-31), with `confidence-threshold = 0.8` and `exceptions = []`. reqwest with `rustls-tls,json,gzip` features pulls (depending on version) `encoding_rs` (MPL-2.0), `ring`/`untrusted` (ISC + custom), and `webpki-roots`/`rustls` deps with mixed licenses. `cargo deny check licenses` will fail.
  - Recommendation: Either (a) extend `[licenses].allow` with the missing licenses now in the same diff as the bans removal, listing exactly which ones and why; or (b) move to per-crate `[licenses].exceptions` so the additions are surgical. Either way, run `cargo deny check licenses` after `cargo update -p reqwest` and pin the deny.toml diff to a verified dep tree, not guesswork.
  - Question: Are you willing to broaden the global license allowlist (e.g., add MPL-2.0) or keep the allowlist narrow and use per-crate exceptions?
  - Recommended answer: Per-crate exceptions. The workspace currently runs a tight Apache/BSD/MIT-only policy; broadening to MPL globally is a separate legal review. Pin the exception list to specific crate/version pairs and re-run `cargo deny check licenses` whenever reqwest is bumped.
  - If unanswered: Default to per-crate exceptions and add an explicit phase step that lists the required additions before merging.
- [high/blocks approval] `harden-4` feature_gating - `async-http` feature has no plausible enabler given current module layout.
  - Status: open
  - Grounded in: code:crates/runx-runtime/src/lib.rs:7-25 and code:crates/runx-runtime/Cargo.toml:18-24
  - Evidence: `pub mod connect;` and `pub mod registry;` are unconditional in lib.rs (lines 12, 22). Both depend on `hosted_http`. The spec says adapter-tier features (`cli-tool`, `mcp`, `a2a`, `agent`, `catalog`) do NOT enable `async-http` by default, but those are exactly the features whose call sites (registry resolve, connect grant fetch) the migration plan promises to flip to reqwest. The CLI binary (`runx-cli/Cargo.toml:20`) enables `cli-tool,mcp` — so either it must also enable `async-http`, or the published CLI cannot use the new HTTP path at all.
  - Recommendation: Resolve the contradiction explicitly in the spec. Two viable options: (i) `cli-tool` implies `async-http` because every CLI invocation can reach registry/connect; or (ii) `async-http` stays orthogonal and registry/connect ship two transports side-by-side (HostedTransport for the default, AsyncTransport for the feature) with a runtime-selected default. Pick one and state which Cargo features fan out into which.
  - Question: Should `cli-tool` (and therefore the cargo-installed launcher) imply `async-http`, or do we keep the curl backend as the always-available default and treat async as opt-in for hosted/cloud builds?
  - Recommended answer: Make `async-http` orthogonal in this spec (just the feature + helper, zero callers). The cutover specs decide per-call-site whether to require `async-http`, and at that point we can decide whether `cli-tool` implies it. Avoid pre-committing here.
  - If unanswered: Default to orthogonal: `async-http` is a leaf feature with no other feature depending on it, and the spec explicitly states that the consumer-wiring decision is owned by each cutover spec.
- [high/blocks approval] `harden-5` nested_runtime - Single global `current_thread` runtime + `block_on` will panic when called from inside an rmcp handler.
  - Status: open
  - Grounded in: spec:rust-async-http-layer.md#Runtime lifecycle and spec:rust-mcp-rmcp-adoption.md:64
  - Evidence: `tokio::runtime::Runtime::block_on` panics with 'Cannot start a runtime from within a runtime' if invoked while a tokio context is active. `rust-mcp-rmcp-adoption` (line 64) is explicitly downstream of this spec and will adopt rmcp + a tokio runtime for the MCP server. Any code path that reaches `connect`/`registry` from within an rmcp tool handler (e.g., a connect-driven tool resolution during `runx mcp serve`) will panic at runtime.
  - Recommendation: Either (a) detect an active runtime via `tokio::runtime::Handle::try_current()` and use `Handle::block_on`/`spawn_blocking` semantics, or (b) document that registry/connect call sites are off-limits from inside async contexts and the rmcp adoption spec must use the async path directly. Add the chosen rule to the spec so the rmcp spec is unambiguous about which surface to call.
  - Question: Is it acceptable to ship a runtime helper that panics if called from inside an existing tokio runtime, on the assumption no caller will do so before the rmcp spec?
  - Recommended answer: No. Detect `Handle::try_current()` and short-circuit by reusing the existing handle when present; emit a clearly named error variant if the caller is on a thread that disallows blocking. Make this part of the helper's first cut so the rmcp spec inherits a safe API.
  - If unanswered: Default: helper checks `Handle::try_current()` and panics-free returns either a borrowed handle or the lazy current_thread runtime; document the chosen branching.
- [medium/advisory] `harden-6` boundary_enforcement - Removing tokio/reqwest from [bans] weakens defense-in-depth across the workspace.
  - Status: open
  - Grounded in: code:crates/deny.toml:7-24 and doc:docs/rust-kernel-architecture.md:320
  - Evidence: deny.toml uses `all-features = true` (line 5) so the ban applies workspace-wide. Architecture doc §10 (line 332) requires `runx-core`, `runx-contracts`, `runx-parser` to have no `tokio`/`reqwest`. After removing the bans, only dependency-direction discipline (and reviewer attention) prevents accidental adoption in pure crates; the cargo-deny tripwire is lost.
  - Recommendation: Instead of deleting the ban entries, scope them via cargo-deny `[bans].deny` allow-list per crate: keep `tokio`/`reqwest` denied but add an allow-list entry permitting the dep only from `runx-runtime`. cargo-deny 0.14+ supports `wrappers`/`allow` per-crate. If that's not viable, at least mention an alternative CI check (e.g., the existing scripts/check-rust-crate-graph.mjs) that fails if pure crates pick up tokio/reqwest.
  - Question: Are we OK losing the cargo-deny tripwire for pure crates, relying only on dep-direction review?
  - Recommended answer: Not OK. Keep the bans and add allow-list entries pointing at `runx-runtime` only. The whole point of `all-features = true` was to keep the tripwire armed; deleting bans wholesale defeats it.
  - If unanswered: Default to per-crate exception in deny.toml (deny stays, runx-runtime is added to an explicit allow-from list) rather than full removal.
- [medium/advisory] `harden-7` abstraction_migration - Sync `HostedTransport::send` trait blocks a clean reqwest migration; the spec doesn't name the shape.
  - Status: open
  - Grounded in: code:crates/runx-runtime/src/hosted_http.rs:98-100
  - Evidence: `HostedTransport::send(&self, request) -> Result<HostedHttpResponse, HostedHttpError>` is synchronous (line 99). `RegistryClient<T>` and `ConnectClient<T>` are generic over this trait. Plugging reqwest in requires either (a) wrapping every call in `runtime.block_on` inside the impl, (b) adding an async variant of the trait, or (c) refactoring callers to be async. The spec promises 'public API stays blocking' but does not say which of these three the cutover specs must adopt.
  - Recommendation: State the migration shape in this spec so the three cutover specs do not each re-litigate it. Recommended: keep `HostedTransport` synchronous and add a `ReqwestHostedTransport` impl that wraps `block_on(client.execute(...))` internally. Document that the runtime helper is only entered through this transport impl, so the public surface remains blocking and tests can swap the transport.
  - Question: Do we want reqwest behind the existing `HostedTransport` (blocking shim), or do we want a parallel async client surface?
  - Recommended answer: Behind `HostedTransport` as a blocking shim. The async path is not yet justified by a consumer; introducing two surfaces multiplies test matrix and contradicts 'public API stays blocking'.
  - If unanswered: Default to the blocking shim: ship `ReqwestHostedTransport: HostedTransport` and keep the trait synchronous.
- [low/advisory] `harden-8` migration_dependency - Named follow-up cutover specs do not yet exist.
  - Status: open
  - Grounded in: spec:rust-async-http-layer.md#Migration plan
  - Evidence: Glob for `.scafld/specs/{drafts,approved,archive}/rust-async-http-cutover-*.md` returned no files. The migration plan section commits to three specs but none are staged.
  - Recommendation: Either pre-create empty draft specs for `rust-async-http-cutover-registry`, `rust-async-http-cutover-connect`, and `rust-async-http-cutover-hosted-http-removal` (so `scafld list` shows the cutover chain), or rephrase this spec's section to say the IDs are illustrative and final names will land with their own plan calls.
  - Question: Should we stub the three cutover drafts now so the chain is discoverable?
  - Recommended answer: Yes — `scafld plan rust-async-http-cutover-registry --title "..."` (and the other two) immediately after this spec is approved, so the deny.toml exception cannot sit in main with no follow-up tracker.
  - If unanswered: Default to stubbing the three drafts when this spec is approved.

### round-2

Status: passed
Started: 2026-05-21T04:20:00Z
Ended: 2026-05-21T04:32:00Z
Verdict: passed
Provider: local
Model: codex
Output format: manual_resolution
Summary: Resolved the async HTTP design blockers. The spec is now a dependency-policy handoff, not a manifest change; it keeps async-http orthogonal, requires exact reviewed pins and per-crate license review, uses a panic-free helper shape, and records the nested-runtime rule for rmcp and future async callers.

Checks:
- path audit
  - Grounded in: code:crates/runx-runtime/src/hosted_http.rs:1
  - Result: passed
  - Evidence: The spec still targets the existing curl-backed hosted HTTP boundary and does not change code in this task.
- command audit
  - Grounded in: code:.scafld/specs/drafts/rust-async-http-layer.md:257
  - Result: passed
  - Evidence: `scafld validate rust-async-http-layer` exited 0 and the stale-pattern text gate exited 0.
- scope/migration audit
  - Grounded in: code:.scafld/specs/drafts/rust-async-http-layer.md:47
  - Result: passed
  - Evidence: Scope now excludes manifest/source edits in this task and reserves implementation for per-call-site cutover specs.
- acceptance timing audit
  - Grounded in: code:.scafld/specs/drafts/rust-async-http-layer.md:257
  - Result: passed
  - Evidence: Acceptance commands and completion criteria are present and executable from `oss/`.
- rollback/repair audit
  - Grounded in: code:.scafld/specs/drafts/rust-async-http-layer.md:275
  - Result: passed
  - Evidence: Rollback explains how to keep or restore the current curl path and deny entries if an exception or later cutover fails.
- design challenge
  - Grounded in: code:.scafld/specs/drafts/rust-async-http-layer.md:165
  - Result: passed
  - Evidence: The runtime lifecycle now uses a fallible helper and returns a structured error when a blocking bridge is called from inside an active tokio runtime.

Issues:
- [critical/blocks approval] `harden-1` executable_contract - Spec had no executable Scope/Phases/Acceptance/Rollback.
  - Status: closed
  - Grounded in: code:.scafld/specs/drafts/rust-async-http-layer.md:47
- [high/blocks approval] `harden-2` lint_violation - Runtime helper used `.expect`.
  - Status: closed
  - Grounded in: code:.scafld/specs/drafts/rust-async-http-layer.md:177
- [high/blocks approval] `harden-3` supply_chain - License allowlist impact was under-specified.
  - Status: closed
  - Grounded in: code:.scafld/specs/drafts/rust-async-http-layer.md:140
- [high/blocks approval] `harden-4` feature_gating - `async-http` had no coherent enabler.
  - Status: closed
  - Grounded in: code:.scafld/specs/drafts/rust-async-http-layer.md:118
- [high/blocks approval] `harden-5` nested_runtime - Blocking bridge could panic inside rmcp/tokio.
  - Status: closed
  - Grounded in: code:.scafld/specs/drafts/rust-async-http-layer.md:198
- [medium/advisory] `harden-6` boundary_enforcement - Removing bans weakened pure-crate defense.
  - Status: closed
  - Grounded in: code:.scafld/specs/drafts/rust-async-http-layer.md:147
- [medium/advisory] `harden-7` abstraction_migration - Sync `HostedTransport` migration shape was unnamed.
  - Status: closed
  - Grounded in: code:.scafld/specs/drafts/rust-async-http-layer.md:306
- [low/advisory] `harden-8` migration_dependency - Named follow-up cutovers did not exist.
  - Status: closed
  - Grounded in: code:.scafld/specs/drafts/rust-async-http-layer.md:216

### round-3

Status: passed
Started: 2026-05-21T04:35:00Z
Ended: 2026-05-21T02:03:05Z
Verdict: passed
Provider: local
Model: codex
Summary: Final manual harden evidence: the async HTTP layer spec is now a dependency-policy handoff with no manifest/source changes under this task, a panic-free runtime helper shape, and explicit follow-up cutover requirements.

Checks:
- path audit
  - Grounded in: code:crates/runx-runtime/src/hosted_http.rs:1
  - Result: passed
  - Evidence: The spec targets the existing hosted HTTP boundary while leaving code untouched in this task.
- command audit
  - Grounded in: code:.scafld/specs/drafts/rust-async-http-layer.md:257
  - Result: passed
  - Evidence: `scafld validate rust-async-http-layer` and the stale-pattern text gate both exited 0.
- scope/migration audit
  - Grounded in: code:.scafld/specs/drafts/rust-async-http-layer.md:47
  - Result: passed
  - Evidence: Scope excludes manifest/source edits and reserves implementation for per-call-site cutover specs.
- acceptance timing audit
  - Grounded in: code:.scafld/specs/drafts/rust-async-http-layer.md:257
  - Result: passed
  - Evidence: Acceptance commands and completion criteria are explicit.
- rollback/repair audit
  - Grounded in: code:.scafld/specs/drafts/rust-async-http-layer.md:275
  - Result: passed
  - Evidence: Rollback explains how to keep or restore curl and deny entries.
- design challenge
  - Grounded in: code:.scafld/specs/drafts/rust-async-http-layer.md:165
  - Result: passed
  - Evidence: The runtime lifecycle uses a fallible helper and returns structured errors for nested-runtime blocking calls.

Issues:
- none


## Review

Status: completed
Verdict: pass
Mode: verify
Summary: Human-reviewed override accepted: Reviewed scoped spec-only diff after harden. scafld validate rust-async-http-layer and stale-pattern gate passed; no Cargo manifest, deny, or source changes landed under this task.

Attack log:
- `review gate`: manual human audit -> clean (Reviewed scoped spec-only diff after harden. scafld validate rust-async-http-layer and stale-pattern gate passed; no Cargo manifest, deny, or source changes landed under this task.)

Findings:
- none

