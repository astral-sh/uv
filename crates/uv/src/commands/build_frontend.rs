use std::borrow::Cow;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::{fmt, io};

use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use thiserror::Error;
use tracing::instrument;
use uv_auth::UrlAuthPolicies;

use crate::commands::pip::operations;
use crate::commands::project::{find_requires_python, ProjectError};
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::ExitStatus;
use crate::printer::Printer;
use crate::settings::{NetworkSettings, ResolverSettings};
use uv_build_backend::check_direct_build;
use uv_cache::{Cache, CacheBucket};
use uv_client::{BaseClientBuilder, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{
    BuildKind, BuildOptions, BuildOutput, Concurrency, ConfigSettings, Constraints,
    HashCheckingMode, IndexStrategy, KeyringProviderType, PreviewMode, SourceStrategy,
};
use uv_dispatch::{BuildDispatch, SharedState};
use uv_distribution_filename::{
    DistFilename, SourceDistExtension, SourceDistFilename, WheelFilename,
};
use uv_distribution_types::{DependencyMetadata, Index, IndexLocations, SourceDist};
use uv_fs::{relative_to, Simplified};
use uv_install_wheel::LinkMode;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_python::{
    EnvironmentPreference, PythonDownloads, PythonEnvironment, PythonInstallation,
    PythonPreference, PythonRequest, PythonVariant, PythonVersionFile, VersionFileDiscoveryOptions,
    VersionRequest,
};
use uv_requirements::RequirementsSource;
use uv_resolver::{ExcludeNewer, FlatIndex, RequiresPython};
use uv_settings::PythonInstallMirrors;
use uv_types::{AnyErrorBuild, BuildContext, BuildIsolation, BuildStack, HashStrategy};
use uv_workspace::{DiscoveryOptions, Workspace, WorkspaceCache, WorkspaceError};

#[derive(Debug, Error)]
enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    FindOrDownloadPython(#[from] uv_python::Error),
    #[error(transparent)]
    HashStrategy(#[from] uv_types::HashStrategyError),
    #[error(transparent)]
    FlatIndex(#[from] uv_client::FlatIndexError),
    #[error(transparent)]
    BuildPlan(anyhow::Error),
    #[error(transparent)]
    Extract(#[from] uv_extract::Error),
    #[error(transparent)]
    Operations(#[from] operations::Error),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    #[error(transparent)]
    BuildBackend(#[from] uv_build_backend::Error),
    #[error(transparent)]
    BuildDispatch(AnyErrorBuild),
    #[error(transparent)]
    BuildFrontend(#[from] uv_build_frontend::Error),
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error("Failed to write message")]
    Fmt(#[from] fmt::Error),
    #[error("Can't use `--force-pep517` with `--list`")]
    ListForcePep517,
    #[error("Can only use `--list` with the uv backend")]
    ListNonUv,
    #[error(
        "`{0}` is not a valid build source. Expected to receive a source directory, or a source \
         distribution ending in one of: {1}."
    )]
    InvalidSourceDistExt(String, uv_distribution_filename::ExtensionError),
    #[error("The built source distribution has an invalid filename")]
    InvalidBuiltSourceDistFilename(#[source] uv_distribution_filename::SourceDistFilenameError),
    #[error("The built wheel has an invalid filename")]
    InvalidBuiltWheelFilename(#[source] uv_distribution_filename::WheelFilenameError),
    #[error("The source distribution declares version {0}, but the wheel declares version {1}")]
    VersionMismatch(Version, Version),
}

/// Build source distributions and wheels.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn build_frontend(
    project_dir: &Path,
    src: Option<PathBuf>,
    package: Option<PackageName>,
    all_packages: bool,
    output_dir: Option<PathBuf>,
    sdist: bool,
    wheel: bool,
    list: bool,
    build_logs: bool,
    force_pep517: bool,
    build_constraints: Vec<RequirementsSource>,
    hash_checking: Option<HashCheckingMode>,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    settings: &ResolverSettings,
    network_settings: &NetworkSettings,
    no_config: bool,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    concurrency: Concurrency,
    cache: &Cache,
    printer: Printer,
    preview: PreviewMode,
) -> Result<ExitStatus> {
    let build_result = build_impl(
        project_dir,
        src.as_deref(),
        package.as_ref(),
        all_packages,
        output_dir.as_deref(),
        sdist,
        wheel,
        list,
        build_logs,
        force_pep517,
        &build_constraints,
        hash_checking,
        python.as_deref(),
        install_mirrors,
        settings,
        network_settings,
        no_config,
        python_preference,
        python_downloads,
        concurrency,
        cache,
        printer,
        preview,
    )
    .await?;

    match build_result {
        BuildResult::Failure => Ok(ExitStatus::Error),
        BuildResult::Success => Ok(ExitStatus::Success),
    }
}

/// Represents the overall result of a build process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuildResult {
    /// Indicates that at least one of the builds failed.
    Failure,
    /// Indicates that all builds succeeded.
    Success,
}

#[allow(clippy::fn_params_excessive_bools)]
async fn build_impl(
    project_dir: &Path,
    src: Option<&Path>,
    package: Option<&PackageName>,
    all_packages: bool,
    output_dir: Option<&Path>,
    sdist: bool,
    wheel: bool,
    list: bool,
    build_logs: bool,
    force_pep517: bool,
    build_constraints: &[RequirementsSource],
    hash_checking: Option<HashCheckingMode>,
    python_request: Option<&str>,
    install_mirrors: PythonInstallMirrors,
    settings: &ResolverSettings,
    network_settings: &NetworkSettings,
    no_config: bool,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    concurrency: Concurrency,
    cache: &Cache,
    printer: Printer,
    preview: PreviewMode,
) -> Result<BuildResult> {
    if list && preview.is_disabled() {
        // We need the direct build for list and that is preview only.
        writeln!(
            printer.stderr(),
            "The `--list` option is only available in preview mode; add the `--preview` flag to use `--list`"
        )?;
        return Ok(BuildResult::Failure);
    }

    // Extract the resolver settings.
    let ResolverSettings {
        index_locations,
        index_strategy,
        keyring_provider,
        resolution: _,
        prerelease: _,
        fork_strategy: _,
        dependency_metadata,
        config_setting,
        no_build_isolation,
        no_build_isolation_package,
        exclude_newer,
        link_mode,
        upgrade: _,
        build_options,
        sources,
    } = settings;

    let client_builder = BaseClientBuilder::default()
        .connectivity(network_settings.connectivity)
        .native_tls(network_settings.native_tls)
        .allow_insecure_host(network_settings.allow_insecure_host.clone());

    // Determine the source to build.
    let src = if let Some(src) = src {
        let src = std::path::absolute(src)?;
        let metadata = match fs_err::tokio::metadata(&src).await {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Err(anyhow::anyhow!(
                    "Source `{}` does not exist",
                    src.user_display()
                ));
            }
            Err(err) => return Err(err.into()),
        };
        if metadata.is_file() {
            Source::File(Cow::Owned(src))
        } else {
            Source::Directory(Cow::Owned(src))
        }
    } else {
        Source::Directory(Cow::Borrowed(project_dir))
    };

    // Attempt to discover the workspace; on failure, save the error for later.
    let workspace_cache = WorkspaceCache::default();
    let workspace = Workspace::discover(
        src.directory(),
        &DiscoveryOptions::default(),
        &workspace_cache,
    )
    .await;

    // If a `--package` or `--all-packages` was provided, adjust the source directory.
    let packages = if let Some(package) = package {
        if matches!(src, Source::File(_)) {
            return Err(anyhow::anyhow!(
                "Cannot specify `--package` when building from a file"
            ));
        }

        let workspace = match workspace {
            Ok(ref workspace) => workspace,
            Err(err) => {
                return Err(anyhow::Error::from(err)
                    .context("`--package` was provided, but no workspace was found"));
            }
        };

        let package = workspace
            .packages()
            .get(package)
            .ok_or_else(|| anyhow::anyhow!("Package `{package}` not found in workspace"))?;

        if !package.pyproject_toml().is_package() {
            let name = &package.project().name;
            let pyproject_toml = package.root().join("pyproject.toml");
            return Err(anyhow::anyhow!("Package `{}` is missing a `{}`. For example, to build with `{}`, add the following to `{}`:\n```toml\n[build-system]\nrequires = [\"setuptools\"]\nbuild-backend = \"setuptools.build_meta\"\n```", name.cyan(), "build-system".green(), "setuptools".cyan(), pyproject_toml.user_display().cyan()));
        }

        vec![AnnotatedSource::from(Source::Directory(Cow::Borrowed(
            package.root(),
        )))]
    } else if all_packages {
        if matches!(src, Source::File(_)) {
            return Err(anyhow::anyhow!(
                "Cannot specify `--all-packages` when building from a file"
            ));
        }

        let workspace = match workspace {
            Ok(ref workspace) => workspace,
            Err(err) => {
                return Err(anyhow::Error::from(err)
                    .context("`--all-packages` was provided, but no workspace was found"));
            }
        };

        if workspace.packages().is_empty() {
            return Err(anyhow::anyhow!("No packages found in workspace"));
        }

        let packages: Vec<_> = workspace
            .packages()
            .values()
            .filter(|package| package.pyproject_toml().is_package())
            .map(|package| AnnotatedSource {
                source: Source::Directory(Cow::Borrowed(package.root())),
                package: Some(package.project().name.clone()),
            })
            .collect();

        if packages.is_empty() {
            let member = workspace.packages().values().next().unwrap();
            let name = &member.project().name;
            let pyproject_toml = member.root().join("pyproject.toml");
            return Err(anyhow::anyhow!("Workspace does not contain any buildable packages. For example, to build `{}` with `{}`, add a `{}` to `{}`:\n```toml\n[build-system]\nrequires = [\"setuptools\"]\nbuild-backend = \"setuptools.build_meta\"\n```", name.cyan(), "setuptools".cyan(), "build-system".green(), pyproject_toml.user_display().cyan()));
        }

        packages
    } else {
        vec![AnnotatedSource::from(src)]
    };

    let results: Vec<_> = futures::future::join_all(packages.into_iter().map(|source| {
        let future = build_package(
            source.clone(),
            output_dir,
            python_request,
            install_mirrors.clone(),
            no_config,
            workspace.as_ref(),
            python_preference,
            python_downloads,
            cache,
            printer,
            index_locations,
            &client_builder,
            hash_checking,
            build_logs,
            force_pep517,
            build_constraints,
            *no_build_isolation,
            no_build_isolation_package,
            network_settings,
            *index_strategy,
            *keyring_provider,
            *exclude_newer,
            *sources,
            concurrency,
            build_options,
            sdist,
            wheel,
            list,
            dependency_metadata,
            *link_mode,
            config_setting,
            preview,
        );
        async {
            let result = future.await;
            (source, result)
        }
    }))
    .await;

    let mut success = true;
    for (source, result) in results {
        match result {
            Ok(messages) => {
                for message in messages {
                    message.print(printer)?;
                }
            }
            Err(err) => {
                #[derive(Debug, miette::Diagnostic, thiserror::Error)]
                #[error("Failed to build `{source}`", source = source.cyan())]
                #[diagnostic()]
                struct Diagnostic {
                    source: String,
                    #[source]
                    cause: anyhow::Error,
                }

                let report = miette::Report::new(Diagnostic {
                    source: source.to_string(),
                    cause: err.into(),
                });
                anstream::eprint!("{report:?}");

                success = false;
            }
        }
    }

    if success {
        Ok(BuildResult::Success)
    } else {
        Ok(BuildResult::Failure)
    }
}

#[allow(clippy::fn_params_excessive_bools)]
async fn build_package(
    source: AnnotatedSource<'_>,
    output_dir: Option<&Path>,
    python_request: Option<&str>,
    install_mirrors: PythonInstallMirrors,
    no_config: bool,
    workspace: Result<&Workspace, &WorkspaceError>,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    cache: &Cache,
    printer: Printer,
    index_locations: &IndexLocations,
    client_builder: &BaseClientBuilder<'_>,
    hash_checking: Option<HashCheckingMode>,
    build_logs: bool,
    force_pep517: bool,
    build_constraints: &[RequirementsSource],
    no_build_isolation: bool,
    no_build_isolation_package: &[PackageName],
    network_settings: &NetworkSettings,
    index_strategy: IndexStrategy,
    keyring_provider: KeyringProviderType,
    exclude_newer: Option<ExcludeNewer>,
    sources: SourceStrategy,
    concurrency: Concurrency,
    build_options: &BuildOptions,
    sdist: bool,
    wheel: bool,
    list: bool,
    dependency_metadata: &DependencyMetadata,
    link_mode: LinkMode,
    config_setting: &ConfigSettings,
    preview: PreviewMode,
) -> Result<Vec<BuildMessage>, Error> {
    let output_dir = if let Some(output_dir) = output_dir {
        Cow::Owned(std::path::absolute(output_dir)?)
    } else {
        if let Ok(workspace) = workspace {
            Cow::Owned(workspace.install_path().join("dist"))
        } else {
            match &source.source {
                Source::Directory(src) => Cow::Owned(src.join("dist")),
                Source::File(src) => Cow::Borrowed(src.parent().unwrap()),
            }
        }
    };

    // (1) Explicit request from user
    let mut interpreter_request = python_request.map(PythonRequest::parse);

    // (2) Request from `.python-version`
    if interpreter_request.is_none() {
        interpreter_request = PythonVersionFile::discover(
            source.directory(),
            &VersionFileDiscoveryOptions::default().with_no_config(no_config),
        )
        .await?
        .and_then(PythonVersionFile::into_version);
    }

    // (3) `Requires-Python` in `pyproject.toml`
    if interpreter_request.is_none() {
        if let Ok(workspace) = workspace {
            interpreter_request = find_requires_python(workspace)?
                .as_ref()
                .map(RequiresPython::specifiers)
                .map(|specifiers| {
                    PythonRequest::Version(VersionRequest::Range(
                        specifiers.clone(),
                        PythonVariant::Default,
                    ))
                });
        }
    }

    // Locate the Python interpreter to use in the environment.
    let interpreter = PythonInstallation::find_or_download(
        interpreter_request.as_ref(),
        EnvironmentPreference::Any,
        python_preference,
        python_downloads,
        client_builder,
        cache,
        Some(&PythonDownloadReporter::single(printer)),
        install_mirrors.python_install_mirror.as_deref(),
        install_mirrors.pypy_install_mirror.as_deref(),
    )
    .await?
    .into_interpreter();

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

    // Read build constraints.
    let build_constraints = operations::read_constraints(build_constraints, client_builder).await?;

    // Collect the set of required hashes.
    let hasher = if let Some(hash_checking) = hash_checking {
        HashStrategy::from_requirements(
            std::iter::empty(),
            build_constraints
                .iter()
                .map(|entry| (&entry.requirement, entry.hashes.as_slice())),
            Some(&interpreter.resolver_marker_environment()),
            hash_checking,
        )?
    } else {
        HashStrategy::None
    };

    let build_constraints = Constraints::from_requirements(
        build_constraints
            .iter()
            .map(|constraint| constraint.requirement.clone()),
    );

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(cache.clone())
        .native_tls(network_settings.native_tls)
        .connectivity(network_settings.connectivity)
        .allow_insecure_host(network_settings.allow_insecure_host.clone())
        .url_auth_policies(UrlAuthPolicies::from(index_locations))
        .index_urls(index_locations.index_urls())
        .index_strategy(index_strategy)
        .keyring(keyring_provider)
        .markers(interpreter.markers())
        .platform(interpreter.platform())
        .build();

    // Determine whether to enable build isolation.
    let environment;
    let build_isolation = if no_build_isolation {
        environment = PythonEnvironment::from_interpreter(interpreter.clone());
        BuildIsolation::Shared(&environment)
    } else if no_build_isolation_package.is_empty() {
        BuildIsolation::Isolated
    } else {
        environment = PythonEnvironment::from_interpreter(interpreter.clone());
        BuildIsolation::SharedPackage(&environment, no_build_isolation_package)
    };

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, cache);
        let entries = client
            .fetch(index_locations.flat_indexes().map(Index::url))
            .await?;
        FlatIndex::from_entries(entries, None, &hasher, build_options)
    };

    // Initialize any shared state.
    let state = SharedState::default();
    let workspace_cache = WorkspaceCache::default();

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        build_constraints,
        &interpreter,
        index_locations,
        &flat_index,
        dependency_metadata,
        state.clone(),
        index_strategy,
        config_setting,
        build_isolation,
        link_mode,
        build_options,
        &hasher,
        exclude_newer,
        sources,
        workspace_cache,
        concurrency,
        preview,
    );

    prepare_output_directory(&output_dir).await?;

    // Determine the build plan.
    let plan = BuildPlan::determine(&source, sdist, wheel).map_err(Error::BuildPlan)?;

    // Check if the build backend is matching uv version that allows calling in the uv build backend
    // directly.
    let build_action = if list {
        if force_pep517 {
            return Err(Error::ListForcePep517);
        }

        if !check_direct_build(source.path(), source.path().user_display()) {
            // TODO(konsti): Provide more context on what mismatched
            return Err(Error::ListNonUv);
        }

        BuildAction::List
    } else if preview.is_enabled()
        && !force_pep517
        && check_direct_build(source.path(), source.path().user_display())
    {
        BuildAction::DirectBuild
    } else {
        BuildAction::Pep517
    };

    // Prepare some common arguments for the build.
    let dist = None;
    let subdirectory = None;
    let version_id = source.path().file_name().and_then(|name| name.to_str());

    let build_output = match printer {
        Printer::Default | Printer::NoProgress | Printer::Verbose => {
            if build_logs {
                BuildOutput::Stderr
            } else {
                BuildOutput::Quiet
            }
        }
        Printer::Quiet => BuildOutput::Quiet,
    };

    let mut build_results = Vec::new();
    match plan {
        BuildPlan::SdistToWheel => {
            // Even when listing files, we still need to build the source distribution for the wheel
            // build.
            if list {
                let sdist_list = build_sdist(
                    source.path(),
                    &output_dir,
                    build_action,
                    &source,
                    printer,
                    "source distribution",
                    &build_dispatch,
                    sources,
                    dist,
                    subdirectory,
                    version_id,
                    build_output,
                )
                .await?;
                build_results.push(sdist_list);
            }
            let sdist_build = build_sdist(
                source.path(),
                &output_dir,
                build_action.force_build(),
                &source,
                printer,
                "source distribution",
                &build_dispatch,
                sources,
                dist,
                subdirectory,
                version_id,
                build_output,
            )
            .await?;
            build_results.push(sdist_build.clone());

            // Extract the source distribution into a temporary directory.
            let path = output_dir.join(sdist_build.raw_filename());
            let reader = fs_err::tokio::File::open(&path).await?;
            let ext = SourceDistExtension::from_path(path.as_path())
                .map_err(|err| Error::InvalidSourceDistExt(path.user_display().to_string(), err))?;
            let temp_dir = tempfile::tempdir_in(cache.bucket(CacheBucket::SourceDistributions))?;
            uv_extract::stream::archive(reader, ext, temp_dir.path()).await?;

            // Extract the top-level directory from the archive.
            let extracted = match uv_extract::strip_component(temp_dir.path()) {
                Ok(top_level) => top_level,
                Err(uv_extract::Error::NonSingularArchive(_)) => temp_dir.path().to_path_buf(),
                Err(err) => return Err(err.into()),
            };

            let wheel_build = build_wheel(
                &extracted,
                &output_dir,
                build_action,
                &source,
                printer,
                "wheel from source distribution",
                &build_dispatch,
                sources,
                dist,
                subdirectory,
                version_id,
                build_output,
                Some(sdist_build.normalized_filename().version()),
            )
            .await?;
            build_results.push(wheel_build);
        }
        BuildPlan::Sdist => {
            let sdist_build = build_sdist(
                source.path(),
                &output_dir,
                build_action,
                &source,
                printer,
                "source distribution",
                &build_dispatch,
                sources,
                dist,
                subdirectory,
                version_id,
                build_output,
            )
            .await?;
            build_results.push(sdist_build);
        }
        BuildPlan::Wheel => {
            let wheel_build = build_wheel(
                source.path(),
                &output_dir,
                build_action,
                &source,
                printer,
                "wheel",
                &build_dispatch,
                sources,
                dist,
                subdirectory,
                version_id,
                build_output,
                None,
            )
            .await?;
            build_results.push(wheel_build);
        }
        BuildPlan::SdistAndWheel => {
            let sdist_build = build_sdist(
                source.path(),
                &output_dir,
                build_action,
                &source,
                printer,
                "source distribution",
                &build_dispatch,
                sources,
                dist,
                subdirectory,
                version_id,
                build_output,
            )
            .await?;

            let wheel_build = build_wheel(
                source.path(),
                &output_dir,
                build_action,
                &source,
                printer,
                "wheel",
                &build_dispatch,
                sources,
                dist,
                subdirectory,
                version_id,
                build_output,
                Some(sdist_build.normalized_filename().version()),
            )
            .await?;
            build_results.push(sdist_build);
            build_results.push(wheel_build);
        }
        BuildPlan::WheelFromSdist => {
            // Extract the source distribution into a temporary directory.
            let reader = fs_err::tokio::File::open(source.path()).await?;
            let ext = SourceDistExtension::from_path(source.path()).map_err(|err| {
                Error::InvalidSourceDistExt(source.path().user_display().to_string(), err)
            })?;
            let temp_dir = tempfile::tempdir_in(&output_dir)?;
            uv_extract::stream::archive(reader, ext, temp_dir.path()).await?;

            // If the source distribution has a version in its filename, check the version.
            let version = source
                .path()
                .file_name()
                .and_then(|filename| filename.to_str())
                .and_then(|filename| SourceDistFilename::parsed_normalized_filename(filename).ok())
                .map(|filename| filename.version);

            // Extract the top-level directory from the archive.
            let extracted = match uv_extract::strip_component(temp_dir.path()) {
                Ok(top_level) => top_level,
                Err(uv_extract::Error::NonSingularArchive(_)) => temp_dir.path().to_path_buf(),
                Err(err) => return Err(err.into()),
            };

            let wheel_build = build_wheel(
                &extracted,
                &output_dir,
                build_action,
                &source,
                printer,
                "wheel from source distribution",
                &build_dispatch,
                sources,
                dist,
                subdirectory,
                version_id,
                build_output,
                version.as_ref(),
            )
            .await?;
            build_results.push(wheel_build);
        }
    }

    Ok(build_results)
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum BuildAction {
    /// Only list the files that would be included, don't actually build.
    List,
    /// Build by calling directly into the build backend.
    DirectBuild,
    /// Build through the PEP 517 hooks.
    Pep517,
}

impl BuildAction {
    /// If in list mode, still build the distribution.
    fn force_build(self) -> Self {
        match self {
            // List is only available for the uv build backend
            Self::List => Self::DirectBuild,
            Self::DirectBuild => Self::DirectBuild,
            Self::Pep517 => Self::Pep517,
        }
    }
}

/// Build a source distribution, either through PEP 517 or through a direct build.
#[instrument(skip_all)]
async fn build_sdist(
    source_tree: &Path,
    output_dir: &Path,
    action: BuildAction,
    source: &AnnotatedSource<'_>,
    printer: Printer,
    build_kind_message: &str,
    // Below is only used with PEP 517 builds
    build_dispatch: &BuildDispatch<'_>,
    sources: SourceStrategy,
    dist: Option<&SourceDist>,
    subdirectory: Option<&Path>,
    version_id: Option<&str>,
    build_output: BuildOutput,
) -> Result<BuildMessage, Error> {
    let build_result = match action {
        BuildAction::List => {
            let source_tree_ = source_tree.to_path_buf();
            let (filename, file_list) = tokio::task::spawn_blocking(move || {
                uv_build_backend::list_source_dist(&source_tree_, uv_version::version())
            })
            .await??;
            let raw_filename = filename.to_string();
            BuildMessage::List {
                normalized_filename: DistFilename::SourceDistFilename(filename),
                raw_filename,
                source_tree: source_tree.to_path_buf(),
                file_list,
            }
        }
        BuildAction::DirectBuild => {
            writeln!(
                printer.stderr(),
                "{}",
                format!(
                    "{}Building {} (uv build backend)...",
                    source.message_prefix(),
                    build_kind_message
                )
                .bold()
            )?;
            let source_tree = source_tree.to_path_buf();
            let output_dir_ = output_dir.to_path_buf();
            let filename = tokio::task::spawn_blocking(move || {
                uv_build_backend::build_source_dist(
                    &source_tree,
                    &output_dir_,
                    uv_version::version(),
                )
            })
            .await??
            .to_string();

            BuildMessage::Build {
                normalized_filename: DistFilename::SourceDistFilename(
                    SourceDistFilename::parsed_normalized_filename(&filename)
                        .map_err(Error::InvalidBuiltSourceDistFilename)?,
                ),
                raw_filename: filename,
                output_dir: output_dir.to_path_buf(),
            }
        }
        BuildAction::Pep517 => {
            writeln!(
                printer.stderr(),
                "{}",
                format!(
                    "{}Building {}...",
                    source.message_prefix(),
                    build_kind_message
                )
                .bold()
            )?;
            let builder = build_dispatch
                .setup_build(
                    source_tree,
                    subdirectory,
                    source.path(),
                    version_id,
                    dist,
                    sources,
                    BuildKind::Sdist,
                    build_output,
                    BuildStack::default(),
                )
                .await
                .map_err(|err| Error::BuildDispatch(err.into()))?;
            let filename = builder.build(output_dir).await?;
            BuildMessage::Build {
                normalized_filename: DistFilename::SourceDistFilename(
                    SourceDistFilename::parsed_normalized_filename(&filename)
                        .map_err(Error::InvalidBuiltSourceDistFilename)?,
                ),
                raw_filename: filename,
                output_dir: output_dir.to_path_buf(),
            }
        }
    };
    Ok(build_result)
}

/// Build a wheel, either through PEP 517 or through a direct build.
#[instrument(skip_all)]
async fn build_wheel(
    source_tree: &Path,
    output_dir: &Path,
    action: BuildAction,
    source: &AnnotatedSource<'_>,
    printer: Printer,
    build_kind_message: &str,
    // Below is only used with PEP 517 builds
    build_dispatch: &BuildDispatch<'_>,
    sources: SourceStrategy,
    dist: Option<&SourceDist>,
    subdirectory: Option<&Path>,
    version_id: Option<&str>,
    build_output: BuildOutput,
    // Used for checking version consistency
    version: Option<&Version>,
) -> Result<BuildMessage, Error> {
    let build_message = match action {
        BuildAction::List => {
            let source_tree_ = source_tree.to_path_buf();
            let (filename, file_list) = tokio::task::spawn_blocking(move || {
                uv_build_backend::list_wheel(&source_tree_, uv_version::version())
            })
            .await??;
            let raw_filename = filename.to_string();
            BuildMessage::List {
                normalized_filename: DistFilename::WheelFilename(filename),
                raw_filename,
                source_tree: source_tree.to_path_buf(),
                file_list,
            }
        }
        BuildAction::DirectBuild => {
            writeln!(
                printer.stderr(),
                "{}",
                format!(
                    "{}Building {} (uv build backend)...",
                    source.message_prefix(),
                    build_kind_message
                )
                .bold()
            )?;
            let source_tree = source_tree.to_path_buf();
            let output_dir_ = output_dir.to_path_buf();
            let filename = tokio::task::spawn_blocking(move || {
                uv_build_backend::build_wheel(
                    &source_tree,
                    &output_dir_,
                    None,
                    uv_version::version(),
                )
            })
            .await??;

            let raw_filename = filename.to_string();
            BuildMessage::Build {
                normalized_filename: DistFilename::WheelFilename(filename),
                raw_filename,
                output_dir: output_dir.to_path_buf(),
            }
        }
        BuildAction::Pep517 => {
            writeln!(
                printer.stderr(),
                "{}",
                format!(
                    "{}Building {}...",
                    source.message_prefix(),
                    build_kind_message
                )
                .bold()
            )?;
            let builder = build_dispatch
                .setup_build(
                    source_tree,
                    subdirectory,
                    source.path(),
                    version_id,
                    dist,
                    sources,
                    BuildKind::Wheel,
                    build_output,
                    BuildStack::default(),
                )
                .await
                .map_err(|err| Error::BuildDispatch(err.into()))?;
            let filename = builder.build(output_dir).await?;
            BuildMessage::Build {
                normalized_filename: DistFilename::WheelFilename(
                    WheelFilename::from_str(&filename).map_err(Error::InvalidBuiltWheelFilename)?,
                ),
                raw_filename: filename,
                output_dir: output_dir.to_path_buf(),
            }
        }
    };
    if let Some(expected) = version {
        let actual = build_message.normalized_filename().version();
        if expected != actual {
            return Err(Error::VersionMismatch(expected.clone(), actual.clone()));
        }
    }
    Ok(build_message)
}

/// Create the output directory and add a `.gitignore`.
async fn prepare_output_directory(output_dir: &Path) -> Result<(), Error> {
    // Create the output directory.
    fs_err::tokio::create_dir_all(&output_dir).await?;

    // Add a .gitignore.
    match fs_err::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output_dir.join(".gitignore"))
    {
        Ok(mut file) => file.write_all(b"*")?,
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => (),
        Err(err) => return Err(err.into()),
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AnnotatedSource<'a> {
    /// The underlying [`Source`] to build.
    source: Source<'a>,
    /// The package name, if known.
    package: Option<PackageName>,
}

impl AnnotatedSource<'_> {
    fn path(&self) -> &Path {
        self.source.path()
    }

    fn directory(&self) -> &Path {
        self.source.directory()
    }

    fn message_prefix(&self) -> Cow<'_, str> {
        if let Some(package) = &self.package {
            Cow::Owned(format!("[{}] ", package.cyan()))
        } else {
            Cow::Borrowed("")
        }
    }
}

impl<'a> From<Source<'a>> for AnnotatedSource<'a> {
    fn from(source: Source<'a>) -> Self {
        Self {
            source,
            package: None,
        }
    }
}

impl fmt::Display for AnnotatedSource<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(package) = &self.package {
            write!(f, "{} @ {}", package, self.path().simplified_display())
        } else {
            write!(f, "{}", self.path().simplified_display())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Source<'a> {
    /// The input source is a file (i.e., a source distribution in a `.tar.gz` or `.zip` file).
    File(Cow<'a, Path>),
    /// The input source is a directory.
    Directory(Cow<'a, Path>),
}

impl Source<'_> {
    fn path(&self) -> &Path {
        match self {
            Self::File(path) => path.as_ref(),
            Self::Directory(path) => path.as_ref(),
        }
    }

    fn directory(&self) -> &Path {
        match self {
            Self::File(path) => path.parent().unwrap(),
            Self::Directory(path) => path,
        }
    }
}

/// We run all builds in parallel, so we wait until all builds are done to show the success messages
/// in order.
#[derive(Debug, Clone)]
enum BuildMessage {
    /// A built wheel or source distribution.
    Build {
        /// The normalized name of the built distribution.
        normalized_filename: DistFilename,
        /// The name of the built distribution before parsing and normalization.
        raw_filename: String,
        /// The location of the built distribution.
        output_dir: PathBuf,
    },
    /// Show the list of files that would be included in a distribution.
    List {
        /// The normalized name of the build distribution.
        normalized_filename: DistFilename,
        /// The name of the built distribution before parsing and normalization.
        raw_filename: String,
        // All source files are relative to the source tree.
        source_tree: PathBuf,
        // Included file and source file, if not generated.
        file_list: Vec<(String, Option<PathBuf>)>,
    },
}

impl BuildMessage {
    /// The normalized filename of the wheel or source distribution.
    fn normalized_filename(&self) -> &DistFilename {
        match self {
            BuildMessage::Build {
                normalized_filename: name,
                ..
            } => name,
            BuildMessage::List {
                normalized_filename: name,
                ..
            } => name,
        }
    }

    /// The filename of the wheel or source distribution before normalization.
    fn raw_filename(&self) -> &str {
        match self {
            BuildMessage::Build {
                raw_filename: name, ..
            } => name,
            BuildMessage::List {
                raw_filename: name, ..
            } => name,
        }
    }

    fn print(&self, printer: Printer) -> Result<()> {
        match self {
            BuildMessage::Build {
                raw_filename,
                output_dir,
                ..
            } => {
                writeln!(
                    printer.stderr(),
                    "Successfully built {}",
                    output_dir.join(raw_filename).user_display().bold().cyan()
                )?;
            }
            BuildMessage::List {
                raw_filename,
                file_list,
                source_tree,
                ..
            } => {
                writeln!(
                    printer.stdout(),
                    "{}",
                    format!("Building {raw_filename} will include the following files:").bold()
                )?;
                for (file, source) in file_list {
                    if let Some(source) = source {
                        writeln!(
                            printer.stdout(),
                            "{file} ({})",
                            relative_to(source, source_tree)
                                .context("Included files must be relative to source tree")?
                                .display()
                        )?;
                    } else {
                        writeln!(printer.stdout(), "{file} (generated)")?;
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum BuildPlan {
    /// Build a source distribution from source, then build the wheel from the source distribution.
    SdistToWheel,

    /// Build a source distribution from source.
    Sdist,

    /// Build a wheel from source.
    Wheel,

    /// Build a source distribution and a wheel from source.
    SdistAndWheel,

    /// Build a wheel from a source distribution.
    WheelFromSdist,
}

impl BuildPlan {
    fn determine(source: &AnnotatedSource, sdist: bool, wheel: bool) -> Result<Self> {
        Ok(match &source.source {
            Source::File(_) => {
                // We're building from a file, which must be a source distribution.
                match (sdist, wheel) {
                    (false, true) => Self::WheelFromSdist,
                    (false, false) => {
                        return Err(anyhow::anyhow!(
                            "Pass `--wheel` explicitly to build a wheel from a source distribution"
                        ));
                    }
                    (true, _) => {
                        return Err(anyhow::anyhow!(
                            "Building an `--sdist` from a source distribution is not supported"
                        ));
                    }
                }
            }
            Source::Directory(_) => {
                // We're building from a directory.
                match (sdist, wheel) {
                    (false, false) => Self::SdistToWheel,
                    (false, true) => Self::Wheel,
                    (true, false) => Self::Sdist,
                    (true, true) => Self::SdistAndWheel,
                }
            }
        })
    }
}
