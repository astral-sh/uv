use std::borrow::Cow;
use std::path::Path;
use std::str::FromStr;

use itertools::Either;
use rustc_hash::FxHashSet;

use uv_configuration::{DependencyGroupsWithDefaults, ExtrasSpecification};
use uv_distribution_types::Index;
use uv_normalize::PackageName;
use uv_pypi_types::{DependencyGroupSpecifier, LenientRequirement, VerbatimParsedUrl};
use uv_resolver::{Installable, Lock, Package};
use uv_scripts::Pep723Script;
use uv_workspace::pyproject::{Source, Sources, ToolUvSources};
use uv_workspace::Workspace;

use crate::commands::project::ProjectError;

/// A target that can be installed from a lockfile.
#[derive(Debug, Copy, Clone)]
pub(crate) enum InstallTarget<'lock> {
    /// A project (which could be a workspace root or member).
    Project {
        workspace: &'lock Workspace,
        name: &'lock PackageName,
        lock: &'lock Lock,
    },
    /// An entire workspace.
    Workspace {
        workspace: &'lock Workspace,
        lock: &'lock Lock,
    },
    /// An entire workspace with a (legacy) non-project root.
    NonProjectWorkspace {
        workspace: &'lock Workspace,
        lock: &'lock Lock,
    },
    /// A PEP 723 script.
    Script {
        script: &'lock Pep723Script,
        lock: &'lock Lock,
    },
}

impl<'lock> Installable<'lock> for InstallTarget<'lock> {
    fn install_path(&self) -> &'lock Path {
        match self {
            Self::Project { workspace, .. } => workspace.install_path(),
            Self::Workspace { workspace, .. } => workspace.install_path(),
            Self::NonProjectWorkspace { workspace, .. } => workspace.install_path(),
            Self::Script { script, .. } => script.path.parent().unwrap(),
        }
    }

    fn lock(&self) -> &'lock Lock {
        match self {
            Self::Project { lock, .. } => lock,
            Self::Workspace { lock, .. } => lock,
            Self::NonProjectWorkspace { lock, .. } => lock,
            Self::Script { lock, .. } => lock,
        }
    }

    fn roots(&self) -> impl Iterator<Item = &PackageName> {
        match self {
            Self::Project { name, .. } => Either::Left(Either::Left(std::iter::once(*name))),
            Self::NonProjectWorkspace { lock, .. } => {
                Either::Left(Either::Right(lock.members().iter()))
            }
            Self::Workspace { lock, .. } => {
                // Identify the workspace members.
                //
                // The members are encoded directly in the lockfile, unless the workspace contains a
                // single member at the root, in which case, we identify it by its source.
                if lock.members().is_empty() {
                    Either::Right(Either::Left(lock.root().into_iter().map(Package::name)))
                } else {
                    Either::Left(Either::Right(lock.members().iter()))
                }
            }
            Self::Script { .. } => Either::Right(Either::Right(std::iter::empty())),
        }
    }

    fn project_name(&self) -> Option<&PackageName> {
        match self {
            Self::Project { name, .. } => Some(name),
            Self::Workspace { .. } => None,
            Self::NonProjectWorkspace { .. } => None,
            Self::Script { .. } => None,
        }
    }
}

impl<'lock> InstallTarget<'lock> {
    /// Return an iterator over the [`Index`] definitions in the target.
    pub(crate) fn indexes(self) -> impl Iterator<Item = &'lock Index> {
        match self {
            Self::Project { workspace, .. }
            | Self::Workspace { workspace, .. }
            | Self::NonProjectWorkspace { workspace, .. } => {
                Either::Left(workspace.indexes().iter().chain(
                    workspace.packages().values().flat_map(|member| {
                        member
                            .pyproject_toml()
                            .tool
                            .as_ref()
                            .and_then(|tool| tool.uv.as_ref())
                            .and_then(|uv| uv.index.as_ref())
                            .into_iter()
                            .flatten()
                    }),
                ))
            }
            Self::Script { script, .. } => Either::Right(
                script
                    .metadata
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.uv.as_ref())
                    .and_then(|uv| uv.top_level.index.as_deref())
                    .into_iter()
                    .flatten(),
            ),
        }
    }

    /// Return an iterator over all [`Sources`] defined by the target.
    pub(crate) fn sources(&self) -> impl Iterator<Item = &Source> {
        match self {
            Self::Project { workspace, .. }
            | Self::Workspace { workspace, .. }
            | Self::NonProjectWorkspace { workspace, .. } => {
                Either::Left(workspace.sources().values().flat_map(Sources::iter).chain(
                    workspace.packages().values().flat_map(|member| {
                        member
                            .pyproject_toml()
                            .tool
                            .as_ref()
                            .and_then(|tool| tool.uv.as_ref())
                            .and_then(|uv| uv.sources.as_ref())
                            .map(ToolUvSources::inner)
                            .into_iter()
                            .flat_map(|sources| sources.values().flat_map(Sources::iter))
                    }),
                ))
            }
            Self::Script { script, .. } => {
                Either::Right(script.sources().values().flat_map(Sources::iter))
            }
        }
    }

    /// Return an iterator over all requirements defined by the target.
    pub(crate) fn requirements(
        &self,
    ) -> impl Iterator<Item = Cow<'lock, uv_pep508::Requirement<VerbatimParsedUrl>>> {
        match self {
            Self::Project { workspace, .. }
            | Self::Workspace { workspace, .. }
            | Self::NonProjectWorkspace { workspace, .. } => {
                Either::Left(
                    // Iterate over the non-member requirements in the workspace.
                    workspace
                        .requirements()
                        .into_iter()
                        .map(Cow::Owned)
                        .chain(workspace.dependency_groups().ok().into_iter().flat_map(
                            |dependency_groups| {
                                dependency_groups.into_values().flatten().map(Cow::Owned)
                            },
                        ))
                        .chain(workspace.packages().values().flat_map(|member| {
                            // Iterate over all dependencies in each member.
                            let dependencies = member
                                .pyproject_toml()
                                .project
                                .as_ref()
                                .and_then(|project| project.dependencies.as_ref())
                                .into_iter()
                                .flatten();
                            let optional_dependencies = member
                                .pyproject_toml()
                                .project
                                .as_ref()
                                .and_then(|project| project.optional_dependencies.as_ref())
                                .into_iter()
                                .flat_map(|optional| optional.values())
                                .flatten();
                            let dependency_groups = member
                                .pyproject_toml()
                                .dependency_groups
                                .as_ref()
                                .into_iter()
                                .flatten()
                                .flat_map(|(_, dependencies)| {
                                    dependencies.iter().filter_map(|specifier| {
                                        if let DependencyGroupSpecifier::Requirement(requirement) =
                                            specifier
                                        {
                                            Some(requirement)
                                        } else {
                                            None
                                        }
                                    })
                                });
                            let dev_dependencies = member
                                .pyproject_toml()
                                .tool
                                .as_ref()
                                .and_then(|tool| tool.uv.as_ref())
                                .and_then(|uv| uv.dev_dependencies.as_ref())
                                .into_iter()
                                .flatten();
                            dependencies
                                .chain(optional_dependencies)
                                .chain(dependency_groups)
                                .filter_map(|requires_dist| {
                                    LenientRequirement::<VerbatimParsedUrl>::from_str(requires_dist)
                                        .map(uv_pep508::Requirement::from)
                                        .map(Cow::Owned)
                                        .ok()
                                })
                                .chain(dev_dependencies.map(Cow::Borrowed))
                        })),
                )
            }
            Self::Script { script, .. } => Either::Right(
                script
                    .metadata
                    .dependencies
                    .iter()
                    .flatten()
                    .map(Cow::Borrowed),
            ),
        }
    }

    /// Validate the extras requested by the [`ExtrasSpecification`].
    #[allow(clippy::result_large_err)]
    pub(crate) fn validate_extras(self, extras: &ExtrasSpecification) -> Result<(), ProjectError> {
        let extras = match extras {
            ExtrasSpecification::Some(extras) => {
                if extras.is_empty() {
                    return Ok(());
                }
                Either::Left(extras.iter())
            }
            ExtrasSpecification::Exclude(extras) => {
                if extras.is_empty() {
                    return Ok(());
                }
                Either::Right(extras.iter())
            }
            _ => return Ok(()),
        };

        match self {
            Self::Project { lock, .. }
            | Self::Workspace { lock, .. }
            | Self::NonProjectWorkspace { lock, .. } => {
                if !lock.supports_provides_extra() {
                    return Ok(());
                }

                let roots = self.roots().collect::<FxHashSet<_>>();
                let member_packages: Vec<&Package> = lock
                    .packages()
                    .iter()
                    .filter(|package| roots.contains(package.name()))
                    .collect();

                // Collect all known extras from the member packages.
                let known_extras = member_packages
                    .iter()
                    .flat_map(|package| package.provides_extras().iter())
                    .collect::<FxHashSet<_>>();

                for extra in extras {
                    if !known_extras.contains(extra) {
                        return match self {
                            Self::Project { .. } => {
                                Err(ProjectError::MissingExtraProject(extra.clone()))
                            }
                            _ => Err(ProjectError::MissingExtraWorkspace(extra.clone())),
                        };
                    }
                }
            }
            Self::Script { .. } => {
                // We shouldn't get here if the list is empty so we can assume it isn't
                let extra = extras.into_iter().next().expect("non-empty extras").clone();
                return Err(ProjectError::MissingExtraScript(extra));
            }
        }

        Ok(())
    }

    /// Validate the dependency groups requested by the [`DependencyGroupSpecifier`].
    #[allow(clippy::result_large_err)]
    pub(crate) fn validate_groups(
        self,
        groups: &DependencyGroupsWithDefaults,
    ) -> Result<(), ProjectError> {
        // If no groups were specified, short-circuit.
        if groups.explicit_names().next().is_none() {
            return Ok(());
        }

        match self {
            Self::Workspace { lock, workspace } | Self::NonProjectWorkspace { lock, workspace } => {
                let roots = self.roots().collect::<FxHashSet<_>>();
                let member_packages: Vec<&Package> = lock
                    .packages()
                    .iter()
                    .filter(|package| roots.contains(package.name()))
                    .collect();

                // Extract the dependency groups that are exclusive to the workspace root.
                let known_groups = member_packages
                    .iter()
                    .flat_map(|package| package.dependency_groups().keys().map(Cow::Borrowed))
                    .chain(workspace.dependency_groups().ok().into_iter().flat_map(
                        |dependency_groups| dependency_groups.into_keys().map(Cow::Owned),
                    ))
                    .collect::<FxHashSet<_>>();

                for group in groups.explicit_names() {
                    if !known_groups.contains(group) {
                        return Err(ProjectError::MissingGroupWorkspace(group.clone()));
                    }
                }
            }
            Self::Project { lock, .. } => {
                let roots = self.roots().collect::<FxHashSet<_>>();
                let member_packages: Vec<&Package> = lock
                    .packages()
                    .iter()
                    .filter(|package| roots.contains(package.name()))
                    .collect();

                // Extract the dependency groups defined in the relevant member.
                let known_groups = member_packages
                    .iter()
                    .flat_map(|package| package.dependency_groups().keys())
                    .collect::<FxHashSet<_>>();

                for group in groups.explicit_names() {
                    if !known_groups.contains(group) {
                        return Err(ProjectError::MissingGroupProject(group.clone()));
                    }
                }
            }
            Self::Script { .. } => {
                if let Some(group) = groups.explicit_names().next() {
                    return Err(ProjectError::MissingGroupScript(group.clone()));
                }
            }
        }

        Ok(())
    }
}
