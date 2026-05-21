---
name: pay-quote
description: Deterministically quote the x402 paid echo fixture payment.
source:
  type: cli-tool
  command: sh
  args:
    - ./run.sh
  timeout_seconds: 10
  sandbox:
    profile: readonly
    cwd_policy: skill-directory
inputs: {}
---

Emit a deterministic mock payment quote for the x402 paid echo fixture.
