use runx_contracts::{AuthorityTerm, AuthorityVerb, Reference};

#[must_use]
pub fn same_reference_address(child: &Reference, parent: &Reference) -> bool {
    child.reference_type == parent.reference_type && child.uri == parent.uri
}

#[must_use]
pub fn authority_term_has_verb(term: &AuthorityTerm, verb: AuthorityVerb) -> bool {
    term.verbs.iter().any(|candidate| candidate == &verb)
}

#[must_use]
pub fn items_subset<T: PartialEq>(child: &[T], parent: &[T]) -> bool {
    child.iter().all(|item| parent.contains(item))
}

#[must_use]
pub fn parent_items_preserved<T: PartialEq>(child: &[T], parent: &[T]) -> bool {
    parent.iter().all(|item| child.contains(item))
}

#[must_use]
pub fn optional_exact_or_narrower<T: Eq>(child: &Option<T>, parent: &Option<T>) -> bool {
    match (child, parent) {
        (_, None) => true,
        (Some(child), Some(parent)) => child == parent,
        (None, Some(_)) => false,
    }
}

#[must_use]
pub fn optional_bound_subset<T: Ord + Copy>(child: Option<T>, parent: Option<T>) -> bool {
    match (child, parent) {
        (Some(child), Some(parent)) => child <= parent,
        (None, Some(_)) => false,
        (Some(_), None) | (None, None) => true,
    }
}

#[must_use]
pub fn optional_ref_bound_subset<T: Ord>(child: Option<&T>, parent: Option<&T>) -> bool {
    match (child, parent) {
        (Some(child), Some(parent)) => child <= parent,
        (None, Some(_)) => false,
        (Some(_), None) | (None, None) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        authority_term_has_verb, items_subset, optional_bound_subset, parent_items_preserved,
    };
    use runx_contracts::{AuthorityTerm, AuthorityVerb, Reference, ReferenceType};

    #[test]
    fn item_subset_is_reflexive() {
        let values = ["read", "write", "verify"];

        assert!(items_subset(&values, &values));
    }

    #[test]
    fn authority_term_verb_lookup_is_exact() {
        let term = AuthorityTerm {
            term_id: "deployment".to_owned().into(),
            principal_ref: Reference::with_uri(ReferenceType::Principal, "runx:principal:agent"),
            resource_ref: Reference::with_uri(ReferenceType::Grant, "runx:grant:deploy"),
            resource_family: runx_contracts::AuthorityResourceFamily::Deployment,
            verbs: vec![AuthorityVerb::Read, AuthorityVerb::Verify],
            bounds: Default::default(),
            conditions: Vec::new(),
            approvals: Vec::new(),
            capabilities: Vec::new(),
            expires_at: None,
            issued_by_ref: Reference::with_uri(ReferenceType::Principal, "runx:principal:issuer"),
            credential_ref: None,
        };

        assert!(authority_term_has_verb(&term, AuthorityVerb::Verify));
        assert!(!authority_term_has_verb(&term, AuthorityVerb::Spend));
    }

    #[test]
    fn parent_items_are_preserved_when_child_keeps_parent_requirements() {
        let parent = ["approval", "mfa"];
        let child = ["approval", "mfa", "reason"];

        assert!(parent_items_preserved(&child, &parent));
        assert!(!parent_items_preserved(&["approval"], &parent));
    }

    #[test]
    fn optional_bounds_deny_missing_or_larger_child_bounds() {
        assert!(optional_bound_subset(Some(5_u64), Some(10_u64)));
        assert!(!optional_bound_subset(Some(11_u64), Some(10_u64)));
        assert!(!optional_bound_subset::<u64>(None, Some(10_u64)));
        assert!(optional_bound_subset(Some(10_u64), None));
    }
}
