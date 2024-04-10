use rustc_hash::FxHashMap;
use std::str::FromStr;

use pep508_rs::{MarkerEnvironment, Requirement, VersionOrUrl};
use pypi_types::{HashDigest, HashError};
use uv_normalize::PackageName;

/// A set of package versions that are permitted, even if they're marked as yanked by the
/// relevant index.
#[derive(Debug, Default, Clone)]
pub struct RequiredHashes(FxHashMap<PackageName, Vec<HashDigest>>);

impl RequiredHashes {
    /// Generate the [`RequiredHashes`] from a set of requirement entries.
    pub fn from_requirements(
        requirements: impl Iterator<Item = (Requirement, Vec<String>)>,
        markers: &MarkerEnvironment,
    ) -> Result<Self, RequiredHashesError> {
        let mut allowed_hashes = FxHashMap::<PackageName, Vec<HashDigest>>::default();

        // For each requirement, map from name to allowed hashes. We use the last entry for each
        // package.
        //
        // For now, unnamed requirements are unsupported. This should be fine, since `--require-hashes`
        // tends to be used after `pip-compile`, which will always output named requirements.
        //
        // TODO(charlie): Preserve hashes from `requirements.txt` through to this pass, so that we
        // can iterate over requirements directly, rather than iterating over the entries.
        for (requirement, hashes) in requirements {
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
                        return Err(RequiredHashesError::UnpinnedRequirement(
                            requirement.to_string(),
                        ));
                    }
                }
                None => {
                    return Err(RequiredHashesError::UnpinnedRequirement(
                        requirement.to_string(),
                    ))
                }
            }

            // Every requirement must include a hash.
            if hashes.is_empty() {
                return Err(RequiredHashesError::MissingHashes(requirement.to_string()));
            }

            // Parse the hashes.
            let hashes = hashes
                .iter()
                .map(|hash| HashDigest::from_str(hash))
                .collect::<Result<Vec<_>, _>>()
                .unwrap();

            // TODO(charlie): Extract hashes from URL fragments.
            allowed_hashes.insert(requirement.name, hashes);
        }

        Ok(Self(allowed_hashes))
    }

    /// Returns versions for the given package which are allowed even if marked as yanked by the
    /// relevant index.
    pub fn get(&self, package_name: &PackageName) -> Option<&[HashDigest]> {
        self.0.get(package_name).map(Vec::as_slice)
    }

    /// Returns whether the given package is allowed even if marked as yanked by the relevant index.
    pub fn contains(&self, package_name: &PackageName) -> bool {
        self.0.contains_key(package_name)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum RequiredHashesError {
    #[error(transparent)]
    Hash(#[from] HashError),
    #[error("Unnamed requirements are not supported in `--require-hashes`")]
    UnnamedRequirement,
    #[error("In `--require-hashes` mode, all requirement must have their versions pinned with `==`, but found: {0}")]
    UnpinnedRequirement(String),
    #[error("In `--require-hashes` mode, all requirement must have a hash, but none were provided for: {0}")]
    MissingHashes(String),
}
