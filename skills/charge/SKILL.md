---
name: charge
description: Govern one inbound provider-side paid tool call through price, challenge, credential verification, receipt sealing, and receipt-gated forwarding.
runx:
  category: payments
---

# Charge

Govern one inbound paid tool call that runx exposes to another agent.

This skill is the public provider-side charge verb. It prices an inbound MCP
operation, emits a payment challenge, verifies the returned credential under the
priced authority, seals the charge receipt, and forwards the upstream operation
only after the sealed receipt exists. It is the seller-side mirror of `spend`.

The settlement family is a runtime path, not a separate catalog skill. Mock, MPP,
and Stripe paths share the same authority story: price first, challenge with
idempotency, verify against the exact challenge, seal before forward, and never
print raw credential material into the receipt.

## What this skill does

1. **Price the inbound operation.** Use `charge-price` to bind the tool call to
   provider policy, amount, currency, counterparty, accepted families, expiry,
   and requested payment authority.
2. **Issue a challenge.** Use `charge-challenge` to produce the
   `effect_required` signal and idempotency packet that the caller must satisfy.
3. **Verify the returned credential.** Use `charge-verify` to bind the credential
   to the exact price, challenge, family, counterparty, amount, and idempotency
   key.
4. **Seal before forwarding.** Seal the charge receipt with the verification
   evidence before the provider forwards the paid operation.
5. **Forward only under proof.** Forwarding is modeled as a separate step gated
   by `charge_seal.data.sealed == true`.

It does not calculate outbound spend, issue refunds, resolve disputes, or accept
raw merchant credentials as output.

## When to use this skill

- A runx-hosted provider is about to expose a paid MCP operation to a caller.
- A paid provider harness needs to prove receipt-before-forward behavior across
  mock, MPP, or Stripe settlement families.
- A dispute or audit workflow needs a sealed seller-side charge receipt linked
  to the original price, challenge, verification, and forwarded result.

## When not to use this skill

- To spend money as the buyer. Use `spend`.
- To reverse a prior charge. Use `refund`.
- To issue a challenge without a provider pricing policy.
- To verify a credential for a different amount, counterparty, challenge,
  operation, or settlement family.
- To forward the paid tool call before the charge receipt is sealed.

## Procedure

1. Validate `mcp_tool_call`, `provider_policy`, `returned_credential`,
   `verify_capability_ref`, and idempotency material.
2. Select the settlement family from provider policy and returned credential. If
   the family is missing or unsupported, return `needs_agent`.
3. Run `charge-price`. Stop when amount, currency, operation, counterparty,
   settlement family, or price evidence is ambiguous.
4. Run `charge-challenge`. The challenge must carry a stable idempotency key and
   require receipt-before-forward.
5. Run `charge-verify`. The returned credential must match the challenge and
   priced authority exactly.
6. Seal the charge receipt. The receipt must include price evidence, challenge
   id, verification result, settlement proof ref, idempotency key, redactions,
   and receipt ref.
7. Forward the upstream operation only when the seal step records `sealed: true`.
8. If any step is ambiguous, return `needs_agent` or `escalated`; do not forward
   the paid call.

## Runtime paths

| Path | Use when | Required proof/evidence | Secret handling |
|---|---|---|---|
| `mock` | Deterministic local provider-charge fixtures. | Mock proof ref, challenge id, idempotency key, sealed charge receipt ref. | No real credentials; still redact fixture credential material. |
| `mpp` | Provider policy accepts MPP settlement. | MPP credential ref, settlement proof ref, challenge id, idempotency key. | Output refs only; do not expose rail session material. |
| `stripe` | Provider policy accepts Stripe-side charge credentials. | Stripe credential/proof ref, provider event or charge ref when present, challenge id, idempotency key. | Never emit Stripe secret keys, webhook secrets, card data, PANs, or unrestricted tokens. |

There is no x402 provider-side charge runner in this skill. Current x402 support
is buyer-side `spend` unless a separate product decision adds seller-side x402
charge semantics.

## Edge cases and stop conditions

- **No provider policy:** return `needs_agent`; no default price exists.
- **Family mismatch:** return `escalated` when challenge, policy, and returned
  credential name different settlement families.
- **Credential replay:** return `escalated` unless the idempotency policy proves
  the prior verification is equivalent and sealed.
- **Verification accepted but receipt missing:** do not forward; return
  `escalated` with a seal-required finding.
- **Forward step requested early:** refuse; forwarding is gated by sealed
  receipt evidence.
- **Raw credential material in output:** redact and record the redaction; if it
  cannot be safely represented, return `escalated`.

## Output schema (`charge_execution`)

```yaml
decision: sealed | denied | needs_agent | escalated
runtime_path: mock | mpp | stripe
charge_price_packet:
  charge_price: object
  requested_payment_authority: object
charge_challenge_packet:
  effect_required_signal: object
  charge_challenge: object
  idempotency: object
charge_verification_packet:
  verification_result: object
  settlement_proof: object
  sealed_receipt_ref: string | null
  redactions: [string]
charge_seal:
  sealed: boolean
  receipt_ref: string
forwarded_result:
  forwarded: boolean
  result_ref: string | null
open_questions: [string]
```

A forwarded result requires a sealed charge receipt. A verified credential
without a sealed receipt is not enough.

## Worked example

A caller asks for `search.paid`. Provider policy prices the call at `1.25 USD`,
accepts `stripe`, and requires receipt-before-forward. `charge` emits a
challenge, verifies the returned Stripe credential against that exact challenge,
seals `receipt:charge:stripe:paid-search-001`, then forwards the operation. The
result is `decision: sealed`.

If the returned credential is for `mpp` while the challenge accepted `stripe`,
the skill returns `decision: escalated`; it does not reinterpret the credential
or forward the request.

## Inputs

- `mcp_tool_call` (required): inbound MCP operation request.
- `provider_policy` (required): provider price and settlement family policy.
- `returned_credential` (required): caller-returned payment credential.
- `parent_payment_authority` (optional): parent payment authority term or ref.
- `verify_capability_ref` (required): single-use verification capability
  reference.
- `idempotency_seed` (optional): stable challenge idempotency seed.
