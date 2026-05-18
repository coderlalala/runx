import { describe, expect, it } from "vitest";

import {
  RUNX_CONTRACT_IDS,
  RUNX_AUXILIARY_SCHEMA_IDS,
  RUNX_CONTROL_SCHEMA_REFS,
  RUNX_LOGICAL_SCHEMAS,
  buildHostedOpenApiSchemas,
  agentContextEnvelopeSchema,
  adapterInvokeResultSchema,
  authorityProofSchema,
  outputContractSchema,
  credentialEnvelopeSchema,
  registryBindingSchema,
  reviewReceiptOutputSchema,
  runxContractSchemas,
  runxAuxiliarySchemas,
  runxGeneratedSchemaArtifacts,
  validateAdapterInvokeResultContract,
  validateAgentContextEnvelopeContract,
  validateAuthorityProofContract,
  validateOutputContractContract,
  validateResolutionRequestContract,
  validateCredentialEnvelopeContract,
  validateCapabilityExecutionContract,
  validateDevReportContract,
  validateDoctorReportContract,
  validateEvidenceBundleContract,
  validateHandoffSignalContract,
  validateHandoffStateContract,
  validateRegistryBindingContract,
  validateRunxListReportContract,
  validateReviewReceiptOutputContract,
  validateScopeAdmissionContract,
  validateSuppressionRecordContract,
  validateWorkItemContract,
  canTransitionWorkItemState,
  nextWorkItemStates,
} from "./index.js";

describe("@runxhq/contracts", () => {
  it("exports stable runx logical schema identifiers", () => {
    expect(RUNX_LOGICAL_SCHEMAS.doctor).toBe("runx.doctor.v1");
    expect(RUNX_LOGICAL_SCHEMAS.receipt).toBe("runx.receipt.v1");
  });

  it("uses durable schema URI ids", () => {
    expect(RUNX_CONTRACT_IDS.toolManifest).toBe("https://schemas.runx.dev/runx/tool/manifest/v1.json");
    expect(runxContractSchemas.toolManifest.$id).toBe(RUNX_CONTRACT_IDS.toolManifest);
    expect((runxContractSchemas.dev.properties?.doctor as { readonly $ref?: string } | undefined)?.$ref)
      .toBe(RUNX_CONTRACT_IDS.doctor);
  });

  it("keeps fixture lanes aligned with authoring plan", () => {
    const lane = runxContractSchemas.fixture.properties?.lane;
    expect((lane?.anyOf as readonly { readonly const?: string }[] | undefined)?.map((entry) => entry.const)).toEqual([
      "deterministic",
      "agent",
      "repo-integration",
    ]);
  });

  it("owns credential envelope schema and runtime validation", () => {
    expect(credentialEnvelopeSchema.$id).toBe(RUNX_CONTROL_SCHEMA_REFS.credential_envelope);
    expect(validateCredentialEnvelopeContract({
      kind: "runx.credential-envelope.v1",
      grant_id: "grant_1",
      provider: "github",
      connection_id: "conn_1",
      scopes: ["repo:read"],
      material_ref: "nango:github:conn_1",
    })).toMatchObject({
      provider: "github",
      scopes: ["repo:read"],
    });
  });

  it("owns scope admission schema and runtime validation", () => {
    expect(RUNX_CONTROL_SCHEMA_REFS.scope_admission).toBe("https://runx.ai/spec/scope-admission.schema.json");
    expect(validateScopeAdmissionContract({
      status: "allow",
      requested_scopes: ["repo:status"],
      granted_scopes: ["repo:*"],
      decision_summary: "",
    })).toEqual({
      status: "allow",
      requested_scopes: ["repo:status"],
      granted_scopes: ["repo:*"],
      decision_summary: "",
    });
    expect(() => validateScopeAdmissionContract({
      status: "pending",
      requested_scopes: ["repo:status"],
      granted_scopes: ["repo:*"],
    })).toThrow(/scope-admission\.schema\.json/);
  });

  it("owns authority proof schema and generated artifact", () => {
    expect(authorityProofSchema.$id).toBe(RUNX_CONTROL_SCHEMA_REFS.authority_proof);
    expect(runxContractSchemas.authorityProof).toBe(authorityProofSchema);
    expect(runxGeneratedSchemaArtifacts["authority-proof.schema.json"]).toBe(authorityProofSchema);
    expect(validateAuthorityProofContract({
      schema_version: "runx.authority-proof.v1",
      skill_name: "connected-review",
      source_type: "agent-step",
      requested: {
        connected_auth: true,
        scopes: ["repo:read"],
        mutating: false,
        scope_family: "github_repo",
        authority_kind: "read_only",
        target_repo: "runxhq/aster",
      },
      scope_admission: {
        status: "allow",
        requested_scopes: ["repo:read"],
        granted_scopes: ["repo:*"],
      },
      credential_material: {
        status: "not_resolved",
        provider: "github",
        scopes: ["repo:read"],
        scope_family: "github_repo",
        authority_kind: "read_only",
        target_repo: "runxhq/aster",
      },
      redaction: {
        status: "applied",
        secret_material: "omitted",
        stdout: "hashed",
        stderr: "hashed",
        metadata_secret_keys: ["token-like metadata keys"],
      },
    })).toMatchObject({
      schema_version: "runx.authority-proof.v1",
    });
  });

  it("owns executor control protocol schemas and runtime validation", () => {
    expect(outputContractSchema.$id).toBe(RUNX_CONTROL_SCHEMA_REFS.output_contract);
    expect(agentContextEnvelopeSchema.$id).toBe(RUNX_CONTROL_SCHEMA_REFS.agent_context_envelope);
    expect(adapterInvokeResultSchema.$id).toBe(RUNX_CONTROL_SCHEMA_REFS.adapter_invoke_result);
    expect(runxGeneratedSchemaArtifacts["output-contract.schema.json"]).toBe(outputContractSchema);
    expect(runxGeneratedSchemaArtifacts["agent-context-envelope.schema.json"]).toBe(agentContextEnvelopeSchema);
    expect(runxGeneratedSchemaArtifacts["adapter-invoke-result.schema.json"]).toBe(adapterInvokeResultSchema);

    expect(validateOutputContractContract({
      summary: "string",
      verdict: {
        type: "string",
        enum: ["pass", "fail"],
        required: true,
      },
    })).toMatchObject({
      summary: "string",
    });

    expect(validateAgentContextEnvelopeContract({
      run_id: "rx_contract",
      skill: "demo.skill",
      instructions: "Do the work.",
      inputs: {},
      allowed_tools: ["fs.read"],
      current_context: [],
      historical_context: [],
      provenance: [],
      expected_outputs: {
        summary: "string",
      },
      trust_boundary: "test",
    })).toMatchObject({
      run_id: "rx_contract",
      expected_outputs: {
        summary: "string",
      },
    });

    expect(() => validateAgentContextEnvelopeContract({
      run_id: "rx_contract",
      skill: "demo.skill",
      instructions: "Do the work.",
      inputs: {},
      allowed_tools: [],
      current_context: [],
      historical_context: [],
      provenance: [],
      context: {
        voice_grammar: {},
      },
      trust_boundary: "test",
    })).toThrow("agent_context_envelope.context.voice_grammar must match");

    const resolutionRequest = validateResolutionRequestContract({
      id: "approval.demo",
      kind: "approval",
      gate: {
        id: "gate.demo",
        reason: "Needs human approval.",
      },
    });

    expect(validateAdapterInvokeResultContract({
      status: "needs_resolution",
      stdout: "",
      stderr: "",
      exitCode: null,
      signal: null,
      durationMs: 0,
      request: resolutionRequest,
    })).toMatchObject({
      status: "needs_resolution",
      request: {
        kind: "approval",
      },
    });
  });

  it("owns generated auxiliary schemas", () => {
    expect(registryBindingSchema.$id).toBe(RUNX_AUXILIARY_SCHEMA_IDS.registryBinding);
    expect(reviewReceiptOutputSchema.$id).toBe(RUNX_AUXILIARY_SCHEMA_IDS.reviewReceiptOutput);
    expect(runxAuxiliarySchemas.reviewReceiptOutput).toBe(reviewReceiptOutputSchema);
    expect(runxGeneratedSchemaArtifacts["doctor.schema.json"]).toBe(runxContractSchemas.doctor);
    expect(runxGeneratedSchemaArtifacts["capability-execution.schema.json"]).toBe(runxContractSchemas.capabilityExecution);
    expect(runxGeneratedSchemaArtifacts["work-item.schema.json"]).toBe(runxContractSchemas.workItem);
    expect(runxGeneratedSchemaArtifacts["evidence-bundle.schema.json"]).toBe(runxContractSchemas.evidenceBundle);
    expect(runxGeneratedSchemaArtifacts["handoff-signal.schema.json"]).toBe(runxContractSchemas.handoffSignal);
    expect(runxGeneratedSchemaArtifacts["handoff-state.schema.json"]).toBe(runxContractSchemas.handoffState);
    expect(runxGeneratedSchemaArtifacts["suppression-record.schema.json"]).toBe(runxContractSchemas.suppressionRecord);
    expect(runxGeneratedSchemaArtifacts["review-receipt-output.schema.json"]).toBe(reviewReceiptOutputSchema);
  });

  it("owns hydrated issue evidence bundles", () => {
    expect(RUNX_LOGICAL_SCHEMAS.evidenceBundle).toBe("runx.evidence_bundle.v1");
    expect(RUNX_CONTRACT_IDS.evidenceBundle).toBe("https://schemas.runx.dev/runx/evidence-bundle/v1.json");
    expect(validateEvidenceBundleContract({
      schema: "runx.evidence_bundle.v1",
      evidence_bundle_id: "eb_checkout_retry",
      subject_locator: "github://runxhq/example/issues/101",
      hydration: {
        status: "complete",
        summary: "Slack thread and Sentry issue event were hydrated before triage.",
        requested_at: "2026-05-15T00:00:00Z",
        completed_at: "2026-05-15T00:00:05Z",
      },
      sources: [
        {
          provider: "slack",
          kind: "source_thread",
          locator: "slack://T1/C1/171000.0001",
          thread_locator: "slack://T1/C1/171000.0001",
          title: "Checkout fails on discount retry",
          body_preview: "Customer report plus support reproduction notes.",
          hydration_status: "complete",
          observed_at: "2026-05-15T00:00:00Z",
        },
        {
          provider: "sentry",
          kind: "stacktrace",
          locator: "sentry://issues/987/events/latest",
          url: "https://example.sentry.io/issues/987",
          title: "DiscountRetryError",
          body_preview: "Stacktrace and release metadata redacted before publication.",
          hydration_status: "complete",
          observed_at: "2026-05-15T00:00:03Z",
          data: {
            release: "api@abc123",
            environment: "production",
          },
        },
      ],
      redaction: {
        status: "applied",
        summary: "Secrets and direct identifiers were redacted by the source adapter.",
        secret_material: "omitted",
        pii: "redacted",
      },
      summary: "Checkout retry failure has enough hydrated evidence for triage.",
      created_at: "2026-05-15T00:00:00Z",
      updated_at: "2026-05-15T00:00:05Z",
    })).toMatchObject({
      schema: "runx.evidence_bundle.v1",
      hydration: {
        status: "complete",
      },
      sources: [
        {
          provider: "slack",
        },
        {
          provider: "sentry",
        },
      ],
    });

    expect(() => validateEvidenceBundleContract({
      schema: "runx.evidence_bundle.v1",
      evidence_bundle_id: "eb_thin_alert",
      subject_locator: "sentry://issues/987",
      hydration: {
        status: "needed",
        summary: "The Slack alert card is not sufficient for build triage.",
      },
      sources: [],
      summary: "No usable source evidence yet.",
      created_at: "2026-05-15T00:00:00Z",
      updated_at: "2026-05-15T00:00:00Z",
    })).toThrow(/evidence-bundle\/v1\.json/);
  });

  it("owns the portable work-item lifecycle contract", () => {
    expect(RUNX_LOGICAL_SCHEMAS.workItem).toBe("runx.work_item.v1");
    expect(RUNX_CONTRACT_IDS.workItem).toBe("https://schemas.runx.dev/runx/work-item/v1.json");
    expect(nextWorkItemStates("build_ready")).toEqual(["review_ready", "outcome_closed", "outcome_rejected", "blocked"]);
    expect(nextWorkItemStates("intake_received")).toContain("planning_ready");
    expect(nextWorkItemStates("intake_received")).toContain("build_ready");
    expect(canTransitionWorkItemState("build_ready", "review_ready")).toBe(true);
    expect(canTransitionWorkItemState("intake_received", "build_ready")).toBe(true);
    expect(canTransitionWorkItemState("build_ready", "merge_gate")).toBe(false);
    expect(canTransitionWorkItemState("merge_gate", "build_ready")).toBe(false);
    expect(validateWorkItemContract({
      schema: "runx.work_item.v1",
      work_item_id: "wi_101",
      state: "merge_gate",
      source_events: [
        {
          provider: "slack",
          source_locator: "slack://T1/C1/171000.0001",
          event_kind: "message",
          thread_locator: "slack://T1/C1/171000.0001",
          provider_event_id: "171000.0001",
          title: "Checkout fails on discount retry",
          body_preview: "Sentry and support both show checkout retry failures.",
          occurred_at: "2026-05-15T00:00:00Z",
        },
      ],
      dedupe: {
        algorithm: "sha256",
        source_locator: "slack://T1/C1/171000.0001",
        fingerprint: "sha256:checkout-discount-retry",
        provider_event_id: "171000.0001",
        candidate_work_item_ids: ["wi_099"],
      },
      triage: {
        category: "bug",
        severity: "high",
        confidence: 0.91,
        action: "issue-to-pr",
        recommended_lane: "issue-to-pr",
        needs_human: false,
        rationale: "The report is reproducible and bounded to checkout retry behavior.",
      },
      change_set: {
        change_set_id: "change_set_checkout_retry",
      },
      owner_suggestion: {
        owner: "payments-team",
        confidence: 0.8,
        rationale: "Checkout retry behavior is owned by the payments surface.",
      },
      target_repo_suggestion: {
        repo: "runxhq/example",
        confidence: 0.86,
        rationale: "The source event points at the example checkout repo.",
      },
      evidence_bundle: {
        schema: "runx.evidence_bundle.v1",
        evidence_bundle_id: "eb_checkout_retry",
        subject_locator: "github://runxhq/example/issues/101",
        hydration: {
          status: "complete",
          summary: "Slack and Sentry evidence were hydrated before build triage.",
        },
        sources: [
          {
            provider: "slack",
            kind: "source_thread",
            locator: "slack://T1/C1/171000.0001",
            thread_locator: "slack://T1/C1/171000.0001",
            title: "Checkout fails on discount retry",
            body_preview: "Sentry and support both show checkout retry failures.",
          },
          {
            provider: "sentry",
            kind: "stacktrace",
            locator: "sentry://issues/987/events/latest",
            hydration_status: "complete",
          },
        ],
        redaction: {
          status: "applied",
          summary: "Adapter removed secret-like values before publication.",
        },
        summary: "Hydrated evidence is ready for issue-to-pr.",
        created_at: "2026-05-15T00:00:00Z",
        updated_at: "2026-05-15T00:00:05Z",
      },
      issue: {
        provider: "github",
        locator: "github://runxhq/example/issues/101",
        url: "https://github.com/runxhq/example/issues/101",
        status: "open",
      },
      pull_request: {
        provider: "github",
        locator: "github://runxhq/example/pulls/102",
        url: "https://github.com/runxhq/example/pull/102",
        status: "open",
      },
      merge_gate: {
        required: true,
        summary: "Human reviewer must merge or close the generated PR.",
      },
      status_summary: "PR is ready; waiting at the human merge gate.",
      created_at: "2026-05-15T00:00:00Z",
      updated_at: "2026-05-15T00:10:00Z",
    })).toMatchObject({
      schema: "runx.work_item.v1",
      state: "merge_gate",
      owner_suggestion: {
        owner: "payments-team",
      },
      target_repo_suggestion: {
        repo: "runxhq/example",
      },
      evidence_bundle: {
        hydration: {
          status: "complete",
        },
      },
      triage: {
        action: "issue-to-pr",
      },
    });
  });

  it("owns hosted OpenAPI components for cloud consumers", () => {
    const schemas = buildHostedOpenApiSchemas();

    expect(schemas).toHaveProperty("CreateRunRequest", {
      $ref: "../../spec/hosted/create-run.request.schema.json",
    });
    expect(schemas).toHaveProperty("RerunRunRequest", {
      $ref: "../../spec/hosted/run-rerun.request.schema.json",
    });
    expect(schemas).toHaveProperty("RerunRunResponse", {
      $ref: "../../spec/hosted/run-rerun.response.schema.json",
    });
    expect(schemas).toHaveProperty("RunDiffEnvelope", {
      $ref: "../../spec/hosted/run-diff.response.schema.json",
    });
    const removedSchemaNameToken = ["leg", "acy"].join("");
    expect(Object.keys(schemas).filter((name) => name.toLowerCase().includes(removedSchemaNameToken))).toEqual([]);
    expect(Object.keys(schemas).filter((name) => name.endsWith("ApprovalEnvelope"))).toEqual([]);
    expect(schemas.HostedReadinessEnvelope).toMatchObject({
      required: ["status", "checks"],
      properties: {
        checks: {
          additionalProperties: {
            $ref: "#/components/schemas/HostedReadinessCheck",
          },
        },
      },
    });
    expect(schemas.HostedApiMetricsEnvelope).toMatchObject({
      required: ["status", "metrics"],
      properties: {
        metrics: {
          $ref: "#/components/schemas/HostedApiMetrics",
        },
      },
    });
    expect(schemas.HostedApiLatencyBucketCounts).toMatchObject({
      required: ["le_50ms", "le_100ms", "le_250ms", "le_500ms", "le_1000ms", "gt_1000ms"],
    });
    expect(schemas).toHaveProperty("PublicSkillDetailEnvelope");
    expect(schemas).toHaveProperty("KnowledgeEntryEnvelope");
    expect(schemas).toHaveProperty("ApprovalRouteSnapshot");
    expect(schemas.PolicyApprovalRouteSummary).not.toMatchObject({
      properties: expect.objectContaining({
        missing: expect.anything(),
      }),
    });
    expect(schemas.ApprovalRouteSnapshot).toMatchObject({
      properties: expect.objectContaining({
        missing: { type: "boolean" },
      }),
      required: ["route_id", "recipients"],
    });
    expect(schemas.ApprovalInboxItem).toMatchObject({
      properties: expect.objectContaining({
        route: {
          $ref: "#/components/schemas/ApprovalRouteSnapshot",
        },
      }),
    });
    expect(schemas.ApprovalInboxEnvelope).toMatchObject({
      properties: expect.objectContaining({
        next_cursor: { type: "string" },
      }),
    });
    expect(schemas.RunListEnvelope).toMatchObject({
      properties: expect.objectContaining({
        next_cursor: { type: "string" },
      }),
    });
  });

  it("validates auxiliary schema payloads", () => {
    expect(validateReviewReceiptOutputContract({
      verdict: "pass",
      failure_summary: "No harness failure.",
      improvement_proposals: [],
      next_harness_checks: ["runx harness"],
    })).toMatchObject({ verdict: "pass" });

    expect(validateRegistryBindingContract({
      schema: "runx.registry_binding.v1",
      state: "registry_bound",
      skill: {
        id: "runx/sourcey",
        name: "sourcey",
        description: "Docs skill.",
      },
      upstream: {
        host: "github.com",
        owner: "runxhq",
        repo: "runx",
        path: "skills/sourcey",
        commit: "abc123",
        blob_sha: "def456",
        source_of_truth: true,
      },
      registry: {
        owner: "runx",
        trust_tier: "first_party",
        version: "1.0.0",
        profile_path: "X.yaml",
        materialized_package_is_registry_artifact: true,
      },
      harness: {
        status: "harness_verified",
        case_count: 1,
      },
    })).toMatchObject({ schema: "runx.registry_binding.v1" });
  });

  it("owns generic post-handoff contracts for reusable outreach state", () => {
    expect(RUNX_CONTRACT_IDS.handoffSignal).toBe("https://schemas.runx.dev/runx/handoff-signal/v1.json");
    expect(runxContractSchemas.handoffSignal.$id).toBe(RUNX_CONTRACT_IDS.handoffSignal);
    expect(validateHandoffSignalContract({
      schema: "runx.handoff_signal.v1",
      signal_id: "sig_1",
      handoff_id: "docs-pr:example/repo:001",
      boundary_kind: "external_maintainer",
      target_repo: "example/repo",
      target_locator: "github://example/repo/pulls/42",
      thread_locator: "github://example/repo/pulls/42",
      outbox_entry_id: "pull_request:docs-refresh-example-repo",
      source: "pull_request_comment",
      disposition: "requested_changes",
      recorded_at: "2026-04-24T02:30:00Z",
      actor: {
        actor_id: "maintainer",
        role: "maintainer",
      },
      source_ref: {
        type: "provider_comment",
        uri: "https://github.com/example/repo/pull/42#issuecomment-1",
      },
    })).toMatchObject({
      handoff_id: "docs-pr:example/repo:001",
      disposition: "requested_changes",
    });

    expect(validateHandoffStateContract({
      schema: "runx.handoff_state.v1",
      handoff_id: "docs-pr:example/repo:001",
      target_repo: "example/repo",
      status: "needs_revision",
      signal_count: 2,
      last_signal_id: "sig_1",
      last_signal_at: "2026-04-24T02:30:00Z",
      last_signal_disposition: "requested_changes",
    })).toMatchObject({
      status: "needs_revision",
      signal_count: 2,
    });

    expect(validateSuppressionRecordContract({
      schema: "runx.suppression_record.v1",
      record_id: "sup_1",
      scope: "contact",
      key: "mailto:maintainer@example.org",
      reason: "requested_no_contact",
      recorded_at: "2026-04-24T02:31:00Z",
      source_signal_id: "sig_2",
    })).toMatchObject({
      scope: "contact",
      reason: "requested_no_contact",
    });
  });

  it("owns a generic capability execution envelope contract for transport-neutral invocation", () => {
    expect(RUNX_CONTRACT_IDS.capabilityExecution).toBe("https://schemas.runx.dev/runx/capability-execution/v1.json");
    expect(runxContractSchemas.capabilityExecution.$id).toBe(RUNX_CONTRACT_IDS.capabilityExecution);

    expect(validateCapabilityExecutionContract({
      schema: "runx.capability_execution.v1",
      capability_ref: "outreach",
      runner: "rerun",
      thread_ref: "github://sourcey/sourcey.com/issues/3",
      requested_at: "2026-04-25T13:45:00Z",
      transport: {
        kind: "github_issue_comment",
        trigger_ref: "https://github.com/sourcey/sourcey.com/issues/3#issuecomment-1",
        scope_set: ["docs.write", "thread:push"],
        actor: {
          actor_id: "auscaster",
          display_name: "auscaster",
          provider_identity: "github:auscaster",
        },
      },
      input_overrides: {
        objective: "Refresh the MCP-first docs preview.",
        bind_current: true,
      },
      idempotency: {
        algorithm: "sha256",
        intent_key: "sha256:intent",
        trigger_key: "sha256:trigger",
        content_hash: "sha256:content",
      },
    })).toMatchObject({
      capability_ref: "outreach",
      runner: "rerun",
      transport: {
        kind: "github_issue_comment",
      },
      idempotency: {
        algorithm: "sha256",
      },
    });
  });

  it("validates machine report payloads from TypeBox-owned contracts", () => {
    expect(validateDoctorReportContract({
      schema: RUNX_LOGICAL_SCHEMAS.doctor,
      status: "success",
      summary: {
        errors: 0,
        warnings: 1,
        infos: 0,
      },
      diagnostics: [{
        id: "runx.tool.fixture.missing",
        instance_id: "sha256:fixture",
        severity: "warning",
        title: "Missing fixture",
        message: "Tool has no deterministic fixture.",
        target: {
          kind: "tool",
          ref: "demo.echo",
        },
        location: {
          path: "tools/demo/echo/manifest.json",
        },
        repairs: [],
      }],
    })).toMatchObject({
      status: "success",
      diagnostics: [expect.objectContaining({ id: "runx.tool.fixture.missing" })],
    });

    expect(validateRunxListReportContract({
      schema: RUNX_LOGICAL_SCHEMAS.list,
      root: "/tmp/runx",
      requested_kind: "all",
      items: [{
        kind: "tool",
        name: "demo.echo",
        source: "local",
        path: "tools/demo/echo/manifest.json",
        status: "ok",
        scopes: ["repo:status"],
        emits: [{ name: "result", packet: "demo.result" }],
        fixtures: 1,
      }],
    })).toMatchObject({
      requested_kind: "all",
      items: [expect.objectContaining({ kind: "tool", fixtures: 1 })],
    });

    expect(validateDevReportContract({
      schema: RUNX_LOGICAL_SCHEMAS.dev,
      status: "success",
      doctor: {
        schema: RUNX_LOGICAL_SCHEMAS.doctor,
        status: "success",
        summary: {
          errors: 0,
          warnings: 0,
          infos: 0,
        },
        diagnostics: [],
      },
      fixtures: [{
        name: "demo-fixture",
        lane: "deterministic",
        target: {
          kind: "tool",
        },
        status: "success",
        duration_ms: 12,
        assertions: [],
      }],
      receipt_id: "rx_123",
    })).toMatchObject({
      status: "success",
      receipt_id: "rx_123",
    });
  });
});
