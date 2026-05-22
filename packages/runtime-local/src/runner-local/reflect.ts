import path from "node:path";

import {
  appendLedgerEntries,
  createRunEventEntry,
  readLedgerEntries,
  SYSTEM_ARTIFACT_TYPES,
  type ArtifactEnvelope,
} from "@runxhq/core/artifacts";
import { createFileKnowledgeStore } from "@runxhq/core/knowledge";
import { errorMessage } from "@runxhq/core/util";
import { resolveRunxKnowledgeDir } from "@runxhq/core/config";

import type { Caller, RunLocalGraphResult, RunLocalSkillResult } from "./index.js";
import {
  runnerReceiptCategory,
  runnerReceiptCompletedAt,
  runnerReceiptGraphSteps,
  runnerReceiptSource,
  runnerReceiptStatus,
} from "./graph-governance.js";

export type PostRunReflectPolicy = "auto" | "always" | "never";

type ReflectReceipt = NonNullable<
  | Extract<RunLocalSkillResult, { readonly status: "sealed" | "failure" }>["receipt"]
  | Extract<RunLocalGraphResult, { readonly status: "sealed" | "failure" | "escalated" }>["receipt"]
  | Extract<RunLocalSkillResult, { readonly status: "policy_denied" }>["receipt"]
  | Extract<RunLocalGraphResult, { readonly status: "policy_denied" }>["receipt"]
>;

export interface ReflectProjectionOptions {
  readonly caller: Caller;
  readonly receipt: ReflectReceipt;
  readonly receiptDir: string;
  readonly runId: string;
  readonly skillName: string;
  readonly knowledgeDir?: string;
  readonly env?: NodeJS.ProcessEnv;
  readonly selectedRunnerName?: string;
  readonly postRunReflectPolicy?: PostRunReflectPolicy;
  readonly involvedAgentMediatedWork: boolean;
}

interface LocalReflectProjection {
  readonly schema_version: "runx.reflect.v1";
  readonly skill_ref: string;
  readonly receipt_id: string;
  readonly run_id: string;
  readonly receipt_category: string;
  readonly status: "sealed" | "failure";
  readonly selected_runner?: string;
  readonly policy: PostRunReflectPolicy;
  readonly mediation: "agentic" | "deterministic";
  readonly summary: string;
  readonly signals: readonly string[];
  readonly ledger: {
    readonly event_kinds: readonly string[];
    readonly artifact_count: number;
    readonly artifact_types: readonly string[];
  };
  readonly step_summary?: {
    readonly total_steps: number;
    readonly successful_steps: number;
    readonly failed_steps: number;
    readonly runner_types: readonly string[];
  };
  readonly projected_at: string;
}

export async function projectReflectIfEnabled(options: ReflectProjectionOptions): Promise<void> {
  const policy = options.postRunReflectPolicy ?? "never";
  if (!shouldProjectReflect(policy, options.involvedAgentMediatedWork)) {
    return;
  }

  const knowledgeDir = resolveOptionalKnowledgeDir(options);
  if (!knowledgeDir) {
    return;
  }

  const projectedAt = runnerReceiptCompletedAt(options.receipt) ?? new Date().toISOString();

  try {
    const ledgerEntries = await readLedgerEntries(options.receiptDir, options.runId);
    const reflectProjection = buildReflectProjection({
      receipt: options.receipt,
      runId: options.runId,
      skillName: options.skillName,
      selectedRunnerName: options.selectedRunnerName,
      policy,
      involvedAgentMediatedWork: options.involvedAgentMediatedWork,
      ledgerEntries,
      projectedAt,
    });
    const projectionEntry = await createFileKnowledgeStore(knowledgeDir).addProjection({
      project: resolveKnowledgeProject(options.env),
      scope: "reflect",
      key: `receipt:${options.receipt.id}`,
      value: reflectProjection,
      source: "post_run.reflect",
      confidence: 1,
      freshness: "derived",
      receiptId: options.receipt.id,
      createdAt: projectedAt,
    });
    await appendLedgerEntries({
      receiptDir: options.receiptDir,
      runId: options.runId,
      entries: [
        createRunEventEntry({
          runId: options.runId,
          producer: {
            skill: options.skillName,
            runner: runnerReceiptCategory(options.receipt) === "graph" ? "graph" : runnerReceiptSource(options.receipt) ?? "runtime",
          },
          kind: "reflect_projected",
          status: "success",
          detail: {
            projection_entry_id: projectionEntry.entry_id,
            receipt_id: options.receipt.id,
            policy,
            mediation: reflectProjection.mediation,
          },
          createdAt: projectedAt,
        }),
      ],
    });
  } catch (error) {
    await options.caller.report({
      type: "warning",
      message: "Post-run reflect projection failed; continuing with the persisted receipt.",
      data: {
        receiptId: options.receipt.id,
        error: errorMessage(error),
      },
    });
  }
}

function buildReflectProjection(options: {
  readonly receipt: ReflectReceipt;
  readonly runId: string;
  readonly skillName: string;
  readonly selectedRunnerName?: string;
  readonly policy: PostRunReflectPolicy;
  readonly involvedAgentMediatedWork: boolean;
  readonly ledgerEntries: readonly ArtifactEnvelope[];
  readonly projectedAt: string;
}): LocalReflectProjection {
  const eventKinds = uniqueStrings(
    options.ledgerEntries
      .filter((entry) => entry.type === "run_event")
      .map((entry) => String(entry.data.kind)),
  );
  const artifactEntries = options.ledgerEntries.filter((entry) => entry.type === null || !SYSTEM_ARTIFACT_TYPES.has(entry.type));
  const artifactTypes = uniqueStrings(
    artifactEntries
      .map((entry) => entry.type)
      .filter((type): type is string => typeof type === "string"),
  );
  const signals = [
    options.involvedAgentMediatedWork ? "agent-mediated" : "deterministic",
    runnerReceiptCategory(options.receipt) === "graph" ? "graph-execution" : "skill-execution",
    runnerReceiptStatus(options.receipt) === "failure" ? "run-failed" : "run-succeeded",
    ...(artifactEntries.length > 0 ? ["artifacts-emitted"] : []),
    ...(eventKinds.includes("step_waiting_resolution") ? ["paused-before-completion"] : []),
  ];

  const steps = runnerReceiptGraphSteps(options.receipt);
  const stepSummary =
    runnerReceiptCategory(options.receipt) === "graph"
      ? {
          total_steps: steps.length,
          successful_steps: steps.filter((step) => step.status === "sealed").length,
          failed_steps: steps.filter((step) => step.status === "failure").length,
          runner_types: uniqueStrings(steps.map((step) => step.runner ?? "default")),
        }
      : undefined;
  const receiptStatus = runnerReceiptStatus(options.receipt);

  return {
    schema_version: "runx.reflect.v1",
    skill_ref: options.skillName,
    receipt_id: options.receipt.id,
    run_id: options.runId,
    receipt_category: runnerReceiptCategory(options.receipt),
    status: receiptStatus,
    selected_runner: options.selectedRunnerName,
    policy: options.policy,
    mediation: options.involvedAgentMediatedWork ? "agentic" : "deterministic",
    summary:
      runnerReceiptCategory(options.receipt) === "graph"
        ? `${options.skillName} ${receiptStatus} with ${steps.length} step(s)`
        : `${options.skillName} ${receiptStatus} via ${runnerReceiptSource(options.receipt) ?? "runtime"}`,
    signals,
    ledger: {
      event_kinds: eventKinds,
      artifact_count: artifactEntries.length,
      artifact_types: artifactTypes,
    },
    step_summary: stepSummary,
    projected_at: options.projectedAt,
  };
}

function shouldProjectReflect(policy: PostRunReflectPolicy, involvedAgentMediatedWork: boolean): boolean {
  if (policy === "always") {
    return true;
  }
  if (policy === "auto") {
    return involvedAgentMediatedWork;
  }
  return false;
}

function resolveOptionalKnowledgeDir(options: {
  readonly knowledgeDir?: string;
  readonly env?: NodeJS.ProcessEnv;
}): string | undefined {
  if (options.knowledgeDir) {
    return options.knowledgeDir;
  }
  if (!options.env?.RUNX_KNOWLEDGE_DIR) {
    return undefined;
  }
  return resolveRunxKnowledgeDir(options.env);
}

function resolveKnowledgeProject(env?: NodeJS.ProcessEnv): string {
  return path.resolve(env?.RUNX_PROJECT ?? env?.RUNX_CWD ?? env?.INIT_CWD ?? process.cwd());
}

function uniqueStrings(values: readonly (string | null | undefined)[]): readonly string[] {
  return Array.from(
    new Set(values.filter((value): value is string => typeof value === "string" && value.trim().length > 0)),
  );
}
