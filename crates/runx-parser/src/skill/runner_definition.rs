use runx_contracts::{JsonObject, JsonValue};

use crate::ValidationError;

use super::{
    FIELDS, SkillGovernance, SkillRunnerDefinition, field_value, first_value, nested_value,
    validate_allowed_tools, validate_artifact_contract, validate_execution_semantics,
    validate_idempotency, validate_inputs, validate_mutating, validate_retry, validate_source,
};

pub(crate) fn validate_runner_definition(
    name: &str,
    runner: JsonObject,
) -> Result<SkillRunnerDefinition, ValidationError> {
    let runx = FIELDS.optional_object(runner.get("runx"), &format!("runners.{name}.runx"))?;
    crate::runner::resolve_post_run_reflect_policy(runx.as_ref(), &format!("runners.{name}.runx"))?;
    let source_record = FIELDS
        .optional_object(runner.get("source"), &format!("runners.{name}.source"))?
        .unwrap_or_else(|| runner.clone());
    let risk = runner.get("risk").cloned();
    let governance = validate_runner_governance(name, &runner, runx.as_ref(), risk.as_ref())?;
    Ok(SkillRunnerDefinition {
        name: name.to_owned(),
        default: FIELDS
            .optional_bool(runner.get("default"), &format!("runners.{name}.default"))?
            .unwrap_or(false),
        source: validate_source(&source_record, runx.as_ref())?,
        inputs: validate_inputs(
            FIELDS
                .optional_object(runner.get("inputs"), &format!("runners.{name}.inputs"))?
                .unwrap_or_default(),
        )?,
        auth: runner.get("auth").cloned(),
        risk: risk.clone(),
        runtime: runner.get("runtime").cloned(),
        retry: governance.retry,
        idempotency: governance.idempotency,
        mutating: governance.mutating,
        artifacts: governance.artifacts,
        allowed_tools: governance.allowed_tools,
        execution: governance.execution,
        runx,
        raw: runner,
    })
}

fn validate_runner_governance(
    name: &str,
    runner: &JsonObject,
    runx: Option<&JsonObject>,
    risk: Option<&JsonValue>,
) -> Result<SkillGovernance, ValidationError> {
    Ok(SkillGovernance {
        retry: validate_retry(
            first_value(runner.get("retry"), field_value(runx, "retry")),
            &format!("runners.{name}.retry"),
        )?,
        idempotency: validate_idempotency(
            first_value(runner.get("idempotency"), field_value(runx, "idempotency")),
            &format!("runners.{name}.idempotency"),
        )?,
        mutating: validate_mutating(
            first_value(
                first_value(runner.get("mutating"), nested_value(risk, "mutating")),
                field_value(runx, "mutating"),
            ),
            &format!("runners.{name}.mutating"),
        )?,
        artifacts: validate_artifact_contract(
            first_value(runner.get("artifacts"), field_value(runx, "artifacts")),
            &format!("runners.{name}.artifacts"),
        )?,
        allowed_tools: validate_allowed_tools(
            field_value(runx, "allowed_tools"),
            &format!("runners.{name}.runx.allowed_tools"),
        )?,
        execution: validate_execution_semantics(
            first_value(runner.get("execution"), field_value(runx, "execution")),
            &format!("runners.{name}.execution"),
        )?,
    })
}
