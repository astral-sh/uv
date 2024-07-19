use rustc_hash::FxHashMap;
use std::str::FromStr;
use std::sync::Arc;
use url::Url;

use distribution_types::{
    DistributionMetadata, HashPolicy, Name, Resolution, UnresolvedRequirement, VersionId,
};
use pep440_rs::Version;
use pep508_rs::MarkerEnvironment;
use pypi_types::{HashDigest, HashError, Requirement, RequirementSource};
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
        markers: Option<&MarkerEnvironment>,
        mode: HashCheckingMode,
    ) -> Result<Self, HashStrategyError> {
        let mut hashes = FxHashMap::<VersionId, Vec<HashDigest>>::default();

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
                        HashStrategyError::UnpinnedRequirement(requirement.to_string(), mode)
                    })?
                }
                UnresolvedRequirement::Unnamed(requirement) => {
                    // Direct URLs are always allowed.
                    VersionId::from_url(&requirement.url.verbatim)
                }
            };

            if digests.is_empty() {
                // Under `--require-hashes`, every requirement must include a hash.
                if mode.is_require() {
                    return Err(HashStrategyError::MissingHashes(
                        requirement.to_string(),
                        mode,
                    ));
                }
                continue;
            }

            // Parse the hashes.
            let digests = digests
                .iter()
                .map(|digest| HashDigest::from_str(digest))
                .collect::<Result<Vec<_>, _>>()?;

            hashes.insert(id, digests);
        }

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
        "In `{1}` mode, all requirement must have their versions pinned with `==`, but found: {0}"
    )]
    UnpinnedRequirement(String, HashCheckingMode),
    #[error("In `{1}` mode, all requirement must have a hash, but none were provided for: {0}")]
    MissingHashes(String, HashCheckingMode),
}
