use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::str::FromStr;

use distribution_filename::{SourceDistFilename, WheelFilename};
use pep440_rs::Version;
use platform_tags::{TagPriority, Tags};
use puffin_normalize::PackageName;
use pypi_types::SimpleJson;

use crate::file::{DistFile, SdistFile, WheelFile};
use crate::pubgrub::PubGrubVersion;

/// A map from versions to distributions.
#[derive(Debug, Default)]
pub(crate) struct VersionMap(BTreeMap<PubGrubVersion, ScoreDistribution>);

impl VersionMap {
    /// Initialize a [`VersionMap`] from the given metadata.
    pub(crate) fn from_metadata(
        metadata: SimpleJson,
        package_name: &PackageName,
        tags: &Tags,
        python_version: &Version,
    ) -> Self {
        let mut map = BTreeMap::default();

        // Group the distributions by version and kind, discarding any incompatible
        // distributions.
        for file in metadata.files {
            // Only add dists compatible with the python version. This is relevant for source
            // distributions which give no other indication of their compatibility and wheels which
            // may be tagged `py3-none-any` but have `requires-python: ">=3.9"`.
            // TODO(konstin): https://github.com/astral-sh/puffin/issues/406
            if !file
                .requires_python
                .as_ref()
                .map_or(true, |requires_python| {
                    requires_python.contains(python_version)
                })
            {
                continue;
            }

            // When resolving, exclude yanked files.
            // TODO(konstin): When we fail resolving due to a dependency locked to yanked version,
            // we should tell the user.
            if file.yanked.is_yanked() {
                continue;
            }

            if let Ok(filename) = WheelFilename::from_str(file.filename.as_str()) {
                let priority = filename.compatibility(tags);

                match map.entry(filename.version.into()) {
                    Entry::Occupied(mut entry) => {
                        match entry.get() {
                            ScoreDistribution::Sdist(_) => {
                                // Prefer wheels over source distributions.
                                entry.insert(ScoreDistribution::Wheel(
                                    DistFile::from(WheelFile(file)),
                                    priority,
                                ));
                            }
                            ScoreDistribution::Wheel(.., existing) => {
                                // Prefer wheels with higher priority.
                                if priority > *existing {
                                    entry.insert(ScoreDistribution::Wheel(
                                        DistFile::from(WheelFile(file)),
                                        priority,
                                    ));
                                }
                            }
                        }
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(ScoreDistribution::Wheel(
                            DistFile::from(WheelFile(file)),
                            priority,
                        ));
                    }
                }
            } else if let Ok(filename) =
                SourceDistFilename::parse(file.filename.as_str(), package_name)
            {
                if let Entry::Vacant(entry) = map.entry(filename.version.into()) {
                    entry.insert(ScoreDistribution::Sdist(DistFile::from(SdistFile(file))));
                }
            }
        }

        Self(map)
    }

    /// Return the [`DistFile`] for the given version, if any.
    pub(crate) fn get(&self, version: &PubGrubVersion) -> Option<&DistFile> {
        self.0.get(version).map(|file| match file {
            ScoreDistribution::Sdist(file) => file,
            ScoreDistribution::Wheel(file, ..) => file,
        })
    }

    /// Return an iterator over the versions and distributions.
    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = (&PubGrubVersion, &DistFile)> {
        self.0.iter().map(|(version, file)| {
            (
                version,
                match file {
                    ScoreDistribution::Sdist(file) => file,
                    ScoreDistribution::Wheel(file, ..) => file,
                },
            )
        })
    }
}

#[derive(Debug)]
enum ScoreDistribution {
    Sdist(DistFile),
    Wheel(DistFile, Option<TagPriority>),
}
