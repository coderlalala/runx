---
name: refund-quote
description: Quote refundable bounds from a linked sealed charge receipt.
runx:
  category: payments
---

# Refund Quote

Inspect a sealed charge receipt and compute profile-level refundable bounds.

This skill is non-mutating. It links the refund request to exactly one
original charge receipt, reports remaining amount, settlement family, refund
window, and prior refund references, and leaves authorization to reservation
and future runtime enforcement.

## Quality Profile

- Purpose: make refund eligibility legible before any refund authority is
  reserved.
- Audience: provider operators, approval reviewers, registry tooling, and
  future refund runtime enforcement.
- Artifact contract: `refund_quote`, `refundable_bounds`,
  `original_receipt_link`, `settlement_family`, and `open_questions`.
- Evidence bar: every refundable amount and family must trace to the linked
  receipt and prior refund receipts.
- Strategic bar: never infer cross-family refund permission.
- Stop conditions: return `needs_agent` when the original receipt, settlement
  family, amount, or prior refund set is ambiguous.

## Inputs

- `original_receipt_ref` (required): linked sealed charge receipt reference.
- `original_receipt` (optional): redacted receipt summary.
- `refund_request` (optional): requested amount, reason, and operator note.
- `prior_refund_receipt_refs` (optional): prior refund receipts.
- `policy` (optional): provider refund window and limit policy.
