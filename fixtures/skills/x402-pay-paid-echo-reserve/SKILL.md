---
name: pay-reserve
description: Deterministically reserve the x402 paid echo fixture spend.
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

Emit a deterministic reserved payment authority for the x402 paid echo fixture.
