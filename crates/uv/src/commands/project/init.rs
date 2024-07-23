use std::fmt::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use pep440_rs::Version;
use pep508_rs::PackageName;
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, Connectivity};
use uv_configuration::PreviewMode;
use uv_fs::{absolutize_path, Simplified};
use uv_python::{
    EnvironmentPreference, PythonFetch, PythonInstallation, PythonPreference, PythonRequest,
    VersionRequest,
};
use uv_resolver::RequiresPython;
use uv_warnings::warn_user_once;
use uv_workspace::pyproject_mut::PyProjectTomlMut;
use uv_workspace::{Workspace, WorkspaceError};

use crate::commands::project::find_requires_python;
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Add one or more packages to the project requirements.
#[allow(clippy::single_match_else)]
pub(crate) async fn init(
    explicit_path: Option<String>,
    name: Option<PackageName>,
    no_readme: bool,
    python: Option<String>,
    isolated: bool,
    preview: PreviewMode,
    python_preference: PythonPreference,
    python_fetch: PythonFetch,
    connectivity: Connectivity,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv init` is experimental and may change without warning");
    }

    // Default to the current directory if a path was not provided.
    let path = match explicit_path {
        None => std::env::current_dir()?.canonicalize()?,
        Some(ref path) => PathBuf::from(path),
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

    // Canonicalize the path to the project.
    let path = absolutize_path(&path)?;

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

    // Discover the current workspace, if it exists.
    let workspace = if isolated {
        None
    } else {
        // Attempt to find a workspace root.
        let parent = path.parent().expect("Project path has no parent");
        match Workspace::discover(parent, None).await {
            Ok(workspace) => Some(workspace),
            Err(WorkspaceError::MissingPyprojectToml) => None,
            Err(err) => return Err(err.into()),
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
                let interpreter = PythonInstallation::find_or_fetch(
                    Some(request),
                    EnvironmentPreference::Any,
                    python_preference,
                    python_fetch,
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
        let interpreter = PythonInstallation::find_or_fetch(
            Some(request),
            EnvironmentPreference::Any,
            python_preference,
            python_fetch,
            &client_builder,
            cache,
            Some(&reporter),
        )
        .await?
        .into_interpreter();
        RequiresPython::greater_than_equal_version(&interpreter.python_minor_version())
    };

    // Create the `pyproject.toml`.
    let pyproject = indoc::formatdoc! {r#"
        [project]
        name = "{name}"
        version = "0.1.0"
        description = "Add your description here"{readme}
        requires-python = "{requires_python}"
        dependencies = []

        [tool.uv]
        dev-dependencies = []
        "#,
        readme = if no_readme { "" } else { "\nreadme = \"README.md\"" },
        requires_python = requires_python.specifiers(),
    };
    fs_err::create_dir_all(&path)?;
    fs_err::write(path.join("pyproject.toml"), pyproject)?;

    // Create `src/{name}/__init__.py` if it does not already exist.
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

    // Create the `README.md` if it does not already exist.
    if !no_readme {
        let readme = path.join("README.md");
        if !readme.exists() {
            fs_err::write(readme, String::new())?;
        }
    }

    if let Some(workspace) = workspace {
        if workspace.excludes(&path)? {
            // If the member is excluded by the workspace, ignore it.
            writeln!(
                printer.stderr(),
                "Project `{}` is excluded by workspace `{}`",
                name.cyan(),
                workspace.install_path().simplified_display().cyan()
            )?;
        } else if workspace.includes(&path) {
            // If the member is already included in the workspace, skip the `members` addition.
            writeln!(
                printer.stderr(),
                "Project `{}` is already a member of workspace `{}`",
                name.cyan(),
                workspace.install_path().simplified_display().cyan()
            )?;
        } else {
            // Add the package to the workspace.
            let mut pyproject = PyProjectTomlMut::from_toml(workspace.pyproject_toml())?;
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
