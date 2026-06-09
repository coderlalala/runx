use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use runx_cli::export::{ExportError, ExportPlan, Target, run_export_command};

use crate::support::temp_root;

#[test]
fn exports_public_skills_to_claude_global_with_absolute_delegation()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = ExportFixture::new("runx-export-claude-global")?;
    fixture.write_skill("visible", None)?;
    fixture.write_skill("hidden", Some("internal"))?;

    let report = run_export_command(
        &ExportPlan {
            target: Target::Claude,
            refs: Vec::new(),
            project: false,
            json: false,
        },
        &fixture.project,
        &fixture.env,
    )?;

    assert_eq!(report.exported.len(), 1);
    assert_eq!(report.exported[0].skill, "visible");
    let shim = fixture.read_home_file(".claude/skills/visible/SKILL.md")?;
    assert!(shim.contains("allowed-tools: Bash(runx skill *)"));
    assert!(shim.contains("runx skill"));
    assert!(
        shim.contains(
            fixture
                .project
                .join("skills/visible")
                .to_str()
                .unwrap_or_default()
        )
    );
    assert!(shim.contains("runx-export:claude"));
    assert!(!fixture.home.join(".claude/skills/hidden/SKILL.md").exists());
    Ok(())
}

#[test]
fn exports_claude_project_scope_with_bare_skill_ref() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = ExportFixture::new("runx-export-claude-project")?;
    fixture.write_skill("visible", None)?;

    run_export_command(
        &ExportPlan {
            target: Target::Claude,
            refs: Vec::new(),
            project: true,
            json: false,
        },
        &fixture.project,
        &fixture.env,
    )?;

    let shim = fixture.read_project_file(".claude/skills/visible/SKILL.md")?;
    assert!(shim.contains("runx skill visible"));
    assert!(!shim.contains(&format!(
        "runx skill {}",
        fixture.project.to_str().unwrap_or_default()
    )));
    Ok(())
}

#[test]
fn explicit_ref_exports_internal_skill() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = ExportFixture::new("runx-export-explicit-internal")?;
    fixture.write_skill("hidden", Some("internal"))?;

    let report = run_export_command(
        &ExportPlan {
            target: Target::Claude,
            refs: vec!["hidden".to_owned()],
            project: false,
            json: false,
        },
        &fixture.project,
        &fixture.env,
    )?;

    assert_eq!(report.exported.len(), 1);
    assert_eq!(report.exported[0].skill, "hidden");
    assert!(fixture.home.join(".claude/skills/hidden/SKILL.md").exists());
    Ok(())
}

#[test]
fn codex_global_writes_shim_and_idempotent_permission_block()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = ExportFixture::new("runx-export-codex")?;
    fixture.write_skill("visible", None)?;
    fs::create_dir_all(fixture.home.join(".codex/rules"))?;
    fs::write(
        fixture.home.join(".codex/rules/default.rules"),
        "# existing approval\nallow_rule(pattern = [\"git\", \"status\"])\n",
    )?;

    run_export_command(
        &ExportPlan {
            target: Target::Codex,
            refs: Vec::new(),
            project: false,
            json: false,
        },
        &fixture.project,
        &fixture.env,
    )?;
    run_export_command(
        &ExportPlan {
            target: Target::Codex,
            refs: Vec::new(),
            project: false,
            json: false,
        },
        &fixture.project,
        &fixture.env,
    )?;

    let shim = fixture.read_home_file(".codex/skills/visible/SKILL.md")?;
    assert!(shim.contains("name: visible"));
    assert!(!shim.contains("allowed-tools"));
    assert!(shim.contains("runx-export:codex"));
    let rules = fixture.read_home_file(".codex/rules/default.rules")?;
    assert!(rules.contains("# existing approval"));
    assert_eq!(rules.matches("runx-export start").count(), 1);
    assert_eq!(
        rules
            .matches("prefix_rule(pattern = [\"runx\", \"skill\"]")
            .count(),
        1
    );
    Ok(())
}

#[test]
fn reexport_prunes_only_marked_generated_files() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = ExportFixture::new("runx-export-prune")?;
    fixture.write_skill("visible", None)?;
    let managed = fixture.home.join(".claude/skills/stale/SKILL.md");
    let manual = fixture.home.join(".claude/skills/manual/SKILL.md");
    fs::create_dir_all(managed.parent().ok_or("managed parent")?)?;
    fs::create_dir_all(manual.parent().ok_or("manual parent")?)?;
    fs::write(
        &managed,
        "---\nname: stale\n---\n<!-- runx-export:claude source=/missing - generated, do not edit -->\n",
    )?;
    fs::write(&manual, "---\nname: manual\n---\n# Hand-authored\n")?;

    let report = run_export_command(
        &ExportPlan {
            target: Target::Claude,
            refs: Vec::new(),
            project: false,
            json: false,
        },
        &fixture.project,
        &fixture.env,
    )?;

    assert_eq!(report.pruned.len(), 1);
    assert!(!managed.exists());
    assert!(manual.exists());
    Ok(())
}

#[test]
fn codex_project_scope_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = ExportFixture::new("runx-export-codex-project")?;
    fixture.write_skill("visible", None)?;

    let error = match run_export_command(
        &ExportPlan {
            target: Target::Codex,
            refs: Vec::new(),
            project: true,
            json: false,
        },
        &fixture.project,
        &fixture.env,
    ) {
        Ok(_) => return Err("codex project export should be disabled".into()),
        Err(error) => error,
    };

    match error {
        ExportError::Unsupported(message) => {
            assert!(message.contains("not supported until Codex project skill"));
        }
        other => return Err(format!("unexpected error: {other}").into()),
    }
    Ok(())
}

struct ExportFixture {
    root: PathBuf,
    project: PathBuf,
    home: PathBuf,
    env: BTreeMap<String, String>,
}

impl ExportFixture {
    fn new(prefix: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let root = temp_root(prefix);
        let project = root.join("project");
        let home = root.join("home");
        fs::create_dir_all(&project)?;
        fs::create_dir_all(&home)?;
        let env = [("HOME".to_owned(), path_string(&home)?)]
            .into_iter()
            .collect();
        Ok(Self {
            root,
            project,
            home,
            env,
        })
    }

    fn write_skill(
        &self,
        name: &str,
        visibility: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = self.project.join("skills").join(name);
        fs::create_dir_all(&dir)?;
        fs::write(
            dir.join("SKILL.md"),
            format!(
                "---\nname: {name}\ndescription: Export {name} through runx.\ninputs:\n  objective:\n    type: string\n    required: true\n    description: Work to perform.\n---\n# {name}\n\nRun the governed skill.\n"
            ),
        )?;
        if let Some(visibility) = visibility {
            fs::write(
                dir.join("X.yaml"),
                format!(
                    "skill: {name}\ncatalog:\n  kind: skill\n  audience: public\n  visibility: {visibility}\n  role: context\nrunners:\n  default:\n    default: true\n    type: agent-task\n    agent: reviewer\n    task: {name}\n"
                ),
            )?;
        }
        Ok(())
    }

    fn read_home_file(&self, relative: &str) -> Result<String, Box<dyn std::error::Error>> {
        Ok(fs::read_to_string(self.home.join(relative))?)
    }

    fn read_project_file(&self, relative: &str) -> Result<String, Box<dyn std::error::Error>> {
        Ok(fs::read_to_string(self.project.join(relative))?)
    }
}

impl Drop for ExportFixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).ok();
    }
}

fn path_string(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    path.to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| "path is not UTF-8".into())
}
