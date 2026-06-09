---
name: refund-reserve
description: Reserve a profile-level refund decision against a linked charge receipt.
runx:
  category: payments
---

# Refund Reserve

Select or decline a refund intent after a refund quote.

This skill produces a Decision-shaped reservation packet with linked receipt
id, refundable bounds, idempotency key, approval state, and a child payment
authority term using the existing `refund` verb. It does not call a rail or
repair receipt state.

## Quality Profile

- Purpose: make the refund decision and authority subset visible before any
  settlement-family refund step.
- Audience: operators, approval reviewers, registry tooling, and future refund
  runtime enforcement.
- Artifact contract: `payment_decision`, `reserved_payment_authority`,
  `reservation`, `idempotency`, `approval`, and `open_questions`. Refund
  semantics are carried inside those existing payment reservation fields.
- Evidence bar: selected refunds must preserve original receipt link,
  settlement family, amount, currency, and idempotency.
- Strategic bar: reserve no broader authority than the linked charge receipt
  and quote allow.
- Stop conditions: return `policy_denied` when bounds, family, dispute state,
  approval, or idempotency is missing.

## Inputs

- `refund_quote_packet` (required): output from `refund-quote`.
- `parent_payment_authority` (required): parent payment authority term or ref.
- `approval_context` (optional): prior approval evidence.
- `idempotency_seed` (optional): stable seed for refund idempotency.
