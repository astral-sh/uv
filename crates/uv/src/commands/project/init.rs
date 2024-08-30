use std::fmt::Write;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use owo_colors::OwoColorize;
use pep440_rs::Version;
use pep508_rs::PackageName;
use tracing::{debug, warn};
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, Connectivity};
use uv_fs::{Simplified, CWD};
use uv_python::{
    EnvironmentPreference, PythonDownloads, PythonInstallation, PythonPreference, PythonRequest,
    VersionRequest,
};
use uv_resolver::RequiresPython;
use uv_workspace::pyproject_mut::{DependencyTarget, PyProjectTomlMut};
use uv_workspace::{DiscoveryOptions, MemberDiscovery, Workspace, WorkspaceError};

use crate::commands::project::find_requires_python;
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Add one or more packages to the project requirements.
#[allow(clippy::single_match_else, clippy::fn_params_excessive_bools)]
pub(crate) async fn init(
    explicit_path: Option<String>,
    name: Option<PackageName>,
    package: bool,
    project_kind: InitProjectKind,
    no_readme: bool,
    python: Option<String>,
    no_workspace: bool,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    // Default to the current directory if a path was not provided.
    let path = match explicit_path {
        None => CWD.to_path_buf(),
        Some(ref path) => std::path::absolute(path)?,
    };

    // Make sure a project does not already exist in the given directory.
    if path.join("pyproject.toml").exists() {
        let path = path
            .simple_canonicalize()
            .unwrap_or_else(|_| path.simplified().to_path_buf());

        anyhow::bail!(
            "Project is already initialized in `{}`",
            path.display().cyan()
        );
    }

    // Default to the directory name if a name was not provided.
    let name = match name {
        Some(name) => name,
        None => {
            let name = path
                .file_name()
                .and_then(|path| path.to_str())
                .context("Missing directory name")?;

            PackageName::new(name.to_string())?
        }
    };

    init_project(
        &path,
        &name,
        package,
        project_kind,
        no_readme,
        python,
        no_workspace,
        python_preference,
        python_downloads,
        connectivity,
        native_tls,
        cache,
        printer,
    )
    .await?;

    // Create the `README.md` if it does not already exist.
    if !no_readme {
        let readme = path.join("README.md");
        if !readme.exists() {
            fs_err::write(readme, String::new())?;
        }
    }

    match explicit_path {
        // Initialized a project in the current directory.
        None => {
            writeln!(printer.stderr(), "Initialized project `{}`", name.cyan())?;
        }
        // Initialized a project in the given directory.
        Some(path) => {
            let path = path
                .simple_canonicalize()
                .unwrap_or_else(|_| path.simplified().to_path_buf());

            writeln!(
                printer.stderr(),
                "Initialized project `{}` at `{}`",
                name.cyan(),
                path.display().cyan()
            )?;
        }
    }

    Ok(ExitStatus::Success)
}

/// Initialize a project (and, implicitly, a workspace root) at the given path.
#[allow(clippy::fn_params_excessive_bools)]
async fn init_project(
    path: &Path,
    name: &PackageName,
    package: bool,
    project_kind: InitProjectKind,
    no_readme: bool,
    python: Option<String>,
    no_workspace: bool,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<()> {
    // Discover the current workspace, if it exists.
    let workspace = {
        let parent = path.parent().expect("Project path has no parent");
        match Workspace::discover(
            parent,
            &DiscoveryOptions {
                members: MemberDiscovery::Ignore(std::iter::once(path).collect()),
                ..DiscoveryOptions::default()
            },
        )
        .await
        {
            Ok(workspace) => {
                // Ignore the current workspace, if `--no-workspace` was provided.
                if no_workspace {
                    debug!("Ignoring discovered workspace due to `--no-workspace`");
                    None
                } else {
                    Some(workspace)
                }
            }
            Err(WorkspaceError::MissingPyprojectToml | WorkspaceError::NonWorkspace(_)) => {
                // If the user runs with `--no-workspace` and we can't find a workspace, warn.
                if no_workspace {
                    warn!("`--no-workspace` was provided, but no workspace was found");
                }
                None
            }
            Err(err) => {
                // If the user runs with `--no-workspace`, ignore the error.
                if no_workspace {
                    warn!("Ignoring workspace discovery error due to `--no-workspace`: {err}");
                    None
                } else {
                    return Err(anyhow::Error::from(err).context(format!(
                        "Failed to discover parent workspace; use `{}` to ignore",
                        "uv init --no-workspace".green()
                    )));
                }
            }
        }
    };

    // Add a `requires-python` field to the `pyproject.toml`.
    let requires_python = if let Some(request) = python.as_deref() {
        // (1) Explicit request from user
        match PythonRequest::parse(request) {
            PythonRequest::Version(VersionRequest::MajorMinor(major, minor)) => {
                RequiresPython::greater_than_equal_version(&Version::new([
                    u64::from(major),
                    u64::from(minor),
                ]))
            }
            PythonRequest::Version(VersionRequest::MajorMinorPatch(major, minor, patch)) => {
                RequiresPython::greater_than_equal_version(&Version::new([
                    u64::from(major),
                    u64::from(minor),
                    u64::from(patch),
                ]))
            }
            PythonRequest::Version(VersionRequest::Range(specifiers)) => {
                RequiresPython::from_specifiers(&specifiers)?
            }
            request => {
                let reporter = PythonDownloadReporter::single(printer);
                let client_builder = BaseClientBuilder::new()
                    .connectivity(connectivity)
                    .native_tls(native_tls);
                let interpreter = PythonInstallation::find_or_download(
                    Some(&request),
                    EnvironmentPreference::Any,
                    python_preference,
                    python_downloads,
                    &client_builder,
                    cache,
                    Some(&reporter),
                )
                .await?
                .into_interpreter();
                RequiresPython::greater_than_equal_version(&interpreter.python_minor_version())
            }
        }
    } else if let Some(requires_python) = workspace
        .as_ref()
        .and_then(|workspace| find_requires_python(workspace).ok().flatten())
    {
        // (2) `Requires-Python` from the workspace
        requires_python
    } else {
        // (3) Default to the system Python
        let request = PythonRequest::Any;
        let reporter = PythonDownloadReporter::single(printer);
        let client_builder = BaseClientBuilder::new()
            .connectivity(connectivity)
            .native_tls(native_tls);
        let interpreter = PythonInstallation::find_or_download(
            Some(&request),
            EnvironmentPreference::Any,
            python_preference,
            python_downloads,
            &client_builder,
            cache,
            Some(&reporter),
        )
        .await?
        .into_interpreter();
        RequiresPython::greater_than_equal_version(&interpreter.python_minor_version())
    };

    project_kind.init(name, path, &requires_python, no_readme, package)?;

    if let Some(workspace) = workspace {
        if workspace.excludes(path)? {
            // If the member is excluded by the workspace, ignore it.
            writeln!(
                printer.stderr(),
                "Project `{}` is excluded by workspace `{}`",
                name.cyan(),
                workspace.install_path().simplified_display().cyan()
            )?;
        } else if workspace.includes(path)? {
            // If the member is already included in the workspace, skip the `members` addition.
            writeln!(
                printer.stderr(),
                "Project `{}` is already a member of workspace `{}`",
                name.cyan(),
                workspace.install_path().simplified_display().cyan()
            )?;
        } else {
            // Add the package to the workspace.
            let mut pyproject = PyProjectTomlMut::from_toml(
                &workspace.pyproject_toml().raw,
                DependencyTarget::PyProjectToml,
            )?;
            pyproject.add_workspace(path.strip_prefix(workspace.install_path())?)?;

            // Save the modified `pyproject.toml`.
            fs_err::write(
                workspace.install_path().join("pyproject.toml"),
                pyproject.to_string(),
            )?;

            writeln!(
                printer.stderr(),
                "Adding `{}` as member of workspace `{}`",
                name.cyan(),
                workspace.install_path().simplified_display().cyan()
            )?;
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Default)]
pub(crate) enum InitProjectKind {
    #[default]
    Application,
    Library,
}

impl InitProjectKind {
    /// Initialize this project kind at the target path.
    fn init(
        &self,
        name: &PackageName,
        path: &Path,
        requires_python: &RequiresPython,
        no_readme: bool,
        package: bool,
    ) -> Result<()> {
        match self {
            InitProjectKind::Application => {
                init_application(name, path, requires_python, no_readme, package)
            }
            InitProjectKind::Library => {
                init_library(name, path, requires_python, no_readme, package)
            }
        }
    }

    /// Whether or not this project kind is packaged by default.
    pub(crate) fn packaged_by_default(&self) -> bool {
        matches!(self, InitProjectKind::Library)
    }
}

fn init_application(
    name: &PackageName,
    path: &Path,
    requires_python: &RequiresPython,
    no_readme: bool,
    package: bool,
) -> Result<()> {
    // Create the `pyproject.toml`
    let mut pyproject = pyproject_project(name, requires_python, no_readme);

    // Include additional project configuration for packaged applications
    if package {
        // Since it'll be packaged, we can add a `[project.scripts]` entry
        pyproject.push('\n');
        pyproject.push_str(&pyproject_project_scripts(name, "hello", "hello"));

        // Add a build system
        pyproject.push('\n');
        pyproject.push_str(pyproject_build_system());
    }

    fs_err::create_dir_all(path)?;

    // Create the source structure.
    if package {
        // Create `src/{name}/__init__.py`, if it doesn't exist already.
        let src_dir = path.join("src").join(&*name.as_dist_info_name());
        let init_py = src_dir.join("__init__.py");
        if !init_py.try_exists()? {
            fs_err::create_dir_all(&src_dir)?;
            fs_err::write(
                init_py,
                indoc::formatdoc! {r#"
                def hello():
                    print("Hello from {name}!")
                "#},
            )?;
        }
    } else {
        // Create `hello.py` if it doesn't exist
        // TODO(zanieb): Only create `hello.py` if there are no other Python files?
        let hello_py = path.join("hello.py");
        if !hello_py.try_exists()? {
            fs_err::write(
                path.join("hello.py"),
                indoc::formatdoc! {r#"
                def main():
                    print("Hello from {name}!")


                if __name__ == "__main__":
                    main()
                "#},
            )?;
        }
    }
    fs_err::write(path.join("pyproject.toml"), pyproject)?;

    Ok(())
}

fn init_library(
    name: &PackageName,
    path: &Path,
    requires_python: &RequiresPython,
    no_readme: bool,
    package: bool,
) -> Result<()> {
    if !package {
        return Err(anyhow!("Library projects must be packaged"));
    }

    // Create the `pyproject.toml`
    let mut pyproject = pyproject_project(name, requires_python, no_readme);

    // Always include a build system if the project is packaged.
    pyproject.push('\n');
    pyproject.push_str(pyproject_build_system());

    fs_err::create_dir_all(path)?;
    fs_err::write(path.join("pyproject.toml"), pyproject)?;

    // Create `src/{name}/__init__.py`, if it doesn't exist already.
    let src_dir = path.join("src").join(&*name.as_dist_info_name());
    let init_py = src_dir.join("__init__.py");
    if !init_py.try_exists()? {
        fs_err::create_dir_all(&src_dir)?;
        fs_err::write(
            init_py,
            indoc::formatdoc! {r#"
            def hello() -> str:
                return "Hello from {name}!"
            "#},
        )?;
    }

    Ok(())
}

/// Generate the `[project]` section of a `pyproject.toml`.
fn pyproject_project(
    name: &PackageName,
    requires_python: &RequiresPython,
    no_readme: bool,
) -> String {
    indoc::formatdoc! {r#"
            [project]
            name = "{name}"
            version = "0.1.0"
            description = "Add your description here"{readme}
            requires-python = "{requires_python}"
            dependencies = []
            "#,
        readme = if no_readme { "" } else { "\nreadme = \"README.md\"" },
        requires_python = requires_python.specifiers(),
    }
}

/// Generate the `[build-system]` section of a `pyproject.toml`.
fn pyproject_build_system() -> &'static str {
    indoc::indoc! {r#"
        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
    "#}
}

/// Generate the `[project.scripts]` section of a `pyproject.toml`.
fn pyproject_project_scripts(package: &PackageName, executable_name: &str, target: &str) -> String {
    let module_name = package.as_dist_info_name();
    indoc::formatdoc! {r#"
        [project.scripts]
        {executable_name} = "{module_name}:{target}"
    "#}
}
