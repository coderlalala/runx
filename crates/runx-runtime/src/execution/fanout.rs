use std::collections::BTreeMap;

use runx_core::state_machine::{
    FanoutBranchFailurePolicy as CoreFanoutBranchFailurePolicy,
    FanoutConflictGate as CoreFanoutConflictGate, FanoutGateAction, FanoutGroupPolicy,
    FanoutSyncStrategy as CoreFanoutSyncStrategy, FanoutThresholdGate as CoreFanoutThresholdGate,
};
use runx_parser::{
    ExecutionGraph, FanoutBranchFailurePolicy, FanoutConflictAction, FanoutSyncStrategy,
    FanoutThresholdAction,
};

pub(crate) fn fanout_policies(graph: &ExecutionGraph) -> BTreeMap<String, FanoutGroupPolicy> {
    graph
        .fanout_groups
        .iter()
        .map(|(group_id, policy)| (group_id.clone(), fanout_policy(policy)))
        .collect()
}

fn fanout_policy(policy: &runx_parser::FanoutGroupPolicy) -> FanoutGroupPolicy {
    FanoutGroupPolicy {
        group_id: policy.group_id.clone(),
        strategy: fanout_strategy(&policy.strategy),
        min_success: policy.min_success.map(u64_to_u32),
        on_branch_failure: fanout_branch_failure(&policy.on_branch_failure),
        threshold_gates: optional_vec(policy.threshold_gates.iter().map(threshold_gate)),
        conflict_gates: optional_vec(policy.conflict_gates.iter().map(conflict_gate)),
    }
}

fn optional_vec<T>(items: impl Iterator<Item = T>) -> Option<Vec<T>> {
    let values = items.collect::<Vec<_>>();
    (!values.is_empty()).then_some(values)
}

fn threshold_gate(gate: &runx_parser::FanoutThresholdGate) -> CoreFanoutThresholdGate {
    CoreFanoutThresholdGate {
        step: gate.step.clone(),
        field: gate.field.clone(),
        above: runx_contracts::JsonNumber::F64(gate.above),
        action: threshold_action(&gate.action),
    }
}

fn conflict_gate(gate: &runx_parser::FanoutConflictGate) -> CoreFanoutConflictGate {
    CoreFanoutConflictGate {
        field: gate.field.clone(),
        steps: gate.steps.clone(),
        action: conflict_action(&gate.action),
    }
}

fn fanout_strategy(strategy: &FanoutSyncStrategy) -> CoreFanoutSyncStrategy {
    match strategy {
        FanoutSyncStrategy::All => CoreFanoutSyncStrategy::All,
        FanoutSyncStrategy::Any => CoreFanoutSyncStrategy::Any,
        FanoutSyncStrategy::Quorum => CoreFanoutSyncStrategy::Quorum,
    }
}

fn fanout_branch_failure(policy: &FanoutBranchFailurePolicy) -> CoreFanoutBranchFailurePolicy {
    match policy {
        FanoutBranchFailurePolicy::Halt => CoreFanoutBranchFailurePolicy::Halt,
        FanoutBranchFailurePolicy::Continue => CoreFanoutBranchFailurePolicy::Continue,
    }
}

fn threshold_action(action: &FanoutThresholdAction) -> FanoutGateAction {
    match action {
        FanoutThresholdAction::Pause => FanoutGateAction::Pause,
        FanoutThresholdAction::Escalate => FanoutGateAction::Escalate,
    }
}

fn conflict_action(action: &FanoutConflictAction) -> FanoutGateAction {
    match action {
        FanoutConflictAction::Pause => FanoutGateAction::Pause,
        FanoutConflictAction::Escalate => FanoutGateAction::Escalate,
    }
}

fn u64_to_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
