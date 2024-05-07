use crate::commands::reporters::{DownloadReporter, InstallReporter, ResolverReporter};
use crate::commands::ExitStatus;
use crate::commands::{elapsed, ChangeEvent, ChangeEventKind};
use crate::printer::Printer;
use anyhow::{Context, Result};
use distribution_types::{
    IndexLocations, InstalledMetadata, LocalDist, Name, Requirement, Resolution,
};
use install_wheel_rs::linker::LinkMode;
use itertools::Itertools;
use owo_colors::OwoColorize;
use pep508_rs::{MarkerEnvironment, PackageName};
use platform_tags::Tags;
use pypi_types::Yanked;
use std::ffi::OsString;
use std::fmt::Write;
use std::path::PathBuf;
use std::{env, iter};
use tempfile::{tempdir_in, TempDir};
use tokio::process::Command;
use tracing::debug;
use uv_warnings::warn_user;

use uv_cache::Cache;
use uv_client::{BaseClientBuilder, RegistryClient, RegistryClientBuilder};
use uv_configuration::{
    ConfigSettings, Constraints, NoBinary, NoBuild, Overrides, PreviewMode, Reinstall,
    SetupPyStrategy,
};
use uv_dispatch::BuildDispatch;
use uv_fs::Simplified;
use uv_installer::{Downloader, Plan, Planner, SatisfiesResult, SitePackages};
use uv_interpreter::{Interpreter, PythonEnvironment};
use uv_requirements::{
    ExtrasSpecification, LookaheadResolver, NamedRequirementsResolver, RequirementsSource,
    RequirementsSpecification, SourceTreeResolver,
};
use uv_resolver::{
    Exclusions, FlatIndex, InMemoryIndex, Manifest, Options, OptionsBuilder, ResolutionGraph,
    Resolver,
};
use uv_types::{BuildIsolation, EmptyInstalledPackages, HashStrategy, InFlight};

/// Run a command.
#[allow(clippy::unnecessary_wraps, clippy::too_many_arguments)]
pub(crate) async fn run(
    target: Option<String>,
    mut args: Vec<OsString>,
    mut requirements: Vec<RequirementsSource>,
    python: Option<String>,
    isolated: bool,
    preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user!("`uv run` is experimental and may change without warning.");
    }

    let command = if let Some(target) = target {
        let target_path = PathBuf::from(&target);
        if target_path
            .extension()
            .map_or(false, |ext| ext.eq_ignore_ascii_case("py"))
            && target_path.exists()
        {
            args.insert(0, target_path.as_os_str().into());
            "python".to_string()
        } else {
            target
        }
    } else {
        "python".to_string()
    };

    // Copy the requirements into a set of overrides; we'll use this to prioritize
    // requested requirements over those discovered in the project.
    // We must retain these requirements as direct dependencies too, as overrides
    // cannot be applied to transitive dependencies.
    let overrides = requirements.clone();

    if !isolated {
        if let Some(workspace_requirements) = find_workspace_requirements()? {
            requirements.extend(workspace_requirements);
        }
    }

    // Detect the current Python interpreter.
    // TODO(zanieb): Create ephemeral environments
    // TODO(zanieb): Accept `--python`
    let run_env = environment_for_run(
        &requirements,
        &overrides,
        python.as_deref(),
        isolated,
        preview,
        cache,
        printer,
    )
    .await?;
    let python_env = run_env.python;

    // Construct the command
    let mut process = Command::new(&command);
    process.args(&args);

    // Set up the PATH
    debug!(
        "Using Python {} environment at {}",
        python_env.interpreter().python_version(),
        python_env.python_executable().user_display().cyan()
    );
    let new_path = if let Some(path) = std::env::var_os("PATH") {
        let python_env_path =
            iter::once(python_env.scripts().to_path_buf()).chain(env::split_paths(&path));
        env::join_paths(python_env_path)?
    } else {
        OsString::from(python_env.scripts())
    };

    process.env("PATH", new_path);

    // Spawn and wait for completion
    // Standard input, output, and error streams are all inherited
    // TODO(zanieb): Throw a nicer error message if the command is not found
    let space = if args.is_empty() { "" } else { " " };
    debug!(
        "Running `{command}{space}{}`",
        args.iter().map(|arg| arg.to_string_lossy()).join(" ")
    );
    let mut handle = process.spawn()?;
    let status = handle.wait().await?;

    // Exit based on the result of the command
    // TODO(zanieb): Do we want to exit with the code of the child process? Probably.
    if status.success() {
        Ok(ExitStatus::Success)
    } else {
        Ok(ExitStatus::Failure)
    }
}

struct RunEnvironment {
    /// The Python environment to execute the run in.
    python: PythonEnvironment,
    /// A temporary directory, if a new virtual environment was created.
    ///
    /// Included to ensure that the temporary directory exists for the length of the operation, but
    /// is dropped at the end as appropriate.
    _temp_dir_drop: Option<TempDir>,
}

fn find_workspace_requirements() -> Result<Option<Vec<RequirementsSource>>> {
    // TODO(zanieb): Add/use workspace logic to load requirements for a workspace
    // We cannot use `Workspace::find` yet because it depends on a `[tool.uv]` section
    let pyproject_path = std::env::current_dir()?.join("pyproject.toml");
    if pyproject_path.exists() {
        debug!(
            "Loading requirements from {}",
            pyproject_path.user_display()
        );
        return Ok(Some(vec![
            RequirementsSource::from_requirements_file(pyproject_path),
            RequirementsSource::from_package(".".to_string()),
        ]));
    }

    Ok(None)
}

/// Returns an environment for a `run` invocation.
///
/// Will use the current virtual environment (if any) unless `isolated` is true.
/// Will create virtual environments in a temporary directory (if necessary).
async fn environment_for_run(
    requirements: &[RequirementsSource],
    overrides: &[RequirementsSource],
    python: Option<&str>,
    isolated: bool,
    preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> Result<RunEnvironment> {
    let current_venv = if isolated {
        None
    } else {
        // Find the active environment if it exists
        match PythonEnvironment::from_virtualenv(cache) {
            Ok(env) => Some(env),
            Err(uv_interpreter::Error::VenvNotFound) => None,
            Err(err) => return Err(err.into()),
        }
    };

    // TODO(zanieb): Support client configuration
    let client_builder = BaseClientBuilder::default();

    // Read all requirements from the provided sources.
    // TODO(zanieb): Consider allowing constraints and extras
    // TODO(zanieb): Allow specifying extras somehow
    let spec = RequirementsSpecification::from_sources(
        requirements,
        &[],
        overrides,
        &ExtrasSpecification::None,
        &client_builder,
        preview,
    )
    .await?;

    // Determine an interpreter to use
    let python_env = if let Some(python) = python {
        PythonEnvironment::from_requested_python(python, cache)?
    } else {
        PythonEnvironment::from_default_python(cache)?
    };

    // Check if the current environment satisfies the requirements
    if let Some(venv) = current_venv {
        // Ensure it matches the selected interpreter
        // TODO(zanieb): We should check if a version was requested and see if the environment meets that
        //               too but this can wait until we refactor interpreter discovery
        if venv.root() == python_env.root() {
            // Determine the set of installed packages.
            let site_packages = SitePackages::from_executable(&venv)?;

            // If the requirements are already satisfied, we're done. Ideally, the resolver would be fast
            // enough to let us remove this check. But right now, for large environments, it's an order of
            // magnitude faster to validate the environment than to resolve the requirements.
            if spec.source_trees.is_empty() {
                match site_packages.satisfies(
                    &spec.requirements,
                    &spec.editables,
                    &spec.constraints,
                )? {
                    SatisfiesResult::Fresh {
                        recursive_requirements,
                    } => {
                        debug!(
                            "All requirements satisfied: {}",
                            recursive_requirements
                                .iter()
                                .map(|entry| entry.requirement.to_string())
                                .sorted()
                                .join(" | ")
                        );
                        debug!(
                            "All editables satisfied: {}",
                            spec.editables.iter().map(ToString::to_string).join(", ")
                        );
                        return Ok(RunEnvironment {
                            python: venv,
                            _temp_dir_drop: None,
                        });
                    }
                    SatisfiesResult::Unsatisfied(requirement) => {
                        debug!("At least one requirement is not satisfied: {requirement}");
                    }
                }
            }
        }
    }
    // Otherwise, we need a new environment

    // Create a virtual environment
    // TODO(zanieb): Move this path derivation elsewhere
    let uv_state_path = std::env::current_dir()?.join(".uv");
    fs_err::create_dir_all(&uv_state_path)?;
    let tmpdir = tempdir_in(uv_state_path)?;
    let venv = uv_virtualenv::create_venv(
        tmpdir.path(),
        python_env.into_interpreter(),
        uv_virtualenv::Prompt::None,
        false,
        false,
    )?;

    // Determine the tags, markers, and interpreter to use for resolution.
    let interpreter = venv.interpreter().clone();
    let tags = venv.interpreter().tags()?;
    let markers = venv.interpreter().markers();

    // Collect the set of required hashes.
    // TODO(zanieb): Support hash checking
    let hasher = HashStrategy::None;

    // TODO(zanieb): Support index url configs
    let index_locations = IndexLocations::default();

    // TODO(zanieb): Support client options e.g. offline, tls, etc.
    // Initialize the registry client.
    let client = RegistryClientBuilder::new(cache.clone())
        .markers(markers)
        .platform(interpreter.platform())
        .build();

    // TODO(zanieb): Consider support for find links
    let flat_index = FlatIndex::default();

    // TODO(zanieb): Consider support for shared builds
    // Determine whether to enable build isolation.
    let build_isolation = BuildIsolation::Isolated;

    // TODO(zanieb): Consider no-binary and no-build support
    let no_build = NoBuild::None;
    let no_binary = NoBinary::None;

    // Create a shared in-memory index.
    let index = InMemoryIndex::default();

    // Track in-flight downloads, builds, etc., across resolutions.
    let in_flight = InFlight::default();

    let link_mode = LinkMode::default();
    let config_settings = ConfigSettings::default();

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        &interpreter,
        &index_locations,
        &flat_index,
        &index,
        &in_flight,
        SetupPyStrategy::default(),
        &config_settings,
        build_isolation,
        link_mode,
        &no_build,
        &no_binary,
    );
    // TODO(zanieb): Consider `exclude-newer` support

    // Resolve the requirements from the provided sources.
    let requirements = {
        // Convert from unnamed to named requirements.
        let mut requirements = NamedRequirementsResolver::new(
            spec.requirements,
            &hasher,
            &build_dispatch,
            &client,
            &index,
        )
        .with_reporter(ResolverReporter::from(printer))
        .resolve()
        .await?;

        // Resolve any source trees into requirements.
        if !spec.source_trees.is_empty() {
            requirements.extend(
                SourceTreeResolver::new(
                    spec.source_trees,
                    &ExtrasSpecification::None,
                    &hasher,
                    &build_dispatch,
                    &client,
                    &index,
                )
                .with_reporter(ResolverReporter::from(printer))
                .resolve()
                .await?,
            );
        }

        requirements
    };

    let options = OptionsBuilder::new()
        // TODO(zanieb): Support resolver options
        // .resolution_mode(resolution_mode)
        // .prerelease_mode(prerelease_mode)
        // .dependency_mode(dependency_mode)
        // .exclude_newer(exclude_newer)
        .build();

    // Resolve the requirements.
    let resolution = match resolve(
        requirements,
        spec.project,
        &hasher,
        &interpreter,
        tags,
        markers,
        &client,
        &flat_index,
        &index,
        &build_dispatch,
        options,
        printer,
    )
    .await
    {
        Ok(resolution) => Resolution::from(resolution),
        Err(err) => return Err(err.into()),
    };

    // Re-initialize the in-flight map.
    let in_flight = InFlight::default();

    // Sync the environment.
    install(
        &resolution,
        SitePackages::from_executable(&venv)?,
        &no_binary,
        link_mode,
        &index_locations,
        &hasher,
        tags,
        &client,
        &in_flight,
        &build_dispatch,
        cache,
        &venv,
        printer,
    )
    .await?;

    Ok(RunEnvironment {
        python: venv,
        _temp_dir_drop: Some(tmpdir),
    })
}

/// Resolve a set of requirements, similar to running `pip compile`.
#[allow(clippy::too_many_arguments)]
async fn resolve(
    requirements: Vec<Requirement>,
    project: Option<PackageName>,
    hasher: &HashStrategy,
    interpreter: &Interpreter,
    tags: &Tags,
    markers: &MarkerEnvironment,
    client: &RegistryClient,
    flat_index: &FlatIndex,
    index: &InMemoryIndex,
    build_dispatch: &BuildDispatch<'_>,
    options: Options,
    printer: Printer,
) -> Result<ResolutionGraph, Error> {
    let start = std::time::Instant::now();
    let exclusions = Exclusions::None;
    let preferences = Vec::new();
    let constraints = Constraints::default();
    let overrides = Overrides::default();
    let editables = Vec::new();
    let installed_packages = EmptyInstalledPackages;

    // Determine any lookahead requirements.
    let lookaheads = LookaheadResolver::new(
        &requirements,
        &constraints,
        &overrides,
        &editables,
        hasher,
        build_dispatch,
        client,
        index,
    )
    .with_reporter(ResolverReporter::from(printer))
    .resolve(markers)
    .await?;

    // Create a manifest of the requirements.
    let manifest = Manifest::new(
        requirements,
        constraints,
        overrides,
        preferences,
        project,
        editables,
        exclusions,
        lookaheads,
    );

    // Resolve the dependencies.
    let resolver = Resolver::new(
        manifest,
        options,
        markers,
        interpreter,
        tags,
        client,
        flat_index,
        index,
        hasher,
        build_dispatch,
        &installed_packages,
    )?
    .with_reporter(ResolverReporter::from(printer));
    let resolution = resolver.resolve().await?;

    let s = if resolution.len() == 1 { "" } else { "s" };
    writeln!(
        printer.stderr(),
        "{}",
        format!(
            "Resolved {} in {}",
            format!("{} package{}", resolution.len(), s).bold(),
            elapsed(start.elapsed())
        )
        .dimmed()
    )?;

    // Notify the user of any diagnostics.
    for diagnostic in resolution.diagnostics() {
        writeln!(
            printer.stderr(),
            "{}{} {}",
            "warning".yellow().bold(),
            ":".bold(),
            diagnostic.message().bold()
        )?;
    }

    Ok(resolution)
}

/// Install a set of requirements into the current environment.
#[allow(clippy::too_many_arguments)]
async fn install(
    resolution: &Resolution,
    site_packages: SitePackages<'_>,
    no_binary: &NoBinary,
    link_mode: LinkMode,
    index_urls: &IndexLocations,
    hasher: &HashStrategy,
    tags: &Tags,
    client: &RegistryClient,
    in_flight: &InFlight,
    build_dispatch: &BuildDispatch<'_>,
    cache: &Cache,
    venv: &PythonEnvironment,
    printer: Printer,
) -> Result<(), Error> {
    let start = std::time::Instant::now();

    let requirements = resolution.requirements();

    // Partition into those that should be linked from the cache (`local`), those that need to be
    // downloaded (`remote`), and those that should be removed (`extraneous`).
    let plan = Planner::with_requirements(&requirements)
        .build(
            site_packages,
            &Reinstall::None,
            no_binary,
            hasher,
            index_urls,
            cache,
            venv,
            tags,
        )
        .context("Failed to determine installation plan")?;

    let Plan {
        cached,
        remote,
        reinstalls,
        installed: _,
        extraneous: _,
    } = plan;

    // Nothing to do.
    if remote.is_empty() && cached.is_empty() {
        let s = if resolution.len() == 1 { "" } else { "s" };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Audited {} in {}",
                format!("{} package{}", resolution.len(), s).bold(),
                elapsed(start.elapsed())
            )
            .dimmed()
        )?;
        return Ok(());
    }

    // Map any registry-based requirements back to those returned by the resolver.
    let remote = remote
        .iter()
        .map(|dist| {
            resolution
                .get_remote(&dist.name)
                .cloned()
                .expect("Resolution should contain all packages")
        })
        .collect::<Vec<_>>();

    // Download, build, and unzip any missing distributions.
    let wheels = if remote.is_empty() {
        vec![]
    } else {
        let start = std::time::Instant::now();

        let downloader = Downloader::new(cache, tags, hasher, client, build_dispatch)
            .with_reporter(DownloadReporter::from(printer).with_length(remote.len() as u64));

        let wheels = downloader
            .download(remote.clone(), in_flight)
            .await
            .context("Failed to download distributions")?;

        let s = if wheels.len() == 1 { "" } else { "s" };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Downloaded {} in {}",
                format!("{} package{}", wheels.len(), s).bold(),
                elapsed(start.elapsed())
            )
            .dimmed()
        )?;

        wheels
    };

    // Install the resolved distributions.
    let wheels = wheels.into_iter().chain(cached).collect::<Vec<_>>();
    if !wheels.is_empty() {
        let start = std::time::Instant::now();
        uv_installer::Installer::new(venv)
            .with_link_mode(link_mode)
            .with_reporter(InstallReporter::from(printer).with_length(wheels.len() as u64))
            .install(&wheels)?;

        let s = if wheels.len() == 1 { "" } else { "s" };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Installed {} in {}",
                format!("{} package{}", wheels.len(), s).bold(),
                elapsed(start.elapsed())
            )
            .dimmed()
        )?;
    }

    for event in reinstalls
        .into_iter()
        .map(|distribution| ChangeEvent {
            dist: LocalDist::from(distribution),
            kind: ChangeEventKind::Removed,
        })
        .chain(wheels.into_iter().map(|distribution| ChangeEvent {
            dist: LocalDist::from(distribution),
            kind: ChangeEventKind::Added,
        }))
        .sorted_unstable_by(|a, b| {
            a.dist
                .name()
                .cmp(b.dist.name())
                .then_with(|| a.kind.cmp(&b.kind))
                .then_with(|| a.dist.installed_version().cmp(&b.dist.installed_version()))
        })
    {
        match event.kind {
            ChangeEventKind::Added => {
                writeln!(
                    printer.stderr(),
                    " {} {}{}",
                    "+".green(),
                    event.dist.name().as_ref().bold(),
                    event.dist.installed_version().to_string().dimmed()
                )?;
            }
            ChangeEventKind::Removed => {
                writeln!(
                    printer.stderr(),
                    " {} {}{}",
                    "-".red(),
                    event.dist.name().as_ref().bold(),
                    event.dist.installed_version().to_string().dimmed()
                )?;
            }
        }
    }

    // TODO(konstin): Also check the cache whether any cached or installed dist is already known to
    // have been yanked, we currently don't show this message on the second run anymore
    for dist in &remote {
        let Some(file) = dist.file() else {
            continue;
        };
        match &file.yanked {
            None | Some(Yanked::Bool(false)) => {}
            Some(Yanked::Bool(true)) => {
                writeln!(
                    printer.stderr(),
                    "{}{} {dist} is yanked.",
                    "warning".yellow().bold(),
                    ":".bold(),
                )?;
            }
            Some(Yanked::Reason(reason)) => {
                writeln!(
                    printer.stderr(),
                    "{}{} {dist} is yanked (reason: \"{reason}\").",
                    "warning".yellow().bold(),
                    ":".bold(),
                )?;
            }
        }
    }

    Ok(())
}

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error(transparent)]
    Resolve(#[from] uv_resolver::ResolveError),

    #[error(transparent)]
    Client(#[from] uv_client::Error),

    #[error(transparent)]
    Platform(#[from] platform_tags::PlatformError),

    #[error(transparent)]
    Hash(#[from] uv_types::HashStrategyError),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),

    #[error(transparent)]
    Lookahead(#[from] uv_requirements::LookaheadError),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}
