//! Resolve the current [`ProjectWorkspace`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use glob::{glob, GlobError, PatternError};
use tracing::{debug, trace};

use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_warnings::warn_user;

use crate::pyproject::{PyProjectToml, Source, ToolUvWorkspace};
use crate::RequirementsSource;

#[derive(thiserror::Error, Debug)]
pub enum WorkspaceError {
    #[error("No `pyproject.toml` found in current directory or any parent directory")]
    MissingPyprojectToml,
    #[error("Failed to find directories for glob: `{0}`")]
    Pattern(String, #[source] PatternError),
    #[error("Invalid glob in `tool.uv.workspace.members`: `{0}`")]
    Glob(String, #[source] GlobError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Failed to parse: `{}`", _0.user_display())]
    Toml(PathBuf, #[source] Box<toml::de::Error>),
    #[error("No `project` section found in: `{}`", _0.simplified_display())]
    MissingProject(PathBuf),
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
    /// The path to the workspace root, the directory containing the top level `pyproject.toml` with
    /// the `uv.tool.workspace`, or the `pyproject.toml` in an implicit single workspace project.
    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    /// The members of the workspace.
    pub fn packages(&self) -> &BTreeMap<PackageName, WorkspaceMember> {
        &self.packages
    }

    /// The sources table from the workspace `pyproject.toml`.
    pub fn sources(&self) -> &BTreeMap<PackageName, Source> {
        &self.sources
    }
}

/// A project in a workspace.
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(serde::Serialize))]
pub struct WorkspaceMember {
    /// The path to the project root.
    root: PathBuf,
    /// The `pyproject.toml` of the project, found at `<root>/pyproject.toml`.
    pyproject_toml: PyProjectToml,
}

impl WorkspaceMember {
    /// The path to the project root.
    pub fn root(&self) -> &PathBuf {
        &self.root
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
    /// Find the current project and workspace.
    pub fn discover(path: impl AsRef<Path>) -> Result<Self, WorkspaceError> {
        let Some(project_root) = path
            .as_ref()
            .ancestors()
            .find(|path| path.join("pyproject.toml").is_file())
        else {
            return Err(WorkspaceError::MissingPyprojectToml);
        };

        debug!(
            "Found project root: `{}`",
            project_root.simplified_display()
        );

        Self::from_project_root(project_root)
    }

    /// The directory containing the closest `pyproject.toml`, defining the current project.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// The name of the current project.
    pub fn project_name(&self) -> &PackageName {
        &self.project_name
    }

    /// The workspace definition.
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// Return the requirements for the project.
    pub fn requirements(&self) -> Vec<RequirementsSource> {
        vec![
            RequirementsSource::from_requirements_file(self.project_root.join("pyproject.toml")),
            RequirementsSource::from_source_tree(self.project_root.clone()),
        ]
    }

    fn from_project_root(path: &Path) -> Result<Self, WorkspaceError> {
        let pyproject_path = path.join("pyproject.toml");

        // Read the `pyproject.toml`.
        let contents = fs_err::read_to_string(&pyproject_path)?;
        let pyproject_toml: PyProjectToml = toml::from_str(&contents)
            .map_err(|err| WorkspaceError::Toml(pyproject_path.clone(), Box::new(err)))?;

        // Extract the `[project]` metadata.
        let Some(project) = pyproject_toml.project.clone() else {
            return Err(WorkspaceError::MissingProject(pyproject_path));
        };

        Self::from_project(path.to_path_buf(), &pyproject_toml, project.name)
    }

    /// Find the workspace for a project.
    fn from_project(
        project_path: PathBuf,
        project: &PyProjectToml,
        project_name: PackageName,
    ) -> Result<Self, WorkspaceError> {
        let mut workspace = project
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.workspace.as_ref())
            .map(|workspace| (project_path.clone(), workspace.clone(), project.clone()));

        if workspace.is_none() {
            workspace = find_workspace(&project_path)?;
        }

        let mut workspace_members = BTreeMap::new();
        workspace_members.insert(
            project_name.clone(),
            WorkspaceMember {
                root: project_path.clone(),
                pyproject_toml: project.clone(),
            },
        );

        let Some((workspace_root, workspace_definition, project_in_workspace_root)) = workspace
        else {
            // The project and the workspace root are identical
            debug!("No workspace root found, using project root");
            return Ok(Self {
                project_root: project_path.clone(),
                project_name,
                workspace: Workspace {
                    root: project_path,
                    packages: workspace_members,
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
        if workspace_root != project_path {
            let pyproject_path = workspace_root.join("pyproject.toml");
            let contents = fs_err::read_to_string(&pyproject_path)?;
            let pyproject_toml = toml::from_str(&contents)
                .map_err(|err| WorkspaceError::Toml(pyproject_path, Box::new(err)))?;

            if let Some(project) = &project_in_workspace_root.project {
                workspace_members.insert(
                    project.name.clone(),
                    WorkspaceMember {
                        root: workspace_root.clone(),
                        pyproject_toml,
                    },
                );
            };
        }
        for member_glob in workspace_definition.members.unwrap_or_default() {
            let absolute_glob = workspace_root
                .join(member_glob.as_str())
                .to_string_lossy()
                .to_string();
            for member_root in glob(&absolute_glob)
                .map_err(|err| WorkspaceError::Pattern(absolute_glob.to_string(), err))?
            {
                // TODO(konsti): Filter already seen.
                let member_root = member_root
                    .map_err(|err| WorkspaceError::Glob(absolute_glob.to_string(), err))?;
                // Read the `pyproject.toml`.
                let pyproject_path = member_root.join("pyproject.toml");
                let contents = fs_err::read_to_string(&pyproject_path)?;
                let pyproject_toml: PyProjectToml = toml::from_str(&contents)
                    .map_err(|err| WorkspaceError::Toml(pyproject_path, Box::new(err)))?;

                // Extract the package name.
                let Some(project) = pyproject_toml.project.clone() else {
                    return Err(WorkspaceError::MissingProject(member_root));
                };

                let pyproject_toml = workspace_root.join("pyproject.toml");
                let contents = fs_err::read_to_string(&pyproject_toml)?;
                let pyproject_toml = toml::from_str(&contents)
                    .map_err(|err| WorkspaceError::Toml(pyproject_toml, Box::new(err)))?;
                let member = WorkspaceMember {
                    root: member_root.clone(),
                    pyproject_toml,
                };
                workspace_members.insert(project.name, member);
            }
        }
        let workspace_sources = project_in_workspace_root
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.sources.clone())
            .unwrap_or_default();

        check_nested_workspaces(&workspace_root);

        Ok(Self {
            project_root: project_path.clone(),
            project_name,
            workspace: Workspace {
                root: workspace_root,
                packages: workspace_members,
                sources: workspace_sources,
            },
        })
    }
}

/// Find the workspace root above the current project, if any.
fn find_workspace(
    project_root: &Path,
) -> Result<Option<(PathBuf, ToolUvWorkspace, PyProjectToml)>, WorkspaceError> {
    // Skip 1 to ignore the current project itself.
    for workspace_root in project_root.ancestors().skip(1) {
        let pyproject_path = workspace_root.join("pyproject.toml");
        if !pyproject_path.is_file() {
            continue;
        }
        trace!(
            "Found pyproject.toml: {}",
            pyproject_path.simplified_display()
        );

        // Read the `pyproject.toml`.
        let contents = fs_err::read_to_string(&pyproject_path)?;
        let pyproject_toml: PyProjectToml = toml::from_str(&contents)
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
fn check_nested_workspaces(inner_workspace_root: &Path) {
    for outer_workspace_root in inner_workspace_root.ancestors().skip(1) {
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
    use std::path::Path;

    use insta::assert_json_snapshot;

    use crate::workspace::ProjectWorkspace;

    fn workspace_test(folder: impl AsRef<Path>) -> (ProjectWorkspace, String) {
        let root_dir = env::current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("scripts")
            .join("workspaces");
        let project = ProjectWorkspace::discover(root_dir.join(folder)).unwrap();
        let root_escaped = regex::escape(root_dir.to_string_lossy().as_ref());
        (project, root_escaped)
    }

    #[test]
    fn albatross_in_example() {
        let (project, root_escaped) = workspace_test("albatross-in-example/examples/bird-feeder");
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
                "pyproject_toml": "[PYPROJECT_TOML]"
              }
            },
            "sources": {}
          }
        }
        "###);
        });
    }

    #[test]
    fn albatross_project_in_excluded() {
        let (project, root_escaped) =
            workspace_test("albatross-project-in-excluded/excluded/bird-feeder");
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
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  }
                },
                "sources": {}
              }
            }
            "###);
        });
    }

    #[test]
    fn albatross_root_workspace() {
        let (project, root_escaped) = workspace_test("albatross-root-workspace");
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
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  },
                  "bird-feeder": {
                    "root": "[ROOT]/albatross-root-workspace/packages/bird-feeder",
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  },
                  "seeds": {
                    "root": "[ROOT]/albatross-root-workspace/packages/seeds",
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

    #[test]
    fn albatross_virtual_workspace() {
        let (project, root_escaped) =
            workspace_test("albatross-virtual-workspace/packages/albatross");
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
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  },
                  "bird-feeder": {
                    "root": "[ROOT]/albatross-virtual-workspace/packages/bird-feeder",
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  },
                  "seeds": {
                    "root": "[ROOT]/albatross-virtual-workspace/packages/seeds",
                    "pyproject_toml": "[PYPROJECT_TOML]"
                  }
                },
                "sources": {}
              }
            }
            "###);
        });
    }

    #[test]
    fn albatross_just_project() {
        let (project, root_escaped) = workspace_test("albatross-just-project");
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
