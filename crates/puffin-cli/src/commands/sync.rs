use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use bitflags::bitflags;
use tracing::{debug, info};

use platform_host::Platform;
use platform_tags::Tags;
use puffin_client::PypiClientBuilder;
use puffin_installer::{Distribution, RemoteDistribution};
use puffin_interpreter::{PythonExecutable, SitePackages};
use puffin_package::package_name::PackageName;
use puffin_package::requirements::Requirements;

use crate::commands::{elapsed, ExitStatus};

bitflags! {
    #[derive(Debug, Copy, Clone, Default)]
    pub struct SyncFlags: u8 {
        /// Ignore any installed packages, forcing a re-installation.
        const IGNORE_INSTALLED = 1 << 0;
    }
}

/// Install a set of locked requirements into the current Python environment.
pub(crate) async fn sync(src: &Path, cache: Option<&Path>, flags: SyncFlags) -> Result<ExitStatus> {
    let start = std::time::Instant::now();

    // Read the `requirements.txt` from disk.
    let requirements_txt = std::fs::read_to_string(src)?;

    // Parse the `requirements.txt` into a list of requirements.
    let requirements = Requirements::from_str(&requirements_txt)?;
    let initial_requirements = requirements.len();

    // Detect the current Python interpreter.
    let platform = Platform::current()?;
    let python = PythonExecutable::from_env(platform, cache)?;
    debug!(
        "Using Python interpreter: {}",
        python.executable().display()
    );

    // Remove any already-installed packages.
    let requirements = if flags.intersects(SyncFlags::IGNORE_INSTALLED) {
        requirements
    } else {
        let site_packages = SitePackages::from_executable(&python).await?;
        requirements.filter(|requirement| {
            let package = PackageName::normalize(&requirement.name);
            if let Some(version) = site_packages.get(&package) {
                #[allow(clippy::print_stdout)]
                {
                    info!("Requirement already satisfied: {package} ({version})");
                }
                false
            } else {
                true
            }
        })
    };

    if requirements.is_empty() {
        let s = if initial_requirements == 1 { "" } else { "s" };
        info!(
            "Audited {} package{} in {}",
            initial_requirements,
            s,
            elapsed(start.elapsed())
        );
        return Ok(ExitStatus::Success);
    }

    // Detect any cached wheels.
    let (uncached, cached) = if let Some(cache) = cache {
        let mut cached = Vec::with_capacity(requirements.len());
        let mut uncached = Vec::with_capacity(requirements.len());

        let index = puffin_installer::LocalIndex::from_directory(cache).await?;
        for requirement in requirements {
            let package = PackageName::normalize(&requirement.name);
            if let Some(distribution) = index
                .get(&package)
                .filter(|dist| requirement.is_satisfied_by(dist.version()))
            {
                debug!(
                    "Requirement already cached: {} ({})",
                    distribution.name(),
                    distribution.version()
                );
                cached.push(distribution.clone());
            } else {
                debug!("Identified uncached requirement: {}", requirement);
                uncached.push(requirement);
            }
        }

        (Requirements::new(uncached), cached)
    } else {
        (requirements, Vec::new())
    };

    // Determine the current environment markers.
    let markers = python.markers();

    // Determine the compatible platform tags.
    let tags = Tags::from_env(python.platform(), python.simple_version())?;

    // Instantiate a client.
    let client = {
        let mut pypi_client = PypiClientBuilder::default();
        if let Some(cache) = cache {
            pypi_client = pypi_client.cache(cache);
        }
        pypi_client.build()
    };

    // Resolve the dependencies.
    let resolution = if uncached.is_empty() {
        puffin_resolver::Resolution::empty()
    } else {
        puffin_resolver::resolve(
            &uncached,
            markers,
            &tags,
            &client,
            puffin_resolver::ResolveFlags::NO_DEPS,
        )
        .await?
    };

    // Install into the current environment.
    let wheels = cached
        .into_iter()
        .map(|local| Ok(Distribution::Local(local)))
        .chain(
            resolution
                .into_files()
                .map(|file| Ok(Distribution::Remote(RemoteDistribution::from_file(file)?))),
        )
        .collect::<Result<Vec<_>>>()?;
    puffin_installer::install(&wheels, &python, &client, cache).await?;

    let s = if wheels.len() == 1 { "" } else { "s" };
    info!(
        "Installed {} package{} in {}",
        wheels.len(),
        s,
        elapsed(start.elapsed())
    );

    Ok(ExitStatus::Success)
}
