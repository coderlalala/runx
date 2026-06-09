---
spec_version: '2.0'
task_id: issue-to-pr-reach-fix
created: "2026-05-04T00:00:00Z"
updated: "2026-05-04T00:00:00Z"
title: Fix-boundary harness reach
status: approved
harden_status: not_run
size: small
risk_level: low
---

# Fix-boundary harness reach

## Current State

Status: draft
Current phase: none
Next: none
Reason: none
Blockers: none
Allowed follow-up command: none
Latest runner update: none
Review gate: not_started

## Summary

Update the fixture README with one bounded line.

## Context

CWD: `.`

Packages:
- fixture

Files impacted:
- `README.md`

Invariants:
- bounded_scope

Related docs:
- none

## Objectives

- Replace the fixture README text with approved guidance.

## Scope

- `README.md`

## Dependencies

- None.

## Assumptions

- None.

## Touchpoints

- README fixture content.

## Risks

- None.

## Acceptance

Profile: standard

Definition of done:
- [ ] `dod1` README.md contains fixture guidance.

Validation:
- [ ] `v1` test - README contains fixture guidance.
  - Command: `grep -q '^fixture guidance$' README.md`
  - Expected kind: `exit_code_zero`
  - Timeout seconds: none
  - Result: none
  - Status: pending
  - Evidence: none
  - Source event: none
  - Last attempt: none
  - Checked at: none

## Phase 1: Update fixture README

Goal: Write the bounded README change and validate it.

Status: pending
Dependencies: none

Changes:
- `README.md` (all, exclusive) - Replace the contents with fixture guidance.

Acceptance:
- [ ] `ac1_1` test - README contains fixture guidance.
  - Command: `grep -q '^fixture guidance$' README.md`
  - Expected kind: `exit_code_zero`
  - Timeout seconds: none
  - Result: none
  - Status: pending
  - Evidence: none
  - Source event: none
  - Last attempt: none
  - Checked at: none

## Rollback

Strategy: per_phase

Commands:
- `git checkout HEAD -- README.md`

## Review

Status: not_started
Verdict: none

Findings:
- none

Passes:
- none

## Self Eval

Status: not_started

Notes:
none

Improvements:
- none

## Deviations

- none

## Metadata

Tags:
- fixture

## Origin

Source:
- harness

Repo:
- none

Git:
- none

Sync:
- none

Supersession:
- none

## Harden Rounds

- none

## Planning Log

- none
