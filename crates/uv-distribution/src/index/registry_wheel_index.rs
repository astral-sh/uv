use std::borrow::Cow;
use std::collections::hash_map::Entry;

use rustc_hash::{FxHashMap, FxHashSet};

use uv_cache::{Cache, CacheBucket, WheelCache};
use uv_cache_info::CacheInfo;
use uv_distribution_filename::WheelFilename;
use uv_distribution_types::{
    BuildInfo, BuildVariables, CachedRegistryDist, ConfigSettings, ExtraBuildRequirement,
    ExtraBuildRequires, ExtraBuildVariables, HashPolicy, Hashed, Index, IndexLocations, IndexRoute,
    IndexRoutes, IndexUrl, PackageConfigSettings, ProxyIndexError, RegistryBuiltDist,
    RegistrySourceDist,
};
use uv_fs::{directories, files};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_platform_tags::Tags;
use uv_pypi_types::HashDigest;
use uv_types::HashStrategy;

use crate::index::cached_wheel::{CachedWheel, ResolvedWheel};
use crate::source::{HTTP_REVISION, HttpRevisionPointer, LOCAL_REVISION, LocalRevisionPointer};

/// An entry in the [`RegistryWheelIndex`].
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct IndexEntry<'index> {
    /// The cached distribution.
    dist: CachedRegistryDist,
    /// Whether the wheel was built from source (true), or downloaded from the registry directly (false).
    built: bool,
    /// The index from which the wheel was downloaded.
    index: &'index Index,
}

impl IndexEntry<'_> {
    /// The index from which the wheel was downloaded.
    pub fn index(&self) -> &Index {
        self.index
    }

    /// Whether the wheel was built from source.
    pub fn is_built(&self) -> bool {
        self.built
    }

    /// The cached distribution.
    pub fn dist(&self) -> &CachedRegistryDist {
        &self.dist
    }
}

/// An entry loaded from one selected physical cache shard.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct PhysicalIndexEntry {
    /// The cached distribution.
    dist: CachedRegistryDist,
    /// Whether the wheel was built from source (true), or downloaded from the registry directly (false).
    built: bool,
}

impl PhysicalIndexEntry {
    fn allowed_by_build_options(&self, no_build: bool, no_binary: bool) -> bool {
        !(self.built && no_build || !self.built && no_binary)
    }

    fn matches_wheel(&self, filename: &WheelFilename, no_build: bool, no_binary: bool) -> bool {
        self.allowed_by_build_options(no_build, no_binary) && self.dist.filename == *filename
    }

    fn matches_source(
        &self,
        name: &PackageName,
        version: &Version,
        no_build: bool,
        no_binary: bool,
    ) -> bool {
        self.allowed_by_build_options(no_build, no_binary)
            && self.dist.filename.name == *name
            && self.dist.filename.version == *version
    }
}

/// Return the hashes required to admit an artifact from a proxy cache.
///
/// Hashless artifacts discovered through a proxy can use the ordinary cache path, while
/// lock-derived artifacts without proxy provenance require digest evidence.
fn proxy_validation_hashes<'hashes>(
    filename: &impl std::fmt::Display,
    hashes: &'hashes [HashDigest],
    artifact_proxy: Option<&IndexUrl>,
    route: IndexRoute<'_>,
) -> Result<Option<&'hashes [HashDigest]>, crate::Error> {
    if !route.is_proxy() {
        return Ok(None);
    }
    if hashes.is_empty() {
        if artifact_proxy.is_some() {
            return Ok(None);
        }
        return Err(crate::Error::ProxyArtifactMissingDigest {
            filename: filename.to_string(),
            canonical: route.canonical.clone(),
            proxy: route.physical.clone(),
        });
    }
    Ok(Some(hashes))
}

/// A local index of distributions that originate from a registry, like `PyPI`.
#[derive(Debug)]
pub struct RegistryWheelIndex<'a> {
    cache: &'a Cache,
    tags: &'a Tags,
    index_locations: &'a IndexLocations,
    index_routes: IndexRoutes,
    hasher: &'a HashStrategy,
    index: FxHashMap<&'a PackageName, Vec<IndexEntry<'a>>>,
    physical_index: FxHashMap<(&'a PackageName, IndexUrl), Vec<PhysicalIndexEntry>>,
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
    ) -> Result<Self, ProxyIndexError> {
        let index_routes = IndexRoutes::try_from(index_locations)?;
        Ok(Self {
            cache,
            tags,
            index_locations,
            index_routes,
            hasher,
            config_settings,
            config_settings_package,
            extra_build_requires,
            extra_build_variables,
            index: FxHashMap::default(),
            physical_index: FxHashMap::default(),
        })
    }

    /// Return a cached wheel that satisfies a registry wheel requirement, failing if its proxy
    /// route cannot safely admit a cached artifact.
    pub fn try_wheel(
        &mut self,
        wheel: &'a RegistryBuiltDist,
        no_build: bool,
        no_binary: bool,
    ) -> Result<Option<&CachedRegistryDist>, crate::Error> {
        let wheel = wheel.best_wheel();
        let route = self.index_routes.route_for(&wheel.index);
        let validation = proxy_validation_hashes(
            &wheel.filename,
            wheel.file.hashes.as_slice(),
            wheel.proxy.as_ref(),
            route,
        )?;
        let physical_index = route.physical.clone();
        Ok(self
            .get_physical(&wheel.filename.name, &physical_index)
            .find_map(|entry| {
                if validation.is_some_and(|hashes| !entry.dist.satisfies(HashPolicy::Any(hashes)))
                    || !entry.matches_wheel(&wheel.filename, no_build, no_binary)
                {
                    return None;
                }
                Some(&entry.dist)
            }))
    }

    /// Return a cached wheel that satisfies a registry source distribution requirement, failing
    /// if its proxy route cannot safely admit a cached artifact.
    pub fn try_source(
        &mut self,
        source: &'a RegistrySourceDist,
        no_build: bool,
        no_binary: bool,
    ) -> Result<Option<&CachedRegistryDist>, crate::Error> {
        let route = self.index_routes.route_for(&source.index);
        let validation = proxy_validation_hashes(
            &source.file.filename,
            source.file.hashes.as_slice(),
            source.proxy.as_ref(),
            route,
        )?;
        let physical_index = route.physical.clone();
        Ok(self
            .get_physical(&source.name, &physical_index)
            .find_map(|entry| {
                if validation.is_some_and(|hashes| !entry.dist.satisfies(HashPolicy::Any(hashes)))
                    || !entry.matches_source(&source.name, &source.version, no_build, no_binary)
                {
                    return None;
                }
                Some(&entry.dist)
            }))
    }

    /// Return an iterator over available wheels for a given package.
    ///
    /// If the package is not yet indexed, this will index the package by reading from the cache.
    pub fn get(&mut self, name: &'a PackageName) -> impl Iterator<Item = &IndexEntry<'_>> {
        self.get_impl(name).iter().rev()
    }

    /// Get the public entries for a package across the configured indexes.
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

    /// Return an iterator over available wheels for a package and physical index.
    ///
    /// If the package is not yet indexed for this endpoint, this will index the package by reading
    /// only that endpoint's cache shard.
    fn get_physical(
        &mut self,
        name: &'a PackageName,
        index: &IndexUrl,
    ) -> impl Iterator<Item = &PhysicalIndexEntry> {
        self.get_physical_impl(name, index).iter().rev()
    }

    /// Get the entries for one physical cache shard.
    fn get_physical_impl(
        &mut self,
        name: &'a PackageName,
        index: &IndexUrl,
    ) -> &[PhysicalIndexEntry] {
        (match self.physical_index.entry((name, index.clone())) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(Self::index_physical(
                name,
                index,
                self.cache,
                self.tags,
                self.hasher,
                self.config_settings,
                self.config_settings_package,
                self.extra_build_requires,
                self.extra_build_variables,
            )),
        }) as _
    }

    /// Add a package from all configured indexes for the public cache-enumeration API.
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
            entries.extend(
                Self::index_physical(
                    package,
                    index.url(),
                    cache,
                    tags,
                    hasher,
                    config_settings,
                    config_settings_package,
                    extra_build_requires,
                    extra_build_variables,
                )
                .into_iter()
                .map(|entry| IndexEntry {
                    dist: entry.dist,
                    built: entry.built,
                    index,
                }),
            );
        }

        // Preserve the public API's ordering across all configured indexes.
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

    /// Add a package from one physical endpoint to the index by reading from the cache.
    fn index_physical(
        package: &PackageName,
        index: &IndexUrl,
        cache: &Cache,
        tags: &Tags,
        hasher: &HashStrategy,
        config_settings: &ConfigSettings,
        config_settings_package: &PackageConfigSettings,
        extra_build_requires: &ExtraBuildRequires,
        extra_build_variables: &ExtraBuildVariables,
    ) -> Vec<PhysicalIndexEntry> {
        let mut entries = vec![];

        // Read exactly the selected physical endpoint's shard.
        {
            // Index all the wheels that were downloaded directly from the registry.
            let wheel_dir = cache.shard(
                CacheBucket::Wheels,
                WheelCache::Index(index).wheel_dir(package.as_ref()),
            );

            // For registry wheels, the cache structure is: `<index>/<package-name>/<wheel>.http`
            // or `<index>/<package-name>/<version>/<wheel>.rev`.
            for file in files(&wheel_dir).ok().into_iter().flatten() {
                match index {
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
                                        entries.push(PhysicalIndexEntry {
                                            dist: wheel.into_registry_dist(),
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
                                        entries.push(PhysicalIndexEntry {
                                            dist: wheel.into_registry_dist(),
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
                WheelCache::Index(index).wheel_dir(package.as_ref()),
            );

            // For registry source distributions, the cache structure is: `<index>/<package-name>/<version>/`.
            for shard in directories(&cache_shard).ok().into_iter().flatten() {
                let cache_shard = cache_shard.shard(shard);

                // Read the revision from the cache.
                let revision = match index {
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
                        config_settings.into_owned(),
                        extra_build_deps.to_vec(),
                        extra_build_vars.cloned(),
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
                                    entries.push(PhysicalIndexEntry {
                                        dist: wheel.into_registry_dist(),
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

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::str::FromStr;

    use anyhow::anyhow;
    use uv_distribution_filename::SourceDistExtension;
    use uv_distribution_types::{
        ArtifactUrlMap, File, FileLocation, Index, IndexReference, ProxyIndex, RegistryBuiltWheel,
    };
    use uv_platform_tags::{Arch, Os, Platform, TagsOptions};
    use uv_pypi_types::HashDigests;
    use uv_redacted::DisplaySafeUrl;

    use super::*;

    fn entry(
        filename: &WheelFilename,
        built: bool,
        path: &str,
        hashes: &HashDigests,
    ) -> PhysicalIndexEntry {
        PhysicalIndexEntry {
            dist: CachedRegistryDist {
                filename: filename.clone(),
                path: PathBuf::from(path).into_boxed_path(),
                hashes: hashes.clone(),
                cache_info: CacheInfo::default(),
                build_info: None,
            },
            built,
        }
    }

    fn registry_file(filename: &str, hashes: &HashDigests) -> File {
        let base = "https://canonical.example/simple/example/".into();
        File {
            dist_info_metadata: false,
            filename: filename.into(),
            hashes: hashes.clone(),
            requires_python: None,
            size: None,
            upload_time_utc_ms: None,
            url: FileLocation::new(filename.into(), &base),
            yanked: None,
            zstd: None,
        }
    }

    fn registry_wheel(
        canonical: &IndexUrl,
        previous_proxy: Option<&IndexUrl>,
        filename: &WheelFilename,
        hashes: &HashDigests,
    ) -> RegistryBuiltDist {
        RegistryBuiltDist {
            wheels: vec![RegistryBuiltWheel {
                filename: filename.clone(),
                file: Box::new(registry_file(&filename.to_string(), hashes)),
                index: canonical.clone(),
                proxy: previous_proxy.cloned(),
            }],
            best_wheel_index: 0,
            sdist: None,
        }
    }

    fn registry_source(
        canonical: &IndexUrl,
        previous_proxy: Option<&IndexUrl>,
        name: &PackageName,
        version: &Version,
        hashes: &HashDigests,
    ) -> RegistrySourceDist {
        RegistrySourceDist {
            name: name.clone(),
            version: version.clone(),
            file: Box::new(registry_file("example-1.0.tar.gz", hashes)),
            ext: SourceDistExtension::TarGz,
            index: canonical.clone(),
            proxy: previous_proxy.cloned(),
            wheels: Vec::new(),
        }
    }

    #[test]
    fn proxy_cache_isolated_to_current_route() -> anyhow::Result<()> {
        let canonical = IndexUrl::from_str("https://canonical.example/simple")?;
        let proxy_a = IndexUrl::from_str("https://proxy-a.example/simple")?;
        let proxy_b = IndexUrl::from_str("https://proxy-b.example/simple")?;
        let wheel_filename = WheelFilename::from_str("example-1.0-py3-none-any.whl")?;
        let built_filename = WheelFilename::from_str("example-1.0-py2-none-any.whl")?;
        let other_filename = WheelFilename::from_str("example-2.0-py3-none-any.whl")?;
        let package_name = PackageName::from_str("example")?;
        let version = Version::from_str("1.0")?;
        let hashes = HashDigests::from(HashDigest::from_str(
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )?);
        let wrong_hashes = HashDigests::from(HashDigest::from_str(
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )?);
        let empty_hashes = HashDigests::empty();

        let wheel = registry_wheel(&canonical, Some(&proxy_a), &wheel_filename, &hashes);
        let source = registry_source(&canonical, Some(&proxy_a), &package_name, &version, &hashes);
        let live_hashless_wheel =
            registry_wheel(&canonical, Some(&proxy_a), &wheel_filename, &empty_hashes);
        let live_hashless_source = registry_source(
            &canonical,
            Some(&proxy_a),
            &package_name,
            &version,
            &empty_hashes,
        );
        let locked_hashless_wheel =
            registry_wheel(&canonical, None, &wheel_filename, &empty_hashes);
        let locked_hashless_source =
            registry_source(&canonical, None, &package_name, &version, &empty_hashes);

        let index_locations =
            IndexLocations::new(vec![Index::from(canonical.clone())], Vec::new(), false)
                .with_proxy_indexes(vec![ProxyIndex {
                    index: IndexReference::Url(canonical.clone()),
                    url: proxy_b.clone(),
                    artifact_url_map: ArtifactUrlMap::single(
                        DisplaySafeUrl::parse("https://proxy-b.example/files")?,
                        DisplaySafeUrl::parse("https://canonical.example/files")?,
                    ),
                }]);
        let platform = Platform::new(
            Os::Manylinux {
                major: 2,
                minor: 28,
            },
            Arch::X86_64,
        );
        let tags = Tags::from_env(
            platform,
            (3, 12),
            "cpython",
            (3, 12),
            TagsOptions::default(),
        )?;
        let cache = Cache::temp()?;
        let hasher = HashStrategy::default();
        let config_settings = ConfigSettings::default();
        let config_settings_package = PackageConfigSettings::default();
        let extra_build_requires = ExtraBuildRequires::default();
        let extra_build_variables = ExtraBuildVariables::default();
        let mut wheel_index = RegistryWheelIndex::new(
            &cache,
            &tags,
            &index_locations,
            &hasher,
            &config_settings,
            &config_settings_package,
            &extra_build_requires,
            &extra_build_variables,
        )?;

        wheel_index.physical_index.insert(
            (&package_name, canonical.clone()),
            vec![
                entry(&wheel_filename, false, "canonical", &hashes),
                entry(&built_filename, true, "canonical-source", &hashes),
            ],
        );
        wheel_index.physical_index.insert(
            (&package_name, proxy_a.clone()),
            vec![
                entry(&wheel_filename, false, "proxy-a", &hashes),
                entry(&built_filename, true, "proxy-a-source", &hashes),
            ],
        );
        wheel_index.physical_index.insert(
            (&package_name, proxy_b.clone()),
            vec![
                entry(
                    &wheel_filename,
                    false,
                    "proxy-b-wrong-digest",
                    &wrong_hashes,
                ),
                entry(
                    &built_filename,
                    true,
                    "proxy-b-source-wrong-digest",
                    &wrong_hashes,
                ),
            ],
        );

        assert!(wheel_index.try_wheel(&wheel, true, false)?.is_none());
        assert!(wheel_index.try_source(&source, false, true)?.is_none());

        wheel_index.physical_index.insert(
            (&package_name, proxy_b.clone()),
            vec![
                entry(
                    &wheel_filename,
                    false,
                    "proxy-b-wrong-digest",
                    &wrong_hashes,
                ),
                entry(&wheel_filename, false, "proxy-b-wheel", &hashes),
                entry(&built_filename, true, "proxy-b-source", &hashes),
                entry(&other_filename, false, "proxy-b-wrong-version", &hashes),
            ],
        );

        let downloaded = wheel_index
            .try_wheel(&wheel, true, false)?
            .ok_or_else(|| anyhow!("expected the current proxy wheel"))?;
        assert_eq!(downloaded.path.as_ref(), Path::new("proxy-b-wheel"));
        assert!(wheel_index.try_wheel(&wheel, false, true)?.is_none());

        let built = wheel_index
            .try_source(&source, false, true)?
            .ok_or_else(|| anyhow!("expected the current proxy source build"))?;
        assert_eq!(built.path.as_ref(), Path::new("proxy-b-source"));
        let downloaded_for_source = wheel_index
            .try_source(&source, true, false)?
            .ok_or_else(|| anyhow!("expected the current proxy wheel for the source"))?;
        assert_eq!(
            downloaded_for_source.path.as_ref(),
            Path::new("proxy-b-wheel")
        );
        assert!(wheel_index.try_source(&source, true, true)?.is_none());

        let live_downloaded = wheel_index
            .try_wheel(&live_hashless_wheel, true, false)?
            .ok_or_else(|| anyhow!("expected the live hashless proxy wheel"))?;
        assert_eq!(live_downloaded.path.as_ref(), Path::new("proxy-b-wheel"));
        let live_built = wheel_index
            .try_source(&live_hashless_source, false, true)?
            .ok_or_else(|| anyhow!("expected the live hashless proxy source build"))?;
        assert_eq!(live_built.path.as_ref(), Path::new("proxy-b-source"));

        let error = wheel_index
            .try_wheel(&locked_hashless_wheel, true, false)
            .expect_err("a hashless locked proxy wheel lookup should fail");
        assert!(matches!(
            error,
            crate::Error::ProxyArtifactMissingDigest {
                canonical: error_canonical,
                proxy: error_proxy,
                ..
            } if error_canonical == canonical && error_proxy == proxy_b
        ));
        let error = wheel_index
            .try_source(&locked_hashless_source, false, true)
            .expect_err("a hashless locked proxy source lookup should fail");
        assert!(matches!(
            error,
            crate::Error::ProxyArtifactMissingDigest {
                canonical: error_canonical,
                proxy: error_proxy,
                ..
            } if error_canonical == canonical && error_proxy == proxy_b
        ));
        let direct_locations =
            IndexLocations::new(vec![Index::from(canonical.clone())], Vec::new(), false);
        let direct_wheel = registry_wheel(&canonical, None, &wheel_filename, &empty_hashes);
        let canonical_index = Index::from(canonical.clone());
        let mut direct_index = RegistryWheelIndex::new(
            &cache,
            &tags,
            &direct_locations,
            &hasher,
            &config_settings,
            &config_settings_package,
            &extra_build_requires,
            &extra_build_variables,
        )?;
        direct_index.physical_index.insert(
            (&package_name, canonical.clone()),
            vec![entry(&wheel_filename, false, "canonical", &empty_hashes)],
        );
        let direct = direct_index
            .try_wheel(&direct_wheel, true, false)?
            .ok_or_else(|| anyhow!("expected the direct index cache entry"))?;
        assert_eq!(direct.path.as_ref(), Path::new("canonical"));

        let public_entry = entry(&wheel_filename, false, "canonical-public", &empty_hashes);
        direct_index.index.insert(
            &package_name,
            vec![IndexEntry {
                dist: public_entry.dist,
                built: public_entry.built,
                index: &canonical_index,
            }],
        );
        let public_entry = direct_index
            .get(&package_name)
            .next()
            .ok_or_else(|| anyhow!("expected the public canonical cache entry"))?;
        assert_eq!(public_entry.index().url(), &canonical);
        assert!(!public_entry.is_built());
        assert_eq!(
            public_entry.dist().path.as_ref(),
            Path::new("canonical-public")
        );

        Ok(())
    }
}
