use std::path::Path;
use std::str::FromStr;

use rustc_hash::FxHashMap;
use tracing::trace;

use uv_distribution_types::IndexUrl;
use uv_normalize::PackageName;
use uv_pep440::{Operator, Version};
use uv_pep508::{MarkerTree, VersionOrUrl};
use uv_pypi_types::{HashDigest, HashDigests, HashError};
use uv_requirements_txt::{RequirementEntry, RequirementsTxtRequirement};

use crate::universal_marker::UniversalMarker;
use crate::{LockError, ResolverEnvironment};

#[derive(thiserror::Error, Debug)]
pub enum PreferenceError {
    #[error(transparent)]
    Hash(#[from] HashError),
}

/// A pinned requirement, as extracted from a `requirements.txt` file.
#[derive(Clone, Debug)]
pub struct Preference {
    name: PackageName,
    version: Version,
    /// The markers on the requirement itself (those after the semicolon).
    marker: MarkerTree,
    /// The index URL of the package, if any.
    index: PreferenceIndex,
    /// If coming from a package with diverging versions, the markers of the forks this preference
    /// is part of, otherwise `None`.
    fork_markers: Vec<UniversalMarker>,
    hashes: HashDigests,
}

impl Preference {
    /// Create a [`Preference`] from a [`RequirementEntry`].
    pub fn from_entry(entry: RequirementEntry) -> Result<Option<Self>, PreferenceError> {
        let RequirementsTxtRequirement::Named(requirement) = entry.requirement else {
            return Ok(None);
        };

        let Some(VersionOrUrl::VersionSpecifier(specifier)) = requirement.version_or_url.as_ref()
        else {
            trace!("Excluding {requirement} from preferences due to non-version specifier");
            return Ok(None);
        };

        let [specifier] = specifier.as_ref() else {
            trace!("Excluding {requirement} from preferences due to multiple version specifiers");
            return Ok(None);
        };

        if *specifier.operator() != Operator::Equal {
            trace!("Excluding {requirement} from preferences due to inexact version specifier");
            return Ok(None);
        }

        Ok(Some(Self {
            name: requirement.name,
            version: specifier.version().clone(),
            marker: requirement.marker,
            // `requirements.txt` doesn't have fork annotations.
            fork_markers: vec![],
            // `requirements.txt` doesn't allow a requirement to specify an explicit index.
            index: PreferenceIndex::Any,
            hashes: entry
                .hashes
                .iter()
                .map(String::as_str)
                .map(HashDigest::from_str)
                .collect::<Result<_, _>>()?,
        }))
    }

    /// Create a [`Preference`] from a locked distribution.
    pub fn from_lock(
        package: &crate::lock::Package,
        install_path: &Path,
    ) -> Result<Option<Self>, LockError> {
        let Some(version) = package.version() else {
            return Ok(None);
        };
        Ok(Some(Self {
            name: package.id.name.clone(),
            version: version.clone(),
            marker: MarkerTree::TRUE,
            index: PreferenceIndex::from(package.index(install_path)?),
            fork_markers: package.fork_markers().to_vec(),
            hashes: HashDigests::empty(),
        }))
    }

    /// Return the [`PackageName`] of the package for this [`Preference`].
    pub fn name(&self) -> &PackageName {
        &self.name
    }

    /// Return the [`Version`] of the package for this [`Preference`].
    pub fn version(&self) -> &Version {
        &self.version
    }
}

#[derive(Debug, Clone)]
pub enum PreferenceIndex {
    /// The preference should match to any index.
    Any,
    /// The preference should match to an implicit index.
    Implicit,
    /// The preference should match to a specific index.
    Explicit(IndexUrl),
}

impl PreferenceIndex {
    /// Returns `true` if the preference matches the given explicit [`IndexUrl`].
    pub(crate) fn matches(&self, index: &IndexUrl) -> bool {
        match self {
            Self::Any => true,
            Self::Implicit => false,
            Self::Explicit(preference) => preference == index,
        }
    }
}

impl From<Option<IndexUrl>> for PreferenceIndex {
    fn from(index: Option<IndexUrl>) -> Self {
        match index {
            Some(index) => Self::Explicit(index),
            None => Self::Implicit,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Entry {
    marker: UniversalMarker,
    index: PreferenceIndex,
    pin: Pin,
}

impl Entry {
    /// Return the [`UniversalMarker`] associated with the entry.
    pub(crate) fn marker(&self) -> &UniversalMarker {
        &self.marker
    }

    /// Return the [`IndexUrl`] associated with the entry, if any.
    pub(crate) fn index(&self) -> &PreferenceIndex {
        &self.index
    }

    /// Return the pinned data associated with the entry.
    pub(crate) fn pin(&self) -> &Pin {
        &self.pin
    }
}

/// A set of pinned packages that should be preserved during resolution, if possible.
///
/// The marker is the marker of the fork that resolved to the pin, if any.
///
/// Preferences should be prioritized first by whether their marker matches and then by the order
/// they are stored, so that a lockfile has higher precedence than sibling forks.
#[derive(Debug, Clone, Default)]
pub struct Preferences(FxHashMap<PackageName, Vec<Entry>>);

impl Preferences {
    /// Create a map of pinned packages from an iterator of [`Preference`] entries.
    ///
    /// The provided [`ResolverEnvironment`] will be used to filter the preferences
    /// to an applicable subset.
    pub fn from_iter(
        preferences: impl IntoIterator<Item = Preference>,
        env: &ResolverEnvironment,
    ) -> Self {
        let mut map = FxHashMap::<PackageName, Vec<_>>::default();
        for preference in preferences {
            // Filter non-matching preferences when resolving for an environment.
            if let Some(markers) = env.marker_environment() {
                if !preference.marker.evaluate(markers, &[]) {
                    trace!("Excluding {preference} from preferences due to unmatched markers");
                    continue;
                }

                if !preference.fork_markers.is_empty() {
                    if !preference
                        .fork_markers
                        .iter()
                        .any(|marker| marker.evaluate_no_extras(markers))
                    {
                        trace!(
                            "Excluding {preference} from preferences due to unmatched fork markers"
                        );
                        continue;
                    }
                }
            }

            // Flatten the list of markers into individual entries.
            if preference.fork_markers.is_empty() {
                map.entry(preference.name).or_default().push(Entry {
                    marker: UniversalMarker::TRUE,
                    index: preference.index,
                    pin: Pin {
                        version: preference.version,
                        hashes: preference.hashes,
                    },
                });
            } else {
                for fork_marker in preference.fork_markers {
                    map.entry(preference.name.clone()).or_default().push(Entry {
                        marker: fork_marker,
                        index: preference.index.clone(),
                        pin: Pin {
                            version: preference.version.clone(),
                            hashes: preference.hashes.clone(),
                        },
                    });
                }
            }
        }

        Self(map)
    }

    /// Insert a preference at the back.
    pub(crate) fn insert(
        &mut self,
        package_name: PackageName,
        index: Option<IndexUrl>,
        markers: UniversalMarker,
        pin: impl Into<Pin>,
    ) {
        self.0.entry(package_name).or_default().push(Entry {
            marker: markers,
            index: PreferenceIndex::from(index),
            pin: pin.into(),
        });
    }

    /// Returns an iterator over the preferences.
    pub fn iter(
        &self,
    ) -> impl Iterator<
        Item = (
            &PackageName,
            impl Iterator<Item = (&UniversalMarker, &PreferenceIndex, &Version)>,
        ),
    > {
        self.0.iter().map(|(name, preferences)| {
            (
                name,
                preferences
                    .iter()
                    .map(|entry| (&entry.marker, &entry.index, entry.pin.version())),
            )
        })
    }

    /// Return the pinned version for a package, if any.
    pub(crate) fn get(&self, package_name: &PackageName) -> &[Entry] {
        self.0
            .get(package_name)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// Return the hashes for a package, if the version matches that of the pin.
    pub(crate) fn match_hashes(
        &self,
        package_name: &PackageName,
        version: &Version,
    ) -> Option<&[HashDigest]> {
        self.0
            .get(package_name)
            .into_iter()
            .flatten()
            .find(|entry| entry.pin.version() == version)
            .map(|entry| entry.pin.hashes())
    }
}

impl std::fmt::Display for Preference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}=={}", self.name, self.version)
    }
}

/// The pinned data associated with a package in a locked `requirements.txt` file (e.g., `flask==1.2.3`).
#[derive(Debug, Clone)]
pub(crate) struct Pin {
    version: Version,
    hashes: HashDigests,
}

impl Pin {
    /// Return the version of the pinned package.
    pub(crate) fn version(&self) -> &Version {
        &self.version
    }

    /// Return the hashes of the pinned package.
    pub(crate) fn hashes(&self) -> &[HashDigest] {
        self.hashes.as_slice()
    }
}

impl From<Version> for Pin {
    fn from(version: Version) -> Self {
        Self {
            version,
            hashes: HashDigests::empty(),
        }
    }
}
