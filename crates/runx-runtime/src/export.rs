use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use runx_parser::{
    CatalogVisibility, parse_runner_manifest_yaml, parse_skill_markdown, validate_runner_manifest,
    validate_skill,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunxExportSkill {
    pub name: String,
    pub description: String,
    pub inputs: BTreeMap<String, RunxExportSkillInput>,
    pub abs_dir: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunxExportSkillInput {
    pub required: bool,
    pub description: Option<String>,
}

#[derive(Debug)]
pub enum RunxExportLoadError {
    InvalidArgs(String),
    Io {
        context: String,
        source: std::io::Error,
    },
    Parse(String),
}

impl std::fmt::Display for RunxExportLoadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidArgs(message) | Self::Parse(message) => formatter.write_str(message),
            Self::Io { context, source } => write!(formatter, "{context}: {source}"),
        }
    }
}

impl std::error::Error for RunxExportLoadError {}

pub fn load_export_skills(
    root: &Path,
    refs: &[String],
) -> Result<Vec<RunxExportSkill>, RunxExportLoadError> {
    let explicit = !refs.is_empty();
    let paths = if explicit {
        refs.iter()
            .map(|reference| resolve_skill_ref(root, reference))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        discover_skill_paths(root)?
    };

    let mut skills = Vec::new();
    for skill_dir in paths {
        let manifest = read_optional_runner_manifest(&skill_dir)?;
        if !explicit && manifest_visibility(&manifest) == Some(CatalogVisibility::Internal) {
            continue;
        }
        let skill = read_validated_skill(&skill_dir)?;
        skills.push(RunxExportSkill {
            name: skill.name,
            description: skill
                .description
                .unwrap_or_else(|| "Run this skill through runx governance.".to_owned()),
            inputs: skill
                .inputs
                .into_iter()
                .map(|(name, input)| {
                    (
                        name,
                        RunxExportSkillInput {
                            required: input.required,
                            description: input.description,
                        },
                    )
                })
                .collect(),
            abs_dir: skill_dir,
        });
    }
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(skills)
}

fn discover_skill_paths(root: &Path) -> Result<Vec<PathBuf>, RunxExportLoadError> {
    let mut paths = Vec::new();
    if root.join("SKILL.md").exists() {
        paths.push(canonicalize(root, "canonicalizing root skill")?);
    }
    let skills_root = root.join("skills");
    let entries = match fs::read_dir(&skills_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(paths),
        Err(source) => {
            return Err(RunxExportLoadError::Io {
                context: format!("reading {}", display_path(&skills_root)),
                source,
            });
        }
    };
    let mut candidates = entries
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|source| RunxExportLoadError::Io {
                    context: format!("reading {}", display_path(&skills_root)),
                    source,
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    candidates.sort();
    for candidate in candidates {
        if candidate.join("SKILL.md").exists() {
            paths.push(canonicalize(&candidate, "canonicalizing skill directory")?);
        }
    }
    Ok(paths)
}

fn resolve_skill_ref(root: &Path, reference: &str) -> Result<PathBuf, RunxExportLoadError> {
    let reference_path = Path::new(reference);
    let candidates = if reference_path.is_absolute() {
        vec![reference_path.to_path_buf()]
    } else {
        vec![
            root.join("skills").join(reference_path),
            root.join(reference_path),
        ]
    };
    for candidate in candidates {
        let skill_dir = if candidate
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("SKILL.md"))
        {
            candidate.parent().map(Path::to_path_buf)
        } else {
            Some(candidate)
        };
        let Some(skill_dir) = skill_dir else {
            continue;
        };
        if skill_dir.join("SKILL.md").exists() {
            return canonicalize(&skill_dir, "canonicalizing skill reference");
        }
    }
    Err(RunxExportLoadError::InvalidArgs(format!(
        "skill reference {reference} does not resolve to a SKILL.md package"
    )))
}

fn read_validated_skill(
    skill_dir: &Path,
) -> Result<runx_parser::ValidatedSkill, RunxExportLoadError> {
    let path = skill_dir.join("SKILL.md");
    let source = read_to_string(&path)?;
    let raw = parse_skill_markdown(&source).map_err(|error| {
        RunxExportLoadError::Parse(format!("parsing {}: {error}", display_path(&path)))
    })?;
    validate_skill(raw).map_err(|error| {
        RunxExportLoadError::Parse(format!("validating {}: {error}", display_path(&path)))
    })
}

fn read_optional_runner_manifest(
    skill_dir: &Path,
) -> Result<Option<runx_parser::SkillRunnerManifest>, RunxExportLoadError> {
    let path = skill_dir.join("X.yaml");
    if !path.exists() {
        return Ok(None);
    }
    let source = read_to_string(&path)?;
    let raw = parse_runner_manifest_yaml(&source).map_err(|error| {
        RunxExportLoadError::Parse(format!("parsing {}: {error}", display_path(&path)))
    })?;
    validate_runner_manifest(raw).map(Some).map_err(|error| {
        RunxExportLoadError::Parse(format!("validating {}: {error}", display_path(&path)))
    })
}

fn manifest_visibility(
    manifest: &Option<runx_parser::SkillRunnerManifest>,
) -> Option<CatalogVisibility> {
    manifest
        .as_ref()
        .and_then(|manifest| manifest.catalog.as_ref())
        .map(|catalog| catalog.visibility)
}

fn canonicalize(path: &Path, context: &str) -> Result<PathBuf, RunxExportLoadError> {
    fs::canonicalize(path).map_err(|source| RunxExportLoadError::Io {
        context: format!("{context} {}", display_path(path)),
        source,
    })
}

fn read_to_string(path: &Path) -> Result<String, RunxExportLoadError> {
    fs::read_to_string(path).map_err(|source| RunxExportLoadError::Io {
        context: format!("reading {}", display_path(path)),
        source,
    })
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
