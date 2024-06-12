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
    /// Hashes should be validated against a pre-defined list of hashes. If necessary, hashes should
    /// be generated so as to ensure that the archive is valid.
    Validate(FxHashMap<PackageId, Vec<HashDigest>>),
}

impl HashStrategy {
    /// Return the [`HashPolicy`] for the given distribution.
    pub fn get<T: DistributionMetadata>(&self, distribution: &T) -> HashPolicy {
        match self {
            Self::None => HashPolicy::None,
            Self::Generate => HashPolicy::Generate,
            Self::Validate(hashes) => HashPolicy::Validate(
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
            Self::Validate(hashes) => HashPolicy::Validate(
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
            Self::Validate(hashes) => HashPolicy::Validate(
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
            Self::Validate(hashes) => hashes.contains_key(&PackageId::from_registry(name.clone())),
        }
    }

    /// Returns `true` if the given direct URL package is allowed.
    pub fn allows_url(&self, url: &Url) -> bool {
        match self {
            Self::None => true,
            Self::Generate => true,
            Self::Validate(hashes) => hashes.contains_key(&PackageId::from_url(url)),
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
                    uv_requirement_to_package_id(requirement)?
                }
                UnresolvedRequirement::Unnamed(requirement) => {
                    // Direct URLs are always allowed.
                    PackageId::from_url(&requirement.url.verbatim)
                }
            };

            // Every requirement must include a hash.
            if digests.is_empty() {
                return Err(HashStrategyError::MissingHashes(requirement.to_string()));
            }

            // Parse the hashes.
            let digests = digests
                .iter()
                .map(|digest| HashDigest::from_str(digest))
                .collect::<Result<Vec<_>, _>>()?;

            hashes.insert(id, digests);
        }

        Ok(Self::Validate(hashes))
    }
}

fn uv_requirement_to_package_id(requirement: &Requirement) -> Result<PackageId, HashStrategyError> {
    Ok(match &requirement.source {
        RequirementSource::Registry { specifier, .. } => {
            // Must be a single specifier.
            let [specifier] = specifier.as_ref() else {
                return Err(HashStrategyError::UnpinnedRequirement(
                    requirement.to_string(),
                ));
            };

            // Must be pinned to a specific version.
            if *specifier.operator() != pep440_rs::Operator::Equal {
                return Err(HashStrategyError::UnpinnedRequirement(
                    requirement.to_string(),
                ));
            }

            PackageId::from_registry(requirement.name.clone())
        }
        RequirementSource::Url { url, .. }
        | RequirementSource::Git { url, .. }
        | RequirementSource::Path { url, .. }
        | RequirementSource::Directory { url, .. } => PackageId::from_url(url),
    })
}

#[derive(thiserror::Error, Debug)]
pub enum HashStrategyError {
    #[error(transparent)]
    Hash(#[from] HashError),
    #[error("In `--require-hashes` mode, all requirement must have their versions pinned with `==`, but found: {0}")]
    UnpinnedRequirement(String),
    #[error("In `--require-hashes` mode, all requirement must have a hash, but none were provided for: {0}")]
    MissingHashes(String),
}
