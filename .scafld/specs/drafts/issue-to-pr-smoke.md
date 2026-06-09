---
spec_version: '2.0'
task_id: issue-to-pr-smoke
created: '2026-05-04T00:00:00Z'
updated: '2026-05-04T00:00:00Z'
status: draft
harden_status: not_run
size: micro
risk_level: low
---

# Harness Task

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

Harness summary

## Context

CWD: `. `

Packages:
- fixture

Files impacted:
- `README.md`

Invariants:
- bounded_scope

Related docs:
- none

## Objectives

- Update README.md.

## Scope

- `README.md`

## Dependencies

- None.

## Assumptions

- None.

## Touchpoints

- README.md

## Risks

- None.

## Acceptance

Profile: standard

Definition of done:
- [ ] `dod1` README.md contains fixture guidance.

Validation:
- [ ] `v1` test - README contains fixture guidance.
  - Command: `test -f README.md`
  - Expected kind: `exit_code_zero`
  - Status: pending

## Phase 1: Update README

Goal: Update README.md.

Status: pending
Dependencies: none

Changes:
- `README.md` (all, exclusive) - Update README.md.

Acceptance:
- [ ] `ac1_1` test - README exists.
  - Command: `test -f README.md`
  - Expected kind: `exit_code_zero`
  - Status: pending

## Rollback

Strategy: per_phase

Commands:
- none

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
