use std::collections::BTreeMap;
use std::path::Path;

use runx_runtime::export::{RunxExportSkill, RunxExportSkillInput};

use super::{GeneratedFile, Target, display_path};

pub(super) fn plan_files(
    target: Target,
    project: bool,
    skills: &[RunxExportSkill],
    skill_dir: &Path,
) -> Vec<GeneratedFile> {
    skills
        .iter()
        .map(|skill| {
            let command_target = if project {
                skill.name.clone()
            } else {
                display_path(&skill.abs_dir)
            };
            let contents = render_shim(target, skill, &command_target);
            GeneratedFile {
                path: skill_dir.join(&skill.name).join("SKILL.md"),
                contents,
            }
        })
        .collect()
}

fn render_shim(target: Target, skill: &RunxExportSkill, command_target: &str) -> String {
    let mut output = String::new();
    output.push_str("---\n");
    output.push_str(&format!("name: {}\n", yaml_plain_or_quoted(&skill.name)));
    output.push_str("description: |-\n");
    output.push_str(&indent_block(&skill.description));
    if target == Target::Claude {
        output.push_str("allowed-tools: Bash(runx skill *)\n");
    }
    output.push_str("---\n");
    output.push_str(&format!("# {} - governed by runx\n\n", skill.name));
    output.push_str("This skill runs under runx governance. Do not perform the work yourself.\n");
    output.push_str(
        "Execution, policy enforcement, approvals, and the signed receipt happen inside runx.\n\n",
    );
    output.push_str("```bash\n");
    output.push_str(&render_command(command_target, &skill.inputs));
    output.push_str("\n```\n\n");
    output.push_str(&render_inputs(&skill.inputs));
    output.push_str("\nThen surface the returned receipt id, status, and artifact ids. If runx pauses for approval or input, relay its prompt and resume with the printed run-id.\n\n");
    output.push_str(&format!(
        "<!-- {} source={} - generated, do not edit -->\n",
        target.marker(),
        display_path(&skill.abs_dir)
    ));
    output
}

fn render_command(command_target: &str, inputs: &BTreeMap<String, RunxExportSkillInput>) -> String {
    let mut lines = vec![format!("runx skill {}", shell_quote(command_target))];
    for name in inputs.keys() {
        lines.push(format!("  --input {name}=\"<{name}>\""));
    }
    lines.push("  --json".to_owned());
    lines.join(" \\\n")
}

fn render_inputs(inputs: &BTreeMap<String, RunxExportSkillInput>) -> String {
    if inputs.is_empty() {
        return "Inputs: none.\n".to_owned();
    }
    let mut lines = vec!["Inputs:".to_owned()];
    for (name, input) in inputs {
        let requirement = if input.required {
            "required"
        } else {
            "optional"
        };
        let description = input
            .description
            .as_deref()
            .unwrap_or("No description provided.");
        lines.push(format!("- {name} ({requirement}) - {description}"));
    }
    format!("{}\n", lines.join("\n"))
}

fn indent_block(value: &str) -> String {
    let mut output = String::new();
    for line in value.lines() {
        output.push_str("  ");
        output.push_str(line);
        output.push('\n');
    }
    if value.is_empty() {
        output.push_str("  \n");
    }
    output
}

fn yaml_plain_or_quoted(value: &str) -> String {
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    {
        value.to_owned()
    } else {
        serde_json::to_string(value).unwrap_or_else(|_| "\"runx-skill\"".to_owned())
    }
}

fn shell_quote(value: &str) -> String {
    if value.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '/' | '.' | '_' | '-' | ':')
    }) {
        return value.to_owned();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
