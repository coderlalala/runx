import {
  type DeepReadonly,
  type UnknownRecord,
  generatedSchema,
  validateContractSchema,
} from "../internal.js";
import type { ReferenceContract } from "./spine.js";

export type EffectFinalityReceiptPhaseContract =
  | "provisional"
  | "in_flight"
  | "sealed"
  | "failed"
  | "reversed";

export type EffectFinalityReceiptContract = DeepReadonly<{
  schema: "runx.effect_finality_receipt.v1";
  id: string;
  created_at: string;
  family: string;
  phase: EffectFinalityReceiptPhaseContract;
  original_receipt_ref: ReferenceContract;
  criterion_id: string;
  evidence_refs?: readonly ReferenceContract[];
  norm_refs?: readonly string[];
  proof_ref?: ReferenceContract;
  confirmation_depth?: number;
  payload?: UnknownRecord;
}>;

export const effectFinalityReceiptV1Schema = generatedSchema<EffectFinalityReceiptContract>(
  "effect-finality-receipt.schema.json",
);

export function validateEffectFinalityReceiptContract(
  value: unknown,
  label = "effect finality receipt",
): EffectFinalityReceiptContract {
  return validateContractSchema(effectFinalityReceiptV1Schema, value, label) as EffectFinalityReceiptContract;
}
