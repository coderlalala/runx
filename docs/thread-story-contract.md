# Thread Story Contract

The thread story is the reviewer-facing projection for source-thread driven
work. It is derived from receipts, scafld state, outbox entries, and provider
observations; it is not the source of truth.

## Current Shape

The shared implementation lives in `@runxhq/core/knowledge` as typed helpers:

- `validateWorkItemStory`
- `validateWorkItemStoryMilestone`
- `renderWorkItemStoryMarkdown`
- `buildWorkItemStoryOutboxEntry`

The `outbox.build_work_item_story` tool uses those helpers to produce:

- `story.schema`: `runx.work-item-story.v1`
- `story.data.thread_locator`
- `story.data.title`
- `story.data.next_action`
- `story.data.milestones`
- `outbox_entry.metadata.schema_version`: `runx.outbox-entry.work-item-story.v1`
- `outbox_entry.metadata.workflow`
- `outbox_entry.metadata.milestone_kind`
- `outbox_entry.metadata.body_markdown`

The canonical milestone kinds for issue-to-PR style flows are:

- `intake`
- `triage`
- `spec`
- `build`
- `review`
- `pull_request`
- `merge_gate`
- `outcome`

## Schema Decision

Keep the story as an internal typed helper plus stable outbox metadata for this
cut. Do not publish a standalone `@runxhq/contracts` schema yet.

That boundary is deliberate:

- current consumers are first-party tools and tests inside OSS runx
- the provider surface is the outbox entry, not a separate registry packet
- external adapters can already consume the rendered message and metadata
- delaying a public schema avoids freezing fields before live dogfood proves the
  final reviewer shape

Promote the packet to a public contract only when a non-tool consumer needs to
validate or exchange the story independently of the outbox entry. That promotion
must be a hard cut with call sites, docs, and tests updated in one change.

## Required Reviewer Sections

Public story markdown should summarize durable gates:

- source thread and request
- hydrated evidence status when adapter context was needed
- triage decision
- governed scafld task
- build result
- review verdict and finding counts
- PR link, branch, and base when known
- human merge gate
- observed merged or closed outcome

It should not publish low-level run events, full command dumps, raw provider
payloads, local absolute paths, token-shaped values, or consuming-repo policy
such as Slack channel names, Sentry project ids, or owner maps.

## Non-goals

- This contract does not admit Slack, Sentry, or support-channel messages.
- This contract does not decide whether an issue deserves a PR.
- This contract does not merge PRs.
- This contract does not replace receipts, ledgers, or scafld status.
- This contract does not encode Nitrosend, Aster, or runx.ai hosted policy.
