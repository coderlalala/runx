# Orchestrator Webhook Templates

This example contains outbound webhook templates for workflow orchestrators.
They are templates, not live endpoints. Replace each example URL before use.

Use this when runx should perform governance and credential delivery, then call
an existing orchestrator workflow as the effect. The orchestrator workflow may
branch, notify, or fan out after receiving the webhook payload.

## Files

- `templates/n8n-webhook.manifest.json`: POST template for an n8n webhook.
- `templates/zapier-webhook.manifest.json`: POST template for a Zapier Catch Hook.
- `X.yaml`: a graph example wired to the n8n template name.

## Secret Delivery

The templates use `${secret:RUNX_N8N_WEBHOOK_TOKEN}` and
`${secret:RUNX_ZAPIER_WEBHOOK_TOKEN}` in HTTP headers. Deliver those with the
existing local credential flags:

```bash
RUNX_N8N_WEBHOOK_TOKEN=replace-me \
  runx skill ./examples/orchestrator-webhooks --json \
  --credential orchestrator:bearer:RUNX_N8N_WEBHOOK_TOKEN:workflow.invoke \
  --secret-env RUNX_N8N_WEBHOOK_TOKEN \
  --event-id n8n-demo-001 \
  --source runx \
  --payload '{"hello":"workflow"}'
```

Do not paste bearer tokens into the manifest file.

## Boundaries

These templates do not add a hosted runx API, an inbound webhook listener, or
external resume. They are only outbound HTTP effects from a governed runx run.
