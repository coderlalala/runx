---
name: refund
description: Govern one refund linked to a sealed original charge receipt, with quote, reservation, approval, settlement evidence, and refund receipt sealing.
runx:
  category: payments
---

# Refund

Govern one refund linked to a sealed original charge receipt.

This skill is the public provider-side refund verb. It quotes refundable bounds
from the original receipt, reserves refund authority, gates the refund decision,
settles through one runtime path, and emits evidence for a refund receipt. The
original receipt link is mandatory; a refund without provenance is not a
governed refund.

The settlement family is a runtime path, not a separate public skill. Mock, MPP,
and Stripe refunds share the same authority story: prove the original charge,
quote the remaining refundable amount, reserve a refund under the same family,
approve the reversal, settle once under idempotency, and seal the refund
evidence.

## What this skill does

1. **Link the original receipt.** Require `original_receipt_ref` and a redacted
   original receipt summary before any refund authority is discussed.
2. **Quote refundable bounds.** Use `refund-quote` to calculate remaining amount,
   currency, settlement family, prior refund refs, and policy window.
3. **Reserve refund authority.** Use `refund-reserve` to bind the refund decision
   to the original receipt, selected amount, same settlement family, and
   idempotency key.
4. **Gate settlement.** Record the approval decision before any runtime path
   settles the refund.
5. **Settle and seal evidence.** Emit closure, proof ref, refund receipt ref,
   redactions, and recovery posture.

It does not silently refund an open dispute, refund across a different
settlement family, or infer authority from operator intent alone.

## When to use this skill

- A provider needs to reverse a previously sealed charge.
- A support or dispute workflow needs a receipt-linked refund artifact.
- A harness needs to prove refund behavior across mock, MPP, or Stripe runtime
  paths without exposing rail credentials.

## When not to use this skill

- To answer a chargeback or dispute without deciding a refund. Use
  `dispute-respond`.
- To refund when the original charge receipt is missing or unsealed.
- To perform a cross-family refund unless a future policy explicitly models that
  authority. The current graph requires same-family refund semantics.
- To retry an ambiguous refund under a new idempotency key.
- To print raw provider credentials, merchant secrets, or unrestricted rail
  tokens into output.

## Procedure

1. Validate `original_receipt_ref`, `original_receipt`, `refund_request`, and
   `parent_payment_authority`.
2. Confirm the original receipt is sealed and names amount, currency,
   counterparty, settlement family, and charge/refund lineage.
3. Run `refund-quote`. Stop when the original receipt, settlement family,
   refundable amount, prior refund set, or policy window is ambiguous.
4. Run `refund-reserve`. The reserved refund authority must bind to the original
   receipt and stay within remaining refundable bounds.
5. Pause at the refund approval gate. A denied or missing approval prevents
   settlement.
6. Settle through the selected runtime path and return refund closure, proof ref,
   refund receipt ref, redaction notes, and recovery posture.
7. If settlement is ambiguous, require recovery under the same idempotency key
   before retrying.

## Runtime paths

| Path | Use when | Required proof/evidence | Secret handling |
|---|---|---|---|
| `mock` | Deterministic local refund fixtures. | Mock refund proof ref, original receipt ref, refund idempotency key. | No real credentials; still redact fixture credential material. |
| `mpp` | The original charge settled through MPP and policy allows refund. | MPP refund proof ref, original receipt ref, idempotency key, settlement family. | Output refs only; do not expose rail session material. |
| `stripe` | The original charge settled through Stripe and policy allows refund. | Stripe refund proof/refund id when present, original charge receipt ref, idempotency key. | Never emit Stripe secret keys, webhook secrets, card data, PANs, or unrestricted tokens. |

There is no x402 refund runner in this skill. Current x402 support remains
buyer-side `spend` unless a separate product decision adds seller-side x402
refund semantics.

## Edge cases and stop conditions

- **Missing original receipt:** return `needs_agent`; a refund cannot be
  provenance-free.
- **Unsealed original receipt:** return `needs_agent`; the reversal must link to
  sealed charge evidence.
- **Prior refund already covers the amount:** return `denied` or `needs_agent`;
  do not double-refund.
- **Settlement family mismatch:** return `needs_agent`; same-family is required.
- **Approval denied or absent:** do not settle.
- **Ambiguous settlement:** return `escalated` and require recovery under the
  same idempotency key.
- **Dispute is open:** do not mask it with an untracked refund; route through
  `dispute-respond` or record the dispute linkage explicitly.

## Output schema (`refund_execution`)

```yaml
decision: sealed | denied | needs_agent | escalated
runtime_path: mock | mpp | stripe
refund_quote_packet:
  refund_quote: object
  refundable_bounds: object
  original_receipt_link: object
  settlement_family: string
refund_reservation_packet:
  payment_decision: object
  reserved_payment_authority: object
  idempotency: object
  reservation: object
refund_approval:
  approved: boolean
  gate_id: string
refund_rail_packet:
  refund_closure: object
  refund_proof: object
  refund_receipt_ref: string | null
open_questions: [string]
```

A `sealed` decision requires the original receipt link, same-family reservation,
approval, settlement proof, refund receipt ref, and no unresolved recovery
state.

## Worked example

Receipt `receipt:charge:stripe:paid-search-001` proves a sealed Stripe charge
for `1.25 USD`. The operator requests a full refund. `refund` quotes remaining
refundable bounds of `1.25 USD`, reserves refund authority bound to that receipt,
records approval, settles through the `stripe` runtime path, and emits
`receipt:refund:stripe:paid-search-001`. The result is `decision: sealed`.

If a prior refund receipt already covers `1.25 USD`, the skill returns
`decision: denied` or `needs_agent` with the prior receipt refs. It does not
issue a second refund.

## Inputs

- `original_receipt_ref` (required): linked sealed charge receipt reference.
- `original_receipt` (required): redacted original charge receipt summary.
- `refund_request` (required): requested amount and reason.
- `parent_payment_authority` (required): parent payment authority term or ref.
- `approval_context` (optional): prior approval evidence.
- `idempotency_seed` (optional): stable refund idempotency seed.
