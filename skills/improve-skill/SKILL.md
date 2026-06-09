---
name: improve-skill
description: Turn a failed receipt or harness outcome into a bounded skill improvement proposal.
runx:
  category: authoring
---

# Improve Skill

Review a failed or suspicious run and draft the next bounded improvement.

This is a composite skill that graphs `review-receipt` into `write-harness`.
It takes failure evidence, such as a receipt, harness output, or manual
summary, diagnoses the root cause, and produces an updated skill proposal with
replayable fixtures that cover the failure.

## What this skill does

1. Reviews the failure through the `review-receipt` lens.
2. Separates symptoms, root cause, contract gaps, and evidence gaps.
3. Proposes one bounded skill improvement tied to the failure evidence.
4. Drafts acceptance checks or harness fixtures through the `write-harness`
   lens.
5. Produces a receipt-ready improvement package, not a broad rewrite.

## When to use this skill

- A skill run failed and you need to understand why and what to fix.
- A harness test is failing and you need to update the skill, the fixture, or
  the expected output.
- A receipt shows suspicious behavior such as partial success, unexpected tool
  use, refusal drift, missing evidence, or output that does not match the skill
  contract.
- A public skill needs a concrete improvement proposal backed by a failed run,
  not an aspirational quality pass.
- A maintainer needs replayable evidence that the proposed fix covers the
  diagnosed failure.

## When not to use this skill

- For designing a new skill from scratch. Use `design-skill`.
- For general research. Use `prior-art`.
- When the fix is already known and approved. Make the direct change instead.
- When there is no receipt, harness output, manual failure summary, or other
  evidence. Return `needs_more_evidence`.
- When the requested change would hide a failure, weaken a refusal boundary, or
  remove required evidence. Refuse that part.
- When the failure is caused by an external outage or missing credential and no
  skill change would improve future behavior. Return `no_change` or
  `needs_human`.

## Procedure

1. Collect and bound the evidence.
   - Accept at least one of `receipt_id`, `receipt_summary`,
     `harness_output`, or a concrete manual failure report.
   - Identify the skill package if `skill_path` is available.
   - Gate: if there is no failing behavior, expected behavior, or observable
     evidence, stop with `needs_more_evidence`.

2. Reconstruct the skill contract.
   - Read the current `SKILL.md` and any local execution profile or fixture
     files needed to understand the promised behavior.
   - Note declared inputs, procedure, stop states, refusal behavior, and output
     schema.
   - Gate: if the skill path is missing and the evidence does not include the
     contract, return `needs_input`.

3. Diagnose the failure.
   - Trace what happened, where it diverged, and what evidence proves the
     divergence.
   - Classify the root cause as one primary type: `instruction_gap`,
     `contract_gap`, `fixture_gap`, `harness_gap`, `tool_boundary_gap`,
     `evidence_gap`, `runtime_flake`, `upstream_dependency`, or `operator_error`.
   - Do not treat a symptom, such as "test failed", as the root cause.

4. Decide whether a skill change is warranted.
   - Propose a skill improvement only if the evidence shows the skill contract,
     procedure, gates, examples, or fixtures should change.
   - If the harness expectation is wrong, propose a fixture or assertion change
     instead of changing the skill.
   - If the failure is outside the skill's contract, return `no_change` with
     the boundary explanation.

5. Draft one bounded improvement.
   - Keep the change as small as possible while preventing the same failure.
   - Preserve existing terminology and public contract unless the evidence
     proves it is wrong.
   - Split unrelated issues into separate proposals. Do not bundle multiple
     independent fixes into one improvement.

6. Add replayable checks.
   - Write or describe fixtures that reproduce the failure before the fix and
     pass after the fix.
   - Include the expected stop status, output shape, evidence refs, and refusal
     or needs-input behavior when relevant.
   - Gate: no improvement proposal is complete without an acceptance check or a
     clear reason a fixture cannot be produced.

7. Emit receipt expectations.
   - A valid receipt for this skill should record evidence sources, diagnosed
     root cause, proposed file changes or non-change decision, fixture names,
     acceptance criteria, and final status.

## Edge cases and stop conditions

- No evidence source supplied: return `needs_more_evidence`.
- Skill path unavailable and contract not included in the receipt: return
  `needs_input`.
- Failure evidence conflicts with the skill contract or with itself: return
  `needs_human` unless one interpretation is clearly supported.
- Multiple independent root causes: report them, select one primary bounded
  improvement, and list the rest as follow-up proposals.
- Flaky or environment-only failure: return `no_change` unless the skill can
  add a concrete gate, retry rule, or diagnostic that would help future runs.
- Unsafe improvement request: return `refused` for the unsafe change and
  explain the preserved boundary.
- Existing behavior is correct and the harness is stale: propose a harness
  update, not a skill rewrite.
- Evidence supports only a documentation clarification: keep the proposal to
  that clarification and its fixture.

## Output schema

Return a structured improvement package:

```yaml
status: improvement_proposed | fixture_update_proposed | no_change | needs_more_evidence | needs_input | needs_human | refused
skill_path: string | null
evidence:
  receipt_id: string | null
  harness_refs: [string]
  manual_refs: [string]
  limitations: [string]
failure_summary: string
expected_behavior: string
actual_behavior: string
root_cause:
  type: instruction_gap | contract_gap | fixture_gap | harness_gap | tool_boundary_gap | evidence_gap | runtime_flake | upstream_dependency | operator_error
  rationale: string
bounded_improvement:
  summary: string
  files_to_change: [string]
  non_goals: [string]
acceptance_checks:
  - name: string
    fixture: string | null
    should_fail_before: boolean
    expected_status: string
    expected_evidence: [string]
refusal_or_needs_input_behavior:
  applies: boolean
  expected_response: string | null
receipt_expectations:
  root_cause_recorded: boolean
  evidence_refs_recorded: boolean
  fixtures_recorded: [string]
follow_up_proposals: [string]
```

## Worked example

Input:

```yaml
skill_path: skills/sourcey
harness_output: |
  expected status: needs_input
  actual status: success
  case: missing citation source for quoted claim
receipt_summary: |
  The run produced a final answer with a quote but no source URL.
```

Output:

```yaml
status: improvement_proposed
skill_path: skills/sourcey
failure_summary: The skill allowed a quoted claim without a cited source.
expected_behavior: Stop with needs_input or omit the quote when no source is available.
actual_behavior: Returned success with unsourced quoted material.
root_cause:
  type: instruction_gap
  rationale: The procedure did not gate direct quotes on source availability.
bounded_improvement:
  summary: Add a quote-source gate and a fixture for missing source URLs.
  files_to_change:
    - skills/sourcey/SKILL.md
  non_goals:
    - Redesigning citation style
acceptance_checks:
  - name: missing-quote-source
    fixture: fixtures/missing_quote_source.yaml
    should_fail_before: true
    expected_status: needs_input
    expected_evidence:
      - no source URL
```

This is one bounded improvement because the evidence points to a specific
missing gate. A broader rewrite of the citation policy would be out of scope.

## Inputs

All inputs are optional, but at least one evidence source is required:

- `receipt_id`: receipt id to inspect. The receipt should contain step
  statuses, inputs, outputs, scope decisions, tool calls, and timing.
- `receipt_summary`: sanitized receipt or failure summary when the full receipt
  is not available.
- `harness_output`: sanitized harness output, assertion failure text, fixture
  name, or expected-versus-actual block.
- `skill_path`: path to the skill package being improved. Used to read
  `SKILL.md`, local fixtures, and execution profile files needed for diagnosis.
- `objective`: operator intent for the improvement pass. Use it to focus the
  review, not to override the evidence.
