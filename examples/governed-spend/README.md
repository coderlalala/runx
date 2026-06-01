# governed-spend spend (demo)

A prompt-injected agent tries to overspend; runx refuses before any rail is
touched, identically across x402, MPP, and Stripe, and signs a receipt for every
outcome. The agent never holds the authority, so hijacking it changes nothing.

## Run it

```bash
./run.sh
```

No keys, no signup, no network. Needs the `runx` binary (a build under
`crates/target/`, `runx` on `PATH`, or `RUNX_BIN=/path/to/runx`) and `python3`.

The script prints three steps:

1. A bounded authority pays on `x402`, `mpp`, and `stripe`, each sealing a receipt.
2. The reserved child authority is capped at 100 minor; an injected agent tries to
   fulfill 125; runx blocks at the rail gate before any provider call.
3. A sealed refusal receipt: `disposition: refused`, `reason_code: cap_exceeded`,
   `rail_call_performed: false`.

## Over MCP

```bash
./mcp.sh
```

Serves `x402-pay`, `mpp-pay`, `stripe-pay`, and `overspend-refused` as MCP tools.
An agent calls a skill and gets the sealed receipt id, or the refusal, in one
round-trip.

## Recorded payments demo

```bash
node ../../scripts/payments-demo.mjs --record --receipt-dir /tmp/runx-payments-demo
node verify.mjs /tmp/runx-payments-demo/payments-demo-paid.receipt.json
node verify.mjs /tmp/runx-payments-demo/payments-demo-refusal.receipt.json
```

With `ANTHROPIC_API_KEY` and `RUNX_X402_SIGNER` present, the script records an
operator-keyed testnet transcript. Without those keys it writes a deterministic
mock transcript. In both modes the offline receipts are real signed artifacts:
one scoped x402 spend, then one over-run-cap refusal before money moves.

## Tweak it

In [`skills/overspend-refused/X.yaml`](skills/overspend-refused/X.yaml), raise the
reserved child authority's `max_per_call_minor` from `100` to `125` and re-run: the
same agent now fulfills, because the spend is within its authority.

## What is real

The kernel, the quote/reserve/fulfill graph, the fail-closed authority subset proof,
the authority admission that refuses before any rail, and signed receipts are real and ship today. The
rails run through deterministic test supervisors, not live providers, so nothing
settles on-chain. The refusal needs no rail, which is the point.
