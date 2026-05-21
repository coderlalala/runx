---
name: echo
description: Echo a message through the cli-tool adapter.
source:
  type: cli-tool
  command: sh
  args:
    - ./run.sh
  timeout_seconds: 10
  sandbox:
    profile: readonly
    cwd_policy: skill-directory
inputs:
  message:
    type: string
    required: true
    description: Message to echo
runx:
  input_resolution:
    required:
      - message
---

Echo the provided message.
