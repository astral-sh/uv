use anyhow::{bail, Result};
use tracing::debug;

use uv_cache::{Cache, CacheBucket, WheelCache};
use uv_cache_info::{CacheInfo, Timestamp};
use uv_configuration::{BuildOptions, ConfigSettings, Reinstall};
use uv_distribution::{
    BuiltWheelIndex, HttpArchivePointer, LocalArchivePointer, RegistryWheelIndex,
};
use uv_distribution_types::{
    BuiltDist, CachedDirectUrlDist, CachedDist, Dist, Error, Hashed, IndexLocations, InstalledDist,
    Name, Resolution, ResolvedDist, SourceDist,
};
use uv_fs::Simplified;
use uv_platform_tags::Tags;
use uv_pypi_types::Requirement;
use uv_python::PythonEnvironment;
use uv_types::HashStrategy;

use crate::satisfies::RequirementSatisfaction;
use crate::SitePackages;

/// A planner to generate an [`Plan`] based on a set of requirements.
#[derive(Debug)]
pub struct Planner<'a> {
    resolution: &'a Resolution,
}

impl<'a> Planner<'a> {
    /// Set the requirements use in the [`Plan`].
    pub fn new(resolution: &'a Resolution) -> Self {
        Self { resolution }
    }

    /// Partition a set of requirements into those that should be linked from the cache, those that
    /// need to be downloaded, and those that should be removed.
    ///
    /// The install plan will respect cache [`Freshness`]. Specifically, if refresh is enabled, the
    /// plan will respect cache entries created after the current time (as per the [`Refresh`]
    /// policy). Otherwise, entries will be ignored. The downstream distribution database may still
    /// read those entries from the cache after revalidating them.
    ///
    /// The install plan will also respect the required hashes, such that it will never return a
    /// cached distribution that does not match the required hash. Like pip, though, it _will_
    /// return an _installed_ distribution that does not match the required hash.
    pub fn build(
        self,
        mut site_packages: SitePackages,
        reinstall: &Reinstall,
        build_options: &BuildOptions,
        hasher: &HashStrategy,
        index_locations: &IndexLocations,
        config_settings: &ConfigSettings,
        cache: &Cache,
        venv: &PythonEnvironment,
        tags: &Tags,
    ) -> Result<Plan> {
        // Index all the already-downloaded wheels in the cache.
        let mut registry_index = RegistryWheelIndex::new(cache, tags, index_locations, hasher);
        let built_index = BuiltWheelIndex::new(cache, tags, hasher, config_settings);

        let mut cached = vec![];
        let mut remote = vec![];
        let mut reinstalls = vec![];
        let mut extraneous = vec![];

        for dist in self.resolution.distributions() {
            // Check if the package should be reinstalled.
            let reinstall = match reinstall {
                Reinstall::None => false,
                Reinstall::All => true,
                Reinstall::Packages(packages) => packages.contains(dist.name()),
            };

            // Check if installation of a binary version of the package should be allowed.
            let no_binary = build_options.no_binary_package(dist.name());
            let no_build = build_options.no_build_package(dist.name());

            let requirement = Requirement::from(dist);

            // Determine whether the distribution is already installed.
            let installed_dists = site_packages.remove_packages(&requirement.name);
            if reinstall {
                reinstalls.extend(installed_dists);
            } else {
                match installed_dists.as_slice() {
                    [] => {}
                    [distribution] => {
                        match RequirementSatisfaction::check(distribution, &requirement.source)? {
                            RequirementSatisfaction::Mismatch => {
                                debug!("Requirement installed, but mismatched: {distribution:?}");
                            }
                            RequirementSatisfaction::Satisfied => {
                                debug!("Requirement already installed: {distribution}");
                                continue;
                            }
                            RequirementSatisfaction::OutOfDate => {
                                debug!("Requirement installed, but not fresh: {distribution}");
                            }
                        }
                        reinstalls.push(distribution.clone());
                    }
                    // We reinstall installed distributions with multiple versions because
                    // we do not want to keep multiple incompatible versions but removing
                    // one version is likely to break another.
                    _ => reinstalls.extend(installed_dists),
                }
            }

            let ResolvedDist::Installable(installable) = dist else {
                unreachable!("Installed distribution could not be found in site-packages: {dist}");
            };

            if cache.must_revalidate(dist.name()) {
                debug!("Must revalidate requirement: {}", dist.name());
                remote.push(installable.clone());
                continue;
            }

            // Identify any cached distributions that satisfy the requirement.
            match installable {
                Dist::Built(BuiltDist::Registry(wheel)) => {
                    if let Some(distribution) = registry_index.get(wheel.name()).find_map(|entry| {
                        if entry.dist.filename.version != wheel.best_wheel().filename.version {
                            return None;
                        };
                        if entry.built && no_build {
                            return None;
                        }
                        if !entry.built && no_binary {
                            return None;
                        }
                        Some(&entry.dist)
                    }) {
                        debug!("Requirement already cached: {distribution}");
                        cached.push(CachedDist::Registry(distribution.clone()));
                        continue;
                    }
                }
                Dist::Built(BuiltDist::DirectUrl(wheel)) => {
                    if !wheel.filename.is_compatible(tags) {
                        bail!(
                            "A URL dependency is incompatible with the current platform: {}",
                            wheel.url
                        );
                    }

                    if no_binary {
                        bail!(
                            "A URL dependency points to a wheel which conflicts with `--no-binary`: {}",
                            wheel.url
                        );
                    }

                    // Find the exact wheel from the cache, since we know the filename in
                    // advance.
                    let cache_entry = cache
                        .shard(
                            CacheBucket::Wheels,
                            WheelCache::Url(&wheel.url).wheel_dir(wheel.name().as_ref()),
                        )
                        .entry(format!("{}.http", wheel.filename.stem()));

                    // Read the HTTP pointer.
                    if let Some(pointer) = HttpArchivePointer::read_from(&cache_entry)? {
                        let archive = pointer.into_archive();
                        if archive.satisfies(hasher.get(dist)) {
                            let cached_dist = CachedDirectUrlDist::from_url(
                                wheel.filename.clone(),
                                wheel.url.clone(),
                                archive.hashes,
                                CacheInfo::default(),
                                cache.archive(&archive.id),
                            );

                            debug!("URL wheel requirement already cached: {cached_dist}");
                            cached.push(CachedDist::Url(cached_dist));
                            continue;
                        }
                    }
                }
                Dist::Built(BuiltDist::Path(wheel)) => {
                    // Validate that the path exists.
                    if !wheel.install_path.exists() {
                        return Err(Error::NotFound(wheel.url.to_url()).into());
                    }

                    if !wheel.filename.is_compatible(tags) {
                        bail!(
                            "A path dependency is incompatible with the current platform: {}",
                            wheel.install_path.user_display()
                        );
                    }

                    if no_binary {
                        bail!(
                            "A path dependency points to a wheel which conflicts with `--no-binary`: {}",
                            wheel.url
                        );
                    }

                    // Find the exact wheel from the cache, since we know the filename in
                    // advance.
                    let cache_entry = cache
                        .shard(
                            CacheBucket::Wheels,
                            WheelCache::Url(&wheel.url).wheel_dir(wheel.name().as_ref()),
                        )
                        .entry(format!("{}.rev", wheel.filename.stem()));

                    if let Some(pointer) = LocalArchivePointer::read_from(&cache_entry)? {
                        let timestamp = Timestamp::from_path(&wheel.install_path)?;
                        if pointer.is_up_to_date(timestamp) {
                            let cache_info = pointer.to_cache_info();
                            let archive = pointer.into_archive();
                            if archive.satisfies(hasher.get(dist)) {
                                let cached_dist = CachedDirectUrlDist::from_url(
                                    wheel.filename.clone(),
                                    wheel.url.clone(),
                                    archive.hashes,
                                    cache_info,
                                    cache.archive(&archive.id),
                                );

                                debug!("Path wheel requirement already cached: {cached_dist}");
                                cached.push(CachedDist::Url(cached_dist));
                                continue;
                            }
                        }
                    }
                }
                Dist::Source(SourceDist::Registry(sdist)) => {
                    if let Some(distribution) = registry_index.get(sdist.name()).find_map(|entry| {
                        if entry.dist.filename.version != sdist.version {
                            return None;
                        };
                        if entry.built && no_build {
                            return None;
                        }
                        if !entry.built && no_binary {
                            return None;
                        }
                        Some(&entry.dist)
                    }) {
                        debug!("Requirement already cached: {distribution}");
                        cached.push(CachedDist::Registry(distribution.clone()));
                        continue;
                    }
                }
                Dist::Source(SourceDist::DirectUrl(sdist)) => {
                    // Find the most-compatible wheel from the cache, since we don't know
                    // the filename in advance.
                    if let Some(wheel) = built_index.url(sdist)? {
                        let cached_dist = wheel.into_url_dist(sdist.url.clone());
                        debug!("URL source requirement already cached: {cached_dist}");
                        cached.push(CachedDist::Url(cached_dist));
                        continue;
                    }
                }
                Dist::Source(SourceDist::Git(sdist)) => {
                    // Find the most-compatible wheel from the cache, since we don't know
                    // the filename in advance.
                    if let Some(wheel) = built_index.git(sdist) {
                        let cached_dist = wheel.into_url_dist(sdist.url.clone());
                        debug!("Git source requirement already cached: {cached_dist}");
                        cached.push(CachedDist::Url(cached_dist));
                        continue;
                    }
                }
                Dist::Source(SourceDist::Path(sdist)) => {
                    // Validate that the path exists.
                    if !sdist.install_path.exists() {
                        return Err(Error::NotFound(sdist.url.to_url()).into());
                    }

                    // Find the most-compatible wheel from the cache, since we don't know
                    // the filename in advance.
                    if let Some(wheel) = built_index.path(sdist)? {
                        let cached_dist = wheel.into_url_dist(sdist.url.clone());
                        debug!("Path source requirement already cached: {cached_dist}");
                        cached.push(CachedDist::Url(cached_dist));
                        continue;
                    }
                }
                Dist::Source(SourceDist::Directory(sdist)) => {
                    // Validate that the path exists.
                    if !sdist.install_path.exists() {
                        return Err(Error::NotFound(sdist.url.to_url()).into());
                    }

                    // Find the most-compatible wheel from the cache, since we don't know
                    // the filename in advance.
                    if let Some(wheel) = built_index.directory(sdist)? {
                        let cached_dist = if sdist.editable {
                            wheel.into_editable(sdist.url.clone())
                        } else {
                            wheel.into_url_dist(sdist.url.clone())
                        };
                        debug!("Directory source requirement already cached: {cached_dist}");
                        cached.push(CachedDist::Url(cached_dist));
                        continue;
                    }
                }
            }

            debug!("Identified uncached distribution: {installable}");
            remote.push(installable.clone());
        }

        // Remove any unnecessary packages.
        if site_packages.any() {
            // Retain seed packages unless: (1) the virtual environment was created by uv and
            // (2) the `--seed` argument was not passed to `uv venv`.
            let seed_packages = !venv.cfg().is_ok_and(|cfg| cfg.is_uv() && !cfg.is_seed());
            for dist_info in site_packages {
                if seed_packages
                    && matches!(
                        dist_info.name().as_ref(),
                        "pip" | "setuptools" | "wheel" | "uv"
                    )
                {
                    debug!("Preserving seed package: {dist_info}");
                    continue;
                }

                debug!("Unnecessary package: {dist_info}");
                extraneous.push(dist_info);
            }
        }

        Ok(Plan {
            cached,
            remote,
            reinstalls,
            extraneous,
        })
    }
}

#[derive(Debug, Default)]
pub struct Plan {
    /// The distributions that are not already installed in the current environment, but are
    /// available in the local cache.
    pub cached: Vec<CachedDist>,

    /// The distributions that are not already installed in the current environment, and are
    /// not available in the local cache.
    pub remote: Vec<Dist>,

    /// Any distributions that are already installed in the current environment, but will be
    /// re-installed (including upgraded) to satisfy the requirements.
    pub reinstalls: Vec<InstalledDist>,

    /// Any distributions that are already installed in the current environment, and are
    /// _not_ necessary to satisfy the requirements.
    pub extraneous: Vec<InstalledDist>,
}
