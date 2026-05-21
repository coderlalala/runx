---
name: payment-fulfill
description: Deterministically fulfill an approved payment through the fixture rail.
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

Use this fixture only to prove the payment approval graph seals with rail proof.
