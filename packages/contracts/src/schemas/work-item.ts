import { Type, type Static } from "@sinclair/typebox";
import {
  JSON_SCHEMA_DRAFT_2020_12,
  RUNX_CONTRACT_IDS,
  RUNX_LOGICAL_SCHEMAS,
  type DeepReadonly,
  dateTimeStringSchema,
  stringEnum,
  unknownRecordSchema,
  validateContractSchema,
} from "../internal.js";
import { evidenceBundleSchema } from "./evidence-bundle.js";

export const workItemSchemaVersion = RUNX_LOGICAL_SCHEMAS.workItem;

export const workItemStates = [
  "intake_received",
  "dedupe_pending",
  "duplicate_candidate",
  "triage_pending",
  "planning_ready",
  "build_ready",
  "review_ready",
  "pr_ready",
  "merge_gate",
  "outcome_merged",
  "outcome_closed",
  "outcome_rejected",
  "blocked",
] as const;

export type WorkItemStateName = (typeof workItemStates)[number];

export const workItemStateTransitions: Readonly<Record<WorkItemStateName, readonly WorkItemStateName[]>> = {
  intake_received: ["dedupe_pending", "triage_pending", "planning_ready", "build_ready", "blocked", "outcome_closed"],
  dedupe_pending: ["duplicate_candidate", "triage_pending", "blocked", "outcome_closed"],
  duplicate_candidate: ["outcome_closed", "outcome_rejected", "blocked"],
  triage_pending: ["planning_ready", "build_ready", "outcome_closed", "outcome_rejected", "blocked"],
  planning_ready: ["build_ready", "outcome_closed", "outcome_rejected", "blocked"],
  build_ready: ["review_ready", "outcome_closed", "outcome_rejected", "blocked"],
  review_ready: ["pr_ready", "outcome_closed", "outcome_rejected", "blocked"],
  pr_ready: ["merge_gate", "outcome_closed", "outcome_rejected", "blocked"],
  merge_gate: ["outcome_merged", "outcome_closed", "outcome_rejected", "blocked"],
  outcome_merged: [],
  outcome_closed: [],
  outcome_rejected: [],
  blocked: [
    "dedupe_pending",
    "triage_pending",
    "planning_ready",
    "build_ready",
    "review_ready",
    "pr_ready",
    "merge_gate",
    "outcome_closed",
    "outcome_rejected",
  ],
};

export const workItemActions = [
  "reply-only",
  "issue-intake",
  "work-plan",
  "issue-to-pr",
  "manual-review",
] as const;

export const workItemSourceProviders = [
  "slack",
  "sentry",
  "github",
  "file",
  "api",
  "other",
] as const;

export const workItemStateSchema = stringEnum(workItemStates);
export const workItemActionSchema = stringEnum(workItemActions);
export const workItemSourceProviderSchema = stringEnum(workItemSourceProviders);

export function isWorkItemState(value: unknown): value is WorkItemStateName {
  return typeof value === "string" && workItemStates.includes(value as WorkItemStateName);
}

export function nextWorkItemStates(state: WorkItemStateName): readonly WorkItemStateName[] {
  return workItemStateTransitions[state];
}

export function canTransitionWorkItemState(from: WorkItemStateName, to: WorkItemStateName): boolean {
  return from === to || workItemStateTransitions[from].includes(to);
}

export const workItemSourceEventSchema = Type.Object(
  {
    provider: workItemSourceProviderSchema,
    source_locator: Type.String({ minLength: 1 }),
    event_kind: Type.Optional(Type.String({ minLength: 1 })),
    thread_locator: Type.Optional(Type.String({ minLength: 1 })),
    provider_event_id: Type.Optional(Type.String({ minLength: 1 })),
    title: Type.Optional(Type.String({ minLength: 1 })),
    body_preview: Type.Optional(Type.String({ minLength: 1, maxLength: 2000 })),
    confidence: Type.Optional(Type.Number({ minimum: 0, maximum: 1 })),
    fingerprint: Type.Optional(Type.String({ minLength: 1 })),
    occurred_at: Type.Optional(dateTimeStringSchema()),
  },
  { additionalProperties: false },
);

export const workItemDedupeSchema = Type.Object(
  {
    algorithm: Type.Literal("sha256"),
    source_locator: Type.String({ minLength: 1 }),
    fingerprint: Type.String({ minLength: 1 }),
    provider_event_id: Type.Optional(Type.String({ minLength: 1 })),
    duplicate_of: Type.Optional(Type.String({ minLength: 1 })),
    candidate_work_item_ids: Type.Optional(Type.Array(Type.String({ minLength: 1 }))),
  },
  { additionalProperties: false },
);

export const workItemTriageSchema = Type.Object(
  {
    category: Type.String({ minLength: 1 }),
    severity: stringEnum(["low", "medium", "high", "critical"] as const),
    confidence: Type.Number({ minimum: 0, maximum: 1 }),
    action: workItemActionSchema,
    recommended_lane: Type.Optional(workItemActionSchema),
    needs_human: Type.Boolean(),
    rationale: Type.String({ minLength: 1 }),
  },
  { additionalProperties: false },
);

export const workItemProviderRefSchema = Type.Object(
  {
    provider: workItemSourceProviderSchema,
    locator: Type.String({ minLength: 1 }),
    url: Type.Optional(Type.String({ minLength: 1 })),
    status: Type.Optional(Type.String({ minLength: 1 })),
  },
  { additionalProperties: false },
);

export const workItemOwnerSuggestionSchema = Type.Object(
  {
    owner: Type.String({ minLength: 1 }),
    source: Type.Optional(Type.String({ minLength: 1 })),
    confidence: Type.Optional(Type.Number({ minimum: 0, maximum: 1 })),
    rationale: Type.String({ minLength: 1 }),
  },
  { additionalProperties: false },
);

export const workItemTargetRepoSuggestionSchema = Type.Object(
  {
    repo: Type.String({ minLength: 1 }),
    source: Type.Optional(Type.String({ minLength: 1 })),
    confidence: Type.Optional(Type.Number({ minimum: 0, maximum: 1 })),
    rationale: Type.String({ minLength: 1 }),
  },
  { additionalProperties: false },
);

export const workItemMergeGateSchema = Type.Object(
  {
    required: Type.Boolean(),
    summary: Type.String({ minLength: 1 }),
    reviewer_handoff: Type.Optional(Type.String({ minLength: 1 })),
    provider_ref: Type.Optional(workItemProviderRefSchema),
  },
  { additionalProperties: false },
);

export const workItemOutcomeSchema = Type.Object(
  {
    state: stringEnum(["merged", "closed", "rejected", "superseded"] as const),
    summary: Type.String({ minLength: 1 }),
    observed_at: Type.Optional(dateTimeStringSchema()),
    provider_ref: Type.Optional(workItemProviderRefSchema),
  },
  { additionalProperties: false },
);

export const workItemSchema = Type.Object(
  {
    schema: Type.Literal(workItemSchemaVersion),
    work_item_id: Type.String({ minLength: 1 }),
    state: workItemStateSchema,
    source_events: Type.Array(workItemSourceEventSchema, { minItems: 1 }),
    dedupe: workItemDedupeSchema,
    triage: Type.Optional(workItemTriageSchema),
    change_set: Type.Optional(unknownRecordSchema()),
    plan: Type.Optional(unknownRecordSchema()),
    owner_suggestion: Type.Optional(workItemOwnerSuggestionSchema),
    target_repo_suggestion: Type.Optional(workItemTargetRepoSuggestionSchema),
    evidence_bundle: Type.Optional(evidenceBundleSchema),
    issue: Type.Optional(workItemProviderRefSchema),
    pull_request: Type.Optional(workItemProviderRefSchema),
    merge_gate: Type.Optional(workItemMergeGateSchema),
    outcome: Type.Optional(workItemOutcomeSchema),
    status_summary: Type.String({ minLength: 1 }),
    created_at: dateTimeStringSchema(),
    updated_at: dateTimeStringSchema(),
  },
  {
    $schema: JSON_SCHEMA_DRAFT_2020_12,
    $id: RUNX_CONTRACT_IDS.workItem,
    "x-runx-schema": RUNX_LOGICAL_SCHEMAS.workItem,
    additionalProperties: false,
  },
);

export type WorkItemStateContract = DeepReadonly<Static<typeof workItemStateSchema>>;
export type WorkItemActionContract = DeepReadonly<Static<typeof workItemActionSchema>>;
export type WorkItemSourceProviderContract = DeepReadonly<Static<typeof workItemSourceProviderSchema>>;
export type WorkItemSourceEventContract = DeepReadonly<Static<typeof workItemSourceEventSchema>>;
export type WorkItemDedupeContract = DeepReadonly<Static<typeof workItemDedupeSchema>>;
export type WorkItemTriageContract = DeepReadonly<Static<typeof workItemTriageSchema>>;
export type WorkItemProviderRefContract = DeepReadonly<Static<typeof workItemProviderRefSchema>>;
export type WorkItemOwnerSuggestionContract = DeepReadonly<Static<typeof workItemOwnerSuggestionSchema>>;
export type WorkItemTargetRepoSuggestionContract = DeepReadonly<Static<typeof workItemTargetRepoSuggestionSchema>>;
export type WorkItemMergeGateContract = DeepReadonly<Static<typeof workItemMergeGateSchema>>;
export type WorkItemOutcomeContract = DeepReadonly<Static<typeof workItemOutcomeSchema>>;
export type WorkItemContract = DeepReadonly<Static<typeof workItemSchema>>;

export function validateWorkItemContract(value: unknown, label = "work_item"): WorkItemContract {
  return validateContractSchema(workItemSchema, value, label);
}
