use std::fmt::Write;
use std::path::Path;

use anyhow::{bail, Context, Result};
use bitflags::bitflags;
use itertools::{Either, Itertools};
use owo_colors::OwoColorize;
use pep508_rs::Requirement;
use tracing::debug;

use platform_host::Platform;
use platform_tags::Tags;
use puffin_client::PypiClientBuilder;
use puffin_installer::{LocalDistribution, LocalIndex, RemoteDistribution};
use puffin_interpreter::{PythonExecutable, SitePackages};
use puffin_package::package_name::PackageName;
use puffin_package::requirements_txt::RequirementsTxt;
use puffin_resolver::Resolution;

use crate::commands::reporters::{
    DownloadReporter, InstallReporter, UnzipReporter, WheelFinderReporter,
};
use crate::commands::{elapsed, ExitStatus};
use crate::printer::Printer;

bitflags! {
    #[derive(Debug, Copy, Clone, Default)]
    pub struct PipSyncFlags: u8 {
        /// Ignore any installed packages, forcing a re-installation.
        const IGNORE_INSTALLED = 1 << 0;
    }
}

/// Install a set of locked requirements into the current Python environment.
pub(crate) async fn pip_sync(
    src: &Path,
    cache: Option<&Path>,
    flags: PipSyncFlags,
    mut printer: Printer,
) -> Result<ExitStatus> {
    // Read the `requirements.txt` from disk.
    let requirements_txt = RequirementsTxt::parse(src, std::env::current_dir()?)?;
    if !requirements_txt.constraints.is_empty() {
        bail!("Constraints in requirements.txt are not supported");
    }
    let requirements = requirements_txt
        .requirements
        .into_iter()
        .map(|entry| entry.requirement)
        .collect::<Vec<_>>();
    if requirements.is_empty() {
        writeln!(printer, "No requirements found")?;
        return Ok(ExitStatus::Success);
    }

    sync_requirements(&requirements, cache, flags, printer).await
}

/// Install a set of locked requirements into the current Python environment.
pub(crate) async fn sync_requirements(
    requirements: &[Requirement],
    cache: Option<&Path>,
    flags: PipSyncFlags,
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

    // Determine the current environment markers.
    let tags = Tags::from_env(python.platform(), python.simple_version())?;

    // Filter out any already-installed or already-cached packages.
    let (cached, uncached) =
        find_uncached_requirements(requirements, cache, flags, &python).await?;

    // Nothing to do.
    if uncached.is_empty() && cached.is_empty() {
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

    let client = PypiClientBuilder::default().cache(cache).build();

    // Resolve the dependencies.
    let resolution = if uncached.is_empty() {
        Resolution::default()
    } else {
        let start = std::time::Instant::now();

        let wheel_finder = puffin_resolver::WheelFinder::new(&tags, &client)
            .with_reporter(WheelFinderReporter::from(printer).with_length(uncached.len() as u64));
        let resolution = wheel_finder.resolve(&uncached).await?;

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

    let uncached = resolution
        .into_files()
        .map(RemoteDistribution::from_file)
        .collect::<Result<Vec<_>>>()?;
    let staging = tempfile::tempdir()?;

    // Download any missing distributions.
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

    // Install the resolved distributions.
    let start = std::time::Instant::now();
    let wheels = unzips.into_iter().chain(cached).collect::<Vec<_>>();
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

    for wheel in wheels {
        writeln!(
            printer,
            " {} {}{}",
            "+".green(),
            wheel.name().as_ref().white().bold(),
            format!("@{}", wheel.version()).dimmed()
        )?;
    }

    Ok(ExitStatus::Success)
}

async fn find_uncached_requirements(
    requirements: &[Requirement],
    cache: Option<&Path>,
    flags: PipSyncFlags,
    python: &PythonExecutable,
) -> Result<(Vec<LocalDistribution>, Vec<Requirement>)> {
    // Index all the already-installed packages in site-packages.
    let site_packages = if flags.intersects(PipSyncFlags::IGNORE_INSTALLED) {
        SitePackages::default()
    } else {
        SitePackages::from_executable(python).await?
    };

    // Index all the already-downloaded wheels in the cache.
    let local_index = if let Some(cache) = cache {
        LocalIndex::from_directory(cache).await?
    } else {
        LocalIndex::default()
    };

    Ok(split_uncached_requirements(
        requirements,
        &site_packages,
        &local_index,
    ))
}

fn split_uncached_requirements(
    requirements: &[Requirement],
    site_packages: &SitePackages,
    local_index: &LocalIndex,
) -> (Vec<LocalDistribution>, Vec<Requirement>) {
    requirements
        .iter()
        .filter(|requirement| {
            let package = PackageName::normalize(&requirement.name);

            // Filter out already-installed packages.
            if let Some(dist_info) = site_packages.get(&package) {
                debug!(
                    "Requirement already satisfied: {} ({})",
                    package,
                    dist_info.version()
                );
                false
            } else {
                true
            }
        })
        .partition_map(|requirement| {
            let package = PackageName::normalize(&requirement.name);

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
                Either::Left(distribution.clone())
            } else {
                debug!("Identified uncached requirement: {}", requirement);
                Either::Right(requirement.clone())
            }
        })
}
