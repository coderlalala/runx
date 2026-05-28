import { createHash } from "node:crypto";

export function sanitizePublicMarkdown(value: unknown): string | undefined {
  if (value === undefined || value === null) {
    return undefined;
  }
  return String(value)
    .replace(/\b([A-Z][A-Z0-9_]*(?:TOKEN|SECRET|PASSWORD|API[_-]?KEY|MATERIAL[_-]?REF)[A-Z0-9_]*)=("[^"]*"|'[^']*'|\S+)/gi, "$1=[secret]")
    .replace(/\b((?:bearer|authorization|token|secret|password|api[_-]?key|material[_-]?ref|materialRef)\s*[:=]\s*)(["']?)[^\s`),;]+/gi, "$1[secret]")
    .replace(/\b((?:bearer|authorization)\s+)[A-Za-z0-9._:-]{6,}\b/gi, "$1[secret]")
    .replace(/\b(gh[pousr]_[A-Za-z0-9_]{20,}|xox[baprs]-[A-Za-z0-9-]{20,})\b/g, "[secret]")
    .replace(/\bsk-(?:proj-)?[A-Za-z0-9_-]{16,}\b/g, "[secret]")
    .replace(/\b[A-Za-z0-9]+(?:[-_](?:secret|token|password|api[-_]?key))+[A-Za-z0-9_-]*\b(?!\s*=)/gi, "[secret]")
    .replace(/\b([A-Z][A-Z0-9_]*=)(?:\/Users|\/home|\/var|\/private|\/tmp|[A-Za-z]:\\)[^\s`)]+/g, "$1[local-path]")
    .replace(/(?:\/Users|\/home|\/var|\/private|\/tmp)\/[^\s`)]+/g, "[local-path]")
    .replace(/[A-Za-z]:\\[^\s`)]+/g, "[local-path]");
}

export function summarizePublicHandoffMarkdown(value: unknown): string | undefined {
  const sanitized = sanitizePublicMarkdown(value)?.trim();
  if (!sanitized) {
    return undefined;
  }
  if (containsRedactionMarker(sanitized)) {
    return "Detailed handoff omitted from public markdown because it contains local paths or sensitive runtime details.";
  }
  return limitLines(sanitized, 18);
}

export function renderIssueToPrReviewerMarkdown(input: Record<string, unknown>): string {
  const title = clean(input.title) ?? clean(input.taskId) ?? "Issue-to-PR change";
  const sourceTitle = clean(input.sourceTitle);
  const sourceLocator = clean(input.sourceLocator);
  const sourceSummary = clean(input.sourceSummary);
  const handoff = summarizePublicHandoffMarkdown(input.handoffMarkdown);
  const changedFiles = Array.isArray(input.changedFiles)
    ? input.changedFiles.map((entry) => clean(entry)).filter(Boolean)
    : [];

  return [
    `# ${title}`,
    "",
    "## Source Thread",
    sourceTitle ? `- Source: ${sourceTitle}` : undefined,
    sourceLocator ? `- Locator: ${sourceLocator}` : undefined,
    "",
    "## Source Context",
    sourceSummary ?? "Source context was captured in the governed runx receipt.",
    "",
    "## Changed Files",
    changedFiles.length > 0 ? changedFiles.map((file) => `- \`${file}\``).join("\n") : "- No file list reported.",
    "",
    "## Checks",
    bullet("Governance", clean(input.governanceStatus)),
    bullet("Build", clean(input.checkStatus)),
    bullet("Passed", clean(input.buildPassed)),
    bullet("Failed", clean(input.buildFailed)),
    bullet("Review", clean(input.reviewVerdict)),
    bullet("Blocking findings", clean(input.blockingCount)),
    bullet("Non-blocking findings", clean(input.nonBlockingCount)),
    clean(input.qualityGateSummary) ? `- Quality gate: ${clean(input.qualityGateSummary)}` : undefined,
    "",
    "## scafld Handoff",
    handoff ?? "No public handoff summary was available.",
    "",
    "## Human Merge Gate",
    "Review the diff and merge only after the generated change is acceptable. runx does not auto-merge generated PRs.",
  ].filter((line) => line !== undefined).join("\n");
}

export function renderFeedStoryMarkdown(story: Record<string, unknown>): string {
  const title = clean(story.title) ?? "Issue-to-PR story";
  const nextAction = clean(story.next_action);
  const milestones = Array.isArray(story.milestones) ? story.milestones : [];
  const sections = milestones
    .filter((milestone): milestone is Record<string, unknown> => isRecord(milestone))
    .map((milestone) => renderMilestone(milestone));
  return [
    `# ${title}`,
    "",
    nextAction ? `Next: ${nextAction}` : undefined,
    "",
    ...sections,
  ].filter((line) => line !== undefined).join("\n").trimEnd();
}

export function buildFeedStoryOutboxEntry(input: {
  taskId?: unknown;
  threadLocator?: unknown;
  title?: unknown;
  milestone?: Record<string, unknown>;
  bodyMarkdown?: unknown;
  updatedAt?: unknown;
}): Record<string, unknown> {
  const taskId = clean(input.taskId) ?? "unknown-task";
  const threadLocator = clean(input.threadLocator);
  const milestone = isRecord(input.milestone) ? input.milestone : {};
  const milestoneKind = clean(milestone.kind) ?? "update";
  const bodyMarkdown = clean(input.bodyMarkdown) ?? "";
  const receiptHash = createHash("sha256")
    .update(JSON.stringify({
      taskId,
      threadLocator,
      milestoneKind,
      bodyMarkdown,
      updatedAt: clean(input.updatedAt),
    }))
    .digest("hex")
    .slice(0, 20);

  return {
    entry_id: `message:${taskId}:${milestoneKind}`,
    kind: "message",
    status: "proposed",
    thread_locator: threadLocator,
    title: clean(input.title) ?? "Issue-to-PR story",
    metadata: {
      schema_version: "runx.outbox-entry.feed-entry.v1",
      workflow: "issue-to-pr",
      milestone_kind: milestoneKind,
      outbox_receipt_id: `feed:issue-to-pr:${taskId}:${milestoneKind}:${receiptHash}`,
      source_thread: {
        required: true,
        publish_mode: "reply",
        missing_behavior: "fail_closed",
        thread_locator: threadLocator,
      },
      body_markdown: bodyMarkdown,
    },
  };
}

function renderMilestone(milestone: Record<string, unknown>): string {
  const kind = titleize(clean(milestone.kind) ?? "update");
  const status = clean(milestone.status);
  const summary = clean(milestone.summary);
  const details = Array.isArray(milestone.details)
    ? milestone.details.map((detail) => clean(detail)).filter(Boolean)
    : [];
  return [
    `## ${kind}${status ? ` (${status})` : ""}`,
    summary,
    ...details.map((detail) => `- ${detail}`),
    "",
  ].filter((line) => line !== undefined).join("\n");
}

function clean(value: unknown): string | undefined {
  const sanitized = sanitizePublicMarkdown(value)?.trim();
  return sanitized || undefined;
}

function bullet(label: string, value: string | undefined): string | undefined {
  return value ? `- ${label}: ${value}` : undefined;
}

function containsRedactionMarker(value: string): boolean {
  return value.includes("[local-path]") || value.includes("[secret]");
}

function limitLines(value: string, maxLines: number): string {
  const lines = value.split(/\r?\n/);
  if (lines.length <= maxLines) {
    return value;
  }
  return `${lines.slice(0, maxLines).join("\n")}\n...`;
}

function titleize(value: string): string {
  return value
    .split(/[_-]+/)
    .map((part) => part ? `${part[0].toUpperCase()}${part.slice(1)}` : part)
    .join(" ");
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}
