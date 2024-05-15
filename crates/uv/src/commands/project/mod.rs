use std::fmt::Write;

use anyhow::{Context, Result};
use itertools::Itertools;
use owo_colors::OwoColorize;

use distribution_types::{IndexLocations, InstalledMetadata, LocalDist, Name, Resolution};
use install_wheel_rs::linker::LinkMode;
use pep508_rs::MarkerEnvironment;
use platform_tags::Tags;
use pypi_types::Yanked;
use uv_cache::Cache;
use uv_client::RegistryClient;
use uv_configuration::{Concurrency, Constraints, NoBinary, Overrides, Reinstall};
use uv_dispatch::BuildDispatch;
use uv_distribution::DistributionDatabase;
use uv_fs::Simplified;
use uv_installer::{Downloader, Plan, Planner, SitePackages};
use uv_interpreter::{find_default_python, Interpreter, PythonEnvironment};
use uv_requirements::{
    ExtrasSpecification, LookaheadResolver, NamedRequirementsResolver, RequirementsSpecification,
    SourceTreeResolver,
};
use uv_resolver::{
    Exclusions, FlatIndex, InMemoryIndex, Manifest, Options, PythonRequirement, ResolutionGraph,
    Resolver,
};
use uv_types::{HashStrategy, InFlight, InstalledPackagesProvider};

use crate::commands::project::discovery::Project;
use crate::commands::reporters::{DownloadReporter, InstallReporter, ResolverReporter};
use crate::commands::{elapsed, ChangeEvent, ChangeEventKind};
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
    installed_packages: &InstalledPackages,
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
    let editables = Vec::new();

    // Resolve the requirements from the provided sources.
    let requirements = {
        // Convert from unnamed to named requirements.
        let mut requirements = NamedRequirementsResolver::new(
            spec.requirements,
            hasher,
            index,
            DistributionDatabase::new(client, build_dispatch, concurrency.downloads),
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
    let lookaheads = LookaheadResolver::new(
        &requirements,
        &constraints,
        &overrides,
        &editables,
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
        spec.project,
        editables,
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
    concurrency: Concurrency,
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
