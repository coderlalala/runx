import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import {
  createSequentialGraphState,
  createSingleStepState,
  evaluateFanoutSync,
  fanoutSyncDecisionKey,
  planSequentialGraphTransition,
  transitionSequentialGraph,
  transitionSingleStep,
  type FanoutBranchResult,
  type FanoutGroupPolicy,
  type SequentialGraphEvent,
  type SequentialGraphState,
  type SequentialGraphStepDefinition,
  type SingleStepEvent,
  type SingleStepState,
} from "../packages/core/src/state-machine/index.js";
import {
  admitGraphStepScopes,
  admitLocalSkill,
  admitRetryPolicy,
  buildAuthorityProofMetadata,
  buildLocalScopeAdmission,
  evaluatePublicCommentOpportunity,
  evaluatePublicPullRequestCandidate,
  normalizePublicWorkPolicy,
  validateCredentialBinding,
  type AuthorityProofGrant,
  type GraphScopeAdmissionRequest,
  type LocalAdmissionOptions,
  type LocalAdmissionSkill,
  type RetryAdmissionRequest,
} from "../packages/core/src/policy/index.js";
import {
  admitSandbox,
  normalizeSandboxDeclaration,
  sandboxRequiresApproval,
  type SandboxDeclaration,
} from "../packages/core/src/policy/sandbox.js";

const workspaceRoot = path.resolve(fileURLToPath(new URL("..", import.meta.url)));
const fixtureRoot = path.join(workspaceRoot, "fixtures", "kernel");
const schemaRoot = path.join(fixtureRoot, "schema");
const kernelFixtureSchemaRefs = {
  "../schema/policy.schema.json": "policy.schema.json",
  "../schema/state-machine.schema.json": "state-machine.schema.json",
} as const satisfies Record<KernelFixture["$schema"], string>;

export interface KernelFixture {
  readonly $schema: "../schema/state-machine.schema.json" | "../schema/policy.schema.json";
  readonly name: string;
  readonly description?: string;
  readonly input: Readonly<Record<string, unknown>> & { readonly kind: string };
  readonly expected:
    | {
        readonly kind: "output";
        readonly value: unknown;
      }
    | {
        readonly kind: "error";
        readonly code: string;
        readonly message?: string;
      };
}

export class KernelFixtureEvaluationError extends Error {
  readonly code = "kernel.fixture.evaluation_failed";
  readonly sourceErrorName?: string;
  readonly sourceErrorMessage?: string;

  constructor(error: unknown) {
    super("kernel fixture evaluation failed", { cause: error });
    this.name = "KernelFixtureEvaluationError";
    this.sourceErrorName = error instanceof Error ? error.name : undefined;
    this.sourceErrorMessage = error instanceof Error ? error.message : String(error);
  }
}

interface KernelFixtureCase {
  readonly name: string;
  readonly description?: string;
  readonly input: KernelFixture["input"];
  readonly expected?: KernelFixture["expected"];
}

interface PaymentAuthorityBoundsFixture {
  readonly currency: string;
  readonly max_per_call_minor?: number;
  readonly max_per_run_minor?: number;
  readonly max_per_period_minor?: number;
  readonly period?: string;
  readonly rails: readonly string[];
  readonly realm?: string;
  readonly counterparty?: string;
  readonly operation?: string;
  readonly quote_ttl_ms?: number;
  readonly approval_threshold_minor?: number;
  readonly credential_form?: "single_use_spend_capability";
  readonly quote_required?: boolean;
  readonly reservation_required?: boolean;
  readonly idempotency_required?: boolean;
  readonly recovery_required?: boolean;
  readonly receipt_before_success?: boolean;
  readonly single_use_spend?: boolean;
}

interface PaymentAuthorityTermFixture {
  readonly term_id: string;
  readonly principal_ref: Readonly<Record<string, unknown>>;
  readonly resource_ref: Readonly<Record<string, unknown>>;
  readonly resource_family: string;
  readonly verbs: readonly string[];
  readonly bounds: {
    readonly payment?: PaymentAuthorityBoundsFixture;
  };
  readonly conditions: readonly unknown[];
  readonly approvals: readonly unknown[];
  readonly capabilities: readonly string[];
  readonly expires_at?: string;
  readonly issued_by_ref: Readonly<Record<string, unknown>>;
  readonly credential_ref?: Readonly<Record<string, unknown>>;
}

interface ValidationResult {
  readonly valid: boolean;
  readonly errors: readonly string[];
}

type JsonSchema = Readonly<Record<string, unknown>>;

const supportedJsonSchemaKeywords = new Set([
  "$id",
  "$schema",
  "additionalProperties",
  "anyOf",
  "const",
  "items",
  "oneOf",
  "pattern",
  "properties",
  "required",
  "type",
]);

export function buildKernelParityFixtures(): readonly KernelFixture[] {
  return fixtureCases()
    .map(
      (fixtureCase) =>
        normalizeForFixture({
          $schema: fixtureCase.input.kind.startsWith("state-machine.")
            ? "../schema/state-machine.schema.json"
            : "../schema/policy.schema.json",
          name: fixtureCase.name,
          description: fixtureCase.description,
          input: fixtureCase.input,
          expected: fixtureCase.expected ?? {
            kind: "output",
            value: evaluateKernelFixtureInput(fixtureCase.input),
          },
        }) as KernelFixture,
    )
    .sort((left, right) => left.name.localeCompare(right.name));
}

export async function collectKernelFixtureFiles(root: string = fixtureRoot): Promise<readonly string[]> {
  const files: string[] = [];
  for (const directoryName of ["policy", "runner", "state-machine"]) {
    const directory = path.join(root, directoryName);
    let entries: readonly string[] = [];
    try {
      entries = await readdir(directory);
    } catch (error) {
      if (!isNodeError(error) || error.code !== "ENOENT") {
        throw error;
      }
      continue;
    }
    files.push(
      ...entries
        .filter((entry) => entry.endsWith(".json"))
        .map((entry) => path.join(directory, entry)),
    );
  }
  return files.sort();
}

export async function readKernelFixture(filePath: string): Promise<KernelFixture> {
  return JSON.parse(await readFile(filePath, "utf8")) as KernelFixture;
}

export async function validateKernelFixture(fixture: KernelFixture): Promise<ValidationResult> {
  const envelopeSchema = await readJsonSchema(path.join(schemaRoot, "fixture.schema.json"));
  const schemaFile = kernelFixtureSchemaFile(fixture.$schema);
  if (!schemaFile) {
    return {
      valid: false,
      errors: [`fixture.$schema: unsupported kernel fixture schema ref '${fixture.$schema}'`],
    };
  }
  const concreteSchema = await readJsonSchema(path.join(schemaRoot, schemaFile));
  const errors = [
    ...schemaErrors(envelopeSchema, fixture, "fixture.schema.json"),
    ...schemaErrors(concreteSchema, fixture, schemaFile),
    ...runnerFixtureErrors(fixture),
  ];
  return {
    valid: errors.length === 0,
    errors,
  };
}

export function isRunnerKernelFixture(fixture: KernelFixture): boolean {
  return fixture.expected.kind === "error" && fixture.expected.code === "kernel.fixture.evaluation_failed";
}

function kernelFixtureSchemaFile(schemaRef: unknown): string | undefined {
  return typeof schemaRef === "string" && Object.hasOwn(kernelFixtureSchemaRefs, schemaRef)
    ? kernelFixtureSchemaRefs[schemaRef as keyof typeof kernelFixtureSchemaRefs]
    : undefined;
}

function runnerFixtureErrors(fixture: KernelFixture): readonly string[] {
  const isRunnerFixture = isRunnerKernelFixture(fixture);
  if (isRunnerFixture && !fixture.name.startsWith("runner-")) {
    return [`fixture.name: runner ingestion fixtures must use the 'runner-' prefix`];
  }
  if (!isRunnerFixture && fixture.name.startsWith("runner-")) {
    return [`fixture.name: only kernel.fixture.evaluation_failed error fixtures may use the 'runner-' prefix`];
  }
  return [];
}

export function evaluateKernelFixtureInput(input: KernelFixture["input"]): unknown {
  try {
    return evaluateKernelFixtureInputUnchecked(input);
  } catch (error) {
    throw new KernelFixtureEvaluationError(error);
  }
}

function evaluateKernelFixtureInputUnchecked(input: KernelFixture["input"]): unknown {
  switch (input.kind) {
    case "state-machine.createSingleStepState":
      return createSingleStepState(expectString(input.stepId, "input.stepId"));
    case "state-machine.transitionSingleStep":
      return transitionSingleStep(
        expectRecord(input.state, "input.state") as unknown as SingleStepState,
        expectRecord(input.event, "input.event") as unknown as SingleStepEvent,
      );
    case "state-machine.createSequentialGraphState":
      return createSequentialGraphState(
        expectString(input.graphId, "input.graphId"),
        expectArray(input.steps, "input.steps") as readonly SequentialGraphStepDefinition[],
      );
    case "state-machine.planSequentialGraphTransition":
      return planSequentialGraphTransition(
        expectRecord(input.state, "input.state") as unknown as SequentialGraphState,
        expectArray(input.steps, "input.steps") as readonly SequentialGraphStepDefinition[],
        (input.fanoutPolicies ?? {}) as Readonly<Record<string, FanoutGroupPolicy>>,
        {
          resolvedFanoutGateKeys: input.resolvedFanoutGateKeys
            ? new Set(expectArray(input.resolvedFanoutGateKeys, "input.resolvedFanoutGateKeys") as readonly string[])
            : undefined,
        },
      );
    case "state-machine.transitionSequentialGraph":
      return transitionSequentialGraph(
        expectRecord(input.state, "input.state") as unknown as SequentialGraphState,
        expectRecord(input.event, "input.event") as unknown as SequentialGraphEvent,
      );
    case "state-machine.evaluateFanoutSync":
      return evaluateFanoutSync(
        expectRecord(input.policy, "input.policy") as unknown as FanoutGroupPolicy,
        expectArray(input.results, "input.results") as readonly FanoutBranchResult[],
        {
          resolvedGateKeys: input.resolvedGateKeys
            ? new Set(expectArray(input.resolvedGateKeys, "input.resolvedGateKeys") as readonly string[])
            : undefined,
        },
      );
    case "state-machine.fanoutSyncDecisionKey":
      return fanoutSyncDecisionKey(expectRecord(input.decision, "input.decision") as { readonly groupId: string; readonly ruleFired: string });
    case "policy.admitLocalSkill":
      return admitLocalSkill(
        expectRecord(input.skill, "input.skill") as unknown as LocalAdmissionSkill,
        (input.options ?? {}) as LocalAdmissionOptions,
      );
    case "policy.admitRetryPolicy":
      return admitRetryPolicy(expectRecord(input.request, "input.request") as unknown as RetryAdmissionRequest);
    case "policy.admitGraphStepScopes":
      return admitGraphStepScopes(expectRecord(input.request, "input.request") as unknown as GraphScopeAdmissionRequest);
    case "policy.normalizeSandboxDeclaration":
      return normalizeSandboxDeclaration(input.sandbox as SandboxDeclaration | undefined);
    case "policy.sandboxRequiresApproval":
      return sandboxRequiresApproval(input.sandbox as SandboxDeclaration | undefined);
    case "policy.admitSandbox":
      return admitSandbox(input.sandbox as SandboxDeclaration | undefined, (input.options ?? {}) as { readonly approvedEscalation?: boolean; readonly skipEscalation?: boolean });
    case "policy.buildLocalScopeAdmission":
      return buildLocalScopeAdmission(input.auth, expectArray(input.grants ?? [], "input.grants") as never[], (input.options ?? {}) as never);
    case "policy.buildAuthorityProofMetadata":
      return buildAuthorityProofMetadata(expectRecord(input.options, "input.options") as never);
    case "policy.validateCredentialBinding":
      return validateCredentialBinding(expectRecord(input.request, "input.request") as never);
    case "policy.evaluatePublicPullRequestCandidate":
      return evaluatePublicPullRequestCandidate(expectRecord(input.request, "input.request") as never, (input.policy ?? {}) as never);
    case "policy.evaluatePublicCommentOpportunity":
      return evaluatePublicCommentOpportunity(expectRecord(input.request, "input.request") as never, (input.policy ?? {}) as never);
    case "policy.normalizePublicWorkPolicy":
      return normalizePublicWorkPolicy((input.policy ?? {}) as never);
    case "policy.isPaymentAuthoritySubset":
      return isPaymentAuthoritySubset(
        expectRecord(input.child, "input.child") as unknown as PaymentAuthorityTermFixture,
        expectRecord(input.parent, "input.parent") as unknown as PaymentAuthorityTermFixture,
      );
    default:
      throw new Error(`unknown kernel fixture input kind: ${input.kind}`);
  }
}

export function normalizeForFixture(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map((item) => normalizeForFixture(item));
  }
  if (!isPlainRecord(value)) {
    return value;
  }

  const output: Record<string, unknown> = {};
  for (const key of Object.keys(value).sort()) {
    const normalized = normalizeForFixture(value[key]);
    if (normalized !== undefined) {
      output[key] = normalized;
    }
  }
  return output;
}

export function stableFixtureJson(value: unknown): string {
  return `${JSON.stringify(normalizeForFixture(value), null, 2)}\n`;
}

async function main(): Promise<void> {
  const check = process.argv.includes("--check");
  const fixtures = buildKernelParityFixtures();
  const expectedFiles = new Set<string>();

  for (const fixture of fixtures) {
    const directory = fixtureDirectory(fixture);
    const filePath = path.join(directory, `${fixture.name}.json`);
    expectedFiles.add(filePath);
    const content = stableFixtureJson(fixture);
    if (check) {
      let existing = "";
      try {
        existing = await readFile(filePath, "utf8");
      } catch (error) {
        if (isNodeError(error) && error.code === "ENOENT") {
          throw new Error(`missing fixture ${path.relative(workspaceRoot, filePath)}`);
        }
        throw error;
      }
      if (existing !== content) {
        throw new Error(`fixture is stale: ${path.relative(workspaceRoot, filePath)}`);
      }
      continue;
    }
    await mkdir(directory, { recursive: true });
    await writeFile(filePath, content);
  }

  if (check) {
    for (const filePath of await collectKernelFixtureFiles()) {
      if (!expectedFiles.has(filePath)) {
        throw new Error(`stale fixture file: ${path.relative(workspaceRoot, filePath)}`);
      }
    }
  }

  console.log(`${check ? "checked" : "generated"} ${fixtures.length} kernel parity fixtures`);
}

function fixtureCases(): readonly KernelFixtureCase[] {
  const linearSteps: readonly SequentialGraphStepDefinition[] = [
    { id: "first" },
    { id: "second", contextFrom: ["first"] },
  ];
  const fanoutSteps: readonly SequentialGraphStepDefinition[] = [
    { id: "market", fanoutGroup: "advisors" },
    { id: "risk", fanoutGroup: "advisors" },
    { id: "finance", fanoutGroup: "advisors" },
    { id: "synthesize", contextFrom: ["market", "risk"] },
  ];
  const quorumPolicy: FanoutGroupPolicy = {
    groupId: "advisors",
    strategy: "quorum",
    minSuccess: 2,
    onBranchFailure: "continue",
    thresholdGates: [],
    conflictGates: [],
  };
  const pendingGraph = createSequentialGraphState("gx_fixture", linearSteps);
  const startedGraph = transitionSequentialGraph(pendingGraph, {
    type: "start_step",
    stepId: "first",
    at: "2026-04-10T00:00:00.000Z",
  });
  const failedOnceGraph = transitionSequentialGraph(startedGraph, {
    type: "step_failed",
    stepId: "first",
    at: "2026-04-10T00:00:01.000Z",
    error: "boom",
  });
  const fanoutPending = createSequentialGraphState("gx_fanout", fanoutSteps);
  let thresholdResolvedState = createSequentialGraphState("gx_threshold_resolved", fanoutSteps.slice(0, 3));
  thresholdResolvedState = finishFanoutStep(thresholdResolvedState, "market", "succeeded", { recommendation: "go" });
  thresholdResolvedState = finishFanoutStep(thresholdResolvedState, "risk", "succeeded", { risk_score: 0.91 });
  thresholdResolvedState = finishFanoutStep(thresholdResolvedState, "finance", "succeeded", { budget: "approved" });
  const conflictState = finishFanoutStep(
    finishFanoutStep(createSequentialGraphState("gx_conflict", fanoutSteps.slice(0, 2)), "market", "succeeded", { report: "ship" }),
    "risk",
    "succeeded",
    { report: "hold" },
  );
  const githubReadAuth = {
    type: "nango",
    provider: "github",
    scopes: ["repo:read", "repo:read"],
    scope_family: "github_repo",
    authority_kind: "read_only",
    target_repo: "runxhq/aster",
    target_locator: "runxhq/aster#issue/4",
  };
  const githubReadGrant: AuthorityProofGrant = {
    grant_id: "grant_expected",
    provider: "github",
    scopes: ["repo:read", "user:read"],
    status: "active",
    scope_family: "github_repo",
    authority_kind: "read_only",
    target_repo: "runxhq/aster",
    target_locator: "runxhq/aster#issue/4",
  };
  const githubCredential = {
    kind: "runx.credential-envelope.v1",
    grant_id: "grant_expected",
    provider: "github",
    connection_id: "conn_1",
    scopes: ["repo:read"],
    grant_reference: {
      grant_id: "grant_expected",
      scope_family: "github_repo",
      authority_kind: "read_only",
      target_repo: "runxhq/aster",
      target_locator: "runxhq/aster#issue/4",
    },
    material_ref: "nango:github:conn_1",
  };

  return [
    {
      name: "authority-credential-binding-allows-matching",
      input: {
        kind: "policy.validateCredentialBinding",
        request: {
          auth: githubReadAuth,
          grants: [githubReadGrant],
          scopeAdmission: buildLocalScopeAdmission(githubReadAuth, [githubReadGrant]),
          credential: githubCredential,
        },
      },
    },
    {
      name: "authority-credential-binding-denies-grant-reference",
      input: {
        kind: "policy.validateCredentialBinding",
        request: {
          auth: githubReadAuth,
          grants: [githubReadGrant],
          scopeAdmission: buildLocalScopeAdmission(githubReadAuth, [githubReadGrant]),
          credential: {
            ...githubCredential,
            grant_id: "grant_other",
            grant_reference: {
              ...githubCredential.grant_reference,
              grant_id: "grant_other",
            },
          },
        },
      },
    },
    {
      name: "authority-proof-metadata-full",
      input: {
        kind: "policy.buildAuthorityProofMetadata",
        options: {
          runId: "run_policy_fixture",
          skillName: "issue-intake",
          sourceType: "agent-step",
          auth: githubReadAuth,
          grants: [githubReadGrant],
          credential: githubCredential,
          sandboxDeclaration: {
            profile: "workspace-write",
            cwdPolicy: "workspace",
            network: false,
            requireEnforcement: true,
          },
          sandboxMetadata: {
            profile: "workspace-write",
            cwd_policy: "workspace",
            require_enforcement: true,
            network: {
              declared: false,
              enforcement: "isolated-namespace",
            },
            filesystem: {
              enforcement: "bubblewrap-mount-namespace",
              readonly_paths: false,
              writable_paths_enforced: true,
              private_tmp: true,
            },
            runtime: {
              enforcer: "bubblewrap",
              reason: "fixture",
            },
          },
          approval: {
            gate: {
              id: "approval_1",
              type: "human",
              reason: "mutating github action",
            },
            approved: true,
          },
          mutating: true,
        },
      },
    },
    {
      name: "authority-proof-prunes-empty-sandbox-objects",
      input: {
        kind: "policy.buildAuthorityProofMetadata",
        options: {
          runId: "run_policy_fixture",
          skillName: "issue-intake",
          sourceType: "agent-step",
          auth: githubReadAuth,
          grants: [githubReadGrant],
          credential: githubCredential,
          sandboxMetadata: {
            profile: "workspace-write",
            network: {},
            filesystem: {},
            runtime: {},
          },
          mutating: false,
        },
      },
    },
    {
      name: "authority-proof-trims-sandbox-declaration",
      input: {
        kind: "policy.buildAuthorityProofMetadata",
        options: {
          runId: "run_policy_fixture",
          skillName: "issue-intake",
          sourceType: "agent-step",
          auth: githubReadAuth,
          grants: [githubReadGrant],
          credential: githubCredential,
          sandboxDeclaration: {
            profile: "  workspace-write  ",
            cwdPolicy: "  workspace  ",
            network: false,
            requireEnforcement: true,
          },
          mutating: false,
        },
      },
    },
    {
      name: "authority-scope-admission-active-grant",
      input: {
        kind: "policy.buildLocalScopeAdmission",
        auth: githubReadAuth,
        grants: [githubReadGrant],
      },
    },
    {
      name: "authority-scope-admission-denied-before-grant",
      input: {
        kind: "policy.buildLocalScopeAdmission",
        auth: githubReadAuth,
        grants: [githubReadGrant],
        options: {
          deniedBeforeGrantResolution: true,
        },
      },
    },
    {
      name: "authority-scope-admission-no-connected-auth",
      input: {
        kind: "policy.buildLocalScopeAdmission",
        auth: {
          type: "env",
        },
        grants: [githubReadGrant],
      },
    },
    {
      name: "authority-scope-admission-no-matching-grant",
      input: {
        kind: "policy.buildLocalScopeAdmission",
        auth: {
          type: "nango",
          provider: "github",
          scopes: ["repo:write"],
        },
        grants: [githubReadGrant],
      },
    },
    ...paymentAuthorityFixtureCases(),
    {
      name: "public-work-blocks-dependency-bot-pr",
      input: {
        kind: "policy.evaluatePublicPullRequestCandidate",
        request: {
          authorLogin: "dependabot[bot]",
          title: "Bump react from 19.0.0 to 19.0.1",
          labels: ["dependencies"],
          headRefName: "dependabot/npm_and_yarn/react-19.0.1",
        },
      },
    },
    {
      name: "public-work-blocks-hyphen-version-title",
      input: {
        kind: "policy.evaluatePublicPullRequestCandidate",
        request: {
          authorLogin: "maintainer",
          title: "upgrade abc-1.2",
          labels: [],
          headRefName: "feature/upgrade-abc",
        },
      },
    },
    {
      name: "public-work-denies-cold-comment",
      input: {
        kind: "policy.evaluatePublicCommentOpportunity",
        request: {
          source: "github_pull_request",
          lane: "issue-triage",
          authorLogin: "stranger",
          authorAssociation: "NONE",
          title: "Clarify docs wording",
          labels: [],
          headRefName: "docs/fix-wording",
          commentsCount: 0,
          reviewCommentsCount: 0,
        },
      },
    },
    {
      name: "public-work-denies-trust-recovery",
      input: {
        kind: "policy.evaluatePublicCommentOpportunity",
        request: {
          source: "github_pull_request",
          lane: "issue-triage",
          authorLogin: "maintainer",
          authorAssociation: "CONTRIBUTOR",
          title: "Improve onboarding docs",
          labels: [],
          headRefName: "docs/onboarding",
          commentsCount: 1,
          reviewCommentsCount: 0,
          recentOutcomes: [{ status: "cooldown" }],
        },
        policy: {
          trust_recovery_statuses: ["cooldown"],
        },
      },
    },
    {
      name: "public-work-normalizes-policy",
      input: {
        kind: "policy.normalizePublicWorkPolicy",
        policy: {
          blocked_author_patterns: ["  Team-Bot  "],
          blocked_exact_labels: [" Needs Review "],
          require_welcome_signal_for_pull_request_comments: false,
        },
      },
    },
    {
      name: "public-work-normalizes-empty-arrays",
      input: {
        kind: "policy.normalizePublicWorkPolicy",
        policy: {
          blocked_author_patterns: [],
          blocked_head_ref_prefixes: [],
          blocked_exact_labels: [],
          blocked_label_prefixes: [],
          trust_recovery_statuses: [],
        },
      },
    },
    {
      name: "single-step-create-pending",
      description: "Creates a pending single-step state.",
      input: {
        kind: "state-machine.createSingleStepState",
        stepId: "lint",
      },
    },
    {
      name: "single-step-transition-succeed",
      description: "Completes a running single-step state.",
      input: {
        kind: "state-machine.transitionSingleStep",
        state: {
          stepId: "lint",
          status: "running",
          startedAt: "2026-04-10T00:00:00.000Z",
        },
        event: {
          type: "succeed",
          at: "2026-04-10T00:00:01.000Z",
        },
      },
    },
    {
      name: "single-step-transition-ignores-invalid-event",
      description: "Invalid status/event pairs return the current state.",
      input: {
        kind: "state-machine.transitionSingleStep",
        state: {
          stepId: "lint",
          status: "pending",
        },
        event: {
          type: "succeed",
          at: "2026-04-10T00:00:01.000Z",
        },
      },
    },
    {
      name: "sequential-create-graph",
      input: {
        kind: "state-machine.createSequentialGraphState",
        graphId: "gx_fixture",
        steps: linearSteps,
      },
    },
    {
      name: "sequential-plan-first-step",
      input: {
        kind: "state-machine.planSequentialGraphTransition",
        state: pendingGraph,
        steps: linearSteps,
      },
    },
    {
      name: "sequential-transition-step-succeeded",
      input: {
        kind: "state-machine.transitionSequentialGraph",
        state: startedGraph,
        event: {
          type: "step_succeeded",
          stepId: "first",
          at: "2026-04-10T00:00:01.000Z",
          receiptId: "rx_first",
          outputs: {
            z: "last",
            a: "first",
          },
        },
      },
    },
    {
      name: "sequential-plan-retry-after-failure",
      input: {
        kind: "state-machine.planSequentialGraphTransition",
        state: failedOnceGraph,
        steps: [{ id: "first", retry: { maxAttempts: 2 } }],
      },
    },
    {
      name: "fanout-plan-branch-set",
      input: {
        kind: "state-machine.planSequentialGraphTransition",
        state: fanoutPending,
        steps: fanoutSteps,
        fanoutPolicies: {
          advisors: quorumPolicy,
        },
      },
    },
    {
      name: "fanout-evaluate-branch-failure-halts",
      input: {
        kind: "state-machine.evaluateFanoutSync",
        policy: {
          ...quorumPolicy,
          onBranchFailure: "halt",
        },
        results: [
          { stepId: "market", status: "succeeded" },
          { stepId: "risk", status: "succeeded" },
          { stepId: "finance", status: "failed" },
        ],
      },
    },
    {
      name: "fanout-evaluate-threshold-pause",
      input: {
        kind: "state-machine.evaluateFanoutSync",
        policy: {
          groupId: "advisors",
          strategy: "all",
          onBranchFailure: "halt",
          thresholdGates: [{ step: "risk", field: "risk_score", above: 0.8, action: "pause" }],
          conflictGates: [],
        },
        results: [
          { stepId: "market", status: "succeeded", outputs: { recommendation: "go" } },
          { stepId: "risk", status: "succeeded", outputs: { risk_score: 0.91 } },
        ],
      },
    },
    {
      name: "fanout-evaluate-resolved-threshold-proceeds",
      description: "Resolved threshold gates are skipped by evaluateFanoutSync.",
      input: {
        kind: "state-machine.evaluateFanoutSync",
        policy: {
          groupId: "advisors",
          strategy: "all",
          onBranchFailure: "halt",
          thresholdGates: [{ step: "risk", field: "risk_score", above: 0.8, action: "pause" }],
          conflictGates: [],
        },
        resolvedGateKeys: ["advisors:threshold.risk.risk_score.above"],
        results: [
          { stepId: "market", status: "succeeded", outputs: { recommendation: "go" } },
          { stepId: "risk", status: "succeeded", outputs: { risk_score: 0.91 } },
        ],
      },
    },
    {
      name: "fanout-plan-resolved-threshold-proceeds",
      description: "Resolved fanout gate keys let graph planning proceed past a prior pause.",
      input: {
        kind: "state-machine.planSequentialGraphTransition",
        state: thresholdResolvedState,
        steps: fanoutSteps.slice(0, 3),
        fanoutPolicies: {
          advisors: {
            groupId: "advisors",
            strategy: "all",
            onBranchFailure: "halt",
            thresholdGates: [{ step: "risk", field: "risk_score", above: 0.8, action: "pause" }],
            conflictGates: [],
          },
        },
        resolvedFanoutGateKeys: ["advisors:threshold.risk.risk_score.above"],
      },
    },
    {
      name: "fanout-plan-conflict-escalates",
      input: {
        kind: "state-machine.planSequentialGraphTransition",
        state: conflictState,
        steps: fanoutSteps.slice(0, 2),
        fanoutPolicies: {
          advisors: {
            groupId: "advisors",
            strategy: "all",
            onBranchFailure: "halt",
            thresholdGates: [],
            conflictGates: [{ field: "report", action: "escalate", steps: ["market", "risk"] }],
          },
        },
      },
    },
    {
      name: "fanout-decision-key",
      input: {
        kind: "state-machine.fanoutSyncDecisionKey",
        decision: {
          groupId: "advisors",
          ruleFired: "conflict.report",
        },
      },
    },
    {
      name: "local-admission-allows-cli-tool",
      input: {
        kind: "policy.admitLocalSkill",
        skill: {
          name: "echo",
          source: { type: "cli-tool", timeoutSeconds: 10 },
        },
      },
    },
    {
      name: "local-admission-denies-unsupported-source",
      input: {
        kind: "policy.admitLocalSkill",
        skill: {
          name: "unsupported",
          source: { type: "unsupported" },
        },
      },
    },
    {
      name: "local-admission-denies-inline-python-through-env",
      input: {
        kind: "policy.admitLocalSkill",
        skill: {
          name: "inline-python",
          source: {
            type: "cli-tool",
            command: "/usr/bin/env",
            args: ["PYTHONPATH=.", "python3", "-c", "print('hi')"],
          },
        },
        options: {
          executionPolicy: {
            strictCliToolInlineCode: true,
          },
        },
      },
    },
    {
      name: "local-admission-denies-inline-windows-path-interpreter",
      description: "Pins POSIX-only executable normalization for backslash-bearing commands.",
      input: {
        kind: "policy.admitLocalSkill",
        skill: {
          name: "inline-node-windows-path",
          source: {
            type: "cli-tool",
            command: "C:\\Tools\\node.exe",
            args: ["-e", "console.log('hi')"],
          },
        },
        options: {
          executionPolicy: {
            strictCliToolInlineCode: true,
          },
        },
      },
    },
    {
      name: "runner-rejects-missing-source",
      description:
        "Pins the fixture-runner ingestion error envelope for invalid but schema-shaped policy input; this is not a policy decision fixture.",
      input: {
        kind: "policy.admitLocalSkill",
        skill: {
          name: "missing-source",
        },
      },
      expected: {
        kind: "error",
        code: "kernel.fixture.evaluation_failed",
        message: "kernel fixture evaluation failed",
      },
    },
    {
      name: "local-admission-allows-connected-wildcard-grant",
      input: {
        kind: "policy.admitLocalSkill",
        skill: {
          name: "connected",
          source: { type: "cli-tool" },
          auth: { type: "nango", provider: "github", scopes: ["repo:read"] },
        },
        options: {
          connectedGrants: [
            {
              grant_id: "grant_wildcard",
              provider: "github",
              scopes: ["repo:*"],
              status: "active",
            },
          ],
        },
      },
    },
    {
      name: "local-admission-denies-connected-prefix-substring",
      input: {
        kind: "policy.admitLocalSkill",
        skill: {
          name: "connected-prefix-substring",
          source: { type: "cli-tool" },
          auth: { type: "nango", provider: "github", scopes: ["repository:read"] },
        },
        options: {
          connectedGrants: [
            {
              grant_id: "grant_repo_namespace",
              provider: "github",
              scopes: ["repo:*"],
              status: "active",
            },
          ],
        },
      },
    },
    {
      name: "sandbox-normalize-defaults",
      input: {
        kind: "policy.normalizeSandboxDeclaration",
      },
    },
    {
      name: "sandbox-denies-readonly-network",
      input: {
        kind: "policy.admitSandbox",
        sandbox: {
          profile: "readonly",
          network: true,
        },
      },
    },
    {
      name: "sandbox-requires-unrestricted-approval",
      input: {
        kind: "policy.admitSandbox",
        sandbox: {
          profile: "unrestricted-local-dev",
        },
      },
    },
    {
      name: "sandbox-requires-approval-boolean",
      input: {
        kind: "policy.sandboxRequiresApproval",
        sandbox: {
          profile: "unrestricted-local-dev",
        },
      },
    },
    {
      name: "retry-admission-allows-readonly-retry",
      input: {
        kind: "policy.admitRetryPolicy",
        request: {
          stepId: "read",
          retry: { maxAttempts: 2 },
          mutating: false,
        },
      },
    },
    {
      name: "retry-admission-denies-mutating-without-key",
      input: {
        kind: "policy.admitRetryPolicy",
        request: {
          stepId: "deploy",
          retry: { maxAttempts: 2 },
          mutating: true,
        },
      },
    },
    {
      name: "graph-scope-allows-exact-match",
      input: {
        kind: "policy.admitGraphStepScopes",
        request: {
          stepId: "read",
          requestedScopes: ["repo:read"],
          grant: { grant_id: "grant_1", scopes: ["repo:read"] },
        },
      },
    },
    {
      name: "graph-scope-allows-wildcard-narrowing",
      input: {
        kind: "policy.admitGraphStepScopes",
        request: {
          stepId: "checks",
          requestedScopes: ["checks:read"],
          grant: { scopes: ["checks:*", "repo:read"] },
        },
      },
    },
    {
      name: "graph-scope-allows-empty-request",
      input: {
        kind: "policy.admitGraphStepScopes",
        request: {
          stepId: "no-scope",
          requestedScopes: [],
          grant: { scopes: ["repo:read"] },
        },
      },
    },
    {
      name: "graph-scope-denies-widening",
      input: {
        kind: "policy.admitGraphStepScopes",
        request: {
          stepId: "deploy",
          requestedScopes: ["deployments:write"],
          grant: { grant_id: "grant_1", scopes: ["checks:read"] },
        },
      },
    },
    {
      name: "graph-scope-denies-empty-grant",
      input: {
        kind: "policy.admitGraphStepScopes",
        request: {
          stepId: "read",
          requestedScopes: ["repo:read"],
          grant: { scopes: [] },
        },
      },
    },
    {
      name: "graph-scope-denies-partial-widening",
      input: {
        kind: "policy.admitGraphStepScopes",
        request: {
          stepId: "deploy",
          requestedScopes: ["repo:read", "repo:write", "deploy:prod"],
          grant: { scopes: ["repo:*"] },
        },
      },
    },
    {
      name: "graph-scope-denies-prefix-wildcard-request",
      input: {
        kind: "policy.admitGraphStepScopes",
        request: {
          stepId: "read-all",
          requestedScopes: ["repo:*"],
          grant: { scopes: ["repo:read"] },
        },
      },
    },
    {
      name: "graph-scope-denies-prefix-substring",
      input: {
        kind: "policy.admitGraphStepScopes",
        request: {
          stepId: "repository-read",
          requestedScopes: ["repository:read"],
          grant: { scopes: ["repo:*"] },
        },
      },
    },
    {
      name: "graph-scope-deduplicates-requests",
      input: {
        kind: "policy.admitGraphStepScopes",
        request: {
          stepId: "read",
          requestedScopes: ["repo:read", "repo:read"],
          grant: { scopes: ["*"] },
        },
      },
    },
    {
      name: "graph-scope-omits-grant-id-when-absent",
      input: {
        kind: "policy.admitGraphStepScopes",
        request: {
          stepId: "read",
          requestedScopes: ["repo:read"],
          grant: { scopes: ["repo:read"] },
        },
      },
    },
  ];
}

function paymentAuthorityFixtureCases(): readonly KernelFixtureCase[] {
  const requiredCondition = {
    condition_id: "receipt-before-success",
    predicate: "payment_receipt_present",
    refs: [paymentReference("receipt", "receipt:rail:checkout")],
    parameters: {
      rail: "card",
    },
  };
  const requiredApproval = {
    approval_ref: paymentReference("decision", "decision:payment-approval"),
    approved_by_ref: paymentReference("principal", "principal:operator"),
    approved_at: "2026-05-20T00:00:00Z",
    criterion_ids: ["checkout-spend-approved"],
  };
  const parentPayment: PaymentAuthorityBoundsFixture = {
    currency: "USD",
    rails: ["card", "ach"],
    realm: "prod",
    counterparty: "merchant-123",
    operation: "checkout",
    max_per_call_minor: 10_000,
    max_per_run_minor: 25_000,
    max_per_period_minor: 50_000,
    period: "P1D",
    quote_required: true,
    reservation_required: true,
    idempotency_required: true,
    recovery_required: true,
    receipt_before_success: true,
    quote_ttl_ms: 300_000,
    approval_threshold_minor: 7_500,
    credential_form: "single_use_spend_capability",
    single_use_spend: true,
  };
  const narrowerPayment: PaymentAuthorityBoundsFixture = {
    ...parentPayment,
    rails: ["card"],
    max_per_call_minor: 2_500,
    max_per_run_minor: 10_000,
    max_per_period_minor: 20_000,
    quote_ttl_ms: 120_000,
    approval_threshold_minor: 2_500,
  };
  const parent = paymentAuthorityTerm({
    termId: "parent",
    verbs: ["quote", "reserve", "spend", "verify"],
    payment: parentPayment,
    expiresAt: "2026-06-01T00:00:00Z",
    conditions: [requiredCondition],
    approvals: [requiredApproval],
  });
  const narrowerChild = paymentAuthorityTerm({
    termId: "child",
    verbs: ["reserve", "spend"],
    payment: narrowerPayment,
    expiresAt: "2026-05-21T00:00:00Z",
    conditions: [requiredCondition],
    approvals: [requiredApproval],
  });
  const reserveQuoteParent = paymentAuthorityTerm({
    termId: "reserve-quote-parent",
    verbs: ["quote", "reserve"],
    payment: {
      currency: "USD",
      rails: ["card", "ach"],
      max_per_call_minor: 3_000,
      quote_required: true,
      reservation_required: true,
      quote_ttl_ms: 300_000,
    },
    expiresAt: "2026-05-21T00:00:00Z",
  });
  const reserveQuoteChild = paymentAuthorityTerm({
    termId: "reserve-quote-child",
    verbs: ["quote", "reserve"],
    payment: {
      currency: "USD",
      rails: ["card"],
      max_per_call_minor: 1_500,
      max_per_run_minor: 1_500,
      quote_required: true,
      reservation_required: true,
      quote_ttl_ms: 60_000,
    },
    expiresAt: "2026-05-21T00:00:00Z",
  });

  return [
    {
      name: "payment-authority-allows-narrower-child",
      input: paymentAuthorityInput(narrowerChild, parent),
    },
    {
      name: "payment-authority-allows-reserve-without-single-use-spend-capability",
      input: paymentAuthorityInput(reserveQuoteChild, reserveQuoteParent),
    },
    {
      name: "payment-authority-denies-currency-widening",
      input: paymentAuthorityInput(
        paymentAuthorityTerm({
          ...paymentAuthorityTermOptions(narrowerChild),
          termId: "currency-widening",
          payment: { ...narrowerPayment, currency: "EUR" },
        }),
        parent,
      ),
    },
    {
      name: "payment-authority-denies-dropping-receipt-before-success",
      input: paymentAuthorityInput(
        paymentAuthorityTerm({
          ...paymentAuthorityTermOptions(narrowerChild),
          termId: "missing-receipt-before-success",
          payment: { ...narrowerPayment, receipt_before_success: false },
        }),
        parent,
      ),
    },
    {
      name: "payment-authority-denies-omitted-counterparty",
      input: paymentAuthorityInput(
        paymentAuthorityTerm({
          ...paymentAuthorityTermOptions(narrowerChild),
          termId: "omitted-counterparty",
          payment: omitPaymentKey(narrowerPayment, "counterparty"),
        }),
        parent,
      ),
    },
    {
      name: "payment-authority-denies-omitted-operation",
      input: paymentAuthorityInput(
        paymentAuthorityTerm({
          ...paymentAuthorityTermOptions(narrowerChild),
          termId: "omitted-operation",
          payment: omitPaymentKey(narrowerPayment, "operation"),
        }),
        parent,
      ),
    },
    {
      name: "payment-authority-denies-omitted-period",
      input: paymentAuthorityInput(
        paymentAuthorityTerm({
          ...paymentAuthorityTermOptions(narrowerChild),
          termId: "omitted-period",
          payment: omitPaymentKey(narrowerPayment, "period"),
        }),
        parent,
      ),
    },
    {
      name: "payment-authority-denies-omitted-realm",
      input: paymentAuthorityInput(
        paymentAuthorityTerm({
          ...paymentAuthorityTermOptions(narrowerChild),
          termId: "omitted-realm",
          payment: omitPaymentKey(narrowerPayment, "realm"),
        }),
        parent,
      ),
    },
    {
      name: "payment-authority-denies-rail-widening",
      input: paymentAuthorityInput(
        paymentAuthorityTerm({
          ...paymentAuthorityTermOptions(narrowerChild),
          termId: "rail-widening",
          payment: { ...narrowerPayment, rails: ["card", "wire"] },
        }),
        parent,
      ),
    },
    {
      name: "payment-authority-denies-resource-family-mismatch",
      input: paymentAuthorityInput(
        paymentAuthorityTerm({
          ...paymentAuthorityTermOptions(narrowerChild),
          termId: "resource-family-mismatch",
          resourceFamily: "credential",
        }),
        parent,
      ),
    },
    {
      name: "payment-authority-denies-resource-ref-mismatch",
      input: paymentAuthorityInput(
        paymentAuthorityTerm({
          ...paymentAuthorityTermOptions(narrowerChild),
          termId: "resource-ref-mismatch",
          resourceUri: "grant:payment:other-merchant",
        }),
        parent,
      ),
    },
    {
      name: "payment-authority-denies-single-use-spend-without-capability",
      input: paymentAuthorityInput(
        paymentAuthorityTerm({
          ...paymentAuthorityTermOptions(narrowerChild),
          termId: "missing-single-use-spend-capability",
          capabilities: [],
          payment: {
            ...omitPaymentKey(narrowerPayment, "credential_form"),
            single_use_spend: false,
          },
        }),
        parent,
      ),
    },
  ];
}

function paymentAuthorityInput(
  child: PaymentAuthorityTermFixture,
  parent: PaymentAuthorityTermFixture,
): KernelFixture["input"] {
  return {
    kind: "policy.isPaymentAuthoritySubset",
    child,
    parent,
  };
}

interface PaymentAuthorityTermOptions {
  readonly termId: string;
  readonly verbs: readonly string[];
  readonly payment: PaymentAuthorityBoundsFixture;
  readonly expiresAt?: string;
  readonly conditions?: readonly unknown[];
  readonly approvals?: readonly unknown[];
  readonly capabilities?: readonly string[];
  readonly resourceFamily?: string;
  readonly resourceUri?: string;
}

function paymentAuthorityTerm(options: PaymentAuthorityTermOptions): PaymentAuthorityTermFixture {
  return {
    term_id: options.termId,
    principal_ref: paymentReference("principal", "principal:agent"),
    resource_ref: paymentReference("grant", options.resourceUri ?? "grant:payment:checkout"),
    resource_family: options.resourceFamily ?? "payment",
    verbs: options.verbs,
    bounds: {
      payment: options.payment,
    },
    conditions: options.conditions ?? [],
    approvals: options.approvals ?? [],
    capabilities: options.capabilities ?? (options.payment.single_use_spend ? ["payment_single_use_spend"] : []),
    expires_at: options.expiresAt,
    issued_by_ref: paymentReference("principal", "principal:issuer"),
  };
}

function paymentAuthorityTermOptions(term: PaymentAuthorityTermFixture): PaymentAuthorityTermOptions {
  const payment = term.bounds.payment;
  if (!payment) {
    throw new Error("payment authority fixture term must include payment bounds");
  }
  return {
    termId: term.term_id,
    verbs: term.verbs,
    payment,
    expiresAt: term.expires_at,
    conditions: term.conditions,
    approvals: term.approvals,
    capabilities: term.capabilities,
    resourceFamily: term.resource_family,
    resourceUri: typeof term.resource_ref.uri === "string" ? term.resource_ref.uri : undefined,
  };
}

function omitPaymentKey<K extends keyof PaymentAuthorityBoundsFixture>(
  payment: PaymentAuthorityBoundsFixture,
  key: K,
): Omit<PaymentAuthorityBoundsFixture, K> {
  const { [key]: _omitted, ...rest } = payment;
  return rest;
}

function paymentReference(type: string, uri: string): Readonly<Record<string, unknown>> {
  return { type, uri };
}

function isPaymentAuthoritySubset(
  child: PaymentAuthorityTermFixture,
  parent: PaymentAuthorityTermFixture,
): boolean {
  return child.resource_family === "payment"
    && parent.resource_family === "payment"
    && sameAuthorityResource(child.resource_ref, parent.resource_ref)
    && arraySubset(child.verbs, parent.verbs)
    && arraySubset(child.capabilities, parent.capabilities)
    && parent.conditions.every((condition) => child.conditions.some((childCondition) => deepEqual(childCondition, condition)))
    && parent.approvals.every((approval) => child.approvals.some((childApproval) => deepEqual(childApproval, approval)))
    && expirySubset(child.expires_at, parent.expires_at)
    && paymentBoundsSubset(child, parent);
}

function sameAuthorityResource(
  child: Readonly<Record<string, unknown>>,
  parent: Readonly<Record<string, unknown>>,
): boolean {
  return child.type === parent.type && child.uri === parent.uri;
}

function paymentBoundsSubset(
  child: PaymentAuthorityTermFixture,
  parent: PaymentAuthorityTermFixture,
): boolean {
  const childPayment = child.bounds.payment;
  const parentPayment = parent.bounds.payment;
  if (!childPayment || !parentPayment) {
    return false;
  }

  return childPayment.currency === parentPayment.currency
    && minorUnitCapsSubset(child, childPayment, parentPayment)
    && railsSubset(childPayment, parentPayment)
    && optionalExactOrNarrower(childPayment.realm, parentPayment.realm)
    && optionalExactOrNarrower(childPayment.counterparty, parentPayment.counterparty)
    && optionalExactOrNarrower(childPayment.operation, parentPayment.operation)
    && optionalExactOrNarrower(childPayment.period, parentPayment.period)
    && requiredPaymentBooleansSubset(childPayment, parentPayment)
    && optionalNumberLteWhenParentSet(childPayment.quote_ttl_ms, parentPayment.quote_ttl_ms)
    && optionalNumberLteWhenParentSet(childPayment.approval_threshold_minor, parentPayment.approval_threshold_minor)
    && optionalExactOrNarrower(childPayment.credential_form, parentPayment.credential_form)
    && singleUseSpendCapabilityForReserveOrSpend(child, parent);
}

function expirySubset(child: string | undefined, parent: string | undefined): boolean {
  if (parent === undefined) {
    return true;
  }
  return child !== undefined && child <= parent;
}

function minorUnitCapsSubset(
  child: PaymentAuthorityTermFixture,
  childPayment: PaymentAuthorityBoundsFixture,
  parentPayment: PaymentAuthorityBoundsFixture,
): boolean {
  if (usesMinorUnits(child) && childPayment.max_per_call_minor === undefined) {
    return false;
  }
  if (usesMinorUnits(child) && parentPayment.max_per_call_minor === undefined) {
    return false;
  }

  return optionalCapSubset(childPayment.max_per_call_minor, parentPayment.max_per_call_minor)
    && optionalCapSubset(childPayment.max_per_run_minor, parentPayment.max_per_run_minor)
    && optionalCapSubset(childPayment.max_per_period_minor, parentPayment.max_per_period_minor);
}

function usesMinorUnits(term: PaymentAuthorityTermFixture): boolean {
  return term.verbs.some((verb) => ["quote", "reserve", "spend", "refund"].includes(verb));
}

function optionalCapSubset(child: number | undefined, parent: number | undefined): boolean {
  if (child === undefined && parent !== undefined) {
    return false;
  }
  if (child !== undefined && parent !== undefined) {
    return child <= parent;
  }
  return true;
}

function railsSubset(child: PaymentAuthorityBoundsFixture, parent: PaymentAuthorityBoundsFixture): boolean {
  return child.rails.length > 0
    && parent.rails.length > 0
    && child.rails.every((rail) => parent.rails.includes(rail));
}

function optionalExactOrNarrower<T>(child: T | undefined, parent: T | undefined): boolean {
  if (parent === undefined) {
    return true;
  }
  return child !== undefined && deepEqual(child, parent);
}

function requiredPaymentBooleansSubset(
  child: PaymentAuthorityBoundsFixture,
  parent: PaymentAuthorityBoundsFixture,
): boolean {
  return (!parent.quote_required || child.quote_required === true)
    && (!parent.reservation_required || child.reservation_required === true)
    && (!parent.idempotency_required || child.idempotency_required === true)
    && (!parent.recovery_required || child.recovery_required === true)
    && (!parent.receipt_before_success || child.receipt_before_success === true);
}

function optionalNumberLteWhenParentSet(child: number | undefined, parent: number | undefined): boolean {
  return parent === undefined || (child !== undefined && child <= parent);
}

function singleUseSpendCapabilityForReserveOrSpend(
  child: PaymentAuthorityTermFixture,
  parent: PaymentAuthorityTermFixture,
): boolean {
  if (!child.verbs.some((verb) => verb === "spend")) {
    return true;
  }
  return child.bounds.payment?.single_use_spend === true
    && parent.bounds.payment?.single_use_spend === true
    && child.capabilities.includes("payment_single_use_spend")
    && parent.capabilities.includes("payment_single_use_spend")
    && child.bounds.payment?.credential_form === "single_use_spend_capability";
}

function arraySubset<T>(child: readonly T[], parent: readonly T[]): boolean {
  return child.every((value) => parent.includes(value));
}

function finishFanoutStep(
  state: SequentialGraphState,
  stepId: string,
  status: "succeeded" | "failed",
  outputs: Readonly<Record<string, unknown>> = {},
): SequentialGraphState {
  const started = transitionSequentialGraph(state, {
    type: "start_step",
    stepId,
    at: "2026-04-10T00:00:00.000Z",
  });
  return status === "succeeded"
    ? transitionSequentialGraph(started, {
        type: "step_succeeded",
        stepId,
        at: "2026-04-10T00:00:01.000Z",
        receiptId: `rx_${stepId}`,
        outputs,
      })
    : transitionSequentialGraph(started, {
        type: "step_failed",
        stepId,
        at: "2026-04-10T00:00:01.000Z",
        error: "boom",
      });
}

function fixtureDirectory(fixture: KernelFixture): string {
  if (isRunnerKernelFixture(fixture)) {
    return path.join(fixtureRoot, "runner");
  }
  return path.join(fixtureRoot, fixture.input.kind.startsWith("state-machine.") ? "state-machine" : "policy");
}

async function readJsonSchema(filePath: string): Promise<JsonSchema> {
  return JSON.parse(await readFile(filePath, "utf8")) as JsonSchema;
}

function schemaErrors(schema: JsonSchema, value: unknown, schemaName: string): readonly string[] {
  return validateJsonSchemaValue(schema, value, "").map((error) => `${schemaName}${error.path}: ${error.message}`);
}

export function validateJsonSchemaValue(schema: unknown, value: unknown, pathPrefix: string): readonly { readonly path: string; readonly message: string }[] {
  if (!isPlainRecord(schema)) {
    return [{ path: pathPrefix || "/", message: "schema must be an object" }];
  }

  const keywordErrors = unsupportedKeywordErrors(schema, pathPrefix);
  const anyOf = Array.isArray(schema.anyOf) ? schema.anyOf : undefined;
  if (anyOf) {
    if (!anyOf.some((branch) => validateJsonSchemaValue(branch, value, pathPrefix).length === 0)) {
      return [...keywordErrors, { path: pathPrefix || "/", message: "value did not match any allowed schema branch" }];
    }
  }

  const oneOf = Array.isArray(schema.oneOf) ? schema.oneOf : undefined;
  if (oneOf) {
    const branchResults = oneOf.map((branch, index) => ({
      index,
      errors: validateJsonSchemaValue(branch, value, pathPrefix),
    }));
    const matchCount = branchResults.filter((result) => result.errors.length === 0).length;
    if (matchCount !== 1) {
      const branchSummary = branchResults
        .map((result) => {
          if (result.errors.length === 0) {
            return `branch ${result.index}: matched`;
          }
          const firstError = result.errors[0];
          return `branch ${result.index}: ${firstError?.path ?? (pathPrefix || "/")} ${firstError?.message ?? "did not match"}`;
        })
        .join("; ");
      return [
        ...keywordErrors,
        {
          path: pathPrefix || "/",
          message: `value matched ${matchCount} schema branches; expected exactly one (${branchSummary})`,
        },
      ];
    }
  }

  if ("const" in schema && !deepEqual(value, schema.const)) {
    return [...keywordErrors, { path: pathPrefix || "/", message: `value must equal ${JSON.stringify(schema.const)}` }];
  }

  if (typeof schema.pattern === "string" && typeof value === "string" && !(new RegExp(schema.pattern, "u")).test(value)) {
    return [...keywordErrors, { path: pathPrefix || "/", message: `string must match ${schema.pattern}` }];
  }

  const typeErrors = validateJsonSchemaType(schema.type, value, pathPrefix);
  if (typeErrors.length > 0) {
    return [...keywordErrors, ...typeErrors];
  }

  const errors: { path: string; message: string }[] = [...keywordErrors];
  if ((schema.type === "object" || isPlainRecord(schema.properties)) && isPlainRecord(value)) {
    const properties = isPlainRecord(schema.properties) ? schema.properties : {};
    const required = Array.isArray(schema.required) ? schema.required.filter((entry): entry is string => typeof entry === "string") : [];
    for (const requiredKey of required) {
      if (!Object.hasOwn(value, requiredKey)) {
        errors.push({ path: `${pathPrefix}/${requiredKey}`, message: "required property is missing" });
      }
    }
    for (const [key, entry] of Object.entries(value)) {
      const propertySchema = Object.hasOwn(properties, key) ? properties[key] : undefined;
      if (propertySchema) {
        errors.push(...validateJsonSchemaValue(propertySchema, entry, `${pathPrefix}/${key}`));
      } else if (schema.additionalProperties === false) {
        errors.push({ path: `${pathPrefix}/${key}`, message: "additional property is not allowed" });
      }
    }
  }

  if ((schema.type === "array" || schema.items !== undefined) && Array.isArray(value) && schema.items !== undefined) {
    value.forEach((item, index) => {
      errors.push(...validateJsonSchemaValue(schema.items, item, `${pathPrefix}/${index}`));
    });
  }

  return errors;
}

function unsupportedKeywordErrors(schema: JsonSchema, pathPrefix: string): readonly { readonly path: string; readonly message: string }[] {
  return Object.keys(schema)
    .filter((key) => !supportedJsonSchemaKeywords.has(key))
    .map((key) => ({
      path: `${pathPrefix}/${key}`,
      message: `unsupported JSON Schema keyword '${key}'`,
    }));
}

function validateJsonSchemaType(
  type: unknown,
  value: unknown,
  pathPrefix: string,
): readonly { readonly path: string; readonly message: string }[] {
  if (type === undefined) {
    return [];
  }
  const types = Array.isArray(type) ? type : [type];
  if (types.some((entry) => jsonSchemaTypeMatches(entry, value))) {
    return [];
  }
  return [{ path: pathPrefix || "/", message: `value must be ${types.join(" or ")}` }];
}

function jsonSchemaTypeMatches(type: unknown, value: unknown): boolean {
  switch (type) {
    case "array":
      return Array.isArray(value);
    case "boolean":
      return typeof value === "boolean";
    case "integer":
      return typeof value === "number" && Number.isInteger(value);
    case "null":
      return value === null;
    case "number":
      return typeof value === "number" && Number.isFinite(value);
    case "object":
      return isPlainRecord(value);
    case "string":
      return typeof value === "string";
    default:
      return false;
  }
}

function deepEqual(left: unknown, right: unknown): boolean {
  return JSON.stringify(normalizeForFixture(left)) === JSON.stringify(normalizeForFixture(right));
}

function expectRecord(value: unknown, field: string): Readonly<Record<string, unknown>> {
  if (!isPlainRecord(value)) {
    throw new Error(`${field} must be an object`);
  }
  return value;
}

function expectArray(value: unknown, field: string): readonly unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(`${field} must be an array`);
  }
  return value;
}

function expectString(value: unknown, field: string): string {
  if (typeof value !== "string") {
    throw new Error(`${field} must be a string`);
  }
  return value;
}

function isPlainRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  await main();
}
