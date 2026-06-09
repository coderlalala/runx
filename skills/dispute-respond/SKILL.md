---
name: dispute-respond
description: Prepare a governed dispute response artifact from a linked charge receipt.
runx:
  category: payments
---

# Dispute Respond

Prepare a profile-local dispute response artifact for a counterparty-initiated
payment dispute.

This skill attaches the linked sealed charge receipt, prior refund receipts,
and provider evidence, then selects a response posture. It does not settle the
dispute or produce a rail closure receipt.

## What this skill does

1. **Bind the dispute to a charge.** Match the dispute event to the original
   sealed charge receipt and the provider-side charge identity.
2. **Collect linked receipts.** Attach prior refund, verification, reversal, or
   recovery receipts so the response does not hide previous action.
3. **Classify the posture.** Recommend `accept`, `contest`, `refund_already_sent`,
   `needs_more_evidence`, or `operator_review` from the evidence and operator
   posture.
4. **Prepare the response artifact.** Emit the evidence packet a rail adapter or
   operator can submit: receipt refs, provider evidence refs, redactions,
   timeline, posture, and open questions.
5. **Stop on ambiguity.** Return `needs_agent` when the dispute id, linked charge
   receipt, prior refunds, evidence posture, or submission authority is unclear.

It does not submit the response to Stripe, x402, MPP, or another rail; it does
not issue a refund; and it does not mark a dispute closed.

## When to use this skill

- A provider receives a chargeback, payment dispute, or counterparty complaint
  tied to a runx-sealed charge.
- An operator wants a defensible response packet before deciding whether to
  contest or accept the dispute.
- A future rail-specific dispute adapter needs the normalized evidence and
  posture before submission.

## When not to use this skill

- To silently refund a disputed charge. Disputes and refunds are separate
  governed actions with separate receipts.
- To respond without a sealed original charge receipt.
- To fabricate product, delivery, identity, or consent evidence that is not
  already present in receipts, provider logs, or operator-supplied refs.
- To close the rail dispute. This skill prepares the artifact; a rail-specific
  action submits and later records closure.

## Procedure

1. Validate that `dispute_event` and `original_receipt_ref` are present.
2. Resolve the original sealed charge receipt. Confirm it matches the dispute
   amount, currency, provider, counterparty, operation, and settlement family
   when those fields are available.
3. Resolve `prior_refund_receipt_refs` and record whether any refund fully or
   partially covers the disputed amount.
4. Normalize provider evidence refs into a timeline: price, challenge,
   verification, service delivery, user consent, prior support contact, refund,
   and recovery events.
5. Apply `operator_posture` only as a preference. Evidence still controls the
   final posture; a contest request without evidence returns `needs_agent`.
6. Redact raw secrets and personal data that do not need to be submitted.
7. Emit `dispute_response`, `dispute_evidence`, `linked_receipts`, `posture`,
   `open_questions`, and a submission readiness decision.

## Edge cases and stop conditions

- **Missing or unsealed original receipt:** return `needs_agent`; do not prepare
  a contest packet.
- **Dispute does not match the receipt:** return `needs_agent` with the
  mismatched fields.
- **Prior refund exists:** do not recommend a silent second refund. Set posture
  `refund_already_sent` or `operator_review` and cite the refund receipt.
- **Evidence is stale or unauthenticated:** include it as low confidence or
  return `needs_agent` when it is required for the posture.
- **Operator wants to contest with no delivery or consent evidence:** return
  `needs_agent`; do not fabricate narrative.
- **PII or credentials in evidence:** redact before output and list each
  redaction.
- **Rail submission authority absent:** prepare the local artifact only and
  return `needs_agent` for submission.

## Output schema (`dispute_response_artifact`)

```yaml
decision: ready | needs_agent
dispute_response:
  dispute_id: string
  posture: accept | contest | refund_already_sent | needs_more_evidence | operator_review
  amount: string | null
  currency: string | null
  counterparty: string | null
  settlement_family: string | null
dispute_evidence:
  timeline:
    - at: string
      kind: price | challenge | verify | delivery | refund | support | recovery | dispute
      ref: string
      summary: string
  redactions:
    - field: string
      reason: string
linked_receipts:
  original_charge: string
  verification: [string]
  refunds: [string]
  recoveries: [string]
open_questions: [string]
recommendation: string
```

A `ready` decision means the artifact is complete for review or downstream rail
submission. It does not mean the dispute has been submitted or closed.

## Worked example

A Stripe test-mode dispute `dp_test_01` references charge receipt `rcpt_charge`.
The receipt proves the caller accepted challenge `ch_test_01` for `0.08 USD`,
verification receipt `rcpt_verify` sealed, and the provider delivery log ref
shows the paid result was returned. No refund receipts exist. The skill returns
`decision: ready`, posture `contest`, linked receipts, redacted evidence refs,
and a recommendation to submit the packet through the Stripe dispute adapter.

If refund receipt `rcpt_refund` already covers the full disputed amount, the
skill returns posture `refund_already_sent`, cites the refund receipt, and
warns against an untracked second refund.

## Inputs

- `dispute_event` (required): provider-initiated dispute or chargeback event.
- `original_receipt_ref` (required): linked sealed charge receipt reference.
- `prior_refund_receipt_refs` (optional): prior refund receipts.
- `evidence_refs` (optional): provider evidence references.
- `operator_posture` (optional): requested response posture.
