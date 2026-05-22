export const runnerLocalPackage = "@runxhq/runtime-local";

export * from "./official-cache.js";
export * from "./registry-resolver.js";
export * from "./skill-install.js";
export * from "./history.js";
export {
  resolveSkillRunner,
  resolveToolExecutionTarget,
  type OfficialSkillResolver,
} from "./execution-targets.js";
export { readPendingRunState, readPendingSkillPath } from "./inputs.js";
export { createCallerAgentAdapter, createCallerAgentStepAdapter, createCallerApprovalAdapter } from "./caller-adapters.js";
export type {
  ActReceiptEnvelope,
  AdapterActInvocation,
  NestedSkillInvocation,
  NestedSkillInvocationResult,
  NestedSkillInvoker,
  SkillAdapter,
} from "./adapter-types.js";
export {
  runnerReceiptCategory,
  runnerReceiptCompletedAt,
  runnerReceiptDisposition,
  runnerReceiptDurationMs,
  runnerReceiptEvidenceRefs,
  runnerReceiptGraphSteps,
  runnerReceiptInputContext,
  runnerReceiptOutcome,
  runnerReceiptOutcomeState,
  runnerReceiptSource,
  runnerReceiptStartedAt,
  runnerReceiptStatus,
  runnerReceiptSurfaceRefs,
} from "./graph-governance.js";
export {
  cleanupLocalProcessSandbox,
  prepareLocalProcessSandbox,
  type LocalProcessSandboxOptions,
  type LocalProcessSandboxResult,
} from "./process-sandbox.js";
export type { MaterializedContextEdge } from "./graph-context.js";
import { runLocalGraph } from "./orchestrator.js";
export { runLocalGraph };

import { readFile } from "node:fs/promises";

import {
  appendPreparedLedgerEntries,
  createLedgerAnchorMetadata,
  materializeArtifacts,
  prepareLedgerAppend,
  type ArtifactEnvelope,
} from "@runxhq/core/artifacts";
import { errorMessage, isRecord } from "@runxhq/core/util";
import { appendPendingSkillLedgerEntries, buildSkillLedgerEntries } from "./graph-ledger.js";
import {
  createCallerAgentAdapter,
  createCallerAgentStepAdapter,
  createCallerApprovalAdapter,
} from "./caller-adapters.js";
import {
  type ApprovalGateContract as ApprovalGate,
  type ContextContract as Context,
  type ContextDocumentContract as ContextDocument,
  type CredentialEnvelopeContract as CredentialEnvelope,
  type ReferenceContract,
  type ResolutionRequestContract as ResolutionRequest,
  type ResolutionResponseContract as ResolutionResponse,
} from "@runxhq/contracts";
import { executeSkill } from "./execute-skill.js";
import type {
  ActReceiptEnvelope,
  NestedSkillInvocation,
  NestedSkillInvocationResult,
  SkillAdapter,
} from "./adapter-types.js";
import type { MaterializedContextEdge } from "./graph-context.js";
import {
  loadRunxWorkspacePolicy,
  type RunxWorkspacePolicy,
} from "@runxhq/core/config";
import {
  contextReceiptMetadata,
  loadContext,
  loadVoiceProfile,
  prepareAgentContext,
  qualityProfileContext,
  skillQualityProfileReceiptMetadata,
  voiceProfileReceiptMetadata,
} from "./context.js";
import type { ScopeAdmissionContract } from "@runxhq/contracts";
import type { ToolCatalogAdapter } from "@runxhq/runtime-local/tool-catalogs";
import {
  mergeExecutionSemantics,
  normalizeExecutionSemantics,
  type ExecutionSemantics,
  type GovernedDisposition,
  type OutcomeState,
  type ReceiptInputContext,
  type ReceiptOutcome,
  type ReceiptSurfaceRef,
} from "./execution-semantics.js";
import {
  uniqueRunnerReceiptId,
  runnerReceiptCompletedAt,
  runnerReceiptDurationMs,
  runnerReceiptStartedAt,
  writeRunnerSkillReceipt,
  type GraphStepGovernance,
  type RunnerGraphReceipt,
  type RunnerReceipt,
} from "./graph-governance.js";
import { readResumedSelectedRunner, resolveInputs } from "./inputs.js";
import { defaultReceiptDir } from "./receipt-paths.js";
export { defaultReceiptDir } from "./receipt-paths.js";
import type { RegistryStore } from "./registry-resolver.js";
import type { ExecutionGraph, ValidatedSkill } from "../parser-types.js";
import {
  approvalReceiptMetadata,
  approveSandboxEscalationIfNeeded,
  withSandboxApproval,
  writePolicyDeniedReceipt,
} from "./approval.js";
import {
  materializeInlineGraph,
  resolveSkillReference,
  resolveSkillRunner,
  type OfficialSkillResolver,
} from "./execution-targets.js";
import { projectReflectIfEnabled, type PostRunReflectPolicy } from "./reflect.js";
import {
  indexReceiptIfEnabled,
  isAgentMediatedSource,
  mergeMetadata,
  runnerTrustMetadata,
  type RetryReceiptContext,
} from "./runner-helpers.js";
import {
  authorityProofMetadataViaKernel,
  createSingleStepStateViaKernel,
  credentialBindingViaKernel,
  localScopeAdmissionViaKernel,
  localSkillAdmissionViaKernel,
  transitionSingleStepViaKernel,
  type GraphScopeGrant,
  type LocalAdmissionGrant,
  type SequentialGraphState,
  type SingleStepState,
} from "./kernel-bridge.js";
import {
  resolvePostRunReflectPolicyViaParser,
  validateSkillMarkdownViaParser,
} from "./parser-bridge.js";

export interface ApprovalDecision {
  readonly gate: ApprovalGate;
  readonly approved: boolean;
}

export interface ExecutionEvent {
  readonly type:
    | "skill_loaded"
    | "inputs_resolved"
    | "auth_resolved"
    | "resolution_requested"
    | "resolution_resolved"
    | "admitted"
    | "executing"
    | "step_started"
    | "step_waiting_resolution"
    | "step_completed"
    | "warning"
    | "completed";
  readonly message: string;
  readonly data?: unknown;
}

export interface Caller {
  // A Caller is the interaction surface attached to the kernel. It presents
  // questions, approvals, and progress to a host and returns structured
  // answers, but it does not execute skills itself.
  readonly resolve: (request: ResolutionRequest) => Promise<ResolutionResponse | undefined>;
  readonly report: (event: ExecutionEvent) => void | Promise<void>;
}

export interface RunLocalSkillOptions {
  readonly skillPath: string;
  readonly runId?: string;
  readonly inputs?: Readonly<Record<string, unknown>>;
  readonly answersPath?: string;
  readonly caller: Caller;
  readonly env?: NodeJS.ProcessEnv;
  readonly receiptDir?: string;
  readonly runxHome?: string;
  readonly parentReceipt?: string;
  readonly contextFrom?: readonly string[];
  readonly adapters?: readonly SkillAdapter[];
  readonly allowedSourceTypes?: readonly string[];
  readonly runner?: string;
  readonly knowledgeDir?: string;
  readonly authResolver?: AuthResolver;
  readonly receiptMetadata?: Readonly<Record<string, unknown>>;
  readonly resumeFromRunId?: string;
  readonly executionSemantics?: ExecutionSemantics;
  readonly registryStore?: RegistryStore;
  readonly officialSkillResolver?: OfficialSkillResolver;
  readonly skillCacheDir?: string;
  readonly toolCatalogAdapters?: readonly ToolCatalogAdapter[];
  readonly context?: Context;
  readonly voiceProfile?: ContextDocument;
  readonly voiceProfilePath?: string;
  readonly workspacePolicy?: RunxWorkspacePolicy;
  readonly lineage?: RunLineageMetadata;
}

export interface RunValidatedSkillOptions {
  readonly skill: ValidatedSkill;
  readonly skillDirectory: string;
  readonly requestedSkillPath: string;
  readonly runId?: string;
  readonly inputs: Readonly<Record<string, unknown>>;
  readonly caller: Caller;
  readonly env?: NodeJS.ProcessEnv;
  readonly receiptDir?: string;
  readonly runxHome?: string;
  readonly knowledgeDir?: string;
  readonly parentReceipt?: string;
  readonly contextFrom?: readonly string[];
  readonly adapters?: readonly SkillAdapter[];
  readonly allowedSourceTypes?: readonly string[];
  readonly authResolver?: AuthResolver;
  readonly receiptMetadata?: Readonly<Record<string, unknown>>;
  readonly resumeFromRunId?: string;
  readonly skillPathForMissingContext?: string;
  readonly orchestrationRunId?: string;
  readonly orchestrationStepId?: string;
  readonly currentContext?: readonly MaterializedContextEdge[];
  readonly executionSemantics?: ExecutionSemantics;
  readonly registryStore?: RegistryStore;
  readonly officialSkillResolver?: OfficialSkillResolver;
  readonly skillCacheDir?: string;
  readonly toolCatalogAdapters?: readonly ToolCatalogAdapter[];
  readonly context?: Context;
  readonly voiceProfile?: ContextDocument;
  readonly voiceProfilePath?: string;
  readonly selectedRunnerName?: string;
  readonly authorityMutating?: boolean;
  readonly authorityScopeAdmission?: ScopeAdmissionContract;
  readonly workspacePolicy?: RunxWorkspacePolicy;
  readonly lineage?: RunLineageMetadata;
}

export interface AuthResolver {
  readonly resolveGrants: (request: AuthGrantRequest) => Promise<AuthGrantResolution | undefined>;
  readonly resolveCredential: (request: AuthCredentialRequest) => Promise<AuthCredentialResolution | undefined>;
}

export interface AuthGrantRequest {
  readonly skill: ValidatedSkill;
  readonly inputs: Readonly<Record<string, unknown>>;
}

export interface AuthGrantResolution {
  readonly grants: readonly LocalAdmissionGrant[];
}

export interface AuthCredentialRequest extends AuthGrantRequest {
  readonly grants: readonly LocalAdmissionGrant[];
}

export interface AuthCredentialResolution {
  readonly credential?: CredentialEnvelope;
  readonly receiptMetadata?: Readonly<Record<string, unknown>>;
}

export interface RunLineageMetadata {
  readonly kind: "rerun";
  readonly sourceRunId: string;
  readonly sourceReceiptId?: string;
}

function runxReceiptMetadata(options: {
  readonly requestedSkillPath: string;
  readonly resolvedSkillPath: string;
  readonly selectedRunnerName?: string;
  readonly lineage?: RunLineageMetadata;
}): Readonly<Record<string, unknown>> {
  return {
    runx: {
      skill_ref: {
        requested_path: options.requestedSkillPath,
        resolved_path: options.resolvedSkillPath,
      },
      selected_runner: options.selectedRunnerName,
      lineage: options.lineage
        ? {
            kind: options.lineage.kind,
            source_run_id: options.lineage.sourceRunId,
            source_receipt_id: options.lineage.sourceReceiptId,
          }
        : undefined,
    },
  };
}

export type RunLocalSkillResult =
  | {
      readonly status: "needs_agent";
      readonly skill: ValidatedSkill;
      readonly skillPath: string;
      readonly inputs: Readonly<Record<string, unknown>>;
      readonly runId: string;
      readonly requests: readonly ResolutionRequest[];
      readonly stepIds?: readonly string[];
      readonly stepLabels?: readonly string[];
    }
  | {
      readonly status: "policy_denied";
      readonly skill: ValidatedSkill;
      readonly reasons: readonly string[];
      readonly approval?: ApprovalDecision;
      readonly receipt?: RunnerReceipt;
    }
  | {
      readonly status: "sealed" | "failure";
      readonly skill: ValidatedSkill;
      readonly inputs: Readonly<Record<string, unknown>>;
      readonly execution: ActReceiptEnvelope;
      readonly state: SingleStepState;
      readonly receipt: RunnerReceipt;
    };

export interface RunLocalGraphOptions {
  readonly graphPath?: string;
  readonly graph?: ExecutionGraph;
  readonly graphDirectory?: string;
  readonly inputs?: Readonly<Record<string, unknown>>;
  readonly caller: Caller;
  readonly env?: NodeJS.ProcessEnv;
  readonly receiptDir?: string;
  readonly runxHome?: string;
  readonly adapters?: readonly SkillAdapter[];
  readonly allowedSourceTypes?: readonly string[];
  readonly authResolver?: AuthResolver;
  readonly graphGrant?: GraphScopeGrant;
  readonly runId?: string;
  readonly stepRunIds?: Readonly<Record<string, string>>;
  readonly skillEnvironment?: {
    readonly name: string;
    readonly body: string;
  };
  readonly resumeFromRunId?: string;
  readonly executionSemantics?: ExecutionSemantics;
  readonly registryStore?: RegistryStore;
  readonly skillCacheDir?: string;
  readonly officialSkillResolver?: OfficialSkillResolver;
  readonly toolCatalogAdapters?: readonly ToolCatalogAdapter[];
  readonly receiptMetadata?: Readonly<Record<string, unknown>>;
  readonly context?: Context;
  readonly voiceProfile?: ContextDocument;
  readonly voiceProfilePath?: string;
  readonly knowledgeDir?: string;
  readonly selectedRunnerName?: string;
  readonly postRunReflectPolicy?: PostRunReflectPolicy;
  readonly workspacePolicy?: RunxWorkspacePolicy;
}

export interface GraphStepRun {
  readonly stepId: string;
  readonly skill: string;
  readonly skillPath: string;
  readonly runner?: string;
  readonly attempt: number;
  readonly status: "sealed" | "failure";
  readonly receiptId?: string;
  readonly stdout: string;
  readonly stderr: string;
  readonly parentReceipt?: string;
  readonly fanoutGroup?: string;
  readonly retry?: RetryReceiptContext;
  readonly contextFrom: readonly {
    readonly input: string;
    readonly fromStep: string;
    readonly output: string;
    readonly receiptId?: string;
  }[];
  readonly governance?: GraphStepGovernance;
  readonly artifactIds?: readonly string[];
  readonly disposition?: GovernedDisposition;
  readonly inputContext?: ReceiptInputContext;
  readonly outcomeState?: OutcomeState;
  readonly outcome?: ReceiptOutcome;
  readonly surfaceRefs?: readonly ReceiptSurfaceRef[];
  readonly evidenceRefs?: readonly ReceiptSurfaceRef[];
}

export type RunLocalGraphResult =
  | {
      readonly status: "needs_agent";
      readonly graph: ExecutionGraph;
      readonly skillPath: string;
      readonly stepIds: readonly string[];
      readonly requests: readonly ResolutionRequest[];
      readonly skill: ValidatedSkill;
      readonly state: SequentialGraphState;
      readonly runId: string;
      readonly stepLabels?: readonly string[];
    }
  | {
      readonly status: "policy_denied";
      readonly graph: ExecutionGraph;
      readonly stepId: string;
      readonly skill: ValidatedSkill;
      readonly reasons: readonly string[];
      readonly state: SequentialGraphState;
      readonly receipt?: RunnerGraphReceipt;
    }
  | {
      readonly status: "sealed" | "failure" | "escalated";
      readonly graph: ExecutionGraph;
      readonly state: SequentialGraphState;
      readonly steps: readonly GraphStepRun[];
      readonly receipt: RunnerGraphReceipt;
      readonly output: string;
      readonly errorMessage?: string;
    };

export async function runLocalSkill(options: RunLocalSkillOptions): Promise<RunLocalSkillResult> {
  const runId = options.runId ?? options.resumeFromRunId ?? uniqueRunnerReceiptId("rx");
  const workspacePolicy = options.workspacePolicy ?? await loadRunxWorkspacePolicy(options.env ?? process.env);
  const resolvedSkill = await resolveSkillReference(options.skillPath);
  const rawMarkdown = await readFile(resolvedSkill.skillPath, "utf8");
  const resumedRunnerName =
    options.runner || !options.resumeFromRunId
      ? undefined
      : await readResumedSelectedRunner(options.receiptDir ?? defaultReceiptDir(options.env), options.resumeFromRunId);
  const runnerSelection = await resolveSkillRunner(
    await validateSkillMarkdownViaParser(rawMarkdown, { mode: "strict" }, { env: options.env }),
    resolvedSkill.skillPath,
    options.runner ?? resumedRunnerName,
    { env: options.env },
  );
  const skill = runnerSelection.skill;

  await options.caller.report({
    type: "skill_loaded",
    message: `Loaded skill ${skill.name}.`,
    data: { skillPath: resolvedSkill.skillPath, requestedPath: resolvedSkill.requestedPath },
  });

  const inputResolution = await resolveInputs(skill, options);
  if (inputResolution.status === "needs_agent") {
    const pendingResult = {
      status: "needs_agent",
      skill,
      skillPath: resolvedSkill.skillPath,
      inputs: options.inputs ?? {},
      runId,
      requests: [inputResolution.request],
    } satisfies Extract<RunLocalSkillResult, { readonly status: "needs_agent" }>;
    await appendPendingSkillLedgerEntries({
      receiptDir: options.receiptDir ?? defaultReceiptDir(options.env),
      runId: pendingResult.runId,
      skill,
      startedAt: new Date().toISOString(),
      kind: "resolution_requested",
      detail: {
        skill_path: resolvedSkill.requestedPath,
        resolved_path: resolvedSkill.skillPath,
        selected_runner: runnerSelection.selectedRunnerName,
        request_ids: [inputResolution.request.id],
        resolution_kinds: [inputResolution.request.kind],
        requests: [inputResolution.request],
        step_ids: [],
        step_labels: [],
        inputs: pendingResult.inputs,
        lineage: options.lineage
          ? {
              kind: options.lineage.kind,
              source_run_id: options.lineage.sourceRunId,
              source_receipt_id: options.lineage.sourceReceiptId,
            }
          : undefined,
      },
      includeRunStarted: !options.resumeFromRunId,
    });
    return pendingResult;
  }

  await options.caller.report({
    type: "inputs_resolved",
    message: `Resolved ${Object.keys(inputResolution.inputs).length} input(s).`,
  });

  const result = await runValidatedSkill({
    skill,
    skillDirectory: resolvedSkill.skillDirectory,
    runId,
    inputs: inputResolution.inputs,
    caller: options.caller,
    env: options.env,
    receiptDir: options.receiptDir,
    runxHome: options.runxHome,
    knowledgeDir: options.knowledgeDir,
    parentReceipt: options.parentReceipt,
    contextFrom: options.contextFrom,
    adapters: options.adapters,
    allowedSourceTypes: options.allowedSourceTypes,
    authResolver: options.authResolver,
    receiptMetadata: options.receiptMetadata,
    resumeFromRunId: options.resumeFromRunId,
    requestedSkillPath: resolvedSkill.requestedPath,
    skillPathForMissingContext: resolvedSkill.skillPath,
    executionSemantics: options.executionSemantics,
    registryStore: options.registryStore,
    officialSkillResolver: options.officialSkillResolver,
    skillCacheDir: options.skillCacheDir,
    toolCatalogAdapters: options.toolCatalogAdapters,
    context: options.context,
    voiceProfile: options.voiceProfile,
    voiceProfilePath: options.voiceProfilePath,
    selectedRunnerName: runnerSelection.selectedRunnerName,
    workspacePolicy,
    lineage: options.lineage,
  });

  if (result.status === "needs_agent") {
    const pendingResult = {
      ...result,
      inputs: inputResolution.inputs,
    } satisfies Extract<RunLocalSkillResult, { readonly status: "needs_agent" }>;
    await appendPendingSkillLedgerEntries({
      receiptDir: options.receiptDir ?? defaultReceiptDir(options.env),
      runId: pendingResult.runId,
      skill,
      startedAt: new Date().toISOString(),
      kind: "resolution_requested",
      detail: {
        skill_path: resolvedSkill.requestedPath,
        resolved_path: resolvedSkill.skillPath,
        selected_runner: runnerSelection.selectedRunnerName,
        request_ids: pendingResult.requests.map((request) => request.id),
        resolution_kinds: Array.from(new Set(pendingResult.requests.map((request) => request.kind))),
        requests: pendingResult.requests,
        step_ids: pendingResult.stepIds ?? [],
        step_labels: pendingResult.stepLabels ?? [],
        inputs: pendingResult.inputs,
        lineage: options.lineage
          ? {
              kind: options.lineage.kind,
              source_run_id: options.lineage.sourceRunId,
              source_receipt_id: options.lineage.sourceReceiptId,
            }
          : undefined,
      },
      includeRunStarted: !options.resumeFromRunId,
    });
    return pendingResult;
  }

  return result;
}

export async function runValidatedSkill(options: RunValidatedSkillOptions): Promise<RunLocalSkillResult> {
  const { skill } = options;
  const runId = options.runId ?? options.resumeFromRunId ?? uniqueRunnerReceiptId("rx");
  const contextEnvelopeRunId = options.orchestrationRunId ?? runId;
  const authorityMutating = options.authorityMutating ?? skill.mutating === true;
  const workspacePolicy = options.workspacePolicy ?? await loadRunxWorkspacePolicy(options.env ?? process.env);
  const contextSnapshot =
    options.context
    ?? (await loadContext({
      inputs: options.inputs,
      env: options.env,
      fallbackStart: options.skillDirectory,
    }));
  const voiceProfile =
    options.voiceProfile
    ?? (await loadVoiceProfile({
      env: options.env,
      voiceProfilePath: options.voiceProfilePath,
    }));
  const inheritedReceiptMetadata = mergeMetadata(
    runxReceiptMetadata({
      requestedSkillPath: options.requestedSkillPath,
      resolvedSkillPath: options.skillPathForMissingContext ?? options.requestedSkillPath,
      selectedRunnerName: options.selectedRunnerName,
      lineage: options.lineage,
    }),
    contextReceiptMetadata(contextSnapshot),
    voiceProfileReceiptMetadata(voiceProfile),
    skillQualityProfileReceiptMetadata(skill),
    options.receiptMetadata,
  );
  const executionSemantics = normalizeExecutionSemantics(
    mergeExecutionSemantics(skill.execution, options.executionSemantics),
    options.inputs,
  );

  const structuralAdmission = await localSkillAdmissionViaKernel(skill, {
    allowedSourceTypes: options.allowedSourceTypes,
    skipConnectedAuth: true,
    skipSandboxEscalation: true,
    executionPolicy: workspacePolicy,
  }, { env: options.env });
  if (structuralAdmission.status === "deny") {
    const structuralScopeAdmission = await localScopeAdmissionViaKernel(skill.auth, [], {
      deniedBeforeGrantResolution: true,
    }, { env: options.env });
    const receipt = await writePolicyDeniedReceipt({
      skill,
      inputs: options.inputs,
      reasons: structuralAdmission.reasons,
      runOptions: options,
      receiptMetadata: mergeMetadata(
        inheritedReceiptMetadata,
        await authorityProofMetadataViaKernel({
          runId,
          skillName: skill.name,
          sourceType: skill.source.type,
          auth: skill.auth,
          grants: [],
          scopeAdmission: structuralScopeAdmission,
          sandboxDeclaration: skill.source.sandbox,
          mutating: authorityMutating,
        }, { env: options.env }),
      ),
      executionSemantics,
    });
    return {
      status: "policy_denied",
      skill,
      reasons: structuralAdmission.reasons,
      receipt,
    };
  }

  const grantResolution = await options.authResolver?.resolveGrants({
    skill,
    inputs: options.inputs,
  });
  const scopeAdmission = await localScopeAdmissionViaKernel(skill.auth, grantResolution?.grants ?? [], {}, { env: options.env });
  const authorityProofScopeAdmission = isNoConnectedAuthScopeAdmission(scopeAdmission)
    ? options.authorityScopeAdmission ?? scopeAdmission
    : scopeAdmission;
  if (grantResolution) {
    await options.caller.report({
      type: "auth_resolved",
      message: `Resolved ${grantResolution.grants.length} auth grant(s).`,
    });
  }

  const sandboxApproval = await approveSandboxEscalationIfNeeded(skill, options.caller);
  const approvedSandboxEscalation = sandboxApproval?.approved ?? false;

  const admission = await localSkillAdmissionViaKernel(skill, {
    allowedSourceTypes: options.allowedSourceTypes,
    connectedGrants: grantResolution?.grants,
    approvedSandboxEscalation,
    executionPolicy: workspacePolicy,
  }, { env: options.env });
  if (admission.status === "deny") {
    const receipt = await writePolicyDeniedReceipt({
      skill,
      inputs: options.inputs,
      reasons: admission.reasons,
      approval: sandboxApproval && !sandboxApproval.approved ? sandboxApproval : undefined,
      runOptions: options,
      receiptMetadata: mergeMetadata(
        inheritedReceiptMetadata,
        await authorityProofMetadataViaKernel({
          runId,
          skillName: skill.name,
          sourceType: skill.source.type,
          auth: skill.auth,
          grants: grantResolution?.grants ?? [],
          scopeAdmission: authorityProofScopeAdmission,
          sandboxDeclaration: skill.source.sandbox,
          approval: sandboxApproval,
          mutating: authorityMutating,
        }, { env: options.env }),
      ),
      executionSemantics,
    });
    return {
      status: "policy_denied",
      skill,
      reasons: admission.reasons,
      approval: sandboxApproval && !sandboxApproval.approved ? sandboxApproval : undefined,
      receipt,
    };
  }

  await options.caller.report({
    type: "admitted",
    message: "Local policy admitted skill execution.",
  });

  if (skill.source.type === "graph" && skill.source.graph) {
    await options.caller.report({
      type: "executing",
      message: "Executing graph skill source.",
    });

    const graphRunId = options.resumeFromRunId ?? uniqueRunnerReceiptId("gx");
    const graphResult = await runLocalGraph({
      graph: materializeInlineGraph(skill),
      graphDirectory: options.skillDirectory,
      inputs: options.inputs,
      caller: options.caller,
      env: options.env,
      receiptDir: options.receiptDir,
      runxHome: options.runxHome,
      knowledgeDir: options.knowledgeDir,
      adapters: options.adapters,
      allowedSourceTypes: options.allowedSourceTypes,
      authResolver: options.authResolver,
      runId: graphRunId,
      skillEnvironment: {
        name: skill.name,
        body: skill.body,
      },
      resumeFromRunId: options.resumeFromRunId,
      executionSemantics: mergeExecutionSemantics(skill.execution, options.executionSemantics),
      registryStore: options.registryStore,
      officialSkillResolver: options.officialSkillResolver,
      skillCacheDir: options.skillCacheDir,
      toolCatalogAdapters: options.toolCatalogAdapters,
      receiptMetadata: mergeMetadata(
        inheritedReceiptMetadata,
        await authorityProofMetadataViaKernel({
          runId: graphRunId,
          skillName: skill.name,
          sourceType: skill.source.type,
          auth: skill.auth,
          grants: grantResolution?.grants ?? [],
          scopeAdmission,
          sandboxDeclaration: skill.source.sandbox,
          approval: sandboxApproval,
          mutating: authorityMutating,
        }, { env: options.env }),
      ),
      context: contextSnapshot,
      voiceProfile,
      voiceProfilePath: options.voiceProfilePath,
      workspacePolicy,
      selectedRunnerName: options.selectedRunnerName,
      postRunReflectPolicy: await resolvePostRunReflectPolicyViaParser(skill.runx, { env: options.env }),
    });

    if (graphResult.status === "needs_agent") {
      return {
        status: "needs_agent",
        skill,
        skillPath: options.skillPathForMissingContext ?? options.skillDirectory,
        inputs: options.inputs,
        runId: graphResult.runId,
        requests: graphResult.requests,
        stepIds: graphResult.stepIds,
        stepLabels: graphResult.stepLabels,
      };
    }

    if (graphResult.status === "policy_denied") {
      return {
        status: "policy_denied",
        skill,
        reasons: graphResult.reasons,
        receipt: graphResult.receipt,
      };
    }

    let state = await createSingleStepStateViaKernel(skill.name, { env: options.env });
    state = await transitionSingleStepViaKernel(state, { type: "admit" }, { env: options.env });
    state = await transitionSingleStepViaKernel(
      state,
      { type: "start", at: runnerReceiptStartedAt(graphResult.receipt) ?? new Date().toISOString() },
      { env: options.env },
    );
    if (graphResult.status === "sealed") {
      state = await transitionSingleStepViaKernel(
        state,
        {
          type: "succeed",
          at: runnerReceiptCompletedAt(graphResult.receipt) ?? new Date().toISOString(),
          admissionWitness: { stepId: skill.name, receiptId: graphResult.receipt.id },
        },
        { env: options.env },
      );
    } else {
      state = await transitionSingleStepViaKernel(
        state,
        {
          type: "fail",
          at: runnerReceiptCompletedAt(graphResult.receipt) ?? new Date().toISOString(),
          error: graphResult.errorMessage ?? "graph execution failed",
        },
        { env: options.env },
      );
    }

    await options.caller.report({
      type: "completed",
      message: `Skill execution ${graphResult.status}.`,
      data: {
        receiptId: graphResult.receipt.id,
      },
    });

    const skillExecutionStatus = graphResult.status === "sealed" ? "sealed" : "failure";
    return {
      status: skillExecutionStatus,
      skill,
      inputs: options.inputs,
      execution: {
        status: skillExecutionStatus,
        stdout: graphResult.output,
        stderr: graphResult.errorMessage ?? "",
        exitCode: skillExecutionStatus === "sealed" ? 0 : 1,
        signal: null,
        durationMs: runnerReceiptDurationMs(graphResult.receipt) ?? 0,
        errorMessage: graphResult.errorMessage,
        metadata: {
          composite: {
            graph_receipt_id: graphResult.receipt.id,
            graph_status: graphResult.status,
            top_level_skill: skill.name,
          },
        },
      },
      state,
      receipt: graphResult.receipt,
    };
  }

  let state = await createSingleStepStateViaKernel(skill.name, { env: options.env });
  state = await transitionSingleStepViaKernel(state, { type: "admit" }, { env: options.env });
  const startedAt = new Date().toISOString();
  const preparedAgentContext = await prepareAgentContext({
    skill,
    inputs: options.inputs,
    env: options.env,
    receiptDir: options.receiptDir ?? defaultReceiptDir(options.env),
    runxHome: options.runxHome,
    runId: contextEnvelopeRunId,
    stepId: options.orchestrationStepId,
    currentContext: options.currentContext,
    skillDirectory: options.skillDirectory,
    context: contextSnapshot,
    voiceProfile,
    voiceProfilePath: options.voiceProfilePath,
  });

  const admittedCredentialGrants = admittedGrantsForCredential(grantResolution?.grants ?? [], scopeAdmission);
  const credentialResolution = await options.authResolver?.resolveCredential({
    skill,
    inputs: options.inputs,
    grants: admittedCredentialGrants,
  });
  const credentialBinding = await credentialBindingViaKernel({
    auth: skill.auth,
    grants: admittedCredentialGrants,
    scopeAdmission,
    credential: credentialResolution?.credential,
  }, { env: options.env });
  if (credentialBinding.status === "deny") {
    const receipt = await writePolicyDeniedReceipt({
      skill,
      inputs: options.inputs,
      reasons: credentialBinding.reasons,
      runOptions: options,
      receiptMetadata: mergeMetadata(
        inheritedReceiptMetadata,
        credentialResolution?.receiptMetadata,
        preparedAgentContext.receiptMetadata,
        sandboxApproval ? approvalReceiptMetadata(sandboxApproval) : undefined,
        await authorityProofMetadataViaKernel({
          runId,
          skillName: skill.name,
          sourceType: skill.source.type,
          auth: skill.auth,
          grants: grantResolution?.grants ?? [],
          scopeAdmission: authorityProofScopeAdmission,
          credential: credentialResolution?.credential,
          sandboxDeclaration: skill.source.sandbox,
          approval: sandboxApproval,
          mutating: authorityMutating,
        }, { env: options.env }),
      ),
      executionSemantics,
    });
    return {
      status: "policy_denied",
      skill,
      reasons: credentialBinding.reasons,
      receipt,
    };
  }

  await options.caller.report({
    type: "executing",
    message: `Executing ${skill.source.type} skill source.`,
  });

  const nestedSkillInvoker = async (
    nested: NestedSkillInvocation,
  ): Promise<NestedSkillInvocationResult> => {
    const nestedInputResolution = await resolveInputs(nested.skill, {
      skillPath: nested.requestedSkillPath,
      inputs: nested.inputs,
      caller: options.caller,
      env: options.env,
      receiptDir: options.receiptDir,
      runxHome: options.runxHome,
      knowledgeDir: options.knowledgeDir,
      adapters: options.adapters,
      allowedSourceTypes: options.allowedSourceTypes,
      authResolver: options.authResolver,
      registryStore: options.registryStore,
      officialSkillResolver: options.officialSkillResolver,
      skillCacheDir: options.skillCacheDir,
      toolCatalogAdapters: options.toolCatalogAdapters,
      context: options.context,
      voiceProfile: options.voiceProfile,
      voiceProfilePath: options.voiceProfilePath,
      workspacePolicy,
    });
    if (nestedInputResolution.status === "needs_agent") {
      return {
        status: "needs_agent",
        request: nestedInputResolution.request,
      };
    }

    const nestedResult = await runValidatedSkill({
      skill: nested.skill,
      skillDirectory: nested.skillDirectory,
      requestedSkillPath: nested.requestedSkillPath,
      inputs: nestedInputResolution.inputs,
      caller: options.caller,
      env: options.env,
      receiptDir: options.receiptDir,
      runxHome: options.runxHome,
      knowledgeDir: options.knowledgeDir,
      adapters: options.adapters,
      allowedSourceTypes: options.allowedSourceTypes,
      authResolver: options.authResolver,
      receiptMetadata: mergeMetadata(
        {
          runx: {
            parent_run_id: runId,
          },
        },
        nested.receiptMetadata,
      ),
      registryStore: options.registryStore,
      officialSkillResolver: options.officialSkillResolver,
      skillCacheDir: options.skillCacheDir,
      toolCatalogAdapters: options.toolCatalogAdapters,
      workspacePolicy,
    });

    if (nestedResult.status === "needs_agent") {
      const request = nestedResult.requests[0];
      if (!request) {
        throw new Error(`Nested managed-tool execution for '${nested.requestedSkillPath}' requested resolution without a request payload.`);
      }
      return {
        status: "needs_agent",
        request,
      };
    }

    if (nestedResult.status === "policy_denied") {
      return {
        status: "policy_denied",
        reasons: nestedResult.reasons,
        receiptId: nestedResult.receipt?.id,
        errorMessage: nestedResult.reasons.join("; "),
      };
    }

    return {
      status: nestedResult.status,
      stdout: nestedResult.execution.stdout,
      stderr: nestedResult.execution.stderr,
      exitCode: nestedResult.execution.exitCode,
      signal: nestedResult.execution.signal,
      durationMs: nestedResult.execution.durationMs,
      errorMessage: nestedResult.execution.errorMessage,
      receiptId: nestedResult.receipt.id,
    };
  };

  const executionSkill = withSandboxApproval(skill, approvedSandboxEscalation);

  const execution = await executeSkill({
    skill: executionSkill,
    inputs: options.inputs,
    skillDirectory: options.skillDirectory,
    adapters: [
      ...(options.adapters ?? []),
      createCallerAgentAdapter(options.caller),
      createCallerAgentStepAdapter(options.caller),
      createCallerApprovalAdapter(options.caller),
    ],
    env: options.env,
    credential: credentialResolution?.credential,
    allowedTools: executionSkill.allowedTools,
    runId: contextEnvelopeRunId,
    stepId: options.orchestrationStepId,
    currentContext: preparedAgentContext.currentContext,
    historicalContext: preparedAgentContext.historicalContext,
    contextProvenance: preparedAgentContext.provenance,
    context: preparedAgentContext.context,
    voiceProfile: preparedAgentContext.voiceProfile,
    qualityProfile: qualityProfileContext(skill),
    nestedSkillInvoker,
    toolCatalogAdapters: options.toolCatalogAdapters,
  });

  if (execution.status === "needs_agent") {
    return {
      status: "needs_agent",
      skill,
      skillPath: options.skillPathForMissingContext ?? options.skillDirectory,
      inputs: options.inputs,
      runId,
      requests: [execution.request],
    };
  }

  state = await transitionSingleStepViaKernel(state, { type: "start", at: startedAt }, { env: options.env });
  const completedAt = new Date().toISOString();
  if (execution.status === "sealed") {
    state = await transitionSingleStepViaKernel(
      state,
      {
        type: "succeed",
        at: completedAt,
        admissionWitness: { stepId: skill.name, receiptId: runId },
      },
      { env: options.env },
    );
  } else {
    state = await transitionSingleStepViaKernel(
      state,
      {
        type: "fail",
        at: completedAt,
        error: execution.errorMessage ?? execution.stderr,
      },
      { env: options.env },
    );
  }

  const artifactResult = materializeArtifacts({
    stdout: execution.stdout,
    contract: skill.artifacts,
    runId,
    producer: {
      skill: skill.name,
      runner: skill.source.type,
    },
    createdAt: completedAt,
  });
  const verificationRefs = paymentRailProofVerificationRefs(artifactResult.envelopes);
  const receiptDir = options.receiptDir ?? defaultReceiptDir(options.env);
  const skillLedgerEntries = buildSkillLedgerEntries({
    runId,
    skill,
    startedAt,
    completedAt,
    status: execution.status,
    artifactEnvelopes: artifactResult.envelopes,
    receiptId: runId,
    includeRunStarted: !options.resumeFromRunId,
    runStartedDetail: {
      skill_path: options.requestedSkillPath,
      resolved_path: options.skillPathForMissingContext ?? options.requestedSkillPath,
      selected_runner: options.selectedRunnerName,
      inputs: options.inputs,
      lineage: options.lineage
        ? {
            kind: options.lineage.kind,
            source_run_id: options.lineage.sourceRunId,
            source_receipt_id: options.lineage.sourceReceiptId,
          }
        : undefined,
    },
  });
  const ledgerPlan = await prepareLedgerAppend({
    receiptDir,
    runId,
    entries: skillLedgerEntries,
  });

  await appendPreparedLedgerEntries(ledgerPlan);
  const receipt = await writeRunnerSkillReceipt({
    receiptId: runId,
    receiptDir,
    runxHome: options.runxHome ?? options.env?.RUNX_HOME,
    skillName: skill.name,
    sourceType: skill.source.type,
    inputs: options.inputs,
    stdout: execution.stdout,
    stderr: execution.stderr,
    execution: {
      status: execution.status,
      exitCode: execution.exitCode,
      signal: execution.signal,
      durationMs: execution.durationMs,
      errorMessage: execution.errorMessage,
      metadata: mergeMetadata(
        runnerTrustMetadata(skill.source.type),
        execution.metadata,
        credentialResolution?.receiptMetadata,
        preparedAgentContext.receiptMetadata,
        sandboxApproval ? approvalReceiptMetadata(sandboxApproval) : undefined,
        inheritedReceiptMetadata,
        await authorityProofMetadataViaKernel({
          runId,
          skillName: skill.name,
          sourceType: skill.source.type,
          auth: skill.auth,
          grants: grantResolution?.grants ?? [],
          scopeAdmission: authorityProofScopeAdmission,
          credential: credentialResolution?.credential,
          sandboxDeclaration: skill.source.sandbox,
          sandboxMetadata: execution.metadata?.sandbox,
          approval: sandboxApproval,
          mutating: authorityMutating,
        }, { env: options.env }),
        createLedgerAnchorMetadata(ledgerPlan.anchor),
      ),
    },
    startedAt,
    completedAt,
    parentReceipt: options.parentReceipt,
    contextFrom: options.contextFrom,
    artifactIds: artifactResult.envelopes.map((envelope) => envelope.meta.artifact_id),
    disposition: executionSemantics.disposition,
    inputContext: executionSemantics.inputContext,
    outcomeState: executionSemantics.outcomeState,
    outcome: executionSemantics.outcome,
    surfaceRefs: executionSemantics.surfaceRefs,
    evidenceRefs: executionSemantics.evidenceRefs,
    verificationRefs,
  });
  try {
    await indexReceiptIfEnabled(receipt, receiptDir, options);
  } catch (error) {
    await options.caller.report({
      type: "warning",
      message: "Local knowledge indexing failed after receipt write; continuing with the persisted receipt.",
      data: {
        receiptId: receipt.id,
        error: errorMessage(error),
      },
    });
  }
  await projectReflectIfEnabled({
    caller: options.caller,
    receipt,
    receiptDir: options.receiptDir ?? defaultReceiptDir(options.env),
    runId,
    skillName: skill.name,
    knowledgeDir: options.knowledgeDir,
    env: options.env,
    selectedRunnerName: options.selectedRunnerName,
    postRunReflectPolicy: await resolvePostRunReflectPolicyViaParser(skill.runx, { env: options.env }),
    involvedAgentMediatedWork: isAgentMediatedSource(skill.source.type),
  });

  await options.caller.report({
    type: "completed",
    message: `Skill execution ${execution.status}.`,
  });

  return {
    status: execution.status,
    skill,
    inputs: options.inputs,
    execution,
    state,
    receipt,
  };
}

function paymentRailProofVerificationRefs(envelopes: readonly ArtifactEnvelope[]): readonly ReferenceContract[] {
  return envelopes.flatMap((envelope) => {
    const railProof = findRailProof(envelope.data);
    const proofRef = stringField(railProof, "proof_ref");
    if (!proofRef) {
      return [];
    }
    const reference: ReferenceContract = {
      type: "verification",
      uri: proofRef,
      label: "payment rail proof",
      proof_kind: "payment_rail",
    };
    const idempotencyKey = stringField(railProof, "idempotency_key");
    return idempotencyKey ? [{ ...reference, locator: idempotencyKey }] : [reference];
  });
}

function findRailProof(value: unknown): Readonly<Record<string, unknown>> | undefined {
  if (!isRecord(value)) {
    return undefined;
  }
  const direct = recordField(value, "rail_proof");
  if (direct) {
    return direct;
  }
  const packet = recordField(value, "payment_rail_packet");
  const packetData = recordField(packet, "data") ?? packet;
  return recordField(packetData, "rail_proof");
}

function recordField(value: unknown, field: string): Readonly<Record<string, unknown>> | undefined {
  if (!isRecord(value)) {
    return undefined;
  }
  const fieldValue = value[field];
  return isRecord(fieldValue) ? fieldValue : undefined;
}

function stringField(value: unknown, field: string): string | undefined {
  if (!isRecord(value)) {
    return undefined;
  }
  const fieldValue = value[field];
  return typeof fieldValue === "string" && fieldValue.length > 0 ? fieldValue : undefined;
}

function admittedGrantsForCredential(
  grants: readonly LocalAdmissionGrant[],
  scopeAdmission: { readonly status: "allow" | "deny"; readonly grant_id?: string },
): readonly LocalAdmissionGrant[] {
  if (scopeAdmission.status !== "allow" || !scopeAdmission.grant_id) {
    return [];
  }
  return grants.filter((grant) => grant.grant_id === scopeAdmission.grant_id);
}

function isNoConnectedAuthScopeAdmission(scopeAdmission: ScopeAdmissionContract): boolean {
  return scopeAdmission.status === "allow"
    && scopeAdmission.requested_scopes.length === 0
    && scopeAdmission.granted_scopes.length === 0
    && scopeAdmission.decision_summary === "no connected auth requested";
}
