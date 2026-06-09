# Skill Quality Standard

Public runx skills are agent execution context. They are not marketing pages,
feature lists, or aspirational roadmap entries. They tell an agent how to carry
out a consequential action thoroughly and safely: what world it is operating in,
what authority it has, what evidence it may trust, what procedure to follow,
when to stop, and what artifact to emit.

The public catalog is reserved for agent-facing capabilities: canonical
governed actions and branded provider/tool facades over those actions. A skill
belongs there only when it gives the agent enough context to execute a
deployable capability through a clear authority, gate, finality, and receipt
story. Internal lifecycle phases, graph stages, fixtures, and runtime paths
are not skills in the final design.

Every public `SKILL.md` must be specific enough that an agent can execute it
without inventing procedure and a reviewer can audit the result without trusting
the agent's prose.

## Required Structure

Each public skill must include these sections:

- `## What this skill does`: the concrete consequential action, what it emits,
  and what it explicitly does not do.
- `## When to use this skill`: legitimate use cases and the stage of the
  workflow where the skill belongs.
- `## When not to use this skill`: near-misses, higher-risk alternatives, and
  cases that require a different gate or human decision.
- `## Procedure`: ordered execution steps, including evidence collection,
  authority checks, validation, and final decision.
- `## Edge cases and stop conditions`: ambiguity, stale inputs, replay risk,
  missing authority, missing evidence, secret exposure, and explicit refusal or
  needs-input behavior.
- `## Output schema`: the structured artifact an agent should return.
- `## Worked example`: at least one happy path and enough contrast to show how a
  refusal or needs-input case is represented.
- `## Inputs`: required and optional inputs with operational meaning.

## Content Bar

- Write for execution. The agent should finish the file knowing the domain
  context, governing constraints, expected evidence, safe actions, unsafe
  actions, and exact output shape.
- Name the authority involved. Do not imply permission from intent alone.
- Name the gate. If a mutation, charge, delegation, or production action can
  happen, state the approval/finality condition that must exist first.
- Name the evidence. Every amount, scope, actor, counterparty, source, and
  recommendation must be traceable to an input, a receipt, policy, or a named
  inference.
- Name the stop condition. Missing price, stale receipt, mismatched scope,
  replay, ambiguous counterparty, and missing owner are not warnings to work
  around; they change the decision.
- Keep raw secrets out. Skills describe redaction and hash/reference behavior;
  they never ask the agent to print tokens, keys, credentials, or unredacted
  payment material into receipts or outputs.
- Preserve domain boundaries. A pricing skill does not verify credentials; an
  auditor does not repair; an overlay generator does not fork the wrapped skill.
- Prefer refusal over retrofitting. If the evidence does not support the action,
  the skill returns `needs_input`, `needs_more_evidence`, `reject`, or
  `refused`, not a best-effort artifact.
- Keep authoring scaffolding out of the public skill. Internal review concepts
  like purpose, audience, artifact contract, evidence bar, and strategic bar
  should be expressed through the operating instructions, examples, and schema
  rather than repeated as a visible rubric.

## Catalog Gate

The public catalog test enforces the required sections for every skill with
`catalog.visibility: public`. Runnable internals belong in owner-local graph
stages at `skills/<name>/graph/<stage>/X.yaml`; they are not hidden catalog
skills and should not carry public-skill documentation requirements.
