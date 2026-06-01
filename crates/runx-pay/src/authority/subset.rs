//! Subset comparator for payment authority terms.
//!
//! Fail-closed by construction: missing required payment dimensions make terms
//! incomparable, and incomparable terms are denied.

use runx_contracts::{
    AuthorityCapability, AuthorityResourceFamily, AuthorityTerm, AuthorityVerb,
    PaymentAuthorityBounds, PaymentCredentialForm,
};

use super::payment_authority_spends;
use runx_core::policy::authority_algebra::{
    items_subset, optional_bound_subset, optional_exact_or_narrower, optional_ref_bound_subset,
    parent_items_preserved, same_reference_address,
};

#[must_use]
pub fn is_payment_authority_subset(child: &AuthorityTerm, parent: &AuthorityTerm) -> bool {
    child.resource_family == AuthorityResourceFamily::Payment
        && parent.resource_family == AuthorityResourceFamily::Payment
        && same_reference_address(&child.resource_ref, &parent.resource_ref)
        && items_subset(&child.verbs, &parent.verbs)
        && items_subset(&child.capabilities, &parent.capabilities)
        && parent_items_preserved(&child.conditions, &parent.conditions)
        && parent_items_preserved(&child.approvals, &parent.approvals)
        && optional_ref_bound_subset(child.expires_at.as_ref(), parent.expires_at.as_ref())
        && payment_bounds_subset(child, parent)
}

fn payment_bounds_subset(child: &AuthorityTerm, parent: &AuthorityTerm) -> bool {
    let Some(child_payment) = child.bounds.payment.as_ref() else {
        return false;
    };
    let Some(parent_payment) = parent.bounds.payment.as_ref() else {
        return false;
    };

    required_currency_equal(child_payment, parent_payment)
        && minor_unit_caps_subset(child, child_payment, parent_payment)
        && rails_subset(child_payment, parent_payment)
        && optional_exact_or_narrower(&child_payment.realm, &parent_payment.realm)
        && optional_exact_or_narrower(&child_payment.counterparty, &parent_payment.counterparty)
        && optional_exact_or_narrower(&child_payment.operation, &parent_payment.operation)
        && optional_exact_or_narrower(&child_payment.period, &parent_payment.period)
        && required_booleans_subset(child_payment, parent_payment)
        && optional_bound_subset(child_payment.quote_ttl_ms, parent_payment.quote_ttl_ms)
        && optional_bound_subset(
            child_payment.approval_threshold_minor,
            parent_payment.approval_threshold_minor,
        )
        && credential_form_exact_match(child_payment, parent_payment)
        && spend_credential_form_granted(child, parent)
}

fn required_currency_equal(
    child: &PaymentAuthorityBounds,
    parent: &PaymentAuthorityBounds,
) -> bool {
    child.currency == parent.currency
}

fn minor_unit_caps_subset(
    child: &AuthorityTerm,
    child_payment: &PaymentAuthorityBounds,
    parent_payment: &PaymentAuthorityBounds,
) -> bool {
    if uses_minor_units(child) && child_payment.max_per_call_minor.is_none() {
        return false;
    }
    if uses_minor_units(child) && parent_payment.max_per_call_minor.is_none() {
        return false;
    }

    // Spend-class authority must be bounded in aggregate, not only per call: a
    // per-call cap with no per-run/per-period cap permits unbounded total spend
    // (per-call x unlimited calls). Require at least one aggregate cap on both
    // the requested child and the granting parent.
    if payment_authority_spends(child)
        && (!has_aggregate_minor_cap(child_payment) || !has_aggregate_minor_cap(parent_payment))
    {
        return false;
    }

    optional_bound_subset(
        child_payment.max_per_call_minor,
        parent_payment.max_per_call_minor,
    ) && optional_bound_subset(
        child_payment.max_per_run_minor,
        parent_payment.max_per_run_minor,
    ) && optional_bound_subset(
        child_payment.max_per_period_minor,
        parent_payment.max_per_period_minor,
    )
}

fn has_aggregate_minor_cap(payment: &PaymentAuthorityBounds) -> bool {
    payment.max_per_run_minor.is_some() || payment.max_per_period_minor.is_some()
}

fn uses_minor_units(term: &AuthorityTerm) -> bool {
    term.verbs.iter().any(|verb| {
        matches!(
            verb,
            AuthorityVerb::Quote
                | AuthorityVerb::Reserve
                | AuthorityVerb::Spend
                | AuthorityVerb::Refund
        )
    })
}

fn rails_subset(child: &PaymentAuthorityBounds, parent: &PaymentAuthorityBounds) -> bool {
    !child.rails.is_empty()
        && !parent.rails.is_empty()
        && child.rails.iter().all(|rail| parent.rails.contains(rail))
}

fn required_booleans_subset(
    child: &PaymentAuthorityBounds,
    parent: &PaymentAuthorityBounds,
) -> bool {
    (!parent.quote_required || child.quote_required)
        && (!parent.reservation_required || child.reservation_required)
        && (!parent.idempotency_required || child.idempotency_required)
        && (!parent.recovery_required || child.recovery_required)
        && (!parent.receipt_before_success || child.receipt_before_success)
}

fn credential_form_exact_match(
    child: &PaymentAuthorityBounds,
    parent: &PaymentAuthorityBounds,
) -> bool {
    child.credential_form == parent.credential_form
}

fn spend_credential_form_granted(child: &AuthorityTerm, parent: &AuthorityTerm) -> bool {
    if !payment_authority_spends(child) {
        return true;
    }
    let child_payment = child.bounds.payment.as_ref();
    let parent_payment = parent.bounds.payment.as_ref();
    let (Some(child_payment), Some(parent_payment)) = (child_payment, parent_payment) else {
        return false;
    };

    match child_payment.credential_form.as_ref() {
        Some(PaymentCredentialForm::SingleUseSpendCapability) => {
            child_payment.single_use_spend
                && parent_payment.single_use_spend
                && child
                    .capabilities
                    .contains(&AuthorityCapability::PaymentSingleUseSpend)
                && parent
                    .capabilities
                    .contains(&AuthorityCapability::PaymentSingleUseSpend)
        }
        Some(PaymentCredentialForm::ExternalSigner) => true,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::is_payment_authority_subset;
    use runx_contracts::{
        AuthorityBounds, AuthorityCapability, AuthorityResourceFamily, AuthorityTerm,
        AuthorityVerb, PaymentAuthorityBounds, PaymentCredentialForm, Reference, ReferenceType,
    };

    const COUNTERPARTY: &str = "merchant:bridge";

    #[test]
    fn accepts_external_signer_when_parent_grants_same_form() {
        let parent = payment_term("parent", PaymentCredentialForm::ExternalSigner);
        let child = payment_term("child", PaymentCredentialForm::ExternalSigner);

        assert!(is_payment_authority_subset(&child, &parent));
    }

    #[test]
    fn denies_external_signer_child_when_parent_grants_single_use_form() {
        let parent = payment_term("parent", PaymentCredentialForm::SingleUseSpendCapability);
        let child = payment_term("child", PaymentCredentialForm::ExternalSigner);

        assert!(!is_payment_authority_subset(&child, &parent));
    }

    #[test]
    fn denies_single_use_child_when_parent_grants_external_signer_form() {
        let mut parent = payment_term("parent", PaymentCredentialForm::ExternalSigner);
        parent.capabilities = vec![AuthorityCapability::PaymentSingleUseSpend];
        if let Some(payment) = parent.bounds.payment.as_mut() {
            payment.single_use_spend = true;
        }
        let child = payment_term("child", PaymentCredentialForm::SingleUseSpendCapability);

        assert!(!is_payment_authority_subset(&child, &parent));
    }

    fn payment_term(term_id: &str, credential_form: PaymentCredentialForm) -> AuthorityTerm {
        let uses_single_use = matches!(
            &credential_form,
            PaymentCredentialForm::SingleUseSpendCapability
        );

        AuthorityTerm {
            term_id: term_id.into(),
            principal_ref: reference(ReferenceType::Principal, "runx:principal:bridge-agent"),
            resource_ref: reference(ReferenceType::Grant, "runx:payment-grant:bridge"),
            resource_family: AuthorityResourceFamily::Payment,
            verbs: vec![AuthorityVerb::Reserve, AuthorityVerb::Spend],
            bounds: AuthorityBounds {
                payment: Some(PaymentAuthorityBounds {
                    currency: "USD".into(),
                    max_per_call_minor: Some(1_000),
                    max_per_run_minor: Some(5_000),
                    max_per_period_minor: None,
                    period: None,
                    rails: vec!["stripe".into()],
                    realm: None,
                    counterparty: Some(COUNTERPARTY.into()),
                    operation: Some("bridge.spend".into()),
                    quote_ttl_ms: Some(120_000),
                    approval_threshold_minor: None,
                    credential_form: Some(credential_form),
                    quote_required: true,
                    reservation_required: true,
                    idempotency_required: true,
                    recovery_required: true,
                    receipt_before_success: true,
                    single_use_spend: uses_single_use,
                }),
                ..AuthorityBounds::default()
            },
            conditions: Vec::new(),
            approvals: Vec::new(),
            capabilities: if uses_single_use {
                vec![AuthorityCapability::PaymentSingleUseSpend]
            } else {
                Vec::new()
            },
            expires_at: Some("2026-05-22T00:00:00Z".into()),
            issued_by_ref: reference(ReferenceType::Grant, "runx:grant:bridge-issuer"),
            credential_ref: Some(reference(
                ReferenceType::Credential,
                "runx:credential:bridge-session",
            )),
        }
    }

    fn reference(reference_type: ReferenceType, uri: &str) -> Reference {
        Reference {
            reference_type,
            uri: uri.to_owned().into(),
            provider: None,
            locator: None,
            label: None,
            observed_at: None,
            proof_kind: None,
        }
    }
}
