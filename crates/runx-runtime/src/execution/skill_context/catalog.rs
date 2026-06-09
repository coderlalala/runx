use std::fs;
use std::path::Path;

use crate::RuntimeError;

pub(super) fn validate_local_context_manifest(
    step_id: &str,
    reference: &str,
    skill_dir: &Path,
) -> Result<(), RuntimeError> {
    let manifest_path = skill_dir.join("X.yaml");
    if !manifest_path.exists() {
        return Ok(());
    }
    let source = fs::read_to_string(&manifest_path).map_err(|source| {
        RuntimeError::io(format!("reading {}", manifest_path.display()), source)
    })?;
    let manifest = runx_parser::validate_runner_manifest(
        runx_parser::parse_runner_manifest_yaml(&source).map_err(RuntimeError::from)?,
    )
    .map_err(RuntimeError::from)?;
    validate_context_catalog(step_id, reference, manifest.catalog.as_ref())
}

pub(super) fn validate_registry_context_profile(
    step_id: &str,
    reference: &str,
    profile_document: Option<&str>,
) -> Result<(), RuntimeError> {
    let Some(profile_document) = profile_document else {
        return Ok(());
    };
    let manifest = runx_parser::validate_runner_manifest(
        runx_parser::parse_runner_manifest_yaml(profile_document).map_err(RuntimeError::from)?,
    )
    .map_err(RuntimeError::from)?;
    validate_context_catalog(step_id, reference, manifest.catalog.as_ref())
}

fn validate_context_catalog(
    step_id: &str,
    reference: &str,
    catalog: Option<&runx_parser::CatalogMetadata>,
) -> Result<(), RuntimeError> {
    let Some(catalog) = catalog else {
        return Ok(());
    };
    if matches!(
        catalog.role,
        runx_parser::CatalogRole::GraphStage
            | runx_parser::CatalogRole::RuntimePath
            | runx_parser::CatalogRole::HarnessFixture
    ) {
        return Err(RuntimeError::InvalidRunStep {
            step_id: step_id.to_owned(),
            reason: format!(
                "context skill '{reference}' has catalog.role={}, which is not eligible for context_skills",
                catalog.role.as_str()
            ),
        });
    }
    if catalog.visibility == runx_parser::CatalogVisibility::Internal
        && catalog.role != runx_parser::CatalogRole::Context
    {
        return Err(RuntimeError::InvalidRunStep {
            step_id: step_id.to_owned(),
            reason: format!(
                "context skill '{reference}' is internal and must declare catalog.role=context to be used as agent context"
            ),
        });
    }
    Ok(())
}
