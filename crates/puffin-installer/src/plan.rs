use std::hash::BuildHasherDefault;

use anyhow::{bail, Result};
use rustc_hash::FxHashSet;
use tracing::{debug, warn};

use distribution_types::direct_url::git_reference;
use distribution_types::{BuiltDist, Dist, Name, SourceDist};
use distribution_types::{CachedDirectUrlDist, CachedDist, InstalledDist};
use pep508_rs::{Requirement, VersionOrUrl};
use platform_tags::Tags;
use puffin_cache::{Cache, CacheBucket, WheelCache};
use puffin_distribution::{BuiltWheelIndex, RegistryWheelIndex};
use puffin_interpreter::Virtualenv;
use puffin_normalize::PackageName;
use pypi_types::IndexUrls;

use crate::{ResolvedEditable, SitePackages};

#[derive(Debug, Default)]
pub struct InstallPlan {
    /// The distributions that are not already installed in the current environment, but are
    /// available in the local cache.
    pub local: Vec<CachedDist>,

    /// The distributions that are not already installed in the current environment, and are
    /// not available in the local cache.
    pub remote: Vec<Requirement>,

    /// Any distributions that are already installed in the current environment, but will be
    /// re-installed (including upgraded) to satisfy the requirements.
    pub reinstalls: Vec<InstalledDist>,

    /// Any distributions that are already installed in the current environment, and are
    /// _not_ necessary to satisfy the requirements.
    pub extraneous: Vec<InstalledDist>,
}

impl InstallPlan {
    /// Partition a set of requirements into those that should be linked from the cache, those that
    /// need to be downloaded, and those that should be removed.
    #[allow(clippy::too_many_arguments)]
    pub fn from_requirements(
        requirements: &[Requirement],
        editable_requirements: Vec<ResolvedEditable>,
        mut site_packages: SitePackages,
        reinstall: &Reinstall,
        index_urls: &IndexUrls,
        cache: &Cache,
        venv: &Virtualenv,
        tags: &Tags,
    ) -> Result<Self> {
        // Index all the already-downloaded wheels in the cache.
        let mut registry_index = RegistryWheelIndex::new(cache, tags, index_urls);

        let mut local = vec![];
        let mut remote = vec![];
        let mut reinstalls = vec![];
        let mut extraneous = vec![];
        let mut seen =
            FxHashSet::with_capacity_and_hasher(requirements.len(), BuildHasherDefault::default());

        // Remove any editable requirements.
        for requirement in editable_requirements {
            // If we see the same requirement twice, then we have a conflict.
            if !seen.insert(requirement.name().clone()) {
                bail!(
                    "Detected duplicate package in requirements: {}",
                    requirement.name()
                );
            }

            match requirement {
                ResolvedEditable::Installed(installed) => {
                    debug!("Treating editable requirement as immutable: {installed}");

                    // Remove from the site-packages index, to avoid marking as extraneous.
                    let Some(editable) = installed.as_editable() else {
                        warn!("Editable requirement is not editable: {installed}");
                        continue;
                    };
                    if site_packages.remove_editable(editable).is_none() {
                        warn!("Editable requirement is not installed: {installed}");
                        continue;
                    }
                }
                ResolvedEditable::Built(built) => {
                    debug!("Treating editable requirement as mutable: {built}");

                    if let Some(dist) = site_packages.remove_editable(built.editable.raw()) {
                        // Remove any editable installs.
                        reinstalls.push(dist);
                    } else if let Some(dist) = site_packages.remove(built.name()) {
                        // Remove any non-editable installs of the same package.
                        reinstalls.push(dist);
                    }
                    local.push(built.wheel);
                }
            }
        }

        for requirement in requirements {
            // Filter out incompatible requirements.
            if !requirement.evaluate_markers(venv.interpreter().markers(), &[]) {
                continue;
            }

            // If we see the same requirement twice, then we have a conflict.
            if !seen.insert(requirement.name.clone()) {
                bail!(
                    "Detected duplicate package in requirements: {}",
                    requirement.name
                );
            }

            // Check if the package should be reinstalled. A reinstall involves (1) purging any
            // cached distributions, and (2) marking any installed distributions as extraneous.
            let reinstall = match reinstall {
                Reinstall::None => false,
                Reinstall::All => true,
                Reinstall::Packages(packages) => packages.contains(&requirement.name),
            };

            if reinstall {
                // If necessary, purge the cached distributions.
                debug!("Purging cached distributions for: {requirement}");
                cache.purge(&requirement.name)?;
                if let Some(distribution) = site_packages.remove(&requirement.name) {
                    reinstalls.push(distribution);
                }
            } else {
                if let Some(distribution) = site_packages.remove(&requirement.name) {
                    // Filter out already-installed packages.
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
                            if let InstalledDist::Url(distribution) = &distribution {
                                if &distribution.url == url.raw() {
                                    debug!("Requirement already satisfied: {distribution}");
                                    continue;
                                }
                            }
                        }
                    }

                    reinstalls.push(distribution);
                }
            }

            // Identify any locally-available distributions that satisfy the requirement.
            match requirement.version_or_url.as_ref() {
                None => {
                    if let Some((_version, distribution)) =
                        registry_index.get(&requirement.name).next()
                    {
                        debug!("Requirement already cached: {distribution}");
                        local.push(CachedDist::Registry(distribution.clone()));
                        continue;
                    }
                }
                Some(VersionOrUrl::VersionSpecifier(specifier)) => {
                    if let Some(distribution) =
                        registry_index
                            .get(&requirement.name)
                            .find_map(|(version, distribution)| {
                                if specifier.contains(version) {
                                    Some(distribution)
                                } else {
                                    None
                                }
                            })
                    {
                        debug!("Requirement already cached: {distribution}");
                        local.push(CachedDist::Registry(distribution.clone()));
                        continue;
                    }
                }
                Some(VersionOrUrl::Url(url)) => {
                    match Dist::from_url(requirement.name.clone(), url.clone())? {
                        Dist::Built(BuiltDist::Registry(_wheel)) => {
                            // Nothing to do.
                        }
                        Dist::Source(SourceDist::Registry(_)) => {
                            // Nothing to do.
                        }
                        Dist::Built(BuiltDist::DirectUrl(wheel)) => {
                            // Find the exact wheel from the cache, since we know the filename in
                            // advance.
                            let cache_entry = cache.entry(
                                CacheBucket::Wheels,
                                WheelCache::Url(&wheel.url).remote_wheel_dir(wheel.name().as_ref()),
                                wheel.filename.stem(),
                            );

                            if cache_entry.path().exists() {
                                let cached_dist = CachedDirectUrlDist::from_url(
                                    wheel.filename,
                                    wheel.url,
                                    cache_entry.path(),
                                );

                                debug!("URL wheel requirement already cached: {cached_dist}");
                                local.push(CachedDist::Url(cached_dist.clone()));
                                continue;
                            }
                        }
                        Dist::Built(BuiltDist::Path(wheel)) => {
                            // Find the exact wheel from the cache, since we know the filename in
                            // advance.
                            let cache_entry = cache.entry(
                                CacheBucket::Wheels,
                                WheelCache::Url(&wheel.url).remote_wheel_dir(wheel.name().as_ref()),
                                wheel.filename.stem(),
                            );

                            if cache_entry.path().exists() {
                                let cached_dist = CachedDirectUrlDist::from_url(
                                    wheel.filename,
                                    wheel.url,
                                    cache_entry.path(),
                                );

                                debug!("Path wheel requirement already cached: {cached_dist}");
                                local.push(CachedDist::Url(cached_dist.clone()));
                                continue;
                            }
                        }
                        Dist::Source(SourceDist::DirectUrl(sdist)) => {
                            // Find the most-compatible wheel from the cache, since we don't know
                            // the filename in advance.
                            let cache_shard = cache.shard(
                                CacheBucket::BuiltWheels,
                                WheelCache::Url(&sdist.url).remote_wheel_dir(sdist.name().as_ref()),
                            );

                            if let Some(wheel) = BuiltWheelIndex::find(&cache_shard, tags) {
                                let cached_dist = wheel.into_url_dist(url.clone());
                                debug!("URL source requirement already cached: {cached_dist}");
                                local.push(CachedDist::Url(cached_dist.clone()));
                                continue;
                            }
                        }
                        Dist::Source(SourceDist::Path(sdist)) => {
                            // Find the most-compatible wheel from the cache, since we don't know
                            // the filename in advance.
                            let cache_shard = cache.shard(
                                CacheBucket::BuiltWheels,
                                WheelCache::Path(&sdist.url)
                                    .remote_wheel_dir(sdist.name().as_ref()),
                            );

                            if let Some(wheel) = BuiltWheelIndex::find(&cache_shard, tags) {
                                let cached_dist = wheel.into_url_dist(url.clone());
                                debug!("Path source requirement already cached: {cached_dist}");
                                local.push(CachedDist::Url(cached_dist.clone()));
                                continue;
                            }
                        }
                        Dist::Source(SourceDist::Git(sdist)) => {
                            // Find the most-compatible wheel from the cache, since we don't know
                            // the filename in advance.
                            if let Ok(Some(git_sha)) = git_reference(&sdist.url) {
                                let cache_shard = cache.shard(
                                    CacheBucket::BuiltWheels,
                                    WheelCache::Git(&sdist.url, &git_sha.to_short_string())
                                        .remote_wheel_dir(sdist.name().as_ref()),
                                );

                                if let Some(wheel) = BuiltWheelIndex::find(&cache_shard, tags) {
                                    let cached_dist = wheel.into_url_dist(url.clone());
                                    debug!("Git source requirement already cached: {cached_dist}");
                                    local.push(CachedDist::Url(cached_dist.clone()));
                                    continue;
                                }
                            }
                        }
                    }
                }
            }

            debug!("Identified uncached requirement: {requirement}");
            remote.push(requirement.clone());
        }

        // Remove any unnecessary packages.
        if !site_packages.is_empty() {
            // If Puffin created the virtual environment, then remove all packages, regardless of
            // whether they're considered "seed" packages.
            let seed_packages = !venv.cfg().is_ok_and(|cfg| cfg.is_gourgeist());
            for dist_info in site_packages {
                if seed_packages
                    && matches!(dist_info.name().as_ref(), "pip" | "setuptools" | "wheel")
                {
                    debug!("Preserving seed package: {dist_info}");
                    continue;
                }

                debug!("Unnecessary package: {dist_info}");
                extraneous.push(dist_info);
            }
        }

        Ok(InstallPlan {
            local,
            remote,
            reinstalls,
            extraneous,
        })
    }
}

#[derive(Debug)]
pub enum Reinstall {
    /// Don't reinstall any packages; respect the existing installation.
    None,

    /// Reinstall all packages in the plan.
    All,

    /// Reinstall only the specified packages.
    Packages(Vec<PackageName>),
}

impl Reinstall {
    /// Determine the reinstall strategy to use.
    pub fn from_args(reinstall: bool, reinstall_package: Vec<PackageName>) -> Self {
        if reinstall {
            Self::All
        } else if !reinstall_package.is_empty() {
            Self::Packages(reinstall_package)
        } else {
            Self::None
        }
    }

    /// Returns `true` if no packages should be reinstalled.
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}
