use std::borrow::Cow;
use std::fmt::Write as _;
use std::io;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::Result;

use owo_colors::OwoColorize;
use uv_distribution_filename::SourceDistExtension;
use uv_distribution_types::{DependencyMetadata, Index, IndexLocations};
use uv_install_wheel::linker::LinkMode;

use uv_auth::store_credentials;
use uv_cache::{Cache, CacheBucket};
use uv_client::{BaseClientBuilder, Connectivity, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{
    BuildKind, BuildOptions, BuildOutput, Concurrency, ConfigSettings, Constraints,
    HashCheckingMode, IndexStrategy, KeyringProviderType, LowerBound, SourceStrategy, TrustedHost,
};
use uv_dispatch::BuildDispatch;
use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_python::{
    EnvironmentPreference, PythonDownloads, PythonEnvironment, PythonInstallation,
    PythonPreference, PythonRequest, PythonVariant, PythonVersionFile, VersionRequest,
};
use uv_requirements::RequirementsSource;
use uv_resolver::{ExcludeNewer, FlatIndex, RequiresPython};
use uv_types::{BuildContext, BuildIsolation, HashStrategy};
use uv_workspace::{DiscoveryOptions, Workspace, WorkspaceError};

use crate::commands::pip::operations;
use crate::commands::project::find_requires_python;
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::{ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::{ResolverSettings, ResolverSettingsRef};

/// Build source distributions and wheels.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn build_frontend(
    project_dir: &Path,
    src: Option<PathBuf>,
    package: Option<PackageName>,
    all: bool,
    output_dir: Option<PathBuf>,
    sdist: bool,
    wheel: bool,
    build_logs: bool,
    build_constraints: Vec<RequirementsSource>,
    hash_checking: Option<HashCheckingMode>,
    python: Option<String>,
    settings: ResolverSettings,
    no_config: bool,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    let build_result = build_impl(
        project_dir,
        src.as_deref(),
        package.as_ref(),
        all,
        output_dir.as_deref(),
        sdist,
        wheel,
        build_logs,
        &build_constraints,
        hash_checking,
        python.as_deref(),
        settings.as_ref(),
        no_config,
        python_preference,
        python_downloads,
        connectivity,
        concurrency,
        native_tls,
        allow_insecure_host,
        cache,
        printer,
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
    all: bool,
    output_dir: Option<&Path>,
    sdist: bool,
    wheel: bool,
    build_logs: bool,
    build_constraints: &[RequirementsSource],
    hash_checking: Option<HashCheckingMode>,
    python_request: Option<&str>,
    settings: ResolverSettingsRef<'_>,
    no_config: bool,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    cache: &Cache,
    printer: Printer,
) -> Result<BuildResult> {
    // Extract the resolver settings.
    let ResolverSettingsRef {
        index_locations,
        index_strategy,
        keyring_provider,
        resolution: _,
        prerelease: _,
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
        .connectivity(connectivity)
        .native_tls(native_tls)
        .allow_insecure_host(allow_insecure_host.to_vec());

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
    let workspace = Workspace::discover(src.directory(), &DiscoveryOptions::default()).await;

    // If a `--package` or `--all` was provided, adjust the source directory.
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

        let project = workspace
            .packages()
            .get(package)
            .ok_or_else(|| anyhow::anyhow!("Package `{}` not found in workspace", package))?
            .root();

        vec![AnnotatedSource::from(Source::Directory(Cow::Borrowed(
            project,
        )))]
    } else if all {
        if matches!(src, Source::File(_)) {
            return Err(anyhow::anyhow!(
                "Cannot specify `--all` when building from a file"
            ));
        }

        let workspace = match workspace {
            Ok(ref workspace) => workspace,
            Err(err) => {
                return Err(anyhow::Error::from(err)
                    .context("`--all` was provided, but no workspace was found"));
            }
        };

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
            return Err(anyhow::anyhow!("No packages found in workspace"));
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
            build_constraints,
            no_build_isolation,
            no_build_isolation_package,
            native_tls,
            connectivity,
            index_strategy,
            keyring_provider,
            allow_insecure_host,
            exclude_newer,
            sources,
            concurrency,
            build_options,
            sdist,
            wheel,
            dependency_metadata,
            link_mode,
            config_setting,
        );
        async {
            let result = future.await;
            (source, result)
        }
    }))
    .await;

    for (source, result) in &results {
        match result {
            Ok(assets) => match assets {
                BuiltDistributions::Wheel(wheel) => {
                    writeln!(
                        printer.stderr(),
                        "Successfully built {}",
                        wheel.user_display().bold().cyan()
                    )?;
                }
                BuiltDistributions::Sdist(sdist) => {
                    writeln!(
                        printer.stderr(),
                        "Successfully built {}",
                        sdist.user_display().bold().cyan()
                    )?;
                }
                BuiltDistributions::Both(sdist, wheel) => {
                    writeln!(
                        printer.stderr(),
                        "Successfully built {} and {}",
                        sdist.user_display().bold().cyan(),
                        wheel.user_display().bold().cyan()
                    )?;
                }
            },
            Err(err) => {
                let mut causes = err.chain();

                let message = format!(
                    "{}: {}",
                    "error".red().bold(),
                    causes.next().unwrap().to_string().trim()
                );
                writeln!(printer.stderr(), "{}", source.annotate(&message))?;

                for err in causes {
                    writeln!(
                        printer.stderr(),
                        "  {}: {}",
                        "Caused by".red().bold(),
                        err.to_string().trim()
                    )?;
                }
            }
        }
    }

    if results.iter().any(|(_, result)| result.is_err()) {
        Ok(BuildResult::Failure)
    } else {
        Ok(BuildResult::Success)
    }
}

#[allow(clippy::fn_params_excessive_bools)]
async fn build_package(
    source: AnnotatedSource<'_>,
    output_dir: Option<&Path>,
    python_request: Option<&str>,
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
    build_constraints: &[RequirementsSource],
    no_build_isolation: bool,
    no_build_isolation_package: &[PackageName],
    native_tls: bool,
    connectivity: Connectivity,
    index_strategy: IndexStrategy,
    keyring_provider: KeyringProviderType,
    allow_insecure_host: &[TrustedHost],
    exclude_newer: Option<ExcludeNewer>,
    sources: SourceStrategy,
    concurrency: Concurrency,
    build_options: &BuildOptions,
    sdist: bool,
    wheel: bool,
    dependency_metadata: &DependencyMetadata,
    link_mode: LinkMode,
    config_setting: &ConfigSettings,
) -> Result<BuiltDistributions> {
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
        interpreter_request = PythonVersionFile::discover(source.directory(), no_config, false)
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
    )
    .await?
    .into_interpreter();

    // Add all authenticated sources to the cache.
    for index in index_locations.allowed_indexes() {
        if let Some(credentials) = index.credentials() {
            store_credentials(index.raw_url(), credentials);
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
            Some(&interpreter.resolver_markers()),
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
        .native_tls(native_tls)
        .connectivity(connectivity)
        .index_urls(index_locations.index_urls())
        .index_strategy(index_strategy)
        .keyring(keyring_provider)
        .allow_insecure_host(allow_insecure_host.to_vec())
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

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        build_constraints,
        &interpreter,
        index_locations,
        &flat_index,
        dependency_metadata,
        &state.index,
        &state.git,
        &state.capabilities,
        &state.in_flight,
        index_strategy,
        config_setting,
        build_isolation,
        link_mode,
        build_options,
        &hasher,
        exclude_newer,
        LowerBound::Allow,
        sources,
        concurrency,
    );

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

    // Determine the build plan.
    let plan = match &source.source {
        Source::File(_) => {
            // We're building from a file, which must be a source distribution.
            match (sdist, wheel) {
                (false, true) => BuildPlan::WheelFromSdist,
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
                (false, false) => BuildPlan::SdistToWheel,
                (false, true) => BuildPlan::Wheel,
                (true, false) => BuildPlan::Sdist,
                (true, true) => BuildPlan::SdistAndWheel,
            }
        }
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

    let assets = match plan {
        BuildPlan::SdistToWheel => {
            writeln!(
                printer.stderr(),
                "{}",
                source.annotate("Building source distribution...").bold()
            )?;

            // Build the sdist.
            let builder = build_dispatch
                .setup_build(
                    source.path(),
                    subdirectory,
                    source.path(),
                    version_id.map(ToString::to_string),
                    dist,
                    sources,
                    BuildKind::Sdist,
                    build_output,
                )
                .await?;
            let sdist = builder.build(&output_dir).await?;

            // Extract the source distribution into a temporary directory.
            let path = output_dir.join(&sdist);
            let reader = fs_err::tokio::File::open(&path).await?;
            let ext = SourceDistExtension::from_path(path.as_path()).map_err(|err| {
                anyhow::anyhow!("`{}` is not a valid source distribution, as it ends with an unsupported extension. Expected one of: {err}.", path.user_display())
            })?;
            let temp_dir = tempfile::tempdir_in(cache.bucket(CacheBucket::SourceDistributions))?;
            uv_extract::stream::archive(reader, ext, temp_dir.path()).await?;

            // Extract the top-level directory from the archive.
            let extracted = match uv_extract::strip_component(temp_dir.path()) {
                Ok(top_level) => top_level,
                Err(uv_extract::Error::NonSingularArchive(_)) => temp_dir.path().to_path_buf(),
                Err(err) => return Err(err.into()),
            };

            writeln!(
                printer.stderr(),
                "{}",
                source
                    .annotate("Building wheel from source distribution...")
                    .bold()
            )?;

            // Build a wheel from the source distribution.
            let builder = build_dispatch
                .setup_build(
                    &extracted,
                    subdirectory,
                    source.path(),
                    version_id.map(ToString::to_string),
                    dist,
                    sources,
                    BuildKind::Wheel,
                    build_output,
                )
                .await?;
            let wheel = builder.build(&output_dir).await?;

            BuiltDistributions::Both(output_dir.join(sdist), output_dir.join(wheel))
        }
        BuildPlan::Sdist => {
            writeln!(
                printer.stderr(),
                "{}",
                source.annotate("Building source distribution...").bold()
            )?;

            let builder = build_dispatch
                .setup_build(
                    source.path(),
                    subdirectory,
                    source.path(),
                    version_id.map(ToString::to_string),
                    dist,
                    sources,
                    BuildKind::Sdist,
                    build_output,
                )
                .await?;
            let sdist = builder.build(&output_dir).await?;

            BuiltDistributions::Sdist(output_dir.join(sdist))
        }
        BuildPlan::Wheel => {
            writeln!(
                printer.stderr(),
                "{}",
                source.annotate("Building wheel...").bold()
            )?;

            let builder = build_dispatch
                .setup_build(
                    source.path(),
                    subdirectory,
                    source.path(),
                    version_id.map(ToString::to_string),
                    dist,
                    sources,
                    BuildKind::Wheel,
                    build_output,
                )
                .await?;
            let wheel = builder.build(&output_dir).await?;

            BuiltDistributions::Wheel(output_dir.join(wheel))
        }
        BuildPlan::SdistAndWheel => {
            writeln!(
                printer.stderr(),
                "{}",
                source.annotate("Building source distribution...").bold()
            )?;
            let builder = build_dispatch
                .setup_build(
                    source.path(),
                    subdirectory,
                    source.path(),
                    version_id.map(ToString::to_string),
                    dist,
                    sources,
                    BuildKind::Sdist,
                    build_output,
                )
                .await?;
            let sdist = builder.build(&output_dir).await?;

            writeln!(
                printer.stderr(),
                "{}",
                source.annotate("Building wheel...").bold()
            )?;
            let builder = build_dispatch
                .setup_build(
                    source.path(),
                    subdirectory,
                    source.path(),
                    version_id.map(ToString::to_string),
                    dist,
                    sources,
                    BuildKind::Wheel,
                    build_output,
                )
                .await?;
            let wheel = builder.build(&output_dir).await?;

            BuiltDistributions::Both(output_dir.join(&sdist), output_dir.join(&wheel))
        }
        BuildPlan::WheelFromSdist => {
            writeln!(
                printer.stderr(),
                "{}",
                source
                    .annotate("Building wheel from source distribution...")
                    .bold()
            )?;

            // Extract the source distribution into a temporary directory.
            let reader = fs_err::tokio::File::open(source.path()).await?;
            let ext = SourceDistExtension::from_path(source.path()).map_err(|err| {
                anyhow::anyhow!("`{}` is not a valid build source. Expected to receive a source directory, or a source distribution ending in one of: {err}.", source.path().user_display())
            })?;
            let temp_dir = tempfile::tempdir_in(&output_dir)?;
            uv_extract::stream::archive(reader, ext, temp_dir.path()).await?;

            // Extract the top-level directory from the archive.
            let extracted = match uv_extract::strip_component(temp_dir.path()) {
                Ok(top_level) => top_level,
                Err(uv_extract::Error::NonSingularArchive(_)) => temp_dir.path().to_path_buf(),
                Err(err) => return Err(err.into()),
            };

            // Build a wheel from the source distribution.
            let builder = build_dispatch
                .setup_build(
                    &extracted,
                    subdirectory,
                    source.path(),
                    version_id.map(ToString::to_string),
                    dist,
                    sources,
                    BuildKind::Wheel,
                    build_output,
                )
                .await?;
            let wheel = builder.build(&output_dir).await?;

            BuiltDistributions::Wheel(output_dir.join(wheel))
        }
    };

    Ok(assets)
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

    fn annotate<'a>(&self, s: &'a str) -> Cow<'a, str> {
        if let Some(package) = &self.package {
            Cow::Owned(format!("[{}] {s}", package.cyan()))
        } else {
            Cow::Borrowed(s)
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Source<'a> {
    /// The input source is a file (i.e., a source distribution in a `.tar.gz` or `.zip` file).
    File(Cow<'a, Path>),
    /// The input source is a directory.
    Directory(Cow<'a, Path>),
}

impl<'a> Source<'a> {
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum BuiltDistributions {
    /// A built wheel.
    Wheel(PathBuf),
    /// A built source distribution.
    Sdist(PathBuf),
    /// A built source distribution and wheel.
    Both(PathBuf, PathBuf),
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
