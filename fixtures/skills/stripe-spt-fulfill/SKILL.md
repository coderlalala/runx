---
name: pay-fulfill-rail
description: Deterministically fulfill the Stripe SPT fixture rail spend.
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

Emit a deterministic fulfilled Stripe SPT rail packet.
