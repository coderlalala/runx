---
name: charge-verify
description: Verify a returned payment credential and produce provider-side charge receipt evidence.
runx:
  category: payments
---

# Charge Verify

Verify a returned payment credential against a priced provider-side challenge.

This skill models the verification step before a provider forwards a paid MCP
operation. It selects the settlement-family verifier from profile inputs,
checks idempotency evidence, emits a settlement proof reference, and exposes
the receipt evidence that a future runtime must seal before forwarding. It
does not set prices, issue challenges, receive raw merchant credentials, or
forward the upstream tool call.

## What this skill does

1. **Bind to the priced challenge.** Confirm that the returned credential
   belongs to the exact `charge_price_packet`, challenge id, settlement family,
   counterparty, amount, currency, expiry, and idempotency key.
2. **Run the family verifier.** Select the verifier named by the settlement
   family and verify the credential or proof reference under the admitted
   provider-side payment authority.
3. **Check replay and expiry.** Reject stale, reused, mismatched, or
   cross-family credentials before any provider forwarding can happen.
4. **Produce receipt evidence.** Return a redacted settlement proof reference,
   verification status, idempotency state, and the sealed receipt ref required
   before forwarding.
5. **Stop on ambiguity.** Return `escalated` when the credential family,
   challenge binding, idempotency state, proof fields, or sealing state cannot
   be established.

It does not set a price, issue a challenge, widen an authority grant, execute a
refund, settle a dispute, or call the paid upstream tool.

## When to use this skill

- A provider has already priced a paid MCP operation, issued a challenge, and
  received a caller credential.
- A test harness needs to prove that payment evidence would seal before a paid
  provider forwards work.
- A receipt reviewer needs the redacted proof fields that explain why a
  provider-side charge was accepted or rejected.

## When not to use this skill

- To determine what the provider should charge. Use `charge-price`.
- To accept a credential for a different amount, family, counterparty, operation,
  or challenge id.
- To inspect raw merchant secrets or print raw credentials into a receipt.
- To recover or refund a failed charge. Verification can report recovery hints,
  but it does not perform recovery.

## Procedure

1. Validate all required inputs are present: price packet, challenge packet,
   returned credential, priced authority, verifier capability, settlement
   family, and idempotency material.
2. Compare the challenge packet against the price packet. Amount, currency,
   operation, counterparty, expiry, and settlement family must match exactly.
3. Confirm the priced payment authority covers the challenged amount and does
   not allow a broader family, amount, operation, or counterparty than the price
   requires.
4. Confirm the credential claims the same challenge id and idempotency key. A
   reused key is valid only when the replay policy explicitly declares the prior
   verification equivalent and sealed.
5. Select the verifier through `verify_capability_ref`; never infer a verifier
   from credential shape alone.
6. Verify the credential with family-specific code and normalize the result to a
   redacted `settlement_proof`.
7. Require a sealed receipt ref before returning a forwardable `sealed` result.
8. Emit redaction notes for every omitted secret-bearing field.

## Edge cases and stop conditions

- **Amount, currency, operation, or counterparty mismatch:** return
  `escalated`; do not attempt a partial acceptance.
- **Expired challenge:** return `escalated` with recovery hint
  `challenge_expired`.
- **Replay with unknown prior state:** return `escalated`; replay is acceptable
  only when the idempotency policy proves the previous result sealed
  equivalently.
- **Verifier capability absent or wrong family:** return `escalated`; do not
  fall back to a generic verifier.
- **Raw secret-bearing credential field in output:** redact it and record the
  redaction. If the proof cannot be represented safely, return `escalated`.
- **Receipt not sealed:** return `escalated` with recovery hint
  `seal_required`; provider forwarding must wait.

## Output schema (`charge_verification`)

```yaml
decision: sealed | denied | escalated
verification_result:
  status: accepted | rejected | replayed | expired | ambiguous
  settlement_family: string
  challenge_id: string
  idempotency_key: string
settlement_proof:
  proof_ref: string
  family: string
  amount: string
  currency: string
  counterparty: string | null
sealed_receipt_ref: string | null
redactions:
  - field: string
    reason: string
recovery_hint: sealed | denied | reversal_required | challenge_expired | seal_required | operator_review
findings:
  - id: string
    severity: error | warning | info
    message: string
```

A `sealed` decision requires `verification_result.status: accepted` or
`replayed`, a non-null `sealed_receipt_ref`, and no error findings.

## Worked example

A caller returns a Stripe SPT credential for challenge `ch_test_01`. The price
packet, challenge packet, priced authority, credential claim, and idempotency
key all bind to `crm.enrich_lead`, `0.08 USD`, account `acct_test_123`, and
family `stripe-spt`. The Stripe verifier accepts the credential and the runtime
seals receipt `rcpt_abc`. The skill returns `decision: sealed`, a redacted
`settlement_proof.proof_ref`, the receipt ref, and redaction notes for omitted
credential material.

If the credential is for `0.10 USD` or a different challenge id, the skill
returns `decision: escalated`; it does not downscope the credential or forward
the provider call.

## Inputs

- `charge_price_packet` (required): output from `charge-price`.
- `charge_challenge_packet` (required): output from `charge-challenge`.
- `returned_credential` (required): caller-returned payment credential.
- `priced_payment_authority` (required): admitted provider-side payment term.
- `verify_capability_ref` (required): single-use verification capability ref.
- `settlement_family` (required): selected settlement family.
- `idempotency` (required): challenge idempotency key and replay policy.
