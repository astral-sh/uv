use std::collections::BTreeMap;
use std::collections::Bound;
use std::ops::RangeBounds;
use std::sync::OnceLock;

use jiff::Timestamp;
use pubgrub::Ranges;
use tracing::{instrument, trace};

use uv_client::{FlatIndexEntry, OwnedArchive, SimpleDetailMetadata, VersionFiles};
use uv_configuration::BuildOptions;
use uv_distribution_filename::{DistFilename, WheelFilename};
use uv_distribution_types::{
    HashComparison, IncompatibleSource, IncompatibleWheel, IndexUrl, PrioritizedDist,
    RegistryBuiltWheel, RegistrySourceDist, RequiresPython, SourceDistCompatibility,
    WheelCompatibility,
};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_platform_tags::{IncompatibleTag, TagCompatibility, Tags};
use uv_pypi_types::{HashDigest, ResolutionMetadata, Yanked};
use uv_types::HashStrategy;
use uv_warnings::warn_user_once;

use crate::flat_index::FlatDistributions;
use crate::yanks::AllowedYanks;

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
    pub(crate) fn from_simple_metadata(
        simple_metadata: OwnedArchive<SimpleDetailMetadata>,
        package_name: &PackageName,
        index: IndexUrl,
        tags: Option<Tags>,
        requires_python: RequiresPython,
        allowed_yanks: AllowedYanks,
        hasher: HashStrategy,
        included_version_cutoff: Option<Timestamp>,
        available_version_cutoff: Option<Timestamp>,
        flat_index: Option<FlatDistributions>,
        build_options: &BuildOptions,
    ) -> Self {
        let mut stable = false;
        let mut local = false;
        let mut entries = Vec::with_capacity(simple_metadata.iter().size_hint().0);
        // Create stubs for each entry in simple metadata. The full conversion
        // from a `VersionFiles` to a PrioritizedDist for each version
        // isn't done until that specific version is requested.
        for (datum_index, datum) in simple_metadata.iter().enumerate() {
            let version = rkyv::deserialize::<Version, rkyv::rancor::Error>(&datum.version)
                .expect("archived version always deserializes");

            stable |= version.is_stable();
            local |= version.is_local();
            debug_assert!(
                entries
                    .last()
                    .is_none_or(|entry: &VersionMapLazyEntry| entry.version < version),
                "simple metadata versions must be sorted and unique"
            );
            entries.push(VersionMapLazyEntry {
                version,
                dist: LazyPrioritizedDist {
                    flat: None,
                    simple: Some(SimplePrioritizedDist {
                        datum_index,
                        dist: OnceLock::new(),
                    }),
                },
            });
        }
        let mut map = VersionMapLazyIndex { entries };
        // If a set of flat distributions have been given, linearly merge the
        // already sorted flat entries with the archive-ordered simple vector.
        if let Some(flat_index) = flat_index {
            stable |= flat_index.iter().any(|(version, _)| version.is_stable());
            map = map.merge_flat(flat_index);
        }
        Self {
            inner: VersionMapInner::Lazy(VersionMapLazy {
                package_name: package_name.clone(),
                map,
                stable,
                local,
                simple_metadata,
                no_binary: build_options.no_binary_package(package_name),
                no_build: build_options.no_build_package(package_name),
                index,
                tags,
                allowed_yanks,
                hasher,
                requires_python,
                included_version_cutoff,
                available_version_cutoff,
            }),
        }
    }

    #[instrument(skip_all, fields(package_name))]
    pub(crate) fn from_flat_metadata(
        flat_metadata: Vec<FlatIndexEntry>,
        tags: Option<&Tags>,
        hasher: &HashStrategy,
        build_options: &BuildOptions,
    ) -> Self {
        let mut stable = false;
        let mut local = false;
        let mut map = BTreeMap::new();

        for (version, prioritized_dist) in
            FlatDistributions::from_entries(flat_metadata, tags, hasher, build_options)
        {
            stable |= version.is_stable();
            local |= version.is_local();
            map.insert(version, prioritized_dist);
        }

        Self {
            inner: VersionMapInner::Eager(VersionMapEager { map, stable, local }),
        }
    }

    /// Return the [`ResolutionMetadata`] for the given version, if any.
    pub(crate) fn get_metadata(&self, version: &Version) -> Option<ResolutionMetadata> {
        match self.inner {
            VersionMapInner::Eager(_) => None,
            VersionMapInner::Lazy(ref lazy) => lazy.get_metadata(version),
        }
    }

    /// Return the [`DistFile`] for the given version, if any.
    pub(crate) fn get(&self, version: &Version) -> Option<&PrioritizedDist> {
        match self.inner {
            VersionMapInner::Eager(ref eager) => eager.map.get(version),
            VersionMapInner::Lazy(ref lazy) => lazy.get(version),
        }
    }

    /// Return an iterator over the versions in this map.
    pub(crate) fn versions(&self) -> impl DoubleEndedIterator<Item = &Version> {
        match &self.inner {
            VersionMapInner::Eager(eager) => either::Either::Left(eager.map.keys()),
            VersionMapInner::Lazy(lazy) => either::Either::Right(lazy.map.keys()),
        }
    }

    /// Returns versions with at least one file not excluded by an upload-time cutoff, in ascending
    /// order.
    ///
    /// Versions unavailable for other reasons remain present so callers receive a conservative
    /// superset of selectable versions. These reasons include yanks, hashes, `requires-python`,
    /// and `--no-binary` or `--no-build` policies. Flat-index versions bypass upload-time cutoffs
    /// because their files do not carry upload times.
    pub(crate) fn included_versions(&self) -> impl DoubleEndedIterator<Item = &Version> {
        match &self.inner {
            VersionMapInner::Eager(eager) => either::Either::Left(eager.map.keys()),
            VersionMapInner::Lazy(lazy) => either::Either::Right(lazy.included_versions()),
        }
    }

    /// Return the included versions immediately before and after `version`, without materializing
    /// distributions or cloning the complete version map.
    pub(crate) fn neighboring_included_versions(
        &self,
        version: &Version,
    ) -> Option<(Option<&Version>, Option<&Version>)> {
        match &self.inner {
            VersionMapInner::Eager(eager) => {
                eager.map.get(version)?;
                let previous = eager
                    .map
                    .range::<Version, _>((Bound::Unbounded, Bound::Excluded(version)))
                    .next_back()
                    .map(|(version, _)| version);
                let next = eager
                    .map
                    .range::<Version, _>((Bound::Excluded(version), Bound::Unbounded))
                    .next()
                    .map(|(version, _)| version);
                Some((previous, next))
            }
            VersionMapInner::Lazy(lazy) => {
                let position = lazy
                    .map
                    .entries
                    .binary_search_by(|entry| entry.version.cmp(version))
                    .ok()?;
                if !lazy.entry_is_included(&lazy.map.entries[position]) {
                    return None;
                }
                let previous = lazy.map.entries[..position]
                    .iter()
                    .rev()
                    .find(|entry| lazy.entry_is_included(entry))
                    .map(|entry| &entry.version);
                let next = lazy.map.entries[position + 1..]
                    .iter()
                    .find(|entry| lazy.entry_is_included(entry))
                    .map(|entry| &entry.version);
                Some((previous, next))
            }
        }
    }

    /// Return the index URL where this package came from.
    pub(crate) fn index(&self) -> Option<&IndexUrl> {
        match &self.inner {
            VersionMapInner::Eager(_) => None,
            VersionMapInner::Lazy(lazy) => Some(&lazy.index),
        }
    }

    /// Return the included-version cutoff for this version map, if any.
    pub(crate) fn included_version_cutoff(&self) -> Option<&Timestamp> {
        match &self.inner {
            VersionMapInner::Eager(_) => None,
            VersionMapInner::Lazy(lazy) => lazy.included_version_cutoff.as_ref(),
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
        range: &Ranges<Version>,
    ) -> impl DoubleEndedIterator<Item = (&Version, VersionMapDistHandle<'_>)> {
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
                    either::Either::Right(lazy.map.get(version).into_iter().map(move |entry| {
                        let version_map_dist = VersionMapDistHandle {
                            inner: VersionMapDistHandleInner::Lazy {
                                lazy,
                                dist: &entry.dist,
                            },
                        };
                        (&entry.version, version_map_dist)
                    }))
                }
            })
        } else {
            either::Either::Right(match self.inner {
                VersionMapInner::Eager(ref eager) => {
                    either::Either::Left(eager.map.range(BoundingRange::from(range)).map(
                        |(version, dist)| {
                            let version_map_dist = VersionMapDistHandle {
                                inner: VersionMapDistHandleInner::Eager(dist),
                            };
                            (version, version_map_dist)
                        },
                    ))
                }
                VersionMapInner::Lazy(ref lazy) => {
                    either::Either::Right(lazy.map.range(BoundingRange::from(range)).iter().map(
                        |entry| {
                            let version_map_dist = VersionMapDistHandle {
                                inner: VersionMapDistHandleInner::Lazy {
                                    lazy,
                                    dist: &entry.dist,
                                },
                            };
                            (&entry.version, version_map_dist)
                        },
                    ))
                }
            })
        }
    }

    /// Return an iterator over the versions that can be considered for selection.
    ///
    /// Unlike [`Self::iter`], this skips lazy registry versions whose files are all excluded by
    /// an upload-time cutoff without materializing their distributions. Files without an upload
    /// time remain included so that materialization can emit the appropriate warning.
    pub(crate) fn iter_included(
        &self,
        range: &Ranges<Version>,
    ) -> impl DoubleEndedIterator<Item = (&Version, VersionMapDistHandle<'_>)> {
        self.iter(range).filter(|(_, dist)| dist.is_included())
    }

    /// Return the [`Hashes`] for the given version, if any.
    pub(crate) fn hashes(&self, version: &Version) -> Option<&[HashDigest]> {
        match self.inner {
            VersionMapInner::Eager(ref eager) => {
                eager.map.get(version).map(PrioritizedDist::hashes)
            }
            VersionMapInner::Lazy(ref lazy) => lazy.get(version).map(PrioritizedDist::hashes),
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

    /// Returns `true` if the map contains at least one local version (e.g., `2.6.0+cpu`).
    pub(crate) fn local(&self) -> bool {
        match self.inner {
            VersionMapInner::Eager(ref map) => map.local,
            VersionMapInner::Lazy(ref map) => map.local,
        }
    }
}

impl From<FlatDistributions> for VersionMap {
    fn from(flat_index: FlatDistributions) -> Self {
        let stable = flat_index.iter().any(|(version, _)| version.is_stable());
        let local = flat_index.iter().any(|(version, _)| version.is_local());
        let map = flat_index.into();
        Self {
            inner: VersionMapInner::Eager(VersionMapEager { map, stable, local }),
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
    /// Returns whether this distribution can be considered for selection.
    fn is_included(&self) -> bool {
        match self.inner {
            VersionMapDistHandleInner::Eager(_) => true,
            VersionMapDistHandleInner::Lazy { lazy, dist } => match (&dist.flat, &dist.simple) {
                (Some(_), _) => true,
                (None, Some(simple)) => lazy.any_file_materializable(simple),
                (None, None) => false,
            },
        }
    }

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
#[expect(clippy::large_enum_variant)]
enum VersionMapInner {
    /// All distributions are fully materialized in memory.
    ///
    /// This usually happens when one needs a `VersionMap` from a
    /// `FlatDistributions`.
    Eager(VersionMapEager),
    /// Some distributions might be fully materialized (i.e., by initializing
    /// a `VersionMap` with a `FlatDistributions`), but some distributions
    /// might still be in their "raw" `SimpleDetailMetadata` format. In this case, a
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
    /// Whether the version map contains at least one local version.
    local: bool,
}

/// An entry in the immutable lazy version index.
#[derive(Debug)]
struct VersionMapLazyEntry {
    version: Version,
    dist: LazyPrioritizedDist,
}

/// A compact immutable version index, ordered by native PEP 440 versions.
#[derive(Debug)]
struct VersionMapLazyIndex {
    entries: Vec<VersionMapLazyEntry>,
}

impl VersionMapLazyIndex {
    /// Merge the sorted flat index into the sorted simple entries in one pass.
    fn merge_flat(self, flat_index: FlatDistributions) -> Self {
        let flat_count = flat_index.iter().size_hint().0;
        let mut merged = Vec::with_capacity(self.entries.len() + flat_count);
        let mut simple = self.entries.into_iter().peekable();
        let mut flat = flat_index.into_iter().peekable();
        let flat_entry = |(version, dist)| VersionMapLazyEntry {
            version,
            dist: LazyPrioritizedDist {
                flat: Some(dist),
                simple: None,
            },
        };
        while let (Some(simple_entry), Some((flat_version, _))) = (simple.peek(), flat.peek()) {
            match simple_entry.version.cmp(flat_version) {
                std::cmp::Ordering::Less => {
                    if let Some(entry) = simple.next() {
                        merged.push(entry);
                    }
                }
                std::cmp::Ordering::Greater => {
                    if let Some(entry) = flat.next() {
                        merged.push(flat_entry(entry));
                    }
                }
                std::cmp::Ordering::Equal => {
                    if let (Some(mut entry), Some((_, dist))) = (simple.next(), flat.next()) {
                        entry.dist.flat = Some(dist);
                        merged.push(entry);
                    }
                }
            }
        }
        merged.extend(simple);
        merged.extend(flat.map(flat_entry));

        Self { entries: merged }
    }

    fn get(&self, version: &Version) -> Option<&VersionMapLazyEntry> {
        let index = self
            .entries
            .binary_search_by(|entry| entry.version.cmp(version))
            .ok()?;
        self.entries.get(index)
    }

    fn keys(&self) -> impl DoubleEndedIterator<Item = &Version> {
        self.entries.iter().map(|entry| &entry.version)
    }

    fn range(&self, range: BoundingRange<'_>) -> &[VersionMapLazyEntry] {
        let start = match range.min {
            Bound::Included(version) => self
                .entries
                .partition_point(|entry| entry.version < *version),
            Bound::Excluded(version) => self
                .entries
                .partition_point(|entry| entry.version <= *version),
            Bound::Unbounded => 0,
        };
        let end = match range.max {
            Bound::Included(version) => self
                .entries
                .partition_point(|entry| entry.version <= *version),
            Bound::Excluded(version) => self
                .entries
                .partition_point(|entry| entry.version < *version),
            Bound::Unbounded => self.entries.len(),
        };
        self.entries.get(start..end).unwrap_or_default()
    }

    fn len(&self) -> usize {
        self.entries.len()
    }
}

/// A map that lazily materializes some prioritized distributions upon access.
///
/// The idea here is that some packages have a lot of versions published, and
/// needing to materialize a full `VersionMap` with all corresponding metadata
/// for every version in memory is expensive. Since a `SimpleDetailMetadata` can be
/// materialized with very little cost (via `rkyv` in the warm cached case),
/// avoiding another conversion step into a fully filled out `VersionMap` can
/// provide substantial savings in some cases.
#[derive(Debug)]
struct VersionMapLazy {
    /// The normalized package name used to reconstruct cached wheel filenames.
    package_name: PackageName,
    /// An immutable archive-order index from version to possibly-initialized distribution.
    map: VersionMapLazyIndex,
    /// Whether the version map contains at least one stable (non-pre-release) version.
    stable: bool,
    /// Whether the version map contains at least one local version.
    local: bool,
    /// The raw simple metadata from which `PrioritizedDist`s should
    /// be constructed.
    simple_metadata: OwnedArchive<SimpleDetailMetadata>,
    /// When true, wheels aren't allowed.
    no_binary: bool,
    /// When true, source dists aren't allowed.
    no_build: bool,
    /// The URL of the index where this package came from.
    index: IndexUrl,
    /// The set of compatibility tags that determines whether a wheel is usable
    /// in the current environment.
    tags: Option<Tags>,
    /// Files newer than this timestamp are considered excluded, i.e., that they cannot be selected by the
    /// resolver.
    included_version_cutoff: Option<Timestamp>,
    /// Files newer than this timestamp are considered unavailable, i.e., that they do not exist.
    available_version_cutoff: Option<Timestamp>,
    /// Which yanked versions are allowed
    allowed_yanks: AllowedYanks,
    /// The hashes of allowed distributions.
    hasher: HashStrategy,
    /// The `requires-python` constraint for the resolution.
    requires_python: RequiresPython,
}

impl VersionMapLazy {
    /// Returns the registry-provided metadata for the given version, if it exists.
    fn get_metadata(&self, version: &Version) -> Option<ResolutionMetadata> {
        let archived = self
            .map
            .get(version)
            .and_then(|entry| entry.dist.simple.as_ref())
            .and_then(|simple| self.simple_metadata.datum(simple.datum_index))
            .and_then(|datum| datum.metadata.as_deref())?;
        Some(
            rkyv::deserialize::<ResolutionMetadata, rkyv::rancor::Error>(archived)
                .expect("archived metadata always deserializes"),
        )
    }

    /// Returns the distribution for the given version, if it exists.
    fn get(&self, version: &Version) -> Option<&PrioritizedDist> {
        self.get_lazy(&self.map.get(version)?.dist)
    }

    /// Returns an iterator over the versions with at least one file within the exclude-newer
    /// cutoffs, without materializing the distributions.
    fn included_versions(&self) -> impl DoubleEndedIterator<Item = &Version> {
        self.map
            .entries
            .iter()
            .filter(|entry| self.entry_is_included(entry))
            .map(|entry| &entry.version)
    }

    /// Return whether an entry has at least one file within the exclude-newer cutoffs.
    fn entry_is_included(&self, entry: &VersionMapLazyEntry) -> bool {
        match (&entry.dist.flat, &entry.dist.simple) {
            // Flat index files have no upload times and bypass the cutoffs.
            (Some(_), _) => true,
            (None, Some(simple)) => self.any_file_included(simple),
            (None, None) => false,
        }
    }

    /// Returns whether at least one file keeps this version inside the candidate universe.
    ///
    /// This mirrors the per-file cutoff handling in [`Self::get_simple`] without materializing the
    /// distribution, including the two cutoff modes' distinct handling of missing upload times.
    fn any_file_included(&self, simple: &SimplePrioritizedDist) -> bool {
        if self.included_version_cutoff.is_none() && self.available_version_cutoff.is_none() {
            return true;
        }
        let Some(datum) = self.simple_metadata.datum(simple.datum_index) else {
            return false;
        };
        let files = &datum.files;
        files
            .wheels
            .iter()
            .chain(files.source_dists.iter())
            .any(|file| {
                let upload_time = file.upload_time_utc_ms();
                let excluded = if let Some(cutoff) = &self.included_version_cutoff {
                    upload_time.is_none_or(|t| t >= cutoff.as_millisecond())
                } else if let Some(cutoff) = &self.available_version_cutoff {
                    upload_time.is_some_and(|t| t >= cutoff.as_millisecond())
                } else {
                    false
                };
                !excluded
            })
    }

    /// Returns whether a version should be materialized during candidate selection.
    ///
    /// Missing upload times are retained here, even for `included_version_cutoff`, since
    /// materializing them is what emits the corresponding `exclude-newer` warning.
    fn any_file_materializable(&self, simple: &SimplePrioritizedDist) -> bool {
        let Some(cutoff) = self
            .included_version_cutoff
            .as_ref()
            .or(self.available_version_cutoff.as_ref())
        else {
            return true;
        };
        let Some(datum) = self.simple_metadata.datum(simple.datum_index) else {
            return false;
        };
        datum
            .files
            .wheels
            .iter()
            .chain(datum.files.source_dists.iter())
            .any(|file| {
                file.upload_time_utc_ms()
                    .is_none_or(|upload_time| upload_time < cutoff.as_millisecond())
            })
    }

    /// Given a reference to a possibly-initialized distribution that is in
    /// this lazy map, return the corresponding distribution.
    ///
    /// When both a flat and simple distribution are present internally, they
    /// are merged automatically.
    fn get_lazy<'p>(&'p self, lazy_dist: &'p LazyPrioritizedDist) -> Option<&'p PrioritizedDist> {
        match (&lazy_dist.flat, &lazy_dist.simple) {
            (Some(flat), Some(simple)) => self.get_simple(Some(flat), simple),
            (Some(flat), None) => Some(flat),
            (None, Some(simple)) => self.get_simple(None, simple),
            (None, None) => None,
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
            let files = rkyv::deserialize::<VersionFiles, rkyv::rancor::Error>(
                &self
                    .simple_metadata
                    .datum(simple.datum_index)
                    .expect("index to lazy dist is correct")
                    .files,
            )
            .expect("archived version files always deserializes");
            let mut priority_dist = init.cloned().unwrap_or_default();
            for (filename, file) in files.all(&self.package_name) {
                // Support resolving as if it were an earlier timestamp, at least as long files have
                // upload time information.
                let (excluded, upload_time) = if let Some(included_version_cutoff) =
                    &self.included_version_cutoff
                {
                    match file.upload_time_utc_ms.as_ref() {
                        Some(&upload_time)
                            if upload_time >= included_version_cutoff.as_millisecond() =>
                        {
                            trace!(
                                "Excluding `{}` (uploaded {upload_time}) due to exclude-newer ({included_version_cutoff})",
                                file.filename
                            );
                            (true, Some(upload_time))
                        }
                        None => {
                            warn_user_once!(
                                "{} is missing an upload date, but user provided: {included_version_cutoff}",
                                file.filename,
                            );
                            (true, None)
                        }
                        _ => (false, None),
                    }
                } else if let Some(available_version_cutoff) = &self.available_version_cutoff {
                    match file.upload_time_utc_ms.as_ref() {
                        Some(&upload_time)
                            if upload_time >= available_version_cutoff.as_millisecond() =>
                        {
                            trace!(
                                "Excluding `{}` (uploaded {upload_time}) due to available version cutoff ({available_version_cutoff})",
                                file.filename
                            );
                            (true, Some(upload_time))
                        }
                        _ => (false, None),
                    }
                } else {
                    (false, None)
                };

                // Prioritize amongst all available files.
                let yanked = file.yanked.as_deref();
                let hashes = file.hashes.clone();
                match filename {
                    DistFilename::WheelFilename(filename) => {
                        let compatibility = self.wheel_compatibility(
                            &filename,
                            &filename.name,
                            &filename.version,
                            hashes.as_slice(),
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
                            hashes.as_slice(),
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
        yanked: Option<&Yanked>,
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
                return SourceDistCompatibility::Incompatible(IncompatibleSource::Yanked(
                    yanked.clone(),
                ));
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
            } else if hash_policy.matches(hashes) {
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
        yanked: Option<&Yanked>,
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
                return WheelCompatibility::Incompatible(IncompatibleWheel::Yanked(yanked.clone()));
            }
        }

        // Determine a compatibility for the wheel based on tags.
        let priority = if let Some(tags) = &self.tags {
            match filename.compatibility(tags) {
                TagCompatibility::Incompatible(tag) => {
                    return WheelCompatibility::Incompatible(IncompatibleWheel::Tag(tag));
                }
                TagCompatibility::Compatible(priority) => Some(priority),
            }
        } else {
            // Check if the wheel is compatible with the `requires-python` (i.e., the Python
            // ABI tag is not less than the `requires-python` minimum version).
            if !self.requires_python.matches_wheel_tag(filename) {
                return WheelCompatibility::Incompatible(IncompatibleWheel::Tag(
                    IncompatibleTag::AbiPythonVersion,
                ));
            }
            None
        };

        // Check if hashes line up. If hashes aren't required, they're considered matching.
        let hash_policy = self.hasher.get_package(name, version);
        let required_hashes = hash_policy.digests();
        let hash = if required_hashes.is_empty() {
            HashComparison::Matched
        } else {
            if hashes.is_empty() {
                HashComparison::Missing
            } else if hash_policy.matches(hashes) {
                HashComparison::Matched
            } else {
                HashComparison::Mismatched
            }
        };

        // Break ties with the build tag.
        let build_tag = filename.build_tag().cloned();

        WheelCompatibility::Compatible(hash, priority, build_tag)
    }
}

/// Represents a possibly initialized [`PrioritizedDist`] for a package version.
#[derive(Debug)]
struct LazyPrioritizedDist {
    /// An eagerly constructed distribution from [`FlatDistributions`], if present.
    flat: Option<PrioritizedDist>,
    /// A lazy index into [`SimpleDetailMetadata`], if present.
    simple: Option<SimplePrioritizedDist>,
}

/// Represents a lazily initialized `PrioritizedDist`.
#[derive(Debug)]
struct SimplePrioritizedDist {
    /// An offset into `SimpleDetailMetadata` corresponding to a `SimpleMetadatum`.
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

/// A range that can be used to iterate over a subset of a [`BTreeMap`].
#[derive(Debug)]
struct BoundingRange<'a> {
    min: Bound<&'a Version>,
    max: Bound<&'a Version>,
}

impl<'a> From<&'a Ranges<Version>> for BoundingRange<'a> {
    fn from(value: &'a Ranges<Version>) -> Self {
        let (min, max) = value
            .bounding_range()
            .unwrap_or((Bound::Unbounded, Bound::Unbounded));
        Self { min, max }
    }
}

impl<'a> RangeBounds<Version> for BoundingRange<'a> {
    fn start_bound(&self) -> Bound<&'a Version> {
        self.min
    }

    fn end_bound(&self) -> Bound<&'a Version> {
        self.max
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use uv_distribution_types::PrioritizedDist;
    use uv_pep440::Version;

    use super::{VersionMap, VersionMapEager, VersionMapInner};

    #[test]
    fn neighboring_included_versions() {
        let first = Version::new([1]);
        let second = Version::new([2]);
        let third = Version::new([3]);
        let map = VersionMap {
            inner: VersionMapInner::Eager(VersionMapEager {
                map: BTreeMap::from([
                    (first.clone(), PrioritizedDist::default()),
                    (second.clone(), PrioritizedDist::default()),
                    (third.clone(), PrioritizedDist::default()),
                ]),
                stable: true,
                local: false,
            }),
        };

        assert_eq!(
            map.neighboring_included_versions(&first),
            Some((None, Some(&second)))
        );
        assert_eq!(
            map.neighboring_included_versions(&second),
            Some((Some(&first), Some(&third)))
        );
        assert_eq!(
            map.neighboring_included_versions(&third),
            Some((Some(&second), None))
        );
        assert_eq!(map.neighboring_included_versions(&Version::new([4])), None);
    }
}
