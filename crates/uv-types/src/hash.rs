use distribution_types::HashPolicy;
use rustc_hash::FxHashMap;
use std::str::FromStr;

use pep508_rs::{MarkerEnvironment, Requirement, VersionOrUrl};
use pypi_types::{HashDigest, HashError};
use uv_normalize::PackageName;

#[derive(Debug, Clone)]
pub enum HashStrategy {
    /// No hash policy is specified.
    None,
    /// Hashes should be generated (specifically, a SHA-256 hash), but not validated.
    Generate,
    /// Hashes should be validated against a pre-defined list of hashes. If necessary, hashes should
    /// be generated so as to ensure that the archive is valid.
    Validate(FxHashMap<PackageName, Vec<HashDigest>>),
}

impl HashStrategy {
    /// Return the [`HashPolicy`] for the given package.
    pub fn get(&self, package_name: &PackageName) -> HashPolicy {
        match self {
            Self::None => HashPolicy::None,
            Self::Generate => HashPolicy::Generate,
            Self::Validate(hashes) => hashes
                .get(package_name)
                .map(Vec::as_slice)
                .map_or(HashPolicy::None, HashPolicy::Validate),
        }
    }

    /// Returns `true` if the given package is allowed.
    pub fn allows(&self, package_name: &PackageName) -> bool {
        match self {
            Self::None => true,
            Self::Generate => true,
            Self::Validate(hashes) => hashes.contains_key(package_name),
        }
    }

    /// Generate the required hashes from a set of [`Requirement`] entries.
    pub fn from_requirements(
        requirements: impl Iterator<Item = (Requirement, Vec<String>)>,
        markers: &MarkerEnvironment,
    ) -> Result<Self, HashStrategyError> {
        let mut hashes = FxHashMap::<PackageName, Vec<HashDigest>>::default();

        // For each requirement, map from name to allowed hashes. We use the last entry for each
        // package.
        //
        // For now, unnamed requirements are unsupported. This should be fine, since `--require-hashes`
        // tends to be used after `pip-compile`, which will always output named requirements.
        //
        // TODO(charlie): Preserve hashes from `requirements.txt` through to this pass, so that we
        // can iterate over requirements directly, rather than iterating over the entries.
        for (requirement, digests) in requirements {
            if !requirement.evaluate_markers(markers, &[]) {
                continue;
            }

            // Every requirement must be either a pinned version or a direct URL.
            match requirement.version_or_url.as_ref() {
                Some(VersionOrUrl::Url(_)) => {
                    // Direct URLs are always allowed.
                }
                Some(VersionOrUrl::VersionSpecifier(specifiers)) => {
                    if specifiers
                        .iter()
                        .any(|specifier| matches!(specifier.operator(), pep440_rs::Operator::Equal))
                    {
                        // Pinned versions are allowed.
                    } else {
                        return Err(HashStrategyError::UnpinnedRequirement(
                            requirement.to_string(),
                        ));
                    }
                }
                None => {
                    return Err(HashStrategyError::UnpinnedRequirement(
                        requirement.to_string(),
                    ))
                }
            }

            // Every requirement must include a hash.
            if digests.is_empty() {
                return Err(HashStrategyError::MissingHashes(requirement.to_string()));
            }

            // Parse the hashes.
            let digests = digests
                .iter()
                .map(|digest| HashDigest::from_str(digest))
                .collect::<Result<Vec<_>, _>>()
                .unwrap();

            // TODO(charlie): Extract hashes from URL fragments.
            hashes.insert(requirement.name, digests);
        }

        Ok(Self::Validate(hashes))
    }
}

#[derive(thiserror::Error, Debug)]
pub enum HashStrategyError {
    #[error(transparent)]
    Hash(#[from] HashError),
    #[error("Unnamed requirements are not supported in `--require-hashes`")]
    UnnamedRequirement,
    #[error("In `--require-hashes` mode, all requirement must have their versions pinned with `==`, but found: {0}")]
    UnpinnedRequirement(String),
    #[error("In `--require-hashes` mode, all requirement must have a hash, but none were provided for: {0}")]
    MissingHashes(String),
}
