use pubgrub::Range;
use rkyv::{de::deserializers::SharedDeserializeMap, Deserialize};
use std::collections::btree_map::{BTreeMap, Entry};
use std::sync::OnceLock;
use tracing::instrument;

use distribution_filename::{DistFilename, WheelFilename};
use distribution_types::{
    HashComparison, IncompatibleSource, IncompatibleWheel, IndexUrl, PrioritizedDist,
    RegistryBuiltWheel, RegistrySourceDist, SourceDistCompatibility, WheelCompatibility,
};
use pep440_rs::Version;
use platform_tags::{IncompatibleTag, TagCompatibility, Tags};
use pypi_types::{HashDigest, Yanked};
use uv_client::{OwnedArchive, SimpleMetadata, VersionFiles};
use uv_configuration::BuildOptions;
use uv_normalize::PackageName;
use uv_types::HashStrategy;
use uv_warnings::warn_user_once;

use crate::flat_index::FlatDistributions;
use crate::{yanks::AllowedYanks, ExcludeNewer, RequiresPython};

/// A map from versions to distributions.
#[derive(Debug)]
pub struct VersionMap {
    /// The inner representation of the version map.
    inner: VersionMapInner,
}

impl VersionMap {
    /// Initialize a [`VersionMap`] from the given metadata.
    ///
    /// Note it is possible for files to have a different yank status per PEP 592 but in the official
    /// PyPI warehouse this cannot happen.
    ///
    /// Here, we track if each file is yanked separately. If a release is partially yanked, the
    /// unyanked distributions _can_ be used.
    ///
    /// PEP 592: <https://peps.python.org/pep-0592/#warehouse-pypi-implementation-notes>
    #[instrument(skip_all, fields(package_name))]
    pub(crate) fn from_metadata(
        simple_metadata: OwnedArchive<SimpleMetadata>,
        package_name: &PackageName,
        index: &IndexUrl,
        tags: Option<&Tags>,
        requires_python: &RequiresPython,
        allowed_yanks: &AllowedYanks,
        hasher: &HashStrategy,
        exclude_newer: Option<&ExcludeNewer>,
        flat_index: Option<FlatDistributions>,
        build_options: &BuildOptions,
    ) -> Self {
        let mut stable = false;
        let mut map = BTreeMap::new();
        // Create stubs for each entry in simple metadata. The full conversion
        // from a `VersionFiles` to a PrioritizedDist for each version
        // isn't done until that specific version is requested.
        for (datum_index, datum) in simple_metadata.iter().enumerate() {
            let version: Version = datum
                .version
                .deserialize(&mut SharedDeserializeMap::new())
                .expect("archived version always deserializes");
            stable |= version.is_stable();
            map.insert(
                version,
                LazyPrioritizedDist::OnlySimple(SimplePrioritizedDist {
                    datum_index,
                    dist: OnceLock::new(),
                }),
            );
        }
        // If a set of flat distributions have been given, we need to add those
        // to our map of entries as well.
        for (version, prioritized_dist) in flat_index.into_iter().flatten() {
            stable |= version.is_stable();
            match map.entry(version) {
                Entry::Vacant(e) => {
                    e.insert(LazyPrioritizedDist::OnlyFlat(prioritized_dist));
                }
                // When there is both a `VersionFiles` (from the "simple"
                // metadata) and a flat distribution for the same version of
                // a package, we store both and "merge" them into a single
                // `PrioritizedDist` upon access later.
                Entry::Occupied(e) => match e.remove_entry() {
                    (version, LazyPrioritizedDist::OnlySimple(simple_dist)) => {
                        map.insert(
                            version,
                            LazyPrioritizedDist::Both {
                                flat: prioritized_dist,
                                simple: simple_dist,
                            },
                        );
                    }
                    _ => unreachable!(),
                },
            }
        }
        Self {
            inner: VersionMapInner::Lazy(VersionMapLazy {
                map,
                stable,
                simple_metadata,
                no_binary: build_options.no_binary_package(package_name),
                no_build: build_options.no_build_package(package_name),
                index: index.clone(),
                tags: tags.cloned(),
                allowed_yanks: allowed_yanks.clone(),
                hasher: hasher.clone(),
                requires_python: requires_python.clone(),
                exclude_newer: exclude_newer.copied(),
            }),
        }
    }

    /// Return the [`DistFile`] for the given version, if any.
    pub(crate) fn get(&self, version: &Version) -> Option<&PrioritizedDist> {
        self.get_with_version(version).map(|(_version, dist)| dist)
    }

    /// Return the [`DistFile`] and the `Version` from the map for the given
    /// version, if any.
    ///
    /// This is useful when you depend on access to the specific `Version`
    /// stored in this map. For example, the versions `1.2.0` and `1.2` are
    /// semantically equivalent, but when converted to strings, they are
    /// distinct.
    pub(crate) fn get_with_version(
        &self,
        version: &Version,
    ) -> Option<(&Version, &PrioritizedDist)> {
        match self.inner {
            VersionMapInner::Eager(ref eager) => eager.map.get_key_value(version),
            VersionMapInner::Lazy(ref lazy) => lazy.get_with_version(version),
        }
    }

    /// Return an iterator over the versions in this map.
    pub(crate) fn versions(&self) -> impl Iterator<Item = &Version> {
        match &self.inner {
            VersionMapInner::Eager(eager) => either::Either::Left(eager.map.keys()),
            VersionMapInner::Lazy(lazy) => either::Either::Right(lazy.map.keys()),
        }
    }

    /// Return an iterator over the versions and distributions.
    ///
    /// Note that the value returned in this iterator is a [`VersionMapDist`],
    /// which can be used to lazily request a [`CompatibleDist`]. This is
    /// useful in cases where one can skip materializing a full distribution
    /// for each version.
    pub(crate) fn iter(
        &self,
        range: &Range<Version>,
    ) -> impl DoubleEndedIterator<Item = (&Version, VersionMapDistHandle)> + ExactSizeIterator {
        // Performance optimization: If we only have a single version, return that version directly.
        if let Some(version) = range.as_singleton() {
            either::Either::Left(match self.inner {
                VersionMapInner::Eager(ref eager) => {
                    either::Either::Left(eager.map.get_key_value(version).into_iter().map(
                        move |(version, dist)| {
                            let version_map_dist = VersionMapDistHandle {
                                inner: VersionMapDistHandleInner::Eager(dist),
                            };
                            (version, version_map_dist)
                        },
                    ))
                }
                VersionMapInner::Lazy(ref lazy) => {
                    either::Either::Right(lazy.map.get_key_value(version).into_iter().map(
                        move |(version, dist)| {
                            let version_map_dist = VersionMapDistHandle {
                                inner: VersionMapDistHandleInner::Lazy { lazy, dist },
                            };
                            (version, version_map_dist)
                        },
                    ))
                }
            })
        } else {
            either::Either::Right(match self.inner {
                VersionMapInner::Eager(ref eager) => {
                    either::Either::Left(eager.map.iter().map(|(version, dist)| {
                        let version_map_dist = VersionMapDistHandle {
                            inner: VersionMapDistHandleInner::Eager(dist),
                        };
                        (version, version_map_dist)
                    }))
                }
                VersionMapInner::Lazy(ref lazy) => {
                    either::Either::Right(lazy.map.iter().map(|(version, dist)| {
                        let version_map_dist = VersionMapDistHandle {
                            inner: VersionMapDistHandleInner::Lazy { lazy, dist },
                        };
                        (version, version_map_dist)
                    }))
                }
            })
        }
    }

    /// Return the [`Hashes`] for the given version, if any.
    pub(crate) fn hashes(&self, version: &Version) -> Option<Vec<HashDigest>> {
        match self.inner {
            VersionMapInner::Eager(ref eager) => {
                eager.map.get(version).map(|file| file.hashes().to_vec())
            }
            VersionMapInner::Lazy(ref lazy) => lazy.get(version).map(|file| file.hashes().to_vec()),
        }
    }

    /// Returns the total number of distinct versions in this map.
    ///
    /// Note that this may include versions of distributions that are not
    /// usable in the current environment.
    pub(crate) fn len(&self) -> usize {
        match self.inner {
            VersionMapInner::Eager(VersionMapEager { ref map, .. }) => map.len(),
            VersionMapInner::Lazy(VersionMapLazy { ref map, .. }) => map.len(),
        }
    }

    /// Returns `true` if the map contains at least one stable (non-pre-release) version.
    pub(crate) fn stable(&self) -> bool {
        match self.inner {
            VersionMapInner::Eager(ref map) => map.stable,
            VersionMapInner::Lazy(ref map) => map.stable,
        }
    }
}

impl From<FlatDistributions> for VersionMap {
    fn from(flat_index: FlatDistributions) -> Self {
        let stable = flat_index.iter().any(|(version, _)| version.is_stable());
        let map = flat_index.into();
        Self {
            inner: VersionMapInner::Eager(VersionMapEager { map, stable }),
        }
    }
}

/// A lazily initialized distribution.
///
/// This permits access to a handle that can be turned into a resolvable
/// distribution when desired. This is coupled with a `Version` in
/// [`VersionMap::iter`] to permit iteration over all items in a map without
/// necessarily constructing a distribution for every version if it isn't
/// needed.
///
/// Note that because of laziness, not all such items can be turned into
/// a valid distribution. For example, if in the process of building a
/// distribution no compatible wheel or source distribution could be found,
/// then building a `CompatibleDist` will fail.
pub(crate) struct VersionMapDistHandle<'a> {
    inner: VersionMapDistHandleInner<'a>,
}

enum VersionMapDistHandleInner<'a> {
    Eager(&'a PrioritizedDist),
    Lazy {
        lazy: &'a VersionMapLazy,
        dist: &'a LazyPrioritizedDist,
    },
}

impl<'a> VersionMapDistHandle<'a> {
    /// Returns a prioritized distribution from this handle.
    pub(crate) fn prioritized_dist(&self) -> Option<&'a PrioritizedDist> {
        match self.inner {
            VersionMapDistHandleInner::Eager(dist) => Some(dist),
            VersionMapDistHandleInner::Lazy { lazy, dist } => Some(lazy.get_lazy(dist)?),
        }
    }
}

/// The kind of internal version map we have.
#[derive(Debug)]
enum VersionMapInner {
    /// All distributions are fully materialized in memory.
    ///
    /// This usually happens when one needs a `VersionMap` from a
    /// `FlatDistributions`.
    Eager(VersionMapEager),
    /// Some distributions might be fully materialized (i.e., by initializing
    /// a `VersionMap` with a `FlatDistributions`), but some distributions
    /// might still be in their "raw" `SimpleMetadata` format. In this case, a
    /// `PrioritizedDist` isn't actually created in memory until the
    /// specific version has been requested.
    Lazy(VersionMapLazy),
}

/// A map from versions to distributions that are fully materialized in memory.
#[derive(Debug)]
struct VersionMapEager {
    /// A map from version to distribution.
    map: BTreeMap<Version, PrioritizedDist>,
    /// Whether the version map contains at least one stable (non-pre-release) version.
    stable: bool,
}

/// A map that lazily materializes some prioritized distributions upon access.
///
/// The idea here is that some packages have a lot of versions published, and
/// needing to materialize a full `VersionMap` with all corresponding metadata
/// for every version in memory is expensive. Since a `SimpleMetadata` can be
/// materialized with very little cost (via `rkyv` in the warm cached case),
/// avoiding another conversion step into a fully filled out `VersionMap` can
/// provide substantial savings in some cases.
#[derive(Debug)]
struct VersionMapLazy {
    /// A map from version to possibly-initialized distribution.
    map: BTreeMap<Version, LazyPrioritizedDist>,
    /// Whether the version map contains at least one stable (non-pre-release) version.
    stable: bool,
    /// The raw simple metadata from which `PrioritizedDist`s should
    /// be constructed.
    simple_metadata: OwnedArchive<SimpleMetadata>,
    /// When true, wheels aren't allowed.
    no_binary: bool,
    /// When true, source dists aren't allowed.
    no_build: bool,
    /// The URL of the index where this package came from.
    index: IndexUrl,
    /// The set of compatibility tags that determines whether a wheel is usable
    /// in the current environment.
    tags: Option<Tags>,
    /// Whether files newer than this timestamp should be excluded or not.
    exclude_newer: Option<ExcludeNewer>,
    /// Which yanked versions are allowed
    allowed_yanks: AllowedYanks,
    /// The hashes of allowed distributions.
    hasher: HashStrategy,
    /// The `requires-python` constraint for the resolution.
    requires_python: RequiresPython,
}

impl VersionMapLazy {
    /// Returns the distribution for the given version, if it exists.
    fn get(&self, version: &Version) -> Option<&PrioritizedDist> {
        self.get_with_version(version)
            .map(|(_, prioritized_dist)| prioritized_dist)
    }

    /// Returns the distribution for the given version along with the version
    /// in this map, if it exists.
    fn get_with_version(&self, version: &Version) -> Option<(&Version, &PrioritizedDist)> {
        let (version, lazy_dist) = self.map.get_key_value(version)?;
        let priority_dist = self.get_lazy(lazy_dist)?;
        Some((version, priority_dist))
    }

    /// Given a reference to a possibly-initialized distribution that is in
    /// this lazy map, return the corresponding distribution.
    ///
    /// When both a flat and simple distribution are present internally, they
    /// are merged automatically.
    fn get_lazy<'p>(&'p self, lazy_dist: &'p LazyPrioritizedDist) -> Option<&'p PrioritizedDist> {
        match *lazy_dist {
            LazyPrioritizedDist::OnlyFlat(ref dist) => Some(dist),
            LazyPrioritizedDist::OnlySimple(ref dist) => self.get_simple(None, dist),
            LazyPrioritizedDist::Both {
                ref flat,
                ref simple,
            } => self.get_simple(Some(flat), simple),
        }
    }

    /// Given an optional starting point, return the final form of the
    /// given simple distribution. If it wasn't initialized yet, then this
    /// initializes it. If the distribution would otherwise be empty, this
    /// returns `None`.
    fn get_simple<'p>(
        &'p self,
        init: Option<&'p PrioritizedDist>,
        simple: &'p SimplePrioritizedDist,
    ) -> Option<&'p PrioritizedDist> {
        let get_or_init = || {
            let files: VersionFiles = self
                .simple_metadata
                .datum(simple.datum_index)
                .expect("index to lazy dist is correct")
                .files
                .deserialize(&mut SharedDeserializeMap::new())
                .expect("archived version files should deserialize");
            let mut priority_dist = init.cloned().unwrap_or_default();
            for (filename, file) in files.all() {
                // Support resolving as if it were an earlier timestamp, at least as long files have
                // upload time information.
                let (excluded, upload_time) = if let Some(exclude_newer) = self.exclude_newer {
                    match file.upload_time_utc_ms.as_ref() {
                        Some(&upload_time) if upload_time >= exclude_newer.timestamp_millis() => {
                            (true, Some(upload_time))
                        }
                        None => {
                            warn_user_once!(
                                "{} is missing an upload date, but user provided: {exclude_newer}",
                                file.filename,
                            );
                            (true, None)
                        }
                        _ => (false, None),
                    }
                } else {
                    (false, None)
                };

                // Prioritize amongst all available files.
                let yanked = file.yanked.clone();
                let hashes = file.hashes.clone();
                match filename {
                    DistFilename::WheelFilename(filename) => {
                        let compatibility = self.wheel_compatibility(
                            &filename,
                            &filename.name,
                            &filename.version,
                            &hashes,
                            yanked,
                            excluded,
                            upload_time,
                        );
                        let dist = RegistryBuiltWheel {
                            filename,
                            file: Box::new(file),
                            index: self.index.clone(),
                        };
                        priority_dist.insert_built(dist, hashes, compatibility);
                    }
                    DistFilename::SourceDistFilename(filename) => {
                        let compatibility = self.source_dist_compatibility(
                            &filename.name,
                            &filename.version,
                            &hashes,
                            yanked,
                            excluded,
                            upload_time,
                        );
                        let dist = RegistrySourceDist {
                            name: filename.name.clone(),
                            version: filename.version.clone(),
                            ext: filename.extension,
                            file: Box::new(file),
                            index: self.index.clone(),
                            wheels: vec![],
                        };
                        priority_dist.insert_source(dist, hashes, compatibility);
                    }
                }
            }
            if priority_dist.is_empty() {
                None
            } else {
                Some(priority_dist)
            }
        };
        simple.dist.get_or_init(get_or_init).as_ref()
    }

    fn source_dist_compatibility(
        &self,
        name: &PackageName,
        version: &Version,
        hashes: &[HashDigest],
        yanked: Option<Yanked>,
        excluded: bool,
        upload_time: Option<i64>,
    ) -> SourceDistCompatibility {
        // Check if builds are disabled
        if self.no_build {
            return SourceDistCompatibility::Incompatible(IncompatibleSource::NoBuild);
        }

        // Check if after upload time cutoff
        if excluded {
            return SourceDistCompatibility::Incompatible(IncompatibleSource::ExcludeNewer(
                upload_time,
            ));
        }

        // Check if yanked
        if let Some(yanked) = yanked {
            if yanked.is_yanked() && !self.allowed_yanks.contains(name, version) {
                return SourceDistCompatibility::Incompatible(IncompatibleSource::Yanked(yanked));
            }
        }

        // Check if hashes line up. If hashes aren't required, they're considered matching.
        let hash_policy = self.hasher.get_package(name, version);
        let required_hashes = hash_policy.digests();
        let hash = if required_hashes.is_empty() {
            HashComparison::Matched
        } else {
            if hashes.is_empty() {
                HashComparison::Missing
            } else if hashes.iter().any(|hash| required_hashes.contains(hash)) {
                HashComparison::Matched
            } else {
                HashComparison::Mismatched
            }
        };

        SourceDistCompatibility::Compatible(hash)
    }

    fn wheel_compatibility(
        &self,
        filename: &WheelFilename,
        name: &PackageName,
        version: &Version,
        hashes: &[HashDigest],
        yanked: Option<Yanked>,
        excluded: bool,
        upload_time: Option<i64>,
    ) -> WheelCompatibility {
        // Check if binaries are disabled
        if self.no_binary {
            return WheelCompatibility::Incompatible(IncompatibleWheel::NoBinary);
        }

        // Check if after upload time cutoff
        if excluded {
            return WheelCompatibility::Incompatible(IncompatibleWheel::ExcludeNewer(upload_time));
        }

        // Check if yanked
        if let Some(yanked) = yanked {
            if yanked.is_yanked() && !self.allowed_yanks.contains(name, version) {
                return WheelCompatibility::Incompatible(IncompatibleWheel::Yanked(yanked));
            }
        }

        // Determine a compatibility for the wheel based on tags.
        let priority = match &self.tags {
            Some(tags) => match filename.compatibility(tags) {
                TagCompatibility::Incompatible(tag) => {
                    return WheelCompatibility::Incompatible(IncompatibleWheel::Tag(tag))
                }
                TagCompatibility::Compatible(priority) => Some(priority),
            },
            None => None,
        };

        // Check if hashes line up. If hashes aren't required, they're considered matching.
        let hash_policy = self.hasher.get_package(name, version);
        let required_hashes = hash_policy.digests();
        let hash = if required_hashes.is_empty() {
            HashComparison::Matched
        } else {
            if hashes.is_empty() {
                HashComparison::Missing
            } else if hashes.iter().any(|hash| required_hashes.contains(hash)) {
                HashComparison::Matched
            } else {
                HashComparison::Mismatched
            }
        };

        // Check if the wheel is compatible with the `requires-python` (i.e., the Python ABI tag
        // is not less than the `requires-python` minimum version).
        if !self.requires_python.matches_wheel_tag(filename) {
            return WheelCompatibility::Incompatible(IncompatibleWheel::Tag(IncompatibleTag::Abi));
        }

        // Break ties with the build tag.
        let build_tag = filename.build_tag.clone();

        WheelCompatibility::Compatible(hash, priority, build_tag)
    }
}

/// Represents a possibly initialized [`PrioritizedDist`] for
/// a single version of a package.
#[derive(Debug)]
enum LazyPrioritizedDist {
    /// Represents a eagerly constructed distribution from a
    /// `FlatDistributions`.
    OnlyFlat(PrioritizedDist),
    /// Represents a lazily constructed distribution from an index into a
    /// `VersionFiles` from `SimpleMetadata`.
    OnlySimple(SimplePrioritizedDist),
    /// Combines the above. This occurs when we have data from both a flat
    /// distribution and a simple distribution.
    Both {
        flat: PrioritizedDist,
        simple: SimplePrioritizedDist,
    },
}

/// Represents a lazily initialized `PrioritizedDist`.
#[derive(Debug)]
struct SimplePrioritizedDist {
    /// An offset into `SimpleMetadata` corresponding to a `SimpleMetadatum`.
    /// This provides access to a `VersionFiles` that is used to construct a
    /// `PrioritizedDist`.
    datum_index: usize,
    /// A lazily initialized distribution.
    ///
    /// Note that the `Option` does not represent the initialization state.
    /// The `Option` can be `None` even after initialization, for example,
    /// if initialization could not find any usable files from which to
    /// construct a distribution. (One easy way to effect this, at the time
    /// of writing, is to use `--exclude-newer 1900-01-01`.)
    dist: OnceLock<Option<PrioritizedDist>>,
}
