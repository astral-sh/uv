use std::fmt::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cache_key::RepositoryUrl;
use owo_colors::OwoColorize;

use pep440_rs::Version;
use pep508_rs::{ExtraName, MarkerExpression, PackageName, Requirement};
use pypi_types::redact_git_credentials;
use tracing::debug;
use uv_auth::{store_credentials_from_url, Credentials};
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, Connectivity, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{Concurrency, SourceStrategy};
use uv_dispatch::BuildDispatch;
use uv_distribution::DistributionDatabase;
use uv_fs::{absolutize_path, Simplified, CWD};
use uv_git::GIT_STORE;
use uv_python::{
    EnvironmentPreference, PythonDownloads, PythonInstallation, PythonPreference, PythonRequest,
    VersionRequest,
};
use uv_requirements::SourceTreeResolver;
use uv_resolver::{FlatIndex, RequiresPython};
use uv_types::{BuildIsolation, HashStrategy};
use uv_warnings::warn_user_once;
use uv_workspace::pyproject::{DependencyType, Source};
use uv_workspace::pyproject_mut::{DependencyTarget, PyProjectTomlMut};
use uv_workspace::{
    check_nested_workspaces, DiscoveryOptions, VirtualProject, Workspace, WorkspaceError,
};

use crate::commands::pip::resolution_environment;
use crate::commands::project::find_requires_python;
use crate::commands::reporters::{PythonDownloadReporter, ResolverReporter};
use crate::commands::{ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

use super::add::{process_project_and_sync, resolve_requirement, DependencyEdit};

/// Add one or more packages to the project requirements.
#[allow(clippy::single_match_else, clippy::fn_params_excessive_bools)]
pub(crate) async fn init(
    no_sync: bool,
    raw_sources: bool,
    explicit_path: Option<String>,
    name: Option<PackageName>,
    r#virtual: bool,
    no_readme: bool,
    python: Option<String>,
    from_project: Option<PathBuf>,
    no_workspace: bool,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    native_tls: bool,
    cache: &Cache,
    settings: ResolverInstallerSettings,
    concurrency: Concurrency,
    printer: Printer,
) -> Result<ExitStatus> {
    // Default to the current directory if a path was not provided.
    let path = match explicit_path {
        None => CWD.to_path_buf(),
        Some(ref path) => absolutize_path(Path::new(path))?.to_path_buf(),
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

    if r#virtual {
        init_virtual_workspace(&path, no_workspace)?;
    } else {
        init_project(
            &path,
            &name,
            no_readme,
            python.clone(),
            no_workspace,
            python_preference,
            python_downloads,
            connectivity,
            native_tls,
            cache,
            printer,
        )
        .await?;
    }

    // Create the `README.md` if it does not already exist.
    if !no_readme {
        let readme = path.join("README.md");
        if !readme.exists() {
            fs_err::write(readme, String::new())?;
        }
    }

    let project = if r#virtual { "workspace" } else { "project" };
    match explicit_path {
        // Initialized a project in the current directory.
        None => {
            writeln!(
                printer.stderr(),
                "Initialized {} `{}`",
                project,
                name.cyan()
            )?;
        }
        // Initialized a project in the given directory.
        Some(path) => {
            let path = path
                .simple_canonicalize()
                .unwrap_or_else(|_| path.simplified().to_path_buf());

            writeln!(
                printer.stderr(),
                "Initialized {} `{}` at `{}`",
                project,
                name.cyan(),
                path.display().cyan()
            )?;
        }
    }

    if r#virtual {
        return Ok(ExitStatus::Success);
    }

    if let Some(from_project) = from_project {
        let project = VirtualProject::discover(&path, &DiscoveryOptions::default()).await?;

        // Discover or create the virtual environment.
        let venv = super::get_or_init_environment(
            project.workspace(),
            python.as_deref().map(PythonRequest::parse),
            python_preference,
            python_downloads,
            connectivity,
            native_tls,
            cache,
            printer,
        )
        .await?;

        let client_builder = BaseClientBuilder::new()
            .connectivity(connectivity)
            .native_tls(native_tls)
            .keyring(settings.keyring_provider);

        // TODO(charlie): These are all default values. We should consider whether we want to make them
        // optional on the downstream APIs.
        let python_version = None;
        let python_platform = None;
        let hasher = HashStrategy::default();
        let build_isolation = BuildIsolation::default();

        // Determine the environment for the resolution.
        let (tags, markers) =
            resolution_environment(python_version, python_platform, venv.interpreter())?;

        // Add all authenticated sources to the cache.
        for url in settings.index_locations.urls() {
            store_credentials_from_url(url);
        }

        // Initialize the registry client.
        let client = RegistryClientBuilder::from(client_builder)
            .index_urls(settings.index_locations.index_urls())
            .index_strategy(settings.index_strategy)
            .markers(&markers)
            .platform(venv.interpreter().platform())
            .build();

        // Initialize any shared state.
        let state = SharedState::default();

        // Resolve the flat indexes from `--find-links`.
        let flat_index = {
            let client = FlatIndexClient::new(&client, cache);
            let entries = client.fetch(settings.index_locations.flat_index()).await?;
            FlatIndex::from_entries(entries, Some(&tags), &hasher, &settings.build_options)
        };

        let build_constraints = [];
        let sources = SourceStrategy::Enabled;

        // Create a build dispatch.
        let build_dispatch = BuildDispatch::new(
            &client,
            cache,
            &build_constraints,
            venv.interpreter(),
            &settings.index_locations,
            &flat_index,
            &state.index,
            &state.git,
            &state.in_flight,
            settings.index_strategy,
            &settings.config_setting,
            build_isolation,
            settings.link_mode,
            &settings.build_options,
            settings.exclude_newer,
            sources,
            concurrency,
        );

        let resolutions = SourceTreeResolver::new(
            vec![from_project],
            &uv_configuration::ExtrasSpecification::default(),
            &hasher,
            &state.index,
            DistributionDatabase::new(&client, &build_dispatch, concurrency.downloads),
        )
        .with_reporter(ResolverReporter::from(printer))
        .resolve()
        .await?;

        let requirements = if let Some(resolution) = resolutions.first() {
            resolution.requirements.clone()
        } else {
            return Ok(ExitStatus::Success);
        };
        let mut toml = PyProjectTomlMut::from_toml(
            &project.pyproject_toml().raw,
            DependencyTarget::PyProjectToml,
        )?;

        let mut edits = Vec::with_capacity(requirements.len());

        for requirement in requirements {
            let extras: Vec<ExtraName> = requirement
                .marker
                .to_dnf()
                .iter()
                .flatten()
                .filter_map(|marker| {
                    if let MarkerExpression::Extra { name, .. } = marker {
                        Some(name.clone())
                    } else {
                        None
                    }
                })
                .collect();
            let (requirement, source) = {
                let workspace = project
                    .workspace()
                    .packages()
                    .contains_key(&requirement.name);
                resolve_requirement(requirement, workspace, None, None, None, None)?
            };

            // Evaluate the `extras` expression in any markers, but preserve the remaining marker
            // conditions.
            let requirement = Requirement {
                marker: requirement.marker.simplify_extras(&extras),
                ..requirement
            };

            // Redact any credentials. By default, we avoid writing sensitive credentials to files that
            // will be checked into version control (e.g., `pyproject.toml` and `uv.lock`). Instead,
            // we store the credentials in a global store, and reuse them during resolution. The
            // expectation is that subsequent resolutions steps will succeed by reading from (e.g.) the
            // user's credentials store, rather than by reading from the `pyproject.toml` file.
            let source = match source {
                Some(Source::Git {
                    mut git,
                    subdirectory,
                    rev,
                    tag,
                    branch,
                }) => {
                    let credentials = Credentials::from_url(&git);
                    if let Some(credentials) = credentials {
                        debug!("Caching credentials for: {git}");
                        GIT_STORE.insert(RepositoryUrl::new(&git), credentials);

                        // Redact the credentials.
                        redact_git_credentials(&mut git);
                    };
                    Some(Source::Git {
                        git,
                        subdirectory,
                        rev,
                        tag,
                        branch,
                    })
                }
                _ => source,
            };

            // Update the `pyproject.toml`.
            if extras.is_empty() {
                let edit = toml.add_dependency(&requirement, source.as_ref())?;
                edits.push(DependencyEdit::new(
                    DependencyType::Production,
                    requirement,
                    source,
                    edit,
                ));
            } else {
                for extra in extras {
                    let edit =
                        toml.add_optional_dependency(&extra, &requirement, source.as_ref())?;
                    edits.push(DependencyEdit::new(
                        DependencyType::Optional(extra),
                        requirement.clone(),
                        source.clone(),
                        edit,
                    ));
                }
            }
        }

        let content = toml.to_string();
        let modified = {
            if content == *project.pyproject_toml().raw {
                debug!("No changes to dependencies; skipping update");
                false
            } else {
                let pyproject_path = project.root().join("pyproject.toml");
                fs_err::write(pyproject_path, &content)?;
                true
            }
        };
        process_project_and_sync(
            &project,
            &mut toml,
            &content,
            false,
            false,
            raw_sources,
            &edits,
            uv_configuration::ExtrasSpecification::All,
            false,
            &venv,
            &settings,
            connectivity,
            concurrency,
            native_tls,
            cache,
            printer,
            no_sync,
            modified,
        )
        .await
    } else {
        Ok(ExitStatus::Success)
    }
}

/// Initialize a virtual workspace at the given path.
fn init_virtual_workspace(path: &Path, no_workspace: bool) -> Result<()> {
    // Ensure that we aren't creating a nested workspace.
    if !no_workspace {
        check_nested_workspaces(path, &DiscoveryOptions::default());
    }

    // Create the `pyproject.toml`.
    let pyproject = indoc::indoc! {r"
        [tool.uv.workspace]
        members = []
    "};

    fs_err::create_dir_all(path)?;
    fs_err::write(path.join("pyproject.toml"), pyproject)?;

    Ok(())
}

/// Initialize a project (and, implicitly, a workspace root) at the given path.
async fn init_project(
    path: &Path,
    name: &PackageName,
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
                ignore: std::iter::once(path).collect(),
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
                    warn_user_once!("`--no-workspace` was provided, but no workspace was found");
                }
                None
            }
            Err(err) => {
                // If the user runs with `--no-workspace`, ignore the error.
                if no_workspace {
                    warn_user_once!(
                        "Ignoring workspace discovery error due to `--no-workspace`: {err}"
                    );
                    None
                } else {
                    return Err(err.into());
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
                    Some(request),
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
            Some(request),
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

    // Create the `pyproject.toml`.
    let pyproject = indoc::formatdoc! {r#"
        [project]
        name = "{name}"
        version = "0.1.0"
        description = "Add your description here"{readme}
        requires-python = "{requires_python}"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
        readme = if no_readme { "" } else { "\nreadme = \"README.md\"" },
        requires_python = requires_python.specifiers(),
    };

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
