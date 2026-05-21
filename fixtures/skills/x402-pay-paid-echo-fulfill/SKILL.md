---
name: pay-fulfill-rail
description: Deterministically fulfill the x402 paid echo fixture rail spend.
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

Emit a deterministic mock rail packet for the x402 paid echo fixture.
