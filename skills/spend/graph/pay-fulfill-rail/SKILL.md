---
name: pay-fulfill-rail
description: Fulfill a reserved payment challenge through one rail under attenuated runx authority.
runx:
  category: payments
---

# Pay Fulfill Rail

Execute one rail operation below the runx spend gate.

This skill adapts a protocol or provider challenge to the credential/proof
shape needed by the paid tool. It can spend only when the parent harness has
already selected a Decision, reserved budget by idempotency key, and passed an
attenuated `payment` authority term into the child harness.

The skill must receive a scoped spend capability or provider session reference,
never raw funding material. It returns rail proof for the receipt; it
does not decide policy, approval, retry, or success.

## Quality Profile

- Purpose: make a rail-specific payment mutation visible as one governed Act.
- Audience: runtime harness, receipt verifier, rail implementer, and operator.
- Artifact contract: `rail_result`, `rail_proof`, `credential_envelope`,
  `redactions`, and `recovery_hint`.
- Evidence bar: include rail response refs, idempotency key, challenge id, and
  proof hash/ref. Redact sensitive payload fields.
- Voice bar: operational status only: fulfilled, declined, retryable,
  recovered, or ambiguous.
- Strategic bar: keep provider churn inside the rail runner; keep governance in
  core.
- Stop conditions: return `needs_agent` or `ambiguous` when the rail
  response cannot be tied to the idempotency key and reserved authority.

## Output

- `rail_result`: rail status, amount, currency, counterparty, and operation.
- `rail_proof`: redacted proof payload or proof ref for the child harness
  receipt.
- `credential_envelope`: credential or token returned to the paid tool, with
  sensitive fields redacted or referenced.
- `redactions`: fields withheld from receipts and logs.
- `recovery_hint`: idempotency/retry guidance for `pay-recover`.

## Inputs

- `payment_challenge` (required): protocol/provider challenge to fulfill.
- `reserved_payment_authority` (required): child payment authority term.
- `spend_capability_ref` (required): scoped single-use spend capability ref.
- `rail_profile_ref` (required): configured rail profile reference.
- `idempotency` (required): reservation key and recovery fields.
- `quote_packet` (optional): source quote packet for evidence continuity.
