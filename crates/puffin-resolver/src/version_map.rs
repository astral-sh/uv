use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use tracing::warn;

use distribution_filename::{SourceDistFilename, WheelFilename};
use pep508_rs::MarkerEnvironment;
use platform_tags::{TagPriority, Tags};
use puffin_interpreter::Interpreter;
use puffin_macros::warn_once;
use puffin_normalize::PackageName;
use pypi_types::{SimpleJson, Yanked};

use crate::file::{DistFile, SdistFile, WheelFile};
use crate::pubgrub::PubGrubVersion;
use crate::yanks::AllowedYanks;

/// A map from versions to distributions.
#[derive(Debug, Default)]
pub struct VersionMap(BTreeMap<PubGrubVersion, PrioritizedDistribution>);

impl VersionMap {
    /// Initialize a [`VersionMap`] from the given metadata.
    pub(crate) fn from_metadata(
        metadata: SimpleJson,
        package_name: &PackageName,
        tags: &Tags,
        markers: &MarkerEnvironment,
        interpreter: &Interpreter,
        allowed_yanks: &AllowedYanks,
        exclude_newer: Option<&DateTime<Utc>>,
    ) -> Self {
        let mut version_map: BTreeMap<PubGrubVersion, PrioritizedDistribution> =
            BTreeMap::default();

        // Group the distributions by version and kind, discarding any incompatible
        // distributions.
        for file in metadata.files {
            // Only add dists compatible with the python version. This is relevant for source
            // distributions which give no other indication of their compatibility and wheels which
            // may be tagged `py3-none-any` but have `requires-python: ">=3.9"`.
            // TODO(konstin): https://github.com/astral-sh/puffin/issues/406
            if let Some(requires_python) = file.requires_python.as_ref() {
                // The interpreter and marker version are often the same, but can differ. For
                // example, if the user is resolving against a target Python version passed in
                // via the command-line, that version will differ from the interpreter version.
                let interpreter_version = interpreter.version();
                let marker_version = &markers.python_version.version;
                if !requires_python.contains(interpreter_version)
                    || !requires_python.contains(marker_version)
                {
                    continue;
                }
            }

            // Support resolving as if it were an earlier timestamp, at least as long files have
            // upload time information
            if let Some(exclude_newer) = exclude_newer {
                match file.upload_time.as_ref() {
                    Some(upload_time) if upload_time >= exclude_newer => {
                        continue;
                    }
                    None => {
                        warn_once!(
                            "{} is missing an upload date, but user provided {}",
                            file.filename,
                            exclude_newer,
                        );
                        continue;
                    }
                    _ => {}
                }
            }

            if let Ok(filename) = WheelFilename::from_str(file.filename.as_str()) {
                // When resolving, exclude yanked files.
                if file.yanked.as_ref().is_some_and(Yanked::is_yanked) {
                    if allowed_yanks.allowed(package_name, &filename.version) {
                        warn!("Allowing yanked version: {}", file.filename);
                    } else {
                        continue;
                    }
                }

                let priority = filename.compatibility(tags);

                match version_map.entry(filename.version.into()) {
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
            } else if let Ok(filename) =
                SourceDistFilename::parse(file.filename.as_str(), package_name)
            {
                // When resolving, exclude yanked files.
                if file.yanked.as_ref().is_some_and(Yanked::is_yanked) {
                    if allowed_yanks.allowed(package_name, &filename.version) {
                        warn!("Allowing yanked version: {}", file.filename);
                    } else {
                        continue;
                    }
                }

                match version_map.entry(filename.version.into()) {
                    Entry::Occupied(mut entry) => {
                        entry.get_mut().insert_source(SdistFile(file));
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(PrioritizedDistribution::from_source(SdistFile(file)));
                    }
                }
            }
        }

        Self(version_map)
    }

    /// Return the [`DistFile`] for the given version, if any.
    pub(crate) fn get(&self, version: &PubGrubVersion) -> Option<&DistFile> {
        self.0.get(version).and_then(|file| file.get())
    }

    /// Return an iterator over the versions and distributions.
    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = (&PubGrubVersion, &DistFile)> {
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
    fn get(&self) -> Option<&DistFile> {
        match (
            &self.compatible_wheel,
            &self.source,
            &self.incompatible_wheel,
        ) {
            // Prefer the highest-priority, platform-compatible wheel.
            (Some((file, _)), _, _) => Some(file),
            // If we have a source distribution and an incompatible wheel, return the wheel.
            // We assume that all distributions have the same metadata for a given package version.
            // If a source distribution exists, we assume we can build it, but using the wheel is
            // faster.
            (_, Some(_), Some(file)) => Some(file),
            // Otherwise, return the source distribution.
            (_, Some(file), _) => Some(file),
            _ => None,
        }
    }
}
