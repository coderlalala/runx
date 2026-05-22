---
spec_version: '2.0'
task_id: rust-ts-sunset-parser
created: '2026-05-18T00:00:00Z'
updated: '2026-05-22T12:10:02+10:00'
status: draft
harden_status: not_run
size: medium
risk_level: high
---

# TS sunset: parser

## Current State

Status: draft
Current phase: blocked
Next: wait for parser importer migration specs to remove live TS consumers.
Reason: draft created under `plans/rust-takeover.md`. Third TS sunset. On
2026-05-22 this draft was rechecked after the runtime-local direct parser import
slice completed and a follow-up runtime-local structural cleanup removed four
more `parser-types.js` consumers. Deletion is still not valid: 48 live files still import
`@runxhq/core/parser`, reference relative/direct parser source paths, or carry
runtime-local parser structural type surfaces.
Blockers: parser importers still live after `rust-ts-sunset-policy` completion,
but runtime-local direct imports are no longer one of the blockers.
Allowed follow-up command: none while blocked; do not run `scafld harden`
for this draft.
Latest runner update: 2026-05-22T12:10:02+10:00 importer census refreshed after
runtime-local structural cleanup removed parser-shaped type imports from
`harness/agent-hook.test.ts`, `runner-local/graph-context.ts`,
`runner-local/graph-reporting.ts`, and `runner-local/reflect.ts`. Deletion
remains blocked by CLI command readers, core internal consumers,
fixture/oracle generators, tests, and temporary runtime-local `parser-types.js`
structural consumers owned by the runtime-local sunset parent.
Review gate: blocked

## Summary

Delete `packages/core/src/parser/`. By the time this spec runs, the Rust
runtime parses skills, graphs, and execution profiles, and no live TS
consumer reads from `@runxhq/core/parser`.

2026-05-20 validation update: this precondition is false in the current
checkout. Do not approve or execute the deletion phase until the importer census
below is clean.

2026-05-22 12:10 validation update: runtime-local direct parser imports are
gone, and the remaining runtime-local parser structural type surfaces have been
reduced from 21 files to 17 files. The importer census still finds 48 files. The
largest surviving groups are CLI command readers, core internal consumers,
fixture/oracle generators, tests, and runtime-local parser structural type
surfaces. This update is
inspection evidence only; it does not make deletion executable.

## Context

CWD: `.`

Packages:
- `@runxhq/core`
- `crates/runx-parser`
- Every TS package that imports from `@runxhq/core/parser`

Current TypeScript sources:
- `packages/core/src/parser/**` (to be deleted)
- `packages/core/src/index.ts`
- All TS importers (enumerated in Phase 1)

Files impacted:
- `packages/core/src/parser/` (deleted)
- `packages/core/src/index.ts`

Invariants:
- Parsed AST shape consumers (authoring, marketplaces, registry, executor)
  have either migrated to Rust or are themselves sunsetting.
- The deletion phase must not remove `packages/core/src/parser/**` while any
  production package, script, fixture generator, or test still imports it.
- Until the importer census is clean, parser changes are limited to parity
  maintenance, fixture validation, and this blocker record.

## Objectives

- Enumerate importers; verify migration.
- Delete TS parser implementation.

Current executable slice:
- Enumerate importers and record that the sunset deletion is blocked.
- Keep Rust parser parity tests passing while consumers migrate elsewhere.

## Scope

In scope:
- TS parser deletion.

Out of scope:
- Authoring tools that consume parsed AST today (their own sunset path).

## Dependencies

- `rust-ts-sunset-policy`.
- `rust-parser-parity` complete and consumed.

## Blocker Census

Checked on 2026-05-20:

- `rust-ts-sunset-policy` is archived as completed.
- `rust-parser-parity` is archived as completed.
- The "consumed" part of `rust-parser-parity complete and consumed` is not
  satisfied for TS sunset because parser imports remain live.

Importer commands:

```bash
rg -l "@runxhq/core/parser" packages tests scripts --glob '!packages/core/src/parser/**' | wc -l
rg -l "(\.\./parser|\.\./\.\./parser|packages/core/src/parser)" packages/core scripts tests --glob '!packages/core/src/parser/**' | wc -l
rg -n "@runxhq/core/parser|\.\./parser|packages/core/src/parser" packages tests scripts --glob '!packages/core/src/parser/**' | wc -l
```

Observed results on 2026-05-22T12:10:02+10:00:

- 25 files import `@runxhq/core/parser`.
- 6 files refer to relative or direct `packages/core/src/parser` paths outside
  the parser directory.
- 17 files import runtime-local `parser-types.js` structural parser surfaces.
- 50 total import/reference hits remain outside `packages/core/src/parser/**`.
- 48 union files still reference the parser package, source path, or structural
  runtime-local parser type surface.

Representative live production importers:

- `packages/cli/src/commands/doctor.ts`
- `packages/cli/src/commands/mcp.ts`
- `packages/cli/src/commands/tool.ts`
- `packages/core/src/config/index.ts`
- `packages/core/src/registry/ingest.ts`
- `packages/adapters/src/agent/json-schema.ts`

Importer-class work split:

- `rust-ts-sunset-parser-runtime-local-importers`: completed the runtime-local
  direct parser import migration. Remaining runtime-local `parser-types.js`
  structural consumers are temporary runtime-local type surfaces owned by the
  runtime-local sunset parent, not parser value imports. The 2026-05-22T12:10
  cleanup removed four of those structural consumers without touching parser
  implementation.
- CLI command readers: `packages/cli/src/commands/{dev/fixture-runner,
  doctor-structure,doctor,list,tool}.ts`.
- Core internal consumers: `packages/core/src/config/index.ts`,
  `packages/core/src/registry/ingest.ts`, and
  `packages/core/src/registry/tool-catalog-types.ts`.
- Fixture/oracle generators: `scripts/generate-official-lock.mjs`,
  `scripts/generate-rust-parser-fixtures.ts`, and
  `scripts/count-clean-kernel-prs.ts` history classification.
- Tests: the remaining `tests/**` parser imports should either move to Rust
  fixture validation or stay as explicit parser parity tests until the final
  deletion window.

Deletion acceptance, when unblocked:

```bash
! rg -n "@runxhq/core/parser|\.\./parser|packages/core/src/parser" packages tests scripts --glob '!packages/core/src/parser/**'
cargo test --manifest-path crates/Cargo.toml -p runx-parser
pnpm exec vitest run packages/core/src/parser/index.test.ts packages/core/src/parser/graph.test.ts
scafld validate rust-ts-sunset-parser
```

This spec remains a draft and must be re-approved only after the importer
census reaches zero or after a narrower parser-bridge spec explicitly owns the
remaining importers.

## Open Questions

- None at draft time.
