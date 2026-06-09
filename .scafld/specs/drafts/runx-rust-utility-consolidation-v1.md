---
spec_version: '2.0'
task_id: runx-rust-utility-consolidation-v1
created: '2026-06-09T14:07:00Z'
updated: '2026-06-09T14:07:00Z'
status: draft
harden_status: not_run
size: medium
risk_level: medium
---

# Consolidate repeated Rust utilities

## Current State

Status: draft
Current phase: none
Next: approve
Reason: draft created
Blockers: none
Allowed follow-up command: `scafld approve runx-rust-utility-consolidation-v1`
Latest runner update: none
Review gate: not_started

## Summary

Consolidate repeated small Rust helper functions into crate-local utility
modules without changing runtime behavior, crate boundaries, or public command
contracts. This is the repo-wide quality pass that follows the export work:
avoid helper drift, but do not create a global junk drawer.

## Objectives

- Remove repeated helper implementations where the semantics are truly shared.
- Keep abstractions close to their owning crate and error domain.
- Preserve all existing user-visible error messages unless a phase explicitly
  names a safer replacement.
- Avoid broad behavior rewrites: this is consolidation, not redesign.
- Leave intentionally divergent helpers local and document why.

## Scope

- In scope:
  - Parser JSON field extraction/validation helpers in `runx-parser`.
  - CLI arg/IO/env helper cleanup in `runx-cli`.
  - Runtime path/presentation helpers in `runx-runtime`.
  - Runtime JSON redaction and ASCII byte trimming helpers where semantics are
    exact matches.
  - `runx-pay` JSON kind-name / authority helper duplication where behavior is
    proven identical.
- Out of scope:
  - Changing command surfaces, JSON schemas, receipt shapes, or provider
    contracts.
  - Merging helpers whose error types encode different contracts.
  - Changing sandbox path containment behavior.
  - Renaming public modules or exported contract types.

## Dependencies

- Active export work should land first, or this spec must rebase around
  `crates/runx-cli/src/cli_args.rs` and `crates/runx-runtime/src/export.rs`.
- Coordinate with any long-running Cargo jobs before running workspace-wide
  validation.

## Assumptions

- Helper duplication is currently spread across several crates, but the correct
  abstraction boundary is usually crate-local.
- Cargo tests are expensive in this workspace; implementation should use
  targeted tests per phase and reserve workspace validation for final review.

## Touchpoints

- `crates/runx-parser/src/**`
- `crates/runx-cli/src/**`
- `crates/runx-runtime/src/**`
- `crates/runx-pay/src/**`
- Focused tests under each affected crate's consolidated integration test
  surface.
- Rust style / crate graph scripts.

## Risks

- Over-abstracting can erase security-significant differences, especially
  sandbox path handling and provider locator comparison.
- Error message drift can break CLI parity fixtures and user-facing contracts.
- Workspace has concurrent agent work; do not revert unrelated dirty files.

## Acceptance

Profile: standard

Validation:
- `cd crates && cargo test -p runx-parser`
- `cd crates && cargo test -p runx-cli`
- `cd crates && cargo test -p runx-runtime`
- `cd crates && cargo test -p runx-pay`
- `pnpm rust:style`
- `pnpm rust:crate-graph`
- `pnpm fixtures:cli-parity:check`
- `pnpm verify:fast` once, at final integration only and only when no other
  Rust validation is running.

## Phase 1: Inventory Guardrails

Status: pending
Dependencies: none

Objective: Lock the helper inventory and non-merge list before moving code.

Changes:
- Add or update a short internal inventory comment/doc section if needed to
  document intentionally divergent helper families.
- Confirm exact duplication candidates:
  - Parser JSON field helpers:
    `graph/helpers.rs`, `skill.rs`, `tool.rs`, `runner.rs`.
  - CLI helpers:
    `env_map`, `write_stdout`, `write_stderr`, and shared arg parsing
    call-sites.
  - Runtime path helpers:
    YAML path checks, YAML counts, project/display path helpers, lexical
    normalization.
  - Runtime redaction helpers:
    recursive JSON object redaction and byte whitespace trimming.
  - Pay helpers:
    `json_value_kind` and authority term helpers where semantics match.
- Explicitly exclude:
  - sandbox containment path normalization,
  - template placeholder renderers with different validation rules,
  - identifier sanitizers with different punctuation/fallback semantics,
  - provider locator equality where one caller intentionally compares more
    fields.

Acceptance:
- [ ] `ac1` command - Inventory grep shows remaining candidate families before edits
  - Command: `rg -n "fn (required_string|optional_string|optional_bool|optional_u64|field_value|nested_value|env_map|write_stdout|write_stderr|json_value_kind|trim_ascii_whitespace|lexical_normalize|project_path|is_yaml_path|count_yaml_files)\\b" crates --glob '*.rs'`
  - Expected kind: `exit_code_zero`
  - Status: pending

## Phase 2: Parser and CLI Utilities

Status: pending
Dependencies: phase1

Objective: Consolidate parser JSON field extraction and CLI utility drift.

Changes:
- Introduce a parser-local helper module for common JSON field validation.
- Migrate graph, skill, tool, and runner parsers to the shared parser helper
  without changing validation paths/messages unless tests prove current output
  is already inconsistent.
- Introduce or extend CLI-local helpers for env/stdout/stderr wrappers where
  signatures match.
- Do not force command modules with meaningful custom error mapping into one
  generic API; use small adapters at the edge.

Acceptance:
- [ ] `ac2` command - Parser tests pass
  - Command: `cd crates && cargo test -p runx-parser`
  - Expected kind: `exit_code_zero`
  - Status: pending
- [ ] `ac3` command - CLI tests pass
  - Command: `cd crates && cargo test -p runx-cli`
  - Expected kind: `exit_code_zero`
  - Status: pending
- [ ] `ac4` command - CLI parity fixtures still match
  - Command: `pnpm fixtures:cli-parity:check`
  - Expected kind: `exit_code_zero`
  - Status: pending

## Phase 3: Runtime and Pay Utilities

Status: pending
Dependencies: phase2

Objective: Consolidate runtime filesystem/redaction helpers and pay helper
duplicates where semantics are identical.

Changes:
- Extend runtime filesystem/path utilities for YAML checks/counts, project path
  rendering, display rendering, and lexical normalization where not
  security-sensitive.
- Move recursive JSON redaction into one runtime helper and keep
  source-specific secret-key policy local.
- Consolidate exact byte-trim duplicates.
- Move shared JSON kind naming into a contract/pay-local helper as appropriate.
- Reuse core authority helpers in pay where provider/locator-insensitive
  comparison is intended.

Acceptance:
- [ ] `ac5` command - Runtime tests pass
  - Command: `cd crates && cargo test -p runx-runtime`
  - Expected kind: `exit_code_zero`
  - Status: pending
- [ ] `ac6` command - Pay tests pass
  - Command: `cd crates && cargo test -p runx-pay`
  - Expected kind: `exit_code_zero`
  - Status: pending

## Phase 4: Integration Validation

Status: pending
Dependencies: phase3

Objective: Prove the cleanup preserved architecture and workspace behavior.

Changes:
- Fix any crate graph or style regressions caused by the cleanup.
- Run one final fast verifier when no other Rust validation is active.

Acceptance:
- [ ] `ac7` command - Rust crate graph remains clean
  - Command: `pnpm rust:crate-graph`
  - Expected kind: `exit_code_zero`
  - Status: pending
- [ ] `ac8` command - Rust style remains clean
  - Command: `pnpm rust:style`
  - Expected kind: `exit_code_zero`
  - Status: pending
- [ ] `ac9` command - Workspace fast verification passes
  - Command: `pnpm verify:fast`
  - Expected kind: `exit_code_zero`
  - Status: pending

## Rollback

- Revert each crate-local helper migration independently. No migrations,
  persisted data, or generated runtime artifacts are involved.

## Review

Status: not_started
Verdict: none

Findings:
- none

## Self Eval

- none

## Deviations

- none

## Metadata

- created_by: scafld

## Origin

Created by: scafld
Source: plan

## Harden Rounds

- none

## Planning Log

- Duplication sweep completed by subagent:
  - Good candidates: parser JSON field helpers, CLI IO/env helpers, runtime
    path helpers, runtime JSON redaction, byte trimming, pay JSON kind naming,
    pay/core authority helper reuse.
  - Avoid or split carefully: sandbox path containment normalization, template
    renderers, identifier sanitizers, JSON conversion helpers with different
    error types, provider locator equality.
