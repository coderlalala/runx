//! Shared Rust contract types for runx JSON and protocol boundaries.

// Lets the `#[derive(RunxSchema)]` output reference `::runx_contracts::schema`
// from inside this crate, the same way serde_derive references `::serde`.
extern crate self as runx_contracts;

pub mod act;
pub mod act_assignment;
pub mod agent_context;
pub mod artifact;
pub mod aster;
pub mod authority;
pub mod cli;
pub mod credential_delivery;
pub mod decision;
pub mod doctor;
pub mod execution;
pub mod external_adapter;
pub mod fingerprint;
pub mod handoff;
pub mod host_protocol;
pub mod json;
pub mod links;
pub mod maturity;
pub mod operational_policy;
pub mod output;
pub mod packet_index;
pub mod policy_proof;
pub mod post_merge_observer;
pub mod receipt;
pub mod receipts;
pub mod redaction;
pub mod reference;
pub mod registry;
pub mod registry_binding;
pub mod review;
pub mod schema;
pub mod signal;
pub mod suppression;
pub mod target_runner;
pub mod thread_outbox_provider;
pub mod tools;
pub mod verification;

pub use act::{
    Act, ActForm, ActSchema, ChangePlan, ChangeRequest, CriterionBinding, CriterionStatus,
    GovernedActRef, Intent, RevisionDetails, SuccessCriterion, TargetSurface, VerificationDetails,
};
pub use act_assignment::{
    ActAssignment, ActAssignmentActor, ActAssignmentHost, ActAssignmentHostKind,
    ActAssignmentIdempotency, ActAssignmentSchema, BuildActAssignment, IntentKeyInput,
    derive_content_hash, derive_intent_key, derive_trigger_key,
};
pub use agent_context::{
    AgentContextEnvelope, AgentContextProfiles, ContextArtifactMeta, ContextArtifactProducer,
    ContextEntry, ContextEntryVersion, ExecutionLocation, ProfileFile, ProvenanceEntry,
    QualityProfile, QualityProfileSource,
};
pub use artifact::{ARTIFACT_SCHEMA, Artifact, ArtifactProducedBy, ArtifactSchema};
pub use aster::{
    AuthorityCostLevel, FeedEntry, FeedEntrySchema, Opportunity, OpportunitySchema,
    ReflectionEntry, ReflectionEntrySchema, Selection, SelectionCycle, SelectionCycleSchema,
    SelectionCycleState, SelectionSchema, SkillBinding, SkillBindingSchema, Target, TargetCooldown,
    TargetCooldownState, TargetLifecycleState, TargetSchema, TargetTransitionEntry,
    TargetTransitionEntrySchema, ThesisAssessment, ThesisAssessmentSchema, ThesisProofStrength,
};
pub use authority::{
    Authority, AuthorityApproval, AuthorityAttenuation, AuthorityBounds, AuthorityCapability,
    AuthorityCondition, AuthorityConditionPredicate, AuthorityResourceFamily, AuthoritySchema,
    AuthoritySubsetComparison, AuthoritySubsetProof, AuthoritySubsetRelation,
    AuthoritySubsetResult, AuthorityTerm, AuthorityVerb, PaymentAuthorityBounds,
    PaymentCredentialForm,
};
pub use credential_delivery::{
    CredentialDeliveryBrokerResponse, CredentialDeliveryBrokerResponseSchema,
    CredentialDeliveryEnvBinding, CredentialDeliveryHandle, CredentialDeliveryMode,
    CredentialDeliveryObservation, CredentialDeliveryObservationSchema,
    CredentialDeliveryObservationStatus, CredentialDeliveryProfile,
    CredentialDeliveryProfileSchema, CredentialDeliveryPurpose, CredentialDeliveryRequest,
    CredentialDeliveryRequestSchema, CredentialDeliveryStatus, CredentialMaterialRole,
};
pub use decision::{
    Closure, ClosureDisposition, Decision, DecisionChoice, DecisionInputs, DecisionJustification,
};
pub use doctor::{
    DoctorDiagnostic, DoctorDiagnosticSeverity, DoctorLocation, DoctorRepair,
    DoctorRepairConfidence, DoctorRepairKind, DoctorRepairRisk, DoctorReport, DoctorReportSchema,
    DoctorStatus, DoctorSummary,
};
pub use execution::{
    ExecutionSemantics, GovernedDisposition, InputContextCapture, OutcomeState, ReceiptOutcome,
    ReceiptSurfaceRef,
};
pub use external_adapter::{
    EXTERNAL_ADAPTER_PROTOCOL_VERSION, ExternalAdapterArtifactObservation,
    ExternalAdapterCancellationFrame, ExternalAdapterCancellationSchema,
    ExternalAdapterCredentialNeed, ExternalAdapterCredentialPurpose,
    ExternalAdapterCredentialReference, ExternalAdapterCredentialRequest,
    ExternalAdapterCredentialRequestSchema, ExternalAdapterErrorObservation,
    ExternalAdapterHostResolutionFrame, ExternalAdapterHostResolutionSchema,
    ExternalAdapterInvocation, ExternalAdapterInvocationSchema, ExternalAdapterManifest,
    ExternalAdapterManifestSchema, ExternalAdapterProtocolVersion, ExternalAdapterResponse,
    ExternalAdapterSandboxIntent, ExternalAdapterStatus, ExternalAdapterTelemetryObservation,
    ExternalAdapterTelemetryValue, ExternalAdapterTimeouts, ExternalAdapterTransport,
    ExternalAdapterTransportKind,
};
pub use fingerprint::{Fingerprint, FingerprintAlgorithm, hex_lower, sha256_hex, sha256_prefixed};
pub use handoff::{
    HandoffDisposition, HandoffSignal, HandoffSignalActor, HandoffSignalSchema,
    HandoffSignalSource, HandoffSignalSourceRef, HandoffState, HandoffStateSchema, HandoffStatus,
    SuppressionReason,
};
pub use host_protocol::{
    AgentActInvocation, AgentActSourceType, ApprovalDecision, ApprovalGate, ExecutionEvent,
    HostNeedsAgentState, HostRunApproval, HostRunApprovalDecision, HostRunKind, HostRunLineage,
    HostRunLineageKind, HostRunResult, HostRunState, HostRunVerification,
    HostRunVerificationStatus, HostTerminalState, Question, ResolutionRequest, ResolutionResponse,
    ResolutionResponseActor,
};
pub use json::{JsonNumber, JsonObject, JsonValue};
pub use links::{DuplicateCandidate, Links};
pub use operational_policy::{
    OperationalPolicy, OperationalPolicyAction, OperationalPolicyAdmission,
    OperationalPolicyAdmissionRequest, OperationalPolicyAdmissionStatus,
    OperationalPolicyAutomationPermissions, OperationalPolicyDedupePolicy,
    OperationalPolicyDedupeStrategy, OperationalPolicyDuplicateBehavior, OperationalPolicyError,
    OperationalPolicyMissingBehavior, OperationalPolicyOutcomeCloseMode,
    OperationalPolicyOutcomePolicy, OperationalPolicyOwnerRoute, OperationalPolicyPublishMode,
    OperationalPolicyReadback, OperationalPolicyRunnerKind, OperationalPolicyRunnerReadback,
    OperationalPolicyRunnerRule, OperationalPolicyRunnerState, OperationalPolicySchema,
    OperationalPolicySentryPolicy, OperationalPolicySourceProvider,
    OperationalPolicySourceReadback, OperationalPolicySourceRule,
    OperationalPolicySourceThreadPolicy, OperationalPolicyTargetReadback,
    OperationalPolicyTargetRule, OperationalPolicyValidationFinding,
    admit_operational_policy_request, lint_operational_policy_contract,
    project_operational_policy_readback, validate_operational_policy_contract,
    validate_operational_policy_semantics,
};
pub use output::{OutputField, OutputFieldSpec, OutputType};
pub use packet_index::{PacketIndex, PacketIndexEntry, PacketIndexSchema};
pub use post_merge_observer::{
    PostMergeObserverClosureState, PostMergeObserverCriterionPlan,
    PostMergeObserverIdempotencyPlan, PostMergeObserverPlan, PostMergeObserverPlanError,
    PostMergeObserverPlanRequest, PostMergeObserverProviderPlan, PostMergeObserverPublicationPlan,
    PostMergeObserverPublicationProjection, PostMergeObserverRuntimeDecision,
    PostMergeObserverRuntimeDedupePlan, PostMergeObserverSignalSource,
    PostMergeObserverSourceIssuePlan, PostMergeProvider, PostMergePullRequestObservation,
    PostMergePullRequestState, PostMergeSourceIssueDisposition, PostMergeVerificationObservation,
    PostMergeVerificationStatus, plan_post_merge_observer_closure,
    plan_post_merge_observer_runtime_dedupe, project_post_merge_observer_publication_from_receipt,
};
pub use receipt::{
    FanoutReceiptDecision, FanoutReceiptStrategy, FanoutReceiptSyncPoint, Lineage,
    RECEIPT_CANONICALIZATION, RECEIPT_SCHEMA, Receipt, ReceiptAct, ReceiptAuthority,
    ReceiptCommitment, ReceiptCommitmentScope, ReceiptEnforcement, ReceiptIdempotency,
    ReceiptInputContext, ReceiptIssuer, ReceiptIssuerType, ReceiptSchema, ReceiptSignature,
    ReceiptSubjectKind, RunnerProvenance, Seal, SignatureAlgorithm, Subject,
};
pub use redaction::{HashAlgorithm, HashCommitment, REDACTION_SCHEMA, Redaction, RedactionSchema};
pub use reference::{ActRef, ProofKind, Reference, ReferenceType};
pub use registry_binding::{
    RegistryBinding, RegistryBindingHarness, RegistryBindingRegistry, RegistryBindingSchema,
    RegistryBindingSkill, RegistryBindingState, RegistryBindingUpstream, RegistryHarnessStatus,
    RegistryTrustTier,
};
pub use review::{ReviewReceiptImprovementProposal, ReviewReceiptOutput, ReviewReceiptVerdict};
pub use signal::{
    SIGNAL_SCHEMA, Signal, SignalAuthenticity, SignalSchema, SignalTrustLevel, SignalType,
};
pub use suppression::{SuppressionRecord, SuppressionRecordSchema, SuppressionScope};
pub use target_runner::{
    TargetRepoRunnerCheckoutPlan, TargetRepoRunnerDedupeComponent,
    TargetRepoRunnerDedupeLookupExecution, TargetRepoRunnerDedupeLookupObservation,
    TargetRepoRunnerDedupeLookupPlan, TargetRepoRunnerDedupeLookupQuery,
    TargetRepoRunnerDedupePlan, TargetRepoRunnerDedupeResult, TargetRepoRunnerExecutionPlan,
    TargetRepoRunnerExistingPullRequest, TargetRepoRunnerOwnerPlan, TargetRepoRunnerPlan,
    TargetRepoRunnerPlanError, TargetRepoRunnerPlanRequest, TargetRepoRunnerProvider,
    TargetRepoRunnerProviderPullRequest, TargetRepoRunnerPullRequestDisposition,
    TargetRepoRunnerPullRequestReceiptPlan, TargetRepoRunnerReadinessObservation,
    TargetRepoRunnerReadinessPlan, TargetRepoRunnerRunnerPlan, TargetRepoRunnerSourceContext,
    TargetRepoRunnerSourcePlan, TargetRepoRunnerSourcePublicationReceiptPlan,
    TargetRepoRunnerSourceThreadPlan, TargetRepoRunnerTargetPlan,
    apply_target_repo_runner_dedupe_lookup_execution, execute_target_repo_runner_dedupe_lookup,
    plan_target_repo_runner, plan_target_repo_runner_dedupe_lookup,
    plan_target_repo_runner_execution, plan_target_repo_runner_pull_request_receipt,
    plan_target_repo_runner_source_publication_receipt,
};
pub use thread_outbox_provider::{
    THREAD_OUTBOX_PROVIDER_PROTOCOL_VERSION, ThreadOutboxProviderCredentialNeed,
    ThreadOutboxProviderCredentialProfile, ThreadOutboxProviderError, ThreadOutboxProviderFetch,
    ThreadOutboxProviderFetchProviderTarget, ThreadOutboxProviderFetchSchema,
    ThreadOutboxProviderFetchTarget, ThreadOutboxProviderFetchThreadTarget,
    ThreadOutboxProviderIdempotency, ThreadOutboxProviderIdempotencyObservation,
    ThreadOutboxProviderIdempotencyStatus, ThreadOutboxProviderLocator,
    ThreadOutboxProviderManifest, ThreadOutboxProviderManifestSchema,
    ThreadOutboxProviderObservation, ThreadOutboxProviderObservationSchema,
    ThreadOutboxProviderObservationStatus, ThreadOutboxProviderOperation,
    ThreadOutboxProviderPayloadFormat, ThreadOutboxProviderProtocolVersion,
    ThreadOutboxProviderPush, ThreadOutboxProviderPushSchema, ThreadOutboxProviderReadbackSummary,
    ThreadOutboxProviderReceiptCapabilities, ThreadOutboxProviderReceiptContext,
    ThreadOutboxProviderRedactionCapabilities, ThreadOutboxProviderRenderedPayload,
    ThreadOutboxProviderThreadLocator, ThreadOutboxProviderTransport,
    ThreadOutboxProviderTransportKind,
};
pub use verification::{
    ReceiptVerificationSummary, VERIFICATION_SCHEMA, Verification, VerificationCheck,
    VerificationSchema, VerificationStatus,
};
