use std::str::FromStr;

use rustc_hash::FxHashMap;
use url::Url;

use distribution_types::{DistributionMetadata, HashPolicy, PackageId, UnresolvedRequirement};
use pep508_rs::MarkerEnvironment;
use pypi_types::{HashDigest, HashError, Requirement, RequirementSource};
use uv_normalize::PackageName;

#[derive(Debug, Default, Clone)]
pub enum HashStrategy {
    /// No hash policy is specified.
    #[default]
    None,
    /// Hashes should be generated (specifically, a SHA-256 hash), but not validated.
    Generate,
    /// Hashes should be validated, if present, but ignored if absent.
    ///
    /// If necessary, hashes should be generated to ensure that the archive is valid.
    Verify(FxHashMap<PackageId, Vec<HashDigest>>),
    /// Hashes should be validated against a pre-defined list of hashes.
    ///
    /// If necessary, hashes should be generated to ensure that the archive is valid.
    Require(FxHashMap<PackageId, Vec<HashDigest>>),
}

impl HashStrategy {
    /// Return the [`HashPolicy`] for the given distribution.
    pub fn get<T: DistributionMetadata>(&self, distribution: &T) -> HashPolicy {
        match self {
            Self::None => HashPolicy::None,
            Self::Generate => HashPolicy::Generate,
            Self::Verify(hashes) => {
                if let Some(hashes) = hashes.get(&distribution.package_id()) {
                    HashPolicy::Validate(hashes.as_slice())
                } else {
                    HashPolicy::None
                }
            }
            Self::Require(hashes) => HashPolicy::Validate(
                hashes
                    .get(&distribution.package_id())
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
            ),
        }
    }

    /// Return the [`HashPolicy`] for the given registry-based package.
    pub fn get_package(&self, name: &PackageName) -> HashPolicy {
        match self {
            Self::None => HashPolicy::None,
            Self::Generate => HashPolicy::Generate,
            Self::Verify(hashes) => {
                if let Some(hashes) = hashes.get(&PackageId::from_registry(name.clone())) {
                    HashPolicy::Validate(hashes.as_slice())
                } else {
                    HashPolicy::None
                }
            }
            Self::Require(hashes) => HashPolicy::Validate(
                hashes
                    .get(&PackageId::from_registry(name.clone()))
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
            ),
        }
    }

    /// Return the [`HashPolicy`] for the given direct URL package.
    pub fn get_url(&self, url: &Url) -> HashPolicy {
        match self {
            Self::None => HashPolicy::None,
            Self::Generate => HashPolicy::Generate,
            Self::Verify(hashes) => {
                if let Some(hashes) = hashes.get(&PackageId::from_url(url)) {
                    HashPolicy::Validate(hashes.as_slice())
                } else {
                    HashPolicy::None
                }
            }
            Self::Require(hashes) => HashPolicy::Validate(
                hashes
                    .get(&PackageId::from_url(url))
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
            ),
        }
    }

    /// Returns `true` if the given registry-based package is allowed.
    pub fn allows_package(&self, name: &PackageName) -> bool {
        match self {
            Self::None => true,
            Self::Generate => true,
            Self::Verify(_) => true,
            Self::Require(hashes) => hashes.contains_key(&PackageId::from_registry(name.clone())),
        }
    }

    /// Returns `true` if the given direct URL package is allowed.
    pub fn allows_url(&self, url: &Url) -> bool {
        match self {
            Self::None => true,
            Self::Generate => true,
            Self::Verify(_) => true,
            Self::Require(hashes) => hashes.contains_key(&PackageId::from_url(url)),
        }
    }

    /// Generate the required hashes from a set of [`UnresolvedRequirement`] entries.
    ///
    /// When the environment is not given, this treats all marker expressions
    /// that reference the environment as true. In other words, it does
    /// environment independent expression evaluation. (Which in turn devolves
    /// to "only evaluate marker expressions that reference an extra name.")
    pub fn require<'a>(
        requirements: impl Iterator<Item = (&'a UnresolvedRequirement, &'a [String])>,
        markers: Option<&MarkerEnvironment>,
    ) -> Result<Self, HashStrategyError> {
        let mut hashes = FxHashMap::<PackageId, Vec<HashDigest>>::default();

        // For each requirement, map from name to allowed hashes. We use the last entry for each
        // package.
        for (requirement, digests) in requirements {
            if !requirement.evaluate_markers(markers, &[]) {
                continue;
            }

            // Every requirement must be either a pinned version or a direct URL.
            let id = match &requirement {
                UnresolvedRequirement::Named(requirement) => {
                    Self::pin(requirement).ok_or_else(|| {
                        HashStrategyError::UnpinnedRequirement(
                            requirement.to_string(),
                            HashCheckingMode::Require,
                        )
                    })?
                }
                UnresolvedRequirement::Unnamed(requirement) => {
                    // Direct URLs are always allowed.
                    PackageId::from_url(&requirement.url.verbatim)
                }
            };

            // Every requirement must include a hash.
            if digests.is_empty() {
                return Err(HashStrategyError::MissingHashes(
                    requirement.to_string(),
                    HashCheckingMode::Require,
                ));
            }

            // Parse the hashes.
            let digests = digests
                .iter()
                .map(|digest| HashDigest::from_str(digest))
                .collect::<Result<Vec<_>, _>>()?;

            hashes.insert(id, digests);
        }

        Ok(Self::Require(hashes))
    }

    /// Generate the hashes to verify from a set of [`UnresolvedRequirement`] entries.
    pub fn verify<'a>(
        requirements: impl Iterator<Item = (&'a UnresolvedRequirement, &'a [String])>,
        markers: Option<&MarkerEnvironment>,
    ) -> Result<Self, HashStrategyError> {
        let mut hashes = FxHashMap::<PackageId, Vec<HashDigest>>::default();

        // For each requirement, map from name to allowed hashes. We use the last entry for each
        // package.
        for (requirement, digests) in requirements {
            if !requirement.evaluate_markers(markers, &[]) {
                continue;
            }

            // Hashes are optional in this mode.
            if digests.is_empty() {
                continue;
            }

            // Parse the hashes.
            let digests = digests
                .iter()
                .map(|digest| HashDigest::from_str(digest))
                .collect::<Result<Vec<_>, _>>()?;

            // Every requirement must be either a pinned version or a direct URL.
            let id = match &requirement {
                UnresolvedRequirement::Named(requirement) => {
                    Self::pin(requirement).ok_or_else(|| {
                        HashStrategyError::UnpinnedRequirement(
                            requirement.to_string(),
                            HashCheckingMode::Verify,
                        )
                    })?
                }
                UnresolvedRequirement::Unnamed(requirement) => {
                    // Direct URLs are always allowed.
                    PackageId::from_url(&requirement.url.verbatim)
                }
            };

            hashes.insert(id, digests);
        }

        Ok(Self::Verify(hashes))
    }

    /// Pin a [`Requirement`] to a [`PackageId`], if possible.
    fn pin(requirement: &Requirement) -> Option<PackageId> {
        match &requirement.source {
            RequirementSource::Registry { specifier, .. } => {
                // Must be a single specifier.
                let [specifier] = specifier.as_ref() else {
                    return None;
                };

                // Must be pinned to a specific version.
                if *specifier.operator() != pep440_rs::Operator::Equal {
                    return None;
                }

                Some(PackageId::from_registry(requirement.name.clone()))
            }
            RequirementSource::Url { url, .. }
            | RequirementSource::Git { url, .. }
            | RequirementSource::Path { url, .. } => Some(PackageId::from_url(url)),
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum HashCheckingMode {
    Require,
    Verify,
}

impl std::fmt::Display for HashCheckingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Require => write!(f, "--require-hashes"),
            Self::Verify => write!(f, "--verify-hashes"),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum HashStrategyError {
    #[error(transparent)]
    Hash(#[from] HashError),
    #[error(
        "In `{1}` mode, all requirement must have their versions pinned with `==`, but found: {0}"
    )]
    UnpinnedRequirement(String, HashCheckingMode),
    #[error("In `{1}` mode, all requirement must have a hash, but none were provided for: {0}")]
    MissingHashes(String, HashCheckingMode),
}
