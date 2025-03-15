//! Resolve the current [`ProjectWorkspace`] or [`Workspace`].

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use glob::{glob, GlobError, PatternError};
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::{debug, trace, warn};

use uv_distribution_types::Index;
use uv_fs::{Simplified, CWD};
use uv_normalize::{GroupName, PackageName, DEV_DEPENDENCIES};
use uv_pep440::VersionSpecifiers;
use uv_pep508::{MarkerTree, VerbatimUrl};
use uv_pypi_types::{
    Conflicts, Requirement, RequirementSource, SupportedEnvironments, VerbatimParsedUrl,
};
use uv_static::EnvVars;
use uv_warnings::warn_user_once;

use crate::dependency_groups::{DependencyGroupError, FlatDependencyGroups};
use crate::pyproject::{
    Project, PyProjectToml, PyprojectTomlError, Sources, ToolUvSources, ToolUvWorkspace,
};

type WorkspaceMembers = Arc<BTreeMap<PackageName, WorkspaceMember>>;

/// Cache key for workspace discovery.
///
/// Given this key, the discovered workspace member list is the same.
#[derive(Debug, Default, Clone, Hash, PartialEq, Eq)]
struct WorkspaceCacheKey {
    workspace_root: PathBuf,
    discovery_options: DiscoveryOptions,
}

/// Cache for workspace discovery.
///
/// Avoid re-reading the `pyproject.toml` files in a workspace for each member by caching the
/// workspace members by their workspace root.
#[derive(Debug, Default, Clone)]
pub struct WorkspaceCache(Arc<Mutex<FxHashMap<WorkspaceCacheKey, WorkspaceMembers>>>);

#[derive(thiserror::Error, Debug)]
pub enum WorkspaceError {
    // Workspace structure errors.
    #[error("No `pyproject.toml` found in current directory or any parent directory")]
    MissingPyprojectToml,
    #[error("Workspace member `{}` is missing a `pyproject.toml` (matches: `{}`)", _0.simplified_display(), _1)]
    MissingPyprojectTomlMember(PathBuf, String),
    #[error("No `project` table found in: `{}`", _0.simplified_display())]
    MissingProject(PathBuf),
    #[error("No workspace found for: `{}`", _0.simplified_display())]
    MissingWorkspace(PathBuf),
    #[error("The project is marked as unmanaged: `{}`", _0.simplified_display())]
    NonWorkspace(PathBuf),
    #[error("Nested workspaces are not supported, but workspace member (`{}`) has a `uv.workspace` table", _0.simplified_display())]
    NestedWorkspace(PathBuf),
    #[error("Two workspace members are both named: `{name}`: `{}` and `{}`", first.simplified_display(), second.simplified_display())]
    DuplicatePackage {
        name: PackageName,
        first: PathBuf,
        second: PathBuf,
    },
    #[error("pyproject.toml section is declared as dynamic, but must be static: `{0}`")]
    DynamicNotAllowed(&'static str),
    #[error("Failed to find directories for glob: `{0}`")]
    Pattern(String, #[source] PatternError),
    // Syntax and other errors.
    #[error("Directory walking failed for `tool.uv.workspace.members` glob: `{0}`")]
    GlobWalk(String, #[source] GlobError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Failed to parse: `{}`", _0.user_display())]
    Toml(PathBuf, #[source] Box<PyprojectTomlError>),
    #[error("Failed to normalize workspace member path")]
    Normalize(#[source] std::io::Error),
}

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq)]
pub enum MemberDiscovery {
    /// Discover all workspace members.
    #[default]
    All,
    /// Don't discover any workspace members.
    None,
    /// Discover workspace members, but ignore the given paths.
    Ignore(BTreeSet<PathBuf>),
}

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq)]
pub struct DiscoveryOptions {
    /// The path to stop discovery at.
    pub stop_discovery_at: Option<PathBuf>,
    /// The strategy to use when discovering workspace members.
    pub members: MemberDiscovery,
}

/// A workspace, consisting of a root directory and members. See [`ProjectWorkspace`].
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(serde::Serialize))]
pub struct Workspace {
    /// The path to the workspace root.
    ///
    /// The workspace root is the directory containing the top level `pyproject.toml` with
    /// the `uv.tool.workspace`, or the `pyproject.toml` in an implicit single workspace project.
    install_path: PathBuf,
    /// The members of the workspace.
    packages: WorkspaceMembers,
    /// The sources table from the workspace `pyproject.toml`.
    ///
    /// This table is overridden by the project sources.
    sources: BTreeMap<PackageName, Sources>,
    /// The index table from the workspace `pyproject.toml`.
    ///
    /// This table is overridden by the project indexes.
    indexes: Vec<Index>,
    /// The `pyproject.toml` of the workspace root.
    pyproject_toml: PyProjectToml,
}

impl Workspace {
    /// Find the workspace containing the given path.
    ///
    /// Unlike the [`ProjectWorkspace`] discovery, this does not require a current project. It also
    /// always uses absolute path, i.e., this method only supports discovering the main workspace.
    ///
    /// Steps of workspace discovery: Start by looking at the closest `pyproject.toml`:
    /// * If it's an explicit workspace root: Collect workspace from this root, we're done.
    /// * If it's also not a project: Error, must be either a workspace root or a project.
    /// * Otherwise, try to find an explicit workspace root above:
    ///   * If an explicit workspace root exists: Collect workspace from this root, we're done.
    ///   * If there is no explicit workspace: We have a single project workspace, we're done.
    ///
    /// Note that there are two kinds of workspace roots: projects, and (legacy) non-project roots.
    /// The non-project roots lack a `[project]` table, and so are not themselves projects, as in:
    /// ```toml
    /// [tool.uv.workspace]
    /// members = ["packages/*"]
    ///
    /// [tool.uv]
    /// dev-dependencies = ["ruff"]
    /// ```
    pub async fn discover(
        path: &Path,
        options: &DiscoveryOptions,
        cache: &WorkspaceCache,
    ) -> Result<Workspace, WorkspaceError> {
        let path = std::path::absolute(path)
            .map_err(WorkspaceError::Normalize)?
            .clone();

        let project_path = path
            .ancestors()
            .find(|path| path.join("pyproject.toml").is_file())
            .ok_or(WorkspaceError::MissingPyprojectToml)?
            .to_path_buf();

        let pyproject_path = project_path.join("pyproject.toml");
        let contents = fs_err::tokio::read_to_string(&pyproject_path).await?;
        let pyproject_toml = PyProjectToml::from_string(contents)
            .map_err(|err| WorkspaceError::Toml(pyproject_path.clone(), Box::new(err)))?;

        // Check if the project is explicitly marked as unmanaged.
        if pyproject_toml
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.managed)
            == Some(false)
        {
            debug!(
                "Project `{}` is marked as unmanaged",
                project_path.simplified_display()
            );
            return Err(WorkspaceError::NonWorkspace(project_path));
        }

        // Check if the current project is also an explicit workspace root.
        let explicit_root = pyproject_toml
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.workspace.as_ref())
            .map(|workspace| {
                (
                    project_path.clone(),
                    workspace.clone(),
                    pyproject_toml.clone(),
                )
            });

        let (workspace_root, workspace_definition, workspace_pyproject_toml) =
            if let Some(workspace) = explicit_root {
                // We have found the explicit root immediately.
                workspace
            } else if pyproject_toml.project.is_none() {
                // Without a project, it can't be an implicit root
                return Err(WorkspaceError::MissingProject(pyproject_path));
            } else if let Some(workspace) = find_workspace(&project_path, options).await? {
                // We have found an explicit root above.
                workspace
            } else {
                // Support implicit single project workspaces.
                (
                    project_path.clone(),
                    ToolUvWorkspace::default(),
                    pyproject_toml.clone(),
                )
            };

        debug!(
            "Found workspace root: `{}`",
            workspace_root.simplified_display()
        );

        // Unlike in `ProjectWorkspace` discovery, we might be in a legacy non-project root without
        // being in any specific project.
        let current_project = pyproject_toml
            .project
            .clone()
            .map(|project| WorkspaceMember {
                root: project_path,
                project,
                pyproject_toml,
            });

        Self::collect_members(
            workspace_root.clone(),
            workspace_definition,
            workspace_pyproject_toml,
            current_project,
            options,
            cache,
        )
        .await
    }

    /// Set the current project to the given workspace member.
    ///
    /// Returns `None` if the package is not part of the workspace.
    pub fn with_current_project(self, package_name: PackageName) -> Option<ProjectWorkspace> {
        let member = self.packages.get(&package_name)?;
        Some(ProjectWorkspace {
            project_root: member.root().clone(),
            project_name: package_name,
            workspace: self,
        })
    }

    /// Set the [`ProjectWorkspace`] for a given workspace member.
    ///
    /// Assumes that the project name is unchanged in the updated [`PyProjectToml`].
    #[must_use]
    pub fn with_pyproject_toml(
        self,
        package_name: &PackageName,
        pyproject_toml: PyProjectToml,
    ) -> Option<Self> {
        let mut packages = self.packages;
        let member = Arc::make_mut(&mut packages).get_mut(package_name)?;

        if member.root == self.install_path {
            // If the member is also the workspace root, update _both_ the member entry and the
            // root `pyproject.toml`.
            let workspace_pyproject_toml = pyproject_toml.clone();

            // Refresh the workspace sources.
            let workspace_sources = workspace_pyproject_toml
                .tool
                .clone()
                .and_then(|tool| tool.uv)
                .and_then(|uv| uv.sources)
                .map(ToolUvSources::into_inner)
                .unwrap_or_default();

            // Set the `pyproject.toml` for the member.
            member.pyproject_toml = pyproject_toml;

            Some(Self {
                pyproject_toml: workspace_pyproject_toml,
                sources: workspace_sources,
                packages,
                ..self
            })
        } else {
            // Set the `pyproject.toml` for the member.
            member.pyproject_toml = pyproject_toml;

            Some(Self { packages, ..self })
        }
    }

    /// Returns `true` if the workspace has a (legacy) non-project root.
    pub fn is_non_project(&self) -> bool {
        !self
            .packages
            .values()
            .any(|member| *member.root() == self.install_path)
    }

    /// Returns the set of all workspace members.
    pub fn members_requirements(&self) -> impl Iterator<Item = Requirement> + '_ {
        self.packages.values().filter_map(|member| {
            let url = VerbatimUrl::from_absolute_path(&member.root)
                .expect("path is valid URL")
                .with_given(member.root.to_string_lossy());
            Some(Requirement {
                name: member.pyproject_toml.project.as_ref()?.name.clone(),
                extras: vec![],
                groups: vec![],
                marker: MarkerTree::TRUE,
                source: if member.pyproject_toml.is_package() {
                    RequirementSource::Directory {
                        install_path: member.root.clone(),
                        editable: true,
                        r#virtual: false,
                        url,
                    }
                } else {
                    RequirementSource::Directory {
                        install_path: member.root.clone(),
                        editable: false,
                        r#virtual: true,
                        url,
                    }
                },
                origin: None,
            })
        })
    }

    /// Returns the set of all workspace member dependency groups.
    pub fn group_requirements(&self) -> impl Iterator<Item = Requirement> + '_ {
        self.packages.values().filter_map(|member| {
            let url = VerbatimUrl::from_absolute_path(&member.root)
                .expect("path is valid URL")
                .with_given(member.root.to_string_lossy());

            let groups = {
                let mut groups = member
                    .pyproject_toml
                    .dependency_groups
                    .as_ref()
                    .map(|groups| groups.keys().cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                if member
                    .pyproject_toml
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.uv.as_ref())
                    .and_then(|uv| uv.dev_dependencies.as_ref())
                    .is_some()
                {
                    groups.push(DEV_DEPENDENCIES.clone());
                    groups.sort_unstable();
                }
                groups
            };
            if groups.is_empty() {
                return None;
            }

            Some(Requirement {
                name: member.pyproject_toml.project.as_ref()?.name.clone(),
                extras: vec![],
                groups,
                marker: MarkerTree::TRUE,
                source: if member.pyproject_toml.is_package() {
                    RequirementSource::Directory {
                        install_path: member.root.clone(),
                        editable: true,
                        r#virtual: false,
                        url,
                    }
                } else {
                    RequirementSource::Directory {
                        install_path: member.root.clone(),
                        editable: false,
                        r#virtual: true,
                        url,
                    }
                },
                origin: None,
            })
        })
    }

    /// Returns the set of supported environments for the workspace.
    pub fn environments(&self) -> Option<&SupportedEnvironments> {
        self.pyproject_toml
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.environments.as_ref())
    }

    /// Returns the set of required platforms for the workspace.
    pub fn required_environments(&self) -> Option<&SupportedEnvironments> {
        self.pyproject_toml
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.required_environments.as_ref())
    }

    /// Returns the set of conflicts for the workspace.
    pub fn conflicts(&self) -> Conflicts {
        let mut conflicting = Conflicts::empty();
        for member in self.packages.values() {
            conflicting.append(&mut member.pyproject_toml.conflicts());
        }
        conflicting
    }

    /// Returns an iterator over the `requires-python` values for each member of the workspace.
    pub fn requires_python(&self) -> impl Iterator<Item = (&PackageName, &VersionSpecifiers)> {
        self.packages().iter().filter_map(|(name, member)| {
            member
                .pyproject_toml()
                .project
                .as_ref()
                .and_then(|project| project.requires_python.as_ref())
                .map(|requires_python| (name, requires_python))
        })
    }

    /// Returns any requirements that are exclusive to the workspace root, i.e., not included in
    /// any of the workspace members.
    ///
    /// For now, there are no such requirements.
    pub fn requirements(&self) -> Vec<uv_pep508::Requirement<VerbatimParsedUrl>> {
        Vec::new()
    }

    /// Returns any dependency groups that are exclusive to the workspace root, i.e., not included
    /// in any of the workspace members.
    ///
    /// For workspaces with non-`[project]` roots, returns the dependency groups defined in the
    /// corresponding `pyproject.toml`.
    ///
    /// Otherwise, returns an empty list.
    pub fn dependency_groups(
        &self,
    ) -> Result<
        BTreeMap<GroupName, Vec<uv_pep508::Requirement<VerbatimParsedUrl>>>,
        DependencyGroupError,
    > {
        if self
            .packages
            .values()
            .any(|member| *member.root() == self.install_path)
        {
            // If the workspace has an explicit root, the root is a member, so we don't need to
            // include any root-only requirements.
            Ok(BTreeMap::default())
        } else {
            // Otherwise, return the dependency groups in the non-project workspace root.
            // First, collect `tool.uv.dev_dependencies`
            let dev_dependencies = self
                .pyproject_toml
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.dev_dependencies.as_ref());

            // Then, collect `dependency-groups`
            let dependency_groups = self
                .pyproject_toml
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

            Ok(dependency_groups.into_inner())
        }
    }

    /// Returns the set of overrides for the workspace.
    pub fn overrides(&self) -> Vec<uv_pep508::Requirement<VerbatimParsedUrl>> {
        let Some(overrides) = self
            .pyproject_toml
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.override_dependencies.as_ref())
        else {
            return vec![];
        };
        overrides.clone()
    }

    /// Returns the set of constraints for the workspace.
    pub fn constraints(&self) -> Vec<uv_pep508::Requirement<VerbatimParsedUrl>> {
        let Some(constraints) = self
            .pyproject_toml
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.constraint_dependencies.as_ref())
        else {
            return vec![];
        };
        constraints.clone()
    }

    /// Returns the set of build constraints for the workspace.
    pub fn build_constraints(&self) -> Vec<uv_pep508::Requirement<VerbatimParsedUrl>> {
        let Some(build_constraints) = self
            .pyproject_toml
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.build_constraint_dependencies.as_ref())
        else {
            return vec![];
        };
        build_constraints.clone()
    }

    /// The path to the workspace root, the directory containing the top level `pyproject.toml` with
    /// the `uv.tool.workspace`, or the `pyproject.toml` in an implicit single workspace project.
    pub fn install_path(&self) -> &PathBuf {
        &self.install_path
    }

    /// The path to the workspace virtual environment.
    ///
    /// Uses `.venv` in the install path directory by default.
    ///
    /// If `UV_PROJECT_ENVIRONMENT` is set, it will take precedence. If a relative path is provided,
    /// it is resolved relative to the install path.
    ///
    /// If `active` is `true`, the `VIRTUAL_ENV` variable will be preferred. If it is `false`, any
    /// warnings about mismatch between the active environment and the project environment will be
    /// silenced.
    pub fn venv(&self, active: Option<bool>) -> PathBuf {
        /// Resolve the `UV_PROJECT_ENVIRONMENT` value, if any.
        fn from_project_environment_variable(workspace: &Workspace) -> Option<PathBuf> {
            let value = std::env::var_os(EnvVars::UV_PROJECT_ENVIRONMENT)?;

            if value.is_empty() {
                return None;
            };

            let path = PathBuf::from(value);
            if path.is_absolute() {
                return Some(path);
            };

            // Resolve the path relative to the install path.
            Some(workspace.install_path.join(path))
        }

        /// Resolve the `VIRTUAL_ENV` variable, if any.
        fn from_virtual_env_variable() -> Option<PathBuf> {
            let value = std::env::var_os(EnvVars::VIRTUAL_ENV)?;

            if value.is_empty() {
                return None;
            };

            let path = PathBuf::from(value);
            if path.is_absolute() {
                return Some(path);
            };

            // Resolve the path relative to current directory.
            // Note this differs from `UV_PROJECT_ENVIRONMENT`
            Some(CWD.join(path))
        }

        // Determine the default value
        let project_env = from_project_environment_variable(self)
            .unwrap_or_else(|| self.install_path.join(".venv"));

        // Warn if it conflicts with `VIRTUAL_ENV`
        if let Some(from_virtual_env) = from_virtual_env_variable() {
            if !uv_fs::is_same_file_allow_missing(&from_virtual_env, &project_env).unwrap_or(false)
            {
                match active {
                    Some(true) => {
                        debug!(
                            "Using active virtual environment `{}` instead of project environment `{}`",
                            from_virtual_env.user_display(),
                            project_env.user_display()
                        );
                        return from_virtual_env;
                    }
                    Some(false) => {}
                    None => {
                        warn_user_once!(
                            "`VIRTUAL_ENV={}` does not match the project environment path `{}` and will be ignored; use `--active` to target the active environment instead",
                            from_virtual_env.user_display(),
                            project_env.user_display()
                        );
                    }
                }
            }
        } else {
            if active.unwrap_or_default() {
                debug!(
                    "Use of the active virtual environment was requested, but `VIRTUAL_ENV` is not set"
                );
            }
        }

        project_env
    }

    /// The members of the workspace.
    pub fn packages(&self) -> &BTreeMap<PackageName, WorkspaceMember> {
        &self.packages
    }

    /// The sources table from the workspace `pyproject.toml`.
    pub fn sources(&self) -> &BTreeMap<PackageName, Sources> {
        &self.sources
    }

    /// The index table from the workspace `pyproject.toml`.
    pub fn indexes(&self) -> &[Index] {
        &self.indexes
    }

    /// The `pyproject.toml` of the workspace.
    pub fn pyproject_toml(&self) -> &PyProjectToml {
        &self.pyproject_toml
    }

    /// Returns `true` if the path is excluded by the workspace.
    pub fn excludes(&self, project_path: &Path) -> Result<bool, WorkspaceError> {
        if let Some(workspace) = self
            .pyproject_toml
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.workspace.as_ref())
        {
            is_excluded_from_workspace(project_path, &self.install_path, workspace)
        } else {
            Ok(false)
        }
    }

    /// Returns `true` if the path is included by the workspace.
    pub fn includes(&self, project_path: &Path) -> Result<bool, WorkspaceError> {
        if let Some(workspace) = self
            .pyproject_toml
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.workspace.as_ref())
        {
            is_included_in_workspace(project_path, &self.install_path, workspace)
        } else {
            Ok(false)
        }
    }

    /// Collect the workspace member projects from the `members` and `excludes` entries.
    async fn collect_members(
        workspace_root: PathBuf,
        workspace_definition: ToolUvWorkspace,
        workspace_pyproject_toml: PyProjectToml,
        current_project: Option<WorkspaceMember>,
        options: &DiscoveryOptions,
        cache: &WorkspaceCache,
    ) -> Result<Workspace, WorkspaceError> {
        let cache_key = WorkspaceCacheKey {
            workspace_root: workspace_root.clone(),
            discovery_options: options.clone(),
        };
        let cache_entry = {
            // Acquire the lock for the minimal required region
            let cache = cache.0.lock().expect("there was a panic in another thread");
            cache.get(&cache_key).cloned()
        };
        let mut workspace_members = if let Some(workspace_members) = cache_entry {
            trace!(
                "Cached workspace members for: `{}`",
                &workspace_root.simplified_display()
            );
            workspace_members
        } else {
            trace!(
                "Discovering workspace members for: `{}`",
                &workspace_root.simplified_display()
            );
            let workspace_members = Self::collect_members_only(
                &workspace_root,
                &workspace_definition,
                &workspace_pyproject_toml,
                options,
            )
            .await?;
            {
                // Acquire the lock for the minimal required region
                let mut cache = cache.0.lock().expect("there was a panic in another thread");
                cache.insert(cache_key, Arc::new(workspace_members.clone()));
            }
            Arc::new(workspace_members)
        };

        // For the cases such as `MemberDiscovery::None`, add the current project if missing.
        if let Some(root_member) = current_project {
            if !workspace_members.contains_key(&root_member.project.name) {
                debug!(
                    "Adding current workspace member: `{}`",
                    root_member.root.simplified_display()
                );

                Arc::make_mut(&mut workspace_members)
                    .insert(root_member.project.name.clone(), root_member);
            }
        }

        let workspace_sources = workspace_pyproject_toml
            .tool
            .clone()
            .and_then(|tool| tool.uv)
            .and_then(|uv| uv.sources)
            .map(ToolUvSources::into_inner)
            .unwrap_or_default();

        let workspace_indexes = workspace_pyproject_toml
            .tool
            .clone()
            .and_then(|tool| tool.uv)
            .and_then(|uv| uv.index)
            .unwrap_or_default();

        Ok(Workspace {
            install_path: workspace_root,
            packages: workspace_members,
            sources: workspace_sources,
            indexes: workspace_indexes,
            pyproject_toml: workspace_pyproject_toml,
        })
    }

    async fn collect_members_only(
        workspace_root: &PathBuf,
        workspace_definition: &ToolUvWorkspace,
        workspace_pyproject_toml: &PyProjectToml,
        options: &DiscoveryOptions,
    ) -> Result<BTreeMap<PackageName, WorkspaceMember>, WorkspaceError> {
        let mut workspace_members = BTreeMap::new();
        // Avoid reading a `pyproject.toml` more than once.
        let mut seen = FxHashSet::default();

        // Add the project at the workspace root, if it exists and if it's distinct from the current
        // project. If it is the current project, it is added as such in the next step.
        if let Some(project) = &workspace_pyproject_toml.project {
            let pyproject_path = workspace_root.join("pyproject.toml");
            let contents = fs_err::read_to_string(&pyproject_path)?;
            let pyproject_toml = PyProjectToml::from_string(contents)
                .map_err(|err| WorkspaceError::Toml(pyproject_path.clone(), Box::new(err)))?;

            debug!(
                "Adding root workspace member: `{}`",
                workspace_root.simplified_display()
            );

            seen.insert(workspace_root.clone());
            workspace_members.insert(
                project.name.clone(),
                WorkspaceMember {
                    root: workspace_root.clone(),
                    project: project.clone(),
                    pyproject_toml,
                },
            );
        }

        // Add all other workspace members.
        for member_glob in workspace_definition.clone().members.unwrap_or_default() {
            let absolute_glob = PathBuf::from(glob::Pattern::escape(
                workspace_root.simplified().to_string_lossy().as_ref(),
            ))
            .join(member_glob.as_str())
            .to_string_lossy()
            .to_string();
            for member_root in glob(&absolute_glob)
                .map_err(|err| WorkspaceError::Pattern(absolute_glob.to_string(), err))?
            {
                let member_root = member_root
                    .map_err(|err| WorkspaceError::GlobWalk(absolute_glob.to_string(), err))?;
                if !seen.insert(member_root.clone()) {
                    continue;
                }
                let member_root = std::path::absolute(&member_root)
                    .map_err(WorkspaceError::Normalize)?
                    .clone();

                // If the directory is explicitly ignored, skip it.
                let skip = match &options.members {
                    MemberDiscovery::All => false,
                    MemberDiscovery::None => true,
                    MemberDiscovery::Ignore(ignore) => ignore.contains(member_root.as_path()),
                };
                if skip {
                    debug!(
                        "Ignoring workspace member: `{}`",
                        member_root.simplified_display()
                    );
                    continue;
                }

                // If the member is excluded, ignore it.
                if is_excluded_from_workspace(&member_root, workspace_root, workspace_definition)? {
                    debug!(
                        "Ignoring workspace member: `{}`",
                        member_root.simplified_display()
                    );
                    continue;
                }

                trace!(
                    "Processing workspace member: `{}`",
                    member_root.user_display()
                );

                // Read the member `pyproject.toml`.
                let pyproject_path = member_root.join("pyproject.toml");
                let contents = match fs_err::tokio::read_to_string(&pyproject_path).await {
                    Ok(contents) => contents,
                    Err(err) => {
                        if !fs_err::metadata(&member_root)?.is_dir() {
                            warn!(
                                "Ignoring non-directory workspace member: `{}`",
                                member_root.simplified_display()
                            );
                            continue;
                        }

                        // A directory exists, but it doesn't contain a `pyproject.toml`.
                        if err.kind() == std::io::ErrorKind::NotFound {
                            // If the directory is hidden, skip it.
                            if member_root
                                .file_name()
                                .map(|name| name.as_encoded_bytes().starts_with(b"."))
                                .unwrap_or(false)
                            {
                                debug!(
                                    "Ignoring hidden workspace member: `{}`",
                                    member_root.simplified_display()
                                );
                                continue;
                            }

                            return Err(WorkspaceError::MissingPyprojectTomlMember(
                                member_root,
                                member_glob.to_string(),
                            ));
                        }

                        return Err(err.into());
                    }
                };
                let pyproject_toml = PyProjectToml::from_string(contents)
                    .map_err(|err| WorkspaceError::Toml(pyproject_path.clone(), Box::new(err)))?;

                // Check if the current project is explicitly marked as unmanaged.
                if pyproject_toml
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.uv.as_ref())
                    .and_then(|uv| uv.managed)
                    == Some(false)
                {
                    debug!(
                        "Project `{}` is marked as unmanaged; omitting from workspace members",
                        pyproject_toml.project.as_ref().unwrap().name
                    );
                    continue;
                }

                // Extract the package name.
                let Some(project) = pyproject_toml.project.clone() else {
                    return Err(WorkspaceError::MissingProject(pyproject_path));
                };

                debug!(
                    "Adding discovered workspace member: `{}`",
                    member_root.simplified_display()
                );

                if let Some(existing) = workspace_members.insert(
                    project.name.clone(),
                    WorkspaceMember {
                        root: member_root.clone(),
                        project,
                        pyproject_toml,
                    },
                ) {
                    return Err(WorkspaceError::DuplicatePackage {
                        name: existing.project.name,
                        first: existing.root.clone(),
                        second: member_root,
                    });
                }
            }
        }

        // Test for nested workspaces.
        for member in workspace_members.values() {
            if member.root() != workspace_root
                && member
                    .pyproject_toml
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.uv.as_ref())
                    .and_then(|uv| uv.workspace.as_ref())
                    .is_some()
            {
                return Err(WorkspaceError::NestedWorkspace(member.root.clone()));
            }
        }
        Ok(workspace_members)
    }
}

/// A project in a workspace.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(test, derive(serde::Serialize))]
pub struct WorkspaceMember {
    /// The path to the project root.
    root: PathBuf,
    /// The `[project]` table, from the `pyproject.toml` of the project found at
    /// `<root>/pyproject.toml`.
    project: Project,
    /// The `pyproject.toml` of the project, found at `<root>/pyproject.toml`.
    pyproject_toml: PyProjectToml,
}

impl WorkspaceMember {
    /// The path to the project root.
    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    /// The `[project]` table, from the `pyproject.toml` of the project found at
    /// `<root>/pyproject.toml`.
    pub fn project(&self) -> &Project {
        &self.project
    }

    /// The `pyproject.toml` of the project, found at `<root>/pyproject.toml`.
    pub fn pyproject_toml(&self) -> &PyProjectToml {
        &self.pyproject_toml
    }
}

/// The current project and the workspace it is part of, with all of the workspace members.
///
/// # Structure
///
/// The workspace root is a directory with a `pyproject.toml`, all members need to be below that
/// directory. The workspace root defines members and exclusions. All packages below it must either
/// be a member or excluded. The workspace root can be a package itself or a virtual manifest.
///
/// For a simple single package project, the workspace root is implicitly the current project root
/// and the workspace has only this single member. Otherwise, a workspace root is declared through
/// a `tool.uv.workspace` section.
///
/// A workspace itself does not declare dependencies, instead one member is the current project used
/// as main requirement.
///
/// Each member is a directory with a `pyproject.toml` that contains a `[project]` section. Each
/// member is a Python package, with a name, a version and dependencies. Workspace members can
/// depend on other workspace members (`foo = { workspace = true }`). You can consider the
/// workspace another package source or index, similar to `--find-links`.
///
/// # Usage
///
/// There a two main usage patterns: A root package and helpers, and the flat workspace.
///
/// Root package and helpers:
///
/// ```text
/// albatross
/// ├── packages
/// │   ├── provider_a
/// │   │   ├── pyproject.toml
/// │   │   └── src
/// │   │       └── provider_a
/// │   │           ├── __init__.py
/// │   │           └── foo.py
/// │   └── provider_b
/// │       ├── pyproject.toml
/// │       └── src
/// │           └── provider_b
/// │               ├── __init__.py
/// │               └── bar.py
/// ├── pyproject.toml
/// ├── Readme.md
/// ├── uv.lock
/// └── src
///     └── albatross
///         ├── __init__.py
///         └── main.py
/// ```
///
/// Flat workspace:
///
/// ```text
/// albatross
/// ├── packages
/// │   ├── albatross
/// │   │   ├── pyproject.toml
/// │   │   └── src
/// │   │       └── albatross
/// │   │           ├── __init__.py
/// │   │           └── main.py
/// │   ├── provider_a
/// │   │   ├── pyproject.toml
/// │   │   └── src
/// │   │       └── provider_a
/// │   │           ├── __init__.py
/// │   │           └── foo.py
/// │   └── provider_b
/// │       ├── pyproject.toml
/// │       └── src
/// │           └── provider_b
/// │               ├── __init__.py
/// │               └── bar.py
/// ├── pyproject.toml
/// ├── Readme.md
/// └── uv.lock
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(serde::Serialize))]
pub struct ProjectWorkspace {
    /// The path to the project root.
    project_root: PathBuf,
    /// The name of the package.
    project_name: PackageName,
    /// The workspace the project is part of.
    workspace: Workspace,
}

impl ProjectWorkspace {
    /// Find the current project and workspace, given the current directory.
    ///
    /// `stop_discovery_at` must be either `None` or an ancestor of the current directory. If set,
    /// only directories between the current path and `stop_discovery_at` are considered.
    pub async fn discover(
        path: &Path,
        options: &DiscoveryOptions,
        cache: &WorkspaceCache,
    ) -> Result<Self, WorkspaceError> {
        let project_root = path
            .ancestors()
            .take_while(|path| {
                // Only walk up the given directory, if any.
                options
                    .stop_discovery_at
                    .as_deref()
                    .and_then(Path::parent)
                    .map(|stop_discovery_at| stop_discovery_at != *path)
                    .unwrap_or(true)
            })
            .find(|path| path.join("pyproject.toml").is_file())
            .ok_or(WorkspaceError::MissingPyprojectToml)?;

        debug!(
            "Found project root: `{}`",
            project_root.simplified_display()
        );

        Self::from_project_root(project_root, options, cache).await
    }

    /// Discover the workspace starting from the directory containing the `pyproject.toml`.
    async fn from_project_root(
        project_root: &Path,
        options: &DiscoveryOptions,
        cache: &WorkspaceCache,
    ) -> Result<Self, WorkspaceError> {
        // Read the current `pyproject.toml`.
        let pyproject_path = project_root.join("pyproject.toml");
        let contents = fs_err::tokio::read_to_string(&pyproject_path).await?;
        let pyproject_toml = PyProjectToml::from_string(contents)
            .map_err(|err| WorkspaceError::Toml(pyproject_path.clone(), Box::new(err)))?;

        // It must have a `[project]` table.
        let project = pyproject_toml
            .project
            .clone()
            .ok_or(WorkspaceError::MissingProject(pyproject_path))?;

        Self::from_project(project_root, &project, &pyproject_toml, options, cache).await
    }

    /// If the current directory contains a `pyproject.toml` with a `project` table, discover the
    /// workspace and return it, otherwise it is a dynamic path dependency and we return `Ok(None)`.
    pub async fn from_maybe_project_root(
        install_path: &Path,
        options: &DiscoveryOptions,
        cache: &WorkspaceCache,
    ) -> Result<Option<Self>, WorkspaceError> {
        // Read the `pyproject.toml`.
        let pyproject_path = install_path.join("pyproject.toml");
        let Ok(contents) = fs_err::tokio::read_to_string(&pyproject_path).await else {
            // No `pyproject.toml`, but there may still be a `setup.py` or `setup.cfg`.
            return Ok(None);
        };
        let pyproject_toml = PyProjectToml::from_string(contents)
            .map_err(|err| WorkspaceError::Toml(pyproject_path.clone(), Box::new(err)))?;

        // Extract the `[project]` metadata.
        let Some(project) = pyproject_toml.project.clone() else {
            // We have to build to get the metadata.
            return Ok(None);
        };

        match Self::from_project(install_path, &project, &pyproject_toml, options, cache).await {
            Ok(workspace) => Ok(Some(workspace)),
            Err(WorkspaceError::NonWorkspace(_)) => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Returns the directory containing the closest `pyproject.toml` that defines the current
    /// project.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Returns the [`PackageName`] of the current project.
    pub fn project_name(&self) -> &PackageName {
        &self.project_name
    }

    /// Returns the [`Workspace`] containing the current project.
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// Returns the current project as a [`WorkspaceMember`].
    pub fn current_project(&self) -> &WorkspaceMember {
        &self.workspace().packages[&self.project_name]
    }

    /// Set the `pyproject.toml` for the current project.
    ///
    /// Assumes that the project name is unchanged in the updated [`PyProjectToml`].
    #[must_use]
    pub fn with_pyproject_toml(self, pyproject_toml: PyProjectToml) -> Option<Self> {
        Some(Self {
            workspace: self
                .workspace
                .with_pyproject_toml(&self.project_name, pyproject_toml)?,
            ..self
        })
    }

    /// Find the workspace for a project.
    pub async fn from_project(
        install_path: &Path,
        project: &Project,
        project_pyproject_toml: &PyProjectToml,
        options: &DiscoveryOptions,
        cache: &WorkspaceCache,
    ) -> Result<Self, WorkspaceError> {
        let project_path = std::path::absolute(install_path)
            .map_err(WorkspaceError::Normalize)?
            .clone();

        // Check if workspaces are explicitly disabled for the project.
        if project_pyproject_toml
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.managed)
            == Some(false)
        {
            debug!("Project `{}` is marked as unmanaged", project.name);
            return Err(WorkspaceError::NonWorkspace(project_path));
        }

        // Check if the current project is also an explicit workspace root.
        let mut workspace = project_pyproject_toml
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.workspace.as_ref())
            .map(|workspace| {
                (
                    project_path.clone(),
                    workspace.clone(),
                    project_pyproject_toml.clone(),
                )
            });

        if workspace.is_none() {
            // The project isn't an explicit workspace root, check if we're a regular workspace
            // member by looking for an explicit workspace root above.
            workspace = find_workspace(&project_path, options).await?;
        }

        let current_project = WorkspaceMember {
            root: project_path.clone(),
            project: project.clone(),
            pyproject_toml: project_pyproject_toml.clone(),
        };

        let Some((workspace_root, workspace_definition, workspace_pyproject_toml)) = workspace
        else {
            // The project isn't an explicit workspace root, but there's also no workspace root
            // above it, so the project is an implicit workspace root identical to the project root.
            debug!("No workspace root found, using project root");

            let current_project_as_members = Arc::new(BTreeMap::from_iter([(
                project.name.clone(),
                current_project,
            )]));
            return Ok(Self {
                project_root: project_path.clone(),
                project_name: project.name.clone(),
                workspace: Workspace {
                    install_path: project_path.clone(),
                    packages: current_project_as_members,
                    // There may be package sources, but we don't need to duplicate them into the
                    // workspace sources.
                    sources: BTreeMap::default(),
                    indexes: Vec::default(),
                    pyproject_toml: project_pyproject_toml.clone(),
                },
            });
        };

        debug!(
            "Found workspace root: `{}`",
            workspace_root.simplified_display()
        );

        let workspace = Workspace::collect_members(
            workspace_root,
            workspace_definition,
            workspace_pyproject_toml,
            Some(current_project),
            options,
            cache,
        )
        .await?;

        Ok(Self {
            project_root: project_path,
            project_name: project.name.clone(),
            workspace,
        })
    }
}

/// Find the workspace root above the current project, if any.
async fn find_workspace(
    project_root: &Path,
    options: &DiscoveryOptions,
) -> Result<Option<(PathBuf, ToolUvWorkspace, PyProjectToml)>, WorkspaceError> {
    // Skip 1 to ignore the current project itself.
    for workspace_root in project_root
        .ancestors()
        .take_while(|path| {
            // Only walk up the given directory, if any.
            options
                .stop_discovery_at
                .as_deref()
                .and_then(Path::parent)
                .map(|stop_discovery_at| stop_discovery_at != *path)
                .unwrap_or(true)
        })
        .skip(1)
    {
        let pyproject_path = workspace_root.join("pyproject.toml");
        if !pyproject_path.is_file() {
            continue;
        }
        trace!(
            "Found `pyproject.toml` at: `{}`",
            pyproject_path.simplified_display()
        );

        // Read the `pyproject.toml`.
        let contents = fs_err::tokio::read_to_string(&pyproject_path).await?;
        let pyproject_toml = PyProjectToml::from_string(contents)
            .map_err(|err| WorkspaceError::Toml(pyproject_path.clone(), Box::new(err)))?;

        return if let Some(workspace) = pyproject_toml
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.workspace.as_ref())
        {
            if !is_included_in_workspace(project_root, workspace_root, workspace)? {
                debug!(
                    "Found workspace root `{}`, but project is not included",
                    workspace_root.simplified_display()
                );
                return Ok(None);
            }

            if is_excluded_from_workspace(project_root, workspace_root, workspace)? {
                debug!(
                    "Found workspace root `{}`, but project is excluded",
                    workspace_root.simplified_display()
                );
                return Ok(None);
            }

            // We found a workspace root.
            Ok(Some((
                workspace_root.to_path_buf(),
                workspace.clone(),
                pyproject_toml,
            )))
        } else if pyproject_toml.project.is_some() {
            // We're in a directory of another project, e.g. tests or examples.
            // Example:
            // ```
            // albatross
            // ├── examples
            // │   └── bird-feeder [CURRENT DIRECTORY]
            // │       ├── pyproject.toml
            // │       └── src
            // │           └── bird_feeder
            // │               └── __init__.py
            // ├── pyproject.toml
            // └── src
            //     └── albatross
            //         └── __init__.py
            // ```
            // The current project is the example (non-workspace) `bird-feeder` in `albatross`,
            // we ignore all `albatross` is doing and any potential workspace it might be
            // contained in.
            debug!(
                "Project is contained in non-workspace project: `{}`",
                workspace_root.simplified_display()
            );
            Ok(None)
        } else {
            // We require that a `project.toml` file either declares a workspace or a project.
            warn!(
                "`pyproject.toml` does not contain a `project` table: `{}`",
                pyproject_path.simplified_display()
            );
            Ok(None)
        };
    }

    Ok(None)
}

/// Check if we're in the `tool.uv.workspace.excluded` of a workspace.
fn is_excluded_from_workspace(
    project_path: &Path,
    workspace_root: &Path,
    workspace: &ToolUvWorkspace,
) -> Result<bool, WorkspaceError> {
    for exclude_glob in workspace.exclude.iter().flatten() {
        let absolute_glob = PathBuf::from(glob::Pattern::escape(
            workspace_root.simplified().to_string_lossy().as_ref(),
        ))
        .join(exclude_glob.as_str());
        let absolute_glob = absolute_glob.to_string_lossy();
        let exclude_pattern = glob::Pattern::new(&absolute_glob)
            .map_err(|err| WorkspaceError::Pattern(absolute_glob.to_string(), err))?;
        if exclude_pattern.matches_path(project_path) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Check if we're in the `tool.uv.workspace.members` of a workspace.
fn is_included_in_workspace(
    project_path: &Path,
    workspace_root: &Path,
    workspace: &ToolUvWorkspace,
) -> Result<bool, WorkspaceError> {
    for member_glob in workspace.members.iter().flatten() {
        let absolute_glob = PathBuf::from(glob::Pattern::escape(
            workspace_root.simplified().to_string_lossy().as_ref(),
        ))
        .join(member_glob.as_str());
        let absolute_glob = absolute_glob.to_string_lossy();
        let include_pattern = glob::Pattern::new(&absolute_glob)
            .map_err(|err| WorkspaceError::Pattern(absolute_glob.to_string(), err))?;
        if include_pattern.matches_path(project_path) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// A project that can be discovered.
///
/// The project could be a package within a workspace, a real workspace root, or a (legacy)
/// non-project workspace root, which can define its own dev dependencies.
#[derive(Debug, Clone)]
pub enum VirtualProject {
    /// A project (which could be a workspace root or member).
    Project(ProjectWorkspace),
    /// A (legacy) non-project workspace root.
    NonProject(Workspace),
}

impl VirtualProject {
    /// Find the current project or virtual workspace root, given the current directory.
    ///
    /// Similar to calling [`ProjectWorkspace::discover`] with a fallback to [`Workspace::discover`],
    /// but avoids rereading the `pyproject.toml` (and relying on error-handling as control flow).
    ///
    /// This method requires an absolute path and panics otherwise, i.e. this method only supports
    /// discovering the main workspace.
    pub async fn discover(
        path: &Path,
        options: &DiscoveryOptions,
        cache: &WorkspaceCache,
    ) -> Result<Self, WorkspaceError> {
        assert!(
            path.is_absolute(),
            "virtual project discovery with relative path"
        );
        let project_root = path
            .ancestors()
            .take_while(|path| {
                // Only walk up the given directory, if any.
                options
                    .stop_discovery_at
                    .as_deref()
                    .and_then(Path::parent)
                    .map(|stop_discovery_at| stop_discovery_at != *path)
                    .unwrap_or(true)
            })
            .find(|path| path.join("pyproject.toml").is_file())
            .ok_or(WorkspaceError::MissingPyprojectToml)?;

        debug!(
            "Found project root: `{}`",
            project_root.simplified_display()
        );

        // Read the current `pyproject.toml`.
        let pyproject_path = project_root.join("pyproject.toml");
        let contents = fs_err::tokio::read_to_string(&pyproject_path).await?;
        let pyproject_toml = PyProjectToml::from_string(contents)
            .map_err(|err| WorkspaceError::Toml(pyproject_path.clone(), Box::new(err)))?;

        if let Some(project) = pyproject_toml.project.as_ref() {
            // If the `pyproject.toml` contains a `[project]` table, it's a project.
            let project = ProjectWorkspace::from_project(
                project_root,
                project,
                &pyproject_toml,
                options,
                cache,
            )
            .await?;
            Ok(Self::Project(project))
        } else if let Some(workspace) = pyproject_toml
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.workspace.as_ref())
        {
            // Otherwise, if it contains a `tool.uv.workspace` table, it's a non-project workspace
            // root.
            let project_path = std::path::absolute(project_root)
                .map_err(WorkspaceError::Normalize)?
                .clone();

            let workspace = Workspace::collect_members(
                project_path,
                workspace.clone(),
                pyproject_toml,
                None,
                options,
                cache,
            )
            .await?;

            Ok(Self::NonProject(workspace))
        } else {
            Err(WorkspaceError::MissingProject(pyproject_path))
        }
    }

    /// Set the `pyproject.toml` for the current project.
    ///
    /// Assumes that the project name is unchanged in the updated [`PyProjectToml`].
    #[must_use]
    pub fn with_pyproject_toml(self, pyproject_toml: PyProjectToml) -> Option<Self> {
        match self {
            Self::Project(project) => {
                Some(Self::Project(project.with_pyproject_toml(pyproject_toml)?))
            }
            Self::NonProject(workspace) => {
                // If this is a non-project workspace root, then by definition the root isn't a
                // member, so we can just update the top-level `pyproject.toml`.
                Some(Self::NonProject(Workspace {
                    pyproject_toml,
                    ..workspace.clone()
                }))
            }
        }
    }

    /// Return the root of the project.
    pub fn root(&self) -> &Path {
        match self {
            Self::Project(project) => project.project_root(),
            Self::NonProject(workspace) => workspace.install_path(),
        }
    }

    /// Return the [`PyProjectToml`] of the project.
    pub fn pyproject_toml(&self) -> &PyProjectToml {
        match self {
            Self::Project(project) => project.current_project().pyproject_toml(),
            Self::NonProject(workspace) => &workspace.pyproject_toml,
        }
    }

    /// Return the [`Workspace`] of the project.
    pub fn workspace(&self) -> &Workspace {
        match self {
            Self::Project(project) => project.workspace(),
            Self::NonProject(workspace) => workspace,
        }
    }

    /// Return the [`PackageName`] of the project, if available.
    pub fn project_name(&self) -> Option<&PackageName> {
        match self {
            VirtualProject::Project(project) => Some(project.project_name()),
            VirtualProject::NonProject(_) => None,
        }
    }

    /// Returns `true` if the project is a virtual workspace root.
    pub fn is_non_project(&self) -> bool {
        matches!(self, VirtualProject::NonProject(_))
    }
}

#[cfg(test)]
#[cfg(unix)] // Avoid path escaping for the unit tests
mod tests {
    use std::env;
    use std::path::Path;
    use std::str::FromStr;

    use anyhow::Result;
    use assert_fs::fixture::ChildPath;
    use assert_fs::prelude::*;
    use insta::{assert_json_snapshot, assert_snapshot};

    use uv_normalize::GroupName;
    use uv_pypi_types::DependencyGroupSpecifier;

    use crate::pyproject::PyProjectToml;
    use crate::workspace::{DiscoveryOptions, ProjectWorkspace};
    use crate::{WorkspaceCache, WorkspaceError};

    async fn workspace_test(folder: &str) -> (ProjectWorkspace, String) {
        let root_dir = env::current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("scripts")
            .join("workspaces");
        let project = ProjectWorkspace::discover(
            &root_dir.join(folder),
            &DiscoveryOptions::default(),
            &WorkspaceCache::default(),
        )
        .await
        .unwrap();
        let root_escaped = regex::escape(root_dir.to_string_lossy().as_ref());
        (project, root_escaped)
    }

    async fn temporary_test(
        folder: &Path,
    ) -> Result<(ProjectWorkspace, String), (WorkspaceError, String)> {
        let root_escaped = regex::escape(folder.to_string_lossy().as_ref());
        let project = ProjectWorkspace::discover(
            folder,
            &DiscoveryOptions::default(),
            &WorkspaceCache::default(),
        )
        .await
        .map_err(|error| (error, root_escaped.clone()))?;

        Ok((project, root_escaped))
    }

    #[tokio::test]
    async fn albatross_in_example() {
        let (project, root_escaped) =
            workspace_test("albatross-in-example/examples/bird-feeder").await;
        let filters = vec![(root_escaped.as_str(), "[ROOT]")];
        insta::with_settings!({filters => filters}, {
        assert_json_snapshot!(
            project,
            {
                ".workspace.packages.*.pyproject_toml" => "[PYPROJECT_TOML]"
            },
            @r#"
        {
          "project_root": "[ROOT]/albatross-in-example/examples/bird-feeder",
          "project_name": "bird-feeder",
          "workspace": {
            "install_path": "[ROOT]/albatross-in-example/examples/bird-feeder",
            "packages": {
              "bird-feeder": {
                "root": "[ROOT]/albatross-in-example/examples/bird-feeder",
                "project": {
                  "name": "bird-feeder",
                  "version": "1.0.0",
                  "requires-python": ">=3.12",
                  "dependencies": [
                    "iniconfig>=2,<3"
                  ],
                  "optional-dependencies": null
                },
                "pyproject_toml": "[PYPROJECT_TOML]"
              }
            },
            "sources": {},
            "indexes": [],
            "pyproject_toml": {
              "project": {
                "name": "bird-feeder",
                "version": "1.0.0",
                "requires-python": ">=3.12",
                "dependencies": [
                  "iniconfig>=2,<3"
                ],
                "optional-dependencies": null
              },
              "tool": null,
              "dependency-groups": null
            }
          }
        }
        "#);
        });
    }

    #[tokio::test]
    async fn albatross_project_in_excluded() {
        let (project, root_escaped) =
            workspace_test("albatross-project-in-excluded/excluded/bird-feeder").await;
        let filters = vec![(root_escaped.as_str(), "[ROOT]")];
        insta::with_settings!({filters => filters}, {
            assert_json_snapshot!(
            project,
            {
                ".workspace.packages.*.pyproject_toml" => "[PYPROJECT_TOML]"
            },
            @r#"
            {
              "project_root": "[ROOT]/albatross-project-in-excluded/excluded/bird-feeder",
              "project_name": "bird-feeder",
              "workspace": {
                "install_path": "[ROOT]/albatross-project-in-excluded/excluded/bird-feeder",
                "packages": {
                  "bird-feeder": {
                    "root": "[ROOT]/albatross-project-in-excluded/excluded/bird-feeder",
                    "project": {
                      "name": "bird-feeder",
                      "version": "1.0.0",
                      "requires-python": ">=3.12",
                      "dependencies": [
                        "iniconfig>=2,<3"
                      ],
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  }
                },
                "sources": {},
                "indexes": [],
                "pyproject_toml": {
                  "project": {
                    "name": "bird-feeder",
                    "version": "1.0.0",
                    "requires-python": ">=3.12",
                    "dependencies": [
                      "iniconfig>=2,<3"
                    ],
                    "optional-dependencies": null
                  },
                  "tool": null,
                  "dependency-groups": null
                }
              }
            }
            "#);
        });
    }

    #[tokio::test]
    async fn albatross_root_workspace() {
        let (project, root_escaped) = workspace_test("albatross-root-workspace").await;
        let filters = vec![(root_escaped.as_str(), "[ROOT]")];
        insta::with_settings!({filters => filters}, {
            assert_json_snapshot!(
            project,
            {
                ".workspace.packages.*.pyproject_toml" => "[PYPROJECT_TOML]"
            },
            @r#"
            {
              "project_root": "[ROOT]/albatross-root-workspace",
              "project_name": "albatross",
              "workspace": {
                "install_path": "[ROOT]/albatross-root-workspace",
                "packages": {
                  "albatross": {
                    "root": "[ROOT]/albatross-root-workspace",
                    "project": {
                      "name": "albatross",
                      "version": "0.1.0",
                      "requires-python": ">=3.12",
                      "dependencies": [
                        "bird-feeder",
                        "iniconfig>=2,<3"
                      ],
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  },
                  "bird-feeder": {
                    "root": "[ROOT]/albatross-root-workspace/packages/bird-feeder",
                    "project": {
                      "name": "bird-feeder",
                      "version": "1.0.0",
                      "requires-python": ">=3.8",
                      "dependencies": [
                        "iniconfig>=2,<3",
                        "seeds"
                      ],
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  },
                  "seeds": {
                    "root": "[ROOT]/albatross-root-workspace/packages/seeds",
                    "project": {
                      "name": "seeds",
                      "version": "1.0.0",
                      "requires-python": ">=3.12",
                      "dependencies": [
                        "idna==3.6"
                      ],
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  }
                },
                "sources": {
                  "bird-feeder": [
                    {
                      "workspace": true,
                      "extra": null,
                      "group": null
                    }
                  ]
                },
                "indexes": [],
                "pyproject_toml": {
                  "project": {
                    "name": "albatross",
                    "version": "0.1.0",
                    "requires-python": ">=3.12",
                    "dependencies": [
                      "bird-feeder",
                      "iniconfig>=2,<3"
                    ],
                    "optional-dependencies": null
                  },
                  "tool": {
                    "uv": {
                      "sources": {
                        "bird-feeder": [
                          {
                            "workspace": true,
                            "extra": null,
                            "group": null
                          }
                        ]
                      },
                      "index": null,
                      "workspace": {
                        "members": [
                          "packages/*"
                        ],
                        "exclude": null
                      },
                      "managed": null,
                      "package": null,
                      "default-groups": null,
                      "dev-dependencies": null,
                      "override-dependencies": null,
                      "constraint-dependencies": null,
                      "build-constraint-dependencies": null,
                      "environments": null,
                      "required-environments": null,
                      "conflicts": null
                    }
                  },
                  "dependency-groups": null
                }
              }
            }
            "#);
        });
    }

    #[tokio::test]
    async fn albatross_virtual_workspace() {
        let (project, root_escaped) =
            workspace_test("albatross-virtual-workspace/packages/albatross").await;
        let filters = vec![(root_escaped.as_str(), "[ROOT]")];
        insta::with_settings!({filters => filters}, {
            assert_json_snapshot!(
            project,
            {
                ".workspace.packages.*.pyproject_toml" => "[PYPROJECT_TOML]"
            },
            @r#"
            {
              "project_root": "[ROOT]/albatross-virtual-workspace/packages/albatross",
              "project_name": "albatross",
              "workspace": {
                "install_path": "[ROOT]/albatross-virtual-workspace",
                "packages": {
                  "albatross": {
                    "root": "[ROOT]/albatross-virtual-workspace/packages/albatross",
                    "project": {
                      "name": "albatross",
                      "version": "0.1.0",
                      "requires-python": ">=3.12",
                      "dependencies": [
                        "bird-feeder",
                        "iniconfig>=2,<3"
                      ],
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  },
                  "bird-feeder": {
                    "root": "[ROOT]/albatross-virtual-workspace/packages/bird-feeder",
                    "project": {
                      "name": "bird-feeder",
                      "version": "1.0.0",
                      "requires-python": ">=3.12",
                      "dependencies": [
                        "anyio>=4.3.0,<5",
                        "seeds"
                      ],
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  },
                  "seeds": {
                    "root": "[ROOT]/albatross-virtual-workspace/packages/seeds",
                    "project": {
                      "name": "seeds",
                      "version": "1.0.0",
                      "requires-python": ">=3.12",
                      "dependencies": [
                        "idna==3.6"
                      ],
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  }
                },
                "sources": {},
                "indexes": [],
                "pyproject_toml": {
                  "project": null,
                  "tool": {
                    "uv": {
                      "sources": null,
                      "index": null,
                      "workspace": {
                        "members": [
                          "packages/*"
                        ],
                        "exclude": null
                      },
                      "managed": null,
                      "package": null,
                      "default-groups": null,
                      "dev-dependencies": null,
                      "override-dependencies": null,
                      "constraint-dependencies": null,
                      "build-constraint-dependencies": null,
                      "environments": null,
                      "required-environments": null,
                      "conflicts": null
                    }
                  },
                  "dependency-groups": null
                }
              }
            }
            "#);
        });
    }

    #[tokio::test]
    async fn albatross_just_project() {
        let (project, root_escaped) = workspace_test("albatross-just-project").await;
        let filters = vec![(root_escaped.as_str(), "[ROOT]")];
        insta::with_settings!({filters => filters}, {
            assert_json_snapshot!(
            project,
            {
                ".workspace.packages.*.pyproject_toml" => "[PYPROJECT_TOML]"
            },
            @r#"
            {
              "project_root": "[ROOT]/albatross-just-project",
              "project_name": "albatross",
              "workspace": {
                "install_path": "[ROOT]/albatross-just-project",
                "packages": {
                  "albatross": {
                    "root": "[ROOT]/albatross-just-project",
                    "project": {
                      "name": "albatross",
                      "version": "0.1.0",
                      "requires-python": ">=3.12",
                      "dependencies": [
                        "iniconfig>=2,<3"
                      ],
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  }
                },
                "sources": {},
                "indexes": [],
                "pyproject_toml": {
                  "project": {
                    "name": "albatross",
                    "version": "0.1.0",
                    "requires-python": ">=3.12",
                    "dependencies": [
                      "iniconfig>=2,<3"
                    ],
                    "optional-dependencies": null
                  },
                  "tool": null,
                  "dependency-groups": null
                }
              }
            }
            "#);
        });
    }

    #[tokio::test]
    async fn exclude_package() -> Result<()> {
        let root = tempfile::TempDir::new()?;
        let root = ChildPath::new(root.path());

        // Create the root.
        root.child("pyproject.toml").write_str(
            r#"
            [project]
            name = "albatross"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["tqdm>=4,<5"]

            [tool.uv.workspace]
            members = ["packages/*"]
            exclude = ["packages/bird-feeder"]

            [build-system]
            requires = ["hatchling"]
            build-backend = "hatchling.build"
            "#,
        )?;
        root.child("albatross").child("__init__.py").touch()?;

        // Create an included package (`seeds`).
        root.child("packages")
            .child("seeds")
            .child("pyproject.toml")
            .write_str(
                r#"
            [project]
            name = "seeds"
            version = "1.0.0"
            requires-python = ">=3.12"
            dependencies = ["idna==3.6"]

            [build-system]
            requires = ["hatchling"]
            build-backend = "hatchling.build"
            "#,
            )?;
        root.child("packages")
            .child("seeds")
            .child("seeds")
            .child("__init__.py")
            .touch()?;

        // Create an excluded package (`bird-feeder`).
        root.child("packages")
            .child("bird-feeder")
            .child("pyproject.toml")
            .write_str(
                r#"
            [project]
            name = "bird-feeder"
            version = "1.0.0"
            requires-python = ">=3.12"
            dependencies = ["anyio>=4.3.0,<5"]

            [build-system]
            requires = ["hatchling"]
            build-backend = "hatchling.build"
            "#,
            )?;
        root.child("packages")
            .child("bird-feeder")
            .child("bird_feeder")
            .child("__init__.py")
            .touch()?;

        let (project, root_escaped) = temporary_test(root.as_ref()).await.unwrap();
        let filters = vec![(root_escaped.as_str(), "[ROOT]")];
        insta::with_settings!({filters => filters}, {
            assert_json_snapshot!(
            project,
            {
                ".workspace.packages.*.pyproject_toml" => "[PYPROJECT_TOML]"
            },
            @r###"
            {
              "project_root": "[ROOT]",
              "project_name": "albatross",
              "workspace": {
                "install_path": "[ROOT]",
                "packages": {
                  "albatross": {
                    "root": "[ROOT]",
                    "project": {
                      "name": "albatross",
                      "version": "0.1.0",
                      "requires-python": ">=3.12",
                      "dependencies": [
                        "tqdm>=4,<5"
                      ],
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  },
                  "seeds": {
                    "root": "[ROOT]/packages/seeds",
                    "project": {
                      "name": "seeds",
                      "version": "1.0.0",
                      "requires-python": ">=3.12",
                      "dependencies": [
                        "idna==3.6"
                      ],
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  }
                },
                "sources": {},
                "indexes": [],
                "pyproject_toml": {
                  "project": {
                    "name": "albatross",
                    "version": "0.1.0",
                    "requires-python": ">=3.12",
                    "dependencies": [
                      "tqdm>=4,<5"
                    ],
                    "optional-dependencies": null
                  },
                  "tool": {
                    "uv": {
                      "sources": null,
                      "index": null,
                      "workspace": {
                        "members": [
                          "packages/*"
                        ],
                        "exclude": [
                          "packages/bird-feeder"
                        ]
                      },
                      "managed": null,
                      "package": null,
                      "default-groups": null,
                      "dev-dependencies": null,
                      "override-dependencies": null,
                      "constraint-dependencies": null,
                      "build-constraint-dependencies": null,
                      "environments": null,
                      "required-environments": null,
                      "conflicts": null
                    }
                  },
                  "dependency-groups": null
                }
              }
            }
            "###);
        });

        // Rewrite the members to both include and exclude `bird-feeder` by name.
        root.child("pyproject.toml").write_str(
            r#"
            [project]
            name = "albatross"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["tqdm>=4,<5"]

            [tool.uv.workspace]
            members = ["packages/seeds", "packages/bird-feeder"]
            exclude = ["packages/bird-feeder"]

            [build-system]
            requires = ["hatchling"]
            build-backend = "hatchling.build"
            "#,
        )?;

        // `bird-feeder` should still be excluded.
        let (project, root_escaped) = temporary_test(root.as_ref()).await.unwrap();
        let filters = vec![(root_escaped.as_str(), "[ROOT]")];
        insta::with_settings!({filters => filters}, {
            assert_json_snapshot!(
            project,
            {
                ".workspace.packages.*.pyproject_toml" => "[PYPROJECT_TOML]"
            },
            @r###"
            {
              "project_root": "[ROOT]",
              "project_name": "albatross",
              "workspace": {
                "install_path": "[ROOT]",
                "packages": {
                  "albatross": {
                    "root": "[ROOT]",
                    "project": {
                      "name": "albatross",
                      "version": "0.1.0",
                      "requires-python": ">=3.12",
                      "dependencies": [
                        "tqdm>=4,<5"
                      ],
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  },
                  "seeds": {
                    "root": "[ROOT]/packages/seeds",
                    "project": {
                      "name": "seeds",
                      "version": "1.0.0",
                      "requires-python": ">=3.12",
                      "dependencies": [
                        "idna==3.6"
                      ],
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  }
                },
                "sources": {},
                "indexes": [],
                "pyproject_toml": {
                  "project": {
                    "name": "albatross",
                    "version": "0.1.0",
                    "requires-python": ">=3.12",
                    "dependencies": [
                      "tqdm>=4,<5"
                    ],
                    "optional-dependencies": null
                  },
                  "tool": {
                    "uv": {
                      "sources": null,
                      "index": null,
                      "workspace": {
                        "members": [
                          "packages/seeds",
                          "packages/bird-feeder"
                        ],
                        "exclude": [
                          "packages/bird-feeder"
                        ]
                      },
                      "managed": null,
                      "package": null,
                      "default-groups": null,
                      "dev-dependencies": null,
                      "override-dependencies": null,
                      "constraint-dependencies": null,
                      "build-constraint-dependencies": null,
                      "environments": null,
                      "required-environments": null,
                      "conflicts": null
                    }
                  },
                  "dependency-groups": null
                }
              }
            }
            "###);
        });

        // Rewrite the exclusion to use the top-level directory (`packages`).
        root.child("pyproject.toml").write_str(
            r#"
            [project]
            name = "albatross"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["tqdm>=4,<5"]

            [tool.uv.workspace]
            members = ["packages/seeds", "packages/bird-feeder"]
            exclude = ["packages"]

            [build-system]
            requires = ["hatchling"]
            build-backend = "hatchling.build"
            "#,
        )?;

        // `bird-feeder` should now be included.
        let (project, root_escaped) = temporary_test(root.as_ref()).await.unwrap();
        let filters = vec![(root_escaped.as_str(), "[ROOT]")];
        insta::with_settings!({filters => filters}, {
            assert_json_snapshot!(
            project,
            {
                ".workspace.packages.*.pyproject_toml" => "[PYPROJECT_TOML]"
            },
            @r###"
            {
              "project_root": "[ROOT]",
              "project_name": "albatross",
              "workspace": {
                "install_path": "[ROOT]",
                "packages": {
                  "albatross": {
                    "root": "[ROOT]",
                    "project": {
                      "name": "albatross",
                      "version": "0.1.0",
                      "requires-python": ">=3.12",
                      "dependencies": [
                        "tqdm>=4,<5"
                      ],
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  },
                  "bird-feeder": {
                    "root": "[ROOT]/packages/bird-feeder",
                    "project": {
                      "name": "bird-feeder",
                      "version": "1.0.0",
                      "requires-python": ">=3.12",
                      "dependencies": [
                        "anyio>=4.3.0,<5"
                      ],
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  },
                  "seeds": {
                    "root": "[ROOT]/packages/seeds",
                    "project": {
                      "name": "seeds",
                      "version": "1.0.0",
                      "requires-python": ">=3.12",
                      "dependencies": [
                        "idna==3.6"
                      ],
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  }
                },
                "sources": {},
                "indexes": [],
                "pyproject_toml": {
                  "project": {
                    "name": "albatross",
                    "version": "0.1.0",
                    "requires-python": ">=3.12",
                    "dependencies": [
                      "tqdm>=4,<5"
                    ],
                    "optional-dependencies": null
                  },
                  "tool": {
                    "uv": {
                      "sources": null,
                      "index": null,
                      "workspace": {
                        "members": [
                          "packages/seeds",
                          "packages/bird-feeder"
                        ],
                        "exclude": [
                          "packages"
                        ]
                      },
                      "managed": null,
                      "package": null,
                      "default-groups": null,
                      "dev-dependencies": null,
                      "override-dependencies": null,
                      "constraint-dependencies": null,
                      "build-constraint-dependencies": null,
                      "environments": null,
                      "required-environments": null,
                      "conflicts": null
                    }
                  },
                  "dependency-groups": null
                }
              }
            }
            "###);
        });

        // Rewrite the exclusion to use the top-level directory with a glob (`packages/*`).
        root.child("pyproject.toml").write_str(
            r#"
            [project]
            name = "albatross"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["tqdm>=4,<5"]

            [tool.uv.workspace]
            members = ["packages/seeds", "packages/bird-feeder"]
            exclude = ["packages/*"]

            [build-system]
            requires = ["hatchling"]
            build-backend = "hatchling.build"
            "#,
        )?;

        // `bird-feeder` and `seeds` should now be excluded.
        let (project, root_escaped) = temporary_test(root.as_ref()).await.unwrap();
        let filters = vec![(root_escaped.as_str(), "[ROOT]")];
        insta::with_settings!({filters => filters}, {
            assert_json_snapshot!(
            project,
            {
                ".workspace.packages.*.pyproject_toml" => "[PYPROJECT_TOML]"
            },
            @r###"
            {
              "project_root": "[ROOT]",
              "project_name": "albatross",
              "workspace": {
                "install_path": "[ROOT]",
                "packages": {
                  "albatross": {
                    "root": "[ROOT]",
                    "project": {
                      "name": "albatross",
                      "version": "0.1.0",
                      "requires-python": ">=3.12",
                      "dependencies": [
                        "tqdm>=4,<5"
                      ],
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  }
                },
                "sources": {},
                "indexes": [],
                "pyproject_toml": {
                  "project": {
                    "name": "albatross",
                    "version": "0.1.0",
                    "requires-python": ">=3.12",
                    "dependencies": [
                      "tqdm>=4,<5"
                    ],
                    "optional-dependencies": null
                  },
                  "tool": {
                    "uv": {
                      "sources": null,
                      "index": null,
                      "workspace": {
                        "members": [
                          "packages/seeds",
                          "packages/bird-feeder"
                        ],
                        "exclude": [
                          "packages/*"
                        ]
                      },
                      "managed": null,
                      "package": null,
                      "default-groups": null,
                      "dev-dependencies": null,
                      "override-dependencies": null,
                      "constraint-dependencies": null,
                      "build-constraint-dependencies": null,
                      "environments": null,
                      "required-environments": null,
                      "conflicts": null
                    }
                  },
                  "dependency-groups": null
                }
              }
            }
            "###);
        });

        Ok(())
    }

    #[test]
    fn read_dependency_groups() {
        let toml = r#"
[dependency-groups]
foo = ["a", {include-group = "bar"}]
bar = ["b"]
"#;

        let result =
            PyProjectToml::from_string(toml.to_string()).expect("Deserialization should succeed");

        let groups = result
            .dependency_groups
            .expect("`dependency-groups` should be present");
        let foo = groups
            .get(&GroupName::from_str("foo").unwrap())
            .expect("Group `foo` should be present");
        assert_eq!(
            foo,
            &[
                DependencyGroupSpecifier::Requirement("a".to_string()),
                DependencyGroupSpecifier::IncludeGroup {
                    include_group: GroupName::from_str("bar").unwrap(),
                }
            ]
        );

        let bar = groups
            .get(&GroupName::from_str("bar").unwrap())
            .expect("Group `bar` should be present");
        assert_eq!(
            bar,
            &[DependencyGroupSpecifier::Requirement("b".to_string())]
        );
    }

    #[tokio::test]
    async fn nested_workspace() -> Result<()> {
        let root = tempfile::TempDir::new()?;
        let root = ChildPath::new(root.path());

        // Create the root.
        root.child("pyproject.toml").write_str(
            r#"
            [project]
            name = "albatross"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["tqdm>=4,<5"]

            [tool.uv.workspace]
            members = ["packages/*"]
            "#,
        )?;

        // Create an included package (`seeds`).
        root.child("packages")
            .child("seeds")
            .child("pyproject.toml")
            .write_str(
                r#"
            [project]
            name = "seeds"
            version = "1.0.0"
            requires-python = ">=3.12"
            dependencies = ["idna==3.6"]

            [tool.uv.workspace]
            members = ["nested_packages/*"]
            "#,
            )?;

        let (error, root_escaped) = temporary_test(root.as_ref()).await.unwrap_err();
        let filters = vec![(root_escaped.as_str(), "[ROOT]")];
        insta::with_settings!({filters => filters}, {
            assert_snapshot!(
                error,
            @"Nested workspaces are not supported, but workspace member (`[ROOT]/packages/seeds`) has a `uv.workspace` table");
        });

        Ok(())
    }

    #[tokio::test]
    async fn duplicate_names() -> Result<()> {
        let root = tempfile::TempDir::new()?;
        let root = ChildPath::new(root.path());

        // Create the root.
        root.child("pyproject.toml").write_str(
            r#"
            [project]
            name = "albatross"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["tqdm>=4,<5"]

            [tool.uv.workspace]
            members = ["packages/*"]
            "#,
        )?;

        // Create an included package (`seeds`).
        root.child("packages")
            .child("seeds")
            .child("pyproject.toml")
            .write_str(
                r#"
            [project]
            name = "seeds"
            version = "1.0.0"
            requires-python = ">=3.12"
            dependencies = ["idna==3.6"]

            [tool.uv.workspace]
            members = ["nested_packages/*"]
            "#,
            )?;

        // Create an included package (`seeds2`).
        root.child("packages")
            .child("seeds2")
            .child("pyproject.toml")
            .write_str(
                r#"
            [project]
            name = "seeds"
            version = "1.0.0"
            requires-python = ">=3.12"
            dependencies = ["idna==3.6"]

            [tool.uv.workspace]
            members = ["nested_packages/*"]
            "#,
            )?;

        let (error, root_escaped) = temporary_test(root.as_ref()).await.unwrap_err();
        let filters = vec![(root_escaped.as_str(), "[ROOT]")];
        insta::with_settings!({filters => filters}, {
            assert_snapshot!(
                error,
            @"Two workspace members are both named: `seeds`: `[ROOT]/packages/seeds` and `[ROOT]/packages/seeds2`");
        });

        Ok(())
    }
}
