use std::fmt::Write;
use std::path::Path;

use anyhow::{Context, Result};
use colored::Colorize;
use itertools::Itertools;
use tracing::debug;

use pep508_rs::Requirement;
use platform_host::Platform;
use platform_tags::Tags;
use puffin_client::PypiClientBuilder;
use puffin_installer::{
    CachedDistribution, Distribution, InstalledDistribution, LocalIndex, RemoteDistribution,
    SitePackages,
};
use puffin_interpreter::PythonExecutable;
use puffin_package::package_name::PackageName;
use puffin_resolver::Resolution;

use crate::commands::reporters::{
    DownloadReporter, InstallReporter, UnzipReporter, WheelFinderReporter,
};
use crate::commands::{elapsed, ExitStatus};
use crate::printer::Printer;
use crate::requirements::RequirementsSource;

/// Install a set of locked requirements into the current Python environment.
pub(crate) async fn pip_sync(
    sources: &[RequirementsSource],
    cache: Option<&Path>,
    mut printer: Printer,
) -> Result<ExitStatus> {
    // Read all requirements from the provided sources.
    let requirements = sources
        .iter()
        .map(RequirementsSource::requirements)
        .flatten_ok()
        .collect::<Result<Vec<Requirement>>>()?;

    if requirements.is_empty() {
        writeln!(printer, "No requirements found")?;
        return Ok(ExitStatus::Success);
    }

    sync_requirements(&requirements, cache, printer).await
}

/// Install a set of locked requirements into the current Python environment.
pub(crate) async fn sync_requirements(
    requirements: &[Requirement],
    cache: Option<&Path>,
    mut printer: Printer,
) -> Result<ExitStatus> {
    // Audit the requirements.
    let start = std::time::Instant::now();

    // Detect the current Python interpreter.
    let platform = Platform::current()?;
    let python = PythonExecutable::from_env(platform, cache)?;
    debug!(
        "Using Python interpreter: {}",
        python.executable().display()
    );

    // Partition into those that should be linked from the cache (`local`), those that need to be
    // downloaded (`remote`), and those that should be removed (`extraneous`).
    let PartitionedRequirements {
        local,
        remote,
        extraneous,
    } = PartitionedRequirements::try_from_requirements(requirements, cache, &python)?;

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
    let tags = Tags::from_env(python.platform(), python.simple_version())?;
    let client = PypiClientBuilder::default().cache(cache).build();

    // Resolve the dependencies.
    let resolution = if remote.is_empty() {
        Resolution::default()
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

        resolution
    };

    // Download any missing distributions.
    let staging = tempfile::tempdir()?;
    let uncached = resolution
        .into_files()
        .map(RemoteDistribution::from_file)
        .collect::<Result<Vec<_>>>()?;
    let downloads = if uncached.is_empty() {
        vec![]
    } else {
        let start = std::time::Instant::now();

        let downloader = puffin_installer::Downloader::new(&client, cache)
            .with_reporter(DownloadReporter::from(printer).with_length(uncached.len() as u64));

        let downloads = downloader
            .download(&uncached, cache.unwrap_or(staging.path()))
            .await?;

        let s = if uncached.len() == 1 { "" } else { "s" };
        writeln!(
            printer,
            "{}",
            format!(
                "Downloaded {} in {}",
                format!("{} package{}", uncached.len(), s).bold(),
                elapsed(start.elapsed())
            )
            .dimmed()
        )?;

        downloads
    };

    // Unzip any downloaded distributions.
    let unzips = if downloads.is_empty() {
        vec![]
    } else {
        let start = std::time::Instant::now();

        let unzipper = puffin_installer::Unzipper::default()
            .with_reporter(UnzipReporter::from(printer).with_length(downloads.len() as u64));

        let unzips = unzipper
            .download(downloads, cache.unwrap_or(staging.path()))
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
        puffin_installer::Installer::new(&python)
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

#[derive(Debug, Default)]
struct PartitionedRequirements {
    /// The distributions that are not already installed in the current environment, but are
    /// available in the local cache.
    local: Vec<CachedDistribution>,

    /// The distributions that are not already installed in the current environment, and are
    /// not available in the local cache.
    remote: Vec<Requirement>,

    /// The distributions that are already installed in the current environment, and are
    /// _not_ necessary to satisfy the requirements.
    extraneous: Vec<InstalledDistribution>,
}

impl PartitionedRequirements {
    /// Partition a set of requirements into those that should be linked from the cache, those that
    /// need to be downloaded, and those that should be removed.
    pub(crate) fn try_from_requirements(
        requirements: &[Requirement],
        cache: Option<&Path>,
        python: &PythonExecutable,
    ) -> Result<Self> {
        // Index all the already-installed packages in site-packages.
        let mut site_packages = SitePackages::try_from_executable(python)?;

        // Index all the already-downloaded wheels in the cache.
        let local_index = if let Some(cache) = cache {
            LocalIndex::try_from_directory(cache)?
        } else {
            LocalIndex::default()
        };

        let mut local = vec![];
        let mut remote = vec![];
        let mut extraneous = vec![];

        for requirement in requirements {
            let package = PackageName::normalize(&requirement.name);

            // Filter out already-installed packages.
            if let Some(dist) = site_packages.remove(&package) {
                if requirement.is_satisfied_by(dist.version()) {
                    debug!(
                        "Requirement already satisfied: {} ({})",
                        package,
                        dist.version()
                    );
                    continue;
                }
                extraneous.push(dist);
            }

            // Identify any locally-available distributions that satisfy the requirement.
            if let Some(distribution) = local_index
                .get(&package)
                .filter(|dist| requirement.is_satisfied_by(dist.version()))
            {
                debug!(
                    "Requirement already cached: {} ({})",
                    distribution.name(),
                    distribution.version()
                );
                local.push(distribution.clone());
            } else {
                debug!("Identified uncached requirement: {}", requirement);
                remote.push(requirement.clone());
            }
        }

        // Remove any unnecessary packages.
        for (package, dist_info) in site_packages {
            debug!("Unnecessary package: {} ({})", package, dist_info.version());
            extraneous.push(dist_info);
        }

        Ok(PartitionedRequirements {
            local,
            remote,
            extraneous,
        })
    }
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
