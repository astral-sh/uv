use std::collections::btree_map::Entry;
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use tracing::{instrument, warn};

use distribution_filename::DistFilename;
use distribution_types::prioritized_distribution::{PrioritizedDistribution, ResolvableDist};
use distribution_types::{Dist, IndexUrl};
use platform_tags::Tags;
use puffin_client::{FlatIndex, SimpleMetadata};
use puffin_normalize::PackageName;
use puffin_warnings::warn_user_once;
use pypi_types::{BaseUrl, Yanked};

use crate::pubgrub::PubGrubVersion;
use crate::python_requirement::PythonRequirement;
use crate::yanks::AllowedYanks;

/// A map from versions to distributions.
#[derive(Debug, Default, Clone)]
pub struct VersionMap(BTreeMap<PubGrubVersion, PrioritizedDistribution>);

impl VersionMap {
    /// Initialize a [`VersionMap`] from the given metadata.
    #[instrument(skip_all, fields(package_name))]
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
        flat_index: Option<FlatIndex<PubGrubVersion>>,
    ) -> Self {
        // If we have packages of the same name from find links, gives them priority, otherwise start empty
        let mut version_map: BTreeMap<PubGrubVersion, PrioritizedDistribution> =
            flat_index.map(|overrides| overrides.0).unwrap_or_default();

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

                let requires_python = file.requires_python.clone();
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
                                    .insert_built(dist, requires_python, priority);
                            }
                            Entry::Vacant(entry) => {
                                entry.insert(PrioritizedDistribution::from_built(
                                    dist,
                                    requires_python,
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
                                entry.get_mut().insert_source(dist, requires_python);
                            }
                            Entry::Vacant(entry) => {
                                entry.insert(PrioritizedDistribution::from_source(
                                    dist,
                                    requires_python,
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
    pub(crate) fn get(&self, version: &PubGrubVersion) -> Option<ResolvableDist> {
        self.0.get(version).and_then(PrioritizedDistribution::get)
    }

    /// Return an iterator over the versions and distributions.
    pub(crate) fn iter(
        &self,
    ) -> impl DoubleEndedIterator<Item = (&PubGrubVersion, ResolvableDist)> {
        self.0
            .iter()
            .filter_map(|(version, dist)| Some((version, dist.get()?)))
    }
}
