//! Common operations shared across the `pip` API and subcommands.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, anyhow};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::Cache;
use uv_client::{BaseClientBuilder, RegistryClient};
use uv_configuration::{
    BuildOptions, Concurrency, Constraints, DependencyGroups, DryRun, Excludes,
    ExtrasSpecification, Overrides, Reinstall, Upgrade,
};
use uv_dispatch::BuildDispatch;
use uv_distribution::{DistributionDatabase, SourcedDependencyGroups};
use uv_distribution_types::{
    CachedDist, Diagnostic, Dist, InstalledDist, InstalledVersion, LocalDist,
    NameRequirementSpecification, Requirement, ResolutionDiagnostic, UnresolvedRequirement,
    UnresolvedRequirementSpecification, VersionOrUrlRef,
};
use uv_distribution_types::{DistributionMetadata, InstalledMetadata, Name, Resolution};
use uv_fs::Simplified;
use uv_install_wheel::LinkMode;
use uv_installer::{InstallationStrategy, Plan, Planner, Preparer, SitePackages};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pep508::{MarkerEnvironment, RequirementOrigin, VerbatimUrl};
use uv_platform_tags::Tags;
use uv_preview::Preview;
use uv_pypi_types::{Conflicts, ResolverMarkerEnvironment};
use uv_python::{PythonEnvironment, PythonInstallation};
use uv_requirements::{
    GroupsSpecification, LookaheadResolver, NamedRequirementsResolver, RequirementsSource,
    RequirementsSpecification, SourceTree, SourceTreeResolver,
};
use uv_resolver::{
    DependencyMode, Exclusions, FlatIndex, InMemoryIndex, Manifest, Options, Preference,
    Preferences, PythonRequirement, Resolver, ResolverEnvironment, ResolverOutput,
};
use uv_tool::InstalledTools;
use uv_types::{BuildContext, HashStrategy, InFlight, InstalledPackagesProvider};
use uv_warnings::warn_user;

use crate::commands::compile_bytecode;
use crate::commands::pip::loggers::{InstallLogger, ResolveLogger};
use crate::commands::reporters::{InstallReporter, PrepareReporter, ResolverReporter};
use crate::printer::Printer;

/// Consolidate the requirements for an installation.
pub(crate) async fn read_requirements(
    requirements: &[RequirementsSource],
    constraints: &[RequirementsSource],
    overrides: &[RequirementsSource],
    excludes: &[RequirementsSource],
    extras: &ExtrasSpecification,
    groups: Option<&GroupsSpecification>,
    client_builder: &BaseClientBuilder<'_>,
) -> Result<RequirementsSpecification, Error> {
    // If the user requests `extras` but does not provide a valid source (e.g., a `pyproject.toml`),
    // return an error.
    if !extras.is_empty() && !requirements.iter().any(RequirementsSource::allows_extras) {
        let hint = if requirements
            .iter()
            .any(|source| matches!(source, RequirementsSource::Editable(_)))
        {
            "Use `<dir>[extra]` syntax or `-r <file>` instead."
        } else {
            "Use `package[extra]` syntax instead."
        };
        return Err(anyhow!(
            "Requesting extras requires a `pylock.toml`, `pyproject.toml`, `setup.cfg`, or `setup.py` file. {hint}"
        )
        .into());
    }

    // Read all requirements from the provided sources.
    Ok(RequirementsSpecification::from_sources(
        requirements,
        constraints,
        overrides,
        excludes,
        groups,
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
        RequirementsSpecification::from_sources(&[], constraints, &[], &[], None, client_builder)
            .await?
            .constraints,
    )
}

/// Resolve a set of requirements, similar to running `pip compile`.
pub(crate) async fn resolve<InstalledPackages: InstalledPackagesProvider>(
    requirements: Vec<UnresolvedRequirementSpecification>,
    constraints: Vec<NameRequirementSpecification>,
    overrides: Vec<UnresolvedRequirementSpecification>,
    excludes: Vec<PackageName>,
    source_trees: Vec<SourceTree>,
    mut project: Option<PackageName>,
    workspace_members: BTreeSet<PackageName>,
    extras: &ExtrasSpecification,
    groups: &BTreeMap<PathBuf, DependencyGroups>,
    preferences: Vec<Preference>,
    installed_packages: InstalledPackages,
    hasher: &HashStrategy,
    reinstall: &Reinstall,
    upgrade: &Upgrade,
    tags: Option<&Tags>,
    resolver_env: ResolverEnvironment,
    python_requirement: PythonRequirement,
    current_environment: &MarkerEnvironment,
    conflicts: Conflicts,
    client: &RegistryClient,
    flat_index: &FlatIndex,
    index: &InMemoryIndex,
    build_dispatch: &BuildDispatch<'_>,
    concurrency: Concurrency,
    options: Options,
    logger: Box<dyn ResolveLogger>,
    printer: Printer,
) -> Result<ResolverOutput, Error> {
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
                    hasher,
                    index,
                    DistributionDatabase::new(client, build_dispatch, concurrency.downloads),
                )
                .with_reporter(Arc::new(ResolverReporter::from(printer)))
                .resolve(unnamed.into_iter())
                .await?,
            );
        }

        // Resolve any source trees into requirements.
        if !source_trees.is_empty() {
            let resolutions = SourceTreeResolver::new(
                extras,
                hasher,
                index,
                DistributionDatabase::new(client, build_dispatch, concurrency.downloads),
            )
            .with_reporter(Arc::new(ResolverReporter::from(printer)))
            .resolve(source_trees.iter())
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
            let mut unused_extras = extras
                .explicit_names()
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

            // Extend the requirements with the resolved source trees.
            requirements.extend(
                resolutions
                    .into_iter()
                    .flat_map(|resolution| resolution.requirements),
            );
        }

        for (pyproject_path, groups) in groups {
            let metadata = SourcedDependencyGroups::from_virtual_project(
                pyproject_path,
                None,
                build_dispatch.locations(),
                build_dispatch.sources(),
                build_dispatch.workspace_cache(),
                client.credentials_cache(),
            )
            .await
            .with_context(|| {
                format!(
                    "Failed to read dependency groups from: {}",
                    pyproject_path.display()
                )
            })?;

            // Complain if dependency groups are named that don't appear.
            for name in groups.explicit_names() {
                if !metadata.dependency_groups.contains_key(name) {
                    return Err(anyhow!(
                        "The dependency group '{name}' was not found in the project: {}",
                        pyproject_path.user_display()
                    ))?;
                }
            }
            // Apply dependency-groups
            for (group_name, group) in &metadata.dependency_groups {
                if groups.contains(group_name) {
                    requirements.extend(group.iter().cloned().map(|group| Requirement {
                        origin: Some(RequirementOrigin::Group(
                            pyproject_path.clone(),
                            metadata.name.clone(),
                            group_name.clone(),
                        )),
                        ..group
                    }));
                }
            }
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
                    hasher,
                    index,
                    DistributionDatabase::new(client, build_dispatch, concurrency.downloads),
                )
                .with_reporter(Arc::new(ResolverReporter::from(printer)))
                .resolve(unnamed.into_iter())
                .await?,
            );
        }

        overrides
    };

    // Collect constraints, overrides, and excludes.
    let constraints = Constraints::from_requirements(
        constraints
            .into_iter()
            .map(|constraint| constraint.requirement)
            .chain(upgrade.constraints().cloned()),
    );
    let overrides = Overrides::from_requirements(overrides);
    let excludes = excludes.into_iter().collect::<Excludes>();
    let preferences = Preferences::from_iter(preferences, &resolver_env);

    // Determine any lookahead requirements.
    let lookaheads = match options.dependency_mode {
        DependencyMode::Transitive => {
            LookaheadResolver::new(
                &requirements,
                &constraints,
                &overrides,
                hasher,
                index,
                DistributionDatabase::new(client, build_dispatch, concurrency.downloads),
            )
            .with_reporter(Arc::new(ResolverReporter::from(printer)))
            .resolve(&resolver_env)
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
        excludes,
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
            resolver_env,
            current_environment,
            conflicts,
            tags,
            flat_index,
            index,
            hasher,
            build_dispatch,
            installed_packages,
            DistributionDatabase::new(client, build_dispatch, concurrency.downloads),
        )?
        .with_reporter(Arc::new(reporter));

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

/// A distribution which was or would be modified
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum ChangedDist {
    Local(LocalDist),
    Remote(Arc<Dist>),
}

impl Name for ChangedDist {
    fn name(&self) -> &PackageName {
        match self {
            Self::Local(dist) => dist.name(),
            Self::Remote(dist) => dist.name(),
        }
    }
}

/// The [`Version`] or [`VerbatimUrl`] for a changed dist.
#[derive(Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub(crate) enum ShortSpecifier<'a> {
    Version(&'a Version),
    Url(&'a VerbatimUrl),
}

impl std::fmt::Display for ShortSpecifier<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Version(version) => version.fmt(f),
            Self::Url(url) => write!(f, " @ {url}"),
        }
    }
}

/// The [`InstalledVersion`] or [`VerbatimUrl`] for a changed dist.
#[derive(Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub(crate) enum LongSpecifier<'a> {
    InstalledVersion(InstalledVersion<'a>),
    Url(&'a VerbatimUrl),
}

impl std::fmt::Display for LongSpecifier<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InstalledVersion(version) => version.fmt(f),
            Self::Url(url) => write!(f, " @ {url}"),
        }
    }
}

impl ChangedDist {
    pub(crate) fn short_specifier(&self) -> ShortSpecifier<'_> {
        match self {
            Self::Local(dist) => ShortSpecifier::Version(dist.installed_version().version()),
            Self::Remote(dist) => match dist.version_or_url() {
                VersionOrUrlRef::Version(version) => ShortSpecifier::Version(version),
                VersionOrUrlRef::Url(url) => ShortSpecifier::Url(url),
            },
        }
    }

    pub(crate) fn long_specifier(&self) -> LongSpecifier<'_> {
        match self {
            Self::Local(dist) => LongSpecifier::InstalledVersion(dist.installed_version()),
            Self::Remote(dist) => match dist.version_or_url() {
                VersionOrUrlRef::Version(version) => {
                    LongSpecifier::InstalledVersion(InstalledVersion::Version(version))
                }
                VersionOrUrlRef::Url(url) => LongSpecifier::Url(url),
            },
        }
    }

    pub(crate) fn version(&self) -> Option<&Version> {
        match self {
            Self::Local(dist) => Some(dist.installed_version().version()),
            Self::Remote(dist) => dist.version(),
        }
    }
}

/// A summary of the changes made to the environment during an installation.
#[derive(Debug, Clone, Default)]
pub(crate) struct Changelog {
    /// The distributions that were installed.
    pub(crate) installed: HashSet<ChangedDist>,
    /// The distributions that were uninstalled.
    pub(crate) uninstalled: HashSet<ChangedDist>,
    /// The distributions that were reinstalled.
    pub(crate) reinstalled: HashSet<ChangedDist>,
}

impl Changelog {
    /// Create a [`Changelog`] from two iterators of [`ChangedDist`]s.
    pub(crate) fn new<I, U>(installed: I, uninstalled: U) -> Self
    where
        I: IntoIterator<Item = ChangedDist>,
        U: IntoIterator<Item = ChangedDist>,
    {
        // SAFETY: This is allowed because `LocalDist` implements `Hash` and `Eq` based solely on
        // the inner `kind`, and omits the types that rely on internal mutability.
        #[allow(clippy::mutable_key_type)]
        let mut uninstalled: HashSet<_> = uninstalled.into_iter().collect();
        let (reinstalled, installed): (HashSet<_>, HashSet<_>) = installed
            .into_iter()
            .partition(|dist| uninstalled.contains(dist));
        uninstalled.retain(|dist| !reinstalled.contains(dist));

        Self {
            installed,
            uninstalled,
            reinstalled,
        }
    }

    /// Create a [`Changelog`] from a list of local distributions.
    pub(crate) fn from_local(installed: Vec<CachedDist>, uninstalled: Vec<InstalledDist>) -> Self {
        Self::new(
            installed
                .into_iter()
                .map(|dist| ChangedDist::Local(dist.into())),
            uninstalled
                .into_iter()
                .map(|dist| ChangedDist::Local(dist.into())),
        )
    }

    /// Create a [`Changelog`] from a list of installed distributions.
    pub(crate) fn from_installed(installed: Vec<CachedDist>) -> Self {
        Self::from_local(installed, Vec::new())
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
    installation: InstallationStrategy,
    modifications: Modifications,
    reinstall: &Reinstall,
    build_options: &BuildOptions,
    link_mode: LinkMode,
    compile: bool,
    hasher: &HashStrategy,
    tags: &Tags,
    client: &RegistryClient,
    in_flight: &InFlight,
    concurrency: Concurrency,
    build_dispatch: &BuildDispatch<'_>,
    cache: &Cache,
    venv: &PythonEnvironment,
    logger: Box<dyn InstallLogger>,
    installer_metadata: bool,
    dry_run: DryRun,
    printer: Printer,
    preview: Preview,
) -> Result<Changelog, Error> {
    let start = std::time::Instant::now();

    // Partition into those that should be linked from the cache (`local`), those that need to be
    // downloaded (`remote`), and those that should be removed (`extraneous`).
    let plan = Planner::new(resolution)
        .build(
            site_packages,
            installation,
            reinstall,
            build_options,
            hasher,
            build_dispatch.locations(),
            build_dispatch.config_settings(),
            build_dispatch.config_settings_package(),
            build_dispatch.extra_build_requires(),
            build_dispatch.extra_build_variables(),
            cache,
            venv,
            tags,
        )
        .context("Failed to determine installation plan")?;

    if dry_run.enabled() {
        return report_dry_run(
            dry_run,
            resolution,
            plan,
            modifications,
            start,
            logger.as_ref(),
            printer,
        );
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
    if remote.is_empty()
        && cached.is_empty()
        && reinstalls.is_empty()
        && extraneous.is_empty()
        && !compile
    {
        logger.on_audit(resolution.len(), start, printer, dry_run)?;
        return Ok(Changelog::default());
    }

    // Partition into two sets: those that require build isolation, and those that disable it. This
    // is effectively a heuristic to make `--no-build-isolation` work "more often" by way of giving
    // `--no-build-isolation` packages "access" to the rest of the environment.
    let (isolated_phase, shared_phase) = Plan {
        cached,
        remote,
        reinstalls,
        extraneous,
    }
    .partition(|name| build_dispatch.build_isolation().is_isolated(Some(name)));

    let has_isolated_phase = !isolated_phase.is_empty();
    let has_shared_phase = !shared_phase.is_empty();

    let mut installs = vec![];
    let mut uninstalls = vec![];

    // Execute the isolated-build phase.
    if has_isolated_phase {
        let (isolated_installs, isolated_uninstalls) = execute_plan(
            isolated_phase,
            None,
            resolution,
            build_options,
            link_mode,
            hasher,
            tags,
            client,
            in_flight,
            concurrency,
            build_dispatch,
            cache,
            venv,
            logger.as_ref(),
            installer_metadata,
            printer,
            preview,
        )
        .await?;
        installs.extend(isolated_installs);
        uninstalls.extend(isolated_uninstalls);
    }

    if has_shared_phase {
        let (shared_installs, shared_uninstalls) = execute_plan(
            shared_phase,
            if has_isolated_phase {
                Some(InstallPhase::Shared)
            } else {
                None
            },
            resolution,
            build_options,
            link_mode,
            hasher,
            tags,
            client,
            in_flight,
            concurrency,
            build_dispatch,
            cache,
            venv,
            logger.as_ref(),
            installer_metadata,
            printer,
            preview,
        )
        .await?;
        installs.extend(shared_installs);
        uninstalls.extend(shared_uninstalls);
    }

    if compile {
        compile_bytecode(venv, &concurrency, cache, printer).await?;
    }

    // Construct a summary of the changes made to the environment.
    let changelog = Changelog::from_local(installs, uninstalls);

    // Notify the user of any environment modifications.
    logger.on_complete(&changelog, printer, dry_run)?;

    Ok(changelog)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallPhase {
    /// A dedicated phase for building and installing packages with build-isolation disabled.
    Shared,
}

impl InstallPhase {
    fn label(self) -> &'static str {
        match self {
            Self::Shared => "without build isolation",
        }
    }
}

/// Execute a [`Plan`] to install distributions into a Python environment.
async fn execute_plan(
    plan: Plan,
    phase: Option<InstallPhase>,
    resolution: &Resolution,
    build_options: &BuildOptions,
    link_mode: LinkMode,
    hasher: &HashStrategy,
    tags: &Tags,
    client: &RegistryClient,
    in_flight: &InFlight,
    concurrency: Concurrency,
    build_dispatch: &BuildDispatch<'_>,
    cache: &Cache,
    venv: &PythonEnvironment,
    logger: &dyn InstallLogger,
    installer_metadata: bool,
    printer: Printer,
    preview: Preview,
) -> Result<(Vec<CachedDist>, Vec<InstalledDist>), Error> {
    let Plan {
        cached,
        remote,
        reinstalls,
        extraneous,
    } = plan;

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
        .with_reporter(Arc::new(
            PrepareReporter::from(printer).with_length(remote.len() as u64),
        ));

        let wheels = preparer
            .prepare(remote.clone(), in_flight, resolution)
            .await?;

        logger.on_prepare(
            wheels.len(),
            phase.map(InstallPhase::label),
            start,
            printer,
            DryRun::Disabled,
        )?;

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
                    uv_install_wheel::Error::MissingRecord(_),
                )) => {
                    warn_user!(
                        "Failed to uninstall package at {} due to missing `RECORD` file. Installation may result in an incomplete environment.",
                        dist_info.install_path().user_display().cyan(),
                    );
                }
                Err(uv_installer::UninstallError::Uninstall(
                    uv_install_wheel::Error::MissingTopLevel(_),
                )) => {
                    warn_user!(
                        "Failed to uninstall package at {} due to missing `top_level.txt` file. Installation may result in an incomplete environment.",
                        dist_info.install_path().user_display().cyan(),
                    );
                }
                Err(err) => return Err(err.into()),
            }
        }

        logger.on_uninstall(uninstalls.len(), start, printer, DryRun::Disabled)?;
    }

    // Install the resolved distributions.
    let mut installs = wheels.into_iter().chain(cached).collect::<Vec<_>>();
    if !installs.is_empty() {
        let start = std::time::Instant::now();
        installs = uv_installer::Installer::new(venv, preview)
            .with_link_mode(link_mode)
            .with_cache(cache)
            .with_installer_metadata(installer_metadata)
            .with_reporter(Arc::new(
                InstallReporter::from(printer).with_length(installs.len() as u64),
            ))
            // This technically can block the runtime, but we are on the main thread and
            // have no other running tasks at this point, so this lets us avoid spawning a blocking
            // task.
            .install_blocking(installs)?;

        logger.on_install(installs.len(), start, printer, DryRun::Disabled)?;
    }

    Ok((installs, uninstalls))
}

/// Display a message about the interpreter that was selected for the operation.
#[allow(clippy::result_large_err)]
pub(crate) fn report_interpreter(
    python: &PythonInstallation,
    dimmed: bool,
    printer: Printer,
) -> Result<(), Error> {
    let managed = python.source().is_managed();
    let implementation = python.implementation();
    let interpreter = python.interpreter();

    if dimmed {
        if managed {
            writeln!(
                printer.stderr(),
                "{}",
                format!(
                    "Using {} {}{}",
                    implementation.pretty(),
                    interpreter.python_version(),
                    interpreter.variant().display_suffix(),
                )
                .dimmed()
            )?;
        } else {
            writeln!(
                printer.stderr(),
                "{}",
                format!(
                    "Using {} {}{} interpreter at: {}",
                    implementation.pretty(),
                    interpreter.python_version(),
                    interpreter.variant().display_suffix(),
                    interpreter.sys_executable().user_display()
                )
                .dimmed()
            )?;
        }
    } else {
        if managed {
            writeln!(
                printer.stderr(),
                "Using {} {}{}",
                implementation.pretty(),
                interpreter.python_version().cyan(),
                interpreter.variant().display_suffix().cyan()
            )?;
        } else {
            writeln!(
                printer.stderr(),
                "Using {} {}{} interpreter at: {}",
                implementation.pretty(),
                interpreter.python_version(),
                interpreter.variant().display_suffix(),
                interpreter.sys_executable().user_display().cyan()
            )?;
        }
    }

    Ok(())
}

/// Display a message about the target environment for the operation.
#[allow(clippy::result_large_err)]
pub(crate) fn report_target_environment(
    env: &PythonEnvironment,
    cache: &Cache,
    printer: Printer,
) -> Result<(), Error> {
    let message = format!(
        "Using Python {} environment at: {}",
        env.interpreter().python_version(),
        env.root().user_display()
    );

    let Ok(target) = std::path::absolute(env.root()) else {
        debug!("{}", message);
        return Ok(());
    };

    // Do not report environments in the cache
    if target.starts_with(cache.root()) {
        debug!("{}", message);
        return Ok(());
    }

    // Do not report tool environments
    if let Ok(tools) = InstalledTools::from_settings() {
        if target.starts_with(tools.root()) {
            debug!("{}", message);
            return Ok(());
        }
    }

    // Do not report a default environment path
    if let Ok(default) = std::path::absolute(PathBuf::from(".venv")) {
        if target == default {
            debug!("{}", message);
            return Ok(());
        }
    }

    Ok(writeln!(printer.stderr(), "{}", message.dimmed())?)
}

/// Report on the results of a dry-run installation.
#[allow(clippy::result_large_err)]
fn report_dry_run(
    dry_run: DryRun,
    resolution: &Resolution,
    plan: Plan,
    modifications: Modifications,
    start: std::time::Instant,
    logger: &dyn InstallLogger,
    printer: Printer,
) -> Result<Changelog, Error> {
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
        logger.on_audit(resolution.len(), start, printer, dry_run)?;
        return Ok(Changelog::default());
    }

    // Download, build, and unzip any missing distributions.
    let wheels = if remote.is_empty() {
        vec![]
    } else {
        logger.on_prepare(remote.len(), None, start, printer, dry_run)?;
        remote.clone()
    };

    // Remove any upgraded or extraneous installations.
    let uninstalls = extraneous.len() + reinstalls.len();

    if uninstalls > 0 {
        logger.on_uninstall(uninstalls, start, printer, dry_run)?;
    }

    // Install the resolved distributions.
    let installs = wheels.len() + cached.len();

    if installs > 0 {
        logger.on_install(installs, start, printer, dry_run)?;
    }

    let uninstalled = reinstalls
        .into_iter()
        .chain(extraneous)
        .map(|dist| ChangedDist::Local(dist.into()));
    let installed = wheels.into_iter().map(ChangedDist::Remote).chain(
        cached
            .into_iter()
            .map(|dist| ChangedDist::Local(dist.into())),
    );

    let changelog = Changelog::new(installed, uninstalled);

    logger.on_complete(&changelog, printer, dry_run)?;

    if matches!(dry_run, DryRun::Check) {
        return Err(Error::OutdatedEnvironment);
    }

    Ok(changelog)
}

/// Report any diagnostics on resolved distributions.
#[allow(clippy::result_large_err)]
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
#[allow(clippy::result_large_err)]
pub(crate) fn diagnose_environment(
    resolution: &Resolution,
    venv: &PythonEnvironment,
    markers: &ResolverMarkerEnvironment,
    tags: &Tags,
    printer: Printer,
) -> Result<(), Error> {
    let site_packages = SitePackages::from_environment(venv)?;
    for diagnostic in site_packages.diagnostics(markers, tags)? {
        // Only surface diagnostics that are "relevant" to the current resolution.
        if resolution
            .distributions()
            .any(|dist| diagnostic.includes(dist.name()))
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
    #[error("Failed to prepare distributions")]
    Prepare(#[from] uv_installer::PrepareError),

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
    Requirements(#[from] uv_requirements::Error),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),

    #[error("The environment is outdated; run `{}` to update the environment", "uv sync".cyan())]
    OutdatedEnvironment,
}
