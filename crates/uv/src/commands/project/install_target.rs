use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;
use std::str::FromStr;

use itertools::Either;
use rustc_hash::FxHashSet;

use uv_configuration::{Constraints, DependencyGroupsWithDefaults, ExtrasSpecification};
use uv_distribution_types::Index;
use uv_normalize::{ExtraName, PackageName};
use uv_pypi_types::{DependencyGroupSpecifier, LenientRequirement, VerbatimParsedUrl};
use uv_resolver::{Installable, Lock, Package};
use uv_scripts::Pep723Script;
use uv_workspace::Workspace;
use uv_workspace::pyproject::{Source, Sources, ToolUvSources};

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
    /// Multiple specific projects in a workspace.
    Projects {
        workspace: &'lock Workspace,
        names: &'lock [PackageName],
        lock: &'lock Lock,
    },
    /// An entire workspace.
    Workspace {
        workspace: &'lock Workspace,
        lock: &'lock Lock,
    },
    /// An entire workspace with a non-project root.
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
            Self::Projects { workspace, .. } => workspace.install_path(),
            Self::Workspace { workspace, .. } => workspace.install_path(),
            Self::NonProjectWorkspace { workspace, .. } => workspace.install_path(),
            Self::Script { script, .. } => script.path.parent().unwrap(),
        }
    }

    fn lock(&self) -> &'lock Lock {
        match self {
            Self::Project { lock, .. } => lock,
            Self::Projects { lock, .. } => lock,
            Self::Workspace { lock, .. } => lock,
            Self::NonProjectWorkspace { lock, .. } => lock,
            Self::Script { lock, .. } => lock,
        }
    }

    #[allow(refining_impl_trait)]
    fn roots(&self) -> Box<dyn Iterator<Item = &PackageName> + '_> {
        match self {
            Self::Project { name, .. } => Box::new(std::iter::once(*name)),
            Self::Projects { names, .. } => Box::new(names.iter()),
            Self::NonProjectWorkspace { lock, .. } => Box::new(lock.members().iter()),
            Self::Workspace { lock, .. } => {
                // Identify the workspace members.
                //
                // The members are encoded directly in the lockfile, unless the workspace contains a
                // single member at the root, in which case, we identify it by its source.
                if lock.members().is_empty() {
                    Box::new(lock.root().into_iter().map(Package::name))
                } else {
                    Box::new(lock.members().iter())
                }
            }
            Self::Script { .. } => Box::new(std::iter::empty()),
        }
    }

    fn project_name(&self) -> Option<&PackageName> {
        match self {
            Self::Project { name, .. } => Some(name),
            Self::Projects { .. } => None,
            Self::Workspace { lock, .. } => {
                // If the workspace contains a single member at the root, it will be omitted from
                // the list of workspace members encoded in the lockfile. In that case, identify
                // the root project by its source so that install options (e.g.,
                // `--no-emit-workspace`) can filter it correctly.
                if lock.members().is_empty() {
                    lock.root().map(Package::name)
                } else {
                    None
                }
            }
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
            | Self::Projects { workspace, .. }
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
            | Self::Projects { workspace, .. }
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
            | Self::Projects { workspace, .. }
            | Self::Workspace { workspace, .. }
            | Self::NonProjectWorkspace { workspace, .. } => {
                Either::Left(
                    // Iterate over the non-member requirements in the workspace.
                    workspace
                        .requirements()
                        .into_iter()
                        .map(Cow::Owned)
                        .chain(
                            workspace
                                .workspace_dependency_groups()
                                .ok()
                                .into_iter()
                                .flat_map(|dependency_groups| {
                                    dependency_groups
                                        .into_values()
                                        .flat_map(|group| group.requirements)
                                        .map(Cow::Owned)
                                }),
                        )
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

    pub(crate) fn build_constraints(&self) -> Constraints {
        self.lock().build_constraints(self.install_path())
    }

    /// Validate the extras requested by the [`ExtrasSpecification`].
    #[expect(clippy::result_large_err)]
    pub(crate) fn validate_extras(self, extras: &ExtrasSpecification) -> Result<(), ProjectError> {
        if extras.is_empty() {
            return Ok(());
        }
        match self {
            Self::Project { lock, .. }
            | Self::Projects { lock, .. }
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

                for extra in extras.explicit_names() {
                    if !known_extras.contains(extra) {
                        return match self {
                            Self::Project { .. } => {
                                Err(ProjectError::MissingExtraProject(extra.clone()))
                            }
                            Self::Projects { .. } => {
                                Err(ProjectError::MissingExtraProjects(extra.clone()))
                            }
                            _ => Err(ProjectError::MissingExtraProjects(extra.clone())),
                        };
                    }
                }
            }
            Self::Script { .. } => {
                // We shouldn't get here if the list is empty so we can assume it isn't
                let extra = extras
                    .explicit_names()
                    .next()
                    .expect("non-empty extras")
                    .clone();
                return Err(ProjectError::MissingExtraScript(extra));
            }
        }

        Ok(())
    }

    /// Validate the dependency groups requested by the [`DependencyGroupSpecifier`].
    #[expect(clippy::result_large_err)]
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
                    .chain(
                        workspace
                            .workspace_dependency_groups()
                            .ok()
                            .into_iter()
                            .flat_map(|dependency_groups| {
                                dependency_groups.into_keys().map(Cow::Owned)
                            }),
                    )
                    .collect::<FxHashSet<_>>();

                for group in groups.explicit_names() {
                    if !known_groups.contains(group) {
                        return Err(ProjectError::MissingGroupProjects(group.clone()));
                    }
                }
            }
            Self::Project { lock, .. } | Self::Projects { lock, .. } => {
                let roots = self.roots().collect::<FxHashSet<_>>();
                let member_packages: Vec<&Package> = lock
                    .packages()
                    .iter()
                    .filter(|package| roots.contains(package.name()))
                    .collect();

                // Extract the dependency groups defined in the relevant member(s).
                let known_groups = member_packages
                    .iter()
                    .flat_map(|package| package.dependency_groups().keys())
                    .collect::<FxHashSet<_>>();

                for group in groups.explicit_names() {
                    if !known_groups.contains(group) {
                        return match self {
                            Self::Project { .. } => {
                                Err(ProjectError::MissingGroupProject(group.clone()))
                            }
                            Self::Projects { .. } => {
                                Err(ProjectError::MissingGroupProjects(group.clone()))
                            }
                            _ => unreachable!(),
                        };
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

    /// Returns the names of all packages in the workspace that will be installed.
    ///
    /// Note this only includes workspace members.
    pub(crate) fn packages(
        &self,
        extras: &ExtrasSpecification,
        groups: &DependencyGroupsWithDefaults,
    ) -> BTreeSet<&PackageName> {
        match self {
            Self::Project { lock, .. } | Self::Projects { lock, .. } => {
                let roots = self.roots().collect::<FxHashSet<_>>();

                // Collect the packages by name for efficient lookup.
                let packages = lock
                    .packages()
                    .iter()
                    .map(|package| (package.name(), package))
                    .collect::<BTreeMap<_, _>>();

                // We'll include all specified projects
                let mut required_members = BTreeSet::new();
                for name in &roots {
                    required_members.insert(*name);
                }

                // Find all workspace member dependencies recursively for all specified packages
                let mut queue: VecDeque<(&PackageName, Option<&ExtraName>)> = VecDeque::new();
                let mut seen: FxHashSet<(&PackageName, Option<&ExtraName>)> = FxHashSet::default();

                for name in roots {
                    let Some(root_package) = packages.get(name) else {
                        continue;
                    };

                    if groups.prod() {
                        // Add the root package
                        if seen.insert((name, None)) {
                            queue.push_back((name, None));
                        }

                        // Add explicitly activated extras for the root package
                        for extra in extras.extra_names(root_package.optional_dependencies().keys())
                        {
                            if seen.insert((name, Some(extra))) {
                                queue.push_back((name, Some(extra)));
                            }
                        }
                    }

                    // Add activated dependency groups for the root package
                    for (group_name, dependencies) in root_package.resolved_dependency_groups() {
                        if !groups.contains(group_name) {
                            continue;
                        }
                        for dependency in dependencies {
                            let dep_name = dependency.package_name();
                            if seen.insert((dep_name, None)) {
                                queue.push_back((dep_name, None));
                            }
                            for extra in dependency.extra() {
                                if seen.insert((dep_name, Some(extra))) {
                                    queue.push_back((dep_name, Some(extra)));
                                }
                            }
                        }
                    }
                }

                while let Some((package_name, extra)) = queue.pop_front() {
                    if lock.members().contains(package_name) {
                        required_members.insert(package_name);
                    }

                    let Some(package) = packages.get(package_name) else {
                        continue;
                    };

                    let Some(dependencies) = extra
                        .map(|extra_name| {
                            package
                                .optional_dependencies()
                                .get(extra_name)
                                .map(Vec::as_slice)
                        })
                        .unwrap_or(Some(package.dependencies()))
                    else {
                        continue;
                    };

                    for dependency in dependencies {
                        let name = dependency.package_name();
                        if seen.insert((name, None)) {
                            queue.push_back((name, None));
                        }
                        for extra in dependency.extra() {
                            if seen.insert((name, Some(extra))) {
                                queue.push_back((name, Some(extra)));
                            }
                        }
                    }
                }

                required_members
            }
            Self::Workspace { lock, .. } | Self::NonProjectWorkspace { lock, .. } => {
                // Return all workspace members
                lock.members().iter().collect()
            }
            Self::Script { .. } => {
                // Scripts don't have workspace members
                BTreeSet::new()
            }
        }
    }
}
