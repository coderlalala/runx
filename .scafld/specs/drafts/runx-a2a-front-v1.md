---
spec_version: '2.0'
task_id: runx-a2a-front-v1
created: '2026-06-04T06:20:35Z'
updated: '2026-06-04T06:20:35Z'
status: draft
harden_status: not_run
size: medium
risk_level: medium
---

# runx-a2a-front-v1

## Current State

Status: draft
Current phase: none
Next: approve
Reason: draft created
Blockers: none
Allowed follow-up command: `scafld approve runx-a2a-front-v1`
Latest runner update: none
Review gate: not_started

Roadmap: Wave 4 (later, lowest-demand lane). Do not pull forward ahead of demand.

## Summary

Enable the gated-off agent-to-agent (a2a) front. The A2a adapter is fully
implemented (`oss/crates/runx-runtime/src/adapters/a2a.rs`: A2aTask lifecycle,
send-message/get-task, fixture transport, receipt sealing) but compiled out
(`#[cfg(feature = "a2a")]`, not in the runx-cli feature set). Shipping it means
enabling the feature, splitting in a live HTTP transport behind the supervised
lane, and proving it with a governed-delegation skill + demo. skill-seeds treats
multi-agent delegation as secondary, so this is the last/lowest-demand lane —
build when demand pulls, not before.

## Objectives

- Enable `a2a` in runx-cli and split the fixture transport into a live HTTP one
  (submit a task to a peer agent, poll to completion, seal the remote-reported
  result under an authority cap).
- A governed-delegation / portfolio-router branded facade or demo graph that
  exercises the front through canonical delegation/federation semantics, with a
  sealed receipt and an out-of-scope refusal.

## Scope

In scope:
- Enable the feature; live a2a transport behind the runtime-supervised lane; a
  governed-delegation facade/demo harness; the demo.

Out of scope:
- a2a as a default/showcase pattern (host-drives stays the default; runx does not
  spawn nested agents as the headline shape).
- Building this before demand (it is Wave 4 for a reason).

## Dependencies

- The built-but-gated a2a adapter; the agent front; authority + receipts; the
  supervised external lane for the live transport (deny.toml bans reqwest outside
  runx-runtime).

## Assumptions

- The existing fixture-tested A2aTask lifecycle is the right contract; shipping is a
  transport split + enablement, not a rebuild.

## Touchpoints

- `oss/crates/runx-runtime/src/adapters/a2a.rs`, `adapters.rs` (feature gate),
  `runx-cli/Cargo.toml` (feature set); a new governed-delegation skill + example.

## Risks

- **Low demand / over-investment.** Mitigation: keep it minimal; ship the lane +
  one skill, not a delegation framework.
- **Live transport on the wrong side.** Mitigation: the call rides the supervised
  external lane, never a pure crate.

## Acceptance

Profile: strict

Validation:
- `a2a` is enabled in the shipped binary; a governed a2a call to a peer agent seals
  a receipt; an out-of-scope delegation is refused; gates green.

## Phase 1: Enable a2a + live transport + governed-delegation skill

Status: pending
Dependencies: a2a adapter (built/gated), agent front

Objective: ship the a2a lane end to end with one governed-delegation skill.

Changes:
- Enable the feature; live HTTP transport; a governed-delegation skill + harness.

Acceptance:
- [ ] `ac1` command - governed a2a call seals + refuses out-of-scope
  - Command: `runx harness examples/a2a-delegation/<case>.yaml --json`
  - Expected kind: `exit_code_zero`
  - Status: pending

## Rollback

- Additive lane behind a feature flag; disable the feature + remove the skill/example.

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

- none
