---
name: paid-echo
description: Echo only when given scoped payment capability and receipt proof refs.
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

Fail closed if raw rail or session material reaches the paid echo tool.
