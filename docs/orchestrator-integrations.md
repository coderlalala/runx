# Orchestrator Integrations

runx can be a governed step inside a workflow orchestrator. The orchestrator
owns the trigger, schedule, branching, retries, and human workflow. runx owns the
governed effect, local credential delivery, policy enforcement, and receipt.

This document covers what works today without a hosted run-skill API.

## What Works Today

### Self-hosted n8n can call local runx

A self-hosted orchestrator running on the same host or trusted local network can
invoke the local CLI as a command step:

```bash
runx skill ./skills/lead-enrichment --json --company-domain example.com
```

The command step should capture stdout as the runx result. Use ordinary runx
input flags for values from earlier workflow steps. For JSON values, pass a JSON
literal so the CLI parses it as structured input:

```bash
runx skill ./examples/orchestrator-webhooks --json \
  --event-id n8n-demo-001 \
  --source n8n \
  --payload '{"account_id":"acct_123","action":"sync"}'
```

For webhook tokens or provider credentials, deliver secrets through the existing
local credential path:

```bash
RUNX_N8N_WEBHOOK_TOKEN=replace-me \
  runx skill ./examples/orchestrator-webhooks --json \
  --credential orchestrator:bearer:RUNX_N8N_WEBHOOK_TOKEN:workflow.invoke \
  --secret-env RUNX_N8N_WEBHOOK_TOKEN \
  --event-id n8n-demo-001 \
  --source n8n \
  --payload '{"account_id":"acct_123","action":"sync"}'
```

The token is read from the named environment variable for this run; it is not
stored in the manifest or passed as an argument value.

### Self-hosted n8n can consume local MCP HTTP

For a local MCP client, run:

```bash
runx mcp serve --http-listen 127.0.0.1:8787
```

The server generates a bearer token for that process. Keep the listener on
loopback unless you have explicitly decided to expose it. The MCP HTTP token is
useful for local wiring; it is not a SaaS-grade multi-tenant credential.

### runx can call workflow webhooks

The governed HTTP front can POST to an orchestrator webhook. That lets a skill
reuse an existing n8n or Zapier workflow as an outbound effect while still
keeping the upstream provider secret and receipt in runx.

Templates live in:

- `oss/examples/orchestrator-webhooks/templates/n8n-webhook.manifest.json`
- `oss/examples/orchestrator-webhooks/templates/zapier-webhook.manifest.json`

Copy a template into a tool catalog, replace the example URL with your real
workflow webhook URL, and keep the auth header as a `${secret:NAME}` reference.
Do not paste the token into the manifest.

## Non-goals

- No hosted run-skill API exists in this slice.
- Zapier, Make, and n8n Cloud cannot call a local shell or localhost runx process
  unless you add your own network bridge outside runx.
- These webhook templates are outbound effects from runx to an orchestrator.
  They are not inbound triggers that start a run.
- External pause/resume is not implemented here. A workflow step should use
  non-pausing skills or branch on a terminal needs-approval result.
- runx does not become a workflow scheduler or branching engine.

## Operational Notes

- Use public HTTPS webhook URLs for cloud orchestrator webhooks.
- Leave `allow_private_network` unset for cloud webhook calls. Private-network
  access is a local-fixture exception, not the default integration posture.
- Include an idempotency key or event id in webhook payloads so the downstream
  workflow can deduplicate retries.
- Store the sealed runx receipt from stdout or the configured receipt directory
  as the orchestrator step evidence.
