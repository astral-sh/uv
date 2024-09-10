use rustc_hash::FxHashMap;
use std::str::FromStr;
use std::sync::Arc;
use url::Url;

use distribution_types::{
    DistributionMetadata, HashPolicy, Name, Resolution, UnresolvedRequirement, VersionId,
};
use pep440_rs::Version;
use pypi_types::{
    HashDigest, HashError, Hashes, Requirement, RequirementSource, ResolverMarkerEnvironment,
};
use uv_configuration::HashCheckingMode;
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
    Verify(Arc<FxHashMap<VersionId, Vec<HashDigest>>>),
    /// Hashes should be validated against a pre-defined list of hashes.
    ///
    /// If necessary, hashes should be generated to ensure that the archive is valid.
    Require(Arc<FxHashMap<VersionId, Vec<HashDigest>>>),
}

impl HashStrategy {
    /// Return the [`HashPolicy`] for the given distribution.
    pub fn get<T: DistributionMetadata>(&self, distribution: &T) -> HashPolicy {
        match self {
            Self::None => HashPolicy::None,
            Self::Generate => HashPolicy::Generate,
            Self::Verify(hashes) => {
                if let Some(hashes) = hashes.get(&distribution.version_id()) {
                    HashPolicy::Validate(hashes.as_slice())
                } else {
                    HashPolicy::None
                }
            }
            Self::Require(hashes) => HashPolicy::Validate(
                hashes
                    .get(&distribution.version_id())
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
            ),
        }
    }

    /// Return the [`HashPolicy`] for the given registry-based package.
    pub fn get_package(&self, name: &PackageName, version: &Version) -> HashPolicy {
        match self {
            Self::None => HashPolicy::None,
            Self::Generate => HashPolicy::Generate,
            Self::Verify(hashes) => {
                if let Some(hashes) =
                    hashes.get(&VersionId::from_registry(name.clone(), version.clone()))
                {
                    HashPolicy::Validate(hashes.as_slice())
                } else {
                    HashPolicy::None
                }
            }
            Self::Require(hashes) => HashPolicy::Validate(
                hashes
                    .get(&VersionId::from_registry(name.clone(), version.clone()))
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
                if let Some(hashes) = hashes.get(&VersionId::from_url(url)) {
                    HashPolicy::Validate(hashes.as_slice())
                } else {
                    HashPolicy::None
                }
            }
            Self::Require(hashes) => HashPolicy::Validate(
                hashes
                    .get(&VersionId::from_url(url))
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
            ),
        }
    }

    /// Returns `true` if the given registry-based package is allowed.
    pub fn allows_package(&self, name: &PackageName, version: &Version) -> bool {
        match self {
            Self::None => true,
            Self::Generate => true,
            Self::Verify(_) => true,
            Self::Require(hashes) => {
                hashes.contains_key(&VersionId::from_registry(name.clone(), version.clone()))
            }
        }
    }

    /// Returns `true` if the given direct URL package is allowed.
    pub fn allows_url(&self, url: &Url) -> bool {
        match self {
            Self::None => true,
            Self::Generate => true,
            Self::Verify(_) => true,
            Self::Require(hashes) => hashes.contains_key(&VersionId::from_url(url)),
        }
    }

    /// Generate the required hashes from a set of [`UnresolvedRequirement`] entries.
    ///
    /// When the environment is not given, this treats all marker expressions
    /// that reference the environment as true. In other words, it does
    /// environment independent expression evaluation. (Which in turn devolves
    /// to "only evaluate marker expressions that reference an extra name.")
    pub fn from_requirements<'a>(
        requirements: impl Iterator<Item = (&'a UnresolvedRequirement, &'a [String])>,
        constraints: impl Iterator<Item = (&'a Requirement, &'a [String])>,
        marker_env: Option<&ResolverMarkerEnvironment>,
        mode: HashCheckingMode,
    ) -> Result<Self, HashStrategyError> {
        let mut constraint_hashes = FxHashMap::<VersionId, Vec<HashDigest>>::default();

        // First, index the constraints by name.
        for (requirement, digests) in constraints {
            if !requirement
                .evaluate_markers(marker_env.map(ResolverMarkerEnvironment::markers), &[])
            {
                continue;
            }

            // Every constraint must be a pinned version.
            let Some(id) = Self::pin(requirement) else {
                if mode.is_require() {
                    return Err(HashStrategyError::UnpinnedRequirement(
                        requirement.to_string(),
                        mode,
                    ));
                }
                continue;
            };

            let digests = if digests.is_empty() {
                // If there are no hashes, and the distribution is URL-based, attempt to extract
                // it from the fragment.
                requirement
                    .hashes()
                    .map(Hashes::into_digests)
                    .unwrap_or_default()
            } else {
                // Parse the hashes.
                digests
                    .iter()
                    .map(|digest| HashDigest::from_str(digest))
                    .collect::<Result<Vec<_>, _>>()?
            };

            if digests.is_empty() {
                continue;
            }

            constraint_hashes.insert(id, digests);
        }

        // For each requirement, map from name to allowed hashes. We use the last entry for each
        // package.
        let mut requirement_hashes = FxHashMap::<VersionId, Vec<HashDigest>>::default();
        for (requirement, digests) in requirements {
            if !requirement
                .evaluate_markers(marker_env.map(ResolverMarkerEnvironment::markers), &[])
            {
                continue;
            }

            // Every requirement must be either a pinned version or a direct URL.
            let id = match &requirement {
                UnresolvedRequirement::Named(requirement) => {
                    if let Some(id) = Self::pin(requirement) {
                        id
                    } else {
                        if mode.is_require() {
                            return Err(HashStrategyError::UnpinnedRequirement(
                                requirement.to_string(),
                                mode,
                            ));
                        }
                        continue;
                    }
                }
                UnresolvedRequirement::Unnamed(requirement) => {
                    // Direct URLs are always allowed.
                    VersionId::from_url(&requirement.url.verbatim)
                }
            };

            let digests = if digests.is_empty() {
                // If there are no hashes, and the distribution is URL-based, attempt to extract
                // it from the fragment.
                requirement
                    .hashes()
                    .map(Hashes::into_digests)
                    .unwrap_or_default()
            } else {
                // Parse the hashes.
                digests
                    .iter()
                    .map(|digest| HashDigest::from_str(digest))
                    .collect::<Result<Vec<_>, _>>()?
            };

            let digests = if let Some(constraint) = constraint_hashes.remove(&id) {
                if digests.is_empty() {
                    // If there are _only_ hashes on the constraints, use them.
                    constraint
                } else {
                    // If there are constraint and requirement hashes, take the intersection.
                    let intersection: Vec<_> = digests
                        .into_iter()
                        .filter(|digest| constraint.contains(digest))
                        .collect();
                    if intersection.is_empty() {
                        return Err(HashStrategyError::NoIntersection(
                            requirement.to_string(),
                            mode,
                        ));
                    }
                    intersection
                }
            } else {
                digests
            };

            // Under `--require-hashes`, every requirement must include a hash.
            if digests.is_empty() {
                if mode.is_require() {
                    return Err(HashStrategyError::MissingHashes(
                        requirement.to_string(),
                        mode,
                    ));
                }
                continue;
            }

            requirement_hashes.insert(id, digests);
        }

        // Merge the hashes, preferring requirements over constraints, since overlapping
        // requirements were already merged.
        let hashes: FxHashMap<VersionId, Vec<HashDigest>> = constraint_hashes
            .into_iter()
            .chain(requirement_hashes)
            .collect();
        match mode {
            HashCheckingMode::Verify => Ok(Self::Verify(Arc::new(hashes))),
            HashCheckingMode::Require => Ok(Self::Require(Arc::new(hashes))),
        }
    }

    /// Generate the required hashes from a [`Resolution`].
    pub fn from_resolution(
        resolution: &Resolution,
        mode: HashCheckingMode,
    ) -> Result<Self, HashStrategyError> {
        let mut hashes = FxHashMap::<VersionId, Vec<HashDigest>>::default();

        for dist in resolution.distributions() {
            let digests = resolution.get_hashes(dist.name());
            if digests.is_empty() {
                // Under `--require-hashes`, every requirement must include a hash.
                if mode.is_require() {
                    return Err(HashStrategyError::MissingHashes(
                        dist.name().to_string(),
                        mode,
                    ));
                }
                continue;
            }
            hashes.insert(dist.version_id(), digests.to_vec());
        }

        match mode {
            HashCheckingMode::Verify => Ok(Self::Verify(Arc::new(hashes))),
            HashCheckingMode::Require => Ok(Self::Require(Arc::new(hashes))),
        }
    }

    /// Pin a [`Requirement`] to a [`PackageId`], if possible.
    fn pin(requirement: &Requirement) -> Option<VersionId> {
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

                Some(VersionId::from_registry(
                    requirement.name.clone(),
                    specifier.version().clone(),
                ))
            }
            RequirementSource::Url { url, .. }
            | RequirementSource::Git { url, .. }
            | RequirementSource::Path { url, .. }
            | RequirementSource::Directory { url, .. } => Some(VersionId::from_url(url)),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum HashStrategyError {
    #[error(transparent)]
    Hash(#[from] HashError),
    #[error(
        "In `{1}` mode, all requirements must have their versions pinned with `==`, but found: {0}"
    )]
    UnpinnedRequirement(String, HashCheckingMode),
    #[error("In `{1}` mode, all requirements must have a hash, but none were provided for: {0}")]
    MissingHashes(String, HashCheckingMode),
    #[error("In `{1}` mode, all requirements must have a hash, but there were no overlapping hashes between the requirements and constraints for: {0}")]
    NoIntersection(String, HashCheckingMode),
}
