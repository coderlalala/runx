---
name: github-mcp-merge-pr
description: Merge a deterministic GitHub PR through the fixture MCP server.
source:
  type: mcp
  server:
    command: node
    args:
      - ../../../fixtures/runtime/adapters/mcp/github-stdio-server.mjs
  tool: github_pr_merge
  arguments:
    repository: "{{repository}}"
    number: "{{pr_number}}"
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
runx:
  input_resolution:
    required:
      - repository
      - pr_number
---

Merge a PR through the deterministic MCP fixture.
