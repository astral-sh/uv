use std::sync::Arc;

use anyhow::{Result, bail};
use owo_colors::OwoColorize;
use tracing::{debug, warn};

use uv_cache::{Cache, CacheBucket, WheelCache};
use uv_cache_info::Timestamp;
use uv_configuration::{BuildOptions, Reinstall};
use uv_distribution::{
    BuiltWheelIndex, HttpArchivePointer, LocalArchivePointer, RegistryWheelIndex,
};
use uv_distribution_filename::WheelFilename;
use uv_distribution_types::{
    BuiltDist, CachedDirectUrlDist, CachedDist, ConfigSettings, Dist, Error, ExtraBuildRequires,
    ExtraBuildVariables, Hashed, IndexLocations, InstalledDist, Name, PackageConfigSettings,
    RemoteSource, RequirementSource, Resolution, ResolvedDist, SourceDist,
};
use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_platform_tags::{IncompatibleTag, TagCompatibility, Tags};
use uv_pypi_types::VerbatimParsedUrl;
use uv_python::PythonEnvironment;
use uv_types::HashStrategy;

use crate::SitePackages;
use crate::satisfies::RequirementSatisfaction;

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
        config_settings_package: &PackageConfigSettings,
        extra_build_requires: &ExtraBuildRequires,
        extra_build_variables: &ExtraBuildVariables,
        cache: &Cache,
        venv: &PythonEnvironment,
        tags: &Tags,
    ) -> Result<Plan> {
        // Index all the already-downloaded wheels in the cache.
        let mut registry_index = RegistryWheelIndex::new(
            cache,
            tags,
            index_locations,
            hasher,
            config_settings,
            config_settings_package,
            extra_build_requires,
            extra_build_variables,
        );
        let built_index = BuiltWheelIndex::new(
            cache,
            tags,
            hasher,
            config_settings,
            config_settings_package,
            extra_build_requires,
            extra_build_variables,
        );

        let mut cached = vec![];
        let mut remote = vec![];
        let mut reinstalls = vec![];
        let mut extraneous = vec![];

        // TODO(charlie): There are a few assumptions here that are hard to spot:
        //
        // 1. Apparently, we never return direct URL distributions as [`ResolvedDist::Installed`].
        //    If you trace the resolver, we only ever return [`ResolvedDist::Installed`] if you go
        //    through the [`CandidateSelector`], and we only go through the [`CandidateSelector`]
        //    for registry distributions.
        //
        // 2. We expect any distribution returned as [`ResolvedDist::Installed`] to hit the
        //    "Requirement already installed" path (hence the `unreachable!`) a few lines below it.
        //    So, e.g., if a package is marked as `--reinstall`, we _expect_ that it's not passed in
        //    as [`ResolvedDist::Installed`] here.
        for dist in self.resolution.distributions() {
            // Check if the package should be reinstalled.
            let reinstall = reinstall.contains_package(dist.name())
                || dist
                    .source_tree()
                    .is_some_and(|source_tree| reinstall.contains_path(source_tree));

            // Check if installation of a binary version of the package should be allowed.
            let no_binary = build_options.no_binary_package(dist.name());
            let no_build = build_options.no_build_package(dist.name());

            // Determine whether the distribution is already installed.
            let installed_dists = site_packages.remove_packages(dist.name());
            if reinstall {
                reinstalls.extend(installed_dists);
            } else {
                match installed_dists.as_slice() {
                    [] => {}
                    [installed] => {
                        let source = RequirementSource::from(dist);
                        match RequirementSatisfaction::check(
                            dist.name(),
                            installed,
                            &source,
                            tags,
                            config_settings,
                            config_settings_package,
                            extra_build_requires,
                            extra_build_variables,
                        ) {
                            RequirementSatisfaction::Mismatch => {
                                debug!(
                                    "Requirement installed, but mismatched:\n  Installed: {installed:?}\n  Requested: {source:?}"
                                );
                            }
                            RequirementSatisfaction::Satisfied => {
                                debug!("Requirement already installed: {installed}");
                                continue;
                            }
                            RequirementSatisfaction::OutOfDate => {
                                debug!("Requirement installed, but not fresh: {installed}");

                                // If we made it here, something went wrong in the resolver, because it returned an
                                // already-installed distribution that we "shouldn't" use. Typically, this means the
                                // distribution was considered out-of-date, but in a way that the resolver didn't
                                // detect, and is indicative of drift between the resolver's candidate selector and
                                // the install plan. For example, at present, the resolver doesn't check that an
                                // installed distribution was built with the expected build settings. Treat it as
                                // up-to-date for now; it's just means we may not rebuild a package when we otherwise
                                // should. This is a known issue, but should only affect the `uv pip` CLI, as the
                                // project APIs never return installed distributions during resolution (i.e., the
                                // resolver is stateless).
                                // TODO(charlie): Incorporate these checks into the resolver.
                                if matches!(dist, ResolvedDist::Installed { .. }) {
                                    warn!(
                                        "Installed distribution was considered out-of-date, but returned by the resolver: {dist}"
                                    );
                                    continue;
                                }
                            }
                            RequirementSatisfaction::CacheInvalid => {
                                // Already logged
                            }
                        }
                        reinstalls.push(installed.clone());
                    }
                    // We reinstall installed distributions with multiple versions because
                    // we do not want to keep multiple incompatible versions but removing
                    // one version is likely to break another.
                    _ => reinstalls.extend(installed_dists),
                }
            }

            let ResolvedDist::Installable { dist, .. } = dist else {
                unreachable!("Installed distribution could not be found in site-packages: {dist}");
            };

            if cache.must_revalidate_package(dist.name())
                || dist
                    .source_tree()
                    .is_some_and(|source_tree| cache.must_revalidate_path(source_tree))
            {
                debug!("Must revalidate requirement: {}", dist.name());
                remote.push(dist.clone());
                continue;
            }

            // Identify any cached distributions that satisfy the requirement.
            match dist.as_ref() {
                Dist::Built(BuiltDist::Registry(wheel)) => {
                    if let Some(distribution) = registry_index.get(wheel.name()).find_map(|entry| {
                        if *entry.index.url() != wheel.best_wheel().index {
                            return None;
                        }
                        if entry.dist.filename != wheel.best_wheel().filename {
                            return None;
                        }
                        if entry.built && no_build {
                            return None;
                        }
                        if !entry.built && no_binary {
                            return None;
                        }
                        Some(&entry.dist)
                    }) {
                        debug!(
                            "Registry requirement already cached: {distribution} ({})",
                            wheel.best_wheel().filename
                        );
                        cached.push(CachedDist::Registry(distribution.clone()));
                        continue;
                    }
                }
                Dist::Built(BuiltDist::DirectUrl(wheel)) => {
                    if !wheel.filename.is_compatible(tags) {
                        let hint = generate_wheel_compatibility_hint(&wheel.filename, tags);
                        if let Some(hint) = hint {
                            bail!(
                                "A URL dependency is incompatible with the current platform: {}\n\n{}{} {}",
                                wheel.url,
                                "hint".bold().cyan(),
                                ":".bold(),
                                hint
                            );
                        }
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
                        .entry(format!("{}.http", wheel.filename.cache_key()));

                    // Read the HTTP pointer.
                    match HttpArchivePointer::read_from(&cache_entry) {
                        Ok(Some(pointer)) => {
                            let cache_info = pointer.to_cache_info();
                            let build_info = pointer.to_build_info();
                            let archive = pointer.into_archive();
                            if archive.satisfies(hasher.get(dist.as_ref())) {
                                let cached_dist = CachedDirectUrlDist {
                                    filename: wheel.filename.clone(),
                                    url: VerbatimParsedUrl {
                                        parsed_url: wheel.parsed_url(),
                                        verbatim: wheel.url.clone(),
                                    },
                                    hashes: archive.hashes,
                                    cache_info,
                                    build_info,
                                    path: cache.archive(&archive.id).into_boxed_path(),
                                };

                                debug!("URL wheel requirement already cached: {cached_dist}");
                                cached.push(CachedDist::Url(cached_dist));
                                continue;
                            }
                            debug!(
                                "Cached URL wheel requirement does not match expected hash policy for: {wheel}"
                            );
                        }
                        Ok(None) => {}
                        Err(err) => {
                            debug!(
                                "Failed to deserialize cached URL wheel requirement for: {wheel} ({err})"
                            );
                        }
                    }
                }
                Dist::Built(BuiltDist::Path(wheel)) => {
                    // Validate that the path exists.
                    if !wheel.install_path.exists() {
                        return Err(Error::NotFound(wheel.url.to_url()).into());
                    }

                    if !wheel.filename.is_compatible(tags) {
                        let hint = generate_wheel_compatibility_hint(&wheel.filename, tags);
                        if let Some(hint) = hint {
                            bail!(
                                "A path dependency is incompatible with the current platform: {}\n\n{}{} {}",
                                wheel.install_path.user_display(),
                                "hint".bold().cyan(),
                                ":".bold(),
                                hint
                            );
                        }
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
                        .entry(format!("{}.rev", wheel.filename.cache_key()));

                    match LocalArchivePointer::read_from(&cache_entry) {
                        Ok(Some(pointer)) => match Timestamp::from_path(&wheel.install_path) {
                            Ok(timestamp) => {
                                if pointer.is_up_to_date(timestamp) {
                                    let cache_info = pointer.to_cache_info();
                                    let build_info = pointer.to_build_info();
                                    let archive = pointer.into_archive();
                                    if archive.satisfies(hasher.get(dist.as_ref())) {
                                        let cached_dist = CachedDirectUrlDist {
                                            filename: wheel.filename.clone(),
                                            url: VerbatimParsedUrl {
                                                parsed_url: wheel.parsed_url(),
                                                verbatim: wheel.url.clone(),
                                            },
                                            hashes: archive.hashes,
                                            cache_info,
                                            build_info,
                                            path: cache.archive(&archive.id).into_boxed_path(),
                                        };

                                        debug!(
                                            "Path wheel requirement already cached: {cached_dist}"
                                        );
                                        cached.push(CachedDist::Url(cached_dist));
                                        continue;
                                    }
                                    debug!(
                                        "Cached path wheel requirement does not match expected hash policy for: {wheel}"
                                    );
                                }
                            }
                            Err(err) => {
                                debug!("Failed to get timestamp for wheel {wheel} ({err})");
                            }
                        },
                        Ok(None) => {}
                        Err(err) => {
                            debug!(
                                "Failed to deserialize cached path wheel requirement for: {wheel} ({err})"
                            );
                        }
                    }
                }
                Dist::Source(SourceDist::Registry(sdist)) => {
                    if let Some(distribution) = registry_index.get(sdist.name()).find_map(|entry| {
                        if *entry.index.url() != sdist.index {
                            return None;
                        }
                        if entry.dist.filename.name != sdist.name {
                            return None;
                        }
                        if entry.dist.filename.version != sdist.version {
                            return None;
                        }
                        if entry.built && no_build {
                            return None;
                        }
                        if !entry.built && no_binary {
                            return None;
                        }
                        Some(&entry.dist)
                    }) {
                        debug!("Registry requirement already cached: {distribution}");
                        cached.push(CachedDist::Registry(distribution.clone()));
                        continue;
                    }
                }
                Dist::Source(SourceDist::DirectUrl(sdist)) => {
                    // Find the most-compatible wheel from the cache, since we don't know
                    // the filename in advance.
                    match built_index.url(sdist) {
                        Ok(Some(wheel)) => {
                            if wheel.filename.name == sdist.name {
                                let cached_dist = wheel.into_url_dist(sdist);
                                debug!("URL source requirement already cached: {cached_dist}");
                                cached.push(CachedDist::Url(cached_dist));
                                continue;
                            }

                            warn!(
                                "Cached wheel filename does not match requested distribution for: `{}` (found: `{}`)",
                                sdist, wheel.filename
                            );
                        }
                        Ok(None) => {}
                        Err(err) => {
                            debug!(
                                "Failed to deserialize cached wheel filename for: {sdist} ({err})"
                            );
                        }
                    }
                }
                Dist::Source(SourceDist::Git(sdist)) => {
                    // Find the most-compatible wheel from the cache, since we don't know
                    // the filename in advance.
                    if let Some(wheel) = built_index.git(sdist) {
                        if wheel.filename.name == sdist.name {
                            let cached_dist = wheel.into_git_dist(sdist);
                            debug!("Git source requirement already cached: {cached_dist}");
                            cached.push(CachedDist::Url(cached_dist));
                            continue;
                        }

                        warn!(
                            "Cached wheel filename does not match requested distribution for: `{}` (found: `{}`)",
                            sdist, wheel.filename
                        );
                    }
                }
                Dist::Source(SourceDist::Path(sdist)) => {
                    // Validate that the path exists.
                    if !sdist.install_path.exists() {
                        return Err(Error::NotFound(sdist.url.to_url()).into());
                    }

                    // Find the most-compatible wheel from the cache, since we don't know
                    // the filename in advance.
                    match built_index.path(sdist) {
                        Ok(Some(wheel)) => {
                            if wheel.filename.name == sdist.name {
                                let cached_dist = wheel.into_path_dist(sdist);
                                debug!("Path source requirement already cached: {cached_dist}");
                                cached.push(CachedDist::Url(cached_dist));
                                continue;
                            }

                            warn!(
                                "Cached wheel filename does not match requested distribution for: `{}` (found: `{}`)",
                                sdist, wheel.filename
                            );
                        }
                        Ok(None) => {}
                        Err(err) => {
                            debug!(
                                "Failed to deserialize cached wheel filename for: {sdist} ({err})"
                            );
                        }
                    }
                }
                Dist::Source(SourceDist::Directory(sdist)) => {
                    // Validate that the path exists.
                    if !sdist.install_path.exists() {
                        return Err(Error::NotFound(sdist.url.to_url()).into());
                    }

                    // Find the most-compatible wheel from the cache, since we don't know
                    // the filename in advance.
                    match built_index.directory(sdist) {
                        Ok(Some(wheel)) => {
                            if wheel.filename.name == sdist.name {
                                let cached_dist = wheel.into_directory_dist(sdist);
                                debug!(
                                    "Directory source requirement already cached: {cached_dist}"
                                );
                                cached.push(CachedDist::Url(cached_dist));
                                continue;
                            }

                            warn!(
                                "Cached wheel filename does not match requested distribution for: `{}` (found: `{}`)",
                                sdist, wheel.filename
                            );
                        }
                        Ok(None) => {}
                        Err(err) => {
                            debug!(
                                "Failed to deserialize cached wheel filename for: {sdist} ({err})"
                            );
                        }
                    }
                }
            }

            if let Ok(filename) = dist.filename() {
                debug!("Identified uncached distribution: {dist} ({filename})");
            } else {
                debug!("Identified uncached distribution: {dist}");
            }
            remote.push(dist.clone());
        }

        // Remove any unnecessary packages.
        if site_packages.any() {
            // Retain seed packages unless: (1) the virtual environment was created by uv and
            // (2) the `--seed` argument was not passed to `uv venv`.
            let seed_packages = !venv.cfg().is_ok_and(|cfg| cfg.is_uv() && !cfg.is_seed());
            for dist_info in site_packages {
                if seed_packages && is_seed_package(&dist_info, venv) {
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

/// Returns `true` if the given distribution is a seed package.
fn is_seed_package(dist_info: &InstalledDist, venv: &PythonEnvironment) -> bool {
    if venv.interpreter().python_tuple() >= (3, 12) {
        matches!(dist_info.name().as_ref(), "uv" | "pip")
    } else {
        // Include `setuptools` and `wheel` on Python <3.12.
        matches!(
            dist_info.name().as_ref(),
            "pip" | "setuptools" | "wheel" | "uv"
        )
    }
}

/// Generate a hint for explaining wheel compatibility issues.
fn generate_wheel_compatibility_hint(filename: &WheelFilename, tags: &Tags) -> Option<String> {
    let TagCompatibility::Incompatible(incompatible_tag) = filename.compatibility(tags) else {
        return None;
    };

    match incompatible_tag {
        IncompatibleTag::Python => {
            let wheel_tags = filename.python_tags();
            let current_tag = tags.python_tag();

            if let Some(current) = current_tag {
                let message = if let Some(pretty) = current.pretty() {
                    format!("{} (`{}`)", pretty.cyan(), current.cyan())
                } else {
                    format!("`{}`", current.cyan())
                };

                Some(format!(
                    "The wheel is compatible with {}, but you're using {}",
                    wheel_tags
                        .iter()
                        .map(|tag| if let Some(pretty) = tag.pretty() {
                            format!("{} (`{}`)", pretty.cyan(), tag.cyan())
                        } else {
                            format!("`{}`", tag.cyan())
                        })
                        .collect::<Vec<_>>()
                        .join(", "),
                    message
                ))
            } else {
                Some(format!(
                    "The wheel requires {}",
                    wheel_tags
                        .iter()
                        .map(|tag| if let Some(pretty) = tag.pretty() {
                            format!("{} (`{}`)", pretty.cyan(), tag.cyan())
                        } else {
                            format!("`{}`", tag.cyan())
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }
        }
        IncompatibleTag::Abi => {
            let wheel_tags = filename.abi_tags();
            let current_tag = tags.abi_tag();

            if let Some(current) = current_tag {
                let message = if let Some(pretty) = current.pretty() {
                    format!("{} (`{}`)", pretty.cyan(), current.cyan())
                } else {
                    format!("`{}`", current.cyan())
                };
                Some(format!(
                    "The wheel is compatible with {}, but you're using {}",
                    wheel_tags
                        .iter()
                        .map(|tag| if let Some(pretty) = tag.pretty() {
                            format!("{} (`{}`)", pretty.cyan(), tag.cyan())
                        } else {
                            format!("`{}`", tag.cyan())
                        })
                        .collect::<Vec<_>>()
                        .join(", "),
                    message
                ))
            } else {
                Some(format!(
                    "The wheel requires {}",
                    wheel_tags
                        .iter()
                        .map(|tag| if let Some(pretty) = tag.pretty() {
                            format!("{} (`{}`)", pretty.cyan(), tag.cyan())
                        } else {
                            format!("`{}`", tag.cyan())
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }
        }
        IncompatibleTag::Platform => {
            let wheel_tags = filename.platform_tags();
            let current_tag = tags.platform_tag();

            if let Some(current) = current_tag {
                let message = if let Some(pretty) = current.pretty() {
                    format!("{} (`{}`)", pretty.cyan(), current.cyan())
                } else {
                    format!("`{}`", current.cyan())
                };
                Some(format!(
                    "The wheel is compatible with {}, but you're on {}",
                    wheel_tags
                        .iter()
                        .map(|tag| if let Some(pretty) = tag.pretty() {
                            format!("{} (`{}`)", pretty.cyan(), tag.cyan())
                        } else {
                            format!("`{}`", tag.cyan())
                        })
                        .collect::<Vec<_>>()
                        .join(", "),
                    message
                ))
            } else {
                Some(format!(
                    "The wheel requires {}",
                    wheel_tags
                        .iter()
                        .map(|tag| if let Some(pretty) = tag.pretty() {
                            format!("{} (`{}`)", pretty.cyan(), tag.cyan())
                        } else {
                            format!("`{}`", tag.cyan())
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }
        }
        _ => None,
    }
}

#[derive(Debug, Default)]
pub struct Plan {
    /// The distributions that are not already installed in the current environment, but are
    /// available in the local cache.
    pub cached: Vec<CachedDist>,

    /// The distributions that are not already installed in the current environment, and are
    /// not available in the local cache.
    pub remote: Vec<Arc<Dist>>,

    /// Any distributions that are already installed in the current environment, but will be
    /// re-installed (including upgraded) to satisfy the requirements.
    pub reinstalls: Vec<InstalledDist>,

    /// Any distributions that are already installed in the current environment, and are
    /// _not_ necessary to satisfy the requirements.
    pub extraneous: Vec<InstalledDist>,
}

impl Plan {
    /// Returns `true` if the plan is empty.
    pub fn is_empty(&self) -> bool {
        self.cached.is_empty()
            && self.remote.is_empty()
            && self.reinstalls.is_empty()
            && self.extraneous.is_empty()
    }

    /// Partition the remote distributions based on a predicate function.
    ///
    /// Returns a tuple of plans, where the first plan contains the remote distributions that match
    /// the predicate, and the second plan contains those that do not.
    ///
    /// Any extraneous and cached distributions will be returned in the first plan, while the second
    /// plan will contain any `false` matches from the remote distributions, along with any
    /// reinstalls for those distributions.
    pub fn partition<F>(self, mut f: F) -> (Self, Self)
    where
        F: FnMut(&PackageName) -> bool,
    {
        let Self {
            cached,
            remote,
            reinstalls,
            extraneous,
        } = self;

        // Partition the remote distributions based on the predicate function.
        let (left_remote, right_remote) = remote
            .into_iter()
            .partition::<Vec<_>, _>(|dist| f(dist.name()));

        // If any remote distributions are not matched, but are already installed, ensure that
        // they're uninstalled as part of the right plan. (Uninstalling them as part of the left
        // plan risks uninstalling them from the environment _prior_ to the replacement being built.)
        let (left_reinstalls, right_reinstalls) = reinstalls
            .into_iter()
            .partition::<Vec<_>, _>(|dist| !right_remote.iter().any(|d| d.name() == dist.name()));

        // If the right plan is non-empty, then remove extraneous distributions as part of the
        // right plan, so they're present until the very end. Otherwise, we risk removing extraneous
        // packages that are actually build dependencies.
        let (left_extraneous, right_extraneous) = if right_remote.is_empty() {
            (extraneous, vec![])
        } else {
            (vec![], extraneous)
        };

        // Always include the cached distributions in the left plan.
        let (left_cached, right_cached) = (cached, vec![]);

        // Include all cached and extraneous distributions in the left plan.
        let left_plan = Self {
            cached: left_cached,
            remote: left_remote,
            reinstalls: left_reinstalls,
            extraneous: left_extraneous,
        };

        // The right plan will only contain the remote distributions that did not match the predicate,
        // along with any reinstalls for those distributions.
        let right_plan = Self {
            cached: right_cached,
            remote: right_remote,
            reinstalls: right_reinstalls,
            extraneous: right_extraneous,
        };

        (left_plan, right_plan)
    }
}
