import type { ArtifactEnvelope } from "@runxhq/core/artifacts";
import { isRecord } from "@runxhq/core/util";

interface GraphContextEdge {
  readonly input: string;
  readonly fromStep: string;
  readonly output: string;
}

interface GraphContextStep {
  readonly id: string;
  readonly contextEdges: readonly GraphContextEdge[];
}

interface GraphContextGraph<TStep extends GraphContextStep = GraphContextStep> {
  readonly steps: readonly TStep[];
}

export interface GraphStepOutput {
  readonly status: "sealed" | "failure";
  readonly stdout: string;
  readonly stderr: string;
  readonly receiptId: string;
  readonly fields: Readonly<Record<string, unknown>>;
  readonly artifactIds: readonly string[];
  readonly artifacts: readonly ArtifactEnvelope[];
}

export interface MaterializedContextEdge {
  readonly input: string;
  readonly fromStep: string;
  readonly output: string;
  readonly receiptId?: string;
  readonly artifact?: ArtifactEnvelope;
  readonly value: unknown;
}

export function findGraphStep<TGraph extends GraphContextGraph>(
  graph: TGraph,
  stepId: string,
): TGraph["steps"][number] {
  const step = graph.steps.find((candidate) => candidate.id === stepId);
  if (!step) {
    throw new Error(`Graph step '${stepId}' is missing.`);
  }
  return step;
}

export function materializeContext(
  step: GraphContextStep,
  outputs: ReadonlyMap<string, GraphStepOutput>,
): readonly MaterializedContextEdge[] {
  return step.contextEdges.map((edge) => {
    const sourceOutput = outputs.get(edge.fromStep);
    if (!sourceOutput) {
      throw new Error(`Step '${step.id}' is missing context output from '${edge.fromStep}'.`);
    }

    return {
      input: edge.input,
      fromStep: edge.fromStep,
      output: edge.output,
      receiptId: sourceOutput.receiptId,
      artifact: resolveOutputArtifact(sourceOutput, edge.output),
      value: resolveOutputPath(sourceOutput, edge.output),
    };
  });
}

export function materializeStepInputs(
  stepInputs: Readonly<Record<string, unknown>>,
  graphInputs: Readonly<Record<string, unknown>>,
): Readonly<Record<string, unknown>> {
  return resolveGraphInputReferences(stepInputs, graphInputs) as Readonly<Record<string, unknown>>;
}

export function resolveOutputPath(output: GraphStepOutput, outputPath: string): unknown {
  const record: Record<string, unknown> = {
    ...output.fields,
    status: output.status,
    stdout: output.stdout,
    stderr: output.stderr,
    receipt_id: output.receiptId,
    receiptId: output.receiptId,
  };

  return outputPath.split(".").reduce<unknown>((value, key) => {
    if (!isRecord(value)) {
      throw new Error(`Context output path '${outputPath}' was not produced by the source step.`);
    }

    if (key in value) {
      return value[key];
    }

    throw new Error(`Context output path '${outputPath}' was not produced by the source step.`);
  }, record);
}

function resolveGraphInputReferences(value: unknown, graphInputs: Readonly<Record<string, unknown>>): unknown {
  if (typeof value === "string") {
    if (!value.startsWith("$input.")) {
      return value;
    }
    return resolveInputPath(graphInputs, value.slice("$input.".length));
  }
  if (Array.isArray(value)) {
    return value.map((entry) => resolveGraphInputReferences(entry, graphInputs));
  }
  if (isRecord(value)) {
    return Object.fromEntries(
      Object.entries(value).map(([key, entry]) => [key, resolveGraphInputReferences(entry, graphInputs)]),
    );
  }
  return value;
}

function resolveInputPath(inputs: Readonly<Record<string, unknown>>, inputPath: string): unknown {
  if (!inputPath) {
    return undefined;
  }
  return inputPath.split(".").reduce<unknown>((value, key) => {
    if (!isRecord(value) || !(key in value)) {
      return undefined;
    }
    return value[key];
  }, inputs);
}

function resolveOutputArtifact(output: GraphStepOutput, outputPath: string): ArtifactEnvelope | undefined {
  const [field] = outputPath.split(".", 1);
  if (!field) {
    return undefined;
  }
  const candidate = output.fields[field];
  return isArtifactEnvelopeValue(candidate) ? candidate : undefined;
}

function isArtifactEnvelopeValue(value: unknown): value is ArtifactEnvelope {
  return typeof value === "object"
    && value !== null
    && !Array.isArray(value)
    && typeof (value as { version?: unknown }).version === "string"
    && typeof (value as { meta?: { artifact_id?: unknown; run_id?: unknown } }).meta?.artifact_id === "string"
    && typeof (value as { meta?: { artifact_id?: unknown; run_id?: unknown } }).meta?.run_id === "string"
    && "data" in value;
}
