use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, Context, Result};
use owo_colors::OwoColorize;

use tracing::{debug, warn};
use uv_cache::Cache;
use uv_cli::AuthorFrom;
use uv_client::{BaseClientBuilder, Connectivity};
use uv_configuration::{VersionControlError, VersionControlSystem};
use uv_fs::{Simplified, CWD};
use uv_pep440::Version;
use uv_pep508::PackageName;
use uv_python::{
    EnvironmentPreference, PythonDownloads, PythonInstallation, PythonPreference, PythonRequest,
    PythonVariant, PythonVersionFile, VersionRequest,
};
use uv_resolver::RequiresPython;
use uv_scripts::{Pep723Script, ScriptTag};
use uv_warnings::warn_user_once;
use uv_workspace::pyproject_mut::{DependencyTarget, PyProjectTomlMut};
use uv_workspace::{DiscoveryOptions, MemberDiscovery, Workspace, WorkspaceError};

use crate::commands::project::{find_requires_python, script_python_requirement};
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Add one or more packages to the project requirements.
#[allow(clippy::single_match_else, clippy::fn_params_excessive_bools)]
pub(crate) async fn init(
    project_dir: &Path,
    explicit_path: Option<PathBuf>,
    name: Option<PackageName>,
    package: bool,
    init_kind: InitKind,
    vcs: Option<VersionControlSystem>,
    no_readme: bool,
    author_from: Option<AuthorFrom>,
    no_pin_python: bool,
    python: Option<String>,
    no_workspace: bool,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    match init_kind {
        InitKind::Script => {
            let Some(path) = explicit_path.as_deref() else {
                anyhow::bail!("Script initialization requires a file path")
            };

            init_script(
                path,
                python,
                connectivity,
                python_preference,
                python_downloads,
                cache,
                printer,
                no_workspace,
                no_readme,
                author_from,
                no_pin_python,
                package,
                native_tls,
            )
            .await?;

            writeln!(
                printer.stderr(),
                "Initialized script at `{}`",
                path.user_display().cyan()
            )?;
        }
        InitKind::Project(project_kind) => {
            // Default to the current directory if a path was not provided.
            let path = match explicit_path {
                None => project_dir.to_path_buf(),
                Some(ref path) => std::path::absolute(path)?,
            };

            // Make sure a project does not already exist in the given directory.
            if path.join("pyproject.toml").exists() {
                let path =
                    std::path::absolute(&path).unwrap_or_else(|_| path.simplified().to_path_buf());
                anyhow::bail!(
                    "Project is already initialized in `{}` (`pyproject.toml` file exists)",
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
                vcs,
                no_readme,
                author_from,
                no_pin_python,
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
                    let path = std::path::absolute(&path)
                        .unwrap_or_else(|_| path.simplified().to_path_buf());
                    writeln!(
                        printer.stderr(),
                        "Initialized project `{}` at `{}`",
                        name.cyan(),
                        path.display().cyan()
                    )?;
                }
            }
        }
    }

    Ok(ExitStatus::Success)
}

#[allow(clippy::fn_params_excessive_bools)]
async fn init_script(
    script_path: &Path,
    python: Option<String>,
    connectivity: Connectivity,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    cache: &Cache,
    printer: Printer,
    no_workspace: bool,
    no_readme: bool,
    author_from: Option<AuthorFrom>,
    no_pin_python: bool,
    package: bool,
    native_tls: bool,
) -> Result<()> {
    if no_workspace {
        warn_user_once!("`--no-workspace` is a no-op for Python scripts, which are standalone");
    }
    if no_readme {
        warn_user_once!("`--no-readme` is a no-op for Python scripts, which are standalone");
    }
    if author_from.is_some() {
        warn_user_once!("`--author-from` is a no-op for Python scripts, which are standalone");
    }
    if package {
        warn_user_once!("`--package` is a no-op for Python scripts, which are standalone");
    }
    let client_builder = BaseClientBuilder::new()
        .connectivity(connectivity)
        .native_tls(native_tls);

    let reporter = PythonDownloadReporter::single(printer);

    // If the file already exists, read its content.
    let content = match fs_err::tokio::read(script_path).await {
        Ok(metadata) => {
            // If the file is already a script, raise an error.
            if ScriptTag::parse(&metadata)?.is_some() {
                anyhow::bail!(
                    "`{}` is already a PEP 723 script; use `{}` to execute it",
                    script_path.simplified_display().cyan(),
                    "uv run".green()
                );
            }

            Some(metadata)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => {
            return Err(anyhow::Error::from(err).context(format!(
                "Failed to read script at `{}`",
                script_path.simplified_display().cyan()
            )));
        }
    };

    let requires_python = script_python_requirement(
        python.as_deref(),
        &CWD,
        no_pin_python,
        python_preference,
        python_downloads,
        &client_builder,
        cache,
        &reporter,
    )
    .await?;

    if let Some(parent) = script_path.parent() {
        fs_err::tokio::create_dir_all(parent).await?;
    }

    Pep723Script::create(script_path, requires_python.specifiers(), content).await?;

    Ok(())
}

/// Initialize a project (and, implicitly, a workspace root) at the given path.
#[allow(clippy::fn_params_excessive_bools)]
async fn init_project(
    path: &Path,
    name: &PackageName,
    package: bool,
    project_kind: InitProjectKind,
    vcs: Option<VersionControlSystem>,
    no_readme: bool,
    author_from: Option<AuthorFrom>,
    no_pin_python: bool,
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

    let reporter = PythonDownloadReporter::single(printer);
    let client_builder = BaseClientBuilder::new()
        .connectivity(connectivity)
        .native_tls(native_tls);

    // Add a `requires-python` field to the `pyproject.toml` and return the corresponding interpreter.
    let (requires_python, python_request) = if let Some(request) = python.as_deref() {
        // (1) Explicit request from user
        match PythonRequest::parse(request) {
            PythonRequest::Version(VersionRequest::MajorMinor(
                major,
                minor,
                PythonVariant::Default,
            )) => {
                let requires_python = RequiresPython::greater_than_equal_version(&Version::new([
                    u64::from(major),
                    u64::from(minor),
                ]));

                let python_request = if no_pin_python {
                    None
                } else {
                    Some(PythonRequest::Version(VersionRequest::MajorMinor(
                        major,
                        minor,
                        PythonVariant::Default,
                    )))
                };

                (requires_python, python_request)
            }
            PythonRequest::Version(VersionRequest::MajorMinorPatch(
                major,
                minor,
                patch,
                PythonVariant::Default,
            )) => {
                let requires_python = RequiresPython::greater_than_equal_version(&Version::new([
                    u64::from(major),
                    u64::from(minor),
                    u64::from(patch),
                ]));

                let python_request = if no_pin_python {
                    None
                } else {
                    Some(PythonRequest::Version(VersionRequest::MajorMinorPatch(
                        major,
                        minor,
                        patch,
                        PythonVariant::Default,
                    )))
                };

                (requires_python, python_request)
            }
            ref
            python_request @ PythonRequest::Version(VersionRequest::Range(ref specifiers, _)) => {
                let requires_python = RequiresPython::from_specifiers(specifiers)?;

                let python_request = if no_pin_python {
                    None
                } else {
                    let interpreter = PythonInstallation::find_or_download(
                        Some(python_request),
                        EnvironmentPreference::Any,
                        python_preference,
                        python_downloads,
                        &client_builder,
                        cache,
                        Some(&reporter),
                    )
                    .await?
                    .into_interpreter();

                    Some(PythonRequest::Version(VersionRequest::MajorMinor(
                        interpreter.python_major(),
                        interpreter.python_minor(),
                        PythonVariant::Default,
                    )))
                };

                (requires_python, python_request)
            }
            python_request => {
                let interpreter = PythonInstallation::find_or_download(
                    Some(&python_request),
                    EnvironmentPreference::Any,
                    python_preference,
                    python_downloads,
                    &client_builder,
                    cache,
                    Some(&reporter),
                )
                .await?
                .into_interpreter();

                let requires_python =
                    RequiresPython::greater_than_equal_version(&interpreter.python_minor_version());

                let python_request = if no_pin_python {
                    None
                } else {
                    Some(PythonRequest::Version(VersionRequest::MajorMinor(
                        interpreter.python_major(),
                        interpreter.python_minor(),
                        PythonVariant::Default,
                    )))
                };

                (requires_python, python_request)
            }
        }
    } else if let Some(requires_python) = workspace
        .as_ref()
        .and_then(|workspace| find_requires_python(workspace).ok().flatten())
    {
        // (2) `Requires-Python` from the workspace
        let python_request = PythonRequest::Version(VersionRequest::Range(
            requires_python.specifiers().clone(),
            PythonVariant::Default,
        ));

        // Pin to the minor version.
        let python_request = if no_pin_python {
            None
        } else {
            let interpreter = PythonInstallation::find_or_download(
                Some(&python_request),
                EnvironmentPreference::Any,
                python_preference,
                python_downloads,
                &client_builder,
                cache,
                Some(&reporter),
            )
            .await?
            .into_interpreter();

            Some(PythonRequest::Version(VersionRequest::MajorMinor(
                interpreter.python_major(),
                interpreter.python_minor(),
                PythonVariant::Default,
            )))
        };

        (requires_python, python_request)
    } else {
        // (3) Default to the system Python
        let interpreter = PythonInstallation::find_or_download(
            None,
            EnvironmentPreference::Any,
            python_preference,
            python_downloads,
            &client_builder,
            cache,
            Some(&reporter),
        )
        .await?
        .into_interpreter();

        let requires_python =
            RequiresPython::greater_than_equal_version(&interpreter.python_minor_version());

        // Pin to the minor version.
        let python_request = if no_pin_python {
            None
        } else {
            Some(PythonRequest::Version(VersionRequest::MajorMinor(
                interpreter.python_major(),
                interpreter.python_minor(),
                PythonVariant::Default,
            )))
        };

        (requires_python, python_request)
    };

    project_kind
        .init(
            name,
            path,
            &requires_python,
            python_request.as_ref(),
            vcs,
            author_from,
            no_readme,
            package,
        )
        .await?;

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

/// The kind of entity to initialize (either a PEP 723 script or a Python project).
#[derive(Debug, Copy, Clone)]
pub(crate) enum InitKind {
    /// Initialize a Python project.
    Project(InitProjectKind),
    /// Initialize a PEP 723 script.
    Script,
}

impl Default for InitKind {
    fn default() -> Self {
        InitKind::Project(InitProjectKind::default())
    }
}

/// The kind of Python project to initialize (either an application or a library).
#[derive(Debug, Copy, Clone, Default)]
pub(crate) enum InitProjectKind {
    /// Initialize a Python application.
    #[default]
    Application,
    /// Initialize a Python library.
    Library,
}

impl InitKind {
    /// Returns `true` if the project should be packaged by default.
    pub(crate) fn packaged_by_default(self) -> bool {
        matches!(self, InitKind::Project(InitProjectKind::Library))
    }
}

impl InitProjectKind {
    /// Initialize this project kind at the target path.
    async fn init(
        self,
        name: &PackageName,
        path: &Path,
        requires_python: &RequiresPython,
        python_request: Option<&PythonRequest>,
        vcs: Option<VersionControlSystem>,
        author_from: Option<AuthorFrom>,
        no_readme: bool,
        package: bool,
    ) -> Result<()> {
        match self {
            InitProjectKind::Application => {
                self.init_application(
                    name,
                    path,
                    requires_python,
                    python_request,
                    vcs,
                    author_from,
                    no_readme,
                    package,
                )
                .await
            }
            InitProjectKind::Library => {
                self.init_library(
                    name,
                    path,
                    requires_python,
                    python_request,
                    vcs,
                    author_from,
                    no_readme,
                    package,
                )
                .await
            }
        }
    }

    /// Initialize a Python application at the target path.
    async fn init_application(
        self,
        name: &PackageName,
        path: &Path,
        requires_python: &RequiresPython,
        python_request: Option<&PythonRequest>,
        vcs: Option<VersionControlSystem>,
        author_from: Option<AuthorFrom>,
        no_readme: bool,
        package: bool,
    ) -> Result<()> {
        fs_err::create_dir_all(path)?;

        // Do no fill in `authors` for non-packaged applications unless explicitly requested.
        let author_from = author_from.unwrap_or_else(|| {
            if package {
                AuthorFrom::default()
            } else {
                AuthorFrom::None
            }
        });
        let author = get_author_info(path, author_from);

        // Create the `pyproject.toml`
        let mut pyproject = pyproject_project(name, requires_python, author.as_ref(), no_readme);

        // Include additional project configuration for packaged applications
        if package {
            // Since it'll be packaged, we can add a `[project.scripts]` entry
            pyproject.push('\n');
            pyproject.push_str(&pyproject_project_scripts(name, name.as_str(), "main"));

            // Add a build system
            pyproject.push('\n');
            pyproject.push_str(pyproject_build_system());
        }

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
                    def main() -> None:
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

        // Write .python-version if it doesn't exist.
        if let Some(python_request) = python_request {
            if PythonVersionFile::discover(path, false, false)
                .await?
                .is_none()
            {
                PythonVersionFile::new(path.join(".python-version"))
                    .with_versions(vec![python_request.clone()])
                    .write()
                    .await?;
            }
        }

        // Initialize the version control system.
        init_vcs(path, vcs)?;

        Ok(())
    }

    /// Initialize a library project at the target path.
    async fn init_library(
        self,
        name: &PackageName,
        path: &Path,
        requires_python: &RequiresPython,
        python_request: Option<&PythonRequest>,
        vcs: Option<VersionControlSystem>,
        author_from: Option<AuthorFrom>,
        no_readme: bool,
        package: bool,
    ) -> Result<()> {
        if !package {
            return Err(anyhow!("Library projects must be packaged"));
        }

        fs_err::create_dir_all(path)?;

        let author = get_author_info(path, author_from.unwrap_or_default());

        // Create the `pyproject.toml`
        let mut pyproject = pyproject_project(name, requires_python, author.as_ref(), no_readme);

        // Always include a build system if the project is packaged.
        pyproject.push('\n');
        pyproject.push_str(pyproject_build_system());

        fs_err::write(path.join("pyproject.toml"), pyproject)?;

        // Create `src/{name}/__init__.py`, if it doesn't exist already.
        let src_dir = path.join("src").join(&*name.as_dist_info_name());
        fs_err::create_dir_all(&src_dir)?;

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

        // Create a `py.typed` file
        let py_typed = src_dir.join("py.typed");
        if !py_typed.try_exists()? {
            fs_err::write(py_typed, "")?;
        }

        // Write .python-version if it doesn't exist.
        if let Some(python_request) = python_request {
            if PythonVersionFile::discover(path, false, false)
                .await?
                .is_none()
            {
                PythonVersionFile::new(path.join(".python-version"))
                    .with_versions(vec![python_request.clone()])
                    .write()
                    .await?;
            }
        }

        // Initialize the version control system.
        init_vcs(path, vcs)?;

        Ok(())
    }
}

#[derive(Debug)]
enum Author {
    Name(String),
    Email(String),
    NameEmail { name: String, email: String },
}

impl Author {
    fn to_toml_string(&self) -> String {
        match self {
            Self::NameEmail { name, email } => {
                format!("{{ name = \"{name}\", email = \"{email}\" }}")
            }
            Self::Name(name) => format!("{{ name = \"{name}\" }}"),
            Self::Email(email) => format!("{{ email = \"{email}\" }}"),
        }
    }
}

/// Generate the `[project]` section of a `pyproject.toml`.
fn pyproject_project(
    name: &PackageName,
    requires_python: &RequiresPython,
    author: Option<&Author>,
    no_readme: bool,
) -> String {
    indoc::formatdoc! {r#"
        [project]
        name = "{name}"
        version = "0.1.0"
        description = "Add your description here"{readme}{authors}
        requires-python = "{requires_python}"
        dependencies = []
    "#,
        readme = if no_readme { "" } else { "\nreadme = \"README.md\"" },
        authors = author.map_or_else(String::new, |author| format!("\nauthors = [\n    {} \n]", author.to_toml_string())),
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

/// Initialize the version control system at the given path.
fn init_vcs(path: &Path, vcs: Option<VersionControlSystem>) -> Result<()> {
    // Detect any existing version control system.
    let existing = VersionControlSystem::detect(path);

    let implicit = vcs.is_none();

    let vcs = match (vcs, existing) {
        // If no version control system was specified, and none was detected, default to Git.
        (None, None) => VersionControlSystem::default(),
        // If no version control system was specified, but a VCS was detected, leave it as-is.
        (None, Some(existing)) => {
            debug!("Detected existing version control system: {existing}");
            VersionControlSystem::None
        }
        // If the user provides an explicit `--vcs none`,
        (Some(VersionControlSystem::None), _) => VersionControlSystem::None,
        // If a version control system was specified, use it.
        (Some(vcs), None) => vcs,
        // If a version control system was specified, but a VCS was detected...
        (Some(vcs), Some(existing)) => {
            // If they differ, raise an error.
            if vcs != existing {
                anyhow::bail!("The project is already in a version control system (`{existing}`); cannot initialize with `--vcs {vcs}`");
            }

            // Otherwise, ignore the specified VCS, since it's already in use.
            VersionControlSystem::None
        }
    };

    // Attempt to initialize the VCS.
    match vcs.init(path) {
        Ok(()) => (),
        // If the VCS isn't installed, only raise an error if a VCS was explicitly specified.
        Err(err @ VersionControlError::GitNotInstalled) => {
            if implicit {
                debug!("Failed to initialize version control: {err}");
            } else {
                return Err(err.into());
            }
        }
        Err(err) => return Err(err.into()),
    }

    Ok(())
}

/// Try to get the author information.
///
/// Currently, this only tries to get the author information from git.
fn get_author_info(path: &Path, author_from: AuthorFrom) -> Option<Author> {
    if matches!(author_from, AuthorFrom::None) {
        return None;
    }
    if matches!(author_from, AuthorFrom::Auto | AuthorFrom::Git) {
        match get_author_from_git(path) {
            Ok(author) => return Some(author),
            Err(err) => warn!("Failed to get author from git: {err}"),
        }
    }

    None
}

/// Fetch the default author from git configuration.
fn get_author_from_git(path: &Path) -> Result<Author> {
    let Ok(git) = which::which("git") else {
        anyhow::bail!("`git` not found in PATH")
    };

    let mut name = None;
    let mut email = None;

    let output = Command::new(&git)
        .arg("config")
        .arg("--get")
        .arg("user.name")
        .current_dir(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()?;
    if output.status.success() {
        name = Some(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }

    let output = Command::new(&git)
        .arg("config")
        .arg("--get")
        .arg("user.email")
        .current_dir(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()?;
    if output.status.success() {
        email = Some(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }

    let author = match (name, email) {
        (Some(name), Some(email)) => Author::NameEmail { name, email },
        (Some(name), None) => Author::Name(name),
        (None, Some(email)) => Author::Email(email),
        (None, None) => anyhow::bail!("No author information found"),
    };

    Ok(author)
}
