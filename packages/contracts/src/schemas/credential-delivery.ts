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
import { referenceSchema } from "./spine.js";

const credentialDeliveryModes = ["process_env"] as const;
const credentialDeliveryPurposes = [
  "provider_api",
  "registry",
  "artifact_store",
  "webhook_verification",
] as const;
const credentialMaterialRoles = [
  "access_token",
  "refresh_token",
  "api_key",
  "client_secret",
  "session_token",
] as const;
const credentialDeliveryStatuses = [
  "delivered",
  "denied",
  "not_found",
  "profile_mismatch",
] as const;
const credentialDeliveryObservationStatuses = [
  "delivered",
  "denied",
  "not_delivered",
] as const;

export const credentialDeliveryModeSchema = stringEnum(credentialDeliveryModes);
export const credentialDeliveryPurposeSchema = stringEnum(credentialDeliveryPurposes);
export const credentialMaterialRoleSchema = stringEnum(credentialMaterialRoles);
export const credentialDeliveryStatusSchema = stringEnum(credentialDeliveryStatuses);
export const credentialDeliveryObservationStatusSchema = stringEnum(
  credentialDeliveryObservationStatuses,
);

export const credentialDeliveryEnvBindingSchema = Type.Object(
  {
    role: credentialMaterialRoleSchema,
    env_var: Type.String({ minLength: 1, pattern: "^[A-Z_][A-Z0-9_]*$" }),
    required: Type.Boolean(),
  },
  { additionalProperties: false },
);

export type CredentialDeliveryEnvBindingContract =
  DeepReadonly<Static<typeof credentialDeliveryEnvBindingSchema>>;

const credentialDeliveryProfileV1TypeSchema = Type.Object(
  {
    schema: Type.Literal(RUNX_LOGICAL_SCHEMAS.credentialDeliveryProfile),
    profile_id: Type.String({ minLength: 1 }),
    provider: Type.String({ minLength: 1 }),
    auth_mode: Type.String({ minLength: 1 }),
    purpose: credentialDeliveryPurposeSchema,
    delivery_mode: credentialDeliveryModeSchema,
    material_roles: Type.Array(credentialMaterialRoleSchema, { minItems: 1 }),
    env_bindings: Type.Array(credentialDeliveryEnvBindingSchema, { minItems: 1 }),
    redaction_policy_ref: referenceSchema,
  },
  {
    $schema: JSON_SCHEMA_DRAFT_2020_12,
    $id: RUNX_CONTRACT_IDS.credentialDeliveryProfile,
    "x-runx-schema": RUNX_LOGICAL_SCHEMAS.credentialDeliveryProfile,
    additionalProperties: false,
  },
);

export type CredentialDeliveryProfileContract =
  DeepReadonly<Static<typeof credentialDeliveryProfileV1TypeSchema>>;

export const credentialDeliveryProfileV1Schema = generatedSchema<CredentialDeliveryProfileContract>(
  "credential-delivery-profile.schema.json",
);

const credentialDeliveryRequestV1TypeSchema = Type.Object(
  {
    schema: Type.Literal(RUNX_LOGICAL_SCHEMAS.credentialDeliveryRequest),
    request_id: Type.String({ minLength: 1 }),
    harness_ref: referenceSchema,
    host_ref: referenceSchema,
    grant_ref: referenceSchema,
    credential_ref: referenceSchema,
    profile_id: Type.String({ minLength: 1 }),
    provider: Type.String({ minLength: 1 }),
    purpose: credentialDeliveryPurposeSchema,
    requested_roles: Type.Array(credentialMaterialRoleSchema, { minItems: 1 }),
    requested_at: dateTimeStringSchema(),
  },
  {
    $schema: JSON_SCHEMA_DRAFT_2020_12,
    $id: RUNX_CONTRACT_IDS.credentialDeliveryRequest,
    "x-runx-schema": RUNX_LOGICAL_SCHEMAS.credentialDeliveryRequest,
    additionalProperties: false,
  },
);

export type CredentialDeliveryRequestContract =
  DeepReadonly<Static<typeof credentialDeliveryRequestV1TypeSchema>>;

export const credentialDeliveryRequestV1Schema = generatedSchema<CredentialDeliveryRequestContract>(
  "credential-delivery-request.schema.json",
);

export const credentialDeliveryHandleSchema = Type.Object(
  {
    role: credentialMaterialRoleSchema,
    delivery_handle_ref: referenceSchema,
    env_var: Type.Optional(Type.String({ minLength: 1, pattern: "^[A-Z_][A-Z0-9_]*$" })),
  },
  { additionalProperties: false },
);

export type CredentialDeliveryHandleContract =
  DeepReadonly<Static<typeof credentialDeliveryHandleSchema>>;

const credentialDeliveryBrokerResponseV1TypeSchema = Type.Object(
  {
    schema: Type.Literal(RUNX_LOGICAL_SCHEMAS.credentialDeliveryBrokerResponse),
    response_id: Type.String({ minLength: 1 }),
    request_id: Type.String({ minLength: 1 }),
    status: credentialDeliveryStatusSchema,
    delivery_mode: Type.Optional(credentialDeliveryModeSchema),
    handles: Type.Optional(Type.Array(credentialDeliveryHandleSchema)),
    credential_refs: Type.Array(referenceSchema),
    material_ref_hash: Type.Optional(Type.String({ minLength: 1 })),
    denied_reasons: Type.Optional(Type.Array(Type.String({ minLength: 1 }))),
    issued_at: dateTimeStringSchema(),
    expires_at: Type.Optional(dateTimeStringSchema()),
  },
  {
    $schema: JSON_SCHEMA_DRAFT_2020_12,
    $id: RUNX_CONTRACT_IDS.credentialDeliveryBrokerResponse,
    "x-runx-schema": RUNX_LOGICAL_SCHEMAS.credentialDeliveryBrokerResponse,
    additionalProperties: false,
  },
);

export type CredentialDeliveryBrokerResponseContract =
  DeepReadonly<Static<typeof credentialDeliveryBrokerResponseV1TypeSchema>>;

export const credentialDeliveryBrokerResponseV1Schema =
  generatedSchema<CredentialDeliveryBrokerResponseContract>(
    "credential-delivery-broker-response.schema.json",
  );

const credentialDeliveryObservationV1TypeSchema = Type.Object(
  {
    schema: Type.Literal(RUNX_LOGICAL_SCHEMAS.credentialDeliveryObservation),
    observation_id: Type.String({ minLength: 1 }),
    request_id: Type.String({ minLength: 1 }),
    response_id: Type.Optional(Type.String({ minLength: 1 })),
    status: credentialDeliveryObservationStatusSchema,
    harness_ref: referenceSchema,
    host_ref: Type.Optional(referenceSchema),
    profile_id: Type.String({ minLength: 1 }),
    provider: Type.String({ minLength: 1 }),
    purpose: credentialDeliveryPurposeSchema,
    delivery_mode: Type.Optional(credentialDeliveryModeSchema),
    credential_refs: Type.Array(referenceSchema),
    material_ref_hash: Type.Optional(Type.String({ minLength: 1 })),
    delivered_roles: Type.Array(credentialMaterialRoleSchema),
    redaction_refs: Type.Optional(Type.Array(referenceSchema)),
    observed_at: dateTimeStringSchema(),
  },
  {
    $schema: JSON_SCHEMA_DRAFT_2020_12,
    $id: RUNX_CONTRACT_IDS.credentialDeliveryObservation,
    "x-runx-schema": RUNX_LOGICAL_SCHEMAS.credentialDeliveryObservation,
    additionalProperties: false,
  },
);

export type CredentialDeliveryObservationContract =
  DeepReadonly<Static<typeof credentialDeliveryObservationV1TypeSchema>>;

export const credentialDeliveryObservationV1Schema =
  generatedSchema<CredentialDeliveryObservationContract>(
    "credential-delivery-observation.schema.json",
  );

export function validateCredentialDeliveryProfileContract(
  value: unknown,
  label = "credential_delivery_profile",
): CredentialDeliveryProfileContract {
  return validateContractSchema(credentialDeliveryProfileV1Schema, value, label);
}

export function validateCredentialDeliveryRequestContract(
  value: unknown,
  label = "credential_delivery_request",
): CredentialDeliveryRequestContract {
  return validateContractSchema(credentialDeliveryRequestV1Schema, value, label);
}

export function validateCredentialDeliveryBrokerResponseContract(
  value: unknown,
  label = "credential_delivery_broker_response",
): CredentialDeliveryBrokerResponseContract {
  return validateContractSchema(credentialDeliveryBrokerResponseV1Schema, value, label);
}

export function validateCredentialDeliveryObservationContract(
  value: unknown,
  label = "credential_delivery_observation",
): CredentialDeliveryObservationContract {
  return validateContractSchema(credentialDeliveryObservationV1Schema, value, label);
}
