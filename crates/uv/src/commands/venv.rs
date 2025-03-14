use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::vec;

use anstream::eprint;
use anyhow::Result;
use miette::{Diagnostic, IntoDiagnostic};
use owo_colors::OwoColorize;
use thiserror::Error;

use uv_cache::Cache;
use uv_client::{BaseClientBuilder, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{
    BuildOptions, Concurrency, ConfigSettings, Constraints, IndexStrategy, KeyringProviderType,
    NoBinary, NoBuild, PreviewMode, SourceStrategy,
};
use uv_dispatch::{BuildDispatch, SharedState};
use uv_distribution_types::{DependencyMetadata, Index, IndexLocations};
use uv_fs::Simplified;
use uv_install_wheel::LinkMode;
use uv_pypi_types::Requirement;
use uv_python::{
    EnvironmentPreference, PythonDownloads, PythonInstallation, PythonPreference, PythonRequest,
};
use uv_resolver::{ExcludeNewer, FlatIndex};
use uv_settings::PythonInstallMirrors;
use uv_shell::{shlex_posix, shlex_windows, Shell};
use uv_types::{AnyErrorBuild, BuildContext, BuildIsolation, BuildStack, HashStrategy};
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, VirtualProject, WorkspaceCache, WorkspaceError};

use crate::commands::pip::loggers::{DefaultInstallLogger, InstallLogger};
use crate::commands::pip::operations::{report_interpreter, Changelog};
use crate::commands::project::{validate_project_requires_python, WorkspacePython};
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::ExitStatus;
use crate::printer::Printer;
use crate::settings::NetworkSettings;

/// Create a virtual environment.
#[allow(clippy::unnecessary_wraps, clippy::fn_params_excessive_bools)]
pub(crate) async fn venv(
    project_dir: &Path,
    path: Option<PathBuf>,
    python_request: Option<&str>,
    install_mirrors: PythonInstallMirrors,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    link_mode: LinkMode,
    index_locations: &IndexLocations,
    index_strategy: IndexStrategy,
    dependency_metadata: DependencyMetadata,
    keyring_provider: KeyringProviderType,
    network_settings: &NetworkSettings,
    prompt: uv_virtualenv::Prompt,
    system_site_packages: bool,
    seed: bool,
    allow_existing: bool,
    exclude_newer: Option<ExcludeNewer>,
    concurrency: Concurrency,
    no_config: bool,
    no_project: bool,
    cache: &Cache,
    printer: Printer,
    relocatable: bool,
    preview: PreviewMode,
) -> Result<ExitStatus> {
    match venv_impl(
        project_dir,
        path,
        python_request,
        install_mirrors,
        link_mode,
        index_locations,
        index_strategy,
        dependency_metadata,
        keyring_provider,
        network_settings,
        prompt,
        system_site_packages,
        seed,
        python_preference,
        python_downloads,
        allow_existing,
        exclude_newer,
        concurrency,
        no_config,
        no_project,
        cache,
        printer,
        relocatable,
        preview,
    )
    .await
    {
        Ok(status) => Ok(status),
        Err(err) => {
            eprint!("{err:?}");
            Ok(ExitStatus::Failure)
        }
    }
}

#[derive(Error, Debug, Diagnostic)]
enum VenvError {
    #[error("Failed to create virtualenv")]
    #[diagnostic(code(uv::venv::creation))]
    Creation(#[source] uv_virtualenv::Error),

    #[error("Failed to install seed packages")]
    #[diagnostic(code(uv::venv::seed))]
    Seed(#[source] AnyErrorBuild),

    #[error("Failed to extract interpreter tags")]
    #[diagnostic(code(uv::venv::tags))]
    Tags(#[source] uv_platform_tags::TagsError),

    #[error("Failed to resolve `--find-links` entry")]
    #[diagnostic(code(uv::venv::flat_index))]
    FlatIndex(#[source] uv_client::FlatIndexError),
}

/// Create a virtual environment.
#[allow(clippy::fn_params_excessive_bools)]
async fn venv_impl(
    project_dir: &Path,
    path: Option<PathBuf>,
    python_request: Option<&str>,
    install_mirrors: PythonInstallMirrors,
    link_mode: LinkMode,
    index_locations: &IndexLocations,
    index_strategy: IndexStrategy,
    dependency_metadata: DependencyMetadata,
    keyring_provider: KeyringProviderType,
    network_settings: &NetworkSettings,
    prompt: uv_virtualenv::Prompt,
    system_site_packages: bool,
    seed: bool,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    allow_existing: bool,
    exclude_newer: Option<ExcludeNewer>,
    concurrency: Concurrency,
    no_config: bool,
    no_project: bool,
    cache: &Cache,
    printer: Printer,
    relocatable: bool,
    preview: PreviewMode,
) -> miette::Result<ExitStatus> {
    let workspace_cache = WorkspaceCache::default();
    let project = if no_project {
        None
    } else {
        match VirtualProject::discover(project_dir, &DiscoveryOptions::default(), &workspace_cache)
            .await
        {
            Ok(project) => Some(project),
            Err(WorkspaceError::MissingProject(_)) => None,
            Err(WorkspaceError::MissingPyprojectToml) => None,
            Err(WorkspaceError::NonWorkspace(_)) => None,
            Err(WorkspaceError::Toml(path, err)) => {
                warn_user!(
                    "Failed to parse `{}` during environment creation:\n{}",
                    path.user_display().cyan(),
                    textwrap::indent(&err.to_string(), "  ")
                );
                None
            }
            Err(err) => {
                warn_user!("{err}");
                None
            }
        }
    };

    // Determine the default path; either the virtual environment for the project or `.venv`
    let path = path.unwrap_or(
        project
            .as_ref()
            .and_then(|project| {
                // Only use the project environment path if we're invoked from the root
                // This isn't strictly necessary and we may want to change it later, but this
                // avoids a breaking change when adding project environment support to `uv venv`.
                (project.workspace().install_path() == project_dir)
                    .then(|| project.workspace().venv(Some(false)))
            })
            .unwrap_or(PathBuf::from(".venv")),
    );

    let client_builder = BaseClientBuilder::default()
        .connectivity(network_settings.connectivity)
        .native_tls(network_settings.native_tls)
        .allow_insecure_host(network_settings.allow_insecure_host.clone());

    let reporter = PythonDownloadReporter::single(printer);

    let WorkspacePython {
        source,
        python_request,
        requires_python,
    } = WorkspacePython::from_request(
        python_request.map(PythonRequest::parse),
        project.as_ref().map(VirtualProject::workspace),
        project_dir,
        no_config,
    )
    .await
    .into_diagnostic()?;

    // Locate the Python interpreter to use in the environment
    let interpreter = {
        let python = PythonInstallation::find_or_download(
            python_request.as_ref(),
            EnvironmentPreference::OnlySystem,
            python_preference,
            python_downloads,
            &client_builder,
            cache,
            Some(&reporter),
            install_mirrors.python_install_mirror.as_deref(),
            install_mirrors.pypy_install_mirror.as_deref(),
        )
        .await
        .into_diagnostic()?;
        report_interpreter(&python, false, printer).into_diagnostic()?;
        python.into_interpreter()
    };

    // Add all authenticated sources to the cache.
    for index in index_locations.allowed_indexes() {
        if let Some(credentials) = index.credentials() {
            let credentials = Arc::new(credentials);
            uv_auth::store_credentials(index.raw_url(), credentials.clone());
            if let Some(root_url) = index.root_url() {
                uv_auth::store_credentials(&root_url, credentials.clone());
            }
        }
    }

    // Check if the discovered Python version is incompatible with the current workspace
    if let Some(requires_python) = requires_python {
        match validate_project_requires_python(
            &interpreter,
            project.as_ref().map(VirtualProject::workspace),
            &requires_python,
            &source,
        ) {
            Ok(()) => {}
            Err(err) => {
                warn_user!("{err}");
            }
        }
    };

    writeln!(
        printer.stderr(),
        "Creating virtual environment {}at: {}",
        if seed { "with seed packages " } else { "" },
        path.user_display().cyan()
    )
    .into_diagnostic()?;

    // Create the virtual environment.
    let venv = uv_virtualenv::create_venv(
        &path,
        interpreter,
        prompt,
        system_site_packages,
        allow_existing,
        relocatable,
        seed,
    )
    .map_err(VenvError::Creation)?;

    // Install seed packages.
    if seed {
        // Extract the interpreter.
        let interpreter = venv.interpreter();

        // Add all authenticated sources to the cache.
        for index in index_locations.allowed_indexes() {
            if let Some(credentials) = index.credentials() {
                let credentials = Arc::new(credentials);
                uv_auth::store_credentials(index.raw_url(), credentials.clone());
                if let Some(root_url) = index.root_url() {
                    uv_auth::store_credentials(&root_url, credentials.clone());
                }
            }
        }

        // Instantiate a client.
        let client = RegistryClientBuilder::try_from(client_builder)
            .into_diagnostic()?
            .cache(cache.clone())
            .index_urls(index_locations.index_urls())
            .index_strategy(index_strategy)
            .keyring(keyring_provider)
            .allow_insecure_host(network_settings.allow_insecure_host.clone())
            .markers(interpreter.markers())
            .platform(interpreter.platform())
            .build();

        // Resolve the flat indexes from `--find-links`.
        let flat_index = {
            let tags = interpreter.tags().map_err(VenvError::Tags)?;
            let client = FlatIndexClient::new(&client, cache);
            let entries = client
                .fetch(index_locations.flat_indexes().map(Index::url))
                .await
                .map_err(VenvError::FlatIndex)?;
            FlatIndex::from_entries(
                entries,
                Some(tags),
                &HashStrategy::None,
                &BuildOptions::new(NoBinary::None, NoBuild::All),
            )
        };

        // Initialize any shared state.
        let state = SharedState::default();
        let workspace_cache = WorkspaceCache::default();

        // For seed packages, assume a bunch of default settings are sufficient.
        let build_constraints = Constraints::default();
        let build_hasher = HashStrategy::default();
        let config_settings = ConfigSettings::default();
        let sources = SourceStrategy::Disabled;

        // Do not allow builds
        let build_options = BuildOptions::new(NoBinary::None, NoBuild::All);

        // Prep the build context.
        let build_dispatch = BuildDispatch::new(
            &client,
            cache,
            build_constraints,
            interpreter,
            index_locations,
            &flat_index,
            &dependency_metadata,
            state.clone(),
            index_strategy,
            &config_settings,
            BuildIsolation::Isolated,
            link_mode,
            &build_options,
            &build_hasher,
            exclude_newer,
            sources,
            workspace_cache,
            concurrency,
            preview,
        );

        // Resolve the seed packages.
        let requirements = if interpreter.python_tuple() >= (3, 12) {
            vec![Requirement::from(
                uv_pep508::Requirement::from_str("pip").unwrap(),
            )]
        } else {
            // Include `setuptools` and `wheel` on Python <3.12.
            vec![
                Requirement::from(uv_pep508::Requirement::from_str("pip").unwrap()),
                Requirement::from(uv_pep508::Requirement::from_str("setuptools").unwrap()),
                Requirement::from(uv_pep508::Requirement::from_str("wheel").unwrap()),
            ]
        };

        let build_stack = BuildStack::default();

        // Resolve and install the requirements.
        //
        // Since the virtual environment is empty, and the set of requirements is trivial (no
        // constraints, no editables, etc.), we can use the build dispatch APIs directly.
        let resolution = build_dispatch
            .resolve(&requirements, &build_stack)
            .await
            .map_err(|err| VenvError::Seed(err.into()))?;
        let installed = build_dispatch
            .install(&resolution, &venv, &build_stack)
            .await
            .map_err(|err| VenvError::Seed(err.into()))?;

        let changelog = Changelog::from_installed(installed);
        DefaultInstallLogger
            .on_complete(&changelog, printer)
            .into_diagnostic()?;
    }

    // Determine the appropriate activation command.
    let activation = match Shell::from_env() {
        None => None,
        Some(Shell::Bash | Shell::Zsh | Shell::Ksh) => Some(format!(
            "source {}",
            shlex_posix(venv.scripts().join("activate"))
        )),
        Some(Shell::Fish) => Some(format!(
            "source {}",
            shlex_posix(venv.scripts().join("activate.fish"))
        )),
        Some(Shell::Nushell) => Some(format!(
            "overlay use {}",
            shlex_posix(venv.scripts().join("activate.nu"))
        )),
        Some(Shell::Csh) => Some(format!(
            "source {}",
            shlex_posix(venv.scripts().join("activate.csh"))
        )),
        Some(Shell::Powershell) => Some(shlex_windows(
            venv.scripts().join("activate"),
            Shell::Powershell,
        )),
        Some(Shell::Cmd) => Some(shlex_windows(venv.scripts().join("activate"), Shell::Cmd)),
    };
    if let Some(act) = activation {
        writeln!(printer.stderr(), "Activate with: {}", act.green()).into_diagnostic()?;
    }

    Ok(ExitStatus::Success)
}
