use std::fmt::Write;

use anyhow::{Context, Result};
use colored::Colorize;
use fs_err as fs;
use itertools::Itertools;
use tracing::debug;

use distribution_types::{AnyDist, Metadata};
use install_wheel_rs::linker::LinkMode;
use pep508_rs::Requirement;
use platform_host::Platform;
use platform_tags::Tags;
use puffin_cache::Cache;
use puffin_client::RegistryClientBuilder;
use puffin_dispatch::BuildDispatch;
use puffin_distribution::DistributionDatabase;
use puffin_installer::InstallPlan;
use puffin_interpreter::Virtualenv;
use pypi_types::{IndexUrls, Yanked};

use crate::commands::reporters::{FetcherReporter, FinderReporter, InstallReporter, UnzipReporter};
use crate::commands::{elapsed, ExitStatus};
use crate::printer::Printer;
use crate::requirements::{ExtrasSpecification, RequirementsSource, RequirementsSpecification};

/// Install a set of locked requirements into the current Python environment.
pub(crate) async fn pip_sync(
    sources: &[RequirementsSource],
    link_mode: LinkMode,
    index_urls: IndexUrls,
    no_build: bool,
    cache: Cache,
    mut printer: Printer,
) -> Result<ExitStatus> {
    // Read all requirements from the provided sources.
    let RequirementsSpecification {
        project: _,
        requirements,
        constraints: _,
        extras: _,
    } = RequirementsSpecification::try_from_sources(sources, &[], &ExtrasSpecification::None)?;

    if requirements.is_empty() {
        writeln!(printer, "No requirements found")?;
        return Ok(ExitStatus::Success);
    }

    sync_requirements(
        &requirements,
        link_mode,
        index_urls,
        no_build,
        &cache,
        printer,
    )
    .await
}

/// Install a set of locked requirements into the current Python environment.
pub(crate) async fn sync_requirements(
    requirements: &[Requirement],
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
        remote,
        extraneous,
    } = InstallPlan::try_from_requirements(requirements, &index_urls, cache, &venv, &tags)
        .context("Failed to determine installation plan")?;

    // Nothing to do.
    if remote.is_empty() && local.is_empty() && extraneous.is_empty() {
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

    // Download any missing distributions.
    let wheels = if remote.is_empty() {
        vec![]
    } else {
        let start = std::time::Instant::now();

        let build_dispatch = BuildDispatch::new(
            client.clone(),
            cache.clone(),
            venv.interpreter().clone(),
            fs::canonicalize(venv.python_executable())?,
            no_build,
            index_urls.clone(),
        );

        let distribution_database =
            DistributionDatabase::new(cache, &tags, &client, &build_dispatch)
                .with_reporter(FetcherReporter::from(printer).with_length(remote.len() as u64));

        let wheels = distribution_database
            .get_wheels(remote)
            .await
            .context("Failed to download wheels and download and build distributions")?;

        let download_s = if wheels.len() == 1 { "" } else { "s" };
        writeln!(
            printer,
            "{}",
            format!(
                "Downloaded {} in {}",
                format!("{} package{}", wheels.len(), download_s).bold(),
                elapsed(start.elapsed())
            )
            .dimmed()
        )?;

        wheels
    };

    // Unzip any downloaded distributions.
    let unzips = if wheels.is_empty() {
        vec![]
    } else {
        let start = std::time::Instant::now();

        let unzipper = puffin_installer::Unzipper::default()
            .with_reporter(UnzipReporter::from(printer).with_length(wheels.len() as u64));

        let unzips = unzipper
            .unzip(wheels)
            .await
            .context("Failed to unpack wheels")?;

        let s = if unzips.len() == 1 { "" } else { "s" };
        writeln!(
            printer,
            "{}",
            format!(
                "Unzipped {} in {}",
                format!("{} package{}", unzips.len(), s).bold(),
                elapsed(start.elapsed())
            )
            .dimmed()
        )?;

        unzips
    };

    // Remove any unnecessary packages.
    if !extraneous.is_empty() {
        let start = std::time::Instant::now();

        for dist_info in &extraneous {
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

        let s = if extraneous.len() == 1 { "" } else { "s" };
        writeln!(
            printer,
            "{}",
            format!(
                "Uninstalled {} in {}",
                format!("{} package{}", extraneous.len(), s).bold(),
                elapsed(start.elapsed())
            )
            .dimmed()
        )?;
    }

    // Install the resolved distributions.
    let wheels = unzips.into_iter().chain(local).collect::<Vec<_>>();
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

    for event in extraneous
        .into_iter()
        .map(|distribution| ChangeEvent {
            dist: AnyDist::from(distribution),
            kind: ChangeEventKind::Remove,
        })
        .chain(wheels.into_iter().map(|distribution| ChangeEvent {
            dist: AnyDist::from(distribution),
            kind: ChangeEventKind::Add,
        }))
        .sorted_unstable_by_key(|event| event.dist.name().clone())
    {
        match event.kind {
            ChangeEventKind::Add => {
                writeln!(
                    printer,
                    " {} {}{}",
                    "+".green(),
                    event.dist.name().as_ref().white().bold(),
                    event.dist.version_or_url().to_string().dimmed()
                )?;
            }
            ChangeEventKind::Remove => {
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

    Ok(ExitStatus::Success)
}

#[derive(Debug)]
enum ChangeEventKind {
    /// The package was added to the environment.
    Add,
    /// The package was removed from the environment.
    Remove,
}

#[derive(Debug)]
struct ChangeEvent {
    dist: AnyDist,
    kind: ChangeEventKind,
}
