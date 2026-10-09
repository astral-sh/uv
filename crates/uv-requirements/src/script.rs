use std::collections::BTreeMap;

use uv_auth::CredentialsCache;
use uv_cache::Cache;
use uv_configuration::{NoSources, Override, PackageOverride};
use uv_distribution::{LoweredExtraBuildDependencies, LoweredRequirement, LoweringError};
use uv_distribution_types::{
    ExtraBuildRequirement, ExtraBuildRequires, IndexLocations, IndexUrlError,
};
use uv_scripts::Pep723ItemRef;
use uv_workspace::WorkspaceCache;
use uv_workspace::pyproject::ExtraBuildDependency;

use crate::RequirementsSpecification;

/// A failure while lowering a script's requirements or extra build dependencies.
#[derive(Debug, thiserror::Error)]
pub enum ScriptRequirementsError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    IndexUrl(#[from] IndexUrlError),

    #[error(transparent)]
    Lowering(#[from] Box<LoweringError>),
}

impl From<LoweringError> for ScriptRequirementsError {
    fn from(error: LoweringError) -> Self {
        Self::Lowering(Box::new(error))
    }
}

/// Determine the [`RequirementsSpecification`] for a script.
pub async fn script_specification(
    script: Pep723ItemRef<'_>,
    sources: &NoSources,
    index_locations: &IndexLocations,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    credentials_cache: &CredentialsCache,
) -> Result<Option<RequirementsSpecification>, ScriptRequirementsError> {
    let Some(dependencies) = script.metadata().dependencies.as_ref() else {
        return Ok(None);
    };

    let script_dir = script.directory()?;
    let script_indexes = script
        .indexes(sources)
        .iter()
        .cloned()
        .map(|index| index.relative_to(&script_dir))
        .collect::<Result<Vec<_>, _>>()?;
    let script_sources = script.sources(sources);

    let mut requirements = Vec::new();
    for requirement in dependencies.iter().cloned() {
        requirements.extend(
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
            .map(|requirement| requirement.map(LoweredRequirement::into_inner))
            .collect::<Result<Vec<_>, _>>()?,
        );
    }
    let constraint_dependencies = script
        .metadata()
        .tool
        .as_ref()
        .and_then(|tool| tool.uv.as_ref())
        .and_then(|uv| uv.constraint_dependencies.as_ref())
        .into_iter()
        .flatten()
        .cloned();
    let mut constraints = Vec::new();
    for requirement in constraint_dependencies {
        constraints.extend(
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
            .map(|requirement| requirement.map(LoweredRequirement::into_inner))
            .collect::<Result<Vec<_>, _>>()?,
        );
    }
    let overrides = {
        let override_entries = script
            .metadata()
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.override_dependencies.as_ref())
            .into_iter()
            .flatten()
            .cloned();
        let mut overrides = Vec::new();
        for entry in override_entries {
            match entry {
                Override::Requirement(requirement) => {
                    overrides.extend(
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
                            requirement
                                .map(|requirement| Override::Requirement(requirement.into_inner()))
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                    );
                }
                Override::Package(package) => {
                    let mut dependencies = Vec::new();
                    for requirement in package.dependencies.into_vec() {
                        dependencies.extend(
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
                            .map(|requirement| requirement.map(LoweredRequirement::into_inner))
                            .collect::<Result<Vec<_>, _>>()?,
                        );
                    }
                    overrides.push(Override::Package(PackageOverride {
                        package: package.package,
                        dependencies: dependencies.into_boxed_slice(),
                    }));
                }
            }
        }
        overrides
    };
    let excludes = script
        .metadata()
        .tool
        .as_ref()
        .and_then(|tool| tool.uv.as_ref())
        .and_then(|uv| uv.exclude_dependencies.as_ref())
        .into_iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();

    let mut specification =
        RequirementsSpecification::from_excludes(requirements, constraints, Vec::new(), Vec::new());
    specification.override_dependencies = overrides;
    specification.excludes = excludes;
    Ok(Some(specification))
}

/// Determine the extra build requires for a script.
pub async fn script_extra_build_requires(
    script: Pep723ItemRef<'_>,
    sources: &NoSources,
    index_locations: &IndexLocations,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    credentials_cache: &CredentialsCache,
) -> Result<LoweredExtraBuildDependencies, ScriptRequirementsError> {
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
