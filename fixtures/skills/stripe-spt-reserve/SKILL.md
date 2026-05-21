---
name: pay-reserve
description: Deterministically reserve the Stripe SPT fixture spend.
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

Emit a deterministic Stripe SPT reserved payment authority.
