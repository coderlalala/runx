---
name: failing
description: Deterministically fail through the cli-tool adapter.
source:
  type: cli-tool
  command: sh
  args:
    - ./run.sh
  timeout_seconds: 10
inputs: {}
---

Fail deterministically for quorum and retry tests.
