---
name: charge-price
description: Price an inbound provider-side paid tool call without collecting payment.
runx:
  category: payments
---

# Charge Price

Turn an inbound MCP operation plus provider policy into a charge price packet
and requested provider-side payment authority.

This skill is the first read-only step in a provider charge flow. It classifies
the requested operation, selects the smallest acceptable price and settlement
family set, and records the policy evidence that a later challenge step can
expose to the caller. It asks for authority; it does not exercise authority.

## What this skill does

1. **Classify the inbound operation.** Identify the provider, tool, method,
   account realm, counterparty, and requested resource from the MCP tool call.
2. **Resolve the pricing policy.** Match the operation to the provider policy
   rule that sets amount, currency, expiry, settlement families, and any
   counterparty constraints.
3. **Propose the narrow authority.** Emit the exact provider-side payment
   authority that the challenge and verify steps may later use: amount cap,
   currency, settlement families, operation id, counterparty, expiry, and
   idempotency material.
4. **Record evidence.** Return the policy facts and input refs that justify the
   price so a challenge, receipt, and later dispute can explain where the charge
   came from.
5. **Stop on ambiguity.** If price, currency, operation, counterparty, expiry, or
   settlement family cannot be traced, return `needs_agent`.

It does not issue a payment challenge, verify a returned credential, collect or
store rail credentials, forward the upstream tool call, or decide a dispute.

## When to use this skill

- A hosted provider is about to expose a payable MCP operation and needs a
  deterministic price packet before returning `effect_required`.
- A registry or operator wants to preview the authority a provider would request
  for a particular paid tool call.
- A payment challenge skill needs normalized price evidence and a requested
  payment authority.

## When not to use this skill

- To verify that a caller has paid. Use `charge-verify` after a challenge has
  been issued and a credential has been returned.
- To calculate a refund, reservation, or outbound spend. Those are different
  payment lifecycle skills with different authority.
- To infer prices from model confidence or caller willingness to pay. Price must
  come from provider policy or an explicit operator override.
- To widen settlement families because one verifier is easier to run. The skill
  may only return families allowed by policy.

## Procedure

1. Validate that `mcp_tool_call` and `provider_policy` are present.
2. Extract the stable operation identity: provider, tool name, arguments that
   affect price, caller/counterparty when known, and realm.
3. Match the operation to exactly one provider policy rule. If zero or multiple
   rules match, return `needs_agent` with the conflicting rule ids.
4. Normalize amount, currency, expiry, and settlement families. Preserve policy
   precision; do not round up or down unless the policy states the rule.
5. Bind idempotency. Use `idempotency_seed` when supplied; otherwise derive only
   from stable, non-secret operation and policy material.
6. Intersect any `parent_payment_authority` with the provider policy. If the
   parent grant is narrower, keep the narrower bound. If the intersection cannot
   cover the price, return `needs_agent`.
7. Emit `charge_price`, `requested_payment_authority`, `price_evidence`,
   `policy_metadata`, and `open_questions`.
8. Ensure the output contains no raw secrets, bearer tokens, private keys, or raw
   payment credentials.

## Edge cases and stop conditions

- **No matching policy rule:** return `needs_agent`; do not invent a default
  price.
- **Multiple matching rules:** return `needs_agent` with the rule ids and the
  ambiguous fields.
- **Currency mismatch:** return `needs_agent` unless the policy explicitly
  permits conversion and names the conversion source.
- **Parent authority too narrow:** return `needs_agent`; a price packet cannot
  widen a parent grant.
- **Counterparty unknown:** return `needs_agent` when policy requires a
  counterparty-bound charge.
- **Replay-prone idempotency:** return `needs_agent` if idempotency material is
  missing or derived from mutable inputs.
- **Secret material present in inputs:** redact from evidence and report a
  blocker; the price artifact must reference secret material only by hash or
  policy ref.

## Output schema (`charge_price_artifact`)

```yaml
decision: ready | needs_agent
charge_price:
  amount: string
  currency: string
  operation: string
  counterparty: string | null
  settlement_families: [string]
  expires_at: string
  idempotency_key: string
requested_payment_authority:
  family: payment
  max_amount: string
  currency: string
  settlement_families: [string]
  operation: string
  counterparty: string | null
  expires_at: string
price_evidence:
  policy_rule_id: string
  input_refs: [string]
  facts: [string]
policy_metadata:
  provider: string
  realm: string
  labels: [string]
open_questions: [string]
```

A `ready` decision requires an empty `open_questions` list.

## Worked example

An inbound MCP call asks the provider to run `crm.enrich_lead` for account
`acct_test_123`. Provider policy rule `lead-enrichment.basic` prices that
operation at `0.08 USD`, accepts `stripe-spt` and `x402`, expires in five
minutes, and requires the charge to be account-bound. The skill emits a
`charge_price` for `0.08 USD`, requests a payment authority capped at exactly
`0.08 USD` for `crm.enrich_lead`, cites `lead-enrichment.basic`, and returns
`decision: ready`.

If the same call has no account id and the rule requires account binding, the
skill returns `decision: needs_agent` with an open question for the missing
counterparty. It does not fall back to an unbound charge.

## Inputs

- `mcp_tool_call` (required): inbound MCP operation request.
- `provider_policy` (required): pricing policy and settlement family allowlist.
- `parent_payment_authority` (optional): parent payment authority term or ref.
- `realm` (optional): provider realm such as `local`, `test`, or `prod`.
- `idempotency_seed` (optional): stable material for challenge idempotency.
