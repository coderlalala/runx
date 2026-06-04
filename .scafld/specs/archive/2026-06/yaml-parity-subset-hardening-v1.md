---
spec_version: '2.0'
task_id: yaml-parity-subset-hardening-v1
created: '2026-05-29T00:00:00Z'
updated: '2026-06-04T21:10:04Z'
status: completed
harden_status: not_run
size: small
risk_level: low
---

# YAML parity-subset validator: escape correctness + fuzz target

## Current State

Status: completed
Current phase: final
Next: done
Reason: task completed
Blockers: none
Allowed follow-up command: `none`
Latest runner update: 2026-06-04T21:10:04Z
Review gate: pass

## Summary

The validator's job is to reject YAML constructs that parse differently
under serde_norway than a reader would naively expect (`name:embedded`
keys, `key: value: more` plain scalars). It does this by scanning for
top-level mapping delimiters with quote-aware skipping. Today the scanner
treats `previous != '\\'` as "not escaped," which is the C-string rule, not
the YAML double-quoted rule. In YAML's double-quoted form, escape sequences
consume two bytes (`\\` → `\`, `\"` → `"`, etc.) and the quote terminator is
unambiguously identifiable only by walking the escape table.

This spec replaces the toggle-based quote state with a proper
double-quoted-scalar state machine, ports the single-quoted rule (YAML
single quotes escape only `''`), adds a regression fixture covering the known
escape patterns, and adds a `cargo-fuzz` target that compares the
validator's accept/reject decision against `serde_norway::from_str`'s
structural parse outcome for randomly generated inputs.

## The escape rules being modeled

YAML 1.2 double-quoted scalars:

- `\\` → `\`
- `\"` → `"`
- `\n`, `\t`, `\r`, `\0`, `\a`, `\b`, `\f`, `\v`, `\e`, `\ `, `\/` → corresponding control char
- `\xNN`, `\uNNNN`, `\UNNNNNNNN` → unicode codepoint
- A literal `"` not preceded by `\` terminates the scalar.

YAML 1.2 single-quoted scalars:

- `''` → `'`
- A literal `'` not followed by `'` terminates the scalar.

The validator does not need to decode escapes; it only needs to determine
**when the quoted region ends**. For double quotes that means: consume `\`
plus exactly one following byte as a unit. For single quotes that means:
consume `''` as a unit; a lone `'` ends the region.

## Confirmed Defects

Inputs that the current scanner mistracks:

| Input        | YAML termination | Scanner termination | Failure mode |
| ------------ | ---------------- | ------------------- | ------------ |
| `"\\"`       | byte 3 (closing `"`) | byte 3 (`previous == '\\'`, no toggle); never closes in this token | over-stay |
| `"\\""`      | byte 3 (close), byte 4 opens a new empty scalar | scanner sees byte 4 as still-inside-quote, finally toggles at... never (no further `"`) | over-stay |
| `"a\\b"`     | terminates at final `"` | never toggles off because byte 3 is `\` then byte 4 `b`; scanner stays in quote forever | over-stay |
| `'it''s'`    | terminates at final `'` (the `''` is an escape) | scanner toggles off at the first inner `'`, opens at the second, toggles off at the third | mis-segments |

Blast radius today: low. The downstream parser (`serde_norway`) is the
authoritative reader, so a mis-segmented validator does not produce a wrong
value; it produces either a spurious rejection or a missed rejection. The
missed-rejection case is the one this spec closes.

## Scope

In:

- Rewrite the double-quote tracking in `top_level_plain_key` and
  `contains_unquoted_colon_space` (and any other quote-aware scanner in
  `yaml.rs`) to use a state machine that consumes `\` + next-byte as one unit
  in `DoubleQuoted`, and `''` as one unit in `SingleQuoted`.
- Add regression cases to `crates/runx-parser/tests/integration.rs` (under
  the existing parser_fixtures module) for: `"\\"`, `"\\""`, `"a\\b"`,
  `'it''s'`, `"\""`, mixed `'a: b'` in a value position.
- Add a new `fuzz/` crate at workspace root containing a `cargo-fuzz` target
  `fuzz_yaml_parity_subset` that:
  1. Generates a byte string up to 256 bytes.
  2. Runs `runx_parser::yaml::assert_yaml_parity_subset("fuzz", &input)`.
  3. Runs `serde_norway::from_str::<serde_norway::Value>(&input)`.
  4. Asserts: if `assert_yaml_parity_subset` returns Ok and serde_norway
     returns Ok, then a follow-up "extract top-level mapping keys" pass on
     the serde_norway result returns no key containing a colon. (Validator's
     contract is "this YAML has no ambiguous top-level keys"; the property
     test is "and that's what serde_norway agrees the document looks like.")
  5. Asserts no panic in either call.
  The fuzz crate is excluded from the default workspace build and from
  `verify:fast`. It is exercised manually with `cargo +nightly fuzz run
  fuzz_yaml_parity_subset` or in a separate scheduled CI job.

Out:

- Adding fuzz coverage for the skill markdown frontmatter parser. Separate
  spec.
- Replacing serde_norway. The parity subset is layered on top.
- Reworking the public `ParseError` shape.

## Acceptance Criteria

1. `cargo test -p runx-parser` passes, including the new regression cases.
2. The fuzz crate builds with `cargo +nightly fuzz build` against the
   `fuzz_yaml_parity_subset` target.
3. `cargo +nightly fuzz run fuzz_yaml_parity_subset -- -max_total_time=60`
   completes without finding a crash or assertion failure on a 60-second
   local run.
4. No existing parser fixture under `fixtures/parser/skills/` or
   `fixtures/parser/graphs/` regresses.

## Risk

Low. The scanner's failure modes are over-stay-in-quote, which produces
under-rejection (validator accepts more than it should). Replacing with the
correct state machine produces stricter rejection. Risk is that one of the
~30 parser fixtures hits a case the new scanner rejects but the old one
accepted. Mitigation: fixture suite runs as part of acceptance step 4 and
either confirms no regression or surfaces the fixture for review.

## Sequencing

Independent of `canonical-json-float-parity-v1` and
`oracle-fixture-numeric-coverage-v1`. Can land in parallel.

## Review

Status: completed
Verdict: pass
Mode: verify
Provider: command
Output: command.stdout
Summary: YAML parity subset hardening is implemented and verified. The QuoteScanner state machine is present, the fuzz oracle was tightened to allow only truly quoted colon-space mapping keys, fuzz found and the code fixed two production subset gaps (Unicode delimiter byte-boundary panic and explicit ? mapping-key acceptance), cargo test -p runx-parser passed, cargo +nightly fuzz build fuzz_yaml_parity_subset passed, saved crash artifacts reproduce cleanly, and cargo +nightly fuzz run fuzz_yaml_parity_subset -- -max_total_time=60 completed 1489554 runs in 61 seconds with no crash/assertion.

Attack log:
- `runx-parser/src/yaml.rs`: verify delimiter slicing keeps byte index from scanner instead of key.len() after trim -> clean
- `runx-parser/src/yaml.rs`: verify explicit YAML mapping key marker is rejected by the parity subset -> clean
- `runx-parser/src/yaml.rs tests`: run regression tests for quoted escape scanner, Unicode delimiter boundary, and explicit key rejection -> clean
- `runx-parser/fuzz/fuzz_targets/fuzz_yaml_parity_subset.rs`: verify fuzz oracle distinguishes quoted colon-space keys from unquoted ambiguous keys -> clean
- `cargo test -p runx-parser`: run parser unit, integration, and doctests -> clean
- `cargo +nightly fuzz run fuzz_yaml_parity_subset -- -max_total_time=60`: run fuzz acceptance on enlarged corpus -> clean

Findings:
- none

