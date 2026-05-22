import path from "node:path";

import type { ResolutionRequestContract as ResolutionRequest } from "@runxhq/contracts";

import type { Caller } from "./index.js";

export interface GraphStep {
  readonly id: string;
  readonly label?: string;
  readonly skill?: string;
  readonly tool?: string;
  readonly run?: Readonly<Record<string, unknown>>;
  readonly runner?: string;
}

export function graphStepExecutionDirectory(step: GraphStep, stepExecutablePath: string, graphDirectory: string): string {
  if (step.tool) {
    return path.dirname(stepExecutablePath);
  }
  if (step.skill) {
    return stepExecutablePath;
  }
  return graphDirectory;
}

export function graphStepReference(step: GraphStep): string {
  return step.skill ?? step.tool ?? `run:${String(step.run?.type ?? "unknown")}`;
}

export function graphStepRunner(step: GraphStep): string | undefined {
  if (step.tool) {
    return "tool";
  }
  return typeof step.run?.type === "string" ? step.run.type : step.runner;
}

export function graphProducerSkillName(skillEnvironmentName: string | undefined, graphName: string): string {
  return skillEnvironmentName ?? graphName;
}

export async function reportGraphStepStarted(caller: Caller, step: GraphStep, reference: string): Promise<void> {
  await caller.report({
    type: "step_started",
    message: `Starting step ${step.id}.`,
    data: {
      stepId: step.id,
      stepLabel: step.label,
      skill: reference,
      runner: graphStepRunner(step) ?? "default",
    },
  });
}

export async function reportGraphStepWaitingResolution(
  caller: Caller,
  step: GraphStep,
  reference: string,
  requests: readonly ResolutionRequest[],
): Promise<void> {
  await caller.report({
    type: "step_waiting_resolution",
    message: `Step ${step.id} needs agent.`,
    data: {
      stepId: step.id,
      stepLabel: step.label,
      skill: reference,
      runner: graphStepRunner(step) ?? "default",
      kinds: Array.from(new Set(requests.map((request) => request.kind))),
      requestIds: requests.map((request) => request.id),
      resolutionSkills: Array.from(
        new Set(
          requests
            .filter((request): request is Extract<ResolutionRequest, { kind: "agent_act" }> => request.kind === "agent_act")
            .map((request) => request.invocation.envelope.skill),
        ),
      ),
      expectedOutputs: Array.from(
        new Set(
          requests
            .filter((request): request is Extract<ResolutionRequest, { kind: "agent_act" }> => request.kind === "agent_act")
            .flatMap((request) => Object.keys(request.invocation.envelope.output ?? {})),
        ),
      ),
    },
  });
}

export async function reportGraphStepCompleted(
  caller: Caller,
  step: GraphStep,
  reference: string,
  status: "sealed" | "failure",
  detail?: Readonly<Record<string, unknown>>,
): Promise<void> {
  await caller.report({
    type: "step_completed",
    message: `Step ${step.id} ${status}.`,
    data: {
      stepId: step.id,
      stepLabel: step.label,
      skill: reference,
      runner: graphStepRunner(step) ?? "default",
      status,
      ...detail,
    },
  });
}
