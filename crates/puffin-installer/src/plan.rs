use std::path::Path;

use anyhow::Result;
use tracing::debug;

use pep508_rs::Requirement;
use puffin_interpreter::PythonExecutable;
use puffin_package::package_name::PackageName;

use crate::{CachedDistribution, InstalledDistribution, LocalIndex, SitePackages};

#[derive(Debug, Default)]
pub struct PartitionedRequirements {
    /// The distributions that are not already installed in the current environment, but are
    /// available in the local cache.
    pub local: Vec<CachedDistribution>,

    /// The distributions that are not already installed in the current environment, and are
    /// not available in the local cache.
    pub remote: Vec<Requirement>,

    /// The distributions that are already installed in the current environment, and are
    /// _not_ necessary to satisfy the requirements.
    pub extraneous: Vec<InstalledDistribution>,
}

impl PartitionedRequirements {
    /// Partition a set of requirements into those that should be linked from the cache, those that
    /// need to be downloaded, and those that should be removed.
    pub fn try_from_requirements(
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
