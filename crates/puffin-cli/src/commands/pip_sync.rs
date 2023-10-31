use std::fmt::Write;
use std::path::Path;

use anyhow::{Context, Result};
use colored::Colorize;
use itertools::Itertools;
use tracing::debug;

use install_wheel_rs::linker::LinkMode;
use pep508_rs::Requirement;
use platform_host::Platform;
use platform_tags::Tags;
use puffin_client::RegistryClientBuilder;
use puffin_distribution::Distribution;
use puffin_installer::PartitionedRequirements;
use puffin_interpreter::Virtualenv;

use crate::commands::reporters::{
    DownloadReporter, InstallReporter, UnzipReporter, WheelFinderReporter,
};
use crate::commands::{elapsed, ExitStatus};
use crate::index_urls::IndexUrls;
use crate::printer::Printer;
use crate::requirements::{RequirementsSource, RequirementsSpecification};

/// Install a set of locked requirements into the current Python environment.
pub(crate) async fn pip_sync(
    sources: &[RequirementsSource],
    link_mode: LinkMode,
    index_urls: Option<IndexUrls>,
    cache: Option<&Path>,
    mut printer: Printer,
) -> Result<ExitStatus> {
    // Read all requirements from the provided sources.
    let RequirementsSpecification {
        requirements,
        constraints: _,
    } = RequirementsSpecification::try_from_sources(sources, &[], &[])?;

    if requirements.is_empty() {
        writeln!(printer, "No requirements found")?;
        return Ok(ExitStatus::Success);
    }

    sync_requirements(&requirements, link_mode, index_urls, cache, printer).await
}

/// Install a set of locked requirements into the current Python environment.
pub(crate) async fn sync_requirements(
    requirements: &[Requirement],
    link_mode: LinkMode,
    index_urls: Option<IndexUrls>,
    cache: Option<&Path>,
    mut printer: Printer,
) -> Result<ExitStatus> {
    // Audit the requirements.
    let start = std::time::Instant::now();

    // Detect the current Python interpreter.
    let platform = Platform::current()?;
    let venv = Virtualenv::from_env(platform, cache)?;
    debug!(
        "Using Python interpreter: {}",
        venv.python_executable().display()
    );

    // Partition into those that should be linked from the cache (`local`), those that need to be
    // downloaded (`remote`), and those that should be removed (`extraneous`).
    let PartitionedRequirements {
        local,
        remote,
        extraneous,
    } = PartitionedRequirements::try_from_requirements(requirements, cache, &venv)?;

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

    // Determine the current environment markers.
    let tags = Tags::from_env(
        venv.interpreter_info().platform(),
        venv.interpreter_info().simple_version(),
    )?;

    // Instantiate a client.
    let client = {
        let mut builder = RegistryClientBuilder::default();
        builder = builder.cache(cache);
        if let Some(IndexUrls { index, extra_index }) = index_urls {
            if let Some(index) = index {
                builder = builder.index(index);
            }
            builder = builder.extra_index(extra_index);
        } else {
            builder = builder.no_index();
        }
        builder.build()
    };

    // Resolve the dependencies.
    let remote = if remote.is_empty() {
        Vec::new()
    } else {
        let start = std::time::Instant::now();

        let wheel_finder = puffin_resolver::WheelFinder::new(&tags, &client)
            .with_reporter(WheelFinderReporter::from(printer).with_length(remote.len() as u64));
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

    // Download any missing distributions.
    let downloads = if remote.is_empty() {
        vec![]
    } else {
        let start = std::time::Instant::now();

        let downloader = puffin_installer::Downloader::new(&client, cache)
            .with_reporter(DownloadReporter::from(printer).with_length(remote.len() as u64));

        let downloads = downloader.download(remote).await?;

        let s = if downloads.len() == 1 { "" } else { "s" };
        writeln!(
            printer,
            "{}",
            format!(
                "Downloaded {} in {}",
                format!("{} package{}", downloads.len(), s).bold(),
                elapsed(start.elapsed())
            )
            .dimmed()
        )?;

        downloads
    };

    // Unzip any downloaded distributions.
    let staging = tempfile::tempdir()?;
    let unzips = if downloads.is_empty() {
        vec![]
    } else {
        let start = std::time::Instant::now();

        let unzipper = puffin_installer::Unzipper::default()
            .with_reporter(UnzipReporter::from(printer).with_length(downloads.len() as u64));

        let unzips = unzipper
            .unzip(downloads, cache.unwrap_or(staging.path()))
            .await
            .context("Failed to download and unpack wheels")?;

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
            distribution: Distribution::from(distribution),
            kind: ChangeEventKind::Remove,
        })
        .chain(wheels.into_iter().map(|distribution| ChangeEvent {
            distribution: Distribution::from(distribution),
            kind: ChangeEventKind::Add,
        }))
        .sorted_unstable_by_key(|event| event.distribution.name().clone())
    {
        match event.kind {
            ChangeEventKind::Add => {
                writeln!(
                    printer,
                    " {} {}{}",
                    "+".green(),
                    event.distribution.name().as_ref().white().bold(),
                    format!("@{}", event.distribution.version()).dimmed()
                )?;
            }
            ChangeEventKind::Remove => {
                writeln!(
                    printer,
                    " {} {}{}",
                    "-".red(),
                    event.distribution.name().as_ref().white().bold(),
                    format!("@{}", event.distribution.version()).dimmed()
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
    distribution: Distribution,
    kind: ChangeEventKind,
}
