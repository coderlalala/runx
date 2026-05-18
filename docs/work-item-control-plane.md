# Work Item Control Plane

runx issue automation uses one portable work-item packet as the source of
truth for issue intake through outcome observation. Source adapters and hosted
queues may project the packet, but they do not replace it.

## Ownership

OSS owns:

- `runx.work_item.v1`
- `runx.evidence_bundle.v1`
- local skill inputs and outputs
- source-thread and outbox packets
- receipts and scafld-backed issue-to-PR execution

Cloud owns:

- hosted queue storage
- approval inboxes
- authenticated source adapters
- org routing and operational APIs

Consuming repos own:

- Slack, Sentry, GitHub, or file source filters
- target repo policy
- owner suggestion rules
- source-thread notification policy

## Lifecycle

The state machine is finite and explicit:

1. `intake_received` records the admitted source event.
2. `dedupe_pending` computes a source-locator and content fingerprint.
3. `duplicate_candidate` exposes possible duplicates without silently merging.
4. `triage_pending` classifies severity, confidence, and action.
5. `planning_ready` hands the packet to `work-plan`.
6. `build_ready` hands the packet to `issue-to-pr`.
7. `review_ready` records the scafld review boundary.
8. `pr_ready` records the generated provider PR.
9. `merge_gate` waits for a human reviewer to merge or close.
10. `outcome_merged`, `outcome_closed`, or `outcome_rejected` records observed provider outcome.
11. `blocked` records missing evidence, policy denial, or failed validation.

`merge_gate` is always human controlled. runx may create PRs and post source
thread updates, but it must not auto-merge generated PRs.

Hosted queues enforce lifecycle movement for already-known work items. A first
observation may begin at any valid checkpoint because adapters can replay
evidence from an existing source thread, but later updates for the same work
item must follow the contract transition graph:

- `intake_received` -> `dedupe_pending`, `triage_pending`, `blocked`, `outcome_closed`
- `dedupe_pending` -> `duplicate_candidate`, `triage_pending`, `blocked`, `outcome_closed`
- `duplicate_candidate` -> `outcome_closed`, `outcome_rejected`, `blocked`
- `triage_pending` -> `planning_ready`, `build_ready`, `outcome_closed`, `outcome_rejected`, `blocked`
- `planning_ready` -> `build_ready`, `outcome_closed`, `outcome_rejected`, `blocked`
- `build_ready` -> `review_ready`, `outcome_closed`, `outcome_rejected`, `blocked`
- `review_ready` -> `pr_ready`, `outcome_closed`, `outcome_rejected`, `blocked`
- `pr_ready` -> `merge_gate`, `outcome_closed`, `outcome_rejected`, `blocked`
- `merge_gate` -> `outcome_merged`, `outcome_closed`, `outcome_rejected`, `blocked`
- `blocked` -> any non-terminal recovery checkpoint or closed/rejected outcome

Terminal outcomes do not reopen. Submit a new work item when new source
evidence requires new automation.

## Dedupe

Dedupe never depends on a global keyword such as `BUG`. A work item carries:

- `source_locator`: stable locator for the source event or thread
- `provider_event_id`: provider-native id when available
- `fingerprint`: SHA-256 content or issue fingerprint
- `candidate_work_item_ids`: reviewable possible duplicates

Adapters may suppress duplicate mutation after they find an exact already-open
work item, but candidate duplicates remain visible to developers.

## Evidence And Hydration

The source event is not always enough evidence for a build lane. Slack alert
cards, Sentry notifications, and truncated support summaries often point at
richer provider context. Adapters should normalize that context into
`runx.evidence_bundle.v1` before triage.

The bundle records:

- `hydration.status`: `complete`, `unavailable`, or `needed`
- provider-neutral sources such as Slack thread, Sentry event, GitHub issue,
  log, stacktrace, deployment, or user report
- redaction status and summary
- a bounded reviewer-safe summary

Runx core does not know product channel names, Sentry projects, owner maps, or
credential material. Those stay in adapters and hosted policy. When hydration
is `needed`, issue intake should not proceed to a mutation lane until the
adapter supplies the missing evidence or a human explicitly accepts the risk in
the source thread.

## Source Thread Story

Source-thread comments should tell the work story at gate moments:

- intake accepted or rejected
- triage result and chosen lane
- issue created or linked
- PR created or refreshed
- review result
- human merge gate
- observed merged or closed outcome

Receipts retain low-level evidence. Source threads should not become raw run
logs, command dumps, local absolute paths, or repeated retry noise.

## PR Review And Fix-Up

Reviewing or repairing an open PR is work on the same work item, not a fresh
source thread. Reusable lanes should preserve the original source locator,
evidence bundle, outbox PR entry, and merge gate.

Recommended split:

- `pr-review`: consumes the PR diff, checks, review comments, source evidence,
  and scafld review state; emits a reviewer-safe packet and one idempotent PR or
  source-thread comment.
- `pr-fix-up`: consumes an actionable review packet and the existing PR outbox
  entry; applies bounded follow-up changes without creating a duplicate PR.
- `merge-assist`: consumes provider observations after human review; summarizes
  checks, deployment, verification, and remaining risk, then waits for a human
  merge or records an observed terminal outcome.

None of these lanes changes the merge authority rule: `merge_gate` is a human
checkpoint. A runner may prepare context, apply review-requested fixes, and
observe outcomes, but it must not merge the generated PR.

## Developer Status

Developer inboxes should group by work item and prioritize actionable gates:

- needs triage
- needs evidence
- ready to plan
- ready to build
- review failed
- PR ready
- waiting for human merge
- merged or closed

GitHub Projects, Slack posts, and dashboards are projections of this state, not
the source of truth.
