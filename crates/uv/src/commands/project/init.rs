use anyhow::{anyhow, Context, Result};
use owo_colors::OwoColorize;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str::FromStr;

use tracing::{debug, warn};
use uv_cache::Cache;
use uv_cli::AuthorFrom;
use uv_client::BaseClientBuilder;
use uv_configuration::{
    PreviewMode, ProjectBuildBackend, VersionControlError, VersionControlSystem,
};
use uv_fs::{Simplified, CWD};
use uv_git::GIT;
use uv_pep440::Version;
use uv_pep508::PackageName;
use uv_python::{
    EnvironmentPreference, PythonDownloads, PythonEnvironment, PythonInstallation,
    PythonPreference, PythonRequest, PythonVariant, PythonVersionFile, VersionFileDiscoveryOptions,
    VersionRequest,
};
use uv_resolver::RequiresPython;
use uv_scripts::{Pep723Script, ScriptTag};
use uv_settings::PythonInstallMirrors;
use uv_warnings::warn_user_once;
use uv_workspace::pyproject_mut::{DependencyTarget, PyProjectTomlMut};
use uv_workspace::{DiscoveryOptions, MemberDiscovery, Workspace, WorkspaceCache, WorkspaceError};

use crate::commands::project::{find_requires_python, init_script_python_requirement};
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::ExitStatus;
use crate::printer::Printer;
use crate::settings::NetworkSettings;

/// Add one or more packages to the project requirements.
#[allow(clippy::single_match_else, clippy::fn_params_excessive_bools)]
pub(crate) async fn init(
    project_dir: &Path,
    explicit_path: Option<PathBuf>,
    name: Option<PackageName>,
    package: bool,
    init_kind: InitKind,
    bare: bool,
    description: Option<String>,
    no_description: bool,
    vcs: Option<VersionControlSystem>,
    build_backend: Option<ProjectBuildBackend>,
    no_readme: bool,
    author_from: Option<AuthorFrom>,
    pin_python: bool,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    no_workspace: bool,
    network_settings: &NetworkSettings,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    no_config: bool,
    cache: &Cache,
    printer: Printer,
    preview: PreviewMode,
) -> Result<ExitStatus> {
    if build_backend == Some(ProjectBuildBackend::Uv) && preview.is_disabled() {
        warn_user_once!("The uv build backend is experimental and may change without warning");
    }
    match init_kind {
        InitKind::Script => {
            let Some(path) = explicit_path.as_deref() else {
                anyhow::bail!("Script initialization requires a file path")
            };

            init_script(
                path,
                python,
                install_mirrors,
                network_settings,
                python_preference,
                python_downloads,
                cache,
                printer,
                no_workspace,
                no_readme,
                author_from,
                pin_python,
                package,
                no_config,
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

                    // Pre-normalize the package name by removing any leading or trailing
                    // whitespace, and replacing any internal whitespace with hyphens.
                    let name = name.trim().replace(' ', "-");
                    PackageName::from_owned(name)?
                }
            };

            init_project(
                &path,
                &name,
                package,
                project_kind,
                bare,
                description,
                no_description,
                vcs,
                build_backend,
                no_readme,
                author_from,
                pin_python,
                python,
                install_mirrors,
                no_workspace,
                network_settings,
                python_preference,
                python_downloads,
                no_config,
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
    install_mirrors: PythonInstallMirrors,
    network_settings: &NetworkSettings,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    cache: &Cache,
    printer: Printer,
    no_workspace: bool,
    no_readme: bool,
    author_from: Option<AuthorFrom>,
    pin_python: bool,
    package: bool,
    no_config: bool,
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
        .connectivity(network_settings.connectivity)
        .native_tls(network_settings.native_tls)
        .allow_insecure_host(network_settings.allow_insecure_host.clone());

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

    let requires_python = init_script_python_requirement(
        python.as_deref(),
        &install_mirrors,
        &CWD,
        pin_python,
        python_preference,
        python_downloads,
        no_config,
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
    bare: bool,
    description: Option<String>,
    no_description: bool,
    vcs: Option<VersionControlSystem>,
    build_backend: Option<ProjectBuildBackend>,
    no_readme: bool,
    author_from: Option<AuthorFrom>,
    pin_python: bool,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    no_workspace: bool,
    network_settings: &NetworkSettings,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    no_config: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<()> {
    // Discover the current workspace, if it exists.
    let workspace_cache = WorkspaceCache::default();
    let workspace = {
        let parent = path.parent().expect("Project path has no parent");
        match Workspace::discover(
            parent,
            &DiscoveryOptions {
                members: MemberDiscovery::Ignore(std::iter::once(path.to_path_buf()).collect()),
                ..DiscoveryOptions::default()
            },
            &workspace_cache,
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
        .connectivity(network_settings.connectivity)
        .native_tls(network_settings.native_tls)
        .allow_insecure_host(network_settings.allow_insecure_host.clone());

    // First, determine if there is an request for Python
    let python_request = if let Some(request) = python {
        // (1) Explicit request from user
        Some(PythonRequest::parse(&request))
    } else if let Some(file) = PythonVersionFile::discover(
        path,
        &VersionFileDiscoveryOptions::default()
            .with_stop_discovery_at(
                workspace
                    .as_ref()
                    .map(Workspace::install_path)
                    .map(PathBuf::as_ref),
            )
            .with_no_config(no_config),
    )
    .await?
    {
        // (2) Request from `.python-version`
        file.into_version()
    } else {
        None
    };

    // Add a `requires-python` field to the `pyproject.toml` and return the corresponding interpreter.
    let (requires_python, python_request) = if let Some(python_request) = python_request {
        // (1) A request from the user or `.python-version` file
        // This can be arbitrary, i.e., not a version — in which case we may need to resolve the
        // interpreter
        match python_request {
            PythonRequest::Version(VersionRequest::MajorMinor(
                major,
                minor,
                PythonVariant::Default,
            )) => {
                let requires_python = RequiresPython::greater_than_equal_version(&Version::new([
                    u64::from(major),
                    u64::from(minor),
                ]));

                let python_request = if pin_python {
                    Some(PythonRequest::Version(VersionRequest::MajorMinor(
                        major,
                        minor,
                        PythonVariant::Default,
                    )))
                } else {
                    None
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

                let python_request = if pin_python {
                    Some(PythonRequest::Version(VersionRequest::MajorMinorPatch(
                        major,
                        minor,
                        patch,
                        PythonVariant::Default,
                    )))
                } else {
                    None
                };

                (requires_python, python_request)
            }
            ref
            python_request @ PythonRequest::Version(VersionRequest::Range(ref specifiers, _)) => {
                let requires_python = RequiresPython::from_specifiers(specifiers);

                let python_request = if pin_python {
                    let interpreter = PythonInstallation::find_or_download(
                        Some(python_request),
                        EnvironmentPreference::OnlySystem,
                        python_preference,
                        python_downloads,
                        &client_builder,
                        cache,
                        Some(&reporter),
                        install_mirrors.python_install_mirror.as_deref(),
                        install_mirrors.pypy_install_mirror.as_deref(),
                    )
                    .await?
                    .into_interpreter();

                    Some(PythonRequest::Version(VersionRequest::MajorMinor(
                        interpreter.python_major(),
                        interpreter.python_minor(),
                        PythonVariant::Default,
                    )))
                } else {
                    None
                };

                (requires_python, python_request)
            }
            python_request => {
                let interpreter = PythonInstallation::find_or_download(
                    Some(&python_request),
                    EnvironmentPreference::OnlySystem,
                    python_preference,
                    python_downloads,
                    &client_builder,
                    cache,
                    Some(&reporter),
                    install_mirrors.python_install_mirror.as_deref(),
                    install_mirrors.pypy_install_mirror.as_deref(),
                )
                .await?
                .into_interpreter();

                let requires_python =
                    RequiresPython::greater_than_equal_version(&interpreter.python_minor_version());

                let python_request = if pin_python {
                    Some(PythonRequest::Version(VersionRequest::MajorMinor(
                        interpreter.python_major(),
                        interpreter.python_minor(),
                        PythonVariant::Default,
                    )))
                } else {
                    None
                };

                (requires_python, python_request)
            }
        }
    } else if let Ok(virtualenv) = PythonEnvironment::from_root(path.join(".venv"), cache) {
        // (2) An existing Python environment in the target directory
        debug!("Using Python version from existing virtual environment in project");
        let interpreter = virtualenv.into_interpreter();

        let requires_python =
            RequiresPython::greater_than_equal_version(&interpreter.python_minor_version());

        // Pin to the minor version.
        let python_request = if pin_python {
            Some(PythonRequest::Version(VersionRequest::MajorMinor(
                interpreter.python_major(),
                interpreter.python_minor(),
                PythonVariant::Default,
            )))
        } else {
            None
        };

        (requires_python, python_request)
    } else if let Some(requires_python) = workspace
        .as_ref()
        .map(find_requires_python)
        .transpose()?
        .flatten()
    {
        // (3) `requires-python` from the workspace
        debug!("Using Python version from project workspace");
        let python_request = PythonRequest::Version(VersionRequest::Range(
            requires_python.specifiers().clone(),
            PythonVariant::Default,
        ));

        // Pin to the minor version.
        let python_request = if pin_python {
            let interpreter = PythonInstallation::find_or_download(
                Some(&python_request),
                EnvironmentPreference::OnlySystem,
                python_preference,
                python_downloads,
                &client_builder,
                cache,
                Some(&reporter),
                install_mirrors.python_install_mirror.as_deref(),
                install_mirrors.pypy_install_mirror.as_deref(),
            )
            .await?
            .into_interpreter();

            Some(PythonRequest::Version(VersionRequest::MajorMinor(
                interpreter.python_major(),
                interpreter.python_minor(),
                PythonVariant::Default,
            )))
        } else {
            None
        };

        (requires_python, python_request)
    } else {
        // (4) Default to the system Python
        let interpreter = PythonInstallation::find_or_download(
            None,
            EnvironmentPreference::OnlySystem,
            python_preference,
            python_downloads,
            &client_builder,
            cache,
            Some(&reporter),
            install_mirrors.python_install_mirror.as_deref(),
            install_mirrors.pypy_install_mirror.as_deref(),
        )
        .await?
        .into_interpreter();

        let requires_python =
            RequiresPython::greater_than_equal_version(&interpreter.python_minor_version());

        // Pin to the minor version.
        let python_request = if pin_python {
            Some(PythonRequest::Version(VersionRequest::MajorMinor(
                interpreter.python_major(),
                interpreter.python_minor(),
                PythonVariant::Default,
            )))
        } else {
            None
        };

        (requires_python, python_request)
    };

    project_kind.init(
        name,
        path,
        &requires_python,
        description.as_deref(),
        no_description,
        bare,
        vcs,
        build_backend,
        author_from,
        no_readme,
        package,
    )?;

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
        // Write .python-version if it doesn't exist in the workspace or if the version differs
        if let Some(python_request) = python_request {
            if PythonVersionFile::discover(path, &VersionFileDiscoveryOptions::default())
                .await?
                .filter(|file| {
                    file.version()
                        .is_some_and(|version| *version == python_request)
                        && file.path().parent().is_some_and(|parent| {
                            parent == workspace.install_path() || parent == path
                        })
                })
                .is_none()
            {
                PythonVersionFile::new(path.join(".python-version"))
                    .with_versions(vec![python_request.clone()])
                    .write()
                    .await?;
            }
        }
    } else {
        // Write .python-version if it doesn't exist in the project directory.
        if let Some(python_request) = python_request {
            if PythonVersionFile::discover(path, &VersionFileDiscoveryOptions::default())
                .await?
                .filter(|file| file.version().is_some())
                .filter(|file| file.path().parent().is_some_and(|parent| parent == path))
                .is_none()
            {
                PythonVersionFile::new(path.join(".python-version"))
                    .with_versions(vec![python_request.clone()])
                    .write()
                    .await?;
            }
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
    #[allow(clippy::fn_params_excessive_bools)]
    fn init(
        self,
        name: &PackageName,
        path: &Path,
        requires_python: &RequiresPython,
        description: Option<&str>,
        no_description: bool,
        bare: bool,
        vcs: Option<VersionControlSystem>,
        build_backend: Option<ProjectBuildBackend>,
        author_from: Option<AuthorFrom>,
        no_readme: bool,
        package: bool,
    ) -> Result<()> {
        match self {
            InitProjectKind::Application => InitProjectKind::init_application(
                name,
                path,
                requires_python,
                description,
                no_description,
                bare,
                vcs,
                build_backend,
                author_from,
                no_readme,
                package,
            ),
            InitProjectKind::Library => InitProjectKind::init_library(
                name,
                path,
                requires_python,
                description,
                no_description,
                bare,
                vcs,
                build_backend,
                author_from,
                no_readme,
                package,
            ),
        }
    }

    /// Initialize a Python application at the target path.
    #[allow(clippy::fn_params_excessive_bools)]
    fn init_application(
        name: &PackageName,
        path: &Path,
        requires_python: &RequiresPython,
        description: Option<&str>,
        no_description: bool,
        bare: bool,
        vcs: Option<VersionControlSystem>,
        build_backend: Option<ProjectBuildBackend>,
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
        let mut pyproject = pyproject_project(
            name,
            requires_python,
            author.as_ref(),
            description,
            no_description,
            no_readme,
        );

        // Include additional project configuration for packaged applications
        if package {
            // Since it'll be packaged, we can add a `[project.scripts]` entry
            if !bare {
                pyproject.push('\n');
                pyproject.push_str(&pyproject_project_scripts(name, name.as_str(), "main"));
            }

            // Add a build system
            let build_backend = build_backend.unwrap_or_default();
            pyproject.push('\n');
            pyproject.push_str(&pyproject_build_system(name, build_backend));
            pyproject_build_backend_prerequisites(name, path, build_backend)?;

            if !bare {
                // Generate `src` files
                generate_package_scripts(name, path, build_backend, false)?;
            }
        } else {
            // Create `main.py` if it doesn't exist
            // (This isn't intended to be a particularly special or magical filename, just nice)
            // TODO(zanieb): Only create `main.py` if there are no other Python files?
            let main_py = path.join("main.py");
            if !main_py.try_exists()? && !bare {
                fs_err::write(
                    path.join("main.py"),
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

        // Initialize the version control system.
        init_vcs(path, vcs)?;

        Ok(())
    }

    /// Initialize a library project at the target path.
    #[allow(clippy::fn_params_excessive_bools)]
    fn init_library(
        name: &PackageName,
        path: &Path,
        requires_python: &RequiresPython,
        description: Option<&str>,
        no_description: bool,
        bare: bool,
        vcs: Option<VersionControlSystem>,
        build_backend: Option<ProjectBuildBackend>,
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
        let mut pyproject = pyproject_project(
            name,
            requires_python,
            author.as_ref(),
            description,
            no_description,
            no_readme,
        );

        // Always include a build system if the project is packaged.
        let build_backend = build_backend.unwrap_or_default();
        pyproject.push('\n');
        pyproject.push_str(&pyproject_build_system(name, build_backend));
        pyproject_build_backend_prerequisites(name, path, build_backend)?;

        fs_err::write(path.join("pyproject.toml"), pyproject)?;

        // Generate `src` files
        if !bare {
            generate_package_scripts(name, path, build_backend, true)?;
        };

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
    description: Option<&str>,
    no_description: bool,
    no_readme: bool,
) -> String {
    indoc::formatdoc! {r#"
        [project]
        name = "{name}"
        version = "0.1.0"{description}{readme}{authors}
        requires-python = "{requires_python}"
        dependencies = []
    "#,
        readme = if no_readme { "" } else { "\nreadme = \"README.md\"" },
        description = if no_description {
            String::new()
        } else {
            format!("\ndescription = \"{description}\"", description = description.unwrap_or("Add your description here"))
        },
        authors = author.map_or_else(String::new, |author| format!("\nauthors = [\n    {}\n]", author.to_toml_string())),
        requires_python = requires_python.specifiers(),
    }
}

/// Generate the `[build-system]` section of a `pyproject.toml`.
/// Generate the `[tool.]` section of a `pyproject.toml` where applicable.
fn pyproject_build_system(package: &PackageName, build_backend: ProjectBuildBackend) -> String {
    let module_name = package.as_dist_info_name();
    match build_backend {
        ProjectBuildBackend::Uv => {
            // Limit to the stable version range.
            let min_version = Version::from_str(uv_version::version()).unwrap();
            debug_assert!(
                min_version.release()[0] == 0,
                "migrate to major version bumps"
            );
            let max_version = Version::new([0, min_version.release()[1] + 1]);
            indoc::formatdoc! {r#"
                [build-system]
                requires = ["uv_build>={min_version},<{max_version}"]
                build-backend = "uv_build"
            "#}
        }
        .to_string(),
        // Pure-python backends
        ProjectBuildBackend::Hatch => indoc::indoc! {r#"
                [build-system]
                requires = ["hatchling"]
                build-backend = "hatchling.build"
            "#}
        .to_string(),
        ProjectBuildBackend::Flit => indoc::indoc! {r#"
                [build-system]
                requires = ["flit_core>=3.2,<4"]
                build-backend = "flit_core.buildapi"
            "#}
        .to_string(),
        ProjectBuildBackend::PDM => indoc::indoc! {r#"
                [build-system]
                requires = ["pdm-backend"]
                build-backend = "pdm.backend"
            "#}
        .to_string(),
        ProjectBuildBackend::Setuptools => indoc::indoc! {r#"
                [build-system]
                requires = ["setuptools>=61"]
                build-backend = "setuptools.build_meta"
            "#}
        .to_string(),
        // Binary build backends
        ProjectBuildBackend::Maturin => indoc::formatdoc! {r#"
                [tool.maturin]
                module-name = "{module_name}._core"
                python-packages = ["{module_name}"]
                python-source = "src"

                [build-system]
                requires = ["maturin>=1.0,<2.0"]
                build-backend = "maturin"
            "#},
        ProjectBuildBackend::Scikit => indoc::indoc! {r#"
                [tool.scikit-build]
                minimum-version = "build-system.requires"
                build-dir = "build/{wheel_tag}"

                [build-system]
                requires = ["scikit-build-core>=0.10", "pybind11"]
                build-backend = "scikit_build_core.build"
            "#}
        .to_string(),
    }
}

/// Generate the `[project.scripts]` section of a `pyproject.toml`.
fn pyproject_project_scripts(package: &PackageName, executable_name: &str, target: &str) -> String {
    let module_name = package.as_dist_info_name();
    indoc::formatdoc! {r#"
        [project.scripts]
        {executable_name} = "{module_name}:{target}"
    "#}
}

/// Generate additional files as needed for specific build backends.
fn pyproject_build_backend_prerequisites(
    package: &PackageName,
    path: &Path,
    build_backend: ProjectBuildBackend,
) -> Result<()> {
    let module_name = package.as_dist_info_name();
    match build_backend {
        ProjectBuildBackend::Maturin => {
            // Generate Cargo.toml
            let build_file = path.join("Cargo.toml");
            if !build_file.try_exists()? {
                fs_err::write(
                    build_file,
                    indoc::formatdoc! {r#"
                    [package]
                    name = "{module_name}"
                    version = "0.1.0"
                    edition = "2021"

                    [lib]
                    name = "_core"
                    # "cdylib" is necessary to produce a shared library for Python to import from.
                    crate-type = ["cdylib"]

                    [dependencies]
                    # "extension-module" tells pyo3 we want to build an extension module (skips linking against libpython.so)
                    # "abi3-py39" tells pyo3 (and maturin) to build using the stable ABI with minimum Python version 3.9
                    pyo3 = {{ version = "0.22.4", features = ["extension-module", "abi3-py39"] }}
                "#},
                )?;
            }
        }
        ProjectBuildBackend::Scikit => {
            // Generate CMakeLists.txt
            let build_file = path.join("CMakeLists.txt");
            if !build_file.try_exists()? {
                fs_err::write(
                    build_file,
                    indoc::formatdoc! {r"
                    cmake_minimum_required(VERSION 3.15)
                    project(${{SKBUILD_PROJECT_NAME}} LANGUAGES CXX)

                    set(PYBIND11_FINDPYTHON ON)
                    find_package(pybind11 CONFIG REQUIRED)

                    pybind11_add_module(_core MODULE src/main.cpp)
                    install(TARGETS _core DESTINATION ${{SKBUILD_PROJECT_NAME}})
                "},
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Generate startup scripts for a package-based application or library.
fn generate_package_scripts(
    package: &PackageName,
    path: &Path,
    build_backend: ProjectBuildBackend,
    is_lib: bool,
) -> Result<()> {
    let module_name = package.as_dist_info_name();

    let src_dir = path.join("src");
    let pkg_dir = src_dir.join(&*module_name);
    fs_err::create_dir_all(&pkg_dir)?;

    // Python script for pure-python packaged apps or libs
    let pure_python_script = if is_lib {
        indoc::formatdoc! {r#"
        def hello() -> str:
            return "Hello from {package}!"
        "#}
    } else {
        indoc::formatdoc! {r#"
        def main() -> None:
            print("Hello from {package}!")
        "#}
    };

    // Python script for binary-based packaged apps or libs
    let binary_call_script = if is_lib {
        indoc::formatdoc! {r"
        from {module_name}._core import hello_from_bin


        def hello() -> str:
            return hello_from_bin()
        "}
    } else {
        indoc::formatdoc! {r"
        from {module_name}._core import hello_from_bin


        def main() -> None:
            print(hello_from_bin())
        "}
    };

    // .pyi file for binary script
    let pyi_contents = indoc::indoc! {r"
        def hello_from_bin() -> str: ...
    "};

    let package_script = match build_backend {
        ProjectBuildBackend::Maturin => {
            // Generate lib.rs
            let native_src = src_dir.join("lib.rs");
            if !native_src.try_exists()? {
                fs_err::write(
                    native_src,
                    indoc::formatdoc! {r#"
                    use pyo3::prelude::*;

                    #[pyfunction]
                    fn hello_from_bin() -> String {{
                        "Hello from {package}!".to_string()
                    }}

                    /// A Python module implemented in Rust. The name of this function must match
                    /// the `lib.name` setting in the `Cargo.toml`, else Python will not be able to
                    /// import the module.
                    #[pymodule]
                    fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {{
                        m.add_function(wrap_pyfunction!(hello_from_bin, m)?)?;
                        Ok(())
                    }}
                "#},
                )?;
            }
            // Generate .pyi file
            let pyi_file = pkg_dir.join("_core.pyi");
            if !pyi_file.try_exists()? {
                fs_err::write(pyi_file, pyi_contents)?;
            };
            // Return python script calling binary
            binary_call_script
        }
        ProjectBuildBackend::Scikit => {
            // Generate main.cpp
            let native_src = src_dir.join("main.cpp");
            if !native_src.try_exists()? {
                fs_err::write(
                    native_src,
                    indoc::formatdoc! {r#"
                    #include <pybind11/pybind11.h>

                    std::string hello_from_bin() {{ return "Hello from {package}!"; }}

                    namespace py = pybind11;

                    PYBIND11_MODULE(_core, m) {{
                      m.doc() = "pybind11 hello module";

                      m.def("hello_from_bin", &hello_from_bin, R"pbdoc(
                          A function that returns a Hello string.
                      )pbdoc");
                    }}
                "#},
                )?;
            }
            // Generate .pyi file
            let pyi_file = pkg_dir.join("_core.pyi");
            if !pyi_file.try_exists()? {
                fs_err::write(pyi_file, pyi_contents)?;
            };
            // Return python script calling binary
            binary_call_script
        }
        _ => pure_python_script,
    };

    // Create `src/{name}/__init__.py`, if it doesn't exist already.
    let init_py = pkg_dir.join("__init__.py");
    if !init_py.try_exists()? {
        fs_err::write(init_py, package_script)?;
    }

    // Create `src/{name}/py.typed`, if it doesn't exist already.
    if is_lib {
        let py_typed = pkg_dir.join("py.typed");
        if !py_typed.try_exists()? {
            fs_err::write(py_typed, "")?;
        }
    }

    Ok(())
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
    let Ok(git) = GIT.as_ref() else {
        anyhow::bail!("`git` not found in PATH")
    };

    let mut name = None;
    let mut email = None;

    let output = Command::new(git)
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

    let output = Command::new(git)
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
