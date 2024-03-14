use std::fmt::Write;

use anyhow::{anyhow, Context, Result};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::debug;

use distribution_types::{IndexLocations, InstalledMetadata, LocalDist, LocalEditable, Name};
use install_wheel_rs::linker::LinkMode;
use platform_tags::Tags;
use pypi_types::Yanked;
use requirements_txt::EditableRequirement;
use uv_auth::{KeyringProvider, GLOBAL_AUTH_STORE};
use uv_cache::{ArchiveTarget, ArchiveTimestamp, Cache};
use uv_client::{Connectivity, FlatIndex, FlatIndexClient, RegistryClient, RegistryClientBuilder};
use uv_dispatch::BuildDispatch;
use uv_fs::Simplified;
use uv_installer::{
    is_dynamic, Downloader, NoBinary, Plan, Planner, Reinstall, ResolvedEditable, SitePackages,
};
use uv_interpreter::{Interpreter, PythonEnvironment};
use uv_resolver::InMemoryIndex;
use uv_traits::{BuildIsolation, ConfigSettings, InFlight, NoBuild, SetupPyStrategy};

use crate::commands::reporters::{DownloadReporter, FinderReporter, InstallReporter};
use crate::commands::{compile_bytecode, elapsed, ChangeEvent, ChangeEventKind, ExitStatus};
use crate::printer::Printer;
use crate::requirements::{RequirementsSource, RequirementsSpecification};

/// Install a set of locked requirements into the current Python environment.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub(crate) async fn pip_sync(
    sources: &[RequirementsSource],
    reinstall: &Reinstall,
    link_mode: LinkMode,
    compile: bool,
    index_locations: IndexLocations,
    keyring_provider: KeyringProvider,
    setup_py: SetupPyStrategy,
    connectivity: Connectivity,
    config_settings: &ConfigSettings,
    no_build_isolation: bool,
    no_build: &NoBuild,
    no_binary: &NoBinary,
    strict: bool,
    python: Option<String>,
    system: bool,
    break_system_packages: bool,
    native_tls: bool,
    cache: Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    let start = std::time::Instant::now();

    // Read all requirements from the provided sources.
    let RequirementsSpecification {
        project: _project,
        requirements,
        constraints: _constraints,
        overrides: _overrides,
        editables,
        index_url,
        extra_index_urls,
        no_index,
        find_links,
        extras: _extras,
    } = RequirementsSpecification::from_simple_sources(sources, connectivity).await?;

    let num_requirements = requirements.len() + editables.len();
    if num_requirements == 0 {
        writeln!(printer.stderr(), "No requirements found")?;
        return Ok(ExitStatus::Success);
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

    // Determine the current environment markers.
    let tags = venv.interpreter().tags()?;

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

    // Create a shared in-memory index.
    let index = InMemoryIndex::default();

    // Track in-flight downloads, builds, etc., across resolutions.
    let in_flight = InFlight::default();

    // Determine whether to enable build isolation.
    let build_isolation = if no_build_isolation {
        BuildIsolation::Shared(&venv)
    } else {
        BuildIsolation::Isolated
    };

    // Prep the build context.
    let build_dispatch = BuildDispatch::new(
        &client,
        &cache,
        venv.interpreter(),
        &index_locations,
        &flat_index,
        &index,
        &in_flight,
        setup_py,
        config_settings,
        build_isolation,
        no_build,
        no_binary,
    );

    // Determine the set of installed packages.
    let site_packages = SitePackages::from_executable(&venv)?;

    // Resolve any editables.
    let resolved_editables = resolve_editables(
        editables,
        &site_packages,
        reinstall,
        venv.interpreter(),
        tags,
        &cache,
        &client,
        &build_dispatch,
        printer,
    )
    .await?;

    // Partition into those that should be linked from the cache (`local`), those that need to be
    // downloaded (`remote`), and those that should be removed (`extraneous`).
    let Plan {
        local,
        remote,
        reinstalls,
        extraneous,
    } = Planner::with_requirements(&requirements)
        .with_editable_requirements(&resolved_editables.editables)
        .build(
            site_packages,
            reinstall,
            no_binary,
            &index_locations,
            &cache,
            &venv,
            tags,
        )
        .context("Failed to determine installation plan")?;

    // Nothing to do.
    if remote.is_empty() && local.is_empty() && reinstalls.is_empty() && extraneous.is_empty() {
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

        return Ok(ExitStatus::Success);
    }

    // Resolve any registry-based requirements.
    let remote = if remote.is_empty() {
        Vec::new()
    } else {
        let start = std::time::Instant::now();

        let wheel_finder = uv_resolver::DistFinder::new(
            tags,
            &client,
            venv.interpreter(),
            &flat_index,
            no_binary,
            no_build,
        )
        .with_reporter(FinderReporter::from(printer).with_length(remote.len() as u64));
        let resolution = wheel_finder.resolve(&remote).await?;

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

        resolution.into_distributions().collect::<Vec<_>>()
    };

    // Download, build, and unzip any missing distributions.
    let wheels = if remote.is_empty() {
        Vec::new()
    } else {
        let start = std::time::Instant::now();

        let downloader = Downloader::new(&cache, tags, &client, &build_dispatch)
            .with_reporter(DownloadReporter::from(printer).with_length(remote.len() as u64));

        let wheels = downloader
            .download(remote.clone(), &in_flight)
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

    // Remove any unnecessary packages.
    if !extraneous.is_empty() || !reinstalls.is_empty() {
        let start = std::time::Instant::now();

        for dist_info in extraneous.iter().chain(reinstalls.iter()) {
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

        let s = if extraneous.len() + reinstalls.len() == 1 {
            ""
        } else {
            "s"
        };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Uninstalled {} in {}",
                format!("{} package{}", extraneous.len() + reinstalls.len(), s).bold(),
                elapsed(start.elapsed())
            )
            .dimmed()
        )?;
    }

    // Install the resolved distributions.
    let wheels = wheels.into_iter().chain(local).collect::<Vec<_>>();
    if !wheels.is_empty() {
        let start = std::time::Instant::now();
        uv_installer::Installer::new(&venv)
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
        compile_bytecode(&venv, &cache, printer).await?;
    }

    // Report on any changes in the environment.
    for event in extraneous
        .into_iter()
        .chain(reinstalls.into_iter())
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

    // Validate that the environment is consistent.
    if strict {
        let site_packages = SitePackages::from_executable(&venv)?;
        for diagnostic in site_packages.diagnostics()? {
            writeln!(
                printer.stderr(),
                "{}{} {}",
                "warning".yellow().bold(),
                ":".bold(),
                diagnostic.message().bold()
            )?;
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
                    "{}{} {dist} is yanked. Refresh your lockfile to pin an un-yanked version.",
                    "warning".yellow().bold(),
                    ":".bold(),
                )?;
            }
            Some(Yanked::Reason(reason)) => {
                writeln!(
                    printer.stderr(),
                    "{}{} {dist} is yanked (reason: \"{reason}\"). Refresh your lockfile to pin an un-yanked version.",
                    "warning".yellow().bold(),
                    ":".bold(),
                )?;
            }
        }
    }

    Ok(ExitStatus::Success)
}

#[derive(Debug)]
struct ResolvedEditables {
    /// The set of resolved editables, including both those that were already installed and those
    /// that were built.
    editables: Vec<ResolvedEditable>,
    /// The temporary directory in which the built editables were stored.
    #[allow(dead_code)]
    temp_dir: Option<tempfile::TempDir>,
}

/// Resolve the set of editables that need to be installed.
#[allow(clippy::too_many_arguments)]
async fn resolve_editables(
    editables: Vec<EditableRequirement>,
    site_packages: &SitePackages<'_>,
    reinstall: &Reinstall,
    interpreter: &Interpreter,
    tags: &Tags,
    cache: &Cache,
    client: &RegistryClient,
    build_dispatch: &BuildDispatch<'_>,
    printer: Printer,
) -> Result<ResolvedEditables> {
    // Partition the editables into those that are already installed, and those that must be built.
    let mut installed = Vec::with_capacity(editables.len());
    let mut uninstalled = Vec::with_capacity(editables.len());
    for editable in editables {
        match reinstall {
            Reinstall::None => {
                let existing = site_packages.get_editables(editable.raw());
                match existing.as_slice() {
                    [] => uninstalled.push(editable),
                    [dist] => {
                        if ArchiveTimestamp::up_to_date_with(
                            &editable.path,
                            ArchiveTarget::Install(dist),
                        )? && !is_dynamic(&editable)
                        {
                            installed.push((*dist).clone());
                        } else {
                            uninstalled.push(editable);
                        }
                    }
                    _ => {
                        uninstalled.push(editable);
                    }
                }
            }
            Reinstall::All => {
                uninstalled.push(editable);
            }
            Reinstall::Packages(packages) => {
                let existing = site_packages.get_editables(editable.raw());
                match existing.as_slice() {
                    [] => uninstalled.push(editable),
                    [dist] => {
                        if packages.contains(dist.name()) {
                            uninstalled.push(editable);
                        } else if ArchiveTimestamp::up_to_date_with(
                            &editable.path,
                            ArchiveTarget::Install(dist),
                        )? && !is_dynamic(&editable)
                        {
                            installed.push((*dist).clone());
                        } else {
                            uninstalled.push(editable);
                        }
                    }
                    _ => {
                        uninstalled.push(editable);
                    }
                }
            }
        }
    }

    // Build any editable installs.
    let (built_editables, temp_dir) = if uninstalled.is_empty() {
        (Vec::new(), None)
    } else {
        let start = std::time::Instant::now();

        let temp_dir = tempfile::tempdir_in(cache.root())?;

        let downloader = Downloader::new(cache, tags, client, build_dispatch)
            .with_reporter(DownloadReporter::from(printer).with_length(uninstalled.len() as u64));

        let local_editables: Vec<LocalEditable> = uninstalled
            .iter()
            .map(|editable| {
                let EditableRequirement { url, path, extras } = editable;
                Ok(LocalEditable {
                    url: url.clone(),
                    path: path.clone(),
                    extras: extras.clone(),
                })
            })
            .collect::<Result<_>>()?;

        let built_editables: Vec<_> = downloader
            .build_editables(local_editables, temp_dir.path())
            .await
            .context("Failed to build editables")?
            .into_iter()
            .collect();

        // Validate that the editables are compatible with the target Python version.
        for editable in &built_editables {
            if let Some(python_requires) = editable.metadata.requires_python.as_ref() {
                if !python_requires.contains(interpreter.python_version()) {
                    return Err(anyhow!(
                        "Editable `{}` requires Python {}, but {} is installed",
                        editable.metadata.name,
                        python_requires,
                        interpreter.python_version()
                    ));
                }
            }
        }

        let s = if built_editables.len() == 1 { "" } else { "s" };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Built {} in {}",
                format!("{} editable{}", built_editables.len(), s).bold(),
                elapsed(start.elapsed())
            )
            .dimmed()
        )?;

        (built_editables, Some(temp_dir))
    };

    Ok(ResolvedEditables {
        editables: installed
            .into_iter()
            .map(ResolvedEditable::Installed)
            .chain(built_editables.into_iter().map(ResolvedEditable::Built))
            .collect::<Vec<_>>(),
        temp_dir,
    })
}
