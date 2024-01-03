use std::collections::btree_map::Entry;
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use tracing::{instrument, warn};

use distribution_filename::DistFilename;
use platform_tags::{TagPriority, Tags};
use puffin_client::SimpleMetadata;
use puffin_normalize::PackageName;
use puffin_warnings::warn_user_once;
use pypi_types::Yanked;

use crate::file::{DistFile, SdistFile, WheelFile};
use crate::pubgrub::PubGrubVersion;
use crate::python_requirement::PythonRequirement;
use crate::yanks::AllowedYanks;

/// A map from versions to distributions.
#[derive(Debug, Default)]
pub struct VersionMap(BTreeMap<PubGrubVersion, PrioritizedDistribution>);

impl VersionMap {
    /// Initialize a [`VersionMap`] from the given metadata.
    #[instrument(skip_all, fields(package_name = % package_name))]
    pub(crate) fn from_metadata(
        metadata: SimpleMetadata,
        package_name: &PackageName,
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
                // upload time information
                if let Some(exclude_newer) = exclude_newer {
                    match file.upload_time.as_ref() {
                        Some(upload_time) if upload_time >= exclude_newer => {
                            continue;
                        }
                        None => {
                            warn_user_once!(
                                "{} is missing an upload date, but user provided: {}",
                                file.filename,
                                exclude_newer,
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

                match filename {
                    DistFilename::WheelFilename(filename) => {
                        // To be compatible, the wheel must both have compatible tags _and_ have a
                        // compatible Python requirement.
                        let priority = filename.compatibility(tags).filter(|_| {
                            file.requires_python
                                .as_ref()
                                .map_or(true, |requires_python| {
                                    python_requirement
                                        .versions()
                                        .all(|version| requires_python.contains(version))
                                })
                        });
                        match version_map.entry(version.clone().into()) {
                            Entry::Occupied(mut entry) => {
                                entry.get_mut().insert_built(WheelFile(file), priority);
                            }
                            Entry::Vacant(entry) => {
                                entry.insert(PrioritizedDistribution::from_built(
                                    WheelFile(file),
                                    priority,
                                ));
                            }
                        }
                    }
                    DistFilename::SourceDistFilename(_) => {
                        match version_map.entry(version.clone().into()) {
                            Entry::Occupied(mut entry) => {
                                entry.get_mut().insert_source(SdistFile(file));
                            }
                            Entry::Vacant(entry) => {
                                entry.insert(PrioritizedDistribution::from_source(SdistFile(file)));
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
}

#[derive(Debug)]
struct PrioritizedDistribution {
    /// An arbitrary source distribution for the package version.
    source: Option<DistFile>,
    /// The highest-priority, platform-compatible wheel for the package version.
    compatible_wheel: Option<(DistFile, TagPriority)>,
    /// An arbitrary, platform-incompatible wheel for the package version.
    incompatible_wheel: Option<DistFile>,
}

impl PrioritizedDistribution {
    /// Create a new [`PrioritizedDistribution`] from the given wheel distribution.
    fn from_built(dist: WheelFile, priority: Option<TagPriority>) -> Self {
        if let Some(priority) = priority {
            Self {
                source: None,
                compatible_wheel: Some((dist.into(), priority)),
                incompatible_wheel: None,
            }
        } else {
            Self {
                source: None,
                compatible_wheel: None,
                incompatible_wheel: Some(dist.into()),
            }
        }
    }

    /// Create a new [`PrioritizedDistribution`] from the given source distribution.
    fn from_source(dist: SdistFile) -> Self {
        Self {
            source: Some(dist.into()),
            compatible_wheel: None,
            incompatible_wheel: None,
        }
    }

    /// Insert the given built distribution into the [`PrioritizedDistribution`].
    fn insert_built(&mut self, file: WheelFile, priority: Option<TagPriority>) {
        // Prefer the highest-priority, platform-compatible wheel.
        if let Some(priority) = priority {
            if let Some((.., existing_priority)) = &self.compatible_wheel {
                if priority > *existing_priority {
                    self.compatible_wheel = Some((file.into(), priority));
                }
            } else {
                self.compatible_wheel = Some((file.into(), priority));
            }
        } else if self.incompatible_wheel.is_none() {
            self.incompatible_wheel = Some(file.into());
        }
    }

    /// Insert the given source distribution into the [`PrioritizedDistribution`].
    fn insert_source(&mut self, file: SdistFile) {
        if self.source.is_none() {
            self.source = Some(file.into());
        }
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
            (_, Some(sdist), Some(wheel)) => Some(ResolvableFile::IncompatibleWheel(sdist, wheel)),
            // Otherwise, if we have a source distribution, return it.
            (_, Some(sdist), _) => Some(ResolvableFile::SourceDist(sdist)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ResolvableFile<'a> {
    /// The distribution should be resolved and installed using a source distribution.
    SourceDist(&'a DistFile),
    /// The distribution should be resolved and installed using a wheel distribution.
    CompatibleWheel(&'a DistFile),
    /// The distribution should be resolved using an incompatible wheel distribution, but
    /// installed using a source distribution.
    IncompatibleWheel(&'a DistFile, &'a DistFile),
}

impl<'a> ResolvableFile<'a> {
    /// Return the [`DistFile`] to use during resolution.
    pub(crate) fn resolve(&self) -> &DistFile {
        match self {
            ResolvableFile::SourceDist(sdist) => sdist,
            ResolvableFile::CompatibleWheel(wheel) => wheel,
            ResolvableFile::IncompatibleWheel(_, wheel) => wheel,
        }
    }

    /// Return the [`DistFile`] to use during installation.
    pub(crate) fn install(&self) -> &DistFile {
        match self {
            ResolvableFile::SourceDist(sdist) => sdist,
            ResolvableFile::CompatibleWheel(wheel) => wheel,
            ResolvableFile::IncompatibleWheel(sdist, _) => sdist,
        }
    }
}
