use crate::commands::project::find_requires_python;
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::{ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::{ResolverSettings, ResolverSettingsRef};
use std::borrow::Cow;

use crate::commands::pip::operations;
use anyhow::Result;
use distribution_filename::SourceDistExtension;
use owo_colors::OwoColorize;
use std::path::{Path, PathBuf};
use uv_auth::store_credentials_from_url;
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, Connectivity, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{BuildKind, BuildOutput, Concurrency, Constraints, HashCheckingMode};
use uv_dispatch::BuildDispatch;
use uv_fs::{Simplified, CWD};
use uv_normalize::PackageName;
use uv_python::{
    EnvironmentPreference, PythonDownloads, PythonEnvironment, PythonInstallation,
    PythonPreference, PythonRequest, PythonVersionFile, VersionRequest,
};
use uv_requirements::RequirementsSource;
use uv_resolver::{FlatIndex, RequiresPython};
use uv_types::{BuildContext, BuildIsolation, HashStrategy};
use uv_workspace::{DiscoveryOptions, Workspace};

/// Build source distributions and wheels.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn build(
    src: Option<PathBuf>,
    package: Option<PackageName>,
    output_dir: Option<PathBuf>,
    sdist: bool,
    wheel: bool,
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
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    let assets = build_impl(
        src.as_deref(),
        package.as_ref(),
        output_dir.as_deref(),
        sdist,
        wheel,
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
        cache,
        printer,
    )
    .await?;

    match assets {
        BuiltDistributions::Wheel(wheel) => {
            anstream::eprintln!("Successfully built {}", wheel.user_display().bold().cyan());
        }
        BuiltDistributions::Sdist(sdist) => {
            anstream::eprintln!("Successfully built {}", sdist.user_display().bold().cyan());
        }
        BuiltDistributions::Both(sdist, wheel) => {
            anstream::eprintln!(
                "Successfully built {} and {}",
                sdist.user_display().bold().cyan(),
                wheel.user_display().bold().cyan()
            );
        }
    }

    Ok(ExitStatus::Success)
}

#[allow(clippy::fn_params_excessive_bools)]
async fn build_impl(
    src: Option<&Path>,
    package: Option<&PackageName>,
    output_dir: Option<&Path>,
    sdist: bool,
    wheel: bool,
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
    cache: &Cache,
    printer: Printer,
) -> Result<BuiltDistributions> {
    // Extract the resolver settings.
    let ResolverSettingsRef {
        index_locations,
        index_strategy,
        keyring_provider,
        allow_insecure_host,
        resolution: _,
        prerelease: _,
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
        .native_tls(native_tls);

    // Determine the source to build.
    let src = if let Some(src) = src {
        let src = std::path::absolute(src)?;
        let metadata = match fs_err::tokio::metadata(&src).await {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
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
        Source::Directory(Cow::Borrowed(&*CWD))
    };

    // Attempt to discover the workspace; on failure, save the error for later.
    let workspace = Workspace::discover(src.directory(), &DiscoveryOptions::default()).await;

    // If a `--package` was provided, adjust the source directory.
    let src = if let Some(package) = package {
        if matches!(src, Source::File(_)) {
            return Err(anyhow::anyhow!(
                "Cannot specify a `--package` when building from a file"
            ));
        }

        let workspace = match workspace {
            Ok(ref workspace) => workspace,
            Err(err) => {
                return Err(
                    anyhow::anyhow!("`--package` was provided, but no workspace was found")
                        .context(err),
                )
            }
        };

        let project = workspace
            .packages()
            .get(package)
            .ok_or_else(|| anyhow::anyhow!("Package `{}` not found in workspace", package))?
            .root()
            .clone();

        Source::Directory(Cow::Owned(project))
    } else {
        src
    };

    let output_dir = if let Some(output_dir) = output_dir {
        Cow::Owned(std::path::absolute(output_dir)?)
    } else {
        match src {
            Source::Directory(ref src) => Cow::Owned(src.join("dist")),
            Source::File(ref src) => Cow::Borrowed(src.parent().unwrap()),
        }
    };

    // (1) Explicit request from user
    let mut interpreter_request = python_request.map(PythonRequest::parse);

    // (2) Request from `.python-version`
    if interpreter_request.is_none() {
        interpreter_request = PythonVersionFile::discover(src.directory(), no_config, false)
            .await?
            .and_then(PythonVersionFile::into_version);
    }

    // (3) `Requires-Python` in `pyproject.toml`
    if interpreter_request.is_none() {
        if let Ok(ref workspace) = workspace {
            interpreter_request = find_requires_python(workspace)?
                .as_ref()
                .map(RequiresPython::specifiers)
                .map(|specifiers| {
                    PythonRequest::Version(VersionRequest::Range(specifiers.clone()))
                });
        }
    }

    // Locate the Python interpreter to use in the environment.
    let interpreter = PythonInstallation::find_or_download(
        interpreter_request.as_ref(),
        EnvironmentPreference::Any,
        python_preference,
        python_downloads,
        &client_builder,
        cache,
        Some(&PythonDownloadReporter::single(printer)),
    )
    .await?
    .into_interpreter();

    // Add all authenticated sources to the cache.
    for url in index_locations.urls() {
        store_credentials_from_url(url);
    }

    // Read build constraints.
    let build_constraints =
        operations::read_constraints(build_constraints, &client_builder).await?;

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
        let entries = client.fetch(index_locations.flat_index()).await?;
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
        sources,
        concurrency,
    );

    // Create the output directory.
    fs_err::tokio::create_dir_all(&output_dir).await?;

    // Determine the build plan.
    let plan = match &src {
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
    let subdirectory = None;
    let version_id = src.path().file_name().and_then(|name| name.to_str());
    let dist = None;

    let assets = match plan {
        BuildPlan::SdistToWheel => {
            anstream::eprintln!("{}", "Building source distribution...".bold());

            // Build the sdist.
            let builder = build_dispatch
                .setup_build(
                    src.path(),
                    subdirectory,
                    version_id.map(ToString::to_string),
                    dist,
                    BuildKind::Sdist,
                    BuildOutput::Stderr,
                )
                .await?;
            let sdist = builder.build(&output_dir).await?;

            // Extract the source distribution into a temporary directory.
            let path = output_dir.join(&sdist);
            let reader = fs_err::tokio::File::open(&path).await?;
            let ext = SourceDistExtension::from_path(path.as_path()).map_err(|err| {
                anyhow::anyhow!("`{}` is not a valid source distribution, as it ends with an unsupported extension. Expected one of: {err}.", path.user_display())
            })?;
            let temp_dir = tempfile::tempdir_in(&output_dir)?;
            uv_extract::stream::archive(reader, ext, temp_dir.path()).await?;

            // Extract the top-level directory from the archive.
            let extracted = match uv_extract::strip_component(temp_dir.path()) {
                Ok(top_level) => top_level,
                Err(uv_extract::Error::NonSingularArchive(_)) => temp_dir.path().to_path_buf(),
                Err(err) => return Err(err.into()),
            };

            anstream::eprintln!("{}", "Building wheel from source distribution...".bold());

            // Build a wheel from the source distribution.
            let builder = build_dispatch
                .setup_build(
                    &extracted,
                    subdirectory,
                    version_id.map(ToString::to_string),
                    dist,
                    BuildKind::Wheel,
                    BuildOutput::Stderr,
                )
                .await?;
            let wheel = builder.build(&output_dir).await?;

            BuiltDistributions::Both(output_dir.join(sdist), output_dir.join(wheel))
        }
        BuildPlan::Sdist => {
            anstream::eprintln!("{}", "Building source distribution...".bold());

            let builder = build_dispatch
                .setup_build(
                    src.path(),
                    subdirectory,
                    version_id.map(ToString::to_string),
                    dist,
                    BuildKind::Sdist,
                    BuildOutput::Stderr,
                )
                .await?;
            let sdist = builder.build(&output_dir).await?;

            BuiltDistributions::Sdist(output_dir.join(sdist))
        }
        BuildPlan::Wheel => {
            anstream::eprintln!("{}", "Building wheel...".bold());

            let builder = build_dispatch
                .setup_build(
                    src.path(),
                    subdirectory,
                    version_id.map(ToString::to_string),
                    dist,
                    BuildKind::Wheel,
                    BuildOutput::Stderr,
                )
                .await?;
            let wheel = builder.build(&output_dir).await?;

            BuiltDistributions::Wheel(output_dir.join(wheel))
        }
        BuildPlan::SdistAndWheel => {
            anstream::eprintln!("{}", "Building source distribution...".bold());
            let builder = build_dispatch
                .setup_build(
                    src.path(),
                    subdirectory,
                    version_id.map(ToString::to_string),
                    dist,
                    BuildKind::Sdist,
                    BuildOutput::Stderr,
                )
                .await?;
            let sdist = builder.build(&output_dir).await?;

            anstream::eprintln!("{}", "Building wheel...".bold());
            let builder = build_dispatch
                .setup_build(
                    src.path(),
                    subdirectory,
                    version_id.map(ToString::to_string),
                    dist,
                    BuildKind::Wheel,
                    BuildOutput::Stderr,
                )
                .await?;
            let wheel = builder.build(&output_dir).await?;

            BuiltDistributions::Both(output_dir.join(&sdist), output_dir.join(&wheel))
        }
        BuildPlan::WheelFromSdist => {
            anstream::eprintln!("{}", "Building wheel from source distribution...".bold());

            // Extract the source distribution into a temporary directory.
            let reader = fs_err::tokio::File::open(src.path()).await?;
            let ext = SourceDistExtension::from_path(src.path()).map_err(|err| {
                anyhow::anyhow!("`{}` is not a valid build source. Expected to receive a source directory, or a source distribution ending in one of: {err}.", src.path().user_display())
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
                    version_id.map(ToString::to_string),
                    dist,
                    BuildKind::Wheel,
                    BuildOutput::Stderr,
                )
                .await?;
            let wheel = builder.build(&output_dir).await?;

            BuiltDistributions::Wheel(output_dir.join(wheel))
        }
    };

    Ok(assets)
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
