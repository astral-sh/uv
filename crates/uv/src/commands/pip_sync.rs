use std::borrow::Cow;
use std::fmt::Write;

use anstream::eprint;
use anyhow::{anyhow, Context, Result};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::debug;

use distribution_types::{
    IndexLocations, InstalledMetadata, LocalDist, LocalEditable, LocalEditables, Name, ResolvedDist,
};
use install_wheel_rs::linker::LinkMode;
use platform_tags::Tags;
use pypi_types::Yanked;
use requirements_txt::EditableRequirement;
use uv_auth::store_credentials_from_url;
use uv_cache::{ArchiveTarget, ArchiveTimestamp, Cache};
use uv_client::{
    BaseClientBuilder, Connectivity, FlatIndexClient, RegistryClient, RegistryClientBuilder,
};
use uv_configuration::{
    ConfigSettings, IndexStrategy, NoBinary, NoBuild, PreviewMode, Reinstall, SetupPyStrategy,
};
use uv_configuration::{KeyringProviderType, TargetTriple};
use uv_dispatch::BuildDispatch;
use uv_fs::Simplified;
use uv_installer::{is_dynamic, Downloader, Plan, Planner, ResolvedEditable, SitePackages};
use uv_interpreter::{Interpreter, PythonEnvironment, PythonVersion, Target};
use uv_requirements::{
    ExtrasSpecification, NamedRequirementsResolver, RequirementsSource, RequirementsSpecification,
    SourceTreeResolver,
};
use uv_resolver::{DependencyMode, FlatIndex, InMemoryIndex, Manifest, OptionsBuilder, Resolver};
use uv_types::{BuildIsolation, EmptyInstalledPackages, HashStrategy, InFlight};
use uv_warnings::warn_user;

use crate::commands::reporters::{DownloadReporter, InstallReporter, ResolverReporter};
use crate::commands::{compile_bytecode, elapsed, ChangeEvent, ChangeEventKind, ExitStatus};
use crate::printer::Printer;

/// Install a set of locked requirements into the current Python environment.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub(crate) async fn pip_sync(
    sources: &[RequirementsSource],
    reinstall: &Reinstall,
    link_mode: LinkMode,
    compile: bool,
    require_hashes: bool,
    index_locations: IndexLocations,
    index_strategy: IndexStrategy,
    keyring_provider: KeyringProviderType,
    setup_py: SetupPyStrategy,
    connectivity: Connectivity,
    config_settings: &ConfigSettings,
    no_build_isolation: bool,
    no_build: NoBuild,
    no_binary: NoBinary,
    python_version: Option<PythonVersion>,
    python_platform: Option<TargetTriple>,
    strict: bool,
    python: Option<String>,
    system: bool,
    break_system_packages: bool,
    target: Option<Target>,
    native_tls: bool,
    preview: PreviewMode,
    cache: Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    let start = std::time::Instant::now();

    let client_builder = BaseClientBuilder::new()
        .connectivity(connectivity)
        .native_tls(native_tls)
        .keyring(keyring_provider);

    // Read all requirements from the provided sources.
    let RequirementsSpecification {
        project: _,
        requirements,
        constraints: _,
        overrides: _,
        editables,
        source_trees,
        extras: _,
        index_url,
        extra_index_urls,
        no_index,
        find_links,
        no_binary: specified_no_binary,
        no_build: specified_no_build,
    } = RequirementsSpecification::from_simple_sources(sources, &client_builder, preview).await?;

    // Validate that the requirements are non-empty.
    let num_requirements = requirements.len() + source_trees.len() + editables.len();
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
        venv.python_executable().user_display().cyan()
    );

    // Apply any `--target` directory.
    let venv = if let Some(target) = target {
        debug!(
            "Using `--target` directory at {}",
            target.root().user_display()
        );
        target.init()?;
        venv.with_target(target)
    } else {
        venv
    };

    // If the environment is externally managed, abort.
    if let Some(externally_managed) = venv.interpreter().is_externally_managed() {
        if break_system_packages {
            debug!("Ignoring externally managed environment due to `--break-system-packages`");
        } else {
            return if let Some(error) = externally_managed.into_error() {
                Err(anyhow::anyhow!(
                    "The interpreter at {} is externally managed, and indicates the following:\n\n{}\n\nConsider creating a virtual environment with `uv venv`.",
                    venv.root().user_display().cyan(),
                    textwrap::indent(&error, "  ").green(),
                ))
            } else {
                Err(anyhow::anyhow!(
                    "The interpreter at {} is externally managed. Instead, create a virtual environment with `uv venv`.",
                    venv.root().user_display().cyan()
                ))
            };
        }
    }

    let _lock = venv.lock()?;

    let interpreter = venv.interpreter();

    // Determine the current environment markers.
    let tags = match (python_platform, python_version.as_ref()) {
        (Some(python_platform), Some(python_version)) => Cow::Owned(Tags::from_env(
            &python_platform.platform(),
            (python_version.major(), python_version.minor()),
            interpreter.implementation_name(),
            interpreter.implementation_tuple(),
            interpreter.gil_disabled(),
        )?),
        (Some(python_platform), None) => Cow::Owned(Tags::from_env(
            &python_platform.platform(),
            interpreter.python_tuple(),
            interpreter.implementation_name(),
            interpreter.implementation_tuple(),
            interpreter.gil_disabled(),
        )?),
        (None, Some(python_version)) => Cow::Owned(Tags::from_env(
            interpreter.platform(),
            (python_version.major(), python_version.minor()),
            interpreter.implementation_name(),
            interpreter.implementation_tuple(),
            interpreter.gil_disabled(),
        )?),
        (None, None) => Cow::Borrowed(interpreter.tags()?),
    };

    // Apply the platform tags to the markers.
    let markers = match (python_platform, python_version) {
        (Some(python_platform), Some(python_version)) => {
            Cow::Owned(python_version.markers(&python_platform.markers(interpreter.markers())))
        }
        (Some(python_platform), None) => Cow::Owned(python_platform.markers(interpreter.markers())),
        (None, Some(python_version)) => Cow::Owned(python_version.markers(interpreter.markers())),
        (None, None) => Cow::Borrowed(interpreter.markers()),
    };

    // Collect the set of required hashes.
    let hasher = if require_hashes {
        HashStrategy::from_requirements(
            requirements
                .iter()
                .map(|entry| (&entry.requirement, entry.hashes.as_slice())),
            &markers,
        )?
    } else {
        HashStrategy::None
    };

    // Incorporate any index locations from the provided sources.
    let index_locations =
        index_locations.combine(index_url, extra_index_urls, find_links, no_index);

    // Add all authenticated sources to the cache.
    for url in index_locations.urls() {
        store_credentials_from_url(url);
    }

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(cache.clone())
        .native_tls(native_tls)
        .connectivity(connectivity)
        .index_urls(index_locations.index_urls())
        .index_strategy(index_strategy)
        .keyring(keyring_provider)
        .markers(venv.interpreter().markers())
        .platform(venv.interpreter().platform())
        .build();

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, &cache);
        let entries = client.fetch(index_locations.flat_index()).await?;
        FlatIndex::from_entries(entries, &tags, &hasher, &no_build, &no_binary)
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

    // Combine the `--no-binary` and `--no-build` flags.
    let no_binary = no_binary.combine(specified_no_binary);
    let no_build = no_build.combine(specified_no_build);

    // Determine the set of installed packages.
    let site_packages = SitePackages::from_executable(&venv)?;

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
        link_mode,
        &no_build,
        &no_binary,
    );

    // Convert from unnamed to named requirements.
    let requirements = {
        // Convert from unnamed to named requirements.
        let mut requirements =
            NamedRequirementsResolver::new(requirements, &hasher, &build_dispatch, &client, &index)
                .with_reporter(ResolverReporter::from(printer))
                .resolve()
                .await?;

        // Resolve any source trees into requirements.
        if !source_trees.is_empty() {
            requirements.extend(
                SourceTreeResolver::new(
                    source_trees,
                    &ExtrasSpecification::None,
                    &hasher,
                    &build_dispatch,
                    &client,
                    &index,
                )
                .with_reporter(ResolverReporter::from(printer))
                .resolve()
                .await?,
            );
        }

        requirements
    };

    // Resolve any editables.
    let resolved_editables = resolve_editables(
        editables,
        &site_packages,
        reinstall,
        &hasher,
        venv.interpreter(),
        &tags,
        &cache,
        &client,
        &build_dispatch,
        printer,
    )
    .await?;

    // Partition into those that should be linked from the cache (`cached`), those that need to be
    // downloaded (`remote`), and those that should be removed (`extraneous`).
    let Plan {
        cached,
        remote,
        reinstalls,
        installed: _,
        extraneous,
    } = Planner::with_requirements(&requirements)
        .with_editable_requirements(&resolved_editables.editables)
        .build(
            site_packages,
            reinstall,
            &no_binary,
            &hasher,
            &index_locations,
            &cache,
            &venv,
            &tags,
        )
        .context("Failed to determine installation plan")?;

    // Nothing to do.
    if remote.is_empty() && cached.is_empty() && reinstalls.is_empty() && extraneous.is_empty() {
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

        // Determine the tags, markers, and interpreter to use for resolution.
        let interpreter = venv.interpreter();
        let tags = interpreter.tags()?;
        let markers = interpreter.markers();

        // Resolve with `--no-deps`.
        let options = OptionsBuilder::new()
            .dependency_mode(DependencyMode::Direct)
            .build();

        // Create a bound on the progress bar, since we know the number of packages upfront.
        let reporter = ResolverReporter::from(printer).with_length(remote.len() as u64);

        // Run the resolver.
        let resolver = Resolver::new(
            Manifest::simple(remote),
            options,
            markers,
            interpreter,
            tags,
            &client,
            &flat_index,
            &index,
            &hasher,
            &build_dispatch,
            // TODO(zanieb): We should consider support for installed packages in pip sync
            &EmptyInstalledPackages,
        )?
        .with_reporter(reporter);

        let resolution = match resolver.resolve().await {
            Err(uv_resolver::ResolveError::NoSolution(err)) => {
                let report = miette::Report::msg(format!("{err}"))
                    .context("No solution found when resolving dependencies:");
                eprint!("{report:?}");
                return Ok(ExitStatus::Failure);
            }
            result => result,
        }?;

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

        resolution
            .into_distributions()
            .filter_map(|dist| match dist {
                ResolvedDist::Installable(dist) => Some(dist),
                ResolvedDist::Installed(_) => None,
            })
            .collect::<Vec<_>>()
    };

    // Download, build, and unzip any missing distributions.
    let wheels = if remote.is_empty() {
        Vec::new()
    } else {
        let start = std::time::Instant::now();

        let downloader = Downloader::new(&cache, &tags, &hasher, &client, &build_dispatch)
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
                "Uninstalled {} in {}",
                format!("{} package{}", extraneous.len() + reinstalls.len(), s).bold(),
                elapsed(start.elapsed())
            )
            .dimmed()
        )?;
    }

    // Install the resolved distributions.
    let wheels = wheels.into_iter().chain(cached).collect::<Vec<_>>();
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
    hasher: &HashStrategy,
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

        let downloader = Downloader::new(cache, tags, hasher, client, build_dispatch)
            .with_reporter(DownloadReporter::from(printer).with_length(uninstalled.len() as u64));

        let editables = LocalEditables::from_editables(uninstalled.iter().map(|editable| {
            let EditableRequirement { url, path, extras } = editable;
            LocalEditable {
                url: url.clone(),
                path: path.clone(),
                extras: extras.clone(),
            }
        }));

        let editable_wheel_dir = tempfile::tempdir_in(cache.root())?;
        let editables: Vec<_> = downloader
            .build_editables(editables, editable_wheel_dir.path())
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
                    ));
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

        (editables, Some(editable_wheel_dir))
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
