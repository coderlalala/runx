---
name: skill-level-retry
description: Echo with retry metadata declared on the skill.
source:
  type: cli-tool
  command: sh
  args:
    - ./run.sh
  timeout_seconds: 10
retry:
  max_attempts: 2
inputs:
  message:
    type: string
    required: true
---

Echo with skill-level retry metadata.
