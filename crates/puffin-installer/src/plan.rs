use std::hash::BuildHasherDefault;
use std::io;
use std::path::Path;

use anyhow::{bail, Result};
use puffin_traits::NoBinary;
use rustc_hash::FxHashSet;
use tracing::{debug, warn};

use distribution_types::{
    git_reference, BuiltDist, CachedDirectUrlDist, CachedDist, Dist, IndexLocations,
    InstalledDirectUrlDist, InstalledDist, Name, SourceDist,
};
use pep508_rs::{Requirement, VersionOrUrl};
use platform_tags::Tags;
use puffin_cache::{Cache, CacheBucket, CacheEntry, WheelCache};
use puffin_distribution::{BuiltWheelIndex, RegistryWheelIndex};
use puffin_interpreter::Virtualenv;
use puffin_normalize::PackageName;

use crate::{ResolvedEditable, SitePackages};

/// A planner to generate an [`Plan`] based on a set of requirements.
#[derive(Debug)]
pub struct Planner<'a> {
    requirements: &'a [Requirement],
    editable_requirements: Vec<ResolvedEditable>,
}

impl<'a> Planner<'a> {
    /// Set the requirements use in the [`Plan`].
    #[must_use]
    pub fn with_requirements(requirements: &'a [Requirement]) -> Self {
        Self {
            requirements,
            editable_requirements: Vec::new(),
        }
    }

    /// Set the editable requirements use in the [`Plan`].
    #[must_use]
    pub fn with_editable_requirements(self, editable_requirements: Vec<ResolvedEditable>) -> Self {
        Self {
            editable_requirements,
            ..self
        }
    }

    /// Partition a set of requirements into those that should be linked from the cache, those that
    /// need to be downloaded, and those that should be removed.
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        self,
        mut site_packages: SitePackages,
        reinstall: &Reinstall,
        no_binary: &NoBinary,
        index_locations: &IndexLocations,
        cache: &Cache,
        venv: &Virtualenv,
        tags: &Tags,
    ) -> Result<Plan> {
        // Index all the already-downloaded wheels in the cache.
        let mut registry_index = RegistryWheelIndex::new(cache, tags, index_locations);

        let mut local = vec![];
        let mut remote = vec![];
        let mut reinstalls = vec![];
        let mut extraneous = vec![];
        let mut seen = FxHashSet::with_capacity_and_hasher(
            self.requirements.len(),
            BuildHasherDefault::default(),
        );

        // Remove any editable requirements.
        for requirement in self.editable_requirements {
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

        for requirement in self.requirements {
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

            // Check if installation of a binary version of the package should be allowed.
            let no_binary = match no_binary {
                NoBinary::None => false,
                NoBinary::All => true,
                NoBinary::Packages(packages) => packages.contains(&requirement.name),
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
                                    // If the requirement came from a local path, check freshness.
                                    if let Ok(archive) = url.to_file_path() {
                                        if is_fresh_install(distribution, &archive)? {
                                            debug!("Requirement already satisfied (and up-to-date): {distribution}");
                                            continue;
                                        }
                                    } else {
                                        // Otherwise, assume the requirement is up-to-date.
                                        debug!("Requirement already satisfied (assumed up-to-date): {distribution}");
                                        continue;
                                    }
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
                        Dist::Built(BuiltDist::Registry(_)) => {
                            // Nothing to do.
                        }
                        Dist::Source(SourceDist::Registry(_)) => {
                            // Nothing to do.
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
                            let cache_entry = cache.entry(
                                CacheBucket::Wheels,
                                WheelCache::Url(&wheel.url).remote_wheel_dir(wheel.name().as_ref()),
                                wheel.filename.stem(),
                            );

                            if cache_entry.path().exists() {
                                let cached_dist = CachedDirectUrlDist::from_url(
                                    wheel.filename,
                                    wheel.url,
                                    cache_entry.into_path_buf(),
                                );

                                debug!("URL wheel requirement already cached: {cached_dist}");
                                local.push(CachedDist::Url(cached_dist));
                                continue;
                            }
                        }
                        Dist::Built(BuiltDist::Path(wheel)) => {
                            if !wheel.filename.is_compatible(tags) {
                                bail!(
                                    "A path dependency is incompatible with the current platform: {}",
                                    wheel.path.display()
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
                            let cache_entry = cache.entry(
                                CacheBucket::Wheels,
                                WheelCache::Url(&wheel.url).remote_wheel_dir(wheel.name().as_ref()),
                                wheel.filename.stem(),
                            );

                            if is_fresh_cache(&cache_entry, &wheel.path)? {
                                let cached_dist = CachedDirectUrlDist::from_url(
                                    wheel.filename,
                                    wheel.url,
                                    cache_entry.into_path_buf(),
                                );

                                debug!("Path wheel requirement already cached: {cached_dist}");
                                local.push(CachedDist::Url(cached_dist));
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
                                local.push(CachedDist::Url(cached_dist));
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
                                if is_fresh_cache(&wheel.entry, &sdist.path)? {
                                    let cached_dist = wheel.into_url_dist(url.clone());
                                    debug!("Path source requirement already cached: {cached_dist}");
                                    local.push(CachedDist::Url(cached_dist));
                                    continue;
                                }
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
                                    local.push(CachedDist::Url(cached_dist));
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

        Ok(Plan {
            local,
            remote,
            reinstalls,
            extraneous,
        })
    }
}

/// Returns `true` if the cache entry linked to the file at the given [`Path`] is fresh.
///
/// A cache entry is considered fresh if it exists and is newer than the file at the given path.
/// If the cache entry is stale, it will be removed from the cache.
fn is_fresh_cache(cache_entry: &CacheEntry, artifact: &Path) -> Result<bool, io::Error> {
    match fs_err::metadata(cache_entry.path()).and_then(|metadata| metadata.modified()) {
        Ok(cache_mtime) => {
            // Determine the modification time of the wheel.
            let Some(artifact_mtime) = puffin_cache::archive_mtime(artifact)? else {
                // The artifact doesn't exist, so it's not fresh.
                return Ok(false);
            };
            if cache_mtime >= artifact_mtime {
                Ok(true)
            } else {
                debug!(
                    "Removing stale built wheels for: {}",
                    cache_entry.path().display()
                );
                if let Err(err) = fs_err::remove_dir_all(cache_entry.dir()) {
                    warn!("Failed to remove stale built wheel cache directory: {err}");
                }
                Ok(false)
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            // The cache entry doesn't exist, so it's not fresh.
            Ok(false)
        }
        Err(err) => Err(err),
    }
}

/// Returns `true` if the installed distribution linked to the file at the given [`Path`] is fresh,
/// based on the modification time of the installed distribution.
fn is_fresh_install(dist: &InstalledDirectUrlDist, artifact: &Path) -> Result<bool, io::Error> {
    // Determine the modification time of the installed distribution.
    let dist_metadata = fs_err::metadata(&dist.path)?;
    let dist_mtime = dist_metadata.modified()?;

    // Determine the modification time of the wheel.
    let Some(artifact_mtime) = puffin_cache::archive_mtime(artifact)? else {
        // The artifact doesn't exist, so it's not fresh.
        return Ok(false);
    };

    Ok(dist_mtime >= artifact_mtime)
}

#[derive(Debug, Default)]
pub struct Plan {
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
