use std::fmt::Write;

use anyhow::{Context, Result};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::debug;

use distribution_types::{IndexUrls, InstalledMetadata, LocalDist, LocalEditable, Name, CachedDist};
use install_wheel_rs::linker::LinkMode;
use platform_host::Platform;
use platform_tags::Tags;
use puffin_cache::Cache;
use puffin_client::{RegistryClient, RegistryClientBuilder};
use puffin_dispatch::BuildDispatch;
use puffin_installer::{Downloader, InstallPlan, Reinstall, ResolvedEditable, SitePackages};
use puffin_interpreter::Virtualenv;
use puffin_traits::{OnceMap, SetupPyStrategy};
use pypi_types::Yanked;
use requirements_txt::EditableRequirement;

use crate::commands::reporters::{DownloadReporter, FinderReporter, InstallReporter};
use crate::commands::{elapsed, ChangeEvent, ChangeEventKind, ExitStatus};
use crate::printer::Printer;
use crate::requirements::{RequirementsSource, RequirementsSpecification};

/// Install a set of locked requirements into the current Python environment.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn pip_sync(
    sources: &[RequirementsSource],
    reinstall: &Reinstall,
    link_mode: LinkMode,
    index_urls: IndexUrls,
    setup_py: SetupPyStrategy,
    no_build: bool,
    strict: bool,
    cache: Cache,
    mut printer: Printer,
) -> Result<ExitStatus> {
    let start = std::time::Instant::now();

    // Read all requirements from the provided sources.
    let (requirements, editables) = RequirementsSpecification::requirements_and_editables(sources)?;
    let num_requirements = requirements.len() + editables.len();
    if num_requirements == 0 {
        writeln!(printer, "No requirements found")?;
        return Ok(ExitStatus::Success);
    }

    // Detect the current Python interpreter.
    let platform = Platform::current()?;
    let venv = Virtualenv::from_env(platform, &cache)?;
    debug!(
        "Using Python interpreter: {}",
        venv.python_executable().display()
    );
    let _lock = venv.lock()?;

    // Determine the current environment markers.
    let tags = venv.interpreter().tags()?;

    // Prep the registry client.
    let client = RegistryClientBuilder::new(cache.clone())
        .index_urls(index_urls.clone())
        .build();

    // Prep the build context.
    let build_dispatch = BuildDispatch::new(
        &client,
        &cache,
        venv.interpreter(),
        &index_urls,
        venv.python_executable(),
        setup_py,
        no_build,
    );

    // Determine the set of installed packages.
    let site_packages =
        SitePackages::from_executable(&venv).context("Failed to list installed packages")?;

    // Resolve any editables.
    let resolved_editables = resolve_editables(
        editables,
        &site_packages,
        reinstall,
        &venv,
        tags,
        &cache,
        &client,
        &build_dispatch,
        printer,
    )
    .await?;

    // Partition into those that should be linked from the cache (`local`), those that need to be
    // downloaded (`remote`), and those that should be removed (`extraneous`).
    let InstallPlan {
        local,
        remote,
        reinstalls,
        extraneous,
    } = InstallPlan::from_requirements(
        &requirements,
        resolved_editables.editables,
        site_packages,
        reinstall,
        &index_urls,
        &cache,
        &venv,
        tags,
    )
    .context("Failed to determine installation plan")?;

    // Nothing to do.
    if remote.is_empty() && local.is_empty() && reinstalls.is_empty() && extraneous.is_empty() {
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

    // Instantiate a client.
    let client = RegistryClientBuilder::new(cache.clone())
        .index_urls(index_urls.clone())
        .build();

    // XXX
    let wheel_finder = puffin_resolver::DistFinder::new(tags, &client, venv.interpreter())
        .with_reporter(FinderReporter::from(printer).with_length(remote.len() as u64));
    let downloader = Downloader::new(&cache, tags, &client, &build_dispatch)
        .with_reporter(DownloadReporter::from(printer).with_length(remote.len() as u64));
    let wheel_finder = Box::leak(Box::new(wheel_finder));
    let downloader = Box::leak(Box::new(downloader));

    let in_flight = OnceMap::default();
    {
        let wheel_finder = &wheel_finder;
        let downloader = &downloader;
        let in_flight = &in_flight;
        let remote = Box::leak(Box::new(remote));
        use futures::StreamExt;
        let _res: Vec<CachedDist> = futures::stream::iter(remote)
            .map(|req| async move {
                let (_, dist) = wheel_finder.resolve_requirement(&req).await.unwrap();
                let wheel = downloader.get_wheel(dist, &in_flight).await.unwrap();
                wheel
            })
            .buffer_unordered(50)
            .collect()
            .await;
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
    venv: &Virtualenv,
    tags: &Tags,
    cache: &Cache,
    client: &RegistryClient,
    build_dispatch: &BuildDispatch<'_>,
    mut printer: Printer,
) -> Result<ResolvedEditables> {
    // Partition the editables into those that are already installed, and those that must be built.
    let mut installed = Vec::with_capacity(editables.len());
    let mut uninstalled = Vec::with_capacity(editables.len());
    for editable in editables {
        match reinstall {
            Reinstall::None => {
                if let Some(dist) = site_packages.get_editable(editable.raw()) {
                    installed.push(dist.clone());
                } else {
                    uninstalled.push(editable);
                }
            }
            Reinstall::All => {
                uninstalled.push(editable);
            }
            Reinstall::Packages(packages) => {
                if let Some(dist) = site_packages.get_editable(editable.raw()) {
                    if packages.contains(dist.name()) {
                        uninstalled.push(editable);
                    } else {
                        installed.push(dist.clone());
                    }
                } else {
                    uninstalled.push(editable);
                }
            }
        }
    }

    // Build any editable installs.
    let (built_editables, temp_dir) = if uninstalled.is_empty() {
        (Vec::new(), None)
    } else {
        let start = std::time::Instant::now();

        let temp_dir = tempfile::tempdir_in(venv.root())?;

        let downloader = Downloader::new(cache, tags, client, build_dispatch)
            .with_reporter(DownloadReporter::from(printer).with_length(uninstalled.len() as u64));

        let local_editables: Vec<LocalEditable> = uninstalled
            .iter()
            .map(|editable| match editable {
                EditableRequirement::Path { path, .. } => Ok(LocalEditable {
                    path: path.clone(),
                    requirement: editable.clone(),
                }),
                EditableRequirement::Url { path, .. } => Ok(LocalEditable {
                    path: path.clone(),
                    requirement: editable.clone(),
                }),
            })
            .collect::<Result<_>>()?;

        let built_editables: Vec<_> = downloader
            .build_editables(local_editables, temp_dir.path())
            .await
            .context("Failed to build editables")?
            .into_iter()
            .collect();

        let s = if built_editables.len() == 1 { "" } else { "s" };
        writeln!(
            printer,
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
