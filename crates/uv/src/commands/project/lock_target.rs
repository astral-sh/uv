use itertools::Either;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tracing::{debug, info_span};

use uv_auth::CredentialsCache;
use uv_configuration::{DependencyGroupsWithDefaults, NoSources};
use uv_distribution::LoweredRequirement;
use uv_distribution_types::{
    Index, IndexLocations, Requirement, RequirementSource, RequiresPython,
};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep508::{MarkerTree, RequirementOrigin};
use uv_pypi_types::{Conflicts, LenientRequirement, SupportedEnvironments, VerbatimParsedUrl};
use uv_resolver::{Lock, LockVersion, VERSION};
use uv_scripts::Pep723Script;
use uv_workspace::dependency_groups::{
    DependencyGroupError, FlatDependencyGroup, FlatDependencyGroups,
};
use uv_workspace::{Editability, Workspace, WorkspaceMember};

use crate::commands::project::{ProjectError, find_requires_python};

/// A target that can be resolved into a lockfile.
#[derive(Debug, Copy, Clone)]
pub(crate) enum LockTarget<'lock> {
    Workspace(&'lock Workspace),
    Script(&'lock Pep723Script),
}

impl<'lock> From<&'lock Workspace> for LockTarget<'lock> {
    fn from(workspace: &'lock Workspace) -> Self {
        Self::Workspace(workspace)
    }
}

impl<'lock> From<&'lock Pep723Script> for LockTarget<'lock> {
    fn from(script: &'lock Pep723Script) -> Self {
        LockTarget::Script(script)
    }
}

#[derive(Debug, Default)]
struct DirectDependencyContexts {
    any: BTreeSet<PackageName>,
    extras: BTreeSet<(PackageName, ExtraName)>,
    groups: BTreeSet<(PackageName, GroupName)>,
}

impl DirectDependencyContexts {
    fn insert_any(&mut self, requirement: &uv_pep508::Requirement<VerbatimParsedUrl>) {
        self.any.insert(requirement.name.clone());
    }

    fn insert_extra(
        &mut self,
        requirement: &uv_pep508::Requirement<VerbatimParsedUrl>,
        extra: &ExtraName,
    ) {
        self.extras
            .insert((requirement.name.clone(), extra.clone()));
    }

    fn insert_group(
        &mut self,
        requirement: &uv_pep508::Requirement<VerbatimParsedUrl>,
        group: &GroupName,
    ) {
        self.groups
            .insert((requirement.name.clone(), group.clone()));
    }

    /// Populate direct dependency contexts from a `pyproject.toml`'s project table and dependency
    /// groups.
    fn collect_from_pyproject_toml(
        &mut self,
        path: &Path,
        pyproject_toml: &uv_workspace::pyproject::PyProjectToml,
    ) -> Result<(), DependencyGroupError> {
        if let Some(project) = &pyproject_toml.project {
            if let Some(dependencies) = &project.dependencies {
                for requirement in dependencies.iter().filter_map(|dependency| {
                    LenientRequirement::<VerbatimParsedUrl>::from_str(dependency)
                        .inspect_err(|err| {
                            debug!("Failed to parse dependency `{dependency}`: {err}");
                        })
                        .ok()
                }) {
                    let requirement: uv_pep508::Requirement<VerbatimParsedUrl> = requirement.into();
                    self.insert_any(&requirement);
                }
            }

            if let Some(optional_dependencies) = &project.optional_dependencies {
                for (extra, dependencies) in optional_dependencies {
                    for requirement in dependencies.iter().filter_map(|dependency| {
                        LenientRequirement::<VerbatimParsedUrl>::from_str(dependency)
                            .inspect_err(|err| {
                                debug!("Failed to parse dependency `{dependency}`: {err}");
                            })
                            .ok()
                    }) {
                        let requirement: uv_pep508::Requirement<VerbatimParsedUrl> =
                            requirement.into();
                        self.insert_any(&requirement);
                        self.insert_extra(&requirement, extra);
                    }
                }
            }
        }

        let dependency_groups = FlatDependencyGroups::from_pyproject_toml(path, pyproject_toml)?;
        for (group, flat_group) in dependency_groups {
            for requirement in &flat_group.requirements {
                self.insert_any(requirement);
                self.insert_group(requirement, &group);
            }
        }

        Ok(())
    }

    fn is_direct(
        &self,
        package: &PackageName,
        extra: Option<&ExtraName>,
        group: Option<&GroupName>,
    ) -> bool {
        match (extra, group) {
            (Some(extra), None) => self.extras.contains(&(package.clone(), extra.clone())),
            (None, Some(group)) => self.groups.contains(&(package.clone(), group.clone())),
            (None, None) => self.any.contains(package),
            // Sources have either `extra` or `group`, never both, so this case is unreachable
            // via `source_scopes`.
            (Some(_), Some(_)) => false,
        }
    }
}

fn is_noop_constraint(requirement: &Requirement) -> bool {
    matches!(
        &requirement.source,
        RequirementSource::Registry { specifier, index, .. }
            if specifier.is_empty() && index.is_none()
    )
}

fn source_scopes(
    sources: &uv_workspace::pyproject::Sources,
) -> BTreeSet<(Option<ExtraName>, Option<GroupName>)> {
    sources
        .iter()
        .map(|source| (source.extra().cloned(), source.group().cloned()))
        .collect()
}

fn source_scope(
    source: &uv_workspace::pyproject::Source,
) -> (Option<ExtraName>, Option<GroupName>) {
    (source.extra().cloned(), source.group().cloned())
}

fn source_with_marker(
    source: &uv_workspace::pyproject::Source,
    marker: MarkerTree,
) -> uv_workspace::pyproject::Source {
    match source {
        uv_workspace::pyproject::Source::Git {
            git,
            subdirectory,
            rev,
            tag,
            branch,
            lfs,
            extra,
            group,
            ..
        } => uv_workspace::pyproject::Source::Git {
            git: git.clone(),
            subdirectory: subdirectory.clone(),
            rev: rev.clone(),
            tag: tag.clone(),
            branch: branch.clone(),
            lfs: *lfs,
            marker,
            extra: extra.clone(),
            group: group.clone(),
        },
        uv_workspace::pyproject::Source::Url {
            url,
            subdirectory,
            extra,
            group,
            ..
        } => uv_workspace::pyproject::Source::Url {
            url: url.clone(),
            subdirectory: subdirectory.clone(),
            marker,
            extra: extra.clone(),
            group: group.clone(),
        },
        uv_workspace::pyproject::Source::Path {
            path,
            editable,
            package,
            extra,
            group,
            ..
        } => uv_workspace::pyproject::Source::Path {
            path: path.clone(),
            editable: *editable,
            package: *package,
            marker,
            extra: extra.clone(),
            group: group.clone(),
        },
        uv_workspace::pyproject::Source::Registry {
            index,
            extra,
            group,
            ..
        } => uv_workspace::pyproject::Source::Registry {
            index: index.clone(),
            marker,
            extra: extra.clone(),
            group: group.clone(),
        },
        uv_workspace::pyproject::Source::Workspace {
            workspace,
            editable,
            extra,
            group,
            ..
        } => uv_workspace::pyproject::Source::Workspace {
            workspace: *workspace,
            editable: *editable,
            marker,
            extra: extra.clone(),
            group: group.clone(),
        },
    }
}

fn filter_workspace_root_transitive_sources(
    workspace: &Workspace,
) -> BTreeMap<PackageName, uv_workspace::pyproject::Sources> {
    workspace
        .sources()
        .iter()
        .filter_map(|(package, sources)| {
            let filtered = sources
                .iter()
                .filter_map(|source| {
                    let mut marker = source.marker();
                    for member_source in workspace
                        .packages()
                        .values()
                        .filter(|member| member.root() != workspace.install_path())
                        .filter_map(|member| {
                            member
                                .pyproject_toml()
                                .tool
                                .as_ref()
                                .and_then(|tool| tool.uv.as_ref())
                                .and_then(|uv| uv.sources.as_ref())
                                .and_then(|sources| sources.inner().get(package))
                        })
                        .flat_map(uv_workspace::pyproject::Sources::iter)
                        .filter(|member_source| source_scope(member_source) == source_scope(source))
                    {
                        if marker.is_disjoint(member_source.marker()) {
                            continue;
                        }
                        marker.and(member_source.marker().negate());
                        if marker == MarkerTree::FALSE {
                            break;
                        }
                    }

                    (marker != MarkerTree::FALSE).then(|| source_with_marker(source, marker))
                })
                .collect::<uv_workspace::pyproject::Sources>();
            (!filtered.is_empty()).then(|| (package.clone(), filtered))
        })
        .collect()
}

struct WorkspaceProjectTransitiveSources<'a> {
    project_name: Option<PackageName>,
    project_path: PathBuf,
    project_root: &'a Path,
    project_sources: &'a BTreeMap<PackageName, uv_workspace::pyproject::Sources>,
    inherited_sources: &'a BTreeMap<PackageName, uv_workspace::pyproject::Sources>,
    project_indexes: &'a [Index],
    direct_contexts: &'a DirectDependencyContexts,
}

fn lower_workspace_project_transitive_source_constraints(
    project: WorkspaceProjectTransitiveSources<'_>,
    workspace: &Workspace,
    locations: &IndexLocations,
    source_strategy: &NoSources,
    credentials_cache: &CredentialsCache,
) -> Result<Vec<Requirement>, uv_distribution::MetadataError> {
    let WorkspaceProjectTransitiveSources {
        project_name,
        project_path,
        project_root,
        project_sources,
        inherited_sources,
        project_indexes,
        direct_contexts,
    } = project;

    let package_names = project_sources
        .keys()
        .chain(inherited_sources.keys())
        .cloned()
        .collect::<BTreeSet<_>>();

    let mut lowered_constraints = Vec::new();
    for package in package_names {
        if source_strategy.for_package(&package) {
            continue;
        }

        let Some(package_sources) = project_sources
            .get(&package)
            .or_else(|| inherited_sources.get(&package))
        else {
            continue;
        };

        for (extra, group) in source_scopes(package_sources) {
            if direct_contexts.is_direct(&package, extra.as_ref(), group.as_ref()) {
                continue;
            }

            let origin = if let Some(extra) = &extra {
                RequirementOrigin::Extra(project_path.clone(), project_name.clone(), extra.clone())
            } else if let Some(group) = &group {
                RequirementOrigin::Group(project_path.clone(), project_name.clone(), group.clone())
            } else if let Some(project_name) = &project_name {
                RequirementOrigin::Project(project_path.clone(), project_name.clone())
            } else {
                RequirementOrigin::Workspace
            };

            let requirement_name = package.clone();
            let mut lowered = LoweredRequirement::from_requirement(
                uv_pep508::Requirement {
                    name: package.clone(),
                    extras: Box::default(),
                    marker: MarkerTree::TRUE,
                    version_or_url: None,
                    origin: Some(origin),
                },
                project_name.as_ref(),
                project_root,
                project_sources,
                project_indexes,
                extra.as_ref(),
                group.as_ref(),
                locations,
                workspace,
                None,
                credentials_cache,
            )
            .map(move |requirement| match requirement {
                Ok(requirement) => Ok(requirement.into_inner()),
                Err(err) => Err(uv_distribution::MetadataError::LoweringError(
                    requirement_name.clone(),
                    Box::new(err),
                )),
            })
            .collect::<Result<Vec<_>, _>>()?;
            lowered_constraints.append(&mut lowered);
        }
    }

    Ok(lowered_constraints
        .into_iter()
        .filter(|requirement| !is_noop_constraint(requirement))
        .collect())
}

impl<'lock> LockTarget<'lock> {
    /// Return the set of requirements that are attached to the target directly, as opposed to being
    /// attached to any members within the target.
    pub(crate) fn requirements(self) -> Vec<uv_pep508::Requirement<VerbatimParsedUrl>> {
        match self {
            Self::Workspace(workspace) => workspace.requirements(),
            Self::Script(script) => script.metadata.dependencies.clone().unwrap_or_default(),
        }
    }

    /// Returns the set of overrides for the [`LockTarget`].
    pub(crate) fn overrides(self) -> Vec<uv_pep508::Requirement<VerbatimParsedUrl>> {
        match self {
            Self::Workspace(workspace) => workspace.overrides(),
            Self::Script(script) => script
                .metadata
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.override_dependencies.as_ref())
                .into_iter()
                .flatten()
                .cloned()
                .collect(),
        }
    }

    /// Returns the set of dependency exclusions for the [`LockTarget`].
    pub(crate) fn exclude_dependencies(self) -> Vec<uv_normalize::PackageName> {
        match self {
            Self::Workspace(workspace) => workspace.exclude_dependencies(),
            Self::Script(script) => script
                .metadata
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.exclude_dependencies.as_ref())
                .into_iter()
                .flatten()
                .cloned()
                .collect(),
        }
    }

    /// Returns the set of constraints for the [`LockTarget`].
    pub(crate) fn constraints(self) -> Vec<uv_pep508::Requirement<VerbatimParsedUrl>> {
        match self {
            Self::Workspace(workspace) => workspace.constraints(),
            Self::Script(script) => script
                .metadata
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.constraint_dependencies.as_ref())
                .into_iter()
                .flatten()
                .cloned()
                .collect(),
        }
    }

    /// Returns the set of build constraints for the [`LockTarget`].
    pub(crate) fn build_constraints(self) -> Vec<uv_pep508::Requirement<VerbatimParsedUrl>> {
        match self {
            Self::Workspace(workspace) => workspace.build_constraints(),
            Self::Script(script) => script
                .metadata
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.build_constraint_dependencies.as_ref())
                .into_iter()
                .flatten()
                .cloned()
                .collect(),
        }
    }

    /// Return the dependency groups that are attached to the target directly, as opposed to being
    /// attached to any members within the target.
    pub(crate) fn dependency_groups(
        self,
    ) -> Result<BTreeMap<GroupName, FlatDependencyGroup>, DependencyGroupError> {
        match self {
            Self::Workspace(workspace) => workspace.workspace_dependency_groups(),
            Self::Script(_) => Ok(BTreeMap::new()),
        }
    }

    /// Returns the set of all members within the target.
    pub(crate) fn members_requirements(self) -> impl Iterator<Item = Requirement> + 'lock {
        match self {
            Self::Workspace(workspace) => Either::Left(workspace.members_requirements()),
            Self::Script(_) => Either::Right(std::iter::empty()),
        }
    }

    /// Returns the set of all dependency groups within the target.
    pub(crate) fn group_requirements(self) -> impl Iterator<Item = Requirement> + 'lock {
        match self {
            Self::Workspace(workspace) => Either::Left(workspace.group_requirements()),
            Self::Script(_) => Either::Right(std::iter::empty()),
        }
    }

    /// Return the list of members to include in the [`Lock`].
    pub(crate) fn members(self) -> Vec<PackageName> {
        match self {
            Self::Workspace(workspace) => {
                let mut members = workspace.packages().keys().cloned().collect::<Vec<_>>();
                members.sort();

                // If this is a non-virtual project with a single member, we can omit it from the lockfile.
                // If any members are added or removed, it will inherently mismatch. If the member is
                // renamed, it will also mismatch.
                if members.len() == 1 && !workspace.is_non_project() {
                    members.clear();
                }

                members
            }
            Self::Script(_) => Vec::new(),
        }
    }

    /// Return the list of packages.
    pub(crate) fn packages(self) -> &'lock BTreeMap<PackageName, WorkspaceMember> {
        match self {
            Self::Workspace(workspace) => workspace.packages(),
            Self::Script(_) => {
                static EMPTY: BTreeMap<PackageName, WorkspaceMember> = BTreeMap::new();
                &EMPTY
            }
        }
    }

    /// Return the set of required workspace members, i.e., those that are required by other
    /// members.
    pub(crate) fn required_members(self) -> &'lock BTreeMap<PackageName, Editability> {
        match self {
            Self::Workspace(workspace) => workspace.required_members(),
            Self::Script(_) => {
                static EMPTY: BTreeMap<PackageName, Editability> = BTreeMap::new();
                &EMPTY
            }
        }
    }

    /// Returns the set of supported environments for the [`LockTarget`].
    pub(crate) fn environments(self) -> Option<&'lock SupportedEnvironments> {
        match self {
            Self::Workspace(workspace) => workspace.environments(),
            Self::Script(_) => {
                // TODO(charlie): Add support for environments in scripts.
                None
            }
        }
    }

    /// Returns the set of required platforms for the [`LockTarget`].
    pub(crate) fn required_environments(self) -> Option<&'lock SupportedEnvironments> {
        match self {
            Self::Workspace(workspace) => workspace.required_environments(),
            Self::Script(_) => {
                // TODO(charlie): Add support for environments in scripts.
                None
            }
        }
    }

    /// Returns the set of conflicts for the [`LockTarget`].
    pub(crate) fn conflicts(self) -> Conflicts {
        match self {
            Self::Workspace(workspace) => workspace.conflicts(),
            Self::Script(_) => Conflicts::empty(),
        }
    }

    /// Return an iterator over the [`Index`] definitions in the [`LockTarget`].
    pub(crate) fn indexes(self) -> impl Iterator<Item = &'lock Index> {
        match self {
            Self::Workspace(workspace) => Either::Left(workspace.indexes().iter().chain(
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
            )),
            Self::Script(script) => Either::Right(
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

    /// Return the `Requires-Python` bound for the [`LockTarget`].
    pub(crate) fn requires_python(self) -> Result<Option<RequiresPython>, ProjectError> {
        match self {
            Self::Workspace(workspace) => {
                // When locking, don't try to enforce requires-python bounds that appear on groups
                let groups = DependencyGroupsWithDefaults::none();
                find_requires_python(workspace, &groups)
            }
            Self::Script(script) => Ok(script
                .metadata
                .requires_python
                .as_ref()
                .map(RequiresPython::from_specifiers)),
        }
    }

    /// Return the path to the lock root.
    pub(crate) fn install_path(self) -> &'lock Path {
        match self {
            Self::Workspace(workspace) => workspace.install_path(),
            Self::Script(script) => script.path.parent().unwrap(),
        }
    }

    /// Return the path to the lockfile.
    pub(crate) fn lock_path(self) -> PathBuf {
        match self {
            // `uv.lock`
            Self::Workspace(workspace) => workspace.install_path().join("uv.lock"),
            // `script.py.lock`
            Self::Script(script) => {
                let mut file_name = match script.path.file_name() {
                    Some(f) => f.to_os_string(),
                    None => panic!("Script path has no file name"),
                };
                file_name.push(".lock");
                script.path.with_file_name(file_name)
            }
        }
    }

    /// Read the lockfile from the workspace.
    ///
    /// Returns `Ok(None)` if the lockfile does not exist.
    pub(crate) async fn read(self) -> Result<Option<Lock>, ProjectError> {
        let lock_path = self.lock_path();
        match fs_err::tokio::read_to_string(&lock_path).await {
            Ok(encoded) => {
                let result = info_span!("toml::from_str lock", path = %lock_path.display())
                    .in_scope(|| toml::from_str::<Lock>(&encoded));
                match result {
                    Ok(lock) => {
                        // If the lockfile uses an unsupported version, raise an error.
                        if lock.version() != VERSION {
                            return Err(ProjectError::UnsupportedLockVersion(
                                VERSION,
                                lock.version(),
                            ));
                        }
                        Ok(Some(lock))
                    }
                    Err(err) => {
                        // If we failed to parse the lockfile, determine whether it's a supported
                        // version.
                        if let Ok(lock) = toml::from_str::<LockVersion>(&encoded) {
                            if lock.version() != VERSION {
                                return Err(ProjectError::UnparsableLockVersion(
                                    VERSION,
                                    lock.version(),
                                    err,
                                ));
                            }
                        }
                        Err(ProjectError::UvLockParse(err))
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    /// Read the lockfile from the workspace as bytes.
    pub(crate) async fn read_bytes(self) -> Result<Option<Vec<u8>>, std::io::Error> {
        match fs_err::tokio::read(self.lock_path()).await {
            Ok(encoded) => Ok(Some(encoded)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Write the lockfile to disk.
    pub(crate) async fn commit(self, lock: &Lock) -> Result<(), ProjectError> {
        let encoded = lock.to_toml()?;
        fs_err::tokio::write(self.lock_path(), encoded).await?;
        Ok(())
    }

    /// Lower the requirements for the [`LockTarget`], relative to the target root.
    pub(crate) fn lower(
        self,
        requirements: Vec<uv_pep508::Requirement<VerbatimParsedUrl>>,
        locations: &IndexLocations,
        sources: &NoSources,
        credentials_cache: &CredentialsCache,
    ) -> Result<Vec<Requirement>, uv_distribution::MetadataError> {
        match self {
            Self::Workspace(workspace) => {
                let name = workspace
                    .pyproject_toml()
                    .project
                    .as_ref()
                    .map(|project| project.name.clone());

                // We model these as `build-requires`, since, like build requirements, it doesn't define extras
                // or dependency groups.
                let metadata = uv_distribution::BuildRequires::from_workspace(
                    uv_pypi_types::BuildRequires {
                        name,
                        requires_dist: requirements,
                    },
                    workspace,
                    locations,
                    sources,
                    credentials_cache,
                )?;

                Ok(metadata
                    .requires_dist
                    .into_iter()
                    .map(|requirement| requirement.with_origin(RequirementOrigin::Workspace))
                    .collect::<Vec<_>>())
            }
            Self::Script(script) => {
                // Collect any `tool.uv.index` from the script.
                let empty = Vec::default();
                let indexes = script
                    .metadata
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.uv.as_ref())
                    .and_then(|uv| uv.top_level.index.as_deref())
                    .unwrap_or(&empty);

                // Collect any `tool.uv.sources` from the script.
                let empty = BTreeMap::default();
                let sources_map = script
                    .metadata
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.uv.as_ref())
                    .and_then(|uv| uv.sources.as_ref())
                    .unwrap_or(&empty);

                Ok(requirements
                    .into_iter()
                    .flat_map(|requirement| {
                        // Check if sources should be disabled for this specific package
                        if sources.for_package(&requirement.name) {
                            vec![Ok(Requirement::from(requirement))].into_iter()
                        } else {
                            let requirement_name = requirement.name.clone();
                            LoweredRequirement::from_non_workspace_requirement(
                                requirement,
                                script.path.parent().unwrap(),
                                sources_map,
                                indexes,
                                locations,
                                credentials_cache,
                            )
                            .map(move |requirement| match requirement {
                                Ok(requirement) => Ok(requirement.into_inner()),
                                Err(err) => Err(uv_distribution::MetadataError::LoweringError(
                                    requirement_name.clone(),
                                    Box::new(err),
                                )),
                            })
                            .collect::<Vec<_>>()
                            .into_iter()
                        }
                    })
                    .collect::<Result<_, _>>()?)
            }
        }
    }

    /// Lower any `tool.uv.sources` entries that refer to transitive dependencies into synthetic
    /// constraints.
    pub(crate) fn lower_transitive_source_constraints(
        self,
        conflicts: &Conflicts,
        locations: &IndexLocations,
        source_strategy: &NoSources,
        credentials_cache: &CredentialsCache,
    ) -> Result<Vec<Requirement>, uv_distribution::MetadataError> {
        if source_strategy.all() {
            return Ok(Vec::new());
        }

        match self {
            Self::Workspace(workspace) => {
                let root_has_project_conflict = workspace
                    .pyproject_toml()
                    .project
                    .as_ref()
                    .is_some_and(|project| {
                        conflicts.iter().any(|set| {
                            set.iter().any(|item| {
                                item.package() == &project.name
                                    && matches!(item.kind(), uv_pypi_types::ConflictKind::Project)
                            })
                        })
                    });

                let empty_sources = BTreeMap::default();
                let root_sources = if root_has_project_conflict {
                    workspace.sources().clone()
                } else {
                    filter_workspace_root_transitive_sources(workspace)
                };
                let mut lowered_constraints = Vec::new();

                if !workspace.is_non_project() || !workspace.sources().is_empty() {
                    let empty_indexes = vec![];
                    let project_indexes = workspace
                        .pyproject_toml()
                        .tool
                        .as_ref()
                        .and_then(|tool| tool.uv.as_ref())
                        .and_then(|uv| uv.index.as_deref())
                        .unwrap_or(&empty_indexes);

                    let mut direct_contexts = DirectDependencyContexts::default();
                    direct_contexts.collect_from_pyproject_toml(
                        workspace.install_path(),
                        workspace.pyproject_toml(),
                    )?;

                    lowered_constraints.extend(
                        lower_workspace_project_transitive_source_constraints(
                            WorkspaceProjectTransitiveSources {
                                project_name: workspace
                                    .pyproject_toml()
                                    .project
                                    .as_ref()
                                    .map(|project| project.name.clone()),
                                project_path: workspace.install_path().join("pyproject.toml"),
                                project_root: workspace.install_path(),
                                project_sources: &root_sources,
                                inherited_sources: &empty_sources,
                                project_indexes,
                                direct_contexts: &direct_contexts,
                            },
                            workspace,
                            locations,
                            source_strategy,
                            credentials_cache,
                        )?,
                    );
                }

                for project_member in workspace.packages().values() {
                    if project_member.root() == workspace.install_path() {
                        continue;
                    }

                    let empty_sources = BTreeMap::default();
                    let project_sources = project_member
                        .pyproject_toml()
                        .tool
                        .as_ref()
                        .and_then(|tool| tool.uv.as_ref())
                        .and_then(|uv| uv.sources.as_ref())
                        .map(uv_workspace::pyproject::ToolUvSources::inner)
                        .unwrap_or(&empty_sources);

                    let empty_indexes = vec![];
                    let project_indexes = project_member
                        .pyproject_toml()
                        .tool
                        .as_ref()
                        .and_then(|tool| tool.uv.as_ref())
                        .and_then(|uv| uv.index.as_deref())
                        .unwrap_or(&empty_indexes);

                    let mut direct_contexts = DirectDependencyContexts::default();
                    direct_contexts.collect_from_pyproject_toml(
                        project_member.root(),
                        project_member.pyproject_toml(),
                    )?;

                    lowered_constraints.extend(
                        lower_workspace_project_transitive_source_constraints(
                            WorkspaceProjectTransitiveSources {
                                project_name: project_member
                                    .pyproject_toml()
                                    .project
                                    .as_ref()
                                    .map(|project| project.name.clone()),
                                project_path: project_member.root().join("pyproject.toml"),
                                project_root: project_member.root(),
                                project_sources,
                                inherited_sources: workspace.sources(),
                                project_indexes,
                                direct_contexts: &direct_contexts,
                            },
                            workspace,
                            locations,
                            source_strategy,
                            credentials_cache,
                        )?,
                    );
                }

                Ok(lowered_constraints)
            }
            Self::Script(script) => {
                let empty_indexes = Vec::default();
                let indexes = script
                    .metadata
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.uv.as_ref())
                    .and_then(|uv| uv.top_level.index.as_deref())
                    .unwrap_or(&empty_indexes);

                let empty_sources = BTreeMap::default();
                let sources_map = script
                    .metadata
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.uv.as_ref())
                    .and_then(|uv| uv.sources.as_ref())
                    .unwrap_or(&empty_sources);

                if sources_map.is_empty() {
                    return Ok(Vec::new());
                }

                let direct_contexts = {
                    let mut direct_contexts = DirectDependencyContexts::default();
                    for requirement in script.metadata.dependencies.iter().flatten() {
                        direct_contexts.insert_any(requirement);
                        if let Some(extra) = requirement.marker.top_level_extra_name() {
                            direct_contexts.insert_extra(requirement, extra.as_ref());
                        }
                    }
                    direct_contexts
                };

                let mut lowered_constraints = Vec::new();
                for (package, package_sources) in sources_map {
                    if source_strategy.for_package(package) {
                        continue;
                    }

                    for (extra, group) in source_scopes(package_sources) {
                        // Dependency groups are not supported in script metadata.
                        if group.is_some() {
                            continue;
                        }

                        if direct_contexts.is_direct(package, extra.as_ref(), None) {
                            continue;
                        }

                        let origin = if let Some(extra) = &extra {
                            RequirementOrigin::Extra(script.path.clone(), None, extra.clone())
                        } else {
                            RequirementOrigin::File(script.path.clone())
                        };

                        let requirement_name = package.clone();
                        let mut lowered = LoweredRequirement::from_non_workspace_requirement(
                            uv_pep508::Requirement {
                                name: package.clone(),
                                extras: Box::default(),
                                marker: MarkerTree::TRUE,
                                version_or_url: None,
                                origin: Some(origin),
                            },
                            script.path.parent().unwrap(),
                            sources_map,
                            indexes,
                            locations,
                            credentials_cache,
                        )
                        .map(move |requirement| match requirement {
                            Ok(requirement) => Ok(requirement.into_inner()),
                            Err(err) => Err(uv_distribution::MetadataError::LoweringError(
                                requirement_name.clone(),
                                Box::new(err),
                            )),
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                        lowered_constraints.append(&mut lowered);
                    }
                }

                Ok(lowered_constraints
                    .into_iter()
                    .filter(|requirement| !is_noop_constraint(requirement))
                    .collect())
            }
        }
    }
}
