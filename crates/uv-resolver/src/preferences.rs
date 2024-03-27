use std::str::FromStr;

use rustc_hash::FxHashMap;

use pep440_rs::{Operator, Version};
use pep508_rs::{
    MarkerEnvironment, Requirement, RequirementsTxtRequirement, UnnamedRequirement, VersionOrUrl,
};
use pypi_types::{HashError, Hashes};
use requirements_txt::RequirementEntry;
use tracing::debug;
use uv_normalize::PackageName;

use crate::Exclusions;

#[derive(thiserror::Error, Debug)]
pub enum PreferenceError {
    #[error("direct URL requirements without package names are not supported: {0}")]
    Bare(UnnamedRequirement),
    #[error(transparent)]
    Hash(#[from] HashError),
}

/// A pinned requirement, as extracted from a `requirements.txt` file.
#[derive(Clone, Debug)]
pub struct Preference {
    requirement: Requirement,
    hashes: Vec<Hashes>,
}

impl Preference {
    /// Create a [`Preference`] from a [`RequirementEntry`].
    pub fn from_entry(entry: RequirementEntry) -> Result<Self, PreferenceError> {
        Ok(Self {
            requirement: match entry.requirement {
                RequirementsTxtRequirement::Pep508(requirement) => requirement,
                RequirementsTxtRequirement::Unnamed(requirement) => {
                    return Err(PreferenceError::Bare(requirement))
                }
            },
            hashes: entry
                .hashes
                .iter()
                .map(String::as_str)
                .map(Hashes::from_str)
                .collect::<Result<_, _>>()?,
        })
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
pub(crate) struct Preferences(FxHashMap<PackageName, Pin>);

impl Preferences {
    /// Create a map of pinned packages from an iterator of [`Preference`] entries.
    /// Takes ownership of the [`Preference`] entries.
    ///
    /// The provided [`Exclusions`] and [`MarkerEnvironment`] will be used to filter
    /// the preferences to an applicable step.
    pub(crate) fn from_iter<PreferenceIterator: IntoIterator<Item = Preference>>(
        preferences: PreferenceIterator,
        exclusions: &Exclusions,
        markers: &MarkerEnvironment,
    ) -> Self {
        Self(
            preferences
                .into_iter()
                .filter_map(|preference| {
                    let Preference {
                        requirement,
                        hashes,
                    } = preference;

                    if exclusions.contains(&requirement.name) {
                        debug!(
                            "Excluding {requirement} from preferences due to presence in exclusions."
                        );
                        return None;
                    }

                    // Search for, e.g., `flask==1.2.3` entries that match the current environment.
                    if !requirement.evaluate_markers(markers, &[]) {
                        debug!(
                            "Excluding {requirement} from preferences due to unmatched markers."
                        );
                        return None;
                    }
                    match requirement.version_or_url.as_ref() {
                        Some(VersionOrUrl::VersionSpecifier(version_specifiers)) =>
                         {
                            let [version_specifier] = version_specifiers.as_ref() else {
                                debug!(
                                    "Excluding {requirement} from preferences due to multiple version specifiers."
                                );
                                return None;
                            };
                            if *version_specifier.operator() != Operator::Equal {
                                debug!(
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
                        Some(VersionOrUrl::Url(_)) => {
                            debug!(
                                "Excluding {requirement} from preferences due to URL dependency."
                            );
                            None
                        }
                        _ => {
                        None
                    }
                    }

                })
                .collect(),
        )
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
    ) -> Option<&[Hashes]> {
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
    hashes: Vec<Hashes>,
}

impl Pin {
    /// Return the version of the pinned package.
    fn version(&self) -> &Version {
        &self.version
    }

    /// Return the hashes of the pinned package.
    fn hashes(&self) -> &[Hashes] {
        &self.hashes
    }
}
