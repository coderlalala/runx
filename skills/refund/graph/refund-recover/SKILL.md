---
name: refund-recover
description: Inspect an ambiguous refund idempotency key and recommend a terminal action.
runx:
  category: payments
---

# Refund Recover

Reconcile a refund idempotency key after timeout, crash, retry, or ambiguous
settlement state.

This skill is profile-only. It reports whether a prior refund attempt appears
mutated, pending, declined, safely retryable, or escalated. It does not repair
durable receipt state or issue another rail mutation.

## Quality Profile

- Purpose: make ambiguous refund state visible before any repeated mutation.
- Audience: operators, recovery reviewers, registry tooling, and future refund
  runtime enforcement.
- Artifact contract: `recovery_assessment`, `refund_lookup`, `proof_refs`,
  `recommended_action`, and `open_questions`.
- Evidence bar: every recommendation must be keyed by original receipt ref and
  refund idempotency key.
- Strategic bar: never hide ambiguous settlement state as success.
- Stop conditions: return `escalated` when rail lookup or proof refs are
  incomplete.

## Inputs

- `original_receipt_ref` (required): linked sealed charge receipt reference.
- `refund_idempotency` (required): refund key and replay metadata.
- `settlement_family` (required): original receipt settlement family.
- `prior_refund_attempt` (optional): prior rail attempt summary.
- `receipt_refs` (optional): existing receipt or proof refs.
