//! Common operations shared across the `pip` API and subcommands.

use std::fmt::Write;

use anyhow::{anyhow, Context};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::debug;

use distribution_types::Requirement;
use distribution_types::{
    DistributionMetadata, IndexLocations, InstalledMetadata, InstalledVersion, LocalDist, Name,
    ParsedUrl, RequirementSource, Resolution,
};
use install_wheel_rs::linker::LinkMode;
use pep440_rs::{VersionSpecifier, VersionSpecifiers};
use pep508_rs::{MarkerEnvironment, VerbatimUrl};
use platform_tags::Tags;
use pypi_types::Yanked;
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, RegistryClient};
use uv_configuration::{
    Concurrency, Constraints, NoBinary, Overrides, PreviewMode, Reinstall, Upgrade,
};
use uv_dispatch::BuildDispatch;
use uv_distribution::DistributionDatabase;
use uv_fs::Simplified;
use uv_installer::{Downloader, Plan, Planner, ResolvedEditable, SitePackages};
use uv_interpreter::{Interpreter, PythonEnvironment};
use uv_normalize::PackageName;
use uv_requirements::{
    ExtrasSpecification, LookaheadResolver, RequirementsSource, RequirementsSpecification,
};
use uv_resolver::{
    DependencyMode, Exclusions, FlatIndex, InMemoryIndex, Manifest, Options, Preference,
    PythonRequirement, ResolutionGraph, Resolver,
};
use uv_types::{HashStrategy, InFlight};
use uv_warnings::warn_user;

use crate::commands::reporters::{DownloadReporter, InstallReporter, ResolverReporter};
use crate::commands::DryRunEvent;
use crate::commands::{compile_bytecode, elapsed, ChangeEvent, ChangeEventKind};
use crate::editables::ResolvedEditables;
use crate::printer::Printer;

/// Consolidate the requirements for an installation.
pub(crate) async fn read_requirements(
    requirements: &[RequirementsSource],
    constraints: &[RequirementsSource],
    overrides: &[RequirementsSource],
    extras: &ExtrasSpecification,
    client_builder: &BaseClientBuilder<'_>,
    preview: PreviewMode,
) -> Result<RequirementsSpecification, Error> {
    // If the user requests `extras` but does not provide a valid source (e.g., a `pyproject.toml`),
    // return an error.
    if !extras.is_empty() && !requirements.iter().any(RequirementsSource::allows_extras) {
        return Err(anyhow!(
            "Requesting extras requires a `pyproject.toml`, `setup.cfg`, or `setup.py` file."
        )
        .into());
    }

    // Read all requirements from the provided sources.
    let spec = RequirementsSpecification::from_sources(
        requirements,
        constraints,
        overrides,
        extras,
        client_builder,
        preview,
    )
    .await?;

    // If all the metadata could be statically resolved, validate that every extra was used. If we
    // need to resolve metadata via PEP 517, we don't know which extras are used until much later.
    if spec.source_trees.is_empty() {
        if let ExtrasSpecification::Some(extras) = extras {
            let mut unused_extras = extras
                .iter()
                .filter(|extra| !spec.extras.contains(extra))
                .collect::<Vec<_>>();
            if !unused_extras.is_empty() {
                unused_extras.sort_unstable();
                unused_extras.dedup();
                let s = if unused_extras.len() == 1 { "" } else { "s" };
                return Err(anyhow!(
                    "Requested extra{s} not found: {}",
                    unused_extras.iter().join(", ")
                )
                .into());
            }
        }
    }

    Ok(spec)
}

/// Resolve a set of requirements, similar to running `pip compile`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn resolve(
    requirements: Vec<Requirement>,
    constraints: Vec<Requirement>,
    overrides: Vec<Requirement>,
    project: Option<PackageName>,
    editables: &ResolvedEditables,
    hasher: &HashStrategy,
    site_packages: SitePackages,
    reinstall: &Reinstall,
    upgrade: &Upgrade,
    interpreter: &Interpreter,
    tags: &Tags,
    markers: &MarkerEnvironment,
    client: &RegistryClient,
    flat_index: &FlatIndex,
    index: &InMemoryIndex,
    build_dispatch: &BuildDispatch<'_>,
    concurrency: Concurrency,
    options: Options,
    printer: Printer,
) -> Result<ResolutionGraph, Error> {
    let start = std::time::Instant::now();

    // TODO(zanieb): Consider consuming these instead of cloning
    let exclusions = Exclusions::new(reinstall.clone(), upgrade.clone());

    // Prefer current site packages; filter out packages that are marked for reinstall or upgrade
    let preferences = site_packages
        .iter()
        .filter(|dist| !exclusions.contains(dist.name()))
        .map(|dist| {
            let source = match dist.installed_version() {
                InstalledVersion::Version(version) => RequirementSource::Registry {
                    specifier: VersionSpecifiers::from(VersionSpecifier::equals_version(
                        version.clone(),
                    )),
                    // TODO(konstin): track index
                    index: None,
                },
                InstalledVersion::Url(url, _version) => {
                    let parsed_url = ParsedUrl::try_from(url.clone())?;
                    RequirementSource::from_parsed_url(
                        parsed_url,
                        VerbatimUrl::from_url(url.clone()),
                    )
                }
            };
            let requirement = Requirement {
                name: dist.name().clone(),
                extras: vec![],
                marker: None,
                source,
                origin: None,
            };
            Ok(Preference::from_requirement(requirement))
        })
        .collect::<Result<_, _>>()
        .map_err(Error::UnsupportedInstalledDist)?;

    // Collect constraints and overrides.
    let constraints = Constraints::from_requirements(constraints);
    let overrides = Overrides::from_requirements(overrides);
    let python_requirement = PythonRequirement::from_marker_environment(interpreter, markers);

    // Map the editables to their metadata.
    let editables = editables.as_metadata().map_err(Error::ParsedUrl)?;

    // Determine any lookahead requirements.
    let lookaheads = match options.dependency_mode {
        DependencyMode::Transitive => {
            LookaheadResolver::new(
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
            .await?
        }
        DependencyMode::Direct => Vec::new(),
    };

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
        &python_requirement,
        Some(markers),
        tags,
        flat_index,
        index,
        hasher,
        build_dispatch,
        site_packages,
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
    editables: &[ResolvedEditable],
    site_packages: SitePackages,
    reinstall: &Reinstall,
    no_binary: &NoBinary,
    link_mode: LinkMode,
    compile: bool,
    index_urls: &IndexLocations,
    hasher: &HashStrategy,
    tags: &Tags,
    client: &RegistryClient,
    in_flight: &InFlight,
    concurrency: Concurrency,
    build_dispatch: &BuildDispatch<'_>,
    cache: &Cache,
    venv: &PythonEnvironment,
    dry_run: bool,
    printer: Printer,
) -> Result<(), Error> {
    let start = std::time::Instant::now();

    let requirements = resolution.requirements();

    // Partition into those that should be linked from the cache (`local`), those that need to be
    // downloaded (`remote`), and those that should be removed (`extraneous`).
    let plan = Planner::with_requirements(&requirements)
        .with_editable_requirements(editables)
        .build(
            site_packages,
            reinstall,
            no_binary,
            hasher,
            index_urls,
            cache,
            venv,
            tags,
        )
        .context("Failed to determine installation plan")?;

    if dry_run {
        return report_dry_run(resolution, plan, start, printer);
    }

    let Plan {
        cached,
        remote,
        reinstalls,
        extraneous: _,
    } = plan;

    // Nothing to do.
    if remote.is_empty() && cached.is_empty() && reinstalls.is_empty() {
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

    // Remove any existing installations.
    if !reinstalls.is_empty() {
        for dist_info in &reinstalls {
            match uv_installer::uninstall(dist_info).await {
                Ok(summary) => {
                    debug!(
                        "Uninstalled {} ({} file{}, {} director{})",
                        dist_info.name(),
                        summary.file_count,
                        if summary.file_count == 1 { "" } else { "s" },
                        summary.dir_count,
                        if summary.dir_count == 1 { "y" } else { "ies" },
                    );
                }
                Err(uv_installer::UninstallError::Uninstall(
                    install_wheel_rs::Error::MissingRecord(_),
                )) => {
                    warn_user!(
                        "Failed to uninstall package at {} due to missing RECORD file. Installation may result in an incomplete environment.",
                        dist_info.path().user_display().cyan(),
                    );
                }
                Err(err) => return Err(err.into()),
            }
        }
    }

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

    if compile {
        compile_bytecode(venv, cache, printer).await?;
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

/// Report on the results of a dry-run installation.
fn report_dry_run(
    resolution: &Resolution,
    plan: Plan,
    start: std::time::Instant,
    printer: Printer,
) -> Result<(), Error> {
    let Plan {
        cached,
        remote,
        reinstalls,
        extraneous: _,
    } = plan;

    // Nothing to do.
    if remote.is_empty() && cached.is_empty() && reinstalls.is_empty() {
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
        writeln!(printer.stderr(), "Would make no changes")?;
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
        let s = if remote.len() == 1 { "" } else { "s" };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Would download {}",
                format!("{} package{}", remote.len(), s).bold(),
            )
            .dimmed()
        )?;
        remote
    };

    // Remove any existing installations.
    if !reinstalls.is_empty() {
        let s = if reinstalls.len() == 1 { "" } else { "s" };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Would uninstall {}",
                format!("{} package{}", reinstalls.len(), s).bold(),
            )
            .dimmed()
        )?;
    }

    // Install the resolved distributions.
    let installs = wheels.len() + cached.len();

    if installs > 0 {
        let s = if installs == 1 { "" } else { "s" };
        writeln!(
            printer.stderr(),
            "{}",
            format!("Would install {}", format!("{installs} package{s}").bold()).dimmed()
        )?;
    }

    for event in reinstalls
        .into_iter()
        .map(|distribution| DryRunEvent {
            name: distribution.name().clone(),
            version: distribution.installed_version().to_string(),
            kind: ChangeEventKind::Removed,
        })
        .chain(wheels.into_iter().map(|distribution| DryRunEvent {
            name: distribution.name().clone(),
            version: distribution.version_or_url().to_string(),
            kind: ChangeEventKind::Added,
        }))
        .chain(cached.into_iter().map(|distribution| DryRunEvent {
            name: distribution.name().clone(),
            version: distribution.installed_version().to_string(),
            kind: ChangeEventKind::Added,
        }))
        .sorted_unstable_by(|a, b| a.name.cmp(&b.name).then_with(|| a.kind.cmp(&b.kind)))
    {
        match event.kind {
            ChangeEventKind::Added => {
                writeln!(
                    printer.stderr(),
                    " {} {}{}",
                    "+".green(),
                    event.name.as_ref().bold(),
                    event.version.dimmed()
                )?;
            }
            ChangeEventKind::Removed => {
                writeln!(
                    printer.stderr(),
                    " {} {}{}",
                    "-".red(),
                    event.name.as_ref().bold(),
                    event.version.dimmed()
                )?;
            }
        }
    }

    Ok(())
}

/// Validate the installed packages in the virtual environment.
pub(crate) fn validate(
    resolution: &Resolution,
    venv: &PythonEnvironment,
    printer: Printer,
) -> Result<(), Error> {
    let site_packages = SitePackages::from_executable(venv)?;
    let diagnostics = site_packages.diagnostics()?;
    for diagnostic in diagnostics {
        // Only surface diagnostics that are "relevant" to the current resolution.
        if resolution
            .packages()
            .any(|package| diagnostic.includes(package))
        {
            writeln!(
                printer.stderr(),
                "{}{} {}",
                "warning".yellow().bold(),
                ":".bold(),
                diagnostic.message().bold()
            )?;
        }
    }
    Ok(())
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error(transparent)]
    Resolve(#[from] uv_resolver::ResolveError),

    #[error(transparent)]
    Uninstall(#[from] uv_installer::UninstallError),

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
    ParsedUrl(Box<distribution_types::ParsedUrlError>),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),

    #[error("Installed distribution has unsupported type")]
    UnsupportedInstalledDist(#[source] Box<distribution_types::ParsedUrlError>),
}
