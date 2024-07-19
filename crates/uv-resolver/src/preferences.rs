use std::collections::hash_map::Entry;
use std::str::FromStr;

use rustc_hash::FxHashMap;
use tracing::trace;

use distribution_types::{InstalledDist, InstalledMetadata, InstalledVersion, Name};
use pep440_rs::{Operator, Version};
use pep508_rs::{MarkerEnvironment, MarkerTree, VersionOrUrl};
use pypi_types::{HashDigest, HashError};
use requirements_txt::{RequirementEntry, RequirementsTxtRequirement};
use uv_normalize::PackageName;

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
    marker: Option<MarkerTree>,
    hashes: Vec<HashDigest>,
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
            hashes: entry
                .hashes
                .iter()
                .map(String::as_str)
                .map(HashDigest::from_str)
                .collect::<Result<_, _>>()?,
        }))
    }

    /// Create a [`Preference`] from an installed distribution.
    pub fn from_installed(dist: &InstalledDist) -> Self {
        let version = match dist.installed_version() {
            InstalledVersion::Version(version) => version,
            InstalledVersion::Url(_, version) => version,
        };
        Self {
            name: dist.name().clone(),
            version: version.clone(),
            marker: None,
            hashes: Vec::new(),
        }
    }

    /// Create a [`Preference`] from a locked distribution.
    pub fn from_lock(dist: &crate::lock::Distribution) -> Self {
        Self {
            name: dist.id.name.clone(),
            version: dist.id.version.clone(),
            marker: None,
            hashes: Vec::new(),
        }
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

/// A set of pinned packages that should be preserved during resolution, if possible.
#[derive(Debug, Clone, Default)]
pub struct Preferences(FxHashMap<PackageName, Pin>);

impl Preferences {
    /// Create a map of pinned packages from an iterator of [`Preference`] entries.
    /// Takes ownership of the [`Preference`] entries.
    ///
    /// The provided [`MarkerEnvironment`] will be used to filter  the preferences
    /// to an applicable subset.
    pub fn from_iter<PreferenceIterator: IntoIterator<Item = Preference>>(
        preferences: PreferenceIterator,
        markers: Option<&MarkerEnvironment>,
    ) -> Self {
        // TODO(zanieb): We should explicitly ensure that when a package name is seen multiple times
        // that the newest or oldest version is preferred depending on the resolution strategy;
        // right now, the order is dependent on the given iterator.
        let preferences = preferences
            .into_iter()
            .filter_map(|preference| {
                if preference.marker.as_ref().map_or(true, |marker| {
                    marker.evaluate_optional_environment(markers, &[])
                }) {
                    Some((
                        preference.name,
                        Pin {
                            version: preference.version,
                            hashes: preference.hashes,
                        },
                    ))
                } else {
                    trace!("Excluding {preference} from preferences due to unmatched markers");
                    None
                }
            })
            .collect();

        Self(preferences)
    }

    /// Return the [`Entry`] for a package in the preferences.
    pub fn entry(&mut self, package_name: PackageName) -> Entry<PackageName, Pin> {
        self.0.entry(package_name)
    }

    /// Returns an iterator over the preferences.
    pub fn iter(&self) -> impl Iterator<Item = (&PackageName, &Version)> {
        self.0.iter().map(|(name, pin)| (name, pin.version()))
    }

    /// Return the pinned version for a package, if any.
    pub(crate) fn version(&self, package_name: &PackageName) -> Option<&Version> {
        self.0.get(package_name).map(Pin::version)
    }

    /// Return the hashes for a package, if the version matches that of the pin.
    pub(crate) fn match_hashes(
        &self,
        package_name: &PackageName,
        version: &Version,
    ) -> Option<&[HashDigest]> {
        self.0
            .get(package_name)
            .filter(|pin| pin.version() == version)
            .map(Pin::hashes)
    }
}

impl std::fmt::Display for Preference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}=={}", self.name, self.version)
    }
}

/// The pinned data associated with a package in a locked `requirements.txt` file (e.g., `flask==1.2.3`).
#[derive(Debug, Clone)]
pub struct Pin {
    version: Version,
    hashes: Vec<HashDigest>,
}

impl Pin {
    /// Return the version of the pinned package.
    fn version(&self) -> &Version {
        &self.version
    }

    /// Return the hashes of the pinned package.
    fn hashes(&self) -> &[HashDigest] {
        &self.hashes
    }
}

impl From<Version> for Pin {
    fn from(version: Version) -> Self {
        Self {
            version,
            hashes: Vec::new(),
        }
    }
}
