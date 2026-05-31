use proptest::prelude::*;
use proptest::test_runner::TestCaseError;
use runx_contracts::{JsonObject, JsonValue};
use runx_core::policy::authority_algebra::{items_subset, optional_bound_subset};
use runx_core::policy::{
    GraphScopeAdmissionDecision, GraphScopeAdmissionRequest, GraphScopeGrant, LocalAdmissionGrant,
    LocalAdmissionGrantStatus, LocalAdmissionOptions, LocalAdmissionSkill, LocalAdmissionSource,
    LocalExecutionPolicy, RetryAdmissionRequest, RetryPolicy, admit_graph_step_scopes,
    admit_local_skill,
};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn local_admission_is_deterministic(
        skill in local_admission_skill(),
        options in local_admission_options(),
    ) {
        let left = admit_local_skill(&skill, &options);
        let right = admit_local_skill(&skill, &options);
        let left_json = serde_json::to_string(&left).map_err(test_case_error)?;
        let right_json = serde_json::to_string(&right).map_err(test_case_error)?;

        prop_assert_eq!(left_json, right_json);
    }

    // The chosen connected-auth grant is intentionally not exposed through
    // AdmissionDecision. The first-match ordering property is asserted where
    // the selector is visible: policy::credential_grant::tests.

    #[test]
    fn graph_scope_admission_is_deterministic(
        request in graph_scope_request(),
    ) {
        let left = admit_graph_step_scopes(&request);
        let right = admit_graph_step_scopes(&request);
        let left_json = serde_json::to_string(&left).map_err(test_case_error)?;
        let right_json = serde_json::to_string(&right).map_err(test_case_error)?;

        prop_assert_eq!(left_json, right_json);
    }

    #[test]
    fn graph_scope_deduplication_is_idempotent(
        request in graph_scope_request(),
    ) {
        let first = admit_graph_step_scopes(&request);
        let normalized = request_from_decision(&first);
        let second = admit_graph_step_scopes(&normalized);

        prop_assert_eq!(first, second);
    }

    #[test]
    fn retry_admission_is_deterministic(
        request in retry_request(),
    ) {
        let left = runx_core::policy::admit_retry_policy(&request);
        let right = runx_core::policy::admit_retry_policy(&request);

        prop_assert_eq!(left, right);
    }

    #[test]
    fn authority_item_subset_is_reflexive(
        values in prop::collection::vec(any::<u8>(), 0..24),
    ) {
        prop_assert!(items_subset(&values, &values));
    }

    #[test]
    fn authority_item_subset_is_transitive(
        parent in prop::collection::vec(any::<u8>(), 0..24),
        middle in prop::collection::vec(any::<u8>(), 0..24),
        child in prop::collection::vec(any::<u8>(), 0..24),
    ) {
        let middle_subset_parent = items_subset(&middle, &parent);
        let child_subset_middle = items_subset(&child, &middle);

        prop_assert!(
            !middle_subset_parent || !child_subset_middle || items_subset(&child, &parent)
        );
    }

    #[test]
    fn authority_item_subset_denies_widening(
        parent in prop::collection::vec(any::<u8>(), 0..24),
        missing in any::<u8>(),
    ) {
        prop_assume!(!parent.contains(&missing));

        let mut child = parent.clone();
        child.push(missing);

        prop_assert!(!items_subset(&child, &parent));
    }

    #[test]
    fn authority_optional_bounds_are_reflexive(
        value in any::<u64>(),
    ) {
        prop_assert!(optional_bound_subset(Some(value), Some(value)));
        prop_assert!(optional_bound_subset::<u64>(None, None));
    }

    #[test]
    fn authority_optional_bounds_allow_stricter_child_bounds(
        parent in any::<u64>(),
    ) {
        let child = parent / 2;

        prop_assert!(optional_bound_subset(Some(child), Some(parent)));
    }

    #[test]
    fn authority_optional_bounds_deny_missing_child_bound(
        parent in any::<u64>(),
    ) {
        prop_assert!(!optional_bound_subset::<u64>(None, Some(parent)));
    }

    #[test]
    fn authority_optional_bounds_allow_parent_unbounded(
        child in any::<u64>(),
    ) {
        prop_assert!(optional_bound_subset(Some(child), None));
    }

    #[test]
    fn authority_optional_bounds_deny_widening(
        (child, parent) in (1_u64..100_000).prop_flat_map(|child| (Just(child), 0_u64..child)),
    ) {
        prop_assert!(!optional_bound_subset(Some(child), Some(parent)));
    }
}

fn request_from_decision(decision: &GraphScopeAdmissionDecision) -> GraphScopeAdmissionRequest {
    match decision {
        GraphScopeAdmissionDecision::Allow {
            step_id,
            requested_scopes,
            granted_scopes,
            grant_id,
            ..
        }
        | GraphScopeAdmissionDecision::Deny {
            step_id,
            requested_scopes,
            granted_scopes,
            grant_id,
            ..
        } => GraphScopeAdmissionRequest {
            step_id: step_id.clone(),
            requested_scopes: requested_scopes.clone(),
            grant: GraphScopeGrant {
                grant_id: grant_id.clone(),
                scopes: granted_scopes.clone(),
            },
        },
    }
}

fn local_admission_skill() -> impl Strategy<Value = LocalAdmissionSkill> {
    (
        safe_id(),
        source_type(),
        prop::option::of(command()),
        prop::collection::vec(arg(), 0..4),
        prop::option::of(1_i64..600),
        prop::option::of(auth_requirement()),
    )
        .prop_map(
            |(name, source_type, command, args, timeout_seconds, auth)| LocalAdmissionSkill {
                name,
                source: LocalAdmissionSource {
                    source_type,
                    command,
                    args: Some(args),
                    timeout_seconds,
                    sandbox: None,
                },
                auth,
                runtime: None,
            },
        )
}

fn local_admission_options() -> impl Strategy<Value = LocalAdmissionOptions> {
    (
        prop::option::of(prop::collection::vec(source_type(), 0..5)),
        prop::option::of(1_i64..600),
        prop::collection::vec(local_admission_grant(), 0..4),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(
            |(
                allowed_source_types,
                max_timeout_seconds,
                connected_grants,
                skip_connected_auth,
                strict_cli_tool_inline_code,
            )| LocalAdmissionOptions {
                allowed_source_types,
                max_timeout_seconds,
                connected_grants: Some(connected_grants),
                connected_auth_checked_at: Some("2026-05-22T00:00:00Z".to_owned()),
                skip_connected_auth: Some(skip_connected_auth),
                approved_sandbox_escalation: None,
                skip_sandbox_escalation: None,
                execution_policy: Some(LocalExecutionPolicy {
                    strict_cli_tool_inline_code: Some(strict_cli_tool_inline_code),
                }),
            },
        )
}

fn graph_scope_request() -> impl Strategy<Value = GraphScopeAdmissionRequest> {
    (
        safe_id(),
        prop::collection::vec(scope(), 0..6),
        prop::collection::vec(scope(), 0..6),
        prop::option::of(safe_id()),
    )
        .prop_map(|(step_id, requested_scopes, granted_scopes, grant_id)| {
            GraphScopeAdmissionRequest {
                step_id,
                requested_scopes,
                grant: GraphScopeGrant {
                    grant_id,
                    scopes: granted_scopes,
                },
            }
        })
}

fn retry_request() -> impl Strategy<Value = RetryAdmissionRequest> {
    (
        safe_id(),
        prop::option::of(0_i64..5),
        any::<bool>(),
        prop::option::of(idempotency_key()),
    )
        .prop_map(
            |(step_id, max_attempts, mutating, idempotency_key)| RetryAdmissionRequest {
                step_id,
                retry: max_attempts.map(|max_attempts| RetryPolicy { max_attempts }),
                mutating: Some(mutating),
                idempotency_key,
            },
        )
}

fn local_admission_grant() -> impl Strategy<Value = LocalAdmissionGrant> {
    (
        safe_id(),
        prop::collection::vec(scope(), 0..4),
        prop::option::of(prop::sample::select(&[
            LocalAdmissionGrantStatus::Active,
            LocalAdmissionGrantStatus::Revoked,
        ])),
    )
        .prop_map(|(grant_id, scopes, status)| LocalAdmissionGrant {
            grant_id,
            provider: "github".to_owned(),
            scopes,
            status,
            not_before: Some("2026-05-21T00:00:00Z".to_owned()),
            expires_at: Some("2026-05-23T00:00:00Z".to_owned()),
            scope_family: None,
            authority_kind: None,
            target_repo: None,
            target_locator: None,
        })
}

fn auth_requirement() -> impl Strategy<Value = JsonValue> {
    prop::collection::vec(scope(), 0..4).prop_map(|scopes| {
        let scope_values = scopes.into_iter().map(JsonValue::String).collect();
        JsonValue::Object(JsonObject::from([
            (
                "provider".to_owned(),
                JsonValue::String("github".to_owned()),
            ),
            ("type".to_owned(), JsonValue::String("connected".to_owned())),
            ("scopes".to_owned(), JsonValue::Array(scope_values)),
        ]))
    })
}

fn source_type() -> impl Strategy<Value = String> {
    prop::sample::select(&[
        "agent",
        "agent-task",
        "approval",
        "cli-tool",
        "mcp",
        "a2a",
        "catalog",
        "graph",
        "unsupported",
    ])
    .prop_map(str::to_owned)
}

fn command() -> impl Strategy<Value = String> {
    prop::sample::select(&["node", "python3", "/usr/bin/env", "bash", "runx-tool"])
        .prop_map(str::to_owned)
}

fn arg() -> impl Strategy<Value = String> {
    prop::sample::select(&[
        "-e",
        "-c",
        "--eval",
        "print('hi')",
        "PYTHONPATH=.",
        "script.js",
    ])
    .prop_map(str::to_owned)
}

fn scope() -> impl Strategy<Value = String> {
    prop::sample::select(&[
        "*",
        "repo:read",
        "repo:write",
        "repo:*",
        "repository:read",
        "repos:list",
        "checks:read",
        "checks:*",
        "checks2:read",
        "deploy:prod",
    ])
    .prop_map(str::to_owned)
}

fn safe_id() -> impl Strategy<Value = String> {
    prop::sample::select(&["read", "write", "deploy", "checks", "graph", "step"])
        .prop_map(str::to_owned)
}

fn idempotency_key() -> impl Strategy<Value = String> {
    prop::sample::select(&["", "retry-key", "deploy-1", "same-request"]).prop_map(str::to_owned)
}

fn test_case_error(error: serde_json::Error) -> TestCaseError {
    TestCaseError::fail(error.to_string())
}
