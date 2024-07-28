//! Common operations shared across the `pip` API and subcommands.

use std::fmt::{self, Write};
use std::path::PathBuf;

use anyhow::{anyhow, Context};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::debug;

use distribution_types::{
    CachedDist, Diagnostic, InstalledDist, ResolutionDiagnostic, UnresolvedRequirementSpecification,
};
use distribution_types::{
    DistributionMetadata, IndexLocations, InstalledMetadata, LocalDist, Name, Resolution,
};
use install_wheel_rs::linker::LinkMode;
use platform_tags::Tags;
use pypi_types::Requirement;
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, RegistryClient};
use uv_configuration::{
    BuildOptions, Concurrency, Constraints, ExtrasSpecification, Overrides, PreviewMode, Reinstall,
    Upgrade,
};
use uv_dispatch::BuildDispatch;
use uv_distribution::DistributionDatabase;
use uv_fs::Simplified;
use uv_installer::{Plan, Planner, Preparer, SitePackages};
use uv_normalize::{GroupName, PackageName};
use uv_python::PythonEnvironment;
use uv_requirements::{
    LookaheadResolver, NamedRequirementsResolver, RequirementsSource, RequirementsSpecification,
    SourceTreeResolver,
};
use uv_resolver::{
    DependencyMode, Exclusions, FlatIndex, InMemoryIndex, Manifest, Options, Preference,
    Preferences, PythonRequirement, ResolutionGraph, Resolver, ResolverMarkers,
};
use uv_types::{HashStrategy, InFlight, InstalledPackagesProvider};
use uv_warnings::warn_user;

use crate::commands::reporters::{InstallReporter, PrepareReporter, ResolverReporter};
use crate::commands::{compile_bytecode, elapsed, ChangeEvent, ChangeEventKind, DryRunEvent};
use crate::printer::Printer;

/// Consolidate the requirements for an installation.
pub(crate) async fn read_requirements(
    requirements: &[RequirementsSource],
    constraints: &[RequirementsSource],
    overrides: &[RequirementsSource],
    extras: &ExtrasSpecification,
    client_builder: &BaseClientBuilder<'_>,
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
    Ok(RequirementsSpecification::from_sources(
        requirements,
        constraints,
        overrides,
        client_builder,
    )
    .await?)
}

/// Resolve a set of requirements, similar to running `pip compile`.
pub(crate) async fn resolve<InstalledPackages: InstalledPackagesProvider>(
    requirements: Vec<UnresolvedRequirementSpecification>,
    constraints: Vec<Requirement>,
    overrides: Vec<UnresolvedRequirementSpecification>,
    dev: Vec<GroupName>,
    source_trees: Vec<PathBuf>,
    mut project: Option<PackageName>,
    extras: &ExtrasSpecification,
    preferences: Vec<Preference>,
    installed_packages: InstalledPackages,
    hasher: &HashStrategy,
    reinstall: &Reinstall,
    upgrade: &Upgrade,
    tags: Option<&Tags>,
    markers: ResolverMarkers,
    python_requirement: PythonRequirement,
    client: &RegistryClient,
    flat_index: &FlatIndex,
    index: &InMemoryIndex,
    build_dispatch: &BuildDispatch<'_>,
    concurrency: Concurrency,
    options: Options,
    printer: Printer,
    preview: PreviewMode,
    quiet: bool,
) -> Result<ResolutionGraph, Error> {
    let start = std::time::Instant::now();

    // Resolve the requirements from the provided sources.
    let requirements = {
        // Convert from unnamed to named requirements.
        let mut requirements = NamedRequirementsResolver::new(
            requirements,
            hasher,
            index,
            DistributionDatabase::new(client, build_dispatch, concurrency.downloads, preview),
        )
        .with_reporter(ResolverReporter::from(printer))
        .resolve()
        .await?;

        // Resolve any source trees into requirements.
        if !source_trees.is_empty() {
            let resolutions = SourceTreeResolver::new(
                source_trees,
                extras,
                hasher,
                index,
                DistributionDatabase::new(client, build_dispatch, concurrency.downloads, preview),
            )
            .with_reporter(ResolverReporter::from(printer))
            .resolve()
            .await?;

            // If we resolved a single project, use it for the project name.
            project = project.or_else(|| {
                if let [resolution] = &resolutions[..] {
                    Some(resolution.project.clone())
                } else {
                    None
                }
            });

            // If any of the extras were unused, surface a warning.
            if let ExtrasSpecification::Some(extras) = extras {
                let mut unused_extras = extras
                    .iter()
                    .filter(|extra| {
                        !resolutions
                            .iter()
                            .any(|resolution| resolution.extras.contains(extra))
                    })
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

            // Extend the requirements with the resolved source trees.
            requirements.extend(
                resolutions
                    .into_iter()
                    .flat_map(|resolution| resolution.requirements),
            );
        }

        requirements
    };

    // Resolve the overrides from the provided sources.
    let overrides = NamedRequirementsResolver::new(
        overrides,
        hasher,
        index,
        DistributionDatabase::new(client, build_dispatch, concurrency.downloads, preview),
    )
    .with_reporter(ResolverReporter::from(printer))
    .resolve()
    .await?;

    // Collect constraints and overrides.
    let constraints = Constraints::from_requirements(
        constraints
            .into_iter()
            .chain(upgrade.constraints().cloned()),
    );
    let overrides = Overrides::from_requirements(overrides);
    let preferences = Preferences::from_iter(preferences, markers.marker_environment());

    // Determine any lookahead requirements.
    let lookaheads = match options.dependency_mode {
        DependencyMode::Transitive => {
            LookaheadResolver::new(
                &requirements,
                &constraints,
                &overrides,
                &dev,
                hasher,
                index,
                DistributionDatabase::new(client, build_dispatch, concurrency.downloads, preview),
            )
            .with_reporter(ResolverReporter::from(printer))
            .resolve(&markers)
            .await?
        }
        DependencyMode::Direct => Vec::new(),
    };

    // TODO(zanieb): Consider consuming these instead of cloning
    let exclusions = Exclusions::new(reinstall.clone(), upgrade.clone());

    // Create a manifest of the requirements.
    let manifest = Manifest::new(
        requirements,
        constraints,
        overrides,
        dev,
        preferences,
        project,
        exclusions,
        lookaheads,
    );

    // Resolve the dependencies.
    let resolution = {
        // If possible, create a bound on the progress bar.
        let reporter = match options.dependency_mode {
            DependencyMode::Transitive => ResolverReporter::from(printer),
            DependencyMode::Direct => {
                ResolverReporter::from(printer).with_length(manifest.num_requirements() as u64)
            }
        };

        let resolver = Resolver::new(
            manifest,
            options,
            &python_requirement,
            markers,
            tags,
            flat_index,
            index,
            hasher,
            build_dispatch,
            installed_packages,
            DistributionDatabase::new(client, build_dispatch, concurrency.downloads, preview),
        )?
        .with_reporter(reporter);

        resolver.resolve().await?
    };

    if !quiet {
        resolution_success(&resolution, start, printer)?;
    }

    Ok(resolution)
}

// Prints a success message after completing resolution.
pub(crate) fn resolution_success(
    resolution: &ResolutionGraph,
    start: std::time::Instant,
    printer: Printer,
) -> fmt::Result {
    let s = if resolution.len() == 1 { "" } else { "s" };

    writeln!(
        printer.stderr(),
        "{}",
        format!(
            "Resolved {} {}",
            format!("{} package{}", resolution.len(), s).bold(),
            format!("in {}", elapsed(start.elapsed())).dimmed()
        )
        .dimmed()
    )
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Modifications {
    /// Use `pip install` semantics, whereby existing installations are left as-is, unless they are
    /// marked for re-installation or upgrade.
    ///
    /// Ensures that the resulting environment is sufficient to meet the requirements, but without
    /// any unnecessary changes.
    Sufficient,
    /// Use `pip sync` semantics, whereby any existing, extraneous installations are removed.
    ///
    /// Ensures that the resulting environment is an exact match for the requirements, but may
    /// result in more changes than necessary.
    Exact,
}

/// Install a set of requirements into the current environment.
pub(crate) async fn install(
    resolution: &Resolution,
    site_packages: SitePackages,
    modifications: Modifications,
    reinstall: &Reinstall,
    build_options: &BuildOptions,
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
    preview: PreviewMode,
) -> Result<(), Error> {
    let start = std::time::Instant::now();

    // Extract the requirements from the resolution.
    let requirements = resolution.requirements().collect::<Vec<_>>();

    // Partition into those that should be linked from the cache (`local`), those that need to be
    // downloaded (`remote`), and those that should be removed (`extraneous`).
    let plan = Planner::new(&requirements)
        .build(
            site_packages,
            reinstall,
            build_options,
            hasher,
            index_urls,
            cache,
            venv,
            tags,
        )
        .context("Failed to determine installation plan")?;

    if dry_run {
        return report_dry_run(resolution, plan, modifications, start, printer);
    }

    let Plan {
        cached,
        remote,
        reinstalls,
        extraneous,
    } = plan;

    // If we're in `install` mode, ignore any extraneous distributions.
    let extraneous = match modifications {
        Modifications::Sufficient => vec![],
        Modifications::Exact => extraneous,
    };

    // Nothing to do.
    if remote.is_empty() && cached.is_empty() && reinstalls.is_empty() && extraneous.is_empty() {
        let s = if resolution.len() == 1 { "" } else { "s" };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Audited {} {}",
                format!("{} package{}", resolution.len(), s).bold(),
                format!("in {}", elapsed(start.elapsed())).dimmed()
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

        let preparer = Preparer::new(
            cache,
            tags,
            hasher,
            DistributionDatabase::new(client, build_dispatch, concurrency.downloads, preview),
        )
        .with_reporter(PrepareReporter::from(printer).with_length(remote.len() as u64));

        let wheels = preparer
            .prepare(remote.clone(), in_flight)
            .await
            .context("Failed to prepare distributions")?;

        let s = if wheels.len() == 1 { "" } else { "s" };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Prepared {} {}",
                format!("{} package{}", wheels.len(), s).bold(),
                format!("in {}", elapsed(start.elapsed())).dimmed()
            )
            .dimmed()
        )?;

        wheels
    };

    // Remove any upgraded or extraneous installations.
    if !extraneous.is_empty() || !reinstalls.is_empty() {
        let start = std::time::Instant::now();

        for dist_info in extraneous.iter().chain(reinstalls.iter()) {
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

        let s = if extraneous.len() + reinstalls.len() == 1 {
            ""
        } else {
            "s"
        };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Uninstalled {} {}",
                format!("{} package{}", extraneous.len() + reinstalls.len(), s).bold(),
                format!("in {}", elapsed(start.elapsed())).dimmed()
            )
            .dimmed()
        )?;
    }

    // Install the resolved distributions.
    let mut wheels = wheels.into_iter().chain(cached).collect::<Vec<_>>();
    if !wheels.is_empty() {
        let start = std::time::Instant::now();
        wheels = uv_installer::Installer::new(venv)
            .with_link_mode(link_mode)
            .with_cache(cache)
            .with_reporter(InstallReporter::from(printer).with_length(wheels.len() as u64))
            // This technically can block the runtime, but we are on the main thread and
            // have no other running tasks at this point, so this lets us avoid spawning a blocking
            // task.
            .install_blocking(wheels)?;

        let s = if wheels.len() == 1 { "" } else { "s" };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Installed {} {}",
                format!("{} package{}", wheels.len(), s).bold(),
                format!("in {}", elapsed(start.elapsed())).dimmed()
            )
            .dimmed()
        )?;
    }

    if compile {
        compile_bytecode(venv, cache, printer).await?;
    }

    // Notify the user of any environment modifications.
    report_modifications(wheels, reinstalls, extraneous, printer)?;

    Ok(())
}

/// Report on the results of a dry-run installation.
fn report_dry_run(
    resolution: &Resolution,
    plan: Plan,
    modifications: Modifications,
    start: std::time::Instant,
    printer: Printer,
) -> Result<(), Error> {
    let Plan {
        cached,
        remote,
        reinstalls,
        extraneous,
    } = plan;

    // If we're in `install` mode, ignore any extraneous distributions.
    let extraneous = match modifications {
        Modifications::Sufficient => vec![],
        Modifications::Exact => extraneous,
    };

    // Nothing to do.
    if remote.is_empty() && cached.is_empty() && reinstalls.is_empty() && extraneous.is_empty() {
        let s = if resolution.len() == 1 { "" } else { "s" };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Audited {} {}",
                format!("{} package{}", resolution.len(), s).bold(),
                format!("in {}", elapsed(start.elapsed())).dimmed()
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
        remote.clone()
    };

    // Remove any upgraded or extraneous installations.
    if !extraneous.is_empty() || !reinstalls.is_empty() {
        let s = if extraneous.len() + reinstalls.len() == 1 {
            ""
        } else {
            "s"
        };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Would uninstall {}",
                format!("{} package{}", extraneous.len() + reinstalls.len(), s).bold(),
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

    // TDOO(charlie): DRY this up with `report_modifications`. The types don't quite line up.
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
                    event.name.bold(),
                    event.version.dimmed()
                )?;
            }
            ChangeEventKind::Removed => {
                writeln!(
                    printer.stderr(),
                    " {} {}{}",
                    "-".red(),
                    event.name.bold(),
                    event.version.dimmed()
                )?;
            }
        }
    }

    Ok(())
}

/// Report on any modifications to the Python environment.
pub(crate) fn report_modifications(
    installed: Vec<CachedDist>,
    reinstalled: Vec<InstalledDist>,
    uninstalled: Vec<InstalledDist>,
    printer: Printer,
) -> Result<(), Error> {
    for event in uninstalled
        .into_iter()
        .chain(reinstalled)
        .map(|distribution| ChangeEvent {
            dist: LocalDist::from(distribution),
            kind: ChangeEventKind::Removed,
        })
        .chain(installed.into_iter().map(|distribution| ChangeEvent {
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
                    event.dist.name().bold(),
                    event.dist.installed_version().dimmed()
                )?;
            }
            ChangeEventKind::Removed => {
                writeln!(
                    printer.stderr(),
                    " {} {}{}",
                    "-".red(),
                    event.dist.name().bold(),
                    event.dist.installed_version().dimmed()
                )?;
            }
        }
    }
    Ok(())
}

/// Report any diagnostics on resolved distributions.
pub(crate) fn diagnose_resolution(
    diagnostics: &[ResolutionDiagnostic],
    printer: Printer,
) -> Result<(), Error> {
    for diagnostic in diagnostics {
        writeln!(
            printer.stderr(),
            "{}{} {}",
            "warning".yellow().bold(),
            ":".bold(),
            diagnostic.message().bold()
        )?;
    }
    Ok(())
}

/// Report any diagnostics on installed distributions in the Python environment.
pub(crate) fn diagnose_environment(
    resolution: &Resolution,
    venv: &PythonEnvironment,
    printer: Printer,
) -> Result<(), Error> {
    let site_packages = SitePackages::from_environment(venv)?;
    for diagnostic in site_packages.diagnostics()? {
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
    Hash(#[from] uv_types::HashStrategyError),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),

    #[error(transparent)]
    Lookahead(#[from] uv_requirements::LookaheadError),

    #[error(transparent)]
    Named(#[from] uv_requirements::NamedRequirementsError),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),

    #[error(transparent)]
    PubGrubSpecifier(#[from] uv_resolver::PubGrubSpecifierError),
}
