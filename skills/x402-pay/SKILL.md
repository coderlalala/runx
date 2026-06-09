---
name: x402-pay
description: Execute a governed x402 payment by delegating to the canonical spend flow with the x402 runtime path selected.
runx:
  category: payments
---

# X402 Pay

Execute a governed outbound payment over x402.

This is a branded catalog skill, not a separate payment model. It exists because
operators and agents recognize x402 as the capability they want to use. At
runtime it delegates to the canonical `spend` family with runtime path `x402`,
then seals the same spend receipt with x402-specific proof evidence attached.

## What this skill does

`x402-pay` turns an x402 payment-required challenge into a governed spend:
quote, reserve, approval when required, x402 fulfillment, recovery posture, and
receipt-before-success. The branded surface selects the x402 runtime path and
documents x402 evidence, redaction, hosted/local requirements, and failure
states.

It does not bypass `spend`, mint unrestricted wallet authority, retry with new
idempotency material, or treat a provider success response as final until the
runx receipt seals.

## When to use this skill

- A service returns an x402-compatible payment-required challenge.
- An operator wants the x402-branded catalog path while preserving canonical
  spend authority and receipts.
- A testnet or hosted x402 profile is configured and the agent must prove that
  the paid action stayed inside a payment grant.
- A demo needs a recognizable x402 entrypoint rather than the generic `spend`
  skill name.

## When not to use this skill

- To settle through Stripe SPT, MPP, CDP, or mock fixtures. Use the matching
  branded skill or `spend` with the selected runtime path.
- To choose a rail based only on available credentials. Runtime path selection
  must be driven by the payment signal and policy.
- To price or verify inbound customer payments. Use `charge`.
- To accept raw wallet private keys, seed phrases, bearer tokens, facilitator
  secrets, or raw payment payloads as agent-visible output.
- To claim success when x402 settlement is ambiguous or the runx receipt is not
  sealed. Return `needs_agent` or `escalated`.

## Procedure

1. Validate that `payment_signal.rail` or equivalent challenge metadata is
   `x402`. If another path is requested, stop with `needs_agent`.
2. Validate `parent_payment_authority` and `rail_profile_ref`. The authority
   must permit the quoted counterparty, amount, currency, operation, realm, and
   x402 channel.
3. Delegate to `spend` runner/runtime path `x402` with the original signal,
   parent authority, x402 profile reference, realm, spend policy, approval
   context, and idempotency seed.
4. Require the canonical spend flow to quote and reserve a proven subset before
   any x402 settlement call receives authority.
5. Pause at the spend approval gate when policy requires it. A denied or missing
   approval prevents x402 fulfillment.
6. Fulfill through the scoped x402 rail runner. Pass capability refs and profile
   refs only; do not expose raw funding or facilitator material.
7. Record x402 evidence as references, hashes, transaction/facilitator refs, or
   redacted provider metadata. Never print secret-bearing material.
8. If the x402 response is ambiguous, preserve recovery evidence under the same
   idempotency key and return `escalated`; do not retry under a new key.
9. Return success only after the canonical spend receipt seals with x402 proof
   evidence.

## Edge cases and stop conditions

- **Non-x402 signal:** return `needs_agent`; this facade must not silently route
  to another runtime path.
- **Challenge drift:** stop when amount, currency, counterparty, operation,
  network, or challenge id changes after quote.
- **Parent grant mismatch:** stop when the parent grant does not cover x402,
  the counterparty, or the quoted amount.
- **Missing profile or funding reference:** return `needs_agent`; do not request
  raw wallet keys from the agent.
- **Approval missing or denied:** do not call the x402 rail runner.
- **Ambiguous settlement:** return `escalated` with recovery refs and preserve
  idempotency.
- **Unsealed receipt:** return `escalated`; provider evidence alone is not final.

## Output schema

```yaml
decision: sealed | denied | needs_agent | escalated
canonical_skill: runx/spend
runtime_path: x402
payment_execution:
  payment_quote_packet: object
  payment_reservation_packet: object
  payment_approval: object
  effect_evidence_packet:
    rail_result: object
    rail_proof:
      proof_ref: string
      challenge_id: string
      transaction_ref: string | null
      facilitator_ref: string | null
    redactions: [string]
    recovery_hint: object | null
sealed_receipt_ref: string | null
open_questions: [string]
```

## Worked example

An x402 paid-search endpoint returns challenge `ch_x402_001` for `1.25 USD`
against counterparty `merchant:demo`. The parent grant allows one payment
commit up to `1.25 USD` for that counterparty in realm `test`. `x402-pay`
delegates to `spend:x402`, reserves a child authority for the exact amount and
operation, records the approval gate, fulfills through the x402 runtime path,
redacts rail session material, and returns `decision: sealed` only after the
receipt includes the x402 proof ref.

If the challenge changes to `2.00 USD` after quote, the skill returns
`needs_agent` or `escalated` and does not spend under the stale approval.

## Inputs

- `payment_signal` (required): x402 payment-required signal or challenge.
- `parent_payment_authority` (required): parent payment authority term or
  authority reference.
- `rail_profile_ref` (required): configured x402 runtime-path profile reference.
- `realm` (optional): authority realm such as `local`, `test`, or `prod`.
- `spend_policy` (optional): policy limits and approval thresholds.
- `approval_context` (optional): prior approval evidence.
- `idempotency_seed` (optional): stable idempotency material.
