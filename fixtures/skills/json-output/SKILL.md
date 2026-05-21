---
name: json-output
description: Echo all resolved inputs as a JSON object through the cli-tool adapter.
source:
  type: cli-tool
  command: sh
  args:
    - ./run.sh
  timeout_seconds: 10
inputs: {}
---

Emit the resolved inputs as structured JSON.
