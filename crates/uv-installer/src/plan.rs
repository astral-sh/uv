use std::collections::hash_map::Entry;
use std::hash::BuildHasherDefault;
use std::io;

use anyhow::{bail, Result};
use rustc_hash::FxHashMap;
use tracing::{debug, warn};

use distribution_types::{
    BuiltDist, CachedDirectUrlDist, CachedDist, Dist, IndexLocations, InstalledDist,
    InstalledMetadata, InstalledVersion, Name, SourceDist,
};
use pep508_rs::{Requirement, VersionOrUrl};
use platform_tags::Tags;
use uv_cache::{ArchiveTarget, ArchiveTimestamp, Cache, CacheBucket, WheelCache};
use uv_distribution::{BuiltWheelIndex, RegistryWheelIndex};
use uv_fs::Simplified;
use uv_interpreter::PythonEnvironment;
use uv_normalize::PackageName;
use uv_traits::NoBinary;

use crate::{ResolvedEditable, SitePackages};

/// A planner to generate an [`Plan`] based on a set of requirements.
#[derive(Debug)]
pub struct Planner<'a> {
    requirements: &'a [Requirement],
    editable_requirements: &'a [ResolvedEditable],
}

impl<'a> Planner<'a> {
    /// Set the requirements use in the [`Plan`].
    #[must_use]
    pub fn with_requirements(requirements: &'a [Requirement]) -> Self {
        Self {
            requirements,
            editable_requirements: &[],
        }
    }

    /// Set the editable requirements use in the [`Plan`].
    #[must_use]
    pub fn with_editable_requirements(self, editable_requirements: &'a [ResolvedEditable]) -> Self {
        Self {
            editable_requirements,
            ..self
        }
    }

    /// Partition a set of requirements into those that should be linked from the cache, those that
    /// need to be downloaded, and those that should be removed.
    ///
    /// The install plan will respect cache [`Freshness`]. Specifically, if refresh is enabled, the
    /// plan will respect cache entries created after the current time (as per the [`Refresh`]
    /// policy). Otherwise, entries will be ignored. The downstream distribution database may still
    /// read those entries from the cache after revalidating them.
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        self,
        mut site_packages: SitePackages<'_>,
        reinstall: &Reinstall,
        no_binary: &NoBinary,
        index_locations: &IndexLocations,
        cache: &Cache,
        venv: &PythonEnvironment,
        tags: &Tags,
    ) -> Result<Plan> {
        // Index all the already-downloaded wheels in the cache.
        let mut registry_index = RegistryWheelIndex::new(cache, tags, index_locations);

        let mut local = vec![];
        let mut remote = vec![];
        let mut reinstalls = vec![];
        let mut extraneous = vec![];
        let mut seen = FxHashMap::with_capacity_and_hasher(
            self.requirements.len(),
            BuildHasherDefault::default(),
        );

        // Remove any editable requirements.
        for requirement in self.editable_requirements {
            // If we see the same requirement twice, then we have a conflict.
            let specifier = Specifier::Editable(requirement.installed_version());
            match seen.entry(requirement.name().clone()) {
                Entry::Occupied(value) => {
                    if value.get() == &specifier {
                        continue;
                    }
                    bail!(
                        "Detected duplicate package in requirements: {}",
                        requirement.name()
                    );
                }
                Entry::Vacant(entry) => {
                    entry.insert(specifier);
                }
            }

            match requirement {
                ResolvedEditable::Installed(installed) => {
                    debug!("Treating editable requirement as immutable: {installed}");

                    // Remove from the site-packages index, to avoid marking as extraneous.
                    let Some(editable) = installed.as_editable() else {
                        warn!("Editable requirement is not editable: {installed}");
                        continue;
                    };
                    let existing = site_packages.remove_editables(editable);
                    if existing.is_empty() {
                        warn!("Editable requirement is not installed: {installed}");
                        continue;
                    }
                }
                ResolvedEditable::Built(built) => {
                    debug!("Treating editable requirement as mutable: {built}");

                    // Remove any editable installs.
                    let existing = site_packages.remove_editables(built.editable.raw());
                    reinstalls.extend(existing);

                    // Remove any non-editable installs of the same package.
                    let existing = site_packages.remove_packages(built.name());
                    reinstalls.extend(existing);

                    local.push(built.wheel.clone());
                }
            }
        }

        for requirement in self.requirements {
            // Filter out incompatible requirements.
            if !requirement.evaluate_markers(venv.interpreter().markers(), &[]) {
                continue;
            }

            // If we see the same requirement twice, then we have a conflict.
            let specifier = Specifier::NonEditable(requirement.version_or_url.as_ref());
            match seen.entry(requirement.name.clone()) {
                Entry::Occupied(value) => {
                    if value.get() == &specifier {
                        continue;
                    }
                    bail!(
                        "Detected duplicate package in requirements: {}",
                        requirement.name
                    );
                }
                Entry::Vacant(entry) => {
                    entry.insert(specifier);
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
            let no_binary = match no_binary {
                NoBinary::None => false,
                NoBinary::All => true,
                NoBinary::Packages(packages) => packages.contains(&requirement.name),
            };

            if reinstall {
                let installed = site_packages.remove_packages(&requirement.name);
                reinstalls.extend(installed);
            } else {
                let installed = site_packages.remove_packages(&requirement.name);
                match installed.as_slice() {
                    [] => {}
                    [distribution] => {
                        // Filter out already-installed packages.
                        match requirement.version_or_url.as_ref() {
                            // Accept any version of the package.
                            None => continue,

                            // If the requirement comes from a registry, check by name.
                            Some(VersionOrUrl::VersionSpecifier(version_specifier)) => {
                                if version_specifier.contains(distribution.version()) {
                                    debug!("Requirement already satisfied: {distribution}");
                                    continue;
                                }
                            }

                            // If the requirement comes from a direct URL, check by URL.
                            Some(VersionOrUrl::Url(url)) => {
                                if let InstalledDist::Url(installed) = &distribution {
                                    if &installed.url == url.raw() {
                                        // If the requirement came from a local path, check freshness.
                                        if let Ok(archive) = url.to_file_path() {
                                            if ArchiveTimestamp::up_to_date_with(
                                                &archive,
                                                ArchiveTarget::Install(distribution),
                                            )? {
                                                debug!("Requirement already satisfied (and up-to-date): {installed}");
                                                continue;
                                            }
                                            debug!("Requirement already satisfied (but not up-to-date): {installed}");
                                        } else {
                                            // Otherwise, assume the requirement is up-to-date.
                                            debug!("Requirement already satisfied (assumed up-to-date): {installed}");
                                            continue;
                                        }
                                    }
                                }
                            }
                        }

                        reinstalls.push(distribution.clone());
                    }
                    _ => reinstalls.extend(installed),
                }
            }

            if cache.must_revalidate(&requirement.name) {
                debug!("Must revalidate requirement: {requirement}");
                remote.push(requirement.clone());
                continue;
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
                            let cache_entry = cache
                                .shard(
                                    CacheBucket::Wheels,
                                    WheelCache::Url(&wheel.url)
                                        .remote_wheel_dir(wheel.name().as_ref()),
                                )
                                .entry(wheel.filename.stem());

                            match cache_entry.path().canonicalize() {
                                Ok(archive) => {
                                    let cached_dist = CachedDirectUrlDist::from_url(
                                        wheel.filename,
                                        wheel.url,
                                        archive,
                                    );

                                    debug!("URL wheel requirement already cached: {cached_dist}");
                                    local.push(CachedDist::Url(cached_dist));
                                    continue;
                                }
                                Err(err) if err.kind() == io::ErrorKind::NotFound => {
                                    // The cache entry doesn't exist, so it's not fresh.
                                }
                                Err(err) => return Err(err.into()),
                            }
                        }
                        Dist::Built(BuiltDist::Path(wheel)) => {
                            if !wheel.filename.is_compatible(tags) {
                                bail!(
                                    "A path dependency is incompatible with the current platform: {}",
                                    wheel.path.simplified_display()
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
                                    WheelCache::Url(&wheel.url)
                                        .remote_wheel_dir(wheel.name().as_ref()),
                                )
                                .entry(wheel.filename.stem());

                            match cache_entry.path().canonicalize() {
                                Ok(archive) => {
                                    if ArchiveTimestamp::up_to_date_with(
                                        &wheel.path,
                                        ArchiveTarget::Cache(&archive),
                                    )? {
                                        let cached_dist = CachedDirectUrlDist::from_url(
                                            wheel.filename,
                                            wheel.url,
                                            archive,
                                        );

                                        debug!(
                                            "URL wheel requirement already cached: {cached_dist}"
                                        );
                                        local.push(CachedDist::Url(cached_dist));
                                        continue;
                                    }
                                }
                                Err(err) if err.kind() == io::ErrorKind::NotFound => {
                                    // The cache entry doesn't exist, so it's not fresh.
                                }
                                Err(err) => return Err(err.into()),
                            }
                        }
                        Dist::Source(SourceDist::DirectUrl(sdist)) => {
                            // Find the most-compatible wheel from the cache, since we don't know
                            // the filename in advance.
                            if let Some(wheel) = BuiltWheelIndex::url(&sdist, cache, tags)? {
                                let cached_dist = wheel.into_url_dist(url.clone());
                                debug!("URL source requirement already cached: {cached_dist}");
                                local.push(CachedDist::Url(cached_dist));
                                continue;
                            }
                        }
                        Dist::Source(SourceDist::Path(sdist)) => {
                            // Find the most-compatible wheel from the cache, since we don't know
                            // the filename in advance.
                            if let Some(wheel) = BuiltWheelIndex::path(&sdist, cache, tags)? {
                                let cached_dist = wheel.into_url_dist(url.clone());
                                debug!("Path source requirement already cached: {cached_dist}");
                                local.push(CachedDist::Url(cached_dist));
                                continue;
                            }
                        }
                        Dist::Source(SourceDist::Git(sdist)) => {
                            // Find the most-compatible wheel from the cache, since we don't know
                            // the filename in advance.
                            if let Some(wheel) = BuiltWheelIndex::git(&sdist, cache, tags) {
                                let cached_dist = wheel.into_url_dist(url.clone());
                                debug!("Git source requirement already cached: {cached_dist}");
                                local.push(CachedDist::Url(cached_dist));
                                continue;
                            }
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
            local,
            remote,
            reinstalls,
            extraneous,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Specifier<'a> {
    /// An editable requirement, marked by the installed version of the package.
    Editable(InstalledVersion<'a>),
    /// A non-editable requirement, marked by the version or URL specifier.
    NonEditable(Option<&'a VersionOrUrl>),
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

#[derive(Debug, Clone)]
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

    /// Returns `true` if all packages should be reinstalled.
    pub fn is_all(&self) -> bool {
        matches!(self, Self::All)
    }
}
