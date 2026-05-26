# TypeScript interop boundary

This document records the surviving TypeScript and Python package boundary for
the Rust takeover. It is the package-disposition source of truth for OSS
packages during the final runtime cutover.

## Current boundary after takeover

Rust is canonical for trusted local runtime and execution: local skill and
graph execution, harness and dogfood execution, receipt sealing and
verification, history, policy and registry configuration, authority admission,
payment authority, sandbox admission/metadata, built-in adapter execution, and
external execution-adapter supervision. Future OS sandbox enforcement belongs
in the Rust runtime; TypeScript is not a fallback confinement layer.
TypeScript remains for generated contracts, CLI/client wrappers,
cloud/product integrations, host adapters, authoring tooling, docs,
and helper SDKs over language-neutral external protocols. TypeScript does not
own a runtime-local or adapters fallback for trusted local behavior.

MCP follows the same rule. `rmcp` is an adapter-tier Rust dependency for MCP
protocol behavior, while runx keeps graph state, policy, authority, approvals,
and receipts in the Rust kernel. TypeScript MCP code may survive only as
generated contracts, hosted cloud code, helper SDKs over documented protocol
surfaces, or compatibility fixtures. It must not execute trusted local fallback
behavior.

The package CLI is a distribution and UX shim, not a second runtime. A usable
installation must be able to execute the Rust `runx` binary without TypeScript
source, tsx, or a local workspace. If the wrapper cannot find a supported
native binary, it should fail closed with installation guidance rather than
falling back to a TypeScript implementation of local behavior. No wrapper may
import `@runxhq/runtime-local` or `@runxhq/adapters` as a hidden execution
fallback.

Four crossing families exist between the surviving TypeScript surface and the
Rust runtime. Each crossing has a contract surface that owns the wire shape.

1. **CLI JSON.** Anything that shells `runx`, including package launchers, the
   LangChain bridge, `runx-py`, user scripts, and CI workflows. The Rust CLI
   owns behavior; the contract is each command's JSON output shape, exit
   codes, and human-output stability.
2. **Published TypeScript contracts.** Anything that imports from
   `@runxhq/contracts`, including host adapters, cloud packages, and external
   TypeScript consumers. The contract is the TypeScript shape that mirrors
   `runx-contracts` Rust types, with fixture cross-validation.
3. **Cloud HTTP contracts.** Anything where the Rust runtime calls cloud, such
   as approval routing, connect/auth, registry, and receipts-store. The
   contract is versioned, documented HTTP endpoints.
4. **Language-neutral extension protocols.** Anything the Rust runtime launches
   or calls outside the trusted local runtime. This is a family of protocols, not
   one catch-all plugin API: skill subprocess ABI, external execution adapter,
   source-event ingress, hosted/embedded runtime binding, tool catalog/read
   model access, and thread/outbox provider adapters are distinct lanes. The
   contract is the lane-specific language-neutral protocol and manifest shape,
   not a TypeScript package API or an in-process Rust trait. External authors can
   implement these protocols in TypeScript, Python, Rust, or another language
   without a core fork. `external-adapter-plugin-protocol-v1` is the external
   execution-adapter lane only; it must not be used as the umbrella answer for
   source ingress, hosted runtime binding, catalog/read-model, or outbox queues.
   Thread/outbox provider mutation is owned by
   `thread-outbox-provider-protocol-v1`; provider adapters that need tokens must
   consume Rust-supervised `CredentialDelivery`, while the existing file-thread
   helper remains a credential-free local persistence path.

No fifth boundary is added without updating this document. No published
TypeScript package is silently broken: each package disposition is named here.
Stable-boundary edits to consume new `runx-contracts` versions are normal
maintenance. Sunset means deletion or rename through
`oss-runtime-s-tier-engine-cutover-v1`; old package names must not survive as
aliases.

## OSS package dispositions

| Package | Disposition |
| --- | --- |
| `@runxhq/adapters` | Sunset as a trusted runtime-local adapter implementation with `oss-runtime-s-tier-engine-cutover-v1`. It must not survive as a package name, alias, TypeScript path, Vitest alias, or workspace dependency. Surviving protocol/client behavior moves to generated contracts, CLI JSON, or a future language-neutral protocol package with a new explicit name. |
| `@runxhq/authoring` | Stays as authoring tooling for skills, manifests, protocol fixtures, and generated artifacts until the authoring DX plan decides whether any piece moves to Rust or scafld. It does not own trusted local execution. |
| `@runxhq/cli` | Stays as a platform-aware npm launcher that resolves and execs the Rust binary. It must remain useful from an installed package without TypeScript sources and must fail closed instead of falling back to TypeScript local execution. |
| `@runxhq/contracts` | Stays as the published generated TypeScript view of `runx-contracts`, maintained with fixture cross-validation. |
| `@runxhq/core` | Sunset across the named Rust TS sunset specs for state-machine, policy, parser, executor, receipts, registry, marketplaces, and final shell cleanup. |
| `@runxhq/create-skill` | Stays as a thin npm bootstrapper that wraps `runx new` through the CLI. |
| `@runxhq/host-adapters` | Stays as thin host response adapters over the runx host protocol, retargeted to `@runxhq/contracts` types. It can shape host/client responses, not execute trusted local runtime behavior. |
| `@runxhq/langchain` | Stays as an optional LangChain bridge that shells the `runx` CLI or uses documented external protocols for governed skill and tool invocation. |
| `@runxhq/runtime-local` | Sunset with `oss-runtime-s-tier-engine-cutover-v1`; runner, sandbox, harness, MCP, SDK caller, and host-protocol execution move to Rust. The package name must not survive as an alias, TypeScript path, Vitest alias, or workspace dependency. No trusted local orchestration starts here, and it must not be used as a fallback after native Rust cutover. |
| `runx-py` | Stays as a thin Python client over `runx` CLI JSON output. |

Cloud packages remain TypeScript. The Rust runtime consumes cloud through the
cloud HTTP contracts; cloud cutovers are separate future specs. Local registry
and policy configuration remains Rust-owned when exercised by the native CLI.
Cloud/product integrations, host adapters, authoring tooling, and helper SDKs
can remain TypeScript as long as they stay on one of the contract surfaces
above. External integration authors target the correct language-neutral protocol
lane; they must not need Rust, `runx-core`, `runx-runtime`, or a fork of the
core repository to ship an extension.

## Test ownership

Language-owned unit tests stay with their implementation. Contract and parity
tests use durable fixtures under `fixtures/`. End-to-end tests should spawn the
`runx` binary and assert stdout, exit codes, and JSON instead of importing
TypeScript internals. Trusted local graph, harness, receipt, authority,
registry/policy config, and payment behavior needs Rust coverage or a TS-free
Rust CLI fixture before wrapper tests can count as proof. External
execution-adapter tests prove protocol conformance by spawning or simulating the
external process through the documented wire contract, not by importing
TypeScript runtime-local internals. Other extension-lane tests must use their
own wire contract and must not borrow the execution-adapter protocol as a
stand-in.

The TypeScript oracle is temporary. Once a TypeScript domain sunsets, no new
fixtures should be derived from that domain's TypeScript implementation.
