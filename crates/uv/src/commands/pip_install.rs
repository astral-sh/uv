use std::collections::HashSet;
use std::fmt::Write;

use std::path::Path;

use anstream::eprint;
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tempfile::tempdir_in;
use tracing::debug;

use distribution_types::{
    IndexLocations, InstalledMetadata, LocalDist, LocalEditable, Name, Resolution,
};
use install_wheel_rs::linker::LinkMode;
use pep508_rs::{MarkerEnvironment, Requirement};
use platform_host::Platform;
use platform_tags::Tags;
use pypi_types::Yanked;
use requirements_txt::EditableRequirement;
use uv_cache::Cache;
use uv_client::{Connectivity, FlatIndex, FlatIndexClient, RegistryClient, RegistryClientBuilder};
use uv_dispatch::BuildDispatch;
use uv_fs::Normalized;
use uv_installer::{
    BuiltEditable, Downloader, NoBinary, Plan, Planner, Reinstall, ResolvedEditable, SitePackages,
};
use uv_interpreter::{Interpreter, Virtualenv};
use uv_normalize::PackageName;
use uv_resolver::{
    DependencyMode, InMemoryIndex, Manifest, Options, OptionsBuilder, PreReleaseMode,
    ResolutionGraph, ResolutionMode, Resolver,
};
use uv_traits::{InFlight, NoBuild, SetupPyStrategy};

use crate::commands::reporters::{DownloadReporter, InstallReporter, ResolverReporter};
use crate::commands::{elapsed, ChangeEvent, ChangeEventKind, ExitStatus};
use crate::printer::Printer;
use crate::requirements::{ExtrasSpecification, RequirementsSource, RequirementsSpecification};

use super::Upgrade;

/// Install packages into the current environment.
#[allow(clippy::too_many_arguments)]
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
    reinstall: &Reinstall,
    link_mode: LinkMode,
    setup_py: SetupPyStrategy,
    connectivity: Connectivity,
    no_build: &NoBuild,
    no_binary: &NoBinary,
    strict: bool,
    exclude_newer: Option<DateTime<Utc>>,
    cache: Cache,
    mut printer: Printer,
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
    } = specification(requirements, constraints, overrides, extras)?;

    // Incorporate any index locations from the provided sources.
    let index_locations =
        index_locations.combine(index_url, extra_index_urls, find_links, no_index);

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
    let platform = Platform::current()?;
    let venv = Virtualenv::from_env(platform, &cache)?;
    debug!(
        "Using Python {} environment at {}",
        venv.interpreter().python_version(),
        venv.python_executable().normalized_display().cyan()
    );

    let _lock = venv.lock()?;

    // Determine the set of installed packages.
    let site_packages =
        SitePackages::from_executable(&venv).context("Failed to list installed packages")?;

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
            printer,
            "{}",
            format!(
                "Audited {} in {}",
                format!("{num_requirements} package{s}").bold(),
                elapsed(start.elapsed())
            )
            .dimmed()
        )?;
        return Ok(ExitStatus::Success);
    }

    // Determine the tags, markers, and interpreter to use for resolution.
    let interpreter = venv.interpreter().clone();
    let tags = venv.interpreter().tags()?;
    let markers = venv.interpreter().markers();

    // Instantiate a client.
    let client = RegistryClientBuilder::new(cache.clone())
        .index_urls(index_locations.index_urls())
        .connectivity(connectivity)
        .build();

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, &cache);
        let entries = client.fetch(index_locations.flat_index()).await?;
        FlatIndex::from_entries(entries, tags)
    };

    // Create a shared in-memory index.
    let index = InMemoryIndex::default();

    // Track in-flight downloads, builds, etc., across resolutions.
    let in_flight = InFlight::default();

    let options = OptionsBuilder::new()
        .resolution_mode(resolution_mode)
        .prerelease_mode(prerelease_mode)
        .dependency_mode(dependency_mode)
        .exclude_newer(exclude_newer)
        .build();

    let resolve_dispatch = BuildDispatch::new(
        &client,
        &cache,
        &interpreter,
        &index_locations,
        &flat_index,
        &index,
        &in_flight,
        venv.python_executable(),
        setup_py,
        no_build,
        no_binary,
    )
    .with_options(options);

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
            tags,
            &client,
            &resolve_dispatch,
            printer,
        )
        .await?
    };

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
            venv.python_executable(),
            setup_py,
            no_build,
            no_binary,
        )
    };

    // Sync the environment.
    install(
        &resolution,
        editables,
        site_packages,
        reinstall,
        no_binary,
        link_mode,
        &index_locations,
        tags,
        &client,
        &in_flight,
        &install_dispatch,
        &cache,
        &venv,
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
fn specification(
    requirements: &[RequirementsSource],
    constraints: &[RequirementsSource],
    overrides: &[RequirementsSource],
    extras: &ExtrasSpecification<'_>,
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
    let spec =
        RequirementsSpecification::from_sources(requirements, constraints, overrides, extras)?;

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
async fn build_editables(
    editables: &[EditableRequirement],
    editable_wheel_dir: &Path,
    cache: &Cache,
    tags: &Tags,
    client: &RegistryClient,
    build_dispatch: &BuildDispatch<'_>,
    mut printer: Printer,
) -> Result<Vec<BuiltEditable>, Error> {
    let start = std::time::Instant::now();

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

    let s = if editables.len() == 1 { "" } else { "s" };
    writeln!(
        printer,
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
    mut printer: Printer,
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
    )
    .with_reporter(ResolverReporter::from(printer));
    let resolution = resolver.resolve().await?;

    let s = if resolution.len() == 1 { "" } else { "s" };
    writeln!(
        printer,
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
    index_urls: &IndexLocations,
    tags: &Tags,
    client: &RegistryClient,
    in_flight: &InFlight,
    build_dispatch: &BuildDispatch<'_>,
    cache: &Cache,
    venv: &Virtualenv,
    mut printer: Printer,
) -> Result<(), Error> {
    let start = std::time::Instant::now();

    // Partition into those that should be linked from the cache (`local`), those that need to be
    // downloaded (`remote`), and those that should be removed (`extraneous`).
    let requirements = resolution.requirements();
    let editables = built_editables
        .into_iter()
        .map(ResolvedEditable::Built)
        .collect::<Vec<_>>();

    let Plan {
        local,
        remote,
        reinstalls,
        extraneous: _,
    } = Planner::with_requirements(&requirements)
        .with_editable_requirements(editables)
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

    // Nothing to do.
    if remote.is_empty() && local.is_empty() && reinstalls.is_empty() {
        let s = if resolution.len() == 1 { "" } else { "s" };
        writeln!(
            printer,
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
                    printer,
                    "{}{} {dist} is yanked.",
                    "warning".yellow().bold(),
                    ":".bold(),
                )?;
            }
            Some(Yanked::Reason(reason)) => {
                writeln!(
                    printer,
                    "{}{} {dist} is yanked (reason: \"{reason}\").",
                    "warning".yellow().bold(),
                    ":".bold(),
                )?;
            }
        }
    }

    // Download, build, and unzip any missing distributions.
    let wheels = if remote.is_empty() {
        vec![]
    } else {
        let start = std::time::Instant::now();

        let downloader = Downloader::new(cache, tags, client, build_dispatch)
            .with_reporter(DownloadReporter::from(printer).with_length(remote.len() as u64));

        let wheels = downloader
            .download(remote, in_flight)
            .await
            .context("Failed to download distributions")?;

        let s = if wheels.len() == 1 { "" } else { "s" };
        writeln!(
            printer,
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
            printer,
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
        })
    {
        match event.kind {
            ChangeEventKind::Added => {
                writeln!(
                    printer,
                    " {} {}{}",
                    "+".green(),
                    event.dist.name().as_ref().white().bold(),
                    event.dist.installed_version().to_string().dimmed()
                )?;
            }
            ChangeEventKind::Removed => {
                writeln!(
                    printer,
                    " {} {}{}",
                    "-".red(),
                    event.dist.name().as_ref().white().bold(),
                    event.dist.installed_version().to_string().dimmed()
                )?;
            }
        }
    }

    Ok(())
}

/// Validate the installed packages in the virtual environment.
fn validate(resolution: &Resolution, venv: &Virtualenv, mut printer: Printer) -> Result<(), Error> {
    let site_packages = SitePackages::from_executable(venv)?;
    let diagnostics = site_packages.diagnostics()?;
    for diagnostic in diagnostics {
        // Only surface diagnostics that are "relevant" to the current resolution.
        if resolution
            .packages()
            .any(|package| diagnostic.includes(package))
        {
            writeln!(
                printer,
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
    Platform(#[from] platform_host::PlatformError),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}
