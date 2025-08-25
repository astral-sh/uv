use std::collections::BTreeMap;
use std::path::Path;

use uv_configuration::SourceStrategy;
use uv_distribution_types::{
    ExtraBuildRequirement, ExtraBuildRequires, IndexLocations, Requirement,
};
use uv_normalize::PackageName;
use uv_workspace::pyproject::{ExtraBuildDependencies, ExtraBuildDependency, ToolUvSources};
use uv_workspace::{
    DiscoveryOptions, MemberDiscovery, ProjectWorkspace, Workspace, WorkspaceCache,
};

use crate::metadata::{LoweredRequirement, MetadataError};

/// Lowered requirements from a `[build-system.requires]` field in a `pyproject.toml` file.
#[derive(Debug, Clone)]
pub struct BuildRequires {
    pub name: Option<PackageName>,
    pub requires_dist: Vec<Requirement>,
}

impl BuildRequires {
    /// Lower without considering `tool.uv` in `pyproject.toml`, used for index and other archive
    /// dependencies.
    pub fn from_metadata23(metadata: uv_pypi_types::BuildRequires) -> Self {
        Self {
            name: metadata.name,
            requires_dist: metadata
                .requires_dist
                .into_iter()
                .map(Requirement::from)
                .collect(),
        }
    }

    /// Lower by considering `tool.uv` in `pyproject.toml` if present, used for Git and directory
    /// dependencies.
    pub async fn from_project_maybe_workspace(
        metadata: uv_pypi_types::BuildRequires,
        install_path: &Path,
        locations: &IndexLocations,
        sources: SourceStrategy,
        cache: &WorkspaceCache,
    ) -> Result<Self, MetadataError> {
        let discovery = match sources {
            SourceStrategy::Enabled => DiscoveryOptions::default(),
            SourceStrategy::Disabled => DiscoveryOptions {
                members: MemberDiscovery::None,
                ..Default::default()
            },
        };
        let Some(project_workspace) =
            ProjectWorkspace::from_maybe_project_root(install_path, &discovery, cache).await?
        else {
            return Ok(Self::from_metadata23(metadata));
        };

        Self::from_project_workspace(metadata, &project_workspace, locations, sources)
    }

    /// Lower the `build-system.requires` field from a `pyproject.toml` file.
    pub fn from_project_workspace(
        metadata: uv_pypi_types::BuildRequires,
        project_workspace: &ProjectWorkspace,
        locations: &IndexLocations,
        source_strategy: SourceStrategy,
    ) -> Result<Self, MetadataError> {
        // Collect any `tool.uv.index` entries.
        let empty = vec![];
        let project_indexes = match source_strategy {
            SourceStrategy::Enabled => project_workspace
                .current_project()
                .pyproject_toml()
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.index.as_deref())
                .unwrap_or(&empty),
            SourceStrategy::Disabled => &empty,
        };

        // Collect any `tool.uv.sources` and `tool.uv.dev_dependencies` from `pyproject.toml`.
        let empty = BTreeMap::default();
        let project_sources = match source_strategy {
            SourceStrategy::Enabled => project_workspace
                .current_project()
                .pyproject_toml()
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.sources.as_ref())
                .map(ToolUvSources::inner)
                .unwrap_or(&empty),
            SourceStrategy::Disabled => &empty,
        };

        // Lower the requirements.
        let requires_dist = metadata.requires_dist.into_iter();
        let requires_dist = match source_strategy {
            SourceStrategy::Enabled => requires_dist
                .flat_map(|requirement| {
                    let requirement_name = requirement.name.clone();
                    let extra = requirement.marker.top_level_extra_name();
                    let group = None;
                    LoweredRequirement::from_requirement(
                        requirement,
                        metadata.name.as_ref(),
                        project_workspace.project_root(),
                        project_sources,
                        project_indexes,
                        extra.as_deref(),
                        group,
                        locations,
                        project_workspace.workspace(),
                        None,
                    )
                    .map(move |requirement| match requirement {
                        Ok(requirement) => Ok(requirement.into_inner()),
                        Err(err) => Err(MetadataError::LoweringError(
                            requirement_name.clone(),
                            Box::new(err),
                        )),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            SourceStrategy::Disabled => requires_dist.into_iter().map(Requirement::from).collect(),
        };

        Ok(Self {
            name: metadata.name,
            requires_dist,
        })
    }

    /// Lower the `build-system.requires` field from a `pyproject.toml` file.
    pub fn from_workspace(
        metadata: uv_pypi_types::BuildRequires,
        workspace: &Workspace,
        locations: &IndexLocations,
        source_strategy: SourceStrategy,
    ) -> Result<Self, MetadataError> {
        // Collect any `tool.uv.index` entries.
        let empty = vec![];
        let project_indexes = match source_strategy {
            SourceStrategy::Enabled => workspace
                .pyproject_toml()
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.index.as_deref())
                .unwrap_or(&empty),
            SourceStrategy::Disabled => &empty,
        };

        // Collect any `tool.uv.sources` and `tool.uv.dev_dependencies` from `pyproject.toml`.
        let empty = BTreeMap::default();
        let project_sources = match source_strategy {
            SourceStrategy::Enabled => workspace
                .pyproject_toml()
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.sources.as_ref())
                .map(ToolUvSources::inner)
                .unwrap_or(&empty),
            SourceStrategy::Disabled => &empty,
        };

        // Lower the requirements.
        let requires_dist = metadata.requires_dist.into_iter();
        let requires_dist = match source_strategy {
            SourceStrategy::Enabled => requires_dist
                .flat_map(|requirement| {
                    let requirement_name = requirement.name.clone();
                    let extra = requirement.marker.top_level_extra_name();
                    let group = None;
                    LoweredRequirement::from_requirement(
                        requirement,
                        None,
                        workspace.install_path(),
                        project_sources,
                        project_indexes,
                        extra.as_deref(),
                        group,
                        locations,
                        workspace,
                        None,
                    )
                    .map(move |requirement| match requirement {
                        Ok(requirement) => Ok(requirement.into_inner()),
                        Err(err) => Err(MetadataError::LoweringError(
                            requirement_name.clone(),
                            Box::new(err),
                        )),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            SourceStrategy::Disabled => requires_dist.into_iter().map(Requirement::from).collect(),
        };

        Ok(Self {
            name: metadata.name,
            requires_dist,
        })
    }
}

/// Lowered extra build dependencies.
///
/// This is a wrapper around [`ExtraBuildRequires`] that provides methods to lower
/// [`ExtraBuildDependencies`] from a workspace context or from already lowered dependencies.
#[derive(Debug, Clone, Default)]
pub struct LoweredExtraBuildDependencies(ExtraBuildRequires);

impl LoweredExtraBuildDependencies {
    /// Return the [`ExtraBuildRequires`] that this was lowered into.
    pub fn into_inner(self) -> ExtraBuildRequires {
        self.0
    }

    /// Create from a workspace, lowering the extra build dependencies.
    pub fn from_workspace(
        extra_build_dependencies: ExtraBuildDependencies,
        workspace: &Workspace,
        index_locations: &IndexLocations,
        source_strategy: SourceStrategy,
    ) -> Result<Self, MetadataError> {
        match source_strategy {
            SourceStrategy::Enabled => {
                // Collect project sources and indexes
                let project_indexes = workspace
                    .pyproject_toml()
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.uv.as_ref())
                    .and_then(|uv| uv.index.as_deref())
                    .unwrap_or(&[]);

                let empty_sources = BTreeMap::default();
                let project_sources = workspace
                    .pyproject_toml()
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.uv.as_ref())
                    .and_then(|uv| uv.sources.as_ref())
                    .map(ToolUvSources::inner)
                    .unwrap_or(&empty_sources);

                // Lower each package's extra build dependencies
                let mut build_requires = ExtraBuildRequires::default();
                for (package_name, requirements) in extra_build_dependencies {
                    let lowered: Vec<ExtraBuildRequirement> = requirements
                        .into_iter()
                        .flat_map(
                            |ExtraBuildDependency {
                                 requirement,
                                 match_runtime,
                             }| {
                                let requirement_name = requirement.name.clone();
                                let extra = requirement.marker.top_level_extra_name();
                                let group = None;
                                LoweredRequirement::from_requirement(
                                    requirement,
                                    None,
                                    workspace.install_path(),
                                    project_sources,
                                    project_indexes,
                                    extra.as_deref(),
                                    group,
                                    index_locations,
                                    workspace,
                                    None,
                                )
                                .map(move |requirement| {
                                    match requirement {
                                        Ok(requirement) => Ok(ExtraBuildRequirement {
                                            requirement: requirement.into_inner(),
                                            match_runtime,
                                        }),
                                        Err(err) => Err(MetadataError::LoweringError(
                                            requirement_name.clone(),
                                            Box::new(err),
                                        )),
                                    }
                                })
                            },
                        )
                        .collect::<Result<Vec<_>, _>>()?;
                    build_requires.insert(package_name, lowered);
                }
                Ok(Self(build_requires))
            }
            SourceStrategy::Disabled => Ok(Self::from_non_lowered(extra_build_dependencies)),
        }
    }

    /// Create from lowered dependencies (for non-workspace contexts, like scripts).
    pub fn from_lowered(extra_build_dependencies: ExtraBuildRequires) -> Self {
        Self(extra_build_dependencies)
    }

    /// Create from unlowered dependencies (e.g., for contexts in the pip CLI).
    pub fn from_non_lowered(extra_build_dependencies: ExtraBuildDependencies) -> Self {
        Self(
            extra_build_dependencies
                .into_iter()
                .map(|(name, requirements)| {
                    (
                        name,
                        requirements
                            .into_iter()
                            .map(
                                |ExtraBuildDependency {
                                     requirement,
                                     match_runtime,
                                 }| {
                                    ExtraBuildRequirement {
                                        requirement: requirement.into(),
                                        match_runtime,
                                    }
                                },
                            )
                            .collect::<Vec<_>>(),
                    )
                })
                .collect(),
        )
    }
}
