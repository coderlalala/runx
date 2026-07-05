---
name: secret-catcher
description: Scan a code diff for credential-like spans without echoing raw secrets.
source:
  type: cli-tool
  command: node
  args:
    - ./run.mjs
  timeout_seconds: 10
  sandbox:
    profile: readonly
    cwd_policy: skill-directory
inputs:
  diff:
    type: string
    required: true
    description: Unified or plain code diff text to scan.
  scan_context:
    type: string
    required: false
    description: Optional JSON or free-form context about the source being scanned.
runx:
  artifacts:
    wrap_as: secret_catcher_result
  input_resolution:
    required:
      - diff
---

Secret Catcher scans a supplied diff for credential-like spans and returns
redacted, checkable findings. It never edits the repository and never prints raw
secret values.

The output is JSON with:

- `findings`: credential-like findings with type, file, line, column, severity,
  diff side, and a SHA-256 evidence hash instead of the matched secret.
- `redaction_proposal`: replacement guidance that names the affected line and
  recommends `[REDACTED:<type>]` placeholders without exposing the original
  value.
- `block`: `true` when at least one high-confidence credential-like span is
  found, otherwise `false`.

Use it as a pre-merge PR check when you need evidence that is safe to paste into
an issue, receipt, or review packet.
