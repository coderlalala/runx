//! Subset comparator for payment authority terms.
//!
//! Fail-closed by construction: missing required payment dimensions make terms
//! incomparable, and incomparable terms are denied.

use runx_contracts::{
    AuthorityCapability, AuthorityResourceFamily, AuthorityTerm, AuthorityVerb,
    PaymentAuthorityBounds, PaymentCredentialForm,
};

use super::super::authority_algebra::{
    items_subset, optional_bound_subset, optional_exact_or_narrower, optional_ref_bound_subset,
    parent_items_preserved, same_reference_address,
};
use super::payment_authority_spends;

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
        && optional_exact_or_narrower(
            &child_payment.credential_form,
            &parent_payment.credential_form,
        )
        && single_use_spend_capability_for_reserve_or_spend(child, parent)
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

fn single_use_spend_capability_for_reserve_or_spend(
    child: &AuthorityTerm,
    parent: &AuthorityTerm,
) -> bool {
    if !requires_single_use_capability(child) {
        return true;
    }
    let child_payment = child.bounds.payment.as_ref();
    let parent_payment = parent.bounds.payment.as_ref();
    child_payment.is_some_and(|payment| payment.single_use_spend)
        && parent_payment.is_some_and(|payment| payment.single_use_spend)
        && child
            .capabilities
            .contains(&AuthorityCapability::PaymentSingleUseSpend)
        && parent
            .capabilities
            .contains(&AuthorityCapability::PaymentSingleUseSpend)
        && child
            .bounds
            .payment
            .as_ref()
            .and_then(|payment| payment.credential_form.as_ref())
            == Some(&PaymentCredentialForm::SingleUseSpendCapability)
}

fn requires_single_use_capability(term: &AuthorityTerm) -> bool {
    term.verbs
        .iter()
        .any(|verb| matches!(verb, AuthorityVerb::Spend))
}
