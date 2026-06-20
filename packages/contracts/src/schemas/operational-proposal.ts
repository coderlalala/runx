import {
  type DeepReadonly,
  generatedSchema,
  validateContractSchema,
} from "../internal.js";
import type { ReferenceContract, ReferenceLinkContract } from "./spine.js";

export const operationalProposalSchemaVersion = "runx.operational_proposal.v1" as const;

export type OperationalProposalRedactionStatusContract =
  | "redacted"
  | "summary_only"
  | "blocked";

export type OperationalProposalRecommendedActionContract = DeepReadonly<{
  action_intent: string;
  summary: string;
  mutating: boolean;
  target_refs?: readonly OperationalProposalReferenceContract[];
}>;

export type OperationalProposalIdempotencyContract = DeepReadonly<{
  key: string;
  fingerprint: string;
}>;

export type OperationalProposalAuthorityContract = DeepReadonly<{
  proposal_only: true;
  mutation_authority_granted: false;
  publication_authority_granted: false;
  final_decision_authority_granted: false;
  notes?: readonly string[];
}>;

export type OperationalProposalHumanGateContract = DeepReadonly<{
  gate_id: string;
  gate_kind: string;
  required: boolean;
  decision: string;
  reason: string;
}>;

export type OperationalProposalOutcomeContract = DeepReadonly<{
  observed: boolean;
  status: string;
  summary: string;
  observed_at?: string;
  refs?: readonly OperationalProposalReferenceContract[];
}>;

export type OperationalProposalReferenceContract = ReferenceContract;
export type OperationalProposalReferenceLinkContract = ReferenceLinkContract;

export type OperationalProposalEscalationExtensionContract = DeepReadonly<{
  severity: string;
  urgency: string;
  suspected_area?: string;
}>;

export type OperationalProposalExtensionsContract = DeepReadonly<Record<string, unknown> & {
  "runx.escalation"?: OperationalProposalEscalationExtensionContract;
}>;

export type OperationalProposalContract = DeepReadonly<{
  schema: typeof operationalProposalSchemaVersion;
  proposal_id: string;
  proposal_kind: string;
  source_event_id: string;
  idempotency: OperationalProposalIdempotencyContract;
  source_ref: OperationalProposalReferenceContract;
  source_thread_ref?: OperationalProposalReferenceContract;
  hydrated_context_ref: OperationalProposalReferenceContract;
  redaction_status: OperationalProposalRedactionStatusContract;
  decision_summary: string;
  rationale: string;
  recommended_actions?: readonly OperationalProposalRecommendedActionContract[];
  evidence_refs?: readonly OperationalProposalReferenceContract[];
  artifact_refs?: readonly OperationalProposalReferenceContract[];
  receipt_refs?: readonly OperationalProposalReferenceContract[];
  story_refs?: readonly OperationalProposalReferenceContract[];
  result_refs?: readonly OperationalProposalReferenceLinkContract[];
  publication_refs?: readonly OperationalProposalReferenceLinkContract[];
  owner_route_id: string;
  confidence: number;
  risks?: readonly string[];
  caveats?: readonly string[];
  missing_context?: readonly string[];
  authority: OperationalProposalAuthorityContract;
  human_gates?: readonly OperationalProposalHumanGateContract[];
  allowed_next_actions?: readonly string[];
  final_outcome?: OperationalProposalOutcomeContract;
  public_summary: string;
  extensions?: OperationalProposalExtensionsContract;
}>;

export const operationalProposalSchema = generatedSchema<OperationalProposalContract>(
  "operational-proposal.schema.json",
);

export function validateOperationalProposalContract(
  value: unknown,
  label = "operational_proposal",
): OperationalProposalContract {
  return validateContractSchema(operationalProposalSchema, value, label) as OperationalProposalContract;
}
