use std::borrow::Cow;
use std::collections::hash_map::Entry;

use rustc_hash::{FxHashMap, FxHashSet};

use uv_cache::{Cache, CacheBucket, WheelCache};
use uv_cache_info::CacheInfo;
use uv_distribution_types::{
    BuildInfo, BuildVariables, CachedRegistryDist, ConfigSettings, ExtraBuildRequirement,
    ExtraBuildRequires, ExtraBuildVariables, Hashed, Index, IndexLocations, IndexUrl,
    PackageConfigSettings,
};
use uv_fs::{directories, files};
use uv_normalize::PackageName;
use uv_platform_tags::Tags;
use uv_types::HashStrategy;

use crate::index::cached_wheel::{CachedWheel, ResolvedWheel};
use crate::source::{HTTP_REVISION, HttpRevisionPointer, LOCAL_REVISION, LocalRevisionPointer};

/// An entry in the [`RegistryWheelIndex`].
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct IndexEntry<'index> {
    /// The cached distribution.
    pub dist: CachedRegistryDist,
    /// Whether the wheel was built from source (true), or downloaded from the registry directly (false).
    pub built: bool,
    /// The index from which the wheel was downloaded.
    pub index: &'index Index,
}

/// A local index of distributions that originate from a registry, like `PyPI`.
#[derive(Debug)]
pub struct RegistryWheelIndex<'a> {
    cache: &'a Cache,
    tags: &'a Tags,
    index_locations: &'a IndexLocations,
    hasher: &'a HashStrategy,
    index: FxHashMap<&'a PackageName, Vec<IndexEntry<'a>>>,
    config_settings: &'a ConfigSettings,
    config_settings_package: &'a PackageConfigSettings,
    extra_build_requires: &'a ExtraBuildRequires,
    extra_build_variables: &'a ExtraBuildVariables,
}

impl<'a> RegistryWheelIndex<'a> {
    /// Initialize an index of registry distributions.
    pub fn new(
        cache: &'a Cache,
        tags: &'a Tags,
        index_locations: &'a IndexLocations,
        hasher: &'a HashStrategy,
        config_settings: &'a ConfigSettings,
        config_settings_package: &'a PackageConfigSettings,
        extra_build_requires: &'a ExtraBuildRequires,
        extra_build_variables: &'a ExtraBuildVariables,
    ) -> Self {
        Self {
            cache,
            tags,
            index_locations,
            hasher,
            config_settings,
            config_settings_package,
            extra_build_requires,
            extra_build_variables,
            index: FxHashMap::default(),
        }
    }

    /// Return an iterator over available wheels for a given package.
    ///
    /// If the package is not yet indexed, this will index the package by reading from the cache.
    pub fn get(&mut self, name: &'a PackageName) -> impl Iterator<Item = &IndexEntry<'_>> {
        self.get_impl(name).iter().rev()
    }

    /// Get an entry in the index.
    fn get_impl(&mut self, name: &'a PackageName) -> &[IndexEntry<'_>] {
        (match self.index.entry(name) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(Self::index(
                name,
                self.cache,
                self.tags,
                self.index_locations,
                self.hasher,
                self.config_settings,
                self.config_settings_package,
                self.extra_build_requires,
                self.extra_build_variables,
            )),
        }) as _
    }

    /// Add a package to the index by reading from the cache.
    fn index<'index>(
        package: &PackageName,
        cache: &Cache,
        tags: &Tags,
        index_locations: &'index IndexLocations,
        hasher: &HashStrategy,
        config_settings: &ConfigSettings,
        config_settings_package: &PackageConfigSettings,
        extra_build_requires: &ExtraBuildRequires,
        extra_build_variables: &ExtraBuildVariables,
    ) -> Vec<IndexEntry<'index>> {
        let mut entries = vec![];

        let mut seen = FxHashSet::default();
        for index in index_locations.allowed_indexes() {
            if !seen.insert(index.url()) {
                continue;
            }

            // Index all the wheels that were downloaded directly from the registry.
            let wheel_dir = cache.shard(
                CacheBucket::Wheels,
                WheelCache::Index(index.url()).wheel_dir(package.as_ref()),
            );

            // For registry wheels, the cache structure is: `<index>/<package-name>/<wheel>.http`
            // or `<index>/<package-name>/<version>/<wheel>.rev`.
            for file in files(&wheel_dir).ok().into_iter().flatten() {
                match index.url() {
                    // Add files from remote registries.
                    IndexUrl::Pypi(_) | IndexUrl::Url(_) => {
                        if file
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("http"))
                        {
                            if let Some(wheel) =
                                CachedWheel::from_http_pointer(wheel_dir.join(file), cache)
                            {
                                if wheel.filename.compatibility(tags).is_compatible() {
                                    // Enforce hash-checking based on the built distribution.
                                    if wheel.satisfies(
                                        hasher.get_package(
                                            &wheel.filename.name,
                                            &wheel.filename.version,
                                        ),
                                    ) {
                                        entries.push(IndexEntry {
                                            dist: wheel.into_registry_dist(),
                                            index,
                                            built: false,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    // Add files from local registries (e.g., `--find-links`).
                    IndexUrl::Path(_) => {
                        if file
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("rev"))
                        {
                            if let Some(wheel) =
                                CachedWheel::from_local_pointer(wheel_dir.join(file), cache)
                            {
                                if wheel.filename.compatibility(tags).is_compatible() {
                                    // Enforce hash-checking based on the built distribution.
                                    if wheel.satisfies(
                                        hasher.get_package(
                                            &wheel.filename.name,
                                            &wheel.filename.version,
                                        ),
                                    ) {
                                        entries.push(IndexEntry {
                                            dist: wheel.into_registry_dist(),
                                            index,
                                            built: false,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Index all the built wheels, created by downloading and building source distributions
            // from the registry.
            let cache_shard = cache.shard(
                CacheBucket::SourceDistributions,
                WheelCache::Index(index.url()).wheel_dir(package.as_ref()),
            );

            // For registry source distributions, the cache structure is: `<index>/<package-name>/<version>/`.
            for shard in directories(&cache_shard).ok().into_iter().flatten() {
                let cache_shard = cache_shard.shard(shard);

                // Read the revision from the cache.
                let revision = match index.url() {
                    // Add files from remote registries.
                    IndexUrl::Pypi(_) | IndexUrl::Url(_) => {
                        let revision_entry = cache_shard.entry(HTTP_REVISION);
                        if let Ok(Some(pointer)) = HttpRevisionPointer::read_from(revision_entry) {
                            Some(pointer.into_revision())
                        } else {
                            None
                        }
                    }
                    // Add files from local registries (e.g., `--find-links`).
                    IndexUrl::Path(_) => {
                        let revision_entry = cache_shard.entry(LOCAL_REVISION);
                        if let Ok(Some(pointer)) = LocalRevisionPointer::read_from(revision_entry) {
                            Some(pointer.into_revision())
                        } else {
                            None
                        }
                    }
                };

                if let Some(revision) = revision {
                    let cache_shard = cache_shard.shard(revision.id());

                    // If there are build settings, we need to scope to a cache shard.
                    let extra_build_deps =
                        Self::extra_build_requires_for(package, extra_build_requires);
                    let extra_build_vars =
                        Self::extra_build_variables_for(package, extra_build_variables);
                    let config_settings = Self::config_settings_for(
                        package,
                        config_settings,
                        config_settings_package,
                    );
                    let build_info = BuildInfo::from_settings(
                        &config_settings,
                        extra_build_deps,
                        extra_build_vars,
                    );
                    let cache_shard = build_info
                        .cache_shard()
                        .map(|digest| cache_shard.shard(digest))
                        .unwrap_or(cache_shard);

                    for wheel_dir in uv_fs::entries(cache_shard).ok().into_iter().flatten() {
                        // Ignore any `.lock` files.
                        if wheel_dir
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("lock"))
                        {
                            continue;
                        }

                        if let Some(wheel) = ResolvedWheel::from_built_source(wheel_dir, cache) {
                            if wheel.filename.compatibility(tags).is_compatible() {
                                // Enforce hash-checking based on the source distribution.
                                if revision.satisfies(
                                    hasher
                                        .get_package(&wheel.filename.name, &wheel.filename.version),
                                ) {
                                    let wheel = CachedWheel::from_entry(
                                        wheel,
                                        revision.hashes().into(),
                                        CacheInfo::default(),
                                        build_info.clone(),
                                    );
                                    entries.push(IndexEntry {
                                        dist: wheel.into_registry_dist(),
                                        index,
                                        built: true,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // Sort the cached distributions by (1) version, (2) compatibility, and (3) build status.
        // We want the highest versions, with the greatest compatibility, that were built from source.
        // at the end of the list.
        entries.sort_unstable_by(|a, b| {
            a.dist
                .filename
                .version
                .cmp(&b.dist.filename.version)
                .then_with(|| {
                    a.dist
                        .filename
                        .compatibility(tags)
                        .cmp(&b.dist.filename.compatibility(tags))
                        .then_with(|| a.built.cmp(&b.built))
                })
        });

        entries
    }

    /// Determine the [`ConfigSettings`] for the given package name.
    fn config_settings_for<'settings>(
        name: &PackageName,
        config_settings: &'settings ConfigSettings,
        config_settings_package: &PackageConfigSettings,
    ) -> Cow<'settings, ConfigSettings> {
        if let Some(package_settings) = config_settings_package.get(name) {
            Cow::Owned(package_settings.clone().merge(config_settings.clone()))
        } else {
            Cow::Borrowed(config_settings)
        }
    }

    /// Determine the extra build requirements for the given package name.
    fn extra_build_requires_for<'settings>(
        name: &PackageName,
        extra_build_requires: &'settings ExtraBuildRequires,
    ) -> &'settings [ExtraBuildRequirement] {
        extra_build_requires
            .get(name)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Determine the extra build variables for the given package name.
    fn extra_build_variables_for<'settings>(
        name: &PackageName,
        extra_build_variables: &'settings ExtraBuildVariables,
    ) -> Option<&'settings BuildVariables> {
        extra_build_variables.get(name)
    }
}
