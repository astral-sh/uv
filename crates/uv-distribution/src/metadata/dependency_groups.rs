use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use uv_configuration::SourceStrategy;
use uv_distribution_types::{IndexLocations, Requirement};
use uv_normalize::{GroupName, PackageName};
use uv_workspace::dependency_groups::FlatDependencyGroups;
use uv_workspace::pyproject::{Sources, ToolUvSources};
use uv_workspace::{
    DiscoveryOptions, MemberDiscovery, VirtualProject, WorkspaceCache, WorkspaceError,
};

use crate::metadata::{GitWorkspaceMember, LoweredRequirement, MetadataError};

/// Like [`crate::RequiresDist`] but only supporting dependency-groups.
///
/// PEP 735 says:
///
/// > A pyproject.toml file with only `[dependency-groups]` and no other tables is valid.
///
/// This is a special carveout to enable users to adopt dependency-groups without having
/// to learn about projects. It is supported by `pip install --group`, and thus interfaces
/// like `uv pip install --group` must also support it for interop and conformance.
///
/// On paper this is trivial to support because dependency-groups are so self-contained
/// that they're basically a `requirements.txt` embedded within a pyproject.toml, so it's
/// fine to just grab that section and handle it independently.
///
/// However several uv extensions make this complicated, notably, as of this writing:
///
/// * tool.uv.sources
/// * tool.uv.index
///
/// These fields may also be present in the pyproject.toml, and, critically,
/// may be defined and inherited in a parent workspace pyproject.toml.
///
/// Therefore, we need to gracefully degrade from a full workspacey situation all
/// the way down to one of these stub pyproject.tomls the PEP defines. This is why
/// we avoid going through `RequiresDist` -- we don't want to muddy up the "compile a package"
/// logic with support for non-project/workspace pyproject.tomls, and we don't want to
/// muddy this logic up with setuptools fallback modes that `RequiresDist` wants.
///
/// (We used to shove this feature into that path, and then we would see there's no metadata
/// and try to run setuptools to try to desperately find any metadata, and then error out.)
#[derive(Debug, Clone)]
pub struct SourcedDependencyGroups {
    pub name: Option<PackageName>,
    pub dependency_groups: BTreeMap<GroupName, Box<[Requirement]>>,
}

impl SourcedDependencyGroups {
    /// Lower by considering `tool.uv` in `pyproject.toml` if present, used for Git and directory
    /// dependencies.
    pub async fn from_virtual_project(
        pyproject_path: &Path,
        git_member: Option<&GitWorkspaceMember<'_>>,
        locations: &IndexLocations,
        source_strategy: SourceStrategy,
        cache: &WorkspaceCache,
    ) -> Result<Self, MetadataError> {
        let discovery = DiscoveryOptions {
            stop_discovery_at: git_member.map(|git_member| {
                git_member
                    .fetch_root
                    .parent()
                    .expect("git checkout has a parent")
                    .to_path_buf()
            }),
            members: match source_strategy {
                SourceStrategy::Enabled => MemberDiscovery::default(),
                SourceStrategy::Disabled => MemberDiscovery::None,
            },
        };

        // The subsequent API takes an absolute path to the dir the pyproject is in
        let empty = PathBuf::new();
        let absolute_pyproject_path =
            std::path::absolute(pyproject_path).map_err(WorkspaceError::Normalize)?;
        let project_dir = absolute_pyproject_path.parent().unwrap_or(&empty);
        let project = VirtualProject::discover_defaulted(project_dir, &discovery, cache).await?;

        // Collect the dependency groups.
        let dependency_groups =
            FlatDependencyGroups::from_pyproject_toml(project.root(), project.pyproject_toml())?;

        // If sources/indexes are disabled we can just stop here
        let SourceStrategy::Enabled = source_strategy else {
            return Ok(Self {
                name: project.project_name().cloned(),
                dependency_groups: dependency_groups
                    .into_iter()
                    .map(|(name, group)| {
                        let requirements = group
                            .requirements
                            .into_iter()
                            .map(Requirement::from)
                            .collect();
                        (name, requirements)
                    })
                    .collect(),
            });
        };

        // Collect any `tool.uv.index` entries.
        let empty = vec![];
        let project_indexes = project
            .pyproject_toml()
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.index.as_deref())
            .unwrap_or(&empty);

        // Collect any `tool.uv.sources` and `tool.uv.dev_dependencies` from `pyproject.toml`.
        let empty = BTreeMap::default();
        let project_sources = project
            .pyproject_toml()
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.sources.as_ref())
            .map(ToolUvSources::inner)
            .unwrap_or(&empty);

        // Now that we've resolved the dependency groups, we can validate that each source references
        // a valid extra or group, if present.
        Self::validate_sources(project_sources, &dependency_groups)?;

        // Lower the dependency groups.
        let dependency_groups = dependency_groups
            .into_iter()
            .map(|(name, group)| {
                let requirements = group
                    .requirements
                    .into_iter()
                    .flat_map(|requirement| {
                        let requirement_name = requirement.name.clone();
                        let group = name.clone();
                        let extra = None;
                        LoweredRequirement::from_requirement(
                            requirement,
                            project.project_name(),
                            project.root(),
                            project_sources,
                            project_indexes,
                            extra,
                            Some(&group),
                            locations,
                            project.workspace(),
                            git_member,
                        )
                        .map(move |requirement| match requirement {
                            Ok(requirement) => Ok(requirement.into_inner()),
                            Err(err) => Err(MetadataError::GroupLoweringError(
                                group.clone(),
                                requirement_name.clone(),
                                Box::new(err),
                            )),
                        })
                    })
                    .collect::<Result<Box<_>, _>>()?;
                Ok::<(GroupName, Box<_>), MetadataError>((name, requirements))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;

        Ok(Self {
            name: project.project_name().cloned(),
            dependency_groups,
        })
    }

    /// Validate the sources.
    ///
    /// If a source is requested with `group`, ensure that the relevant dependency is
    /// present in the relevant `dependency-groups` section.
    fn validate_sources(
        sources: &BTreeMap<PackageName, Sources>,
        dependency_groups: &FlatDependencyGroups,
    ) -> Result<(), MetadataError> {
        for (name, sources) in sources {
            for source in sources.iter() {
                if let Some(group) = source.group() {
                    // If the group doesn't exist at all, error.
                    let Some(flat_group) = dependency_groups.get(group) else {
                        return Err(MetadataError::MissingSourceGroup(
                            name.clone(),
                            group.clone(),
                        ));
                    };

                    // If there is no such requirement with the group, error.
                    if !flat_group
                        .requirements
                        .iter()
                        .any(|requirement| requirement.name == *name)
                    {
                        return Err(MetadataError::IncompleteSourceGroup(
                            name.clone(),
                            group.clone(),
                        ));
                    }
                }
            }
        }

        Ok(())
    }
}
