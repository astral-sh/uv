use std::hash::BuildHasherDefault;

use anyhow::{bail, Context, Result};
use fxhash::FxHashMap;
use tracing::debug;

use distribution_types::direct_url::{git_reference, DirectUrl};
use distribution_types::{
    BuiltDist, CachedDirectUrlDist, CachedDist, Dist, InstalledDist, Metadata, SourceDist,
};
use pep508_rs::{MarkerEnvironment, Requirement, VersionOrUrl};
use platform_tags::Tags;
use puffin_cache::{Cache, CacheBucket, WheelCache};
use puffin_distribution::{BuiltWheelIndex, RegistryWheelIndex};
use puffin_interpreter::Virtualenv;
use puffin_normalize::PackageName;
use pypi_types::IndexUrls;

use crate::SitePackages;

#[derive(Debug, Default)]
pub struct InstallPlan {
    /// The distributions that are not already installed in the current environment, but are
    /// available in the local cache.
    pub local: Vec<CachedDist>,

    /// The distributions that are not already installed in the current environment, and are
    /// not available in the local cache.
    pub remote: Vec<Requirement>,

    /// The distributions that are already installed in the current environment, and are
    /// _not_ necessary to satisfy the requirements.
    pub extraneous: Vec<InstalledDist>,
}

impl InstallPlan {
    /// Partition a set of requirements into those that should be linked from the cache, those that
    /// need to be downloaded, and those that should be removed.
    pub fn from_requirements(
        requirements: &[Requirement],
        reinstall: &Reinstall,
        index_urls: &IndexUrls,
        cache: &Cache,
        venv: &Virtualenv,
        env: &MarkerEnvironment,
        tags: &Tags,
    ) -> Result<Self> {
        // Index all the already-installed packages in site-packages.
        let mut site_packages =
            SitePackages::try_from_executable(venv).context("Failed to list installed packages")?;

        // Index all the already-downloaded wheels in the cache.
        let mut registry_index = RegistryWheelIndex::new(cache, tags, index_urls);

        let mut local = vec![];
        let mut remote = vec![];
        let mut extraneous = vec![];
        let mut seen =
            FxHashMap::with_capacity_and_hasher(requirements.len(), BuildHasherDefault::default());

        for requirement in requirements {
            // Filter out incompatible requirements.
            if !requirement.evaluate_markers(env, &[]) {
                continue;
            }

            // If we see the same requirement twice, then we have a conflict.
            if let Some(existing) = seen.insert(requirement.name.clone(), requirement) {
                bail!("Detected duplicate package in requirements:\n    {requirement}\n    {existing}");
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
            } else if let Some(distribution) = site_packages.remove(&requirement.name) {
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
            for (package, dist_info) in site_packages {
                if seed_packages && matches!(package.as_ref(), "pip" | "setuptools" | "wheel") {
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
}
