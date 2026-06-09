---
name: evolve
description: Governed repo evolution with fixed phase semantics and bounded outcomes.
runx:
  category: code
---

# Evolve

Evolve the current repository through governed phases with fixed semantics and
optional bounded revision. With no objective, the default behavior is
introspective: analyze the repo, recommend one bounded improvement, and stop
at a plan-quality artifact set.

This is not autonomous code generation. It governs the shape around cognition:
every phase produces a typed artifact, every mutation requires approval, and
every step emits a receipt. A single evolve run ends in a bounded artifact, not
an open-ended improvement loop.

## What this skill does

1. Resolves an evolution mode: introspective recommendation or directed plan.
2. Runs fixed phase semantics over the repo context.
3. Produces typed planning artifacts grounded in repo evidence.
4. Stops at `spec` in the shipped runner; it does not mutate files, create
   patches, or open PRs.
5. Emits receipt-ready phase artifacts and an explicit stop state.

## When to use this skill

- To inspect a repository and recommend one bounded high-value improvement.
- To plan a directed change before implementation, patching, or PR work.
- To convert a repo, skill, receipt, or self-improvement objective into a
  governed artifact set.
- To create a scafld-style spec when governance applies.
- To preserve phase semantics while allowing the concrete runner to compress
  phases into fewer visible steps.

## When not to use this skill

- For autonomous mutation. The shipped runner stops at plan or spec artifacts.
- For `terminate=patch` or `terminate=pr`. Those modes are currently rejected
  until a real execution lane exists.
- For vague "make it better" loops without evidence or a bounded objective.
- To bypass policy, approval, receipts, or dirty-worktree risk reporting.
- When the target repository cannot be inspected. Return `needs_input` or
  `needs_more_evidence`.

## Procedure

1. Resolve mode and target.
   - No objective means introspective recommendation mode.
   - An objective means directed planning mode.
   - Resolve the target as repo, skill, receipt, or self from the objective and
     runner flags.
   - Gate: if `terminate` is `patch` or `pr`, stop immediately with
     `rejected_unsupported_termination`.
   - Gate: if the repo root or target cannot be resolved, return `needs_input`.

2. Preflight using `scope + ingest`.
   - Inspect repo root, git state, base branch, dirty worktree, `.ai/`
     presence, detected languages, likely test commands, and risk signals.
   - This step is deterministic: no agent cognition and no mutation.
   - Evidence expected: paths or commands inspected, git state, target
     resolution, and any constraints discovered.

3. Model the opportunity or objective.
   - In introspective mode, rank opportunities grounded in repo state,
     receipts, failing runs, visible docs, current plans, or explicit operator
     context.
   - In directed mode, restate the objective with target kind, constraints,
     success criteria, and non-goals.
   - Gate: if evidence does not support a bounded valuable move, return
     `no_recommendation` for introspection or `needs_more_evidence` for a
     directed objective.

4. Materialize planning artifacts.
   - Produce `opportunity_report` and `recommended_objective` for
     introspection.
   - Produce `objective_brief`, `diagnosis_report`, `change_plan`, and
     `spec_document` when applicable.
   - Include touchpoints, risk, acceptance checks, approval gates, and expected
     receipts. Do not include patch content unless a separate approved runner
     exists.

5. Evaluate plan quality.
   - Confirm the plan is one bounded outcome, not a hidden loop.
   - Confirm every recommendation is grounded in cited evidence.
   - Confirm mutation is not implied by prose.
   - Gate: if the plan depends on unresolved policy or ownership decisions,
     return `needs_human` with the exact decision required.

6. Stop and emit receipt expectations.
   - Stop at `spec` by default.
   - A valid receipt should record mode, target, preflight profile, phase
     artifacts, evidence refs, stop state, rejected termination requests, and
     whether runner-owned post-run reflect was appended.

## Canonical semantics

Complex runx skills share one internal phase language:

- `scope`
- `ingest`
- `model`
- `materialize`
- `evaluate`
- `revise`
- `verify`
- `ratify`

The current `evolve` runner uses a bounded subset and compresses some phases
into fewer concrete steps. That is allowed. What stays fixed is the meaning of
the phases, not the number of steps. When a runner opts in, runx may also
append a runner-owned post-run reflect projection after the receipt is written.
That projection is Knowledge-only metadata, not another canonical phase.

## Current runner mapping

### Introspect

Caller-mediated. This is the zero-argument recommendation lane. It uses
`scope + ingest + model` to analyze the current repo and produce:

- `opportunity_report`: ranked opportunities grounded in repo evidence.
- `recommended_objective`: one bounded next move.
- `change_plan`: a concrete plan for that recommendation.
- `spec_document`: a draft scafld-style spec when governance applies.

No approval gate and no mutation happen in this runner. It is introspection
only. It also opts out of post-run reflect because it is already an
introspective lane.

### Preflight

Deterministic. This is the current `scope + ingest` step. It inspects the
target repo and produces a `repo_profile`: repo root, git state, base branch,
dirty worktree, `.ai/` presence (scafld initialized), detected languages, test
commands, and risk signals. No agent cognition, no mutation.

### Plan

Caller-mediated. This is the current `model` step and also drafts bounded plan
artifacts. Given the objective and repo profile, it produces four artifacts in
one pass:

- `objective_brief`: restatement with target kind, constraints, and success
  criteria.
- `diagnosis_report`: current repo state relative to the objective.
- `change_plan`: ordered phases, acceptance checks, touchpoints, and risk.
- `spec_document`: draft scafld spec when governance applies.

Directed `evolve` runs opt into runner-owned post-run reflect. That projection
is derived from the completed receipt and run ledger after the bounded plan
lane finishes; it does not add another visible graph step or mutation path.

### Termination guard

`evolve` currently stops at plan/spec artifacts. If a caller requests
`terminate=patch` or `terminate=pr`, the runner fails immediately with a clear
error instead of pretending it can mutate or publish.

## Edge cases and stop conditions

- Unsupported termination: return `rejected_unsupported_termination` for
  `patch` or `pr`.
- No objective and no evidence-backed opportunity: return `no_recommendation`.
- Directed objective is too broad: return `needs_input` with a bounded rewrite
  request.
- Repo root missing, unreadable, or not a repo when one is required: return
  `needs_input`.
- Dirty worktree: continue only for planning, record the dirty state, and do
  not imply that changes can be applied safely.
- Target skill, receipt, or self context cannot be resolved: return
  `needs_input` with the missing selector.
- Evidence supports multiple unrelated improvements: rank them but recommend
  one bounded move; list the rest as non-selected opportunities.
- Requested hidden revision loop: refuse the loop and stop at the governed
  artifact.
- Policy or ownership decision required before planning can be trusted: return
  `needs_human`.

## Output schema

Return a structured artifact set:

```yaml
status: introspection_complete | plan_complete | no_recommendation | needs_input | needs_more_evidence | needs_human | rejected_unsupported_termination | refused
mode: introspect | directed
target:
  kind: repo | skill | receipt | self | unknown
  ref: string | null
inputs:
  objective: string | null
  repo_root: string
  terminate: spec | patch | pr
repo_profile:
  git_state: string
  base_branch: string | null
  dirty_worktree: boolean
  scafld_initialized: boolean
  languages: [string]
  test_commands: [string]
  risk_signals: [string]
opportunity_report:
  ranked: [object]
  evidence_refs: [string]
recommended_objective: string | null
objective_brief: object | null
diagnosis_report: object | null
change_plan:
  phases: [object]
  touchpoints: [string]
  acceptance_checks: [string]
  approval_gates: [string]
  risks: [string]
spec_document: string | null
stop_state:
  termination: spec
  mutation_performed: false
  reason: string
receipt_expectations:
  phase_artifacts_recorded: [string]
  evidence_refs_recorded: [string]
  rejected_requests: [string]
  post_run_reflect: appended | opted_out | not_applicable
```

## Worked example

Command:

```sh
runx evolve "add websocket adapter support"
```

Expected result:

```yaml
status: plan_complete
mode: directed
target:
  kind: repo
  ref: .
objective_brief:
  summary: Add websocket adapter support as a bounded repo change.
  success_criteria:
    - Adapter contract documented
    - Integration points identified
    - Acceptance checks named before mutation
change_plan:
  phases:
    - name: scope
      artifact: repo_profile
    - name: model
      artifact: objective_brief
    - name: materialize
      artifact: spec_document
  approval_gates:
    - Human approval before any patch lane
stop_state:
  termination: spec
  mutation_performed: false
  reason: Shipped evolve runner stops at plan/spec artifacts.
```

If the caller instead runs `runx evolve "add websocket adapter support"
--terminate pr`, the correct result is `rejected_unsupported_termination`,
not a synthetic PR plan.

## Inputs

- `objective` (optional): what to evolve toward. If omitted, `evolve` uses the
  introspective recommendation runner.
- `repo_root` (optional): repository root. Defaults to cwd.
- `terminate` (optional): defaults to `spec`. `patch` and `pr` are currently
  rejected by the shipped runner.
- `target` (optional): explicit repo, skill path, receipt id, or self target
  when the objective alone is not enough.
- `constraints` (optional): operator-provided boundaries such as ownership,
  forbidden files, required evidence, or delivery policy.
