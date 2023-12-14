use std::fmt::Write;

use anyhow::{bail, Context, Result};
use colored::Colorize;
use fs_err as fs;
use itertools::Itertools;
use tempfile::tempdir_in;
use tracing::debug;

use distribution_types::{AnyDist, CachedDist, LocalEditable, Metadata};
use install_wheel_rs::linker::LinkMode;
use pep508_rs::Requirement;
use platform_host::Platform;
use platform_tags::Tags;
use puffin_cache::Cache;
use puffin_client::RegistryClientBuilder;
use puffin_dispatch::BuildDispatch;
use puffin_installer::{Downloader, InstallPlan, Reinstall, SitePackages};
use puffin_interpreter::Virtualenv;
use puffin_traits::OnceMap;
use pypi_types::{IndexUrls, Yanked};
use requirements_txt::EditableRequirement;

use crate::commands::reporters::{DownloadReporter, FinderReporter, InstallReporter};
use crate::commands::{elapsed, ChangeEvent, ChangeEventKind, ExitStatus};
use crate::printer::Printer;
use crate::requirements::{RequirementsSource, RequirementsSpecification};

/// Install a set of locked requirements into the current Python environment.
pub(crate) async fn pip_sync(
    sources: &[RequirementsSource],
    reinstall: &Reinstall,
    link_mode: LinkMode,
    index_urls: IndexUrls,
    no_build: bool,
    cache: Cache,
    mut printer: Printer,
) -> Result<ExitStatus> {
    // Read all requirements from the provided sources.
    let (requirements, editables) = RequirementsSpecification::requirements_and_editables(sources)?;

    if requirements.is_empty() && editables.is_empty() {
        writeln!(printer, "No requirements found")?;
        return Ok(ExitStatus::Success);
    }

    sync_requirements(
        &requirements,
        reinstall,
        &editables,
        link_mode,
        index_urls,
        no_build,
        &cache,
        printer,
    )
    .await
}

/// Install a set of locked requirements into the current Python environment.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn sync_requirements(
    requirements: &[Requirement],
    reinstall: &Reinstall,
    editables: &[EditableRequirement],
    link_mode: LinkMode,
    index_urls: IndexUrls,
    no_build: bool,
    cache: &Cache,
    mut printer: Printer,
) -> Result<ExitStatus> {
    let start = std::time::Instant::now();

    // Detect the current Python interpreter.
    let platform = Platform::current()?;
    let venv = Virtualenv::from_env(platform, cache)?;
    debug!(
        "Using Python interpreter: {}",
        venv.python_executable().display()
    );

    // Determine the current environment markers.
    let tags = Tags::from_interpreter(venv.interpreter())?;

    // Partition into those that should be linked from the cache (`local`), those that need to be
    // downloaded (`remote`), and those that should be removed (`extraneous`).
    let InstallPlan {
        local,
        editables,
        remote,
        reinstalls,
        extraneous,
    } = InstallPlan::from_requirements(
        requirements,
        reinstall,
        editables,
        &index_urls,
        cache,
        &venv,
        &tags,
    )
    .context("Failed to determine installation plan")?;

    // Nothing to do.
    if remote.is_empty()
        && local.is_empty()
        && reinstalls.is_empty()
        && extraneous.is_empty()
        && editables.is_empty()
    {
        let s = if requirements.len() == 1 { "" } else { "s" };
        writeln!(
            printer,
            "{}",
            format!(
                "Audited {} in {}",
                format!("{} package{}", requirements.len(), s).bold(),
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

    // Resolve any registry-based requirements.
    let remote = if remote.is_empty() {
        Vec::new()
    } else {
        let start = std::time::Instant::now();

        let wheel_finder = puffin_resolver::DistFinder::new(&tags, &client, venv.interpreter())
            .with_reporter(FinderReporter::from(printer).with_length(remote.len() as u64));
        let resolution = wheel_finder.resolve(&remote).await?;

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

        resolution.into_distributions().collect::<Vec<_>>()
    };

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
                    "{}{} {dist} is yanked. Refresh your lockfile to pin an un-yanked version.",
                    "warning".yellow().bold(),
                    ":".bold(),
                )?;
            }
            Some(Yanked::Reason(reason)) => {
                writeln!(
                    printer,
                    "{}{} {dist} is yanked (reason: \"{reason}\"). Refresh your lockfile to pin an un-yanked version.",
                    "warning".yellow().bold(),
                    ":".bold(),
                )?;
            }
        }
    }

    let build_dispatch = BuildDispatch::new(
        client.clone(),
        cache.clone(),
        venv.interpreter().clone(),
        fs::canonicalize(venv.python_executable())?,
        no_build,
        index_urls.clone(),
    );
    let downloader = Downloader::new(cache, &tags, &client, &build_dispatch).with_reporter(
        DownloadReporter::from(printer).with_length((editables.len() + remote.len()) as u64),
    );

    // We must not cache editable wheels, so we put them in a temp dir.
    let editable_wheel_dir = tempdir_in(venv.root())?;
    let built_editables = if editables.is_empty() {
        Vec::new()
    } else {
        let start = std::time::Instant::now();

        let editables: Vec<LocalEditable> = editables
            .into_iter()
            .map(|editable| match editable.clone() {
                EditableRequirement::Path {
                    resolved,
                    original: _,
                } => Ok(LocalEditable {
                    requirement: editable,
                    path: resolved,
                }),
                EditableRequirement::Url(_) => {
                    bail!("url editables are not supported yet");
                }
            })
            .collect::<Result<_>>()?;

        let built_editables: Vec<CachedDist> = downloader
            .build_editables(editables, editable_wheel_dir.path())
            .await
            .context("Failed to build editables")?
            .into_iter()
            .map(|built_editable| built_editable.wheel)
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

        built_editables
    };

    // Download, build, and unzip any missing distributions.
    let wheels = if remote.is_empty() {
        Vec::new()
    } else {
        let start = std::time::Instant::now();

        let wheels = downloader
            .download(remote, &OnceMap::default())
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

    // Remove any unnecessary packages.
    if !extraneous.is_empty() || !reinstalls.is_empty() {
        let start = std::time::Instant::now();

        for dist_info in extraneous.iter().chain(reinstalls.iter()) {
            let summary = puffin_installer::uninstall(dist_info).await?;
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
            printer,
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
    let wheels = wheels
        .into_iter()
        .chain(local)
        .chain(built_editables)
        .collect::<Vec<_>>();
    if !wheels.is_empty() {
        let start = std::time::Instant::now();
        puffin_installer::Installer::new(&venv)
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

    // Report on any changes in the environment.
    for event in extraneous
        .into_iter()
        .chain(reinstalls.into_iter())
        .map(|distribution| ChangeEvent {
            dist: AnyDist::from(distribution),
            kind: ChangeEventKind::Removed,
        })
        .chain(wheels.into_iter().map(|distribution| ChangeEvent {
            dist: AnyDist::from(distribution),
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
                    event.dist.version_or_url().to_string().dimmed()
                )?;
            }
            ChangeEventKind::Removed => {
                writeln!(
                    printer,
                    " {} {}{}",
                    "-".red(),
                    event.dist.name().as_ref().white().bold(),
                    event.dist.version_or_url().to_string().dimmed()
                )?;
            }
        }
    }

    // Validate that the environment is consistent.
    let site_packages = SitePackages::from_executable(&venv)?;
    for diagnostic in site_packages.diagnostics()? {
        writeln!(
            printer,
            "{}{} {}",
            "warning".yellow().bold(),
            ":".bold(),
            diagnostic.message().bold()
        )?;
    }

    Ok(ExitStatus::Success)
}
