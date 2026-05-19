use runx_contracts::{
    AuthorityApproval, AuthorityBounds, AuthorityCapability, AuthorityCondition,
    AuthorityConditionPredicate, AuthorityResourceFamily, AuthorityTerm, AuthorityVerb, Decision,
    DecisionChoice, DecisionInputs, DecisionJustification, Intent, PaymentAuthorityBounds,
    PaymentCredentialForm, Reference, ReferenceType,
};
use runx_runtime::{
    PaymentAuthorityError, PaymentRailAuthorization, PaymentRailAuthorizationDecision,
    PaymentSpendCapabilityBinding, authorize_payment_rail,
};

const ACT_ID: &str = "act_payment_spend";
const IDEMPOTENCY_KEY: &str = "idem:decision_payment_reservation:harness-payment-rail";
const COUNTERPARTY: &str = "merchant-123";

#[test]
fn admits_reserved_spend_with_subset_proof_and_rail_proof() {
    let scenario = PaymentScenario::standard();

    let result = scenario.authorize_decision();

    assert_eq!(
        result.map(|decision| (
            decision.parent_term_id,
            decision.child_term_id,
            decision.idempotency_key,
            decision.rail_proof_refs.len(),
        )),
        Ok(("parent", "child", Some(IDEMPOTENCY_KEY), 1))
    );
}

#[test]
fn denies_amount_widening_before_rail() {
    let mut scenario = PaymentScenario::with_child(payment_term(
        "child",
        vec![AuthorityVerb::Spend],
        PaymentShape::new(2_000, &["card"]),
    ));
    scenario.parent = payment_term(
        "parent",
        vec![AuthorityVerb::Reserve, AuthorityVerb::Spend],
        PaymentShape::new(1_000, &["card"]),
    );

    assert_eq!(
        scenario.authorize(),
        Err(PaymentAuthorityError::AuthorityNotSubset)
    );
}

#[test]
fn denies_spend_derived_from_reserve_only_parent() {
    let mut scenario = PaymentScenario::standard();
    scenario.parent.verbs = vec![AuthorityVerb::Reserve, AuthorityVerb::Verify];

    assert_eq!(
        scenario.authorize(),
        Err(PaymentAuthorityError::AuthorityNotSubset)
    );
}

#[test]
fn denies_removing_parent_conditions_or_approvals() {
    let mut scenario = PaymentScenario::standard();
    scenario.parent.conditions = vec![payment_condition()];
    scenario.parent.approvals = vec![payment_approval()];
    scenario.child.conditions = Vec::new();
    scenario.child.approvals = Vec::new();

    assert_eq!(
        scenario.authorize(),
        Err(PaymentAuthorityError::AuthorityNotSubset)
    );
}

#[test]
fn admits_child_that_preserves_parent_conditions_and_approvals() {
    let mut scenario = PaymentScenario::standard();
    let condition = payment_condition();
    let approval = payment_approval();
    scenario.parent.conditions = vec![condition.clone()];
    scenario.parent.approvals = vec![approval.clone()];
    scenario.child.conditions = vec![condition];
    scenario.child.approvals = vec![approval];

    assert_eq!(scenario.authorize(), Ok(()));
}

#[test]
fn denies_missing_reservation_decision() {
    let scenario = PaymentScenario::standard();

    assert_eq!(
        scenario.authorize_with(AuthorizationOverride {
            reservation_decision: Some(None),
            ..AuthorizationOverride::default()
        }),
        Err(PaymentAuthorityError::MissingReservationDecision)
    );
}

#[test]
fn denies_unselected_reservation_decision() {
    let scenario = PaymentScenario::with_decision(unselected_decision());

    assert_eq!(
        scenario.authorize(),
        Err(PaymentAuthorityError::ReservationDecisionNotSelected)
    );
}

#[test]
fn denies_missing_subset_proof() {
    let scenario = PaymentScenario::standard();

    assert_eq!(
        scenario.authorize_with(AuthorizationOverride {
            subset_proof_present: Some(false),
            ..AuthorizationOverride::default()
        }),
        Err(PaymentAuthorityError::MissingSubsetProof)
    );
}

#[test]
fn denies_missing_idempotency_key_for_spend() {
    let scenario = PaymentScenario::standard();

    assert_eq!(
        scenario.authorize_with(AuthorizationOverride {
            idempotency_key: Some(None),
            ..AuthorizationOverride::default()
        }),
        Err(PaymentAuthorityError::MissingIdempotencyKey)
    );
}

#[test]
fn denies_wildcard_counterparty_for_spend() {
    let scenario = PaymentScenario::with_child(child_wildcard_counterparty_term());

    assert_eq!(
        scenario.authorize(),
        Err(PaymentAuthorityError::WildcardCounterpartyDenied)
    );
}

#[test]
fn denies_spend_capability_binding_that_does_not_match_act() {
    let scenario = PaymentScenario::standard();

    assert_eq!(
        scenario.authorize_with(AuthorizationOverride {
            spend_capability_binding: Some(Some(PaymentSpendCapabilityBinding {
                act_id: "act_payment_other",
                ..scenario.capability_binding()
            })),
            ..AuthorizationOverride::default()
        }),
        Err(PaymentAuthorityError::SpendCapabilityBindingMismatch)
    );
}

#[test]
fn denies_missing_rail_proof_when_receipt_before_success_required() {
    let scenario = PaymentScenario::standard();

    assert_eq!(
        scenario.authorize_with(AuthorizationOverride {
            rail_proof_refs: Some(&[]),
            ..AuthorizationOverride::default()
        }),
        Err(PaymentAuthorityError::MissingReceiptBeforeSuccess)
    );
}

#[test]
fn denies_sibling_reuse_of_single_use_spend_capability() {
    let mut scenario = PaymentScenario::standard();
    scenario.consumed_spend_capability_refs = vec![scenario.spend_capability_ref.clone()];

    assert_eq!(
        scenario.authorize(),
        Err(PaymentAuthorityError::SpendCapabilityAlreadyConsumed)
    );
}

struct PaymentScenario {
    parent: AuthorityTerm,
    child: AuthorityTerm,
    decision: Decision,
    rail_proof_refs: Vec<Reference>,
    consumed_spend_capability_refs: Vec<Reference>,
    child_harness_ref: Reference,
    spend_capability_ref: Reference,
}

impl PaymentScenario {
    fn standard() -> Self {
        Self::with_child(child_spend_term())
    }

    fn with_child(child: AuthorityTerm) -> Self {
        Self {
            parent: parent_spend_term(),
            child,
            decision: selected_decision(),
            rail_proof_refs: vec![reference(ReferenceType::Receipt, "runx:receipt:rail-1")],
            consumed_spend_capability_refs: Vec::new(),
            child_harness_ref: reference(
                ReferenceType::Harness,
                "runx:harness:harness-payment-rail",
            ),
            spend_capability_ref: reference(
                ReferenceType::Credential,
                "runx:payment-capability:spend-1",
            ),
        }
    }

    fn with_decision(decision: Decision) -> Self {
        Self {
            decision,
            ..Self::standard()
        }
    }

    fn authorize(&self) -> Result<(), PaymentAuthorityError> {
        self.authorize_with(AuthorizationOverride::default())
    }

    fn authorize_decision(
        &self,
    ) -> Result<PaymentRailAuthorizationDecision<'_>, PaymentAuthorityError> {
        let binding = self.capability_binding();

        authorize_payment_rail(PaymentRailAuthorization {
            parent_authority: &self.parent,
            child_authority: &self.child,
            reservation_decision: Some(&self.decision),
            subset_proof_present: true,
            child_harness_ref: &self.child_harness_ref,
            act_id: ACT_ID,
            idempotency_key: Some(IDEMPOTENCY_KEY),
            spend_capability_binding: Some(binding),
            rail_proof_refs: &self.rail_proof_refs,
            consumed_spend_capability_refs: &self.consumed_spend_capability_refs,
            spend_capability_ref: Some(&self.spend_capability_ref),
        })
    }

    fn authorize_with(
        &self,
        overrides: AuthorizationOverride<'_>,
    ) -> Result<(), PaymentAuthorityError> {
        let default_binding = self.capability_binding();
        let reservation_decision = overrides
            .reservation_decision
            .unwrap_or(Some(&self.decision));
        let idempotency_key = overrides.idempotency_key.unwrap_or(Some(IDEMPOTENCY_KEY));
        let spend_capability_binding = overrides
            .spend_capability_binding
            .unwrap_or(Some(default_binding));
        let rail_proof_refs = overrides.rail_proof_refs.unwrap_or(&self.rail_proof_refs);
        let subset_proof_present = overrides.subset_proof_present.unwrap_or(true);

        authorize_payment_rail(PaymentRailAuthorization {
            parent_authority: &self.parent,
            child_authority: &self.child,
            reservation_decision,
            subset_proof_present,
            child_harness_ref: &self.child_harness_ref,
            act_id: ACT_ID,
            idempotency_key,
            spend_capability_binding,
            rail_proof_refs,
            consumed_spend_capability_refs: &self.consumed_spend_capability_refs,
            spend_capability_ref: Some(&self.spend_capability_ref),
        })
        .map(|_| ())
    }

    fn capability_binding(&self) -> PaymentSpendCapabilityBinding<'_> {
        PaymentSpendCapabilityBinding {
            child_harness_ref: &self.child_harness_ref,
            act_id: ACT_ID,
            reservation_decision_id: "decision_payment_reservation",
            idempotency_key: IDEMPOTENCY_KEY,
            amount_minor: 1_250,
            currency: "USD",
            counterparty: COUNTERPARTY,
            rail: "card",
        }
    }
}

#[derive(Default)]
struct AuthorizationOverride<'a> {
    reservation_decision: Option<Option<&'a Decision>>,
    subset_proof_present: Option<bool>,
    idempotency_key: Option<Option<&'a str>>,
    spend_capability_binding: Option<Option<PaymentSpendCapabilityBinding<'a>>>,
    rail_proof_refs: Option<&'a [Reference]>,
}

fn parent_spend_term() -> AuthorityTerm {
    payment_term(
        "parent",
        vec![
            AuthorityVerb::Quote,
            AuthorityVerb::Reserve,
            AuthorityVerb::Spend,
            AuthorityVerb::Verify,
        ],
        PaymentShape::new(10_000, &["card", "ach"]),
    )
}

fn child_spend_term() -> AuthorityTerm {
    payment_term(
        "child",
        vec![AuthorityVerb::Reserve, AuthorityVerb::Spend],
        PaymentShape::new(2_500, &["card"]),
    )
}

fn child_wildcard_counterparty_term() -> AuthorityTerm {
    let mut term = child_spend_term();
    if let Some(payment) = term.bounds.payment.as_mut() {
        payment.counterparty = Some("*".to_owned());
    }
    term
}

struct PaymentShape {
    max_per_call_minor: u64,
    rails: Vec<String>,
}

impl PaymentShape {
    fn new(max_per_call_minor: u64, rails: &[&str]) -> Self {
        Self {
            max_per_call_minor,
            rails: rails.iter().map(|rail| (*rail).to_owned()).collect(),
        }
    }
}

fn payment_term(term_id: &str, verbs: Vec<AuthorityVerb>, shape: PaymentShape) -> AuthorityTerm {
    AuthorityTerm {
        term_id: term_id.to_owned(),
        principal_ref: reference(ReferenceType::Principal, "runx:principal:merchant-agent"),
        resource_ref: reference(ReferenceType::Grant, "runx:payment-grant:checkout"),
        resource_family: AuthorityResourceFamily::Payment,
        verbs,
        bounds: AuthorityBounds {
            payment: Some(PaymentAuthorityBounds {
                currency: "USD".to_owned(),
                max_per_call_minor: Some(shape.max_per_call_minor),
                max_per_run_minor: Some(25_000),
                max_per_period_minor: None,
                period: None,
                rails: shape.rails,
                realm: None,
                counterparty: Some(COUNTERPARTY.to_owned()),
                operation: Some("checkout".to_owned()),
                quote_ttl_ms: Some(120_000),
                approval_threshold_minor: Some(7_500),
                credential_form: Some(PaymentCredentialForm::SingleUseSpendCapability),
                quote_required: true,
                reservation_required: true,
                idempotency_required: true,
                recovery_required: true,
                receipt_before_success: true,
                single_use_spend: true,
            }),
            ..AuthorityBounds::default()
        },
        conditions: Vec::new(),
        approvals: Vec::new(),
        capabilities: vec![AuthorityCapability::PaymentSingleUseSpend],
        expires_at: Some("2026-05-21T00:00:00Z".to_owned()),
        issued_by_ref: reference(ReferenceType::Grant, "runx:grant:issuer"),
        credential_ref: Some(reference(
            ReferenceType::Credential,
            "runx:credential:payment-session",
        )),
    }
}

fn selected_decision() -> Decision {
    Decision {
        decision_id: "decision_payment_reservation".to_owned(),
        choice: DecisionChoice::Continue,
        inputs: DecisionInputs::default(),
        proposed_intent: intent(),
        selected_act_id: Some(ACT_ID.to_owned()),
        selected_harness_ref: None,
        justification: DecisionJustification {
            summary: "reservation selected a bounded spend act".to_owned(),
            evidence_refs: Vec::new(),
        },
        closure: None,
        artifact_refs: Vec::new(),
    }
}

fn unselected_decision() -> Decision {
    Decision {
        selected_act_id: None,
        selected_harness_ref: None,
        ..selected_decision()
    }
}

fn intent() -> Intent {
    Intent {
        purpose: "complete a bounded checkout payment".to_owned(),
        legitimacy: "authorized by selected reservation decision".to_owned(),
        success_criteria: Vec::new(),
        constraints: Vec::new(),
        derived_from: Vec::new(),
    }
}

fn reference(reference_type: ReferenceType, uri: &str) -> Reference {
    Reference {
        reference_type,
        uri: uri.to_owned(),
        provider: None,
        locator: None,
        label: None,
        observed_at: None,
    }
}

fn payment_condition() -> AuthorityCondition {
    AuthorityCondition {
        condition_id: "condition_payment_receipt".to_owned(),
        predicate: AuthorityConditionPredicate::PaymentReceiptPresent,
        refs: Vec::new(),
        parameters: None,
    }
}

fn payment_approval() -> AuthorityApproval {
    AuthorityApproval {
        approval_ref: reference(ReferenceType::Decision, "runx:decision:payment-approval"),
        approved_by_ref: Some(reference(
            ReferenceType::Principal,
            "runx:principal:operator",
        )),
        approved_at: Some("2026-05-20T00:00:00Z".to_owned()),
        criterion_ids: vec!["payment_receipt".to_owned()],
    }
}
