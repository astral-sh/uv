//! Common operations shared across the `pip` API and subcommands.

use anyhow::{anyhow, Context};
use itertools::Itertools;
use owo_colors::OwoColorize;
use std::collections::{BTreeSet, HashSet};
use std::fmt::Write;
use std::path::PathBuf;
use tracing::debug;

use distribution_types::{
    CachedDist, Diagnostic, InstalledDist, LocalDist, NameRequirementSpecification,
    ResolutionDiagnostic, UnresolvedRequirement, UnresolvedRequirementSpecification,
};
use distribution_types::{
    DistributionMetadata, IndexLocations, InstalledMetadata, Name, Resolution,
};
use install_wheel_rs::linker::LinkMode;
use platform_tags::Tags;
use pypi_types::ResolverMarkerEnvironment;
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, RegistryClient};
use uv_configuration::{
    BuildOptions, Concurrency, ConfigSettings, Constraints, ExtrasSpecification, Overrides,
    Reinstall, Upgrade,
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

use crate::commands::pip::loggers::{DefaultInstallLogger, InstallLogger, ResolveLogger};
use crate::commands::reporters::{InstallReporter, PrepareReporter, ResolverReporter};
use crate::commands::{compile_bytecode, ChangeEventKind, DryRunEvent};
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

/// Resolve a set of constraints.
pub(crate) async fn read_constraints(
    constraints: &[RequirementsSource],
    client_builder: &BaseClientBuilder<'_>,
) -> Result<Vec<NameRequirementSpecification>, Error> {
    Ok(
        RequirementsSpecification::from_sources(&[], constraints, &[], client_builder)
            .await?
            .constraints,
    )
}

/// Resolve a set of requirements, similar to running `pip compile`.
pub(crate) async fn resolve<InstalledPackages: InstalledPackagesProvider>(
    requirements: Vec<UnresolvedRequirementSpecification>,
    constraints: Vec<NameRequirementSpecification>,
    overrides: Vec<UnresolvedRequirementSpecification>,
    dev: Vec<GroupName>,
    source_trees: Vec<PathBuf>,
    mut project: Option<PackageName>,
    workspace_members: Option<BTreeSet<PackageName>>,
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
    logger: Box<dyn ResolveLogger>,
    printer: Printer,
) -> Result<ResolutionGraph, Error> {
    let start = std::time::Instant::now();

    // Resolve the requirements from the provided sources.
    let requirements = {
        // Partition the requirements into named and unnamed requirements.
        let (mut requirements, unnamed): (Vec<_>, Vec<_>) =
            requirements
                .into_iter()
                .partition_map(|spec| match spec.requirement {
                    UnresolvedRequirement::Named(requirement) => {
                        itertools::Either::Left(requirement)
                    }
                    UnresolvedRequirement::Unnamed(requirement) => {
                        itertools::Either::Right(requirement)
                    }
                });

        // Resolve any unnamed requirements.
        if !unnamed.is_empty() {
            requirements.extend(
                NamedRequirementsResolver::new(
                    unnamed,
                    hasher,
                    index,
                    DistributionDatabase::new(client, build_dispatch, concurrency.downloads),
                )
                .with_reporter(ResolverReporter::from(printer))
                .resolve()
                .await?,
            );
        }

        // Resolve any source trees into requirements.
        if !source_trees.is_empty() {
            let resolutions = SourceTreeResolver::new(
                source_trees,
                extras,
                hasher,
                index,
                DistributionDatabase::new(client, build_dispatch, concurrency.downloads),
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
    let overrides = {
        // Partition the overrides into named and unnamed requirements.
        let (mut overrides, unnamed): (Vec<_>, Vec<_>) =
            overrides
                .into_iter()
                .partition_map(|spec| match spec.requirement {
                    UnresolvedRequirement::Named(requirement) => {
                        itertools::Either::Left(requirement)
                    }
                    UnresolvedRequirement::Unnamed(requirement) => {
                        itertools::Either::Right(requirement)
                    }
                });

        // Resolve any unnamed overrides.
        if !unnamed.is_empty() {
            overrides.extend(
                NamedRequirementsResolver::new(
                    unnamed,
                    hasher,
                    index,
                    DistributionDatabase::new(client, build_dispatch, concurrency.downloads),
                )
                .with_reporter(ResolverReporter::from(printer))
                .resolve()
                .await?,
            );
        }

        overrides
    };

    // Collect constraints and overrides.
    let constraints = Constraints::from_requirements(
        constraints
            .into_iter()
            .map(|constraint| constraint.requirement)
            .chain(upgrade.constraints().cloned()),
    );
    let overrides = Overrides::from_requirements(overrides);
    let preferences = Preferences::from_iter(preferences, &markers);

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
                DistributionDatabase::new(client, build_dispatch, concurrency.downloads),
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
        workspace_members,
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
            DistributionDatabase::new(client, build_dispatch, concurrency.downloads),
        )?
        .with_reporter(reporter);

        resolver.resolve().await?
    };

    logger.on_complete(resolution.len(), start, printer)?;

    Ok(resolution)
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

/// A summary of the changes made to the environment during an installation.
#[derive(Debug, Clone, Default)]
pub(crate) struct Changelog {
    /// The distributions that were installed.
    pub(crate) installed: HashSet<LocalDist>,
    /// The distributions that were uninstalled.
    pub(crate) uninstalled: HashSet<LocalDist>,
    /// The distributions that were reinstalled.
    pub(crate) reinstalled: HashSet<LocalDist>,
}

impl Changelog {
    /// Create a [`Changelog`] from a list of installed and uninstalled distributions.
    pub(crate) fn new(installed: Vec<CachedDist>, uninstalled: Vec<InstalledDist>) -> Self {
        let mut uninstalled: HashSet<_> = uninstalled.into_iter().map(LocalDist::from).collect();

        let (reinstalled, installed): (HashSet<_>, HashSet<_>) = installed
            .into_iter()
            .map(LocalDist::from)
            .partition(|dist| uninstalled.contains(dist));

        uninstalled.retain(|dist| !reinstalled.contains(dist));

        Self {
            installed,
            uninstalled,
            reinstalled,
        }
    }

    /// Create a [`Changelog`] from a list of installed distributions.
    pub(crate) fn from_installed(installed: Vec<CachedDist>) -> Self {
        Self {
            installed: installed.into_iter().map(LocalDist::from).collect(),
            uninstalled: HashSet::default(),
            reinstalled: HashSet::default(),
        }
    }

    /// Returns `true` if the changelog includes a distribution with the given name, either via
    /// an installation or uninstallation.
    pub(crate) fn includes(&self, name: &PackageName) -> bool {
        self.installed.iter().any(|dist| dist.name() == name)
            || self.uninstalled.iter().any(|dist| dist.name() == name)
    }

    /// Returns `true` if the changelog is empty.
    pub(crate) fn is_empty(&self) -> bool {
        self.installed.is_empty() && self.uninstalled.is_empty()
    }
}

/// Install a set of requirements into the current environment.
///
/// Returns a [`Changelog`] summarizing the changes made to the environment.
pub(crate) async fn install(
    resolution: &Resolution,
    site_packages: SitePackages,
    modifications: Modifications,
    reinstall: &Reinstall,
    build_options: &BuildOptions,
    link_mode: LinkMode,
    compile: bool,
    index_urls: &IndexLocations,
    config_settings: &ConfigSettings,
    hasher: &HashStrategy,
    markers: &ResolverMarkerEnvironment,
    tags: &Tags,
    client: &RegistryClient,
    in_flight: &InFlight,
    concurrency: Concurrency,
    build_dispatch: &BuildDispatch<'_>,
    cache: &Cache,
    venv: &PythonEnvironment,
    logger: Box<dyn InstallLogger>,
    dry_run: bool,
    printer: Printer,
) -> Result<Changelog, Error> {
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
            config_settings,
            cache,
            venv,
            markers,
            tags,
        )
        .context("Failed to determine installation plan")?;

    if dry_run {
        report_dry_run(resolution, plan, modifications, start, printer)?;
        return Ok(Changelog::default());
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
        logger.on_audit(resolution.len(), start, printer)?;
        return Ok(Changelog::default());
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
            build_options,
            DistributionDatabase::new(client, build_dispatch, concurrency.downloads),
        )
        .with_reporter(PrepareReporter::from(printer).with_length(remote.len() as u64));

        let wheels = preparer
            .prepare(remote.clone(), in_flight)
            .await
            .context("Failed to prepare distributions")?;

        logger.on_prepare(wheels.len(), start, printer)?;

        wheels
    };

    // Remove any upgraded or extraneous installations.
    let uninstalls = extraneous.into_iter().chain(reinstalls).collect::<Vec<_>>();
    if !uninstalls.is_empty() {
        let start = std::time::Instant::now();

        for dist_info in &uninstalls {
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
                        "Failed to uninstall package at {} due to missing `RECORD` file. Installation may result in an incomplete environment.",
                        dist_info.path().user_display().cyan(),
                    );
                }
                Err(uv_installer::UninstallError::Uninstall(
                    install_wheel_rs::Error::MissingTopLevel(_),
                )) => {
                    warn_user!(
                        "Failed to uninstall package at {} due to missing `top-level.txt` file. Installation may result in an incomplete environment.",
                        dist_info.path().user_display().cyan(),
                    );
                }
                Err(err) => return Err(err.into()),
            }
        }

        logger.on_uninstall(uninstalls.len(), start, printer)?;
    }

    // Install the resolved distributions.
    let mut installs = wheels.into_iter().chain(cached).collect::<Vec<_>>();
    if !installs.is_empty() {
        let start = std::time::Instant::now();
        installs = uv_installer::Installer::new(venv)
            .with_link_mode(link_mode)
            .with_cache(cache)
            .with_reporter(InstallReporter::from(printer).with_length(installs.len() as u64))
            // This technically can block the runtime, but we are on the main thread and
            // have no other running tasks at this point, so this lets us avoid spawning a blocking
            // task.
            .install_blocking(installs)?;

        logger.on_install(installs.len(), start, printer)?;
    }

    if compile {
        compile_bytecode(venv, cache, printer).await?;
    }

    // Construct a summary of the changes made to the environment.
    let changelog = Changelog::new(installs, uninstalls);

    // Notify the user of any environment modifications.
    logger.on_complete(&changelog, printer)?;

    Ok(changelog)
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
        DefaultInstallLogger.on_audit(resolution.len(), start, printer)?;
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
    let uninstalls = extraneous.len() + reinstalls.len();

    if uninstalls > 0 {
        let s = if uninstalls == 1 { "" } else { "s" };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Would uninstall {}",
                format!("{uninstalls} package{s}").bold(),
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

    // TODO(charlie): DRY this up with `report_modifications`. The types don't quite line up.
    for event in reinstalls
        .into_iter()
        .chain(extraneous.into_iter())
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
            ChangeEventKind::Reinstalled => {
                writeln!(
                    printer.stderr(),
                    " {} {}{}",
                    "~".yellow(),
                    event.name.bold(),
                    event.version.dimmed()
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
    markers: &ResolverMarkerEnvironment,
    printer: Printer,
) -> Result<(), Error> {
    let site_packages = SitePackages::from_environment(venv)?;
    for diagnostic in site_packages.diagnostics(markers)? {
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
