import { describe, expect, it } from "vitest";

import {
  RUNX_CONTRACT_IDS,
  RUNX_AUXILIARY_SCHEMA_IDS,
  RUNX_CONTROL_SCHEMA_REFS,
  RUNX_LOGICAL_SCHEMAS,
  buildHostedOpenApiSchemas,
  agentContextEnvelopeSchema,
  agentActInvocationSchema,
  approvalGateSchema,
  actReceiptEnvelopeSchema,
  actAssignmentV1Schema,
  authorityProofSchema,
  credentialDeliveryBrokerResponseV1Schema,
  credentialDeliveryObservationV1Schema,
  credentialDeliveryProfileV1Schema,
  credentialDeliveryRequestV1Schema,
  devV1Schema,
  doctorV1Schema,
  externalAdapterCancellationFrameV1Schema,
  externalAdapterCredentialRequestV1Schema,
  externalAdapterHostResolutionFrameV1Schema,
  externalAdapterInvocationV1Schema,
  externalAdapterManifestV1Schema,
  externalAdapterResponseV1Schema,
  fixtureV1Schema,
  handoffSignalV1Schema,
  handoffStateV1Schema,
  ledgerRecordSchema,
  listV1Schema,
  outputSchema,
  packetIndexV1Schema,
  credentialEnvelopeSchema,
  questionSchema,
  registryBindingSchema,
  reviewReceiptOutputSchema,
  resolutionRequestSchema,
  resolutionResponseSchema,
  receiptV1Schema,
  runSummaryV1Schema,
  runxContractSchemas,
  runxAuxiliarySchemas,
  runxGeneratedSchemaArtifacts,
  scopeAdmissionSchema,
  suppressionRecordV1Schema,
  threadOutboxProviderFetchV1Schema,
  threadOutboxProviderManifestV1Schema,
  threadOutboxProviderObservationV1Schema,
  threadOutboxProviderPushV1Schema,
  toolManifestV1Schema,
  contractSchemaMatches,
  validateActReceiptEnvelopeContract,
  validateActContract,
  validateAgentContextEnvelopeContract,
  validateAuthorityProofContract,
  validateOutputContract,
  validateResolutionRequestContract,
  validateCredentialEnvelopeContract,
  validateActAssignmentContract,
  validateDevReportContract,
  validateDoctorReportContract,
  validateHandoffSignalContract,
  validateHandoffStateContract,
  validateReceiptContract,
  validateTargetContract,
  validateOpportunityContract,
  validateThesisAssessmentContract,
  validateSelectionContract,
  validateSkillBindingContract,
  validateTargetTransitionEntryContract,
  validateSelectionCycleContract,
  validateReflectionEntryContract,
  validateFeedEntryContract,
  validateRegistryBindingContract,
  validateRunxListReportContract,
  validateReviewReceiptOutputContract,
  validateScopeAdmissionContract,
  validateSuppressionRecordContract,
  validateOperationalPolicyContract,
  validateOperationalPolicySemantics,
  validateSignalContract,
  validateReferenceContract,
  proofKinds,
  proofKindSchema,
} from "./index.js";

describe("@runxhq/contracts", () => {
  it("exports stable runx logical schema identifiers", () => {
    expect(RUNX_LOGICAL_SCHEMAS.doctor).toBe("runx.doctor.v1");
    expect(RUNX_LOGICAL_SCHEMAS.receipt).toBe("runx.receipt.v1");
  });

  it("uses durable schema URI ids", () => {
    expect(RUNX_CONTRACT_IDS.toolManifest).toBe("https://schemas.runx.dev/runx/tool/manifest/v1.json");
    expect(runxContractSchemas.toolManifest.$id).toBe(RUNX_CONTRACT_IDS.toolManifest);
    expect(toolManifestV1Schema).toBe(runxContractSchemas.toolManifest);
    expect((toolManifestV1Schema.properties as Record<string, unknown>).source).toBeDefined();
    expect((toolManifestV1Schema.required as readonly string[])).not.toContain("version");
    const devProperties = runxContractSchemas.dev.properties as Record<string, unknown> | undefined;
    expect(devProperties?.doctor).toMatchObject({ $id: RUNX_CONTRACT_IDS.doctor });
  });

  it("exports Rust-generated artifacts for control schema facades", () => {
    expect(outputSchema).toBe(runxContractSchemas.output);
    expect(agentContextEnvelopeSchema).toBe(runxContractSchemas.agentContextEnvelope);
    expect(agentActInvocationSchema).toBe(runxContractSchemas.agentActInvocation);
    expect(questionSchema).toBe(runxContractSchemas.question);
    expect(approvalGateSchema).toBe(runxContractSchemas.approvalGate);
    expect(resolutionRequestSchema).toBe(runxContractSchemas.resolutionRequest);
    expect(resolutionResponseSchema).toBe(runxContractSchemas.resolutionResponse);
    expect(actReceiptEnvelopeSchema).toBe(runxContractSchemas.actReceiptEnvelope);
    expect(credentialEnvelopeSchema).toBe(runxContractSchemas.credentialEnvelope);
    expect(scopeAdmissionSchema).toBe(runxContractSchemas.scopeAdmission);
    expect(authorityProofSchema).toBe(runxContractSchemas.authorityProof);
    expect(credentialDeliveryProfileV1Schema).toBe(runxContractSchemas.credentialDeliveryProfile);
    expect(credentialDeliveryRequestV1Schema).toBe(runxContractSchemas.credentialDeliveryRequest);
    expect(credentialDeliveryBrokerResponseV1Schema).toBe(runxContractSchemas.credentialDeliveryBrokerResponse);
    expect(credentialDeliveryObservationV1Schema).toBe(runxContractSchemas.credentialDeliveryObservation);
    expect(threadOutboxProviderManifestV1Schema).toBe(runxContractSchemas.threadOutboxProviderManifest);
    expect(threadOutboxProviderPushV1Schema).toBe(runxContractSchemas.threadOutboxProviderPush);
    expect(threadOutboxProviderFetchV1Schema).toBe(runxContractSchemas.threadOutboxProviderFetch);
    expect(threadOutboxProviderObservationV1Schema).toBe(runxContractSchemas.threadOutboxProviderObservation);
    expect(externalAdapterManifestV1Schema).toBe(runxContractSchemas.externalAdapterManifest);
    expect(externalAdapterCredentialRequestV1Schema).toBe(runxContractSchemas.externalAdapterCredentialRequest);
    expect(externalAdapterInvocationV1Schema).toBe(runxContractSchemas.externalAdapterInvocation);
    expect(externalAdapterResponseV1Schema).toBe(runxContractSchemas.externalAdapterResponse);
    expect(externalAdapterHostResolutionFrameV1Schema).toBe(runxContractSchemas.externalAdapterHostResolution);
    expect(externalAdapterCancellationFrameV1Schema).toBe(runxContractSchemas.externalAdapterCancellation);
    expect(receiptV1Schema).toBe(runxContractSchemas.receipt);
    expect(doctorV1Schema).toBe(runxContractSchemas.doctor);
    expect(devV1Schema).toBe(runxContractSchemas.dev);
    expect(listV1Schema).toBe(runxContractSchemas.list);
    expect(runSummaryV1Schema).toBe(runxContractSchemas.runSummary);
    expect(fixtureV1Schema).toBe(runxContractSchemas.fixture);
    expect(packetIndexV1Schema).toBe(runxContractSchemas.packetIndex);
    expect(actAssignmentV1Schema).toBe(runxContractSchemas.actAssignment);
    expect(ledgerRecordSchema).toBe(runxContractSchemas.ledgerEntry);
    expect(handoffSignalV1Schema).toBe(runxContractSchemas.handoffSignal);
    expect(handoffStateV1Schema).toBe(runxContractSchemas.handoffState);
    expect(suppressionRecordV1Schema).toBe(runxContractSchemas.suppressionRecord);
  });

  it("keeps fixture lanes aligned with authoring plan", () => {
    const fixtureProperties = runxContractSchemas.fixture.properties as Record<string, unknown> | undefined;
    const lane = fixtureProperties?.lane as { readonly anyOf?: readonly { readonly const?: string }[] } | undefined;
    expect((lane?.anyOf as readonly { readonly const?: string }[] | undefined)?.map((entry) => entry.const)).toEqual([
      "deterministic",
      "agent",
      "repo-integration",
    ]);
  });

  it("accepts typed proof kinds on references", () => {
    expect(proofKinds).toEqual(["payment_rail"]);
    expect(proofKindSchema).toMatchObject({
      anyOf: [expect.objectContaining({ const: "payment_rail", type: "string" })],
    });
    expect(validateReferenceContract({
      type: "verification",
      uri: "receipt-proof:mock:payment-execution-001",
      proof_kind: "payment_rail",
      label: "display-only text",
    })).toMatchObject({
      type: "verification",
      proof_kind: "payment_rail",
    });
  });

  it("owns credential envelope schema and runtime validation", () => {
    expect(credentialEnvelopeSchema.$id).toBe(RUNX_CONTROL_SCHEMA_REFS.credential_envelope);
    expect(validateCredentialEnvelopeContract({
      kind: "runx.credential-envelope.v1",
      grant_id: "grant_1",
      provider: "github",
      auth_mode: "oauth",
      material_kind: "nango_connection",
      provider_reference: "conn_1",
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
    expect(runxContractSchemas.authorityProof.$id).toBe(authorityProofSchema.$id);
    expect(runxGeneratedSchemaArtifacts["authority-proof.schema.json"].$id).toBe(authorityProofSchema.$id);
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
    expect(outputSchema.$id).toBe(RUNX_CONTROL_SCHEMA_REFS.output);
    expect(agentContextEnvelopeSchema.$id).toBe(RUNX_CONTROL_SCHEMA_REFS.agent_context_envelope);
    expect(actReceiptEnvelopeSchema.$id).toBe(RUNX_CONTROL_SCHEMA_REFS.act_receipt);
    expect(runxGeneratedSchemaArtifacts["output.schema.json"].$id).toBe(outputSchema.$id);
    expect(runxGeneratedSchemaArtifacts["agent-context-envelope.schema.json"].$id).toBe(agentContextEnvelopeSchema.$id);
    expect(runxGeneratedSchemaArtifacts["act-receipt.schema.json"].$id).toBe(actReceiptEnvelopeSchema.$id);

    expect(validateOutputContract({
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
      output: {
        summary: "string",
      },
      trust_boundary: "test",
    })).toMatchObject({
      run_id: "rx_contract",
      output: {
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
    })).toThrow("agent_context_envelope.context must match");

    expect(validateAgentContextEnvelopeContract({
      run_id: "rx_contract",
      step_id: "plan",
      skill: "demo.plan",
      instructions: "Do the work.",
      inputs: {},
      allowed_tools: ["fs.read"],
      current_context: [],
      historical_context: [],
      provenance: [],
      execution_location: {
        skill_directory: "/tmp/demo-skill",
        tool_roots: ["/tmp/extra-tools"],
      },
      trust_boundary: "test",
    })).toMatchObject({
      execution_location: {
        skill_directory: "/tmp/demo-skill",
        tool_roots: ["/tmp/extra-tools"],
      },
    });

    const resolutionRequest = validateResolutionRequestContract({
      id: "approval.demo",
      kind: "approval",
      gate: {
        id: "gate.demo",
        reason: "Needs human approval.",
      },
    });

    expect(validateActReceiptEnvelopeContract({
      status: "needs_agent",
      stdout: "",
      stderr: "",
      exitCode: null,
      signal: null,
      durationMs: 0,
      request: resolutionRequest,
    })).toMatchObject({
      status: "needs_agent",
      request: {
        kind: "approval",
      },
    });
  });

  it("owns generated auxiliary schemas", () => {
    expect(registryBindingSchema.$id).toBe(RUNX_AUXILIARY_SCHEMA_IDS.registryBinding);
    expect(reviewReceiptOutputSchema.$id).toBe(RUNX_AUXILIARY_SCHEMA_IDS.reviewReceiptOutput);
    expect(runxAuxiliarySchemas.registryBinding).toBe(runxGeneratedSchemaArtifacts["registry-binding.schema.json"]);
    expect(runxAuxiliarySchemas.reviewReceiptOutput).toBe(runxGeneratedSchemaArtifacts["review-receipt-output.schema.json"]);
    expect(runxAuxiliarySchemas.registryBinding.$id).toBe(registryBindingSchema.$id);
    expect(runxAuxiliarySchemas.reviewReceiptOutput.$id).toBe(reviewReceiptOutputSchema.$id);
    expect(runxGeneratedSchemaArtifacts["doctor.schema.json"]).toBe(runxContractSchemas.doctor);
    expect(runxGeneratedSchemaArtifacts["act-assignment.schema.json"]).toBe(runxContractSchemas.actAssignment);
    expect(runxGeneratedSchemaArtifacts["receipt.schema.json"]).toBe(runxContractSchemas.receipt);
    expect(runxGeneratedSchemaArtifacts["run-summary.schema.json"]).toBe(runxContractSchemas.runSummary);
    const retiredReceiptArtifact = `${"harness"}-receipt.schema.json` as keyof typeof runxGeneratedSchemaArtifacts;
    expect(runxGeneratedSchemaArtifacts[retiredReceiptArtifact]).toBeUndefined();
    const retiredCentralArtifact = `${"engage"}ment.schema.json` as keyof typeof runxGeneratedSchemaArtifacts;
    const retiredEvidenceArtifact = `${"evidence"}-bundle.schema.json` as keyof typeof runxGeneratedSchemaArtifacts;
    expect(runxGeneratedSchemaArtifacts[retiredCentralArtifact]).toBeUndefined();
    expect(runxGeneratedSchemaArtifacts[retiredEvidenceArtifact])
      .toBeUndefined();
    expect(runxGeneratedSchemaArtifacts["handoff-signal.schema.json"]).toBe(runxContractSchemas.handoffSignal);
    expect(runxGeneratedSchemaArtifacts["handoff-state.schema.json"]).toBe(runxContractSchemas.handoffState);
    expect(runxGeneratedSchemaArtifacts["suppression-record.schema.json"]).toBe(runxContractSchemas.suppressionRecord);
    expect(runxGeneratedSchemaArtifacts["operational-policy.schema.json"]).toBe(runxContractSchemas.operationalPolicy);
    expect(runxGeneratedSchemaArtifacts["thread-outbox-provider-manifest.schema.json"])
      .toBe(runxContractSchemas.threadOutboxProviderManifest);
    expect(runxGeneratedSchemaArtifacts["thread-outbox-provider-push.schema.json"])
      .toBe(runxContractSchemas.threadOutboxProviderPush);
    expect(runxGeneratedSchemaArtifacts["thread-outbox-provider-fetch.schema.json"])
      .toBe(runxContractSchemas.threadOutboxProviderFetch);
    expect(runxGeneratedSchemaArtifacts["thread-outbox-provider-observation.schema.json"])
      .toBe(runxContractSchemas.threadOutboxProviderObservation);
    const retiredIssueArtifact = `${"issue"}-to-pr-${"out"}come.schema.json` as keyof typeof runxGeneratedSchemaArtifacts;
    expect(runxGeneratedSchemaArtifacts[retiredIssueArtifact]).toBeUndefined();
    expect(runxGeneratedSchemaArtifacts["review-receipt-output.schema.json"].$id).toBe(reviewReceiptOutputSchema.$id);
  });

  it("owns operational policy for issue intake and source-thread routing", () => {
    expect(RUNX_LOGICAL_SCHEMAS.operationalPolicy).toBe("runx.operational_policy.v1");
    expect(RUNX_CONTRACT_IDS.operationalPolicy).toBe("https://schemas.runx.dev/runx/operational-policy/v1.json");
    expect(runxContractSchemas.operationalPolicy.$id).toBe(RUNX_CONTRACT_IDS.operationalPolicy);
    const policy = {
      schema: RUNX_LOGICAL_SCHEMAS.operationalPolicy,
      schema_version: "runx.operational_policy.v1",
      policy_id: "example-dev-flow",
      sources: [{
        source_id: "bugs",
        provider: "slack",
        allowed_locators: ["slack://team/T123/channel/CBUGS"],
        allowed_actions: ["issue-intake", "issue-to-pr", "manual-review"],
        source_thread: {
          required: true,
          publish_mode: "reply",
          missing_behavior: "fail_closed",
        },
      }],
      runners: [{
        runner_id: "aster-primary",
        kind: "aster",
        state: "available",
        allowed_actions: ["issue-to-pr", "merge-assist"],
        target_repos: ["example/api"],
        scafld_required: true,
      }],
      owner_routes: [{
        route_id: "api-owner",
        owners: ["Kam"],
        target_repos: ["example/api"],
      }],
      targets: [{
        repo: "example/api",
        runner_ids: ["aster-primary"],
        allowed_actions: ["issue-to-pr", "merge-assist"],
        default_owner_route: "api-owner",
        scafld_required: true,
      }],
      dedupe: {
        strategy: "source_fingerprint",
        key_fields: ["source_locator", "target_repo"],
        on_duplicate: "reuse",
      },
      outcomes: {
        observe_provider: true,
        verification_required: true,
        close_source_issue: "when_verified",
        publish_final_source_thread_update: true,
      },
      permissions: {
        auto_merge: false,
        mutate_target_repo: true,
        require_human_merge_gate: true,
      },
    };
    expect(validateOperationalPolicyContract(policy)).toMatchObject({
      policy_id: "example-dev-flow",
    });
    expect(validateOperationalPolicySemantics(policy)).toMatchObject({
      policy_id: "example-dev-flow",
      permissions: {
        auto_merge: false,
      },
    });
  });

  it("owns the runx harness spine and retires retired central artifacts", () => {
    expect(RUNX_LOGICAL_SCHEMAS.receipt).toBe("runx.receipt.v1");
    expect("harnessReceipt" in RUNX_LOGICAL_SCHEMAS).toBe(false);
    expect("harnessReceipt" in RUNX_CONTRACT_IDS).toBe(false);
    const retiredCentralKey = `${"engage"}ment`;
    expect(retiredCentralKey in RUNX_LOGICAL_SCHEMAS).toBe(false);
    expect("evidenceBundle" in RUNX_LOGICAL_SCHEMAS).toBe(false);
    expect(retiredCentralKey in RUNX_CONTRACT_IDS).toBe(false);
    expect("evidenceBundle" in RUNX_CONTRACT_IDS).toBe(false);

    const issueRef = {
      type: "github_issue",
      uri: "github://runxhq/example/issues/101",
      provider: "github",
      locator: "runxhq/example#101",
      observed_at: "2026-05-18T00:00:00Z",
    };
    const principalRef = { type: "principal", uri: "runx:principal:agent_1" };
    const criterion = {
      criterion_id: "crit_revision_reviewable",
      statement: "Revision is available for review.",
      required: true,
    };
    const intent = {
      purpose: "Prepare a bounded revision for checkout retry behavior.",
      legitimacy: "The authenticated issue requests a fix in the target repository.",
      success_criteria: [criterion],
      constraints: ["Stay inside the checkout surface."],
      derived_from: [issueRef],
    };
    const verification = {
      status: "passed",
      checks: [{
        check_id: "check_pr_open",
        criterion_ids: ["crit_revision_reviewable"],
        status: "passed",
        evidence_refs: [{ type: "github_pull_request", uri: "github://runxhq/example/pulls/102" }],
      }],
      verified_at: "2026-05-18T00:02:00Z",
      evidence_refs: [{ type: "github_pull_request", uri: "github://runxhq/example/pulls/102" }],
    };
    const seal = {
      disposition: "closed",
      reason_code: "revision_ready",
      summary: "Revision act completed and reviewable PR was observed.",
      closed_at: "2026-05-18T00:03:00Z",
      last_observed_at: "2026-05-18T00:03:00Z",
      criteria: [{
        criterion_id: "crit_revision_reviewable",
        status: "verified",
        verification_refs: [{ type: "verification", uri: "runx:verification:check_pr_open" }],
        evidence_refs: [{ type: "github_pull_request", uri: "github://runxhq/example/pulls/102" }],
      }],
    };

    expect(validateSignalContract({
      schema: "runx.signal.v1",
      signal_id: "sig_101",
      source_ref: issueRef,
      authenticity: {
        host_ref: { type: "webhook_delivery", uri: "github://delivery/abc" },
        principal_ref: { type: "principal", uri: "github:user:octocat" },
        verified_by_ref: principalRef,
        trust_level: "verified_signature",
        verified_at: "2026-05-18T00:00:01Z",
      },
      signal_type: "issue_opened",
      title: "Checkout retry failure",
      body_preview: "Retry fails when the discount service flakes.",
      observed_at: "2026-05-18T00:00:00Z",
      evidence_refs: [issueRef],
    })).toMatchObject({ schema: "runx.signal.v1", signal_type: "issue_opened" });

    expect(validateReceiptContract({
      schema: "runx.receipt.v1",
      id: "hrn_rcpt_123",
      created_at: "2026-05-18T00:03:01Z",
      canonicalization: "runx.receipt.c14n.v1",
      issuer: {
        type: "local",
        kid: "key_1",
        public_key_sha256: "sha256:key",
      },
      signature: {
        alg: "Ed25519",
        value: "sig_123",
      },
      digest: "sha256:receipt",
      idempotency: {
        intent_key: "sha256:checkout-retry",
        trigger_fingerprint: "sha256:trigger",
        content_hash: "sha256:content",
      },
      subject: {
        kind: "skill",
        ref: { type: "harness", uri: "runx:harness:local-cli" },
        commitments: [{
          scope: "output",
          algorithm: "sha256",
          value: "sha256:private-transcript",
          canonicalization: "runx.artifact-hash.v1",
        }],
      },
      authority: {
        actor_ref: principalRef,
        grant_refs: [{ type: "grant", uri: "runx:grant:repo_write" }],
        scope_refs: [{ type: "scope_admission", uri: "runx:scope_admission:repo_write" }],
        authority_proof_refs: [{ type: "authority_proof", uri: "runx:authority_proof:proof_1" }],
        attenuation: { parent_authority_ref: null, subset_proof: null },
        terms: [{
          term_id: "term_repo_write",
          principal_ref: principalRef,
          resource_ref: { type: "github_repo", uri: "github://runxhq/example" },
          resource_family: "github_repo",
          verbs: ["read", "write", "create"],
          bounds: {
            repo_path_globs: ["app/checkout/**"],
            branch_patterns: ["runx/**"],
            max_child_depth: 1,
          },
          conditions: [{
            condition_id: "cond_signal_verified",
            predicate: "signal_verified",
            refs: [{ type: "signal", uri: "runx:signal:sig_101" }],
          }],
          approvals: [],
          capabilities: ["filesystem_read", "filesystem_write", "provider_mutation"],
          issued_by_ref: principalRef,
          credential_ref: { type: "credential", uri: "runx:credential:github_installation" },
        }],
        enforcement: {
          profile_hash: "sha256:profile",
          redaction_refs: [{ type: "redaction_policy", uri: "runx:redaction_policy:public_safe" }],
          setup_refs: [],
          teardown_refs: [],
        },
      },
      signals: [{ type: "signal", uri: "runx:signal:sig_101" }],
      decisions: [{
        decision_id: "dec_revision",
        choice: "open",
        inputs: {
          signal_refs: [{ type: "signal", uri: "runx:signal:sig_101" }],
          target_ref: null,
          opportunity_refs: [],
          selection_ref: null,
        },
        proposed_intent: intent,
        selected_act_id: "act_revision",
        selected_harness_ref: null,
        justification: {
          summary: "The authenticated issue authorizes a bounded checkout revision.",
          evidence_refs: [issueRef],
        },
        closure: null,
        artifact_refs: [],
      }],
      acts: [{
        id: "act_revision",
        form: "revision",
        intent,
        summary: "Prepared a reviewable checkout retry revision.",
        criterion_bindings: [{
          criterion_id: "crit_revision_reviewable",
          status: "verified",
          evidence_refs: [{ type: "github_pull_request", uri: "github://runxhq/example/pulls/102" }],
          verification_refs: [{ type: "verification", uri: "runx:verification:check_pr_open" }],
          summary: "Reviewable PR observed.",
        }],
        source_refs: [issueRef],
        target_refs: [{ type: "github_repo", uri: "github://runxhq/example" }],
        artifact_refs: [{ type: "artifact", uri: "runx:artifact:summary_1" }],
        context_ref: { type: "act", uri: "runx:act:act_revision_context" },
        closure: {
          disposition: "closed",
          reason_code: "revision_ready",
          summary: "Revision act completed.",
          closed_at: "2026-05-18T00:03:00Z",
        },
        revision: {
          change_request: {
            request_id: "cr_checkout_retry",
            summary: "Fix checkout retry behavior.",
            target_surfaces: [],
            success_criteria: [criterion],
          },
          change_plan: {
            plan_id: "cp_checkout_retry",
            summary: "Adjust retry guard and open a PR.",
            steps: ["Edit retry guard", "Open PR"],
          },
          target_surfaces: [],
          invariants: [],
          verification,
          handoff_refs: [],
        },
      }],
      seal,
      lineage: {
        children: [],
        sync: [],
      },
    })).toMatchObject({
      schema: "runx.receipt.v1",
      acts: [expect.objectContaining({ id: "act_revision" })],
    });

    expect(() => validateReceiptContract({
      schema: "runx.receipt.v1",
      id: "hrn_rcpt_bad",
    })).toThrow(/receipt\/v1\.json/);
    expect(() => validateSignalContract({
      schema: "runx.signal.v1",
      signal_id: "sig_bad",
      source_ref: { type: `${"evidence"}_bundle`, uri: `runx:${"evidence"}_bundle:old` },
      signal_type: "issue_opened",
      title: "Old evidence bundle ref",
      observed_at: "2026-05-18T00:00:00Z",
    })).toThrow(/signal\/v1\.json/);
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

  it("owns a generic act assignment envelope contract for host-neutral invocation", () => {
    expect(RUNX_CONTRACT_IDS.actAssignment).toBe("https://schemas.runx.dev/runx/act-assignment/v1.json");
    expect(runxContractSchemas.actAssignment.$id).toBe(RUNX_CONTRACT_IDS.actAssignment);

    expect(validateActAssignmentContract({
      schema: "runx.act_assignment.v1",
      skill_ref: "outreach",
      runner: "rerun",
      source_ref: "github://sourcey/sourcey.com/issues/3",
      requested_at: "2026-04-25T13:45:00Z",
      host: {
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
      skill_ref: "outreach",
      runner: "rerun",
      host: {
        kind: "github_issue_comment",
      },
      idempotency: {
        algorithm: "sha256",
      },
    });
  });

  it("routes public schema validation through Rust-generated artifacts", () => {
    const rustAuthoritativeAssignment = {
      schema: "runx.act_assignment.v1",
      skill_ref: "outreach",
      runner: "rerun",
      requested_at: "2026-04-25T13:45:00Z",
      host: {
        kind: "api",
        trigger_ref: "",
        scope_set: [""],
        actor: {
          actor_id: "",
        },
      },
      idempotency: {
        algorithm: "sha256",
        intent_key: "sha256:intent",
        content_hash: "sha256:content",
      },
    };

    expect(contractSchemaMatches(runxContractSchemas.actAssignment, rustAuthoritativeAssignment)).toBe(true);
    expect(contractSchemaMatches(actAssignmentV1Schema, rustAuthoritativeAssignment)).toBe(true);
    expect(validateActAssignmentContract(rustAuthoritativeAssignment)).toMatchObject({
      host: {
        kind: "api",
      },
    });
  });

  it("validates machine report payloads from Rust-generated contract schemas", () => {
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

  it("Aster feed entry proof bindings", () => {
    const targetRef = { type: "target", uri: "runx:target:aster-site" };
    const opportunityRef = { type: "opportunity", uri: "runx:opportunity:docs-gap" };
    const thesisRef = { type: "external_url", uri: "https://aster.runx.ai/thesis" };
    const selectionCycleRef = { type: "selection_cycle", uri: "runx:selection_cycle:cycle_1" };
    const selectionRef = { type: "selection", uri: "runx:selection:sel_1" };
    const decisionRef = { type: "decision", uri: "runx:decision:dec_1" };
    const receiptRef = { type: "receipt", uri: "runx:receipt:hrn_1" };
    const verificationRef = { type: "verification", uri: "runx:verification:ver_1" };
    const evidenceRef = { type: "artifact", uri: "runx:artifact:evidence_1" };
    const redactionPolicyRef = { type: "redaction_policy", uri: "runx:redaction_policy:public" };
    const sourceRef = { type: "signal", uri: "runx:signal:sig_1" };
    const fingerprint = {
      algorithm: "sha256",
      canonicalization: "runx.fingerprint.c14n.v1",
      value: "sha256:target",
      derived_from: [sourceRef],
    };
    const closure = {
      disposition: "closed",
      reason_code: "published",
      summary: "Public projection was published with proof bindings.",
      closed_at: "2026-05-18T00:05:00Z",
    };
    const actRef = {
      receipt_ref: receiptRef,
      act_id: "act_publish_feed",
    };

    expect(validateTargetContract({
      schema: "runx.target.v1",
      target_id: "target_1",
      target_ref: targetRef,
      title: "Aster public proof surface",
      lifecycle_state: "active",
      authority_refs: [{ type: "grant", uri: "runx:grant:aster_publication" }],
      fingerprint,
      cooldown: { state: "none" },
      verification_recipe_refs: [{ type: "verification", uri: "runx:verification_recipe:public_feed" }],
      created_at: "2026-05-18T00:00:00Z",
      updated_at: "2026-05-18T00:01:00Z",
    })).toMatchObject({ schema: "runx.target.v1", lifecycle_state: "active" });

    expect(validateOpportunityContract({
      schema: "runx.opportunity.v1",
      opportunity_id: "opp_1",
      target_ref: targetRef,
      summary: "Publish a clearer proof entry for the selected public surface.",
      proposed_form: "observation",
      value_score: 86,
      risk_score: 12,
      freshness_expires_at: "2026-05-19T00:00:00Z",
      fingerprint,
      source_refs: [sourceRef],
      evidence_refs: [evidenceRef],
      discovered_at: "2026-05-18T00:01:00Z",
    })).toMatchObject({ schema: "runx.opportunity.v1", proposed_form: "observation" });

    expect(validateThesisAssessmentContract({
      schema: "runx.thesis_assessment.v1",
      assessment_id: "assess_1",
      target_ref: targetRef,
      opportunity_ref: opportunityRef,
      thesis_ref: thesisRef,
      score: 91,
      rubric_refs: [thesisRef],
      proof_strength: "strong",
      authority_cost: "low",
      rationale: "The entry improves public proof without broadening authority.",
      evidence_refs: [evidenceRef],
      assessed_at: "2026-05-18T00:02:00Z",
    })).toMatchObject({ schema: "runx.thesis_assessment.v1", score: 91 });

    expect(validateSelectionContract({
      schema: "runx.selection.v1",
      selection_id: "sel_1",
      cycle_ref: selectionCycleRef,
      opportunity_ref: opportunityRef,
      candidate_refs: [opportunityRef],
      rank: 1,
      score: 91,
      selected: true,
      reason: "Highest value public proof candidate inside current authority.",
      decision_ref: decisionRef,
      evidence_refs: [evidenceRef],
      selected_at: "2026-05-18T00:03:00Z",
    })).toMatchObject({ schema: "runx.selection.v1", selected: true });

    expect(validateSkillBindingContract({
      schema: "runx.skill_binding.v1",
      binding_id: "binding_1",
      skill_ref: { type: "artifact", uri: "runx:skill:project-feed-entry" },
      scope_family: "publication",
      allowed_act_forms: ["observation"],
      authority_refs: [{ type: "grant", uri: "runx:grant:aster_publication" }],
      policy_refs: [redactionPolicyRef],
      harness_template_ref: { type: "harness", uri: "runx:harness_template:public_feed" },
      active: true,
      created_at: "2026-05-18T00:00:00Z",
      updated_at: "2026-05-18T00:01:00Z",
    })).toMatchObject({ schema: "runx.skill_binding.v1", allowed_act_forms: ["observation"] });

    expect(validateTargetTransitionEntryContract({
      schema: "runx.target_transition_entry.v1",
      entry_id: "tte_1",
      target_ref: targetRef,
      from_state: "eligible",
      to_state: "active",
      reason_code: "selected",
      summary: "Target entered the active selector set.",
      source_refs: [sourceRef],
      decision_ref: decisionRef,
      receipt_ref: receiptRef,
      recorded_at: "2026-05-18T00:03:30Z",
    })).toMatchObject({ schema: "runx.target_transition_entry.v1", to_state: "active" });

    expect(validateSelectionCycleContract({
      schema: "runx.selection_cycle.v1",
      cycle_id: "cycle_1",
      state: "closed",
      started_at: "2026-05-18T00:00:00Z",
      closed_at: "2026-05-18T00:04:00Z",
      input_refs: [sourceRef],
      target_refs: [targetRef],
      opportunity_refs: [opportunityRef],
      ranked_selection_refs: [selectionRef],
      chosen_selection_ref: selectionRef,
      decision_ref: decisionRef,
      receipt_ref: receiptRef,
      no_action_closure: null,
      fingerprint,
    })).toMatchObject({ schema: "runx.selection_cycle.v1", state: "closed" });

    expect(validateActContract({
      act_id: "act_publish_feed",
      form: "observation",
      intent: {
        purpose: "Publish a public-safe proof projection.",
        legitimacy: "Aster selected the opportunity under publication authority.",
        success_criteria: [{
          criterion_id: "crit_public_proof",
          statement: "The feed entry cites proof, verification, and redaction policy.",
          required: true,
        }],
        constraints: ["Do not publish private evidence."],
        derived_from: [selectionRef],
      },
      summary: "Projected a public proof entry.",
      closure,
      criterion_bindings: [{
        criterion_id: "crit_public_proof",
        status: "verified",
        evidence_refs: [evidenceRef],
        verification_refs: [verificationRef],
      }],
      source_refs: [selectionRef],
      target_refs: [targetRef],
      surface_refs: [],
      artifact_refs: [evidenceRef],
      verification_refs: [verificationRef],
      harness_refs: [{ type: "harness", uri: "runx:harness:hrn_1" }],
      performed_at: "2026-05-18T00:04:00Z",
    })).toMatchObject({ form: "observation", closure });

    expect(validateReflectionEntryContract({
      schema: "runx.reflection_entry.v1",
      reflection_id: "reflect_1",
      target_ref: targetRef,
      opportunity_ref: opportunityRef,
      selection_ref: selectionRef,
      decision_ref: decisionRef,
      receipt_refs: [receiptRef],
      act_refs: [actRef],
      summary: "Public proof entry was useful and low-risk.",
      lessons: ["Keep feed projections tied to sealed harness receipts."],
      follow_up_refs: [],
      evidence_refs: [evidenceRef],
      recorded_at: "2026-05-18T00:06:00Z",
    })).toMatchObject({ schema: "runx.reflection_entry.v1" });

    const feedEntry = validateFeedEntryContract({
      schema: "runx.feed_entry.v1",
      feed_entry_id: "feed_1",
      public_at: "2026-05-18T00:07:00Z",
      title: "Aster published a proof-bound entry",
      summary: "The public entry cites a sealed harness receipt, contained act, decision, verification, and redaction policy.",
      target_ref: targetRef,
      opportunity_ref: opportunityRef,
      selection_ref: selectionRef,
      decision_refs: [decisionRef],
      receipt_refs: [receiptRef],
      act_refs: [actRef],
      verification_refs: [verificationRef],
      evidence_refs: [evidenceRef],
      artifact_refs: [evidenceRef],
      redaction_policy_ref: redactionPolicyRef,
      redaction_refs: [{ type: "redaction_policy", uri: "runx:redaction:redaction_1" }],
    });

    expect(runxContractSchemas.feedEntry.$id).toBe(RUNX_CONTRACT_IDS.feedEntry);
    expect(runxGeneratedSchemaArtifacts["feed-entry.schema.json"]).toBe(runxContractSchemas.feedEntry);
    expect(feedEntry).toMatchObject({
      schema: "runx.feed_entry.v1",
      decision_refs: [decisionRef],
      receipt_refs: [receiptRef],
      act_refs: [actRef],
      verification_refs: [verificationRef],
      redaction_policy_ref: redactionPolicyRef,
    });
    expect(() => validateFeedEntryContract({
      ...feedEntry,
      receipt_refs: [],
    })).toThrow(/feed-entry\/v1\.json/);
  });
});
