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

export const evidenceBundleSchemaVersion = RUNX_LOGICAL_SCHEMAS.evidenceBundle;

export const evidenceBundleSourceProviders = [
  "slack",
  "sentry",
  "github",
  "file",
  "api",
  "other",
] as const;

export const evidenceBundleHydrationStatuses = [
  "complete",
  "unavailable",
  "needed",
] as const;

export const evidenceBundleSourceKinds = [
  "source_thread",
  "alert",
  "event",
  "stacktrace",
  "log",
  "deployment",
  "user_report",
  "other",
] as const;

export const evidenceBundleRedactionStatuses = [
  "applied",
  "not_required",
] as const;

export const evidenceBundleSourceProviderSchema = stringEnum(evidenceBundleSourceProviders);
export const evidenceBundleHydrationStatusSchema = stringEnum(evidenceBundleHydrationStatuses);
export const evidenceBundleSourceKindSchema = stringEnum(evidenceBundleSourceKinds);
export const evidenceBundleRedactionStatusSchema = stringEnum(evidenceBundleRedactionStatuses);

export const evidenceBundleSourceSchema = Type.Object(
  {
    provider: evidenceBundleSourceProviderSchema,
    kind: evidenceBundleSourceKindSchema,
    locator: Type.String({ minLength: 1 }),
    thread_locator: Type.Optional(Type.String({ minLength: 1 })),
    provider_event_id: Type.Optional(Type.String({ minLength: 1 })),
    url: Type.Optional(Type.String({ minLength: 1 })),
    title: Type.Optional(Type.String({ minLength: 1 })),
    body_preview: Type.Optional(Type.String({ minLength: 1, maxLength: 2000 })),
    hydration_status: Type.Optional(evidenceBundleHydrationStatusSchema),
    observed_at: Type.Optional(dateTimeStringSchema()),
    data: Type.Optional(unknownRecordSchema()),
  },
  { additionalProperties: false },
);

export const evidenceBundleHydrationSchema = Type.Object(
  {
    status: evidenceBundleHydrationStatusSchema,
    summary: Type.String({ minLength: 1 }),
    requested_at: Type.Optional(dateTimeStringSchema()),
    completed_at: Type.Optional(dateTimeStringSchema()),
    unavailable_reason: Type.Optional(Type.String({ minLength: 1 })),
  },
  { additionalProperties: false },
);

export const evidenceBundleRedactionSchema = Type.Object(
  {
    status: evidenceBundleRedactionStatusSchema,
    summary: Type.String({ minLength: 1 }),
    secret_material: Type.Optional(Type.String({ minLength: 1 })),
    pii: Type.Optional(Type.String({ minLength: 1 })),
  },
  { additionalProperties: false },
);

export const evidenceBundleSchema = Type.Object(
  {
    schema: Type.Literal(evidenceBundleSchemaVersion),
    evidence_bundle_id: Type.String({ minLength: 1 }),
    subject_locator: Type.String({ minLength: 1 }),
    hydration: evidenceBundleHydrationSchema,
    sources: Type.Array(evidenceBundleSourceSchema, { minItems: 1 }),
    redaction: Type.Optional(evidenceBundleRedactionSchema),
    summary: Type.String({ minLength: 1 }),
    created_at: dateTimeStringSchema(),
    updated_at: dateTimeStringSchema(),
  },
  {
    $schema: JSON_SCHEMA_DRAFT_2020_12,
    $id: RUNX_CONTRACT_IDS.evidenceBundle,
    "x-runx-schema": RUNX_LOGICAL_SCHEMAS.evidenceBundle,
    additionalProperties: false,
  },
);

export type EvidenceBundleSourceProviderContract = DeepReadonly<Static<typeof evidenceBundleSourceProviderSchema>>;
export type EvidenceBundleHydrationStatusContract = DeepReadonly<Static<typeof evidenceBundleHydrationStatusSchema>>;
export type EvidenceBundleSourceKindContract = DeepReadonly<Static<typeof evidenceBundleSourceKindSchema>>;
export type EvidenceBundleRedactionStatusContract = DeepReadonly<Static<typeof evidenceBundleRedactionStatusSchema>>;
export type EvidenceBundleSourceContract = DeepReadonly<Static<typeof evidenceBundleSourceSchema>>;
export type EvidenceBundleHydrationContract = DeepReadonly<Static<typeof evidenceBundleHydrationSchema>>;
export type EvidenceBundleRedactionContract = DeepReadonly<Static<typeof evidenceBundleRedactionSchema>>;
export type EvidenceBundleContract = DeepReadonly<Static<typeof evidenceBundleSchema>>;

export function validateEvidenceBundleContract(
  value: unknown,
  label = "evidence_bundle",
): EvidenceBundleContract {
  return validateContractSchema(evidenceBundleSchema, value, label);
}
