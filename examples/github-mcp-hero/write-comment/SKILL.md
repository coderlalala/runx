---
name: github-mcp-write-comment
description: Write a deterministic GitHub issue comment through the fixture MCP server.
source:
  type: mcp
  server:
    command: node
    args:
      - ../../../fixtures/runtime/adapters/mcp/github-stdio-server.mjs
  tool: github_issue_comment
  arguments:
    repository: "{{repository}}"
    number: "{{issue_number}}"
    body: "{{body}}"
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
  body:
    type: string
    required: true
    description: Comment body.
runx:
  input_resolution:
    required:
      - repository
      - issue_number
      - body
---

Write a GitHub issue comment through the deterministic MCP fixture.
