use std::collections::BTreeMap;

use uv_auth::CredentialsCache;
use uv_cache::Cache;
use uv_configuration::NoSources;
use uv_distribution::{LoweredExtraBuildDependencies, LoweredRequirement, LoweringError};
use uv_distribution_types::{
    ExtraBuildRequirement, ExtraBuildRequires, IndexLocations, IndexUrlError,
};
use uv_scripts::Pep723ItemRef;
use uv_workspace::WorkspaceCache;
use uv_workspace::pyproject::ExtraBuildDependency;

/// A failure while lowering a script's extra build dependencies.
#[derive(Debug, thiserror::Error)]
pub enum ScriptExtraBuildRequiresError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    IndexUrl(#[from] IndexUrlError),

    #[error(transparent)]
    Lowering(#[from] Box<LoweringError>),
}

impl From<LoweringError> for ScriptExtraBuildRequiresError {
    fn from(error: LoweringError) -> Self {
        Self::Lowering(Box::new(error))
    }
}

/// Determine the extra build requires for a script.
pub async fn script_extra_build_requires(
    script: Pep723ItemRef<'_>,
    sources: &NoSources,
    index_locations: &IndexLocations,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    credentials_cache: &CredentialsCache,
) -> Result<LoweredExtraBuildDependencies, ScriptExtraBuildRequiresError> {
    let script_dir = script.directory()?;
    let script_indexes = script
        .indexes(sources)
        .iter()
        .cloned()
        .map(|index| index.relative_to(&script_dir))
        .collect::<Result<Vec<_>, _>>()?;
    let script_sources = script.sources(sources);

    // Collect any `tool.uv.extra-build-dependencies` from the script.
    let empty = BTreeMap::default();
    let script_extra_build_dependencies = script
        .metadata()
        .tool
        .as_ref()
        .and_then(|tool| tool.uv.as_ref())
        .and_then(|uv| uv.extra_build_dependencies.as_ref())
        .unwrap_or(&empty);

    // Lower the extra build dependencies.
    let mut extra_build_requires = ExtraBuildRequires::default();
    for (name, requirements) in script_extra_build_dependencies {
        let mut lowered_requirements = Vec::new();
        for ExtraBuildDependency {
            requirement,
            match_runtime,
        } in requirements.iter().cloned()
        {
            lowered_requirements.extend(
                LoweredRequirement::from_non_workspace_requirement(
                    requirement,
                    script_dir.as_ref(),
                    script_sources.as_ref(),
                    &script_indexes,
                    index_locations,
                    cache,
                    workspace_cache,
                    credentials_cache,
                )
                .await
                .map(|requirement| {
                    requirement.map(|requirement| ExtraBuildRequirement {
                        requirement: requirement.into_inner(),
                        match_runtime,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            );
        }
        extra_build_requires.insert(name.clone(), lowered_requirements);
    }

    Ok(LoweredExtraBuildDependencies::from_lowered(
        extra_build_requires,
    ))
}
