use std::collections::btree_map::{BTreeMap, Entry};
use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use tracing::{instrument, warn};

use distribution_filename::DistFilename;
use distribution_types::{Dist, IndexUrl, PrioritizedDistribution, ResolvableDist};
use pep440_rs::Version;
use platform_tags::Tags;
use puffin_client::{FlatDistributions, OwnedArchive, SimpleMetadata, VersionFiles};
use puffin_normalize::PackageName;
use puffin_traits::NoBinary;
use puffin_warnings::warn_user_once;
use pypi_types::Hashes;
use rkyv::{de::deserializers::SharedDeserializeMap, Deserialize};

use crate::python_requirement::PythonRequirement;

/// A map from versions to distributions.
#[derive(Debug)]
pub struct VersionMap {
    inner: VersionMapInner,
}

impl VersionMap {
    /// Initialize a [`VersionMap`] from the given metadata.
    #[instrument(skip_all, fields(package_name))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_metadata(
        simple_metadata: OwnedArchive<SimpleMetadata>,
        package_name: &PackageName,
        index: &IndexUrl,
        tags: &Tags,
        python_requirement: &PythonRequirement,
        exclude_newer: Option<&DateTime<Utc>>,
        flat_index: Option<FlatDistributions>,
        no_binary: &NoBinary,
    ) -> Self {
        let mut map = BTreeMap::new();
        // Create stubs for each entry in simple metadata. The full conversion
        // from a `VersionFiles` to a PrioritizedDistribution for each version
        // isn't done until that specific version is requested.
        for (datum_index, datum) in simple_metadata.iter().enumerate() {
            let version: Version = datum
                .version
                .deserialize(&mut SharedDeserializeMap::new())
                .expect("archived version always deserializes");
            map.insert(
                version,
                LazyPrioritizedDistribution::OnlySimple(SimplePrioritizedDistribution {
                    datum_index,
                    dist: OnceLock::new(),
                }),
            );
        }
        // If a set of flat distributions have been given, we need to add those
        // to our map of entries as well.
        for (version, prioritized_dist) in flat_index.into_iter().flatten() {
            match map.entry(version) {
                Entry::Vacant(e) => {
                    e.insert(LazyPrioritizedDistribution::OnlyFlat(prioritized_dist));
                }
                // When there is both a `VersionFiles` (from the "simple"
                // metadata) and a flat distribution for the same version of
                // a package, we store both and "merge" them into a single
                // `PrioritizedDistribution` upon access later.
                Entry::Occupied(e) => match e.remove_entry() {
                    (version, LazyPrioritizedDistribution::OnlySimple(simple_dist)) => {
                        map.insert(
                            version,
                            LazyPrioritizedDistribution::Both {
                                flat: prioritized_dist,
                                simple: simple_dist,
                            },
                        );
                    }
                    _ => unreachable!(),
                },
            }
        }
        // Check if binaries are allowed for this package.
        let no_binary = match no_binary {
            NoBinary::None => false,
            NoBinary::All => true,
            NoBinary::Packages(packages) => packages.contains(package_name),
        };
        VersionMap {
            inner: VersionMapInner::Lazy(VersionMapLazy {
                map,
                simple_metadata,
                no_binary,
                index: index.clone(),
                tags: tags.clone(),
                python_requirement: python_requirement.clone(),
                exclude_newer: exclude_newer.copied(),
            }),
        }
    }

    /// Return the [`DistFile`] for the given version, if any.
    pub(crate) fn get(&self, version: &Version) -> Option<ResolvableDist> {
        self.get_with_version(version)
            .map(|(_, resolvable_dist)| resolvable_dist)
    }

    /// Return the [`DistFile`] and the `Version` from the map for the given
    /// version, if any.
    ///
    /// This is useful when you depend on access to the specific `Version`
    /// stored in this map. For example, the versions `1.2.0` and `1.2` are
    /// semantically equivalent, but when converted to strings, they are
    /// distinct.
    pub(crate) fn get_with_version<'a>(
        &'a self,
        version: &Version,
    ) -> Option<(&'a Version, ResolvableDist)> {
        match self.inner {
            VersionMapInner::Eager(ref map) => map
                .get_key_value(version)
                .and_then(|(version, dist)| Some((version, dist.get()?))),
            VersionMapInner::Lazy(ref lazy) => lazy
                .get_with_version(version)
                .and_then(|(version, dist)| Some((version, dist.get()?))),
        }
    }

    /// Return an iterator over the versions and distributions.
    ///
    /// Note that the value returned in this iterator is a [`VersionMapDist`],
    /// which can be used to lazily request a [`ResolvableDist`]. This is
    /// useful in cases where one can skip materializing a full distribution
    /// for each version.
    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = (&Version, VersionMapDistHandle)> {
        match self.inner {
            VersionMapInner::Eager(ref map) => {
                either::Either::Left(map.iter().map(|(version, dist)| {
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
        }
    }

    /// Return the [`Hashes`] for the given version, if any.
    pub(crate) fn hashes(&self, version: &Version) -> Vec<Hashes> {
        match self.inner {
            VersionMapInner::Eager(ref map) => map
                .get(version)
                .map(|file| file.hashes().to_vec())
                .unwrap_or_default(),
            VersionMapInner::Lazy(ref lazy) => lazy
                .get(version)
                .map(|file| file.hashes().to_vec())
                .unwrap_or_default(),
        }
    }

    /// Returns the total number of distinct versions in this map.
    ///
    /// Note that this may include versions of distributions that are not
    /// usable in the current environment.
    pub(crate) fn len(&self) -> usize {
        match self.inner {
            VersionMapInner::Eager(ref map) => map.len(),
            VersionMapInner::Lazy(VersionMapLazy { ref map, .. }) => map.len(),
        }
    }
}

impl From<FlatDistributions> for VersionMap {
    fn from(flat_index: FlatDistributions) -> Self {
        VersionMap {
            inner: VersionMapInner::Eager(flat_index.into()),
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
/// then building a `ResolvableDist` will fail.
pub(crate) struct VersionMapDistHandle<'a> {
    inner: VersionMapDistHandleInner<'a>,
}

enum VersionMapDistHandleInner<'a> {
    Eager(&'a PrioritizedDistribution),
    Lazy {
        lazy: &'a VersionMapLazy,
        dist: &'a LazyPrioritizedDistribution,
    },
}

impl<'a> VersionMapDistHandle<'a> {
    /// Returns a resolvable distribution from this handle.
    pub(crate) fn resolvable_dist(&self) -> Option<ResolvableDist<'a>> {
        match self.inner {
            VersionMapDistHandleInner::Eager(dist) => dist.get(),
            VersionMapDistHandleInner::Lazy { lazy, dist } => Some(lazy.get_lazy(dist)?.get()?),
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
    Eager(BTreeMap<Version, PrioritizedDistribution>),
    /// Some distributions might be fully materialized (i.e., by initializing
    /// a `VersionMap` with a `FlatDistributions`), but some distributions
    /// might still be in their "raw" `SimpleMetadata` format. In this case, a
    /// `PrioritizedDistribution` isn't actually created in memory until the
    /// specific version has been requested.
    Lazy(VersionMapLazy),
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
    map: BTreeMap<Version, LazyPrioritizedDistribution>,
    /// The raw simple metadata from which `PrioritizedDistribution`s should
    /// be constructed.
    simple_metadata: OwnedArchive<SimpleMetadata>,
    /// When true, wheels aren't allowed.
    no_binary: bool,
    /// The URL of the index where this package came from.
    index: IndexUrl,
    /// The set of compatibility tags that determines whether a wheel is usable
    /// in the current environment.
    tags: Tags,
    /// The version of Python active in the current environment. This is used
    /// to determine whether a package's Python version constraint (if one
    /// exists) is satisfied or not.
    python_requirement: PythonRequirement,
    /// Whether files newer than this timestamp should be excluded or not.
    exclude_newer: Option<DateTime<Utc>>,
}

impl VersionMapLazy {
    /// Returns the distribution for the given version, if it exists.
    fn get(&self, version: &Version) -> Option<&PrioritizedDistribution> {
        self.get_with_version(version)
            .map(|(_, prioritized_dist)| prioritized_dist)
    }

    /// Returns the distribution for the given version along with the version
    /// in this map, if it exists.
    fn get_with_version(&self, version: &Version) -> Option<(&Version, &PrioritizedDistribution)> {
        let (version, lazy_dist) = self.map.get_key_value(version)?;
        let priority_dist = self.get_lazy(lazy_dist)?;
        Some((version, priority_dist))
    }

    /// Given a reference to a possibly-initialized distribution that is in
    /// this lazy map, return the corresponding distribution.
    ///
    /// When both a flat and simple distribution are present internally, they
    /// are merged automatically.
    fn get_lazy<'p>(
        &'p self,
        lazy_dist: &'p LazyPrioritizedDistribution,
    ) -> Option<&'p PrioritizedDistribution> {
        match *lazy_dist {
            LazyPrioritizedDistribution::OnlyFlat(ref dist) => Some(dist),
            LazyPrioritizedDistribution::OnlySimple(ref dist) => self.get_simple(None, dist),
            LazyPrioritizedDistribution::Both {
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
        init: Option<&'p PrioritizedDistribution>,
        simple: &'p SimplePrioritizedDistribution,
    ) -> Option<&'p PrioritizedDistribution> {
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
                if let Some(exclude_newer) = self.exclude_newer {
                    match file.upload_time_utc_ms.as_ref() {
                        Some(&upload_time) if upload_time >= exclude_newer.timestamp_millis() => {
                            continue;
                        }
                        None => {
                            warn_user_once!(
                                "{} is missing an upload date, but user provided: {exclude_newer}",
                                file.filename,
                            );
                            continue;
                        }
                        _ => {}
                    }
                }
                let yanked = file.yanked.clone().unwrap_or_default();
                let requires_python = file.requires_python.clone();
                let hash = file.hashes.clone();
                match filename {
                    DistFilename::WheelFilename(filename) => {
                        // If pre-built binaries are disabled, skip this wheel
                        if self.no_binary {
                            continue;
                        }

                        // To be compatible, the wheel must both have
                        // compatible tags _and_ have a compatible Python
                        // requirement.
                        let priority = filename.compatibility(&self.tags).filter(|_| {
                            file.requires_python
                                .as_ref()
                                .map_or(true, |requires_python| {
                                    requires_python.contains(self.python_requirement.target())
                                })
                        });
                        let dist = Dist::from_registry(
                            DistFilename::WheelFilename(filename),
                            file,
                            self.index.clone(),
                        );
                        priority_dist.insert_built(
                            dist,
                            requires_python,
                            yanked,
                            Some(hash),
                            priority,
                        );
                    }
                    DistFilename::SourceDistFilename(filename) => {
                        let dist = Dist::from_registry(
                            DistFilename::SourceDistFilename(filename),
                            file,
                            self.index.clone(),
                        );
                        priority_dist.insert_source(dist, requires_python, yanked, Some(hash));
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
}

/// Represents a possibly initialized `PrioritizedDistribution` for
/// a single version of a package.
#[derive(Debug)]
enum LazyPrioritizedDistribution {
    /// Represents a eagerly constructed distribution from a
    /// `FlatDistributions`.
    OnlyFlat(PrioritizedDistribution),
    /// Represents a lazyily constructed distribution from an index into a
    /// `VersionFiles` from `SimpleMetadata`.
    OnlySimple(SimplePrioritizedDistribution),
    /// Combines the above. This occurs when we have data from both a flat
    /// distribution and a simple distribution.
    Both {
        flat: PrioritizedDistribution,
        simple: SimplePrioritizedDistribution,
    },
}

/// Represents a lazily initialized `PrioritizedDistribution`.
#[derive(Debug)]
struct SimplePrioritizedDistribution {
    /// An offset into `SimpleMetadata` corresponding to a `SimpleMetadatum`.
    /// This provides access to a `VersionFiles` that is used to construct a
    /// `PrioritizedDistribution`.
    datum_index: usize,
    /// A lazily initialized distribution.
    ///
    /// Note that the `Option` does not represent the initialization state.
    /// The `Option` can be `None` even after initialization, for example,
    /// if initialization could not find any usable files from which to
    /// construct a distribution. (One easy way to effect this, at the time
    /// of writing, is to use `--exclude-newer 1900-01-01`.)
    dist: OnceLock<Option<PrioritizedDistribution>>,
}
