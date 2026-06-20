use runx_contracts::{JsonObject, JsonValue};
use runx_parser::{SkillRunnerManifest, ValidatedSkill};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScopeParseError {
    pub(crate) field: String,
    pub(crate) message: String,
}

impl std::fmt::Display for ScopeParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} {}", self.field, self.message)
    }
}

impl std::error::Error for ScopeParseError {}

pub(crate) fn required_scopes_from_skill(
    skill: &ValidatedSkill,
) -> Result<Vec<String>, ScopeParseError> {
    Ok(unique_strings(
        string_array_field(skill.auth.as_ref(), "auth.scopes")?
            .into_iter()
            .chain(string_array_field_from_object(
                skill.runx.as_ref(),
                "runx.scopes",
            )?),
    ))
}

pub(super) fn required_scopes_from_skill_and_runner(
    skill: &ValidatedSkill,
    manifest: Option<&SkillRunnerManifest>,
) -> Result<Vec<String>, ScopeParseError> {
    Ok(unique_strings(
        required_scopes_from_skill(skill)?
            .into_iter()
            .chain(required_scopes_from_runner_manifest(manifest)?),
    ))
}

fn required_scopes_from_runner_manifest(
    manifest: Option<&SkillRunnerManifest>,
) -> Result<Vec<String>, ScopeParseError> {
    let mut scopes = Vec::new();
    let Some(manifest) = manifest else {
        return Ok(scopes);
    };
    for (runner_name, runner) in &manifest.runners {
        scopes.extend(string_array_field(
            runner.auth.as_ref(),
            &format!("runners.{runner_name}.auth.scopes"),
        )?);
        scopes.extend(string_array_field_from_object(
            runner.runx.as_ref(),
            &format!("runners.{runner_name}.runx.scopes"),
        )?);
    }
    Ok(unique_strings(scopes))
}

fn string_array_field(
    value: Option<&JsonValue>,
    field: &str,
) -> Result<Vec<String>, ScopeParseError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let JsonValue::Object(record) = value else {
        return Err(ScopeParseError {
            field: field_parent(field).to_owned(),
            message: "must be an object when declaring scopes".to_owned(),
        });
    };
    string_array_field_from_object(Some(record), field)
}

fn string_array_field_from_object(
    value: Option<&JsonObject>,
    field: &str,
) -> Result<Vec<String>, ScopeParseError> {
    let Some(record) = value else {
        return Ok(Vec::new());
    };
    let scope_key = field.rsplit('.').next().unwrap_or(field);
    let Some(value) = record.get(scope_key) else {
        return Ok(Vec::new());
    };
    let JsonValue::Array(values) = value else {
        return Err(ScopeParseError {
            field: field.to_owned(),
            message: "must be an array of non-empty strings".to_owned(),
        });
    };
    let mut scopes = Vec::new();
    for (index, value) in values.iter().enumerate() {
        let Some(scope) = value
            .as_str()
            .map(str::trim)
            .filter(|scope| !scope.is_empty())
        else {
            return Err(ScopeParseError {
                field: format!("{field}[{index}]"),
                message: "must be a non-empty string".to_owned(),
            });
        };
        scopes.push(scope.to_owned());
    }
    Ok(scopes)
}

fn field_parent(field: &str) -> &str {
    field.rsplit_once('.').map_or(field, |(parent, _)| parent)
}

fn unique_strings(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut unique_values = Vec::new();
    for value in values {
        if !unique_values.contains(&value) {
            unique_values.push(value);
        }
    }
    unique_values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_arrays_reject_non_string_entries() {
        let mut record = JsonObject::new();
        record.insert(
            "scopes".to_owned(),
            JsonValue::Array(vec![
                JsonValue::String("repo.read".to_owned()),
                JsonValue::Bool(true),
            ]),
        );

        let error = string_array_field_from_object(Some(&record), "auth.scopes")
            .expect_err("malformed scope entry must fail closed");
        assert_eq!(error.field, "auth.scopes[1]");
    }

    #[test]
    fn scope_arrays_trim_deduplicate_and_reject_empty_entries() {
        let mut record = JsonObject::new();
        record.insert(
            "scopes".to_owned(),
            JsonValue::Array(vec![
                JsonValue::String(" repo.read ".to_owned()),
                JsonValue::String("".to_owned()),
            ]),
        );

        let error = string_array_field_from_object(Some(&record), "auth.scopes")
            .expect_err("empty scope entry must fail closed");
        assert_eq!(error.field, "auth.scopes[1]");
    }
}
