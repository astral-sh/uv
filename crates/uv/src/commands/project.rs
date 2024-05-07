use std::fmt::Write;

use anyhow::{Context, Result};
use itertools::Itertools;
use owo_colors::OwoColorize;

use distribution_types::{
    IndexLocations, InstalledMetadata, LocalDist, Name, Requirement, Resolution,
};
use install_wheel_rs::linker::LinkMode;
use pep508_rs::{MarkerEnvironment, PackageName};
use platform_tags::Tags;
use pypi_types::Yanked;
use uv_cache::Cache;
use uv_client::RegistryClient;
use uv_configuration::{Constraints, NoBinary, Overrides, Reinstall};
use uv_dispatch::BuildDispatch;
use uv_installer::{Downloader, Plan, Planner, SitePackages};
use uv_interpreter::{Interpreter, PythonEnvironment};
use uv_requirements::LookaheadResolver;
use uv_resolver::{
    Exclusions, FlatIndex, InMemoryIndex, Manifest, Options, ResolutionGraph, Resolver,
};
use uv_types::{EmptyInstalledPackages, HashStrategy, InFlight};

use crate::commands::reporters::{DownloadReporter, InstallReporter, ResolverReporter};
use crate::commands::{elapsed, ChangeEvent, ChangeEventKind};
use crate::printer::Printer;

/// Resolve a set of requirements, similar to running `pip compile`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn resolve(
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
) -> Result<ResolutionGraph, ProjectError> {
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
) -> Result<(), ProjectError> {
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
pub(crate) enum ProjectError {
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
