use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::{
    CODEX_RULE, CODEX_RULE_END, CODEX_RULE_START, ExportError, GeneratedFile, Target, display_path,
};

pub(super) fn write_files(files: &[GeneratedFile]) -> Result<(), ExportError> {
    for file in files {
        let parent = file.path.parent().ok_or_else(|| ExportError::Io {
            context: format!("resolving parent for {}", display_path(&file.path)),
            source: std::io::Error::other("path has no parent"),
        })?;
        fs::create_dir_all(parent).map_err(|source| ExportError::Io {
            context: format!("creating {}", display_path(parent)),
            source,
        })?;
        fs::write(&file.path, &file.contents).map_err(|source| ExportError::Io {
            context: format!("writing {}", display_path(&file.path)),
            source,
        })?;
    }
    Ok(())
}

pub(super) fn prune_managed_files(
    target: Target,
    skill_dir: &Path,
    files: &[GeneratedFile],
) -> Result<Vec<String>, ExportError> {
    let wanted = files
        .iter()
        .map(|file| file.path.clone())
        .collect::<BTreeSet<_>>();
    let entries = match fs::read_dir(skill_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(ExportError::Io {
                context: format!("reading {}", display_path(skill_dir)),
                source,
            });
        }
    };
    let mut pruned = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| ExportError::Io {
            context: format!("reading {}", display_path(skill_dir)),
            source,
        })?;
        let skill_file = entry.path().join("SKILL.md");
        if wanted.contains(&skill_file) || !skill_file.exists() {
            continue;
        }
        let contents = read_to_string(&skill_file)?;
        if !contents.contains(target.marker()) {
            continue;
        }
        fs::remove_file(&skill_file).map_err(|source| ExportError::Io {
            context: format!("removing {}", display_path(&skill_file)),
            source,
        })?;
        let _ignored = fs::remove_dir(entry.path());
        pruned.push(display_path(&skill_file));
    }
    pruned.sort();
    Ok(pruned)
}

pub(super) fn merge_codex_rules(path: &Path) -> Result<PathBuf, ExportError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ExportError::Io {
            context: format!("creating {}", display_path(parent)),
            source,
        })?;
    }
    let existing = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(source) => {
            return Err(ExportError::Io {
                context: format!("reading {}", display_path(path)),
                source,
            });
        }
    };
    let block = format!("{CODEX_RULE_START}\n{CODEX_RULE}\n{CODEX_RULE_END}\n");
    let contents = replace_or_append_block(&existing, &block);
    fs::write(path, contents).map_err(|source| ExportError::Io {
        context: format!("writing {}", display_path(path)),
        source,
    })?;
    Ok(path.to_path_buf())
}

fn replace_or_append_block(existing: &str, block: &str) -> String {
    if let Some(start) = existing.find(CODEX_RULE_START)
        && let Some(relative_end) = existing[start..].find(CODEX_RULE_END)
    {
        let end = start + relative_end + CODEX_RULE_END.len();
        let mut output = String::new();
        output.push_str(&existing[..start]);
        output.push_str(block);
        let suffix = existing[end..].trim_start_matches(['\r', '\n']);
        output.push_str(suffix);
        return ensure_trailing_newline(output);
    }
    let mut output = ensure_trailing_newline(existing.to_owned());
    if !output.is_empty() && !output.ends_with("\n\n") {
        output.push('\n');
    }
    output.push_str(block);
    output
}

fn read_to_string(path: &Path) -> Result<String, ExportError> {
    fs::read_to_string(path).map_err(|source| ExportError::Io {
        context: format!("reading {}", display_path(path)),
        source,
    })
}

fn ensure_trailing_newline(mut value: String) -> String {
    if !value.is_empty() && !value.ends_with('\n') {
        value.push('\n');
    }
    value
}
