use std::str::FromStr;
use std::sync::Arc;

use rustc_hash::FxHashMap;
use tracing::trace;

use distribution_types::{Requirement, RequirementSource};
use pep440_rs::{Operator, Version};
use pep508_rs::MarkerEnvironment;
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
    requirement: Requirement,
    hashes: Vec<HashDigest>,
}

impl Preference {
    /// Create a [`Preference`] from a [`RequirementEntry`].
    pub fn from_entry(entry: RequirementEntry) -> Result<Option<Self>, PreferenceError> {
        Ok(Some(Self {
            requirement: match entry.requirement {
                RequirementsTxtRequirement::Named(requirement) => Requirement::from(requirement),
                RequirementsTxtRequirement::Unnamed(_) => {
                    return Ok(None);
                }
            },
            hashes: entry
                .hashes
                .iter()
                .map(String::as_str)
                .map(HashDigest::from_str)
                .collect::<Result<_, _>>()?,
        }))
    }

    /// Create a [`Preference`] from a [`Requirement`].
    pub fn from_requirement(requirement: Requirement) -> Self {
        Self {
            requirement,
            hashes: Vec::new(),
        }
    }

    /// Return the name of the package for this preference.
    pub fn name(&self) -> &PackageName {
        &self.requirement.name
    }

    /// Return the [`Requirement`] for this preference.
    pub fn requirement(&self) -> &Requirement {
        &self.requirement
    }
}

/// A set of pinned packages that should be preserved during resolution, if possible.
#[derive(Debug, Clone)]
pub(crate) struct Preferences(Arc<FxHashMap<PackageName, Pin>>);

impl Preferences {
    /// Create a map of pinned packages from an iterator of [`Preference`] entries.
    /// Takes ownership of the [`Preference`] entries.
    ///
    /// The provided [`MarkerEnvironment`] will be used to filter  the preferences
    /// to an applicable subset.
    pub(crate) fn from_iter<PreferenceIterator: IntoIterator<Item = Preference>>(
        preferences: PreferenceIterator,
        markers: Option<&MarkerEnvironment>,
    ) -> Self {
        // TODO(zanieb): We should explicitly ensure that when a package name is seen multiple times
        // that the newest or oldest version is preferred dependning on the resolution strategy;
        // right now, the order is dependent on the given iterator.
        let preferences =
            preferences
                .into_iter()
                .filter_map(|preference| {
                    let Preference {
                        requirement,
                        hashes,
                    } = preference;

                    // Search for, e.g., `flask==1.2.3` entries that match the current environment.
                    if !requirement.evaluate_markers(markers, &[]) {
                        trace!(
                            "Excluding {requirement} from preferences due to unmatched markers."
                        );
                        return None;
                    }
                    match &requirement.source {
                        RequirementSource::Registry { specifier, ..} => {
                            let [version_specifier] = specifier.as_ref() else {
                                    trace!(
                                    "Excluding {requirement} from preferences due to multiple version specifiers."
                                );
                                    return None;
                                };
                                if *version_specifier.operator() != Operator::Equal {
                                    trace!(
                                    "Excluding {requirement} from preferences due to inexact version specifier."
                                );
                                    return None;
                                }
                                Some((
                                    requirement.name,
                                    Pin {
                                        version: version_specifier.version().clone(),
                                        hashes,
                                    },
                                ))
                            }
                        RequirementSource::Url {..} | RequirementSource::Git { .. } | RequirementSource::Path { .. }=> {
                            trace!(
                                "Excluding {requirement} from preferences due to URL dependency."
                            );
                            None
                        }
                    }
                })
                .collect();

        Self(Arc::new(preferences))
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

/// The pinned data associated with a package in a locked `requirements.txt` file (e.g., `flask==1.2.3`).
#[derive(Debug, Clone)]
struct Pin {
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
