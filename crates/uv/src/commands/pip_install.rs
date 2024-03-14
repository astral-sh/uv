use std::collections::HashSet;
use std::fmt::Write;
use std::path::Path;
use std::time::Instant;

use anstream::eprint;
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tempfile::tempdir_in;
use tracing::debug;

use distribution_types::{
    DistributionMetadata, IndexLocations, InstalledMetadata, LocalDist, LocalEditable, Name,
    Resolution,
};
use install_wheel_rs::linker::LinkMode;
use pep508_rs::{MarkerEnvironment, Requirement};
use platform_tags::Tags;
use pypi_types::Yanked;
use requirements_txt::EditableRequirement;
use uv_auth::{KeyringProvider, GLOBAL_AUTH_STORE};
use uv_cache::Cache;
use uv_client::{Connectivity, FlatIndex, FlatIndexClient, RegistryClient, RegistryClientBuilder};
use uv_dispatch::BuildDispatch;
use uv_fs::Simplified;
use uv_installer::{
    BuiltEditable, Downloader, NoBinary, Plan, Planner, Reinstall, ResolvedEditable, SitePackages,
};
use uv_interpreter::{Interpreter, PythonEnvironment};
use uv_normalize::PackageName;
use uv_resolver::{
    DependencyMode, InMemoryIndex, Manifest, Options, OptionsBuilder, PreReleaseMode,
    ResolutionGraph, ResolutionMode, Resolver,
};
use uv_traits::{BuildIsolation, ConfigSettings, InFlight, NoBuild, SetupPyStrategy};

use crate::commands::reporters::{DownloadReporter, InstallReporter, ResolverReporter};
use crate::commands::{compile_bytecode, elapsed, ChangeEvent, ChangeEventKind, ExitStatus};
use crate::printer::Printer;
use crate::requirements::{ExtrasSpecification, RequirementsSource, RequirementsSpecification};

use super::{DryRunEvent, Upgrade};

/// Install packages into the current environment.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub(crate) async fn pip_install(
    requirements: &[RequirementsSource],
    constraints: &[RequirementsSource],
    overrides: &[RequirementsSource],
    extras: &ExtrasSpecification<'_>,
    resolution_mode: ResolutionMode,
    prerelease_mode: PreReleaseMode,
    dependency_mode: DependencyMode,
    upgrade: Upgrade,
    index_locations: IndexLocations,
    keyring_provider: KeyringProvider,
    reinstall: &Reinstall,
    link_mode: LinkMode,
    compile: bool,
    setup_py: SetupPyStrategy,
    connectivity: Connectivity,
    config_settings: &ConfigSettings,
    no_build_isolation: bool,
    no_build: &NoBuild,
    no_binary: &NoBinary,
    strict: bool,
    exclude_newer: Option<DateTime<Utc>>,
    python: Option<String>,
    system: bool,
    break_system_packages: bool,
    native_tls: bool,
    cache: Cache,
    dry_run: bool,
    printer: Printer,
) -> Result<ExitStatus> {
    let start = std::time::Instant::now();

    // Read all requirements from the provided sources.
    let RequirementsSpecification {
        project,
        requirements,
        constraints,
        overrides,
        editables,
        index_url,
        extra_index_urls,
        no_index,
        find_links,
        extras: used_extras,
    } = specification(requirements, constraints, overrides, extras, connectivity).await?;

    // Check that all provided extras are used
    if let ExtrasSpecification::Some(extras) = extras {
        let mut unused_extras = extras
            .iter()
            .filter(|extra| !used_extras.contains(extra))
            .collect::<Vec<_>>();
        if !unused_extras.is_empty() {
            unused_extras.sort_unstable();
            unused_extras.dedup();
            let s = if unused_extras.len() == 1 { "" } else { "s" };
            return Err(anyhow!(
                "Requested extra{s} not found: {}",
                unused_extras.iter().join(", ")
            ));
        }
    }

    // Detect the current Python interpreter.
    let venv = if let Some(python) = python.as_ref() {
        PythonEnvironment::from_requested_python(python, &cache)?
    } else if system {
        PythonEnvironment::from_default_python(&cache)?
    } else {
        PythonEnvironment::from_virtualenv(&cache)?
    };
    debug!(
        "Using Python {} environment at {}",
        venv.interpreter().python_version(),
        venv.python_executable().simplified_display().cyan()
    );

    // If the environment is externally managed, abort.
    if let Some(externally_managed) = venv.interpreter().is_externally_managed() {
        if break_system_packages {
            debug!("Ignoring externally managed environment due to `--break-system-packages`");
        } else {
            return if let Some(error) = externally_managed.into_error() {
                Err(anyhow::anyhow!(
                    "The interpreter at {} is externally managed, and indicates the following:\n\n{}\n\nConsider creating a virtual environment with `uv venv`.",
                    venv.root().simplified_display().cyan(),
                    textwrap::indent(&error, "  ").green(),
                ))
            } else {
                Err(anyhow::anyhow!(
                    "The interpreter at {} is externally managed. Instead, create a virtual environment with `uv venv`.",
                    venv.root().simplified_display().cyan()
                ))
            };
        }
    }

    let _lock = venv.lock()?;

    // Determine the set of installed packages.
    let site_packages = SitePackages::from_executable(&venv)?;

    // If the requirements are already satisfied, we're done. Ideally, the resolver would be fast
    // enough to let us remove this check. But right now, for large environments, it's an order of
    // magnitude faster to validate the environment than to resolve the requirements.
    if reinstall.is_none()
        && upgrade.is_none()
        && site_packages.satisfies(&requirements, &editables, &constraints)?
    {
        let num_requirements = requirements.len() + editables.len();
        let s = if num_requirements == 1 { "" } else { "s" };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Audited {} in {}",
                format!("{num_requirements} package{s}").bold(),
                elapsed(start.elapsed())
            )
            .dimmed()
        )?;
        if dry_run {
            writeln!(printer.stderr(), "Would make no changes")?;
        }
        return Ok(ExitStatus::Success);
    }

    // Determine the tags, markers, and interpreter to use for resolution.
    let interpreter = venv.interpreter().clone();
    let tags = venv.interpreter().tags()?;
    let markers = venv.interpreter().markers();

    // Incorporate any index locations from the provided sources.
    let index_locations =
        index_locations.combine(index_url, extra_index_urls, find_links, no_index);

    // Add all authenticated sources to the store.
    for url in index_locations.urls() {
        GLOBAL_AUTH_STORE.save_from_url(url);
    }

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(cache.clone())
        .native_tls(native_tls)
        .connectivity(connectivity)
        .index_urls(index_locations.index_urls())
        .keyring_provider(keyring_provider)
        .build();

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, &cache);
        let entries = client.fetch(index_locations.flat_index()).await?;
        FlatIndex::from_entries(entries, tags)
    };

    // Determine whether to enable build isolation.
    let build_isolation = if no_build_isolation {
        BuildIsolation::Shared(&venv)
    } else {
        BuildIsolation::Isolated
    };

    // Create a shared in-memory index.
    let index = InMemoryIndex::default();

    // Track in-flight downloads, builds, etc., across resolutions.
    let in_flight = InFlight::default();

    let resolve_dispatch = BuildDispatch::new(
        &client,
        &cache,
        &interpreter,
        &index_locations,
        &flat_index,
        &index,
        &in_flight,
        setup_py,
        config_settings,
        build_isolation,
        no_build,
        no_binary,
    )
    .with_options(OptionsBuilder::new().exclude_newer(exclude_newer).build());

    // Build all editable distributions. The editables are shared between resolution and
    // installation, and should live for the duration of the command. If an editable is already
    // installed in the environment, we'll still re-build it here.
    let editable_wheel_dir;
    let editables = if editables.is_empty() {
        vec![]
    } else {
        editable_wheel_dir = tempdir_in(venv.root())?;
        build_editables(
            &editables,
            editable_wheel_dir.path(),
            &cache,
            &interpreter,
            tags,
            &client,
            &resolve_dispatch,
            printer,
        )
        .await?
    };

    let options = OptionsBuilder::new()
        .resolution_mode(resolution_mode)
        .prerelease_mode(prerelease_mode)
        .dependency_mode(dependency_mode)
        .exclude_newer(exclude_newer)
        .build();

    // Resolve the requirements.
    let resolution = match resolve(
        requirements,
        constraints,
        overrides,
        project,
        &editables,
        &site_packages,
        reinstall,
        &upgrade,
        &interpreter,
        tags,
        markers,
        &client,
        &flat_index,
        &index,
        &resolve_dispatch,
        options,
        printer,
    )
    .await
    {
        Ok(resolution) => Resolution::from(resolution),
        Err(Error::Resolve(uv_resolver::ResolveError::NoSolution(err))) => {
            let report = miette::Report::msg(format!("{err}"))
                .context("No solution found when resolving dependencies:");
            eprint!("{report:?}");
            return Ok(ExitStatus::Failure);
        }
        Err(err) => return Err(err.into()),
    };

    // Re-initialize the in-flight map.
    let in_flight = InFlight::default();

    // If we're running with `--reinstall`, initialize a separate `BuildDispatch`, since we may
    // end up removing some distributions from the environment.
    let install_dispatch = if reinstall.is_none() {
        resolve_dispatch
    } else {
        BuildDispatch::new(
            &client,
            &cache,
            &interpreter,
            &index_locations,
            &flat_index,
            &index,
            &in_flight,
            setup_py,
            config_settings,
            build_isolation,
            no_build,
            no_binary,
        )
        .with_options(OptionsBuilder::new().exclude_newer(exclude_newer).build())
    };

    // Sync the environment.
    install(
        &resolution,
        editables,
        site_packages,
        reinstall,
        no_binary,
        link_mode,
        compile,
        &index_locations,
        tags,
        &client,
        &in_flight,
        &install_dispatch,
        &cache,
        &venv,
        dry_run,
        printer,
    )
    .await?;

    // Validate the environment.
    if strict {
        validate(&resolution, &venv, printer)?;
    }

    Ok(ExitStatus::Success)
}

/// Consolidate the requirements for an installation.
async fn specification(
    requirements: &[RequirementsSource],
    constraints: &[RequirementsSource],
    overrides: &[RequirementsSource],
    extras: &ExtrasSpecification<'_>,
    connectivity: Connectivity,
) -> Result<RequirementsSpecification, Error> {
    // If the user requests `extras` but does not provide a pyproject toml source
    if !matches!(extras, ExtrasSpecification::None)
        && !requirements
            .iter()
            .any(|source| matches!(source, RequirementsSource::PyprojectToml(_)))
    {
        return Err(anyhow!("Requesting extras requires a pyproject.toml input file.").into());
    }

    // Read all requirements from the provided sources.
    let spec = RequirementsSpecification::from_sources(
        requirements,
        constraints,
        overrides,
        extras,
        connectivity,
    )
    .await?;

    // Check that all provided extras are used
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

    Ok(spec)
}

/// Build a set of editable distributions.
#[allow(clippy::too_many_arguments)]
async fn build_editables(
    editables: &[EditableRequirement],
    editable_wheel_dir: &Path,
    cache: &Cache,
    interpreter: &Interpreter,
    tags: &Tags,
    client: &RegistryClient,
    build_dispatch: &BuildDispatch<'_>,
    printer: Printer,
) -> Result<Vec<BuiltEditable>, Error> {
    let start = Instant::now();

    let downloader = Downloader::new(cache, tags, client, build_dispatch)
        .with_reporter(DownloadReporter::from(printer).with_length(editables.len() as u64));

    let editables: Vec<LocalEditable> = editables
        .iter()
        .map(|editable| {
            let EditableRequirement { url, extras, path } = editable;
            Ok(LocalEditable {
                url: url.clone(),
                extras: extras.clone(),
                path: path.clone(),
            })
        })
        .collect::<Result<_>>()?;

    let editables: Vec<_> = downloader
        .build_editables(editables, editable_wheel_dir)
        .await
        .context("Failed to build editables")?
        .into_iter()
        .collect();

    // Validate that the editables are compatible with the target Python version.
    for editable in &editables {
        if let Some(python_requires) = editable.metadata.requires_python.as_ref() {
            if !python_requires.contains(interpreter.python_version()) {
                return Err(anyhow!(
                    "Editable `{}` requires Python {}, but {} is installed",
                    editable.metadata.name,
                    python_requires,
                    interpreter.python_version()
                )
                .into());
            }
        }
    }

    let s = if editables.len() == 1 { "" } else { "s" };
    writeln!(
        printer.stderr(),
        "{}",
        format!(
            "Built {} in {}",
            format!("{} editable{}", editables.len(), s).bold(),
            elapsed(start.elapsed())
        )
        .dimmed()
    )?;

    Ok(editables)
}

/// Resolve a set of requirements, similar to running `pip compile`.
#[allow(clippy::too_many_arguments)]
async fn resolve(
    requirements: Vec<Requirement>,
    constraints: Vec<Requirement>,
    overrides: Vec<Requirement>,
    project: Option<PackageName>,
    editables: &[BuiltEditable],
    site_packages: &SitePackages<'_>,
    reinstall: &Reinstall,
    upgrade: &Upgrade,
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

    let preferences = if upgrade.is_all() || reinstall.is_all() {
        vec![]
    } else {
        // Combine upgrade and reinstall lists
        let mut exclusions: HashSet<&PackageName> = if let Reinstall::Packages(packages) = reinstall
        {
            HashSet::from_iter(packages)
        } else {
            HashSet::default()
        };
        if let Upgrade::Packages(packages) = upgrade {
            exclusions.extend(packages);
        };

        // Prefer current site packages, unless in the upgrade or reinstall lists
        site_packages
            .requirements()
            .filter(|requirement| !exclusions.contains(&requirement.name))
            .collect()
    };

    // Map the editables to their metadata.
    let editables = editables
        .iter()
        .map(|built_editable| {
            (
                built_editable.editable.clone(),
                built_editable.metadata.clone(),
            )
        })
        .collect();

    // Create a manifest of the requirements.
    let manifest = Manifest::new(
        requirements,
        constraints,
        overrides,
        preferences,
        project,
        editables,
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
        build_dispatch,
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

    Ok(resolution)
}

/// Install a set of requirements into the current environment.
#[allow(clippy::too_many_arguments)]
async fn install(
    resolution: &Resolution,
    built_editables: Vec<BuiltEditable>,
    site_packages: SitePackages<'_>,
    reinstall: &Reinstall,
    no_binary: &NoBinary,
    link_mode: LinkMode,
    compile: bool,
    index_urls: &IndexLocations,
    tags: &Tags,
    client: &RegistryClient,
    in_flight: &InFlight,
    build_dispatch: &BuildDispatch<'_>,
    cache: &Cache,
    venv: &PythonEnvironment,
    dry_run: bool,
    printer: Printer,
) -> Result<(), Error> {
    let start = std::time::Instant::now();

    let requirements = resolution.requirements();

    // Map the built editables to their resolved form.
    let editables = built_editables
        .into_iter()
        .map(ResolvedEditable::Built)
        .collect::<Vec<_>>();

    // Partition into those that should be linked from the cache (`local`), those that need to be
    // downloaded (`remote`), and those that should be removed (`extraneous`).
    let plan = Planner::with_requirements(&requirements)
        .with_editable_requirements(&editables)
        .build(
            site_packages,
            reinstall,
            no_binary,
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
        local,
        remote,
        reinstalls,
        extraneous: _,
    } = plan;

    // Nothing to do.
    if remote.is_empty() && local.is_empty() && reinstalls.is_empty() {
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
                .get(&dist.name)
                .cloned()
                .expect("Resolution should contain all packages")
        })
        .collect::<Vec<_>>();

    // Download, build, and unzip any missing distributions.
    let wheels = if remote.is_empty() {
        vec![]
    } else {
        let start = Instant::now();

        let downloader = Downloader::new(cache, tags, client, build_dispatch)
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
            let summary = uv_installer::uninstall(dist_info).await?;
            debug!(
                "Uninstalled {} ({} file{}, {} director{})",
                dist_info.name(),
                summary.file_count,
                if summary.file_count == 1 { "" } else { "s" },
                summary.dir_count,
                if summary.dir_count == 1 { "y" } else { "ies" },
            );
        }
    }

    // Install the resolved distributions.
    let wheels = wheels.into_iter().chain(local).collect::<Vec<_>>();
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

    #[allow(clippy::items_after_statements)]
    fn report_dry_run(
        resolution: &Resolution,
        plan: Plan,
        start: Instant,
        printer: Printer,
    ) -> Result<(), Error> {
        let Plan {
            local,
            remote,
            reinstalls,
            extraneous: _,
        } = plan;

        // Nothing to do.
        if remote.is_empty() && local.is_empty() && reinstalls.is_empty() {
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
                    .get(&dist.name)
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
        let installs = wheels.len() + local.len();

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
            .chain(local.into_iter().map(|distribution| DryRunEvent {
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

/// Validate the installed packages in the virtual environment.
fn validate(
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
enum Error {
    #[error(transparent)]
    Resolve(#[from] uv_resolver::ResolveError),

    #[error(transparent)]
    Client(#[from] uv_client::Error),

    #[error(transparent)]
    Platform(#[from] platform_tags::PlatformError),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}
