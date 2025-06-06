use std::collections::BTreeMap;
use std::path::Path;

use uv_configuration::SourceStrategy;
use uv_distribution_types::{IndexLocations, Requirement};
use uv_normalize::{DEV_DEPENDENCIES, GroupName, PackageName};
use uv_workspace::dependency_groups::FlatDependencyGroups;
use uv_workspace::pyproject::{Sources, ToolUvSources};
use uv_workspace::{DiscoveryOptions, MemberDiscovery, VirtualProject, WorkspaceCache};

use crate::metadata::{GitWorkspaceMember, LoweredRequirement, MetadataError};

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
        let project = VirtualProject::discover_defaulted(pyproject_path, &discovery, cache).await?;

        // Collect any `tool.uv.index` entries.
        let empty = vec![];
        let project_indexes = match source_strategy {
            SourceStrategy::Enabled => project
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
            SourceStrategy::Enabled => project
                .pyproject_toml()
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.sources.as_ref())
                .map(ToolUvSources::inner)
                .unwrap_or(&empty),
            SourceStrategy::Disabled => &empty,
        };

        // Collect the dependency groups.
        let dependency_groups = {
            // First, collect `tool.uv.dev_dependencies`
            let dev_dependencies = project
                .pyproject_toml()
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.dev_dependencies.as_ref());

            // Then, collect `dependency-groups`
            let dependency_groups = project
                .pyproject_toml()
                .dependency_groups
                .iter()
                .flatten()
                .collect::<BTreeMap<_, _>>();

            // Flatten the dependency groups.
            let mut dependency_groups =
                FlatDependencyGroups::from_dependency_groups(&dependency_groups)
                    .map_err(|err| err.with_dev_dependencies(dev_dependencies))?;

            // Add the `dev` group, if `dev-dependencies` is defined.
            if let Some(dev_dependencies) = dev_dependencies {
                dependency_groups
                    .entry(DEV_DEPENDENCIES.clone())
                    .or_insert_with(Vec::new)
                    .extend(dev_dependencies.clone());
            }

            dependency_groups
        };

        // Now that we've resolved the dependency groups, we can validate that each source references
        // a valid extra or group, if present.
        Self::validate_sources(project_sources, &dependency_groups)?;

        // Lower the dependency groups.
        let dependency_groups = dependency_groups
            .into_iter()
            .map(|(name, requirements)| {
                let requirements = match source_strategy {
                    SourceStrategy::Enabled => requirements
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
                            .map(
                                move |requirement| match requirement {
                                    Ok(requirement) => Ok(requirement.into_inner()),
                                    Err(err) => Err(MetadataError::GroupLoweringError(
                                        group.clone(),
                                        requirement_name.clone(),
                                        Box::new(err),
                                    )),
                                },
                            )
                        })
                        .collect::<Result<Box<_>, _>>(),
                    SourceStrategy::Disabled => {
                        Ok(requirements.into_iter().map(Requirement::from).collect())
                    }
                }?;
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
                    let Some(dependencies) = dependency_groups.get(group) else {
                        return Err(MetadataError::MissingSourceGroup(
                            name.clone(),
                            group.clone(),
                        ));
                    };

                    // If there is no such requirement with the group, error.
                    if !dependencies
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
