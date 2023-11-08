use std::path::Path;

use anyhow::{Context, Result};
use tracing::debug;

use pep508_rs::{Requirement, VersionOrUrl};
use puffin_distribution::direct_url::DirectUrl;
use puffin_distribution::{CachedDistribution, InstalledDistribution};
use puffin_interpreter::Virtualenv;

use crate::url_index::UrlIndex;
use crate::{RegistryIndex, SitePackages};

#[derive(Debug, Default)]
pub struct InstallPlan {
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

impl InstallPlan {
    /// Partition a set of requirements into those that should be linked from the cache, those that
    /// need to be downloaded, and those that should be removed.
    pub fn try_from_requirements(
        requirements: &[Requirement],
        cache: &Path,
        venv: &Virtualenv,
    ) -> Result<Self> {
        // Index all the already-installed packages in site-packages.
        let mut site_packages =
            SitePackages::try_from_executable(venv).context("Failed to list installed packages")?;

        // Index all the already-downloaded wheels in the cache.
        let registry_index = RegistryIndex::try_from_directory(cache);
        let url_index = UrlIndex::try_from_directory(cache);

        let mut local = vec![];
        let mut remote = vec![];
        let mut extraneous = vec![];

        for requirement in requirements {
            // Filter out already-installed packages.
            if let Some(distribution) = site_packages.remove(&requirement.name) {
                // We need to map here from the requirement to its DirectUrl, then see if that DirectUrl
                // is anywhere in `site_packages`.
                match requirement.version_or_url.as_ref() {
                    // If the requirement comes from a registry, check by name.
                    None | Some(VersionOrUrl::VersionSpecifier(_)) => {
                        if requirement.is_satisfied_by(distribution.version()) {
                            debug!("Requirement already satisfied: {distribution}");
                            continue;
                        }
                    }

                    // If the requirement comes from a direct URL, check by URL.
                    Some(VersionOrUrl::Url(url)) => {
                        if let InstalledDistribution::Url(distribution) = &distribution {
                            if let Ok(direct_url) = DirectUrl::try_from(url) {
                                if let Ok(direct_url) = pypi_types::DirectUrl::try_from(&direct_url)
                                {
                                    // TODO(charlie): These don't need to be strictly equal. We only care
                                    // about a subset of the fields.
                                    if direct_url == distribution.url {
                                        debug!("Requirement already satisfied: {distribution}");
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                }

                extraneous.push(distribution);
            }

            // Identify any locally-available distributions that satisfy the requirement.
            match requirement.version_or_url.as_ref() {
                None | Some(VersionOrUrl::VersionSpecifier(_)) => {
                    if let Some(distribution) = registry_index
                        .get(&requirement.name)
                        .filter(|dist| requirement.is_satisfied_by(&dist.version))
                    {
                        debug!("Requirement already cached: {distribution}");
                        local.push(CachedDistribution::Registry(distribution.clone()));
                        continue;
                    }
                }
                Some(VersionOrUrl::Url(url)) => {
                    if let Some(distribution) = url_index.get(&requirement.name, url) {
                        debug!("Requirement already cached: {distribution}");
                        local.push(CachedDistribution::Url(distribution.clone()));
                        continue;
                    }
                }
            }

            debug!("Identified uncached requirement: {requirement}");
            remote.push(requirement.clone());
        }

        // Remove any unnecessary packages.
        for (_package, dist_info) in site_packages {
            debug!("Unnecessary package: {dist_info}");
            extraneous.push(dist_info);
        }

        Ok(InstallPlan {
            local,
            remote,
            extraneous,
        })
    }
}
