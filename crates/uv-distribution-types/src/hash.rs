use uv_pypi_types::{HashAlgorithm, HashDigest};

/// The normalized hash policy for a single distribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistHashPolicy<'a> {
    /// No hash policy is specified.
    None,
    /// A hash should be included, but not validated.
    Include,
    /// Hashes should be validated against a pre-defined list of hashes, and any matching digest is
    /// sufficient. If necessary, hashes should be generated so as to ensure that the archive is
    /// valid.
    Any(&'a [HashDigest]),
    /// Hashes should be validated against a pre-defined list of hashes, and every digest must
    /// match. If necessary, hashes should be generated so as to ensure that the archive is valid.
    All(&'a [HashDigest]),
}

impl DistHashPolicy<'_> {
    /// Returns `true` if the hash policy is `None`.
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Returns `true` if the hash policy is `Any` or `All`.
    pub fn requires_validation(&self) -> bool {
        matches!(self, Self::Any(_) | Self::All(_))
    }

    /// Returns `true` if a hash needs to be supplied for the distribution.
    pub fn needs_inclusion(&self, dist: &crate::BuiltDist) -> bool {
        matches!(self, Self::Include) && dist.file().is_none_or(|file| file.hashes.is_empty())
    }

    /// Return the algorithms used in the hash policy.
    pub fn algorithms(&self) -> Vec<HashAlgorithm> {
        match self {
            Self::None => vec![],
            Self::Include => vec![HashAlgorithm::Sha256],
            Self::Any(hashes) | Self::All(hashes) => {
                let mut algorithms = hashes.iter().map(HashDigest::algorithm).collect::<Vec<_>>();
                algorithms.sort();
                algorithms.dedup();
                algorithms
            }
        }
    }

    /// Return the digests used in the hash policy.
    pub fn digests(&self) -> &[HashDigest] {
        match self {
            Self::None => &[],
            Self::Include => &[],
            Self::Any(hashes) | Self::All(hashes) => hashes,
        }
    }

    /// Returns `true` if the given hashes satisfy the policy.
    pub fn matches(&self, hashes: &[HashDigest]) -> bool {
        match self {
            Self::None => true,
            Self::Include => hashes
                .iter()
                .any(|hash| hash.algorithm == HashAlgorithm::Sha256),
            Self::Any(required) => {
                !required.is_empty() && hashes.iter().any(|hash| required.contains(hash))
            }
            Self::All(required) => {
                !required.is_empty() && required.iter().all(|hash| hashes.contains(hash))
            }
        }
    }

    /// Returns `true` if the given hashes include the algorithms required by the policy.
    fn has_required_algorithms(&self, hashes: &[HashDigest]) -> bool {
        match self {
            Self::None => true,
            Self::Include => hashes
                .iter()
                .any(|hash| hash.algorithm == HashAlgorithm::Sha256),
            Self::Any(required) => {
                !required.is_empty()
                    && required
                        .iter()
                        .map(HashDigest::algorithm)
                        .any(|algorithm| hashes.iter().any(|hash| hash.algorithm == algorithm))
            }
            Self::All(required) => {
                !required.is_empty()
                    && required
                        .iter()
                        .map(HashDigest::algorithm)
                        .all(|algorithm| hashes.iter().any(|hash| hash.algorithm == algorithm))
            }
        }
    }
}

/// How to include distribution hashes in a resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HashInclusion {
    missing_registry: MissingRegistryHash,
}

impl HashInclusion {
    /// Create a hash inclusion policy with the given behavior for missing registry hashes.
    pub const fn new(missing_registry: MissingRegistryHash) -> Self {
        Self { missing_registry }
    }

    /// Return the behavior for missing registry hashes.
    pub const fn missing_registry(self) -> MissingRegistryHash {
        self.missing_registry
    }
}

/// How to handle a registry distribution when its index metadata does not provide a hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingRegistryHash {
    /// Do not download the distribution solely to compute a hash.
    Skip,
    /// Download the distribution and compute a hash.
    Compute,
}

pub trait Hashed {
    /// Return the [`HashDigest`]s for the archive.
    fn hashes(&self) -> &[HashDigest];

    /// Returns `true` if the archive satisfies the given hash policy.
    fn satisfies(&self, hashes: DistHashPolicy) -> bool {
        hashes.matches(self.hashes())
    }

    /// Returns `true` if the archive includes the algorithms required by the given hash policy.
    fn has_digests(&self, hashes: DistHashPolicy) -> bool {
        hashes.has_required_algorithms(self.hashes())
    }
}

impl Hashed for Vec<HashDigest> {
    fn hashes(&self) -> &[HashDigest] {
        self
    }
}

impl Hashed for &[HashDigest] {
    fn hashes(&self) -> &[HashDigest] {
        self
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use uv_pypi_types::HashDigest;

    use super::DistHashPolicy;

    #[test]
    fn validate_all_requires_every_digest() {
        let sha256 = HashDigest::from_str(
            "sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f",
        )
        .unwrap();
        let sha512 = HashDigest::from_str(
            "sha512:f30761c1e8725b49c498273b90dba4b05c0fd157811994c806183062cb6647e773364ce45f0e1ff0b10e32fe6d0232ea5ad39476ccf37109d6b49603a09c11c2",
        )
        .unwrap();
        let wrong_sha512 = HashDigest::from_str(
            "sha512:e30761c1e8725b49c498273b90dba4b05c0fd157811994c806183062cb6647e773364ce45f0e1ff0b10e32fe6d0232ea5ad39476ccf37109d6b49603a09c11c2",
        )
        .unwrap();

        let policy = DistHashPolicy::All(&[sha256.clone(), sha512.clone()]);
        assert!(policy.matches(&[sha256.clone(), sha512]));
        assert!(!policy.matches(std::slice::from_ref(&sha256)));
        assert!(!policy.matches(&[sha256, wrong_sha512]));
    }

    #[test]
    fn validate_any_requires_one_digest() {
        let sha256 = HashDigest::from_str(
            "sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f",
        )
        .unwrap();
        let sha512 = HashDigest::from_str(
            "sha512:f30761c1e8725b49c498273b90dba4b05c0fd157811994c806183062cb6647e773364ce45f0e1ff0b10e32fe6d0232ea5ad39476ccf37109d6b49603a09c11c2",
        )
        .unwrap();
        let wrong_sha512 = HashDigest::from_str(
            "sha512:e30761c1e8725b49c498273b90dba4b05c0fd157811994c806183062cb6647e773364ce45f0e1ff0b10e32fe6d0232ea5ad39476ccf37109d6b49603a09c11c2",
        )
        .unwrap();

        let policy = DistHashPolicy::Any(&[sha256.clone(), sha512]);
        assert!(policy.matches(&[sha256]));
        assert!(!policy.matches(&[wrong_sha512]));
    }
}
