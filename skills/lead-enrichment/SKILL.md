---
name: lead-enrichment
description: Enrich a lead from supplied account signals and produce a reviewable outreach recommendation.
runx:
  category: growth
---

# Lead Enrichment

Turn supplied lead, account, and engagement signals into a reviewable enrichment
packet and outreach recommendation.

This skill does not scrape, email, or mutate CRM records. It works over context
that a consuming product has already hydrated through governed provider fronts.
The output is a human-reviewed recommendation, not permission to send.

## Quality Profile

- Purpose: decide whether a lead is worth action and what the next action should
  be.
- Audience: growth operators, sales engineers, and lifecycle owners.
- Artifact contract: enriched profile, evidence-backed fit assessment,
  recommended action, and risk flags.
- Evidence bar: every enrichment claim cites a supplied signal.
- Voice bar: account note, not marketing copy.
- Strategic bar: route high-fit leads to the narrowest useful follow-up.
- Stop conditions: return `needs_more_evidence` when signals are too thin and
  `do_not_contact` when constraints or signals make outreach inappropriate.

## Inputs

- `lead` (required): lead identity and known account fields.
- `signals` (required): engagement, product, CRM, or firmographic signals.
- `constraints` (optional): allowed channels, region, opt-in, or do-not-contact
  flags.

