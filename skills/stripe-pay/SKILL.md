---
name: stripe-pay
description: Run the Stripe SPT payment graph from quote to scoped-token charge proof.
runx:
  category: payments
---

# Stripe Pay

Run the Stripe Shared Payment Token settlement graph.

The graph turns a payment-required signal into a quote, selects and reserves a
payment decision, routes approval when required, fulfills the Stripe SPT rail under
attenuated authority, and leaves recovery evidence if the rail result is
ambiguous.

This is the settlement-pinned Stripe marquee. The face is public and rail-pinned
to `stripe-spt`; the executor path mints a scoped SPT with
`usage_limits[max_amount]` equal to the kernel admission and charges with
`payment_method_data[shared_payment_granted_token]`. Test-mode and live-mode
safety belong in the rail runner and runtime policy; this graph never accepts API
keys, webhook secrets, card data, PANs, or raw unrestricted provider tokens.

## Quality Profile

- Purpose: execute a paid action through runx authority without hiding the
  payment governance path.
- Audience: agent hosts, operators, approval reviewers, and receipt verifiers.
- Artifact contract: `payment_execution`, `payment_quote_packet`,
  `payment_reservation_packet`, `payment_rail_packet`, and `recovery_packet`
  when needed.
- Evidence bar: every successful execution carries a quote, selected decision,
  reserved child authority, idempotency key, Stripe charge id, provider event id,
  shared payment token ref, admission token digest, and receipt seal requirement.
- Voice bar: operator-grade execution record; avoid wallet/product marketing.
- Strategic bar: keep rails pluggable while core owns payment authority.
- Stop conditions: stop before rail execution when quote, approval, parent
  authority, reservation, idempotency, or spend capability is missing.

## Output

- `payment_execution`: overall status and receipt/proof refs.
- `payment_quote_packet`: normalized quote output.
- `payment_reservation_packet`: selected reservation decision and child
  authority term.
- `payment_rail_packet`: rail proof and credential envelope.
- `recovery_packet`: recovery assessment when a rail result is ambiguous.

## Inputs

- `payment_signal` (required): payment-required signal or challenge.
- `parent_payment_authority` (required): parent payment authority term or ref.
- `rail_profile_ref` (required): configured rail profile reference.
- `realm` (optional): authority realm.
- `spend_policy` (optional): policy limits and approval thresholds.
- `approval_context` (optional): prior approval evidence.
- `idempotency_seed` (optional): stable idempotency material.
