use std::fmt::Write;

use anyhow::{Context, Result};
use itertools::Itertools;
use owo_colors::OwoColorize;

use distribution_types::{IndexLocations, InstalledMetadata, LocalDist, Name, Resolution};
use install_wheel_rs::linker::LinkMode;
use pep508_rs::MarkerEnvironment;
use platform_tags::Tags;
use pypi_types::Yanked;
use tracing::debug;
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, RegistryClient, RegistryClientBuilder};
use uv_configuration::{
    Concurrency, ConfigSettings, Constraints, NoBinary, NoBuild, Overrides, PreviewMode, Reinstall,
    SetupPyStrategy,
};
use uv_dispatch::BuildDispatch;
use uv_distribution::DistributionDatabase;
use uv_fs::Simplified;
use uv_installer::{Downloader, Plan, Planner, SatisfiesResult, SitePackages};
use uv_interpreter::{find_default_python, Interpreter, PythonEnvironment};
use uv_requirements::{
    ExtrasSpecification, LookaheadResolver, NamedRequirementsResolver, RequirementsSource,
    RequirementsSpecification, SourceTreeResolver,
};
use uv_resolver::{
    Exclusions, FlatIndex, InMemoryIndex, Manifest, Options, OptionsBuilder, PythonRequirement,
    ResolutionGraph, Resolver,
};
use uv_types::{BuildIsolation, HashStrategy, InFlight, InstalledPackagesProvider};

use crate::commands::project::discovery::Project;
use crate::commands::reporters::{DownloadReporter, InstallReporter, ResolverReporter};
use crate::commands::{elapsed, ChangeEvent, ChangeEventKind};
use crate::editables::ResolvedEditables;
use crate::printer::Printer;

mod discovery;
pub(crate) mod lock;
pub(crate) mod run;
pub(crate) mod sync;

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error(transparent)]
    Resolve(#[from] uv_resolver::ResolveError),

    #[error(transparent)]
    Client(#[from] uv_client::Error),

    #[error(transparent)]
    Platform(#[from] platform_tags::PlatformError),

    #[error(transparent)]
    Interpreter(#[from] uv_interpreter::Error),

    #[error(transparent)]
    Virtualenv(#[from] uv_virtualenv::Error),

    #[error(transparent)]
    Hash(#[from] uv_types::HashStrategyError),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),

    #[error(transparent)]
    Lookahead(#[from] uv_requirements::LookaheadError),

    #[error(transparent)]
    ParsedUrl(#[from] Box<distribution_types::ParsedUrlError>),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

/// Initialize a virtual environment for the current project.
pub(crate) fn init(
    project: &Project,
    cache: &Cache,
    printer: Printer,
) -> Result<PythonEnvironment, Error> {
    let venv = project.root().join(".venv");

    // Discover or create the virtual environment.
    // TODO(charlie): If the environment isn't compatible with `--python`, recreate it.
    match PythonEnvironment::from_root(&venv, cache) {
        Ok(venv) => Ok(venv),
        Err(uv_interpreter::Error::VenvDoesNotExist(_)) => {
            // TODO(charlie): Respect `--python`; if unset, respect `Requires-Python`.
            let interpreter = find_default_python(cache)?;

            writeln!(
                printer.stderr(),
                "Using Python {} interpreter at: {}",
                interpreter.python_version(),
                interpreter.sys_executable().user_display().cyan()
            )?;

            writeln!(
                printer.stderr(),
                "Creating virtualenv at: {}",
                venv.user_display().cyan()
            )?;

            Ok(uv_virtualenv::create_venv(
                &venv,
                interpreter,
                uv_virtualenv::Prompt::None,
                false,
                false,
            )?)
        }
        Err(e) => Err(e.into()),
    }
}

/// Resolve a set of requirements, similar to running `pip compile`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn resolve<InstalledPackages: InstalledPackagesProvider>(
    spec: RequirementsSpecification,
    installed_packages: InstalledPackages,
    editables: &ResolvedEditables,
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
    concurrency: Concurrency,
) -> Result<ResolutionGraph, Error> {
    let start = std::time::Instant::now();

    let exclusions = Exclusions::None;
    let preferences = Vec::new();
    let constraints = Constraints::default();
    let overrides = Overrides::default();
    let python_requirement = PythonRequirement::from_marker_environment(interpreter, markers);

    let RequirementsSpecification {
        project,
        requirements,
        constraints: _,
        overrides: _,
        editables: _,
        source_trees,
        extras: _,
        index_url: _,
        extra_index_urls: _,
        no_index: _,
        find_links: _,
        no_binary: _,
        no_build: _,
    } = spec;

    // Resolve the requirements from the provided sources.
    let requirements = {
        // Convert from unnamed to named requirements.
        let mut requirements = NamedRequirementsResolver::new(
            requirements,
            hasher,
            index,
            DistributionDatabase::new(client, build_dispatch, concurrency.downloads),
        )
        .with_reporter(ResolverReporter::from(printer))
        .resolve()
        .await?;

        // Resolve any source trees into requirements.
        if !source_trees.is_empty() {
            requirements.extend(
                SourceTreeResolver::new(
                    source_trees,
                    &ExtrasSpecification::None,
                    hasher,
                    index,
                    DistributionDatabase::new(client, build_dispatch, concurrency.downloads),
                )
                .with_reporter(ResolverReporter::from(printer))
                .resolve()
                .await?,
            );
        }

        requirements
    };

    // Determine any lookahead requirements.
    let editable_metadata = editables.as_metadata().map_err(Error::ParsedUrl)?;
    let lookaheads = LookaheadResolver::new(
        &requirements,
        &constraints,
        &overrides,
        &editable_metadata,
        hasher,
        index,
        DistributionDatabase::new(client, build_dispatch, concurrency.downloads),
    )
    .with_reporter(ResolverReporter::from(printer))
    .resolve(Some(markers))
    .await?;

    // Create a manifest of the requirements.
    let manifest = Manifest::new(
        requirements,
        constraints,
        overrides,
        preferences,
        project,
        editable_metadata,
        exclusions,
        lookaheads,
    );

    // Resolve the dependencies.
    let resolver = Resolver::new(
        manifest,
        options,
        &python_requirement,
        Some(markers),
        tags,
        flat_index,
        index,
        hasher,
        build_dispatch,
        installed_packages,
        DistributionDatabase::new(client, build_dispatch, concurrency.downloads),
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
pub(crate) async fn install(
    resolution: &Resolution,
    resolved_editables: ResolvedEditables,
    site_packages: SitePackages,
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
    concurrency: Concurrency,
) -> Result<(), Error> {
    let start = std::time::Instant::now();

    let requirements = resolution.requirements();

    // Partition into those that should be linked from the cache (`local`), those that need to be
    // downloaded (`remote`), and those that should be removed (`extraneous`).
    let plan = Planner::with_requirements(&requirements)
        .with_editable_requirements(&resolved_editables.editables)
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

        let downloader = Downloader::new(
            cache,
            tags,
            hasher,
            DistributionDatabase::new(client, build_dispatch, concurrency.downloads),
        )
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

/// Update a [`PythonEnvironment`] to satisfy a set of [`RequirementsSource`]s.
async fn update_environment(
    venv: PythonEnvironment,
    requirements: &[RequirementsSource],
    preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> Result<PythonEnvironment> {
    // TODO(zanieb): Support client configuration
    let client_builder = BaseClientBuilder::default();

    // Read all requirements from the provided sources.
    // TODO(zanieb): Consider allowing constraints and extras
    // TODO(zanieb): Allow specifying extras somehow
    let spec = RequirementsSpecification::from_sources(
        requirements,
        &[],
        &[],
        &ExtrasSpecification::None,
        &client_builder,
        preview,
    )
    .await?;

    // Check if the current environment satisfies the requirements
    let site_packages = SitePackages::from_executable(&venv)?;

    // If the requirements are already satisfied, we're done.
    if spec.source_trees.is_empty() {
        match site_packages.satisfies(&spec.requirements, &spec.editables, &spec.constraints)? {
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
                return Ok(venv);
            }
            SatisfiesResult::Unsatisfied(requirement) => {
                debug!("At least one requirement is not satisfied: {requirement}");
            }
        }
    }

    // Determine the tags, markers, and interpreter to use for resolution.
    let interpreter = venv.interpreter().clone();
    let tags = venv.interpreter().tags()?;
    let markers = venv.interpreter().markers();

    // Initialize the registry client.
    // TODO(zanieb): Support client options e.g. offline, tls, etc.
    let client = RegistryClientBuilder::new(cache.clone())
        .markers(markers)
        .platform(venv.interpreter().platform())
        .build();

    // TODO(charlie): Respect project configuration.
    let build_isolation = BuildIsolation::default();
    let config_settings = ConfigSettings::default();
    let flat_index = FlatIndex::default();
    let hasher = HashStrategy::default();
    let in_flight = InFlight::default();
    let index = InMemoryIndex::default();
    let index_locations = IndexLocations::default();
    let link_mode = LinkMode::default();
    let no_binary = NoBinary::default();
    let no_build = NoBuild::default();
    let setup_py = SetupPyStrategy::default();
    let concurrency = Concurrency::default();
    let reinstall = Reinstall::None;

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        &interpreter,
        &index_locations,
        &flat_index,
        &index,
        &in_flight,
        setup_py,
        &config_settings,
        build_isolation,
        link_mode,
        &no_build,
        &no_binary,
        concurrency,
    );

    let options = OptionsBuilder::new()
        // TODO(zanieb): Support resolver options
        // .resolution_mode(resolution_mode)
        // .prerelease_mode(prerelease_mode)
        // .dependency_mode(dependency_mode)
        // .exclude_newer(exclude_newer)
        .build();

    // Build all editable distributions. The editables are shared between resolution and
    // installation, and should live for the duration of the command.
    let editables = ResolvedEditables::resolve(
        spec.editables.clone(),
        &site_packages,
        &reinstall,
        &hasher,
        &interpreter,
        tags,
        cache,
        &client,
        &build_dispatch,
        concurrency,
        printer,
    )
    .await?;

    // Resolve the requirements.
    let resolution = match resolve(
        spec,
        site_packages.clone(),
        &editables,
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
        concurrency,
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
        editables,
        site_packages,
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
        concurrency,
    )
    .await?;

    Ok(venv)
}
