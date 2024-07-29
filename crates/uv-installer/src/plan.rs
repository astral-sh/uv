use std::collections::hash_map::Entry;
use std::path::Path;
use std::str::FromStr;

use anyhow::{bail, Result};
use rustc_hash::{FxBuildHasher, FxHashMap};
use tracing::debug;

use distribution_filename::WheelFilename;
use distribution_types::{
    CachedDirectUrlDist, CachedDist, DirectUrlBuiltDist, DirectUrlSourceDist, DirectorySourceDist,
    Error, GitSourceDist, Hashed, IndexLocations, InstalledDist, Name, PathBuiltDist,
    PathSourceDist, RemoteSource, Verbatim,
};
use platform_tags::Tags;
use pypi_types::{Requirement, RequirementSource};
use uv_cache::{ArchiveTimestamp, Cache, CacheBucket, WheelCache};
use uv_configuration::{BuildOptions, Reinstall};
use uv_distribution::{
    BuiltWheelIndex, HttpArchivePointer, LocalArchivePointer, RegistryWheelIndex,
};
use uv_fs::Simplified;
use uv_git::GitUrl;
use uv_python::PythonEnvironment;
use uv_types::HashStrategy;

use crate::satisfies::RequirementSatisfaction;
use crate::SitePackages;

/// A planner to generate an [`Plan`] based on a set of requirements.
#[derive(Debug)]
pub struct Planner<'a> {
    requirements: &'a [Requirement],
}

impl<'a> Planner<'a> {
    /// Set the requirements use in the [`Plan`].
    pub fn new(requirements: &'a [Requirement]) -> Self {
        Self { requirements }
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
        cache: &Cache,
        venv: &PythonEnvironment,
        tags: &Tags,
    ) -> Result<Plan> {
        // Index all the already-downloaded wheels in the cache.
        let mut registry_index = RegistryWheelIndex::new(cache, tags, index_locations, hasher);
        let built_index = BuiltWheelIndex::new(cache, tags, hasher);

        let mut cached = vec![];
        let mut remote = vec![];
        let mut reinstalls = vec![];
        let mut extraneous = vec![];
        let mut seen = FxHashMap::with_capacity_and_hasher(self.requirements.len(), FxBuildHasher);

        for requirement in self.requirements {
            // Filter out incompatible requirements.
            if !requirement.evaluate_markers(Some(venv.interpreter().markers()), &[]) {
                continue;
            }

            // If we see the same requirement twice, then we have a conflict.
            match seen.entry(requirement.name.clone()) {
                Entry::Occupied(value) => {
                    if value.get() == &&requirement.source {
                        continue;
                    }
                    bail!(
                        "Detected duplicate package in requirements: {}",
                        requirement.name
                    );
                }
                Entry::Vacant(entry) => {
                    entry.insert(&requirement.source);
                }
            }

            // Check if the package should be reinstalled. A reinstall involves (1) purging any
            // cached distributions, and (2) marking any installed distributions as extraneous.
            let reinstall = match reinstall {
                Reinstall::None => false,
                Reinstall::All => true,
                Reinstall::Packages(packages) => packages.contains(&requirement.name),
            };

            // Check if installation of a binary version of the package should be allowed.
            let no_binary = build_options.no_binary_package(&requirement.name);

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

            if cache.must_revalidate(&requirement.name) {
                debug!("Must revalidate requirement: {}", requirement.name);
                remote.push(requirement.clone());
                continue;
            }

            // Identify any cached distributions that satisfy the requirement.
            match &requirement.source {
                RequirementSource::Registry { specifier, .. } => {
                    if let Some((_version, distribution)) = registry_index
                        .get(&requirement.name)
                        .find(|(version, _)| specifier.contains(version))
                    {
                        debug!("Requirement already cached: {distribution}");
                        cached.push(CachedDist::Registry(distribution.clone()));
                        continue;
                    }
                }
                RequirementSource::Url {
                    subdirectory,
                    location,
                    url,
                } => {
                    // Check if we have a wheel or a source distribution.
                    if Path::new(url.path())
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("whl"))
                    {
                        // Validate that the name in the wheel matches that of the requirement.
                        let filename = WheelFilename::from_str(&url.filename()?)?;
                        if filename.name != requirement.name {
                            return Err(Error::PackageNameMismatch(
                                requirement.name.clone(),
                                filename.name,
                                url.verbatim().to_string(),
                            )
                            .into());
                        }

                        let wheel = DirectUrlBuiltDist {
                            filename,
                            location: location.clone(),
                            url: url.clone(),
                        };

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
                            if archive.satisfies(hasher.get(&wheel)) {
                                let cached_dist = CachedDirectUrlDist::from_url(
                                    wheel.filename,
                                    wheel.url,
                                    archive.hashes,
                                    cache.archive(&archive.id),
                                );

                                debug!("URL wheel requirement already cached: {cached_dist}");
                                cached.push(CachedDist::Url(cached_dist));
                                continue;
                            }
                        }
                    } else {
                        let sdist = DirectUrlSourceDist {
                            name: requirement.name.clone(),
                            location: location.clone(),
                            subdirectory: subdirectory.clone(),
                            url: url.clone(),
                        };
                        // Find the most-compatible wheel from the cache, since we don't know
                        // the filename in advance.
                        if let Some(wheel) = built_index.url(&sdist)? {
                            let cached_dist = wheel.into_url_dist(url.clone());
                            debug!("URL source requirement already cached: {cached_dist}");
                            cached.push(CachedDist::Url(cached_dist));
                            continue;
                        }
                    }
                }
                RequirementSource::Git {
                    repository,
                    reference,
                    precise,
                    subdirectory,
                    url,
                } => {
                    let mut git = GitUrl::new(repository.clone(), reference.clone());
                    if let Some(precise) = precise {
                        git = git.with_precise(*precise);
                    }
                    let sdist = GitSourceDist {
                        name: requirement.name.clone(),
                        git: Box::new(git),
                        subdirectory: subdirectory.clone(),
                        url: url.clone(),
                    };
                    // Find the most-compatible wheel from the cache, since we don't know
                    // the filename in advance.
                    if let Some(wheel) = built_index.git(&sdist) {
                        let cached_dist = wheel.into_url_dist(url.clone());
                        debug!("Git source requirement already cached: {cached_dist}");
                        cached.push(CachedDist::Url(cached_dist));
                        continue;
                    }
                }

                RequirementSource::Directory {
                    url,
                    editable,
                    install_path,
                    lock_path,
                } => {
                    // Store the canonicalized path, which also serves to validate that it exists.
                    let path = match install_path.canonicalize() {
                        Ok(path) => path,
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                            return Err(Error::NotFound(url.to_url()).into());
                        }
                        Err(err) => return Err(err.into()),
                    };

                    let sdist = DirectorySourceDist {
                        name: requirement.name.clone(),
                        url: url.clone(),
                        install_path: path,
                        lock_path: lock_path.clone(),
                        editable: *editable,
                    };

                    // Find the most-compatible wheel from the cache, since we don't know
                    // the filename in advance.
                    if let Some(wheel) = built_index.directory(&sdist)? {
                        let cached_dist = if *editable {
                            wheel.into_editable(url.clone())
                        } else {
                            wheel.into_url_dist(url.clone())
                        };
                        debug!("Directory source requirement already cached: {cached_dist}");
                        cached.push(CachedDist::Url(cached_dist));
                        continue;
                    }
                }

                RequirementSource::Path {
                    url,
                    install_path,
                    lock_path,
                } => {
                    // Store the canonicalized path, which also serves to validate that it exists.
                    let path = match install_path.canonicalize() {
                        Ok(path) => path,
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                            return Err(Error::NotFound(url.to_url()).into());
                        }
                        Err(err) => return Err(err.into()),
                    };

                    // Check if we have a wheel or a source distribution.
                    if path
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("whl"))
                    {
                        // Validate that the name in the wheel matches that of the requirement.
                        let filename = WheelFilename::from_str(&url.filename()?)?;
                        if filename.name != requirement.name {
                            return Err(Error::PackageNameMismatch(
                                requirement.name.clone(),
                                filename.name,
                                url.verbatim().to_string(),
                            )
                            .into());
                        }

                        let wheel = PathBuiltDist {
                            filename,
                            url: url.clone(),
                            path,
                        };

                        if !wheel.filename.is_compatible(tags) {
                            bail!(
                                "A path dependency is incompatible with the current platform: {}",
                                wheel.path.user_display()
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
                            let timestamp = ArchiveTimestamp::from_file(&wheel.path)?;
                            if pointer.is_up_to_date(timestamp) {
                                let archive = pointer.into_archive();
                                if archive.satisfies(hasher.get(&wheel)) {
                                    let cached_dist = CachedDirectUrlDist::from_url(
                                        wheel.filename,
                                        wheel.url,
                                        archive.hashes,
                                        cache.archive(&archive.id),
                                    );

                                    debug!("Path wheel requirement already cached: {cached_dist}");
                                    cached.push(CachedDist::Url(cached_dist));
                                    continue;
                                }
                            }
                        }
                    } else {
                        let sdist = PathSourceDist {
                            name: requirement.name.clone(),
                            url: url.clone(),
                            install_path: path,
                            lock_path: lock_path.clone(),
                        };

                        // Find the most-compatible wheel from the cache, since we don't know
                        // the filename in advance.
                        if let Some(wheel) = built_index.path(&sdist)? {
                            let cached_dist = wheel.into_url_dist(url.clone());
                            debug!("Path source requirement already cached: {cached_dist}");
                            cached.push(CachedDist::Url(cached_dist));
                            continue;
                        }
                    }
                }
            }

            debug!("Identified uncached requirement: {requirement}");
            remote.push(requirement.clone());
        }

        // Remove any unnecessary packages.
        if site_packages.any() {
            // If uv created the virtual environment, then remove all packages, regardless of
            // whether they're considered "seed" packages.
            let seed_packages = !venv.cfg().is_ok_and(|cfg| cfg.is_uv());
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
    pub remote: Vec<Requirement>,

    /// Any distributions that are already installed in the current environment, but will be
    /// re-installed (including upgraded) to satisfy the requirements.
    pub reinstalls: Vec<InstalledDist>,

    /// Any distributions that are already installed in the current environment, and are
    /// _not_ necessary to satisfy the requirements.
    pub extraneous: Vec<InstalledDist>,
}
