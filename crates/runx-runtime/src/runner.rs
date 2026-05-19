// rust-style-allow: large-file because the first runtime skeleton keeps
// orchestration, checkpoints, and graph finalization together until fanout
// parity splits execution modes into smaller modules.
use std::collections::BTreeMap;
use std::path::Path;

use runx_contracts::{
    ApprovalGate, ExecutionEvent, FanoutReceiptDecision, FanoutReceiptStrategy,
    FanoutReceiptSyncPoint, JsonObject, JsonValue, ResolutionResponseActor,
};
use runx_core::state_machine::{
    FanoutBranchResult, FanoutGroupPolicy, FanoutSyncDecision, FanoutSyncOutcome,
    FanoutSyncStrategy, GraphStepStatus, SequentialGraphEvent, SequentialGraphPlan,
    SequentialGraphState, SequentialGraphStepDefinition, create_sequential_graph_state,
    evaluate_fanout_sync, plan_sequential_graph_transition, transition_sequential_graph,
};
use runx_parser::{ExecutionGraph, GraphStep};

use crate::RuntimeError;
use crate::adapter::{InvocationStatus, SkillAdapter, SkillInvocation, SkillOutput};
use crate::approval::{ApprovalResolution, request_approval};
use crate::caller::{Caller, NoopCaller};
use crate::fanout::fanout_policies;
use crate::graph::{find_step, load_graph, load_skill, output_object, resolve_inputs, skill_dir};
use crate::journal::ExecutionJournal;
use crate::receipts::{graph_receipt, step_receipt};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeOptions {
    pub created_at: String,
    pub env: BTreeMap<String, String>,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            created_at: "2026-05-18T00:00:00Z".to_owned(),
            env: safe_default_env(),
        }
    }
}

fn safe_default_env() -> BTreeMap<String, String> {
    let allowed = ["PATH", "SystemRoot", "PATHEXT"];
    allowed
        .into_iter()
        .filter_map(|key| std::env::var(key).ok().map(|value| (key.to_owned(), value)))
        .collect()
}

#[derive(Clone, Debug)]
pub struct StepRun {
    pub step_id: String,
    pub attempt: u32,
    pub skill: String,
    pub runner: Option<String>,
    pub fanout_group: Option<String>,
    pub output: SkillOutput,
    pub outputs: JsonObject,
    pub receipt: runx_contracts::HarnessReceipt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StepFailureMode {
    Propagate,
    RecordAndContinue,
}

#[derive(Clone, Debug)]
pub struct GraphRun {
    pub graph: ExecutionGraph,
    pub state: SequentialGraphState,
    pub steps: Vec<StepRun>,
    pub sync_points: Vec<FanoutReceiptSyncPoint>,
    pub receipt: runx_contracts::HarnessReceipt,
    pub journal: ExecutionJournal,
}

#[derive(Clone, Debug)]
pub struct GraphCheckpoint {
    pub graph_name: String,
    pub state: SequentialGraphState,
    pub steps: Vec<StepRun>,
    pub sync_points: Vec<FanoutReceiptSyncPoint>,
    pub journal: ExecutionJournal,
}

pub struct Runtime<A> {
    adapter: A,
    options: RuntimeOptions,
}

impl<A> Runtime<A>
where
    A: SkillAdapter,
{
    pub fn new(adapter: A, options: RuntimeOptions) -> Self {
        Self { adapter, options }
    }

    pub fn run_graph_file(&self, graph_path: &Path) -> Result<GraphRun, RuntimeError> {
        let mut caller = NoopCaller;
        self.run_graph_file_with_caller(graph_path, &mut caller)
    }

    pub fn run_graph_file_with_caller(
        &self,
        graph_path: &Path,
        caller: &mut dyn Caller,
    ) -> Result<GraphRun, RuntimeError> {
        let graph = load_graph(graph_path)?;
        let graph_dir = graph_path.parent().unwrap_or_else(|| Path::new("."));
        let mut execution = GraphExecution::new(&graph);
        execution.run(self, graph_dir, &graph, caller, None)?;
        let receipt = graph_receipt(
            &graph.name,
            &execution.runs,
            execution.sync_points.clone(),
            &self.options.created_at,
        )?;
        execution.record(
            caller,
            ExecutionEvent::Completed {
                message: format!("graph {} completed", graph.name),
                data: None,
            },
        )?;
        Ok(execution.finish(graph, receipt))
    }

    pub fn run_graph_file_until_steps(
        &self,
        graph_path: &Path,
        max_steps: usize,
    ) -> Result<GraphCheckpoint, RuntimeError> {
        let mut caller = NoopCaller;
        self.run_graph_file_until_steps_with_caller(graph_path, max_steps, &mut caller)
    }

    pub fn run_graph_file_until_steps_with_caller(
        &self,
        graph_path: &Path,
        max_steps: usize,
        caller: &mut dyn Caller,
    ) -> Result<GraphCheckpoint, RuntimeError> {
        let graph = load_graph(graph_path)?;
        let graph_dir = graph_path.parent().unwrap_or_else(|| Path::new("."));
        let mut execution = GraphExecution::new(&graph);
        execution.run(self, graph_dir, &graph, caller, Some(max_steps))?;
        Ok(execution.checkpoint(graph.name))
    }

    pub fn resume_graph_file(
        &self,
        graph_path: &Path,
        checkpoint: GraphCheckpoint,
    ) -> Result<GraphRun, RuntimeError> {
        let mut caller = NoopCaller;
        self.resume_graph_file_with_caller(graph_path, checkpoint, &mut caller)
    }

    pub fn resume_graph_file_with_caller(
        &self,
        graph_path: &Path,
        checkpoint: GraphCheckpoint,
        caller: &mut dyn Caller,
    ) -> Result<GraphRun, RuntimeError> {
        let graph = load_graph(graph_path)?;
        let graph_dir = graph_path.parent().unwrap_or_else(|| Path::new("."));
        let mut execution = GraphExecution::from_checkpoint(&graph, checkpoint)?;
        execution.run(self, graph_dir, &graph, caller, None)?;
        let receipt = graph_receipt(
            &graph.name,
            &execution.runs,
            execution.sync_points.clone(),
            &self.options.created_at,
        )?;
        execution.record(
            caller,
            ExecutionEvent::Completed {
                message: format!("graph {} completed", graph.name),
                data: None,
            },
        )?;
        Ok(execution.finish(graph, receipt))
    }
}

#[cfg(feature = "cli-tool")]
pub fn run_graph_file(graph_path: impl AsRef<Path>) -> Result<GraphRun, RuntimeError> {
    let runtime = Runtime::new(
        crate::adapters::cli_tool::CliToolAdapter,
        RuntimeOptions::default(),
    );
    runtime.run_graph_file(graph_path.as_ref())
}

struct GraphExecution {
    definitions: Vec<SequentialGraphStepDefinition>,
    state: SequentialGraphState,
    runs: Vec<StepRun>,
    sync_points: Vec<FanoutReceiptSyncPoint>,
    journal: ExecutionJournal,
}

struct FanoutRunPlan {
    group_id: String,
    step_ids: Vec<String>,
    attempts: BTreeMap<String, u32>,
}

struct StepExecutionPlan<'a> {
    step_id: &'a str,
    attempt: u32,
    failure_mode: StepFailureMode,
}

impl GraphExecution {
    fn new(graph: &ExecutionGraph) -> Self {
        let definitions = crate::graph::step_definitions(graph);
        Self {
            state: create_sequential_graph_state(graph.name.clone(), &definitions),
            definitions,
            runs: Vec::new(),
            sync_points: Vec::new(),
            journal: ExecutionJournal::default(),
        }
    }

    fn from_checkpoint(
        graph: &ExecutionGraph,
        checkpoint: GraphCheckpoint,
    ) -> Result<Self, RuntimeError> {
        if checkpoint.graph_name != graph.name {
            return Err(RuntimeError::CheckpointGraphMismatch {
                checkpoint_graph: checkpoint.graph_name,
                graph: graph.name.clone(),
            });
        }
        Ok(Self {
            definitions: crate::graph::step_definitions(graph),
            state: checkpoint.state,
            runs: checkpoint.steps,
            sync_points: checkpoint.sync_points,
            journal: checkpoint.journal,
        })
    }

    fn run<A>(
        &mut self,
        runtime: &Runtime<A>,
        graph_dir: &Path,
        graph: &ExecutionGraph,
        caller: &mut dyn Caller,
        max_new_steps: Option<usize>,
    ) -> Result<(), RuntimeError>
    where
        A: SkillAdapter,
    {
        let fanout_policies = fanout_policies(graph);
        let initial_step_count = self.runs.len();
        loop {
            if reached_step_limit(initial_step_count, self.runs.len(), max_new_steps) {
                return Ok(());
            }
            let plan = plan_sequential_graph_transition(
                &self.state,
                &self.definitions,
                &fanout_policies,
                None,
            );
            if self.apply_plan(runtime, graph_dir, graph, caller, &fanout_policies, plan)? {
                break;
            }
        }
        Ok(())
    }

    fn apply_plan<A>(
        &mut self,
        runtime: &Runtime<A>,
        graph_dir: &Path,
        graph: &ExecutionGraph,
        caller: &mut dyn Caller,
        fanout_policies: &BTreeMap<String, FanoutGroupPolicy>,
        plan: SequentialGraphPlan,
    ) -> Result<bool, RuntimeError>
    where
        A: SkillAdapter,
    {
        match plan {
            SequentialGraphPlan::RunStep {
                step_id, attempt, ..
            } => self.apply_step_plan(runtime, graph_dir, graph, caller, &step_id, attempt),
            SequentialGraphPlan::RunFanout {
                group_id,
                step_ids,
                attempts,
                ..
            } => {
                self.run_fanout_plan(
                    runtime,
                    graph_dir,
                    graph,
                    caller,
                    fanout_policies,
                    FanoutRunPlan {
                        group_id,
                        step_ids,
                        attempts,
                    },
                )?;
                Ok(false)
            }
            SequentialGraphPlan::Complete => Ok(self.complete_graph()),
            SequentialGraphPlan::Blocked {
                step_id,
                reason,
                sync_decision,
            } => self.block_graph(graph, step_id, reason, sync_decision),
            SequentialGraphPlan::Failed {
                step_id,
                reason,
                sync_decision,
            } => self.fail_graph(graph, step_id, reason, sync_decision),
            SequentialGraphPlan::Paused {
                step_id,
                reason,
                sync_decision,
            } => self.pause_for_sync(graph, step_id, reason, sync_decision),
            SequentialGraphPlan::Escalated {
                step_id,
                reason,
                sync_decision,
            } => self.escalate_for_sync(graph, step_id, reason, sync_decision),
        }
    }

    fn apply_step_plan<A>(
        &mut self,
        runtime: &Runtime<A>,
        graph_dir: &Path,
        graph: &ExecutionGraph,
        caller: &mut dyn Caller,
        step_id: &str,
        attempt: u32,
    ) -> Result<bool, RuntimeError>
    where
        A: SkillAdapter,
    {
        self.run_one_step(runtime, graph_dir, graph, step_id, attempt, caller)?;
        Ok(false)
    }

    fn complete_graph(&mut self) -> bool {
        self.state = transition_sequential_graph(&self.state, &SequentialGraphEvent::Complete);
        true
    }

    fn run_fanout_plan<A>(
        &mut self,
        runtime: &Runtime<A>,
        graph_dir: &Path,
        graph: &ExecutionGraph,
        caller: &mut dyn Caller,
        fanout_policies: &BTreeMap<String, FanoutGroupPolicy>,
        plan: FanoutRunPlan,
    ) -> Result<(), RuntimeError>
    where
        A: SkillAdapter,
    {
        for step_id in plan.step_ids {
            let attempt = plan.attempts.get(&step_id).copied().unwrap_or(1);
            self.run_one_step_with_mode(
                runtime,
                graph_dir,
                graph,
                caller,
                StepExecutionPlan {
                    step_id: &step_id,
                    attempt,
                    failure_mode: StepFailureMode::RecordAndContinue,
                },
            )?;
        }
        self.record_proceeding_fanout_sync_point(graph, fanout_policies, &plan.group_id)
    }

    fn block_graph(
        &mut self,
        graph: &ExecutionGraph,
        step_id: String,
        reason: String,
        sync_decision: Option<FanoutSyncDecision>,
    ) -> Result<bool, RuntimeError> {
        if let Some(sync_decision) = sync_decision {
            self.push_sync_point(graph, &sync_decision)?;
        }
        Err(RuntimeError::GraphBlocked { step_id, reason })
    }

    fn fail_graph(
        &mut self,
        graph: &ExecutionGraph,
        step_id: String,
        reason: String,
        sync_decision: Option<FanoutSyncDecision>,
    ) -> Result<bool, RuntimeError> {
        if let Some(sync_decision) = sync_decision {
            self.push_sync_point(graph, &sync_decision)?;
        }
        self.state = transition_sequential_graph(
            &self.state,
            &SequentialGraphEvent::FailGraph {
                error: reason.clone(),
            },
        );
        Err(RuntimeError::GraphPlanningFailed { step_id, reason })
    }

    fn pause_graph(
        &mut self,
        step_id: String,
        reason: String,
        sync_decision: runx_core::state_machine::FanoutSyncDecision,
    ) -> Result<bool, RuntimeError> {
        self.state = transition_sequential_graph(
            &self.state,
            &SequentialGraphEvent::PauseGraph {
                reason: reason.clone(),
            },
        );
        Err(RuntimeError::GraphPaused {
            step_id,
            reason,
            sync_decision: Box::new(sync_decision),
        })
    }

    fn pause_for_sync(
        &mut self,
        graph: &ExecutionGraph,
        step_id: String,
        reason: String,
        sync_decision: FanoutSyncDecision,
    ) -> Result<bool, RuntimeError> {
        self.push_sync_point(graph, &sync_decision)?;
        self.pause_graph(step_id, reason, sync_decision)
    }

    fn escalate_graph(
        &mut self,
        step_id: String,
        reason: String,
        sync_decision: runx_core::state_machine::FanoutSyncDecision,
    ) -> Result<bool, RuntimeError> {
        self.state = transition_sequential_graph(
            &self.state,
            &SequentialGraphEvent::EscalateGraph {
                reason: reason.clone(),
            },
        );
        Err(RuntimeError::GraphEscalated {
            step_id,
            reason,
            sync_decision: Box::new(sync_decision),
        })
    }

    fn escalate_for_sync(
        &mut self,
        graph: &ExecutionGraph,
        step_id: String,
        reason: String,
        sync_decision: FanoutSyncDecision,
    ) -> Result<bool, RuntimeError> {
        self.push_sync_point(graph, &sync_decision)?;
        self.escalate_graph(step_id, reason, sync_decision)
    }

    fn run_one_step<A>(
        &mut self,
        runtime: &Runtime<A>,
        graph_dir: &Path,
        graph: &ExecutionGraph,
        step_id: &str,
        attempt: u32,
        caller: &mut dyn Caller,
    ) -> Result<(), RuntimeError>
    where
        A: SkillAdapter,
    {
        self.run_one_step_with_mode(
            runtime,
            graph_dir,
            graph,
            caller,
            StepExecutionPlan {
                step_id,
                attempt,
                failure_mode: StepFailureMode::Propagate,
            },
        )
    }

    fn run_one_step_with_mode<A>(
        &mut self,
        runtime: &Runtime<A>,
        graph_dir: &Path,
        graph: &ExecutionGraph,
        caller: &mut dyn Caller,
        plan: StepExecutionPlan<'_>,
    ) -> Result<(), RuntimeError>
    where
        A: SkillAdapter,
    {
        let step = find_step(graph, plan.step_id)?;
        enforce_transition_gates(graph, step, &self.runs)?;
        self.record(caller, started_event(plan.step_id))?;
        self.start_step(runtime, plan.step_id);
        let run = match run_step(
            runtime,
            graph_dir,
            &graph.name,
            step,
            plan.attempt,
            &self.runs,
            caller,
        ) {
            Ok(run) => run,
            Err(error) if plan.failure_mode == StepFailureMode::RecordAndContinue => {
                runtime_error_step_run(runtime, &graph.name, step, plan.attempt, error)?
            }
            Err(error) => return Err(error),
        };
        if run.output.succeeded() {
            self.succeed_step(runtime, plan.step_id, &run);
            self.runs.push(run);
            self.record(caller, completed_event(plan.step_id))
        } else {
            self.fail_step(runtime, plan.step_id, &run);
            caller.log(format!("step {} failed", plan.step_id))?;
            self.record(caller, failed_event(plan.step_id))?;
            if plan.failure_mode == StepFailureMode::RecordAndContinue {
                self.runs.push(run);
                Ok(())
            } else {
                Err(RuntimeError::SkillFailed {
                    skill_name: plan.step_id.to_owned(),
                    message: run.output.stderr.clone(),
                })
            }
        }
    }

    fn start_step<A>(&mut self, runtime: &Runtime<A>, step_id: &str) {
        self.state = transition_sequential_graph(
            &self.state,
            &SequentialGraphEvent::StartStep {
                step_id: step_id.to_owned(),
                at: runtime.options.created_at.clone(),
            },
        );
    }

    fn succeed_step<A>(&mut self, runtime: &Runtime<A>, step_id: &str, run: &StepRun) {
        self.state = transition_sequential_graph(
            &self.state,
            &SequentialGraphEvent::StepSucceeded {
                step_id: step_id.to_owned(),
                at: runtime.options.created_at.clone(),
                receipt_id: run.receipt.id.clone(),
                outputs: Some(run.outputs.clone()),
            },
        );
    }

    fn fail_step<A>(&mut self, runtime: &Runtime<A>, step_id: &str, run: &StepRun) {
        self.state = transition_sequential_graph(
            &self.state,
            &SequentialGraphEvent::StepFailed {
                step_id: step_id.to_owned(),
                at: runtime.options.created_at.clone(),
                error: output_error(run),
            },
        );
    }

    fn record(
        &mut self,
        caller: &mut dyn Caller,
        event: ExecutionEvent,
    ) -> Result<(), RuntimeError> {
        self.journal.push(event.clone());
        caller.report(event)
    }

    fn finish(self, graph: ExecutionGraph, receipt: runx_contracts::HarnessReceipt) -> GraphRun {
        GraphRun {
            graph,
            state: self.state,
            steps: self.runs,
            sync_points: self.sync_points,
            receipt,
            journal: self.journal,
        }
    }

    fn checkpoint(self, graph_name: String) -> GraphCheckpoint {
        GraphCheckpoint {
            graph_name,
            state: self.state,
            steps: self.runs,
            sync_points: self.sync_points,
            journal: self.journal,
        }
    }

    fn record_proceeding_fanout_sync_point(
        &mut self,
        graph: &ExecutionGraph,
        fanout_policies: &BTreeMap<String, FanoutGroupPolicy>,
        group_id: &str,
    ) -> Result<(), RuntimeError> {
        let follow_up =
            plan_sequential_graph_transition(&self.state, &self.definitions, fanout_policies, None);
        if matches!(
            follow_up,
            SequentialGraphPlan::RunFanout {
                group_id: ref next_group_id,
                ..
            } if next_group_id == group_id
        ) {
            return Ok(());
        }

        let Some(policy) = fanout_policies.get(group_id) else {
            return Ok(());
        };
        let decision = evaluate_fanout_sync(policy, &self.branch_results(graph, group_id), None);
        if decision.decision == FanoutSyncOutcome::Proceed {
            self.push_sync_point(graph, &decision)?;
        }
        Ok(())
    }

    fn push_sync_point(
        &mut self,
        graph: &ExecutionGraph,
        decision: &FanoutSyncDecision,
    ) -> Result<(), RuntimeError> {
        let sync_point = fanout_sync_point(
            decision,
            &latest_fanout_receipt_ids(&self.runs, graph, &decision.group_id),
        );
        let already_recorded = self.sync_points.iter().any(|existing| {
            existing.group_id == sync_point.group_id
                && existing.rule_fired == sync_point.rule_fired
                && existing.decision == sync_point.decision
        });
        if !already_recorded {
            self.sync_points.push(sync_point);
        }
        Ok(())
    }

    fn branch_results(&self, graph: &ExecutionGraph, group_id: &str) -> Vec<FanoutBranchResult> {
        graph
            .steps
            .iter()
            .filter(|step| step.fanout_group.as_deref() == Some(group_id))
            .map(|step| {
                let state = self
                    .state
                    .steps
                    .iter()
                    .find(|candidate| candidate.step_id == step.id);
                FanoutBranchResult {
                    step_id: step.id.clone(),
                    status: state.map_or(GraphStepStatus::Failed, |state| state.status.clone()),
                    outputs: state.and_then(|state| state.outputs.clone()),
                }
            })
            .collect()
    }
}

fn reached_step_limit(initial: usize, current: usize, max_new_steps: Option<usize>) -> bool {
    max_new_steps.is_some_and(|max| current.saturating_sub(initial) >= max)
}

fn enforce_transition_gates(
    graph: &ExecutionGraph,
    step: &GraphStep,
    runs: &[StepRun],
) -> Result<(), RuntimeError> {
    let Some(policy) = &graph.policy else {
        return Ok(());
    };
    for gate in policy.transitions.iter().filter(|gate| gate.to == step.id) {
        let Some(value) = transition_field_value(&gate.field, runs) else {
            return Err(RuntimeError::GraphBlocked {
                step_id: step.id.clone(),
                reason: format!("transition gate '{}' is unresolved", gate.field),
            });
        };
        if let Some(expected) = &gate.equals
            && value != expected
        {
            return Err(RuntimeError::GraphBlocked {
                step_id: step.id.clone(),
                reason: format!(
                    "transition gate '{}' expected {}",
                    gate.field,
                    display_json(expected)
                ),
            });
        }
        if let Some(disallowed) = &gate.not_equals
            && value == disallowed
        {
            return Err(RuntimeError::GraphBlocked {
                step_id: step.id.clone(),
                reason: format!(
                    "transition gate '{}' must not equal {}",
                    gate.field,
                    display_json(disallowed)
                ),
            });
        }
        if gate.equals.is_none() && gate.not_equals.is_none() {
            return Err(RuntimeError::GraphBlocked {
                step_id: step.id.clone(),
                reason: format!("transition gate '{}' has no comparison", gate.field),
            });
        }
    }
    Ok(())
}

fn transition_field_value<'a>(field: &str, runs: &'a [StepRun]) -> Option<&'a JsonValue> {
    let mut segments = field.split('.');
    let step_id = segments.next()?;
    let run = runs.iter().rev().find(|run| run.step_id == step_id)?;
    let mut value = segments.next().and_then(|first| run.outputs.get(first))?;
    for segment in segments {
        let JsonValue::Object(object) = value else {
            return None;
        };
        value = object.get(segment)?;
    }
    Some(value)
}

fn display_json(value: &JsonValue) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "<unprintable>".to_owned())
}

fn output_error(run: &StepRun) -> String {
    if run.output.stderr.is_empty() {
        "cli-tool failed without stderr".to_owned()
    } else {
        run.output.stderr.clone()
    }
}

fn run_step<A>(
    runtime: &Runtime<A>,
    graph_dir: &Path,
    graph_name: &str,
    step: &GraphStep,
    attempt: u32,
    prior_runs: &[StepRun],
    caller: &mut dyn Caller,
) -> Result<StepRun, RuntimeError>
where
    A: SkillAdapter,
{
    let inputs = resolve_inputs(step, prior_runs)?;
    if step.run.is_some() {
        return run_native_step(runtime, graph_name, step, attempt, inputs, caller);
    }

    let skill_dir = skill_dir(graph_dir, step)?;
    let skill = load_skill(&skill_dir)?;
    let skill_name = skill.name.clone();
    let output = runtime.adapter.invoke(SkillInvocation {
        skill_name: skill.name,
        source: skill.source,
        inputs,
        resolved_inputs: JsonObject::new(),
        skill_directory: skill_dir,
        env: runtime.options.env.clone(),
    })?;
    let outputs = output_object(&output);
    let receipt = step_receipt(
        graph_name,
        &step.id,
        attempt,
        &output,
        &runtime.options.created_at,
    )?;
    Ok(StepRun {
        step_id: step.id.clone(),
        attempt,
        skill: skill_name,
        runner: step.runner.clone(),
        fanout_group: step.fanout_group.clone(),
        output,
        outputs,
        receipt,
    })
}

fn run_native_step<A>(
    runtime: &Runtime<A>,
    graph_name: &str,
    step: &GraphStep,
    attempt: u32,
    inputs: JsonObject,
    caller: &mut dyn Caller,
) -> Result<StepRun, RuntimeError>
where
    A: SkillAdapter,
{
    let run_type = run_type(step)?;
    match run_type.as_str() {
        "approval" => run_approval_step(runtime, graph_name, step, attempt, inputs, caller),
        other => Err(RuntimeError::UnsupportedRunStep {
            step_id: step.id.clone(),
            run_type: other.to_owned(),
        }),
    }
}

fn run_approval_step<A>(
    runtime: &Runtime<A>,
    graph_name: &str,
    step: &GraphStep,
    attempt: u32,
    inputs: JsonObject,
    caller: &mut dyn Caller,
) -> Result<StepRun, RuntimeError>
where
    A: SkillAdapter,
{
    let gate = approval_gate(step, &inputs)?;
    let request_id = format!("{}_approval", step.id);
    let resolution = request_approval(caller, request_id, gate.clone()).map_err(|source| {
        RuntimeError::InvalidRunStep {
            step_id: step.id.clone(),
            reason: source.to_string(),
        }
    })?;
    let outputs = approval_outputs(step, &gate, &resolution)?;
    let stdout = serde_json::to_string(&outputs)
        .map_err(|source| RuntimeError::json("serializing approval run output", source))?;
    let output = SkillOutput {
        status: InvocationStatus::Success,
        stdout,
        stderr: String::new(),
        exit_code: Some(0),
        duration_ms: 0,
        metadata: JsonObject::new(),
    };
    let receipt = step_receipt(
        graph_name,
        &step.id,
        attempt,
        &output,
        &runtime.options.created_at,
    )?;
    Ok(StepRun {
        step_id: step.id.clone(),
        attempt,
        skill: "run:approval".to_owned(),
        runner: step.runner.clone(),
        fanout_group: step.fanout_group.clone(),
        output,
        outputs,
        receipt,
    })
}

fn run_type(step: &GraphStep) -> Result<String, RuntimeError> {
    let Some(run) = &step.run else {
        return Err(RuntimeError::InvalidRunStep {
            step_id: step.id.clone(),
            reason: "missing run configuration".to_owned(),
        });
    };
    let Some(value) = run.get("type") else {
        return Err(RuntimeError::InvalidRunStep {
            step_id: step.id.clone(),
            reason: "run.type is required".to_owned(),
        });
    };
    string_value(step, "run.type", value)
}

fn approval_gate(step: &GraphStep, inputs: &JsonObject) -> Result<ApprovalGate, RuntimeError> {
    let gate_id = required_input_string(step, inputs, "gate_id")?;
    let reason = required_input_string(step, inputs, "reason")?;
    let gate_type = optional_input_string(step, inputs, "gate_type")?;
    let summary = approval_summary(inputs);
    Ok(ApprovalGate {
        id: gate_id,
        reason,
        gate_type,
        summary,
    })
}

fn approval_summary(inputs: &JsonObject) -> Option<JsonObject> {
    let mut summary = JsonObject::new();
    for (key, value) in inputs {
        if matches!(key.as_str(), "gate_id" | "reason" | "gate_type") {
            continue;
        }
        summary.insert(key.clone(), value.clone());
    }
    (!summary.is_empty()).then_some(summary)
}

fn approval_outputs(
    step: &GraphStep,
    gate: &ApprovalGate,
    resolution: &ApprovalResolution,
) -> Result<JsonObject, RuntimeError> {
    let mut data = JsonObject::new();
    data.insert("approved".to_owned(), approved_value(resolution));
    data.insert("gate_id".to_owned(), JsonValue::String(gate.id.clone()));
    data.insert(
        "idempotency_key".to_owned(),
        JsonValue::String(resolution.idempotency_key().to_owned()),
    );
    data.insert(
        "status".to_owned(),
        JsonValue::String(approval_status(resolution).to_owned()),
    );
    if let Some(actor) = resolution.actor() {
        data.insert("actor".to_owned(), JsonValue::String(actor_name(actor)));
    }

    let mut packet = JsonObject::new();
    if let Some(packet_id) = artifact_packet(step)? {
        packet.insert("packet".to_owned(), JsonValue::String(packet_id));
    }
    packet.insert("data".to_owned(), JsonValue::Object(data));

    let mut outputs = JsonObject::new();
    outputs.insert(
        artifact_wrap_as(step)?.to_owned(),
        JsonValue::Object(packet),
    );
    Ok(outputs)
}

fn approved_value(resolution: &ApprovalResolution) -> JsonValue {
    resolution
        .approved()
        .map_or(JsonValue::Null, JsonValue::Bool)
}

fn approval_status(resolution: &ApprovalResolution) -> &'static str {
    match resolution {
        ApprovalResolution::Approved { .. } => "approved",
        ApprovalResolution::Denied { .. } => "denied",
        ApprovalResolution::Pending { .. } => "pending",
    }
}

fn actor_name(actor: &ResolutionResponseActor) -> String {
    match actor {
        ResolutionResponseActor::Human => "human".to_owned(),
        ResolutionResponseActor::Agent => "agent".to_owned(),
    }
}

fn artifact_wrap_as(step: &GraphStep) -> Result<&str, RuntimeError> {
    let Some(artifacts) = &step.artifacts else {
        return Ok("approval");
    };
    let Some(value) = artifacts.get("wrap_as") else {
        return Ok("approval");
    };
    string_value_ref(step, "artifacts.wrap_as", value)
}

fn artifact_packet(step: &GraphStep) -> Result<Option<String>, RuntimeError> {
    let Some(artifacts) = &step.artifacts else {
        return Ok(None);
    };
    let Some(value) = artifacts.get("packet") else {
        return Ok(None);
    };
    Ok(Some(string_value(step, "artifacts.packet", value)?))
}

fn required_input_string(
    step: &GraphStep,
    inputs: &JsonObject,
    field: &str,
) -> Result<String, RuntimeError> {
    let Some(value) = inputs.get(field) else {
        return Err(RuntimeError::InvalidRunStep {
            step_id: step.id.clone(),
            reason: format!("{field} input is required"),
        });
    };
    string_value(step, field, value)
}

fn optional_input_string(
    step: &GraphStep,
    inputs: &JsonObject,
    field: &str,
) -> Result<Option<String>, RuntimeError> {
    let Some(value) = inputs.get(field) else {
        return Ok(None);
    };
    Ok(Some(string_value(step, field, value)?))
}

fn string_value(step: &GraphStep, field: &str, value: &JsonValue) -> Result<String, RuntimeError> {
    Ok(string_value_ref(step, field, value)?.to_owned())
}

fn string_value_ref<'a>(
    step: &GraphStep,
    field: &str,
    value: &'a JsonValue,
) -> Result<&'a str, RuntimeError> {
    let JsonValue::String(value) = value else {
        return Err(RuntimeError::InvalidRunStep {
            step_id: step.id.clone(),
            reason: format!("{field} must be a string"),
        });
    };
    Ok(value)
}

fn runtime_error_step_run<A>(
    runtime: &Runtime<A>,
    graph_name: &str,
    step: &GraphStep,
    attempt: u32,
    error: RuntimeError,
) -> Result<StepRun, RuntimeError>
where
    A: SkillAdapter,
{
    let output = SkillOutput {
        status: InvocationStatus::Failure,
        stdout: String::new(),
        stderr: error.to_string(),
        exit_code: None,
        duration_ms: 0,
        metadata: JsonObject::new(),
    };
    let outputs = output_object(&output);
    let receipt = step_receipt(
        graph_name,
        &step.id,
        attempt,
        &output,
        &runtime.options.created_at,
    )?;
    Ok(StepRun {
        step_id: step.id.clone(),
        attempt,
        skill: step.skill.as_deref().unwrap_or(step.id.as_str()).to_owned(),
        runner: step.runner.clone(),
        fanout_group: step.fanout_group.clone(),
        output,
        outputs,
        receipt,
    })
}

fn latest_fanout_receipt_ids(
    runs: &[StepRun],
    graph: &ExecutionGraph,
    group_id: &str,
) -> Vec<String> {
    graph
        .steps
        .iter()
        .filter(|step| step.fanout_group.as_deref() == Some(group_id))
        .filter_map(|step| {
            runs.iter()
                .rev()
                .find(|run| run.step_id == step.id)
                .map(|run| run.receipt.id.clone())
        })
        .collect()
}

fn fanout_sync_point(
    decision: &FanoutSyncDecision,
    branch_receipts: &[String],
) -> FanoutReceiptSyncPoint {
    FanoutReceiptSyncPoint {
        group_id: decision.group_id.clone(),
        strategy: receipt_strategy(&decision.strategy),
        decision: receipt_decision(&decision.decision),
        rule_fired: decision.rule_fired.clone(),
        reason: decision.reason.clone(),
        branch_count: decision.branch_count,
        success_count: decision.success_count,
        failure_count: decision.failure_count,
        required_successes: decision.required_successes,
        branch_receipts: branch_receipts.to_vec(),
        gate: decision_gate(&decision.gate),
    }
}

fn receipt_strategy(strategy: &FanoutSyncStrategy) -> FanoutReceiptStrategy {
    match strategy {
        FanoutSyncStrategy::All => FanoutReceiptStrategy::All,
        FanoutSyncStrategy::Any => FanoutReceiptStrategy::Any,
        FanoutSyncStrategy::Quorum => FanoutReceiptStrategy::Quorum,
    }
}

fn receipt_decision(decision: &FanoutSyncOutcome) -> FanoutReceiptDecision {
    match decision {
        FanoutSyncOutcome::Proceed => FanoutReceiptDecision::Proceed,
        FanoutSyncOutcome::Halt => FanoutReceiptDecision::Halt,
        FanoutSyncOutcome::Pause => FanoutReceiptDecision::Pause,
        FanoutSyncOutcome::Escalate => FanoutReceiptDecision::Escalate,
    }
}

fn decision_gate(gate: &Option<runx_core::state_machine::FanoutGate>) -> Option<JsonObject> {
    let value = serde_json::to_value(gate.as_ref()?).ok()?;
    let runx_contracts::JsonValue::Object(object) = serde_json::from_value(value).ok()? else {
        return None;
    };
    Some(object)
}

fn started_event(step_id: &str) -> ExecutionEvent {
    ExecutionEvent::StepStarted {
        message: format!("step {step_id} started"),
        data: None,
    }
}

fn completed_event(step_id: &str) -> ExecutionEvent {
    ExecutionEvent::StepCompleted {
        message: format!("step {step_id} completed"),
        data: None,
    }
}

fn failed_event(step_id: &str) -> ExecutionEvent {
    ExecutionEvent::Warning {
        message: format!("step {step_id} failed"),
        data: None,
    }
}
