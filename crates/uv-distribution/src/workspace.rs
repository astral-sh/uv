//! Resolve the current [`ProjectWorkspace`] or [`Workspace`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use glob::{glob, GlobError, PatternError};
use rustc_hash::FxHashSet;
use tracing::{debug, trace};

use pep508_rs::{RequirementOrigin, VerbatimUrl};
use pypi_types::{Requirement, RequirementSource};
use uv_fs::{absolutize_path, Simplified};
use uv_normalize::PackageName;
use uv_warnings::warn_user;

use crate::pyproject::{Project, PyProjectToml, Source, ToolUvWorkspace};

#[derive(thiserror::Error, Debug)]
pub enum WorkspaceError {
    // Workspace structure errors.
    #[error("No `pyproject.toml` found in current directory or any parent directory")]
    MissingPyprojectToml,
    #[error("No `project` table found in: `{}`", _0.simplified_display())]
    MissingProject(PathBuf),
    #[error("No workspace found for: `{}`", _0.simplified_display())]
    MissingWorkspace(PathBuf),
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
    Toml(PathBuf, #[source] Box<toml::de::Error>),
    #[error("Failed to normalize workspace member path")]
    Normalize(#[source] std::io::Error),
}

/// A workspace, consisting of a root directory and members. See [`ProjectWorkspace`].
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(serde::Serialize))]
pub struct Workspace {
    /// The path to the workspace root, the directory containing the top level `pyproject.toml` with
    /// the `uv.tool.workspace`, or the `pyproject.toml` in an implicit single workspace project.
    root: PathBuf,
    /// The members of the workspace.
    packages: BTreeMap<PackageName, WorkspaceMember>,
    /// The sources table from the workspace `pyproject.toml`. It is overridden by the project
    /// sources.
    sources: BTreeMap<PackageName, Source>,
}

impl Workspace {
    /// Find the workspace containing the given path.
    ///
    /// Unlike the [`ProjectWorkspace`] discovery, this does not require a current project.
    ///
    /// Steps of workspace discovery: Start by looking at the closest `pyproject.toml`:
    /// * If it's an explicit workspace root: Collect workspace from this root, we're done.
    /// * If it's also not a project: Error, must be either a workspace root or a project.
    /// * Otherwise, try to find an explicit workspace root above:
    ///   * If an explicit workspace root exists: Collect workspace from this root, we're done.
    ///   * If there is no explicit workspace: We have a single project workspace, we're done.
    pub async fn discover(
        path: &Path,
        stop_discovery_at: Option<&Path>,
    ) -> Result<Workspace, WorkspaceError> {
        let project_root = path
            .ancestors()
            .find(|path| path.join("pyproject.toml").is_file())
            .ok_or(WorkspaceError::MissingPyprojectToml)?;

        let pyproject_path = project_root.join("pyproject.toml");
        let contents = fs_err::tokio::read_to_string(&pyproject_path).await?;
        let pyproject_toml = PyProjectToml::from_string(contents)
            .map_err(|err| WorkspaceError::Toml(pyproject_path.clone(), Box::new(err)))?;

        let project_path = absolutize_path(project_root)
            .map_err(WorkspaceError::Normalize)?
            .to_path_buf();

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
                return Err(WorkspaceError::MissingProject(project_path));
            } else if let Some(workspace) = find_workspace(&project_path, stop_discovery_at).await?
            {
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

        // Unlike in `ProjectWorkspace` discovery, we might be in a virtual workspace root without
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
            workspace_root,
            workspace_definition,
            workspace_pyproject_toml,
            current_project,
            stop_discovery_at,
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

    /// Returns the set of requirements that include all packages in the workspace.
    pub fn members_as_requirements(&self) -> Vec<Requirement> {
        self.packages
            .values()
            .filter_map(|member| {
                let project = member.pyproject_toml.project.as_ref()?;
                // Extract the extras available in the project.
                let extras = project
                    .optional_dependencies
                    .as_ref()
                    .map(|optional_dependencies| {
                        // It's a `BTreeMap` so the keys are sorted.
                        optional_dependencies.keys().cloned().collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                let url = VerbatimUrl::from_path(&member.root)
                    .expect("path is valid URL")
                    .with_given(member.root.to_string_lossy());
                Some(Requirement {
                    name: project.name.clone(),
                    extras,
                    marker: None,
                    source: RequirementSource::Directory {
                        install_path: member.root.clone(),
                        lock_path: member
                            .root
                            .strip_prefix(&self.root)
                            .expect("Project must be below workspace root")
                            .to_path_buf(),
                        editable: true,
                        url,
                    },
                    origin: None,
                })
            })
            .collect()
    }

    /// Returns the set of overrides for the workspace.
    pub fn overrides(&self) -> Vec<Requirement> {
        let Some(workspace_package) = self
            .packages
            .values()
            .find(|workspace_package| workspace_package.root() == self.root())
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

    /// The path to the workspace root, the directory containing the top level `pyproject.toml` with
    /// the `uv.tool.workspace`, or the `pyproject.toml` in an implicit single workspace project.
    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    /// The path to the workspace virtual environment.
    pub fn venv(&self) -> PathBuf {
        self.root.join(".venv")
    }

    /// The members of the workspace.
    pub fn packages(&self) -> &BTreeMap<PackageName, WorkspaceMember> {
        &self.packages
    }

    /// The sources table from the workspace `pyproject.toml`.
    pub fn sources(&self) -> &BTreeMap<PackageName, Source> {
        &self.sources
    }

    /// Collect the workspace member projects from the `members` and `excludes` entries.
    async fn collect_members(
        workspace_root: PathBuf,
        workspace_definition: ToolUvWorkspace,
        workspace_pyproject_toml: PyProjectToml,
        current_project: Option<WorkspaceMember>,
        stop_discovery_at: Option<&Path>,
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
                    .map_err(|err| WorkspaceError::Toml(pyproject_path, Box::new(err)))?;

                debug!(
                    "Adding root workspace member: {}",
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
                "Adding current workspace member: {}",
                root_member.root.simplified_display()
            );

            seen.insert(root_member.root.clone());
            workspace_members.insert(root_member.project.name.clone(), root_member);
        }

        // Add all other workspace members.
        for member_glob in workspace_definition.members.unwrap_or_default() {
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
                let member_root = absolutize_path(&member_root)
                    .map_err(WorkspaceError::Normalize)?
                    .to_path_buf();

                trace!("Processing workspace member {}", member_root.user_display());

                // Read the member `pyproject.toml`.
                let pyproject_path = member_root.join("pyproject.toml");
                let contents = fs_err::tokio::read_to_string(&pyproject_path).await?;
                let pyproject_toml = PyProjectToml::from_string(contents)
                    .map_err(|err| WorkspaceError::Toml(pyproject_path, Box::new(err)))?;

                // Extract the package name.
                let Some(project) = pyproject_toml.project.clone() else {
                    return Err(WorkspaceError::MissingProject(member_root));
                };

                debug!(
                    "Adding discovered workspace member: {}",
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
            .and_then(|tool| tool.uv)
            .and_then(|uv| uv.sources)
            .unwrap_or_default();

        check_nested_workspaces(&workspace_root, stop_discovery_at);

        Ok(Workspace {
            root: workspace_root,
            packages: workspace_members,
            sources: workspace_sources,
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
        stop_discovery_at: Option<&Path>,
    ) -> Result<Self, WorkspaceError> {
        let project_root = path
            .ancestors()
            .take_while(|path| {
                // Only walk up the given directory, if any.
                stop_discovery_at
                    .map(|stop_discovery_at| stop_discovery_at != *path)
                    .unwrap_or(true)
            })
            .find(|path| path.join("pyproject.toml").is_file())
            .ok_or(WorkspaceError::MissingPyprojectToml)?;

        debug!(
            "Found project root: `{}`",
            project_root.simplified_display()
        );

        Self::from_project_root(project_root, stop_discovery_at).await
    }

    /// Discover the workspace starting from the directory containing the `pyproject.toml`.
    pub async fn from_project_root(
        project_root: &Path,
        stop_discovery_at: Option<&Path>,
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
            .ok_or_else(|| WorkspaceError::MissingProject(pyproject_path.clone()))?;

        Self::from_project(project_root, &project, &pyproject_toml, stop_discovery_at).await
    }

    /// If the current directory contains a `pyproject.toml` with a `project` table, discover the
    /// workspace and return it, otherwise it is a dynamic path dependency and we return `Ok(None)`.
    pub async fn from_maybe_project_root(
        project_root: &Path,
        stop_discovery_at: Option<&Path>,
    ) -> Result<Option<Self>, WorkspaceError> {
        // Read the `pyproject.toml`.
        let pyproject_path = project_root.join("pyproject.toml");
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

        Ok(Some(
            Self::from_project(project_root, &project, &pyproject_toml, stop_discovery_at).await?,
        ))
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

    /// Find the workspace for a project.
    pub async fn from_project(
        project_path: &Path,
        project: &Project,
        project_pyproject_toml: &PyProjectToml,
        stop_discovery_at: Option<&Path>,
    ) -> Result<Self, WorkspaceError> {
        let project_path = absolutize_path(project_path)
            .map_err(WorkspaceError::Normalize)?
            .to_path_buf();

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
            workspace = find_workspace(&project_path, stop_discovery_at).await?;
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
                    root: project_path.clone(),
                    packages: current_project_as_members,
                    // There may be package sources, but we don't need to duplicate them into the
                    // workspace sources.
                    sources: BTreeMap::default(),
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
            stop_discovery_at,
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
    stop_discovery_at: Option<&Path>,
) -> Result<Option<(PathBuf, ToolUvWorkspace, PyProjectToml)>, WorkspaceError> {
    // Skip 1 to ignore the current project itself.
    for workspace_root in project_root
        .ancestors()
        .take_while(|path| {
            // Only walk up the given directory, if any.
            stop_discovery_at
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
            "Found pyproject.toml: {}",
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
            if is_excluded_from_workspace(project_root, workspace_root, workspace)? {
                debug!(
                    "Found workspace root `{}`, but project is excluded.",
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
            warn_user!(
                "pyproject.toml does not contain `project` table: `{}`",
                workspace_root.simplified_display()
            );
            Ok(None)
        };
    }

    Ok(None)
}

/// Warn when the valid workspace is included in another workspace.
fn check_nested_workspaces(inner_workspace_root: &Path, stop_discovery_at: Option<&Path>) {
    for outer_workspace_root in inner_workspace_root
        .ancestors()
        .take_while(|path| {
            // Only walk up the given directory, if any.
            stop_discovery_at
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
                warn_user!(
                    "Unreadable pyproject.toml `{}`: {}",
                    pyproject_toml_path.user_display(),
                    err
                );
                return;
            }
        };
        let pyproject_toml: PyProjectToml = match toml::from_str(&contents) {
            Ok(contents) => contents,
            Err(err) => {
                warn_user!(
                    "Invalid pyproject.toml `{}`: {}",
                    pyproject_toml_path.user_display(),
                    err
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
            let is_excluded = match is_excluded_from_workspace(
                inner_workspace_root,
                outer_workspace_root,
                workspace,
            ) {
                Ok(contents) => contents,
                Err(err) => {
                    warn_user!(
                        "Invalid pyproject.toml `{}`: {}",
                        pyproject_toml_path.user_display(),
                        err
                    );
                    return;
                }
            };
            if !is_excluded {
                warn_user!(
                    "Outer workspace including existing workspace, nested workspaces are not supported: `{}`",
                    pyproject_toml_path.user_display(),
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
            if excluded_root == project_path {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[cfg(test)]
#[cfg(unix)] // Avoid path escaping for the unit tests
mod tests {
    use std::env;

    use insta::assert_json_snapshot;

    use crate::workspace::ProjectWorkspace;

    async fn workspace_test(folder: &str) -> (ProjectWorkspace, String) {
        let root_dir = env::current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("scripts")
            .join("workspaces");
        let project = ProjectWorkspace::discover(&root_dir.join(folder), None)
            .await
            .unwrap();
        let root_escaped = regex::escape(root_dir.to_string_lossy().as_ref());
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
            "root": "[ROOT]/albatross-in-example/examples/bird-feeder",
            "packages": {
              "bird-feeder": {
                "root": "[ROOT]/albatross-in-example/examples/bird-feeder",
                "project": {
                  "name": "bird-feeder",
                  "requires-python": ">=3.12",
                  "optional-dependencies": null
                },
                "pyproject_toml": "[PYPROJECT_TOML]"
              }
            },
            "sources": {}
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
                "root": "[ROOT]/albatross-project-in-excluded/excluded/bird-feeder",
                "packages": {
                  "bird-feeder": {
                    "root": "[ROOT]/albatross-project-in-excluded/excluded/bird-feeder",
                    "project": {
                      "name": "bird-feeder",
                      "requires-python": ">=3.12",
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  }
                },
                "sources": {}
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
                "root": "[ROOT]/albatross-root-workspace",
                "packages": {
                  "albatross": {
                    "root": "[ROOT]/albatross-root-workspace",
                    "project": {
                      "name": "albatross",
                      "requires-python": ">=3.12",
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  },
                  "bird-feeder": {
                    "root": "[ROOT]/albatross-root-workspace/packages/bird-feeder",
                    "project": {
                      "name": "bird-feeder",
                      "requires-python": ">=3.12",
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  },
                  "seeds": {
                    "root": "[ROOT]/albatross-root-workspace/packages/seeds",
                    "project": {
                      "name": "seeds",
                      "requires-python": ">=3.12",
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  }
                },
                "sources": {
                  "bird-feeder": {
                    "workspace": true,
                    "editable": null
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
                "root": "[ROOT]/albatross-virtual-workspace",
                "packages": {
                  "albatross": {
                    "root": "[ROOT]/albatross-virtual-workspace/packages/albatross",
                    "project": {
                      "name": "albatross",
                      "requires-python": ">=3.12",
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  },
                  "bird-feeder": {
                    "root": "[ROOT]/albatross-virtual-workspace/packages/bird-feeder",
                    "project": {
                      "name": "bird-feeder",
                      "requires-python": ">=3.12",
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  },
                  "seeds": {
                    "root": "[ROOT]/albatross-virtual-workspace/packages/seeds",
                    "project": {
                      "name": "seeds",
                      "requires-python": ">=3.12",
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  }
                },
                "sources": {}
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
                "root": "[ROOT]/albatross-just-project",
                "packages": {
                  "albatross": {
                    "root": "[ROOT]/albatross-just-project",
                    "project": {
                      "name": "albatross",
                      "requires-python": ">=3.12",
                      "optional-dependencies": null
                    },
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  }
                },
                "sources": {}
              }
            }
            "###);
        });
    }
}
