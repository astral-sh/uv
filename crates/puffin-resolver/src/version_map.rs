use std::collections::btree_map::Entry;
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use tracing::{instrument, warn};

use distribution_filename::DistFilename;
use distribution_types::{Dist, IndexUrl};
use pep440_rs::VersionSpecifiers;
use platform_tags::{TagPriority, Tags};
use puffin_client::SimpleMetadata;
use puffin_normalize::PackageName;
use puffin_warnings::warn_user_once;
use pypi_types::{BaseUrl, Hashes, Yanked};

use crate::pubgrub::PubGrubVersion;
use crate::python_requirement::PythonRequirement;
use crate::yanks::AllowedYanks;

/// A map from versions to distributions.
#[derive(Debug, Default)]
pub struct VersionMap(BTreeMap<PubGrubVersion, PrioritizedDistribution>);

impl VersionMap {
    /// Initialize a [`VersionMap`] from the given metadata.
    #[instrument(skip_all, fields(package_name = % package_name))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_metadata(
        metadata: SimpleMetadata,
        package_name: &PackageName,
        index: &IndexUrl,
        base: &BaseUrl,
        tags: &Tags,
        python_requirement: &PythonRequirement,
        allowed_yanks: &AllowedYanks,
        exclude_newer: Option<&DateTime<Utc>>,
    ) -> Self {
        let mut version_map: BTreeMap<PubGrubVersion, PrioritizedDistribution> =
            BTreeMap::default();

        // Collect compatible distributions.
        for (version, files) in metadata {
            for (filename, file) in files.all() {
                // Support resolving as if it were an earlier timestamp, at least as long files have
                // upload time information.
                if let Some(exclude_newer) = exclude_newer {
                    match file.upload_time.as_ref() {
                        Some(upload_time) if upload_time >= exclude_newer => {
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

                // When resolving, exclude yanked files.
                if file.yanked.as_ref().is_some_and(Yanked::is_yanked) {
                    if allowed_yanks.allowed(package_name, &version) {
                        warn!("Allowing yanked version: {}", file.filename);
                    } else {
                        continue;
                    }
                }

                // Prioritize amongst all available files.
                let requires_python = file.requires_python.clone();
                let hash = file.hashes.clone();
                match filename {
                    DistFilename::WheelFilename(filename) => {
                        // To be compatible, the wheel must both have compatible tags _and_ have a
                        // compatible Python requirement.
                        let priority = filename.compatibility(tags).filter(|_| {
                            file.requires_python
                                .as_ref()
                                .map_or(true, |requires_python| {
                                    requires_python.contains(python_requirement.target())
                                })
                        });
                        let dist = Dist::from_registry(
                            filename.name.clone(),
                            filename.version.clone(),
                            file,
                            index.clone(),
                            base.clone(),
                        );
                        match version_map.entry(version.clone().into()) {
                            Entry::Occupied(mut entry) => {
                                entry
                                    .get_mut()
                                    .insert_built(dist, requires_python, hash, priority);
                            }
                            Entry::Vacant(entry) => {
                                entry.insert(PrioritizedDistribution::from_built(
                                    dist,
                                    requires_python,
                                    hash,
                                    priority,
                                ));
                            }
                        }
                    }
                    DistFilename::SourceDistFilename(filename) => {
                        let dist = Dist::from_registry(
                            filename.name.clone(),
                            filename.version.clone(),
                            file,
                            index.clone(),
                            base.clone(),
                        );
                        match version_map.entry(version.clone().into()) {
                            Entry::Occupied(mut entry) => {
                                entry.get_mut().insert_source(dist, requires_python, hash);
                            }
                            Entry::Vacant(entry) => {
                                entry.insert(PrioritizedDistribution::from_source(
                                    dist,
                                    requires_python,
                                    hash,
                                ));
                            }
                        }
                    }
                }
            }
        }

        Self(version_map)
    }

    /// Return the [`DistFile`] for the given version, if any.
    pub(crate) fn get(&self, version: &PubGrubVersion) -> Option<ResolvableFile> {
        self.0.get(version).and_then(PrioritizedDistribution::get)
    }

    /// Return an iterator over the versions and distributions.
    pub(crate) fn iter(
        &self,
    ) -> impl DoubleEndedIterator<Item = (&PubGrubVersion, ResolvableFile)> {
        self.0
            .iter()
            .filter_map(|(version, file)| Some((version, file.get()?)))
    }

    /// Return the [`Hashes`] for the given version, if any.
    pub(crate) fn hashes(&self, version: &PubGrubVersion) -> Vec<Hashes> {
        self.0
            .get(version)
            .map(|file| file.hashes.clone())
            .unwrap_or_default()
    }
}

/// Attach its requires-python to a [`Dist`], since downstream needs this information to filter
/// [`PrioritizedDistribution`].
#[derive(Debug)]
pub(crate) struct DistRequiresPython {
    pub(crate) dist: Dist,
    pub(crate) requires_python: Option<VersionSpecifiers>,
}

#[derive(Debug)]
struct PrioritizedDistribution {
    /// An arbitrary source distribution for the package version.
    source: Option<DistRequiresPython>,
    /// The highest-priority, platform-compatible wheel for the package version.
    compatible_wheel: Option<(DistRequiresPython, TagPriority)>,
    /// An arbitrary, platform-incompatible wheel for the package version.
    incompatible_wheel: Option<DistRequiresPython>,
    /// The hashes for each distribution.
    hashes: Vec<Hashes>,
}

impl PrioritizedDistribution {
    /// Create a new [`PrioritizedDistribution`] from the given wheel distribution.
    fn from_built(
        dist: Dist,
        requires_python: Option<VersionSpecifiers>,
        hash: Hashes,
        priority: Option<TagPriority>,
    ) -> Self {
        if let Some(priority) = priority {
            Self {
                source: None,
                compatible_wheel: Some((
                    DistRequiresPython {
                        dist,

                        requires_python,
                    },
                    priority,
                )),
                incompatible_wheel: None,
                hashes: vec![hash],
            }
        } else {
            Self {
                source: None,
                compatible_wheel: None,
                incompatible_wheel: Some(DistRequiresPython {
                    dist,
                    requires_python,
                }),
                hashes: vec![hash],
            }
        }
    }

    /// Create a new [`PrioritizedDistribution`] from the given source distribution.
    fn from_source(dist: Dist, requires_python: Option<VersionSpecifiers>, hash: Hashes) -> Self {
        Self {
            source: Some(DistRequiresPython {
                dist,
                requires_python,
            }),
            compatible_wheel: None,
            incompatible_wheel: None,
            hashes: vec![hash],
        }
    }

    /// Insert the given built distribution into the [`PrioritizedDistribution`].
    fn insert_built(
        &mut self,
        dist: Dist,
        requires_python: Option<VersionSpecifiers>,
        hash: Hashes,
        priority: Option<TagPriority>,
    ) {
        // Prefer the highest-priority, platform-compatible wheel.
        if let Some(priority) = priority {
            if let Some((.., existing_priority)) = &self.compatible_wheel {
                if priority > *existing_priority {
                    self.compatible_wheel = Some((
                        DistRequiresPython {
                            dist,
                            requires_python,
                        },
                        priority,
                    ));
                }
            } else {
                self.compatible_wheel = Some((
                    DistRequiresPython {
                        dist,
                        requires_python,
                    },
                    priority,
                ));
            }
        } else if self.incompatible_wheel.is_none() {
            self.incompatible_wheel = Some(DistRequiresPython {
                dist,
                requires_python,
            });
        }
        self.hashes.push(hash);
    }

    /// Insert the given source distribution into the [`PrioritizedDistribution`].
    fn insert_source(
        &mut self,
        dist: Dist,
        requires_python: Option<VersionSpecifiers>,
        hash: Hashes,
    ) {
        if self.source.is_none() {
            self.source = Some(DistRequiresPython {
                dist,
                requires_python,
            });
        }
        self.hashes.push(hash);
    }

    /// Return the highest-priority distribution for the package version, if any.
    fn get(&self) -> Option<ResolvableFile> {
        match (
            &self.compatible_wheel,
            &self.source,
            &self.incompatible_wheel,
        ) {
            // Prefer the highest-priority, platform-compatible wheel.
            (Some((wheel, _)), _, _) => Some(ResolvableFile::CompatibleWheel(wheel)),
            // If we have a compatible source distribution and an incompatible wheel, return the
            // wheel. We assume that all distributions have the same metadata for a given package
            // version. If a compatible source distribution exists, we assume we can build it, but
            // using the wheel is faster.
            (_, Some(source_dist), Some(wheel)) => {
                Some(ResolvableFile::IncompatibleWheel(source_dist, wheel))
            }
            // Otherwise, if we have a source distribution, return it.
            (_, Some(source_dist), _) => Some(ResolvableFile::SourceDist(source_dist)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ResolvableFile<'a> {
    /// The distribution should be resolved and installed using a source distribution.
    SourceDist(&'a DistRequiresPython),
    /// The distribution should be resolved and installed using a wheel distribution.
    CompatibleWheel(&'a DistRequiresPython),
    /// The distribution should be resolved using an incompatible wheel distribution, but
    /// installed using a source distribution.
    IncompatibleWheel(&'a DistRequiresPython, &'a DistRequiresPython),
}

impl<'a> ResolvableFile<'a> {
    /// Return the [`DistFile`] to use during resolution.
    pub(crate) fn resolve(&self) -> &DistRequiresPython {
        match *self {
            ResolvableFile::SourceDist(sdist) => sdist,
            ResolvableFile::CompatibleWheel(wheel) => wheel,
            ResolvableFile::IncompatibleWheel(_, wheel) => wheel,
        }
    }

    /// Return the [`DistFile`] to use during installation.
    pub(crate) fn install(&self) -> &DistRequiresPython {
        match *self {
            ResolvableFile::SourceDist(sdist) => sdist,
            ResolvableFile::CompatibleWheel(wheel) => wheel,
            ResolvableFile::IncompatibleWheel(sdist, _) => sdist,
        }
    }
}
