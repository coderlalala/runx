---
name: github-mcp-read-issue
description: Read a deterministic GitHub issue through the fixture MCP server.
source:
  type: mcp
  server:
    command: node
    args:
      - ../../../fixtures/runtime/adapters/mcp/github-stdio-server.mjs
  tool: github_issue_read
  arguments:
    repository: "{{repository}}"
    number: "{{issue_number}}"
  timeout_seconds: 15
  sandbox:
    profile: network
    cwd_policy: skill-directory
inputs:
  repository:
    type: string
    required: true
    description: Repository slug.
  issue_number:
    type: string
    required: true
    description: Issue number.
runx:
  input_resolution:
    required:
      - repository
      - issue_number
---

Read a GitHub issue snapshot through the deterministic MCP fixture.
