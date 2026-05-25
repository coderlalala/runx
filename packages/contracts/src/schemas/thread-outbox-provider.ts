import { Type, type Static } from "../internal.js";
import {
  JSON_SCHEMA_DRAFT_2020_12,
  RUNX_CONTRACT_IDS,
  RUNX_LOGICAL_SCHEMAS,
  type DeepReadonly,
  dateTimeStringSchema,
  generatedSchema,
  stringEnum,
  validateContractSchema,
} from "../internal.js";
import {
  credentialDeliveryModeSchema,
  credentialDeliveryObservationV1Schema,
  credentialDeliveryPurposeSchema,
} from "./credential-delivery.js";
import { referenceSchema } from "./spine.js";

export const threadOutboxProviderProtocolVersion = "runx.thread_outbox_provider.v1" as const;

const threadOutboxProviderOperations = ["push", "fetch"] as const;
const threadOutboxProviderTransportKinds = ["process"] as const;
const threadOutboxProviderPayloadFormats = ["markdown", "plain_text", "json"] as const;
const threadOutboxProviderObservationStatuses = ["accepted", "skipped", "failed"] as const;
const threadOutboxProviderIdempotencyStatuses = ["created", "replayed", "skipped", "failed"] as const;

export const threadOutboxProviderOperationSchema = stringEnum(threadOutboxProviderOperations);
export const threadOutboxProviderTransportKindSchema = stringEnum(threadOutboxProviderTransportKinds);
export const threadOutboxProviderPayloadFormatSchema = stringEnum(threadOutboxProviderPayloadFormats);
export const threadOutboxProviderObservationStatusSchema = stringEnum(
  threadOutboxProviderObservationStatuses,
);
export const threadOutboxProviderIdempotencyStatusSchema = stringEnum(
  threadOutboxProviderIdempotencyStatuses,
);

export const threadOutboxProviderTransportSchema = Type.Object(
  {
    kind: threadOutboxProviderTransportKindSchema,
    command: Type.Optional(Type.String({ minLength: 1 })),
    args: Type.Optional(Type.Array(Type.String())),
    endpoint: Type.Optional(Type.String({ minLength: 1 })),
  },
  { additionalProperties: false },
);

export type ThreadOutboxProviderTransportContract =
  DeepReadonly<Static<typeof threadOutboxProviderTransportSchema>>;

export const threadOutboxProviderCredentialNeedSchema = Type.Object(
  {
    provider: Type.String({ minLength: 1 }),
    purpose: credentialDeliveryPurposeSchema,
    profile_id: Type.String({ minLength: 1 }),
    delivery_mode: credentialDeliveryModeSchema,
    required: Type.Boolean(),
    scope_refs: Type.Optional(Type.Array(referenceSchema)),
  },
  { additionalProperties: false },
);

export type ThreadOutboxProviderCredentialNeedContract =
  DeepReadonly<Static<typeof threadOutboxProviderCredentialNeedSchema>>;

export const threadOutboxProviderReceiptCapabilitiesSchema = Type.Object(
  {
    idempotent_push: Type.Boolean(),
    readback: Type.Boolean(),
    stable_provider_event_hash: Type.Boolean(),
  },
  { additionalProperties: false },
);

export type ThreadOutboxProviderReceiptCapabilitiesContract =
  DeepReadonly<Static<typeof threadOutboxProviderReceiptCapabilitiesSchema>>;

export const threadOutboxProviderRedactionCapabilitiesSchema = Type.Object(
  {
    redacts_credentials: Type.Boolean(),
    redacts_provider_payloads: Type.Boolean(),
    supports_redaction_refs: Type.Boolean(),
  },
  { additionalProperties: false },
);

export type ThreadOutboxProviderRedactionCapabilitiesContract =
  DeepReadonly<Static<typeof threadOutboxProviderRedactionCapabilitiesSchema>>;

const threadOutboxProviderManifestV1TypeSchema = Type.Object(
  {
    schema: Type.Literal(RUNX_LOGICAL_SCHEMAS.threadOutboxProviderManifest),
    protocol_version: Type.Literal(threadOutboxProviderProtocolVersion),
    adapter_id: Type.String({ minLength: 1 }),
    provider: Type.String({ minLength: 1 }),
    name: Type.String({ minLength: 1 }),
    version: Type.String({ minLength: 1 }),
    supported_operations: Type.Array(threadOutboxProviderOperationSchema, { minItems: 1 }),
    transport: threadOutboxProviderTransportSchema,
    credential_needs: Type.Optional(Type.Array(threadOutboxProviderCredentialNeedSchema)),
    receipt_capabilities: threadOutboxProviderReceiptCapabilitiesSchema,
    redaction_capabilities: threadOutboxProviderRedactionCapabilitiesSchema,
  },
  {
    $schema: JSON_SCHEMA_DRAFT_2020_12,
    $id: RUNX_CONTRACT_IDS.threadOutboxProviderManifest,
    "x-runx-schema": RUNX_LOGICAL_SCHEMAS.threadOutboxProviderManifest,
    additionalProperties: false,
  },
);

export type ThreadOutboxProviderManifestContract =
  DeepReadonly<Static<typeof threadOutboxProviderManifestV1TypeSchema>>;

export const threadOutboxProviderManifestV1Schema =
  generatedSchema<ThreadOutboxProviderManifestContract>(
    "thread-outbox-provider-manifest.schema.json",
  );

export const threadOutboxProviderThreadLocatorSchema = Type.Object(
  {
    provider: Type.String({ minLength: 1 }),
    thread_ref: referenceSchema,
    locator: Type.String({ minLength: 1 }),
  },
  { additionalProperties: false },
);

export type ThreadOutboxProviderThreadLocatorContract =
  DeepReadonly<Static<typeof threadOutboxProviderThreadLocatorSchema>>;

export const threadOutboxProviderLocatorSchema = Type.Object(
  {
    provider: Type.String({ minLength: 1 }),
    locator: Type.String({ minLength: 1 }),
    provider_ref: Type.Optional(referenceSchema),
  },
  { additionalProperties: false },
);

export type ThreadOutboxProviderLocatorContract =
  DeepReadonly<Static<typeof threadOutboxProviderLocatorSchema>>;

export const threadOutboxProviderIdempotencySchema = Type.Object(
  {
    key: Type.String({ minLength: 1 }),
    content_hash: Type.Optional(Type.String({ minLength: 1 })),
  },
  { additionalProperties: false },
);

export type ThreadOutboxProviderIdempotencyContract =
  DeepReadonly<Static<typeof threadOutboxProviderIdempotencySchema>>;

export const threadOutboxProviderIdempotencyObservationSchema = Type.Object(
  {
    key: Type.String({ minLength: 1 }),
    status: threadOutboxProviderIdempotencyStatusSchema,
    original_observation_ref: Type.Optional(referenceSchema),
  },
  { additionalProperties: false },
);

export type ThreadOutboxProviderIdempotencyObservationContract =
  DeepReadonly<Static<typeof threadOutboxProviderIdempotencyObservationSchema>>;

export const threadOutboxProviderRenderedPayloadSchema = Type.Object(
  {
    format: threadOutboxProviderPayloadFormatSchema,
    body: Type.String({ minLength: 1 }),
    body_sha256: Type.Optional(Type.String({ minLength: 1 })),
    redaction_refs: Type.Optional(Type.Array(referenceSchema)),
  },
  { additionalProperties: false },
);

export type ThreadOutboxProviderRenderedPayloadContract =
  DeepReadonly<Static<typeof threadOutboxProviderRenderedPayloadSchema>>;

export const threadOutboxProviderCredentialProfileSchema = Type.Object(
  {
    provider: Type.String({ minLength: 1 }),
    purpose: credentialDeliveryPurposeSchema,
    profile_id: Type.String({ minLength: 1 }),
    delivery_mode: credentialDeliveryModeSchema,
    credential_refs: Type.Array(referenceSchema),
  },
  { additionalProperties: false },
);

export type ThreadOutboxProviderCredentialProfileContract =
  DeepReadonly<Static<typeof threadOutboxProviderCredentialProfileSchema>>;

export const threadOutboxProviderReceiptContextSchema = Type.Object(
  {
    harness_ref: referenceSchema,
    host_ref: referenceSchema,
    authority_proof_refs: Type.Optional(Type.Array(referenceSchema)),
    scope_refs: Type.Optional(Type.Array(referenceSchema)),
  },
  { additionalProperties: false },
);

export type ThreadOutboxProviderReceiptContextContract =
  DeepReadonly<Static<typeof threadOutboxProviderReceiptContextSchema>>;

const threadOutboxProviderPushV1TypeSchema = Type.Object(
  {
    schema: Type.Literal(RUNX_LOGICAL_SCHEMAS.threadOutboxProviderPush),
    protocol_version: Type.Literal(threadOutboxProviderProtocolVersion),
    push_id: Type.String({ minLength: 1 }),
    adapter_id: Type.String({ minLength: 1 }),
    provider: Type.String({ minLength: 1 }),
    outbox_entry_id: Type.String({ minLength: 1 }),
    thread_locator: threadOutboxProviderThreadLocatorSchema,
    idempotency: threadOutboxProviderIdempotencySchema,
    payload: threadOutboxProviderRenderedPayloadSchema,
    provider_profile: threadOutboxProviderCredentialProfileSchema,
    credential_delivery_refs: Type.Array(referenceSchema),
    receipt_context: threadOutboxProviderReceiptContextSchema,
    requested_at: dateTimeStringSchema(),
  },
  {
    $schema: JSON_SCHEMA_DRAFT_2020_12,
    $id: RUNX_CONTRACT_IDS.threadOutboxProviderPush,
    "x-runx-schema": RUNX_LOGICAL_SCHEMAS.threadOutboxProviderPush,
    additionalProperties: false,
  },
);

export type ThreadOutboxProviderPushContract =
  DeepReadonly<Static<typeof threadOutboxProviderPushV1TypeSchema>>;

export const threadOutboxProviderPushV1Schema =
  generatedSchema<ThreadOutboxProviderPushContract>(
    "thread-outbox-provider-push.schema.json",
  );

export const threadOutboxProviderFetchThreadTargetSchema = Type.Object(
  {
    thread_locator: threadOutboxProviderThreadLocatorSchema,
  },
  { additionalProperties: false },
);

export type ThreadOutboxProviderFetchThreadTargetContract =
  DeepReadonly<Static<typeof threadOutboxProviderFetchThreadTargetSchema>>;

export const threadOutboxProviderFetchProviderTargetSchema = Type.Object(
  {
    provider_locator: threadOutboxProviderLocatorSchema,
  },
  { additionalProperties: false },
);

export type ThreadOutboxProviderFetchProviderTargetContract =
  DeepReadonly<Static<typeof threadOutboxProviderFetchProviderTargetSchema>>;

export const threadOutboxProviderFetchTargetSchema = Type.Union([
  threadOutboxProviderFetchThreadTargetSchema,
  threadOutboxProviderFetchProviderTargetSchema,
]);

export type ThreadOutboxProviderFetchTargetContract =
  DeepReadonly<Static<typeof threadOutboxProviderFetchTargetSchema>>;

const threadOutboxProviderFetchV1TypeSchema = Type.Object(
  {
    schema: Type.Literal(RUNX_LOGICAL_SCHEMAS.threadOutboxProviderFetch),
    protocol_version: Type.Literal(threadOutboxProviderProtocolVersion),
    fetch_id: Type.String({ minLength: 1 }),
    adapter_id: Type.String({ minLength: 1 }),
    provider: Type.String({ minLength: 1 }),
    target: threadOutboxProviderFetchTargetSchema,
    readback_cursor: Type.Optional(Type.String({ minLength: 1 })),
    idempotency: threadOutboxProviderIdempotencySchema,
    provider_profile: threadOutboxProviderCredentialProfileSchema,
    credential_delivery_refs: Type.Array(referenceSchema),
    receipt_context: threadOutboxProviderReceiptContextSchema,
    requested_at: dateTimeStringSchema(),
  },
  {
    $schema: JSON_SCHEMA_DRAFT_2020_12,
    $id: RUNX_CONTRACT_IDS.threadOutboxProviderFetch,
    "x-runx-schema": RUNX_LOGICAL_SCHEMAS.threadOutboxProviderFetch,
    additionalProperties: false,
  },
);

export type ThreadOutboxProviderFetchContract =
  DeepReadonly<Static<typeof threadOutboxProviderFetchV1TypeSchema>>;

export const threadOutboxProviderFetchV1Schema =
  generatedSchema<ThreadOutboxProviderFetchContract>(
    "thread-outbox-provider-fetch.schema.json",
  );

export const threadOutboxProviderReadbackSummarySchema = Type.Object(
  {
    item_count: Type.Integer({ minimum: 0 }),
    cursor: Type.Optional(Type.String({ minLength: 1 })),
    latest_provider_event_id_hash: Type.Optional(Type.String({ minLength: 1 })),
  },
  { additionalProperties: false },
);

export type ThreadOutboxProviderReadbackSummaryContract =
  DeepReadonly<Static<typeof threadOutboxProviderReadbackSummarySchema>>;

export const threadOutboxProviderErrorSchema = Type.Object(
  {
    code: Type.String({ minLength: 1 }),
    message: Type.String({ minLength: 1 }),
    retryable: Type.Boolean(),
  },
  { additionalProperties: false },
);

export type ThreadOutboxProviderErrorContract =
  DeepReadonly<Static<typeof threadOutboxProviderErrorSchema>>;

const threadOutboxProviderObservationV1TypeSchema = Type.Object(
  {
    schema: Type.Literal(RUNX_LOGICAL_SCHEMAS.threadOutboxProviderObservation),
    protocol_version: Type.Literal(threadOutboxProviderProtocolVersion),
    observation_id: Type.String({ minLength: 1 }),
    adapter_id: Type.String({ minLength: 1 }),
    provider: Type.String({ minLength: 1 }),
    operation: threadOutboxProviderOperationSchema,
    request_id: Type.String({ minLength: 1 }),
    status: threadOutboxProviderObservationStatusSchema,
    idempotency: threadOutboxProviderIdempotencyObservationSchema,
    provider_locator: Type.Optional(threadOutboxProviderLocatorSchema),
    provider_event_id_hash: Type.Optional(Type.String({ minLength: 1 })),
    readback_summary: Type.Optional(threadOutboxProviderReadbackSummarySchema),
    delivery_observations: Type.Optional(Type.Array(credentialDeliveryObservationV1Schema)),
    redaction_refs: Type.Optional(Type.Array(referenceSchema)),
    errors: Type.Optional(Type.Array(threadOutboxProviderErrorSchema)),
    observed_at: dateTimeStringSchema(),
  },
  {
    $schema: JSON_SCHEMA_DRAFT_2020_12,
    $id: RUNX_CONTRACT_IDS.threadOutboxProviderObservation,
    "x-runx-schema": RUNX_LOGICAL_SCHEMAS.threadOutboxProviderObservation,
    additionalProperties: false,
  },
);

export type ThreadOutboxProviderObservationContract =
  DeepReadonly<Static<typeof threadOutboxProviderObservationV1TypeSchema>>;

export const threadOutboxProviderObservationV1Schema =
  generatedSchema<ThreadOutboxProviderObservationContract>(
    "thread-outbox-provider-observation.schema.json",
  );

export function validateThreadOutboxProviderManifestContract(
  value: unknown,
  label = "thread_outbox_provider_manifest",
): ThreadOutboxProviderManifestContract {
  return validateContractSchema(threadOutboxProviderManifestV1Schema, value, label);
}

export function validateThreadOutboxProviderPushContract(
  value: unknown,
  label = "thread_outbox_provider_push",
): ThreadOutboxProviderPushContract {
  return validateContractSchema(threadOutboxProviderPushV1Schema, value, label);
}

export function validateThreadOutboxProviderFetchContract(
  value: unknown,
  label = "thread_outbox_provider_fetch",
): ThreadOutboxProviderFetchContract {
  return validateContractSchema(threadOutboxProviderFetchV1Schema, value, label);
}

export function validateThreadOutboxProviderObservationContract(
  value: unknown,
  label = "thread_outbox_provider_observation",
): ThreadOutboxProviderObservationContract {
  return validateContractSchema(threadOutboxProviderObservationV1Schema, value, label);
}
