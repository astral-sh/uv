//! Resolve the current [`ProjectWorkspace`] or [`Workspace`].

use either::Either;
use glob::{glob, GlobError, PatternError};
use rustc_hash::FxHashSet;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tracing::{debug, trace, warn};

use pep508_rs::{MarkerTree, RequirementOrigin, VerbatimUrl};
use pypi_types::{Requirement, RequirementSource, SupportedEnvironments, VerbatimParsedUrl};
use uv_fs::{Simplified, CWD};
use uv_normalize::{GroupName, PackageName, DEV_DEPENDENCIES};
use uv_warnings::{warn_user, warn_user_once};

use crate::pyproject::{
    Project, PyProjectToml, PyprojectTomlError, Source, ToolUvSources, ToolUvWorkspace,
};

#[derive(thiserror::Error, Debug)]
pub enum WorkspaceError {
    // Workspace structure errors.
    #[error("No `pyproject.toml` found in current directory or any parent directory")]
    MissingPyprojectToml,
    #[error("Workspace member `{}` is missing a `pyproject.toml` (matches: `{1}`)", _0.simplified_display())]
    MissingPyprojectTomlMember(PathBuf, String),
    #[error("No `project` table found in: `{}`", _0.simplified_display())]
    MissingProject(PathBuf),
    #[error("No workspace found for: `{}`", _0.simplified_display())]
    MissingWorkspace(PathBuf),
    #[error("The project is marked as unmanaged: `{}`", _0.simplified_display())]
    NonWorkspace(PathBuf),
    #[error("pyproject.toml section is declared as dynamic, but must be static: `{0}`")]
    DynamicNotAllowed(&'static str),
    #[error("Failed to find directories for glob: `{0}`")]
    Pattern(String, #[source] PatternError),
    // Syntax and other errors.
    #[error("Invalid glob in `tool.uv.workspace.members`: `{0}`")]
    Glob(String, #[source] GlobError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Failed to parse: `{}`", _0.user_display())]
    Toml(PathBuf, #[source] Box<PyprojectTomlError>),
    #[error("Failed to normalize workspace member path")]
    Normalize(#[source] std::io::Error),
}

#[derive(Debug, Default, Clone)]
pub enum MemberDiscovery<'a> {
    /// Discover all workspace members.
    #[default]
    All,
    /// Don't discover any workspace members.
    None,
    /// Discover workspace members, but ignore the given paths.
    Ignore(FxHashSet<&'a Path>),
}

#[derive(Debug, Default, Clone)]
pub struct DiscoveryOptions<'a> {
    /// The path to stop discovery at.
    pub stop_discovery_at: Option<&'a Path>,
    /// The strategy to use when discovering workspace members.
    pub members: MemberDiscovery<'a>,
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
    packages: BTreeMap<PackageName, WorkspaceMember>,
    /// The sources table from the workspace `pyproject.toml`.
    ///
    /// This table is overridden by the project sources.
    sources: BTreeMap<PackageName, Source>,
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
        options: &DiscoveryOptions<'_>,
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

        check_nested_workspaces(&workspace_root, options);

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
        let member = packages.get_mut(package_name)?;

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

    /// Returns the set of requirements that include all packages in the workspace.
    pub fn members_requirements(&self) -> impl Iterator<Item = Requirement> + '_ {
        self.packages.values().filter_map(|member| {
            let project = member.pyproject_toml.project.as_ref()?;
            // Extract the extras available in the project.
            let extras = project
                .optional_dependencies
                .as_ref()
                .map(|optional_dependencies| {
                    // It's a `BTreeMap` so the keys are sorted.
                    optional_dependencies
                        .iter()
                        .filter_map(|(name, dependencies)| {
                            if dependencies.is_empty() {
                                None
                            } else {
                                Some(name)
                            }
                        })
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            let url = VerbatimUrl::from_absolute_path(&member.root)
                .expect("path is valid URL")
                .with_given(member.root.to_string_lossy());
            Some(Requirement {
                name: project.name.clone(),
                extras,
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

    /// Returns any requirements that are exclusive to the workspace root, i.e., not included in
    /// any of the workspace members.
    ///
    /// For workspaces with non-project roots, returns the dev dependencies in the corresponding
    /// `pyproject.toml`.
    ///
    /// Otherwise, returns an empty list.
    pub fn non_project_requirements(&self) -> impl Iterator<Item = Requirement> + '_ {
        if self
            .packages
            .values()
            .any(|member| *member.root() == self.install_path)
        {
            // If the workspace has an explicit root, the root is a member, so we don't need to
            // include any root-only requirements.
            Either::Left(std::iter::empty())
        } else {
            // Otherwise, return the dev dependencies in the non-project workspace root.
            Either::Right(
                self.pyproject_toml
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.uv.as_ref())
                    .and_then(|uv| uv.dev_dependencies.as_ref())
                    .into_iter()
                    .flatten()
                    .map(|requirement| {
                        Requirement::from(
                            requirement
                                .clone()
                                .with_origin(RequirementOrigin::Workspace),
                        )
                    }),
            )
        }
    }

    /// Returns the set of overrides for the workspace.
    pub fn overrides(&self) -> Vec<Requirement> {
        let Some(workspace_package) = self
            .packages
            .values()
            .find(|workspace_package| workspace_package.root() == self.install_path())
        else {
            return vec![];
        };

        let Some(overrides) = workspace_package
            .pyproject_toml()
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.override_dependencies.as_ref())
        else {
            return vec![];
        };

        overrides
            .iter()
            .map(|requirement| {
                Requirement::from(
                    requirement
                        .clone()
                        .with_origin(RequirementOrigin::Workspace),
                )
            })
            .collect()
    }

    /// Returns the set of supported environments for the workspace.
    pub fn environments(&self) -> Option<&SupportedEnvironments> {
        let workspace_package = self
            .packages
            .values()
            .find(|workspace_package| workspace_package.root() == self.install_path())?;

        workspace_package
            .pyproject_toml()
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.environments.as_ref())
    }

    /// Returns the set of constraints for the workspace.
    pub fn constraints(&self) -> Vec<Requirement> {
        let Some(workspace_package) = self
            .packages
            .values()
            .find(|workspace_package| workspace_package.root() == self.install_path())
        else {
            return vec![];
        };

        let Some(constraints) = workspace_package
            .pyproject_toml()
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.constraint_dependencies.as_ref())
        else {
            return vec![];
        };

        constraints
            .iter()
            .map(|requirement| {
                Requirement::from(
                    requirement
                        .clone()
                        .with_origin(RequirementOrigin::Workspace),
                )
            })
            .collect()
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
    pub fn venv(&self) -> PathBuf {
        /// Resolve the `UV_PROJECT_ENVIRONMENT` value, if any.
        fn from_project_environment_variable(workspace: &Workspace) -> Option<PathBuf> {
            let value = std::env::var_os("UV_PROJECT_ENVIRONMENT")?;

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

        // Resolve the `VIRTUAL_ENV` variable, if any.
        fn from_virtual_env_variable() -> Option<PathBuf> {
            let value = std::env::var_os("VIRTUAL_ENV")?;

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

        // Attempt to check if the two paths refer to the same directory.
        fn is_same_dir(left: &Path, right: &Path) -> Option<bool> {
            // First, attempt to check directly
            if let Ok(value) = same_file::is_same_file(left, right) {
                return Some(value);
            };

            // Often, one of the directories won't exist yet so perform the comparison up a level
            if let (Some(left_parent), Some(right_parent), Some(left_name), Some(right_name)) = (
                left.parent(),
                right.parent(),
                left.file_name(),
                right.file_name(),
            ) {
                match same_file::is_same_file(left_parent, right_parent) {
                    Ok(true) => return Some(left_name == right_name),
                    Ok(false) => return Some(false),
                    _ => (),
                }
            };

            // We couldn't determine if they're the same
            None
        }

        // Determine the default value
        let project_env = from_project_environment_variable(self)
            .unwrap_or_else(|| self.install_path.join(".venv"));

        // Warn if it conflicts with `VIRTUAL_ENV`
        if let Some(from_virtual_env) = from_virtual_env_variable() {
            if !is_same_dir(&from_virtual_env, &project_env).unwrap_or(false) {
                warn_user_once!(
                    "`VIRTUAL_ENV={}` does not match the project environment path `{}` and will be ignored",
                    from_virtual_env.user_display(),
                    project_env.user_display()
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
    pub fn sources(&self) -> &BTreeMap<PackageName, Source> {
        &self.sources
    }

    /// Returns an iterator over all sources in the workspace.
    pub fn iter_sources(&self) -> impl Iterator<Item = &Source> {
        self.packages
            .values()
            .filter_map(|member| {
                member.pyproject_toml().tool.as_ref().and_then(|tool| {
                    tool.uv
                        .as_ref()
                        .and_then(|uv| uv.sources.as_ref())
                        .map(ToolUvSources::inner)
                        .map(|sources| sources.values())
                })
            })
            .flatten()
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
        options: &DiscoveryOptions<'_>,
    ) -> Result<Workspace, WorkspaceError> {
        let mut workspace_members = BTreeMap::new();
        // Avoid reading a `pyproject.toml` more than once.
        let mut seen = FxHashSet::default();

        // Add the project at the workspace root, if it exists and if it's distinct from the current
        // project.
        if current_project
            .as_ref()
            .map(|root_member| root_member.root != workspace_root)
            .unwrap_or(true)
        {
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
            };
        }

        // The current project is a workspace member, especially in a single project workspace.
        if let Some(root_member) = current_project {
            debug!(
                "Adding current workspace member: `{}`",
                root_member.root.simplified_display()
            );

            seen.insert(root_member.root.clone());
            workspace_members.insert(root_member.project.name.clone(), root_member);
        }

        // Add all other workspace members.
        for member_glob in workspace_definition.clone().members.unwrap_or_default() {
            let absolute_glob = workspace_root
                .simplified()
                .join(member_glob.as_str())
                .to_string_lossy()
                .to_string();
            for member_root in glob(&absolute_glob)
                .map_err(|err| WorkspaceError::Pattern(absolute_glob.to_string(), err))?
            {
                let member_root = member_root
                    .map_err(|err| WorkspaceError::Glob(absolute_glob.to_string(), err))?;
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
                if is_excluded_from_workspace(&member_root, &workspace_root, &workspace_definition)?
                {
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
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
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
                    // If the entry is _not_ a directory, skip it.
                    Err(_) if !member_root.is_dir() => {
                        warn!(
                            "Ignoring non-directory workspace member: `{}`",
                            member_root.simplified_display()
                        );
                        continue;
                    }
                    Err(err) => return Err(err.into()),
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
                workspace_members.insert(
                    project.name.clone(),
                    WorkspaceMember {
                        root: member_root.clone(),
                        project,
                        pyproject_toml,
                    },
                );
            }
        }
        let workspace_sources = workspace_pyproject_toml
            .tool
            .clone()
            .and_then(|tool| tool.uv)
            .and_then(|uv| uv.sources)
            .map(ToolUvSources::into_inner)
            .unwrap_or_default();

        Ok(Workspace {
            install_path: workspace_root,
            packages: workspace_members,
            sources: workspace_sources,
            pyproject_toml: workspace_pyproject_toml,
        })
    }
}

/// A project in a workspace.
#[derive(Debug, Clone)]
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
        options: &DiscoveryOptions<'_>,
    ) -> Result<Self, WorkspaceError> {
        let project_root = path
            .ancestors()
            .take_while(|path| {
                // Only walk up the given directory, if any.
                options
                    .stop_discovery_at
                    .map(|stop_discovery_at| stop_discovery_at != *path)
                    .unwrap_or(true)
            })
            .find(|path| path.join("pyproject.toml").is_file())
            .ok_or(WorkspaceError::MissingPyprojectToml)?;

        debug!(
            "Found project root: `{}`",
            project_root.simplified_display()
        );

        Self::from_project_root(project_root, options).await
    }

    /// Discover the workspace starting from the directory containing the `pyproject.toml`.
    async fn from_project_root(
        project_root: &Path,
        options: &DiscoveryOptions<'_>,
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
            .ok_or_else(|| WorkspaceError::MissingProject(pyproject_path))?;

        Self::from_project(project_root, &project, &pyproject_toml, options).await
    }

    /// If the current directory contains a `pyproject.toml` with a `project` table, discover the
    /// workspace and return it, otherwise it is a dynamic path dependency and we return `Ok(None)`.
    pub async fn from_maybe_project_root(
        install_path: &Path,
        options: &DiscoveryOptions<'_>,
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

        match Self::from_project(install_path, &project, &pyproject_toml, options).await {
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
        options: &DiscoveryOptions<'_>,
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

            let current_project_as_members =
                BTreeMap::from_iter([(project.name.clone(), current_project)]);
            return Ok(Self {
                project_root: project_path.clone(),
                project_name: project.name.clone(),
                workspace: Workspace {
                    install_path: project_path.clone(),
                    packages: current_project_as_members,
                    // There may be package sources, but we don't need to duplicate them into the
                    // workspace sources.
                    sources: BTreeMap::default(),
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
    options: &DiscoveryOptions<'_>,
) -> Result<Option<(PathBuf, ToolUvWorkspace, PyProjectToml)>, WorkspaceError> {
    // Skip 1 to ignore the current project itself.
    for workspace_root in project_root
        .ancestors()
        .take_while(|path| {
            // Only walk up the given directory, if any.
            options
                .stop_discovery_at
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

/// Warn when the valid workspace is included in another workspace.
pub fn check_nested_workspaces(inner_workspace_root: &Path, options: &DiscoveryOptions) {
    for outer_workspace_root in inner_workspace_root
        .ancestors()
        .take_while(|path| {
            // Only walk up the given directory, if any.
            options
                .stop_discovery_at
                .map(|stop_discovery_at| stop_discovery_at != *path)
                .unwrap_or(true)
        })
        .skip(1)
    {
        let pyproject_toml_path = outer_workspace_root.join("pyproject.toml");
        if !pyproject_toml_path.is_file() {
            continue;
        }
        let contents = match fs_err::read_to_string(&pyproject_toml_path) {
            Ok(contents) => contents,
            Err(err) => {
                warn!(
                    "Unreadable pyproject.toml `{}`: {err}",
                    pyproject_toml_path.simplified_display()
                );
                return;
            }
        };
        let pyproject_toml: PyProjectToml = match toml::from_str(&contents) {
            Ok(contents) => contents,
            Err(err) => {
                warn!(
                    "Invalid pyproject.toml `{}`: {err}",
                    pyproject_toml_path.simplified_display()
                );
                return;
            }
        };

        if let Some(workspace) = pyproject_toml
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.workspace.as_ref())
        {
            let is_included = match is_included_in_workspace(
                inner_workspace_root,
                outer_workspace_root,
                workspace,
            ) {
                Ok(contents) => contents,
                Err(err) => {
                    warn!(
                        "Invalid pyproject.toml `{}`: {err}",
                        pyproject_toml_path.simplified_display()
                    );
                    return;
                }
            };

            let is_excluded = match is_excluded_from_workspace(
                inner_workspace_root,
                outer_workspace_root,
                workspace,
            ) {
                Ok(contents) => contents,
                Err(err) => {
                    warn!(
                        "Invalid pyproject.toml `{}`: {err}",
                        pyproject_toml_path.simplified_display()
                    );
                    return;
                }
            };

            if is_included && !is_excluded {
                warn_user!(
                    "Nested workspaces are not supported, but outer workspace (`{}`) includes `{}`",
                    outer_workspace_root.simplified_display().cyan(),
                    inner_workspace_root.simplified_display().cyan()
                );
            }
        }

        // We're in the examples or tests of another project (not a workspace), this is fine.
        return;
    }
}

/// Check if we're in the `tool.uv.workspace.excluded` of a workspace.
fn is_excluded_from_workspace(
    project_path: &Path,
    workspace_root: &Path,
    workspace: &ToolUvWorkspace,
) -> Result<bool, WorkspaceError> {
    for exclude_glob in workspace.exclude.iter().flatten() {
        let absolute_glob = workspace_root
            .simplified()
            .join(exclude_glob.as_str())
            .to_string_lossy()
            .to_string();
        for excluded_root in glob(&absolute_glob)
            .map_err(|err| WorkspaceError::Pattern(absolute_glob.to_string(), err))?
        {
            let excluded_root = excluded_root
                .map_err(|err| WorkspaceError::Glob(absolute_glob.to_string(), err))?;
            if excluded_root == project_path.simplified() {
                return Ok(true);
            }
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
        let absolute_glob = workspace_root
            .simplified()
            .join(member_glob.as_str())
            .to_string_lossy()
            .to_string();
        for member_root in glob(&absolute_glob)
            .map_err(|err| WorkspaceError::Pattern(absolute_glob.to_string(), err))?
        {
            let member_root =
                member_root.map_err(|err| WorkspaceError::Glob(absolute_glob.to_string(), err))?;
            if member_root == project_path {
                return Ok(true);
            }
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
        options: &DiscoveryOptions<'_>,
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
            let project =
                ProjectWorkspace::from_project(project_root, project, &pyproject_toml, options)
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

            check_nested_workspaces(&project_path, options);

            let workspace = Workspace::collect_members(
                project_path,
                workspace.clone(),
                pyproject_toml,
                None,
                options,
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

/// A target that can be installed.
#[derive(Debug, Clone, Copy)]
pub enum InstallTarget<'env> {
    /// A project (which could be a workspace root or member).
    Project(&'env ProjectWorkspace),
    /// A (legacy) non-project workspace root.
    NonProject(&'env Workspace),
    /// A frozen member within a [`Workspace`].
    FrozenMember(&'env Workspace, &'env PackageName),
}

impl<'env> InstallTarget<'env> {
    /// Create an [`InstallTarget`] for a frozen member within a workspace.
    pub fn frozen_member(project: &'env VirtualProject, package_name: &'env PackageName) -> Self {
        Self::FrozenMember(project.workspace(), package_name)
    }

    /// Return the [`Workspace`] of the target.
    pub fn workspace(&self) -> &Workspace {
        match self {
            Self::Project(project) => project.workspace(),
            Self::NonProject(workspace) => workspace,
            Self::FrozenMember(workspace, _) => workspace,
        }
    }

    /// Return the [`PackageName`] of the target.
    pub fn packages(&self) -> impl Iterator<Item = &PackageName> {
        match self {
            Self::Project(project) => Either::Left(std::iter::once(project.project_name())),
            Self::NonProject(workspace) => Either::Right(workspace.packages().keys()),
            Self::FrozenMember(_, package_name) => Either::Left(std::iter::once(*package_name)),
        }
    }

    /// Return the [`InstallTarget`] dependencies for the given group name.
    ///
    /// Returns dependencies that apply to the workspace root, but not any of its members. As such,
    /// only returns a non-empty iterator for virtual workspaces, which can include dev dependencies
    /// on the virtual root.
    pub fn group(
        &self,
        name: &GroupName,
    ) -> impl Iterator<Item = &pep508_rs::Requirement<VerbatimParsedUrl>> {
        match self {
            Self::Project(_) | Self::FrozenMember(..) => {
                // For projects, dev dependencies are attached to the members.
                Either::Left(std::iter::empty())
            }
            Self::NonProject(workspace) => {
                // For non-projects, we might have dev dependencies that are attached to the
                // workspace root (which isn't a member).
                if name == &*DEV_DEPENDENCIES {
                    Either::Right(
                        workspace
                            .pyproject_toml
                            .tool
                            .as_ref()
                            .and_then(|tool| tool.uv.as_ref())
                            .and_then(|uv| uv.dev_dependencies.as_ref())
                            .map(|dev| dev.iter())
                            .into_iter()
                            .flatten(),
                    )
                } else {
                    Either::Left(std::iter::empty())
                }
            }
        }
    }

    /// Return the [`PackageName`] of the target, if available.
    pub fn project_name(&self) -> Option<&PackageName> {
        match self {
            Self::Project(project) => Some(project.project_name()),
            Self::NonProject(_) => None,
            Self::FrozenMember(_, package_name) => Some(package_name),
        }
    }
}

impl<'env> From<&'env VirtualProject> for InstallTarget<'env> {
    fn from(project: &'env VirtualProject) -> Self {
        match project {
            VirtualProject::Project(project) => Self::Project(project),
            VirtualProject::NonProject(workspace) => Self::NonProject(workspace),
        }
    }
}

#[cfg(test)]
#[cfg(unix)] // Avoid path escaping for the unit tests
mod tests {
    use std::env;

    use std::path::Path;

    use anyhow::Result;
    use assert_fs::fixture::ChildPath;
    use assert_fs::prelude::*;
    use insta::assert_json_snapshot;

    use crate::workspace::{DiscoveryOptions, ProjectWorkspace};

    async fn workspace_test(folder: &str) -> (ProjectWorkspace, String) {
        let root_dir = env::current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("scripts")
            .join("workspaces");
        let project =
            ProjectWorkspace::discover(&root_dir.join(folder), &DiscoveryOptions::default())
                .await
                .unwrap();
        let root_escaped = regex::escape(root_dir.to_string_lossy().as_ref());
        (project, root_escaped)
    }

    async fn temporary_test(folder: &Path) -> (ProjectWorkspace, String) {
        let project = ProjectWorkspace::discover(folder, &DiscoveryOptions::default())
            .await
            .unwrap();
        let root_escaped = regex::escape(folder.to_string_lossy().as_ref());
        (project, root_escaped)
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
            @r###"
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
                    "anyio>=4.3.0,<5"
                  ],
                  "optional-dependencies": null
                },
                "pyproject_toml": "[PYPROJECT_TOML]"
              }
            },
            "sources": {},
            "pyproject_toml": {
              "project": {
                "name": "bird-feeder",
                "version": "1.0.0",
                "requires-python": ">=3.12",
                "dependencies": [
                  "anyio>=4.3.0,<5"
                ],
                "optional-dependencies": null
              },
              "tool": null
            }
          }
        }
        "###);
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
            @r###"
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
                        "anyio>=4.3.0,<5"
                      ],
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  }
                },
                "sources": {},
                "pyproject_toml": {
                  "project": {
                    "name": "bird-feeder",
                    "version": "1.0.0",
                    "requires-python": ">=3.12",
                    "dependencies": [
                      "anyio>=4.3.0,<5"
                    ],
                    "optional-dependencies": null
                  },
                  "tool": null
                }
              }
            }
            "###);
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
            @r###"
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
                        "tqdm>=4,<5"
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
                        "anyio>=4.3.0,<5",
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
                  "bird-feeder": {
                    "workspace": true
                  }
                },
                "pyproject_toml": {
                  "project": {
                    "name": "albatross",
                    "version": "0.1.0",
                    "requires-python": ">=3.12",
                    "dependencies": [
                      "bird-feeder",
                      "tqdm>=4,<5"
                    ],
                    "optional-dependencies": null
                  },
                  "tool": {
                    "uv": {
                      "sources": {
                        "bird-feeder": {
                          "workspace": true
                        }
                      },
                      "workspace": {
                        "members": [
                          "packages/*"
                        ],
                        "exclude": null
                      },
                      "managed": null,
                      "package": null,
                      "dev-dependencies": null,
                      "environments": null,
                      "override-dependencies": null,
                      "constraint-dependencies": null
                    }
                  }
                }
              }
            }
            "###);
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
            @r###"
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
                        "tqdm>=4,<5"
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
                "pyproject_toml": {
                  "project": null,
                  "tool": {
                    "uv": {
                      "sources": null,
                      "workspace": {
                        "members": [
                          "packages/*"
                        ],
                        "exclude": null
                      },
                      "managed": null,
                      "package": null,
                      "dev-dependencies": null,
                      "environments": null,
                      "override-dependencies": null,
                      "constraint-dependencies": null
                    }
                  }
                }
              }
            }
            "###);
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
            @r###"
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
                        "tqdm>=4,<5"
                      ],
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  }
                },
                "sources": {},
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
                  "tool": null
                }
              }
            }
            "###);
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

        let (project, root_escaped) = temporary_test(root.as_ref()).await;
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
                      "dev-dependencies": null,
                      "environments": null,
                      "override-dependencies": null,
                      "constraint-dependencies": null
                    }
                  }
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
        let (project, root_escaped) = temporary_test(root.as_ref()).await;
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
                      "dev-dependencies": null,
                      "environments": null,
                      "override-dependencies": null,
                      "constraint-dependencies": null
                    }
                  }
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
        let (project, root_escaped) = temporary_test(root.as_ref()).await;
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
                      "dev-dependencies": null,
                      "environments": null,
                      "override-dependencies": null,
                      "constraint-dependencies": null
                    }
                  }
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
        let (project, root_escaped) = temporary_test(root.as_ref()).await;
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
                      "dev-dependencies": null,
                      "environments": null,
                      "override-dependencies": null,
                      "constraint-dependencies": null
                    }
                  }
                }
              }
            }
            "###);
        });

        Ok(())
    }
}
