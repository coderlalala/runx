---
name: github-mcp-pr-review-note
description: Add a deterministic GitHub PR review note through the fixture MCP server.
source:
  type: mcp
  server:
    command: node
    args:
      - ../../../fixtures/runtime/adapters/mcp/github-stdio-server.mjs
  tool: github_pr_review_note
  arguments:
    repository: "{{repository}}"
    number: "{{pr_number}}"
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
  pr_number:
    type: string
    required: true
    description: Pull request number.
  body:
    type: string
    required: true
    description: Review note body.
runx:
  input_resolution:
    required:
      - repository
      - pr_number
      - body
---

Add a PR review note through the deterministic MCP fixture.
