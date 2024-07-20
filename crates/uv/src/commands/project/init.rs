use std::fmt::Write;
use std::path::PathBuf;

use anyhow::Result;
use owo_colors::OwoColorize;
use pep508_rs::PackageName;
use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_warnings::warn_user_once;
use uv_workspace::pyproject_mut::PyProjectTomlMut;
use uv_workspace::{ProjectWorkspace, WorkspaceError};

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Add one or more packages to the project requirements.
#[allow(clippy::single_match_else)]
pub(crate) async fn init(
    explicit_path: Option<String>,
    name: Option<PackageName>,
    no_readme: bool,
    preview: PreviewMode,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv init` is experimental and may change without warning");
    }

    // Discover the current workspace, if it exists.
    let current_dir = std::env::current_dir()?.canonicalize()?;
    let workspace = match ProjectWorkspace::discover(&current_dir, None).await {
        Ok(project) => Some(project),
        Err(WorkspaceError::MissingPyprojectToml) => None,
        Err(err) => return Err(err.into()),
    };

    // Default to the current directory if a path was not provided.
    let path = match explicit_path {
        None => current_dir.clone(),
        Some(ref path) => PathBuf::from(path),
    };

    // Default to the directory name if a name was not provided.
    let name = match name {
        Some(name) => name,
        None => {
            let name = path
                .file_name()
                .and_then(|path| path.to_str())
                .expect("Invalid package name");

            PackageName::new(name.to_string())?
        }
    };

    // Make sure a project does not already exist in the given directory.
    if path.join("pyproject.toml").exists() {
        let path = path
            .simple_canonicalize()
            .unwrap_or_else(|_| path.simplified().to_path_buf());

        anyhow::bail!(
            "Project is already initialized in {}",
            path.display().cyan()
        );
    }

    // Create the directory for the project.
    let src_dir = path.join("src").join(name.as_ref());
    fs_err::create_dir_all(&src_dir)?;

    // Canonicalize the path to the project.
    let path = path.canonicalize()?;

    // Create the `pyproject.toml`.
    let pyproject = indoc::formatdoc! {r#"
        [project]
        name = "{name}"
        version = "0.1.0"
        description = "Add your description here"{readme}
        dependencies = []

        [tool.uv]
        dev-dependencies = []
        "#,
        readme = if no_readme { "" } else { "\nreadme = \"README.md\"" },
    };

    fs_err::write(path.join("pyproject.toml"), pyproject)?;

    // Create `src/{name}/__init__.py` if it does not already exist.
    let init_py = src_dir.join("__init__.py");
    if !init_py.try_exists()? {
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
        // Add the package to the workspace.
        let mut pyproject =
            PyProjectTomlMut::from_toml(workspace.current_project().pyproject_toml())?;
        pyproject.add_workspace(path.strip_prefix(workspace.project_root())?)?;

        // Save the modified `pyproject.toml`.
        fs_err::write(
            workspace.current_project().root().join("pyproject.toml"),
            pyproject.to_string(),
        )?;

        writeln!(
            printer.stderr(),
            "Adding {} as member of workspace {}",
            name.cyan(),
            workspace
                .workspace()
                .install_path()
                .simplified_display()
                .cyan()
        )?;
    }

    match explicit_path {
        // Initialized a project in the current directory.
        None => {
            writeln!(printer.stderr(), "Initialized project {}", name.cyan())?;
        }

        // Initialized a project in the given directory.
        Some(path) => {
            let path = path
                .simple_canonicalize()
                .unwrap_or_else(|_| path.simplified().to_path_buf());

            writeln!(
                printer.stderr(),
                "Initialized project {} in {}",
                name.cyan(),
                path.display().cyan()
            )?;
        }
    }

    Ok(ExitStatus::Success)
}
