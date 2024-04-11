use distribution_types::{DistributionMetadata, HashPolicy, PackageId};
use rustc_hash::{FxHashMap, FxHashSet};
use std::str::FromStr;
use url::Url;

use pep508_rs::{
    MarkerEnvironment, Requirement, RequirementsTxtRequirement, VerbatimUrl, VersionOrUrl,
};
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
    Validate {
        hashes: FxHashMap<PackageId, Vec<HashDigest>>,
        packages: FxHashMap<PackageName, Vec<HashDigest>>,
        urls: FxHashSet<Url>,
    },
}

impl HashStrategy {
    /// Return the [`HashPolicy`] for the given distribution.
    pub fn get<T: DistributionMetadata>(&self, distribution: &T) -> HashPolicy {
        match self {
            Self::None => HashPolicy::None,
            Self::Generate => HashPolicy::Generate,
            Self::Validate { hashes, .. } => hashes
                .get(&distribution.package_id())
                .map(Vec::as_slice)
                .map_or(HashPolicy::None, HashPolicy::Validate),
        }
    }

    /// Return the [`HashPolicy`] for the given package ID.
    pub fn by_id(&self, id: &PackageId) -> HashPolicy {
        match self {
            Self::None => HashPolicy::None,
            Self::Generate => HashPolicy::Generate,
            Self::Validate { hashes, .. } => hashes
                .get(id)
                .map(Vec::as_slice)
                .map_or(HashPolicy::None, HashPolicy::Validate),
        }
    }

    /// Return the [`HashPolicy`] for the given package ID.
    pub fn by_package(&self, name: &PackageName) -> HashPolicy {
        match self {
            Self::None => HashPolicy::None,
            Self::Generate => HashPolicy::Generate,
            Self::Validate { packages, .. } => packages
                .get(name)
                .map(Vec::as_slice)
                .map_or(HashPolicy::None, HashPolicy::Validate),
        }
    }

    /// Returns `true` if the given package is allowed. Used to prevent resolvers from inserting
    /// packages that were not specified upfront.
    ///
    /// A package is allowed if it was specified with a pinned version and hash.
    pub fn allows_package(&self, package: &PackageName) -> bool {
        match self {
            Self::None => true,
            Self::Generate => true,
            Self::Validate { packages, .. } => packages.contains_key(package),
        }
    }

    /// Returns `true` if the given URL is allowed. Used to prevent resolvers from inserting URLs
    /// that were not specified upfront.
    ///
    /// A URL is allowed if it was provided with a hash.
    pub fn allows_url(&self, url: &Url) -> bool {
        match self {
            Self::None => true,
            Self::Generate => true,
            Self::Validate { urls, .. } => urls.contains(url),
        }
    }

    /// Generate the required hashes from a set of [`RequirementsTxtRequirement`] entries.
    pub fn from_requirements(
        requirements: impl Iterator<Item = (RequirementsTxtRequirement, Vec<String>)>,
        markers: &MarkerEnvironment,
    ) -> Result<Self, HashStrategyError> {
        let mut hashes = FxHashMap::<PackageId, Vec<HashDigest>>::default();
        let mut packages = FxHashMap::<PackageName, Vec<HashDigest>>::default();
        let mut urls = FxHashSet::<Url>::default();

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

            // Every requirement must be either a pinned version or a direct URL.
            match requirement {
                RequirementsTxtRequirement::Pep508(requirement) => {
                    match requirement.version_or_url.as_ref() {
                        Some(VersionOrUrl::Url(url)) => {
                            // Direct URLs are always allowed.
                            urls.insert(url.to_url());
                            hashes.insert(PackageId::from_url(url), digests);
                        }
                        Some(VersionOrUrl::VersionSpecifier(specifiers)) => {
                            // Must be a single specifier.
                            let [specifier] = specifiers.as_ref() else {
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

                            packages.insert(requirement.name.clone(), digests);
                            PackageId::from_registry(
                                requirement.name.clone(),
                                specifier.version().clone(),
                            )
                        }
                        None => {
                            return Err(HashStrategyError::UnpinnedRequirement(
                                requirement.to_string(),
                            ))
                        }
                    }
                }
                RequirementsTxtRequirement::Unnamed(requirement) => {
                    // Direct URLs are always allowed.
                    urls.insert(requirement.url.to_url());
                    hashes.insert(PackageId::from_url(&requirement.url), digests);
                }
            };
        }

        Ok(Self::Validate {
            hashes,
            packages,
            urls,
        })
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
