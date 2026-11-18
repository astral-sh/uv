use std::sync::Arc;

use uv_pypi_types::{HashAlgorithm, HashDigest, HashDigests, Hashes};
use uv_redacted::SafeUrl;

/// Hash checks that an archive must satisfy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedHashes {
    checks: Arc<[HashCheck]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HashCheck {
    Any(Vec<HashDigest>),
    All(Vec<HashDigest>),
}

impl HashCheck {
    fn digests(&self) -> &[HashDigest] {
        match self {
            Self::Any(digests) | Self::All(digests) => digests,
        }
    }

    fn satisfied_by(&self, predicate: impl Fn(&HashDigest) -> bool) -> bool {
        match self {
            Self::Any(digests) => digests.iter().any(predicate),
            Self::All(digests) => !digests.is_empty() && digests.iter().all(predicate),
        }
    }
}

impl ExpectedHashes {
    pub fn none() -> Self {
        Self {
            checks: Arc::from([]),
        }
    }

    /// Match any digest. An empty list rejects the archive.
    pub fn one_of(digests: &[HashDigest]) -> Self {
        Self {
            checks: Arc::from([HashCheck::Any(digests.to_vec())]),
        }
    }

    /// Match every digest. An empty list rejects the archive.
    pub fn all(digests: &[HashDigest]) -> Self {
        Self {
            checks: Arc::from([HashCheck::All(digests.to_vec())]),
        }
    }

    /// Require both sets of expectations to match the same archive.
    #[must_use]
    pub fn and(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let mut checks = self.checks.to_vec();
        for check in other.checks.iter() {
            if !checks.contains(check) {
                checks.push(check.clone());
            }
        }
        Self {
            checks: checks.into(),
        }
    }

    /// Returns `true` if the archive has no hash checks.
    pub fn is_empty(&self) -> bool {
        self.checks.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &HashDigest> {
        self.checks.iter().flat_map(HashCheck::digests)
    }

    /// Return the expected digests used for validation.
    pub fn digests(&self) -> Vec<HashDigest> {
        self.iter().cloned().collect()
    }

    /// Returns `true` if the given hashes pass every check.
    pub fn matches(&self, actual: &[HashDigest]) -> bool {
        self.first_mismatch(actual).is_none()
    }

    /// Return all expected digests from the first check that fails.
    pub fn first_mismatch(&self, actual: &[HashDigest]) -> Option<&[HashDigest]> {
        self.checks
            .iter()
            .find(|check| !check.satisfied_by(|digest| actual.contains(digest)))
            .map(HashCheck::digests)
    }

    /// Returns `true` if the given hashes cover the required algorithms, regardless of digest values.
    pub fn has_algorithms(&self, actual: &[HashDigest]) -> bool {
        self.checks.iter().all(|check| {
            check.satisfied_by(|digest| {
                actual
                    .iter()
                    .any(|hash| hash.algorithm() == digest.algorithm())
            })
        })
    }

    /// Return the algorithms needed for verification and any additional digests, sorted and deduplicated.
    pub fn algorithms_with(
        &self,
        extra: impl IntoIterator<Item = HashAlgorithm>,
    ) -> Vec<HashAlgorithm> {
        let mut algorithms: Vec<_> = self
            .iter()
            .map(HashDigest::algorithm)
            .chain(extra)
            .collect();
        algorithms.sort_unstable();
        algorithms.dedup();
        algorithms
    }
}

/// Which artifacts need hashes in the resolution output.
///
/// Reuse declared hashes when available; otherwise, compute a SHA-256 hash from the archive.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum HashOutput {
    /// Do not collect hashes during resolution.
    #[default]
    None,
    /// Supply hashes for non-registry distributions. Registry hashes come from index metadata.
    Direct,
    /// Also compute missing registry hashes when the index metadata does not provide them.
    All,
}

/// Returns whether the digest's algorithm is accepted by `--require-hashes`.
pub fn satisfies_require_hashes(digest: &HashDigest) -> bool {
    match digest.algorithm() {
        HashAlgorithm::Md5 => false,
        HashAlgorithm::Sha256
        | HashAlgorithm::Sha384
        | HashAlgorithm::Sha512
        | HashAlgorithm::Blake2b256 => true,
    }
}

/// Read a URL's declared hash for resolution, excluding MD5, which `--require-hashes` rejects.
pub fn parse_url_hashes(url: &SafeUrl) -> Option<HashDigests> {
    let hashes = url
        .fragment()?
        .split('&')
        .find_map(|fragment| Hashes::parse_fragment(fragment).ok())?;
    let hashes = HashDigests::from(hashes);
    let allowed = hashes.iter().all(satisfies_require_hashes);
    allowed.then_some(hashes)
}

pub trait Hashed {
    /// Return the [`HashDigest`]s for the archive.
    fn hashes(&self) -> &[HashDigest];

    /// Whether the cached digests include the requested output algorithm.
    fn has_output_hash(&self, algorithm: Option<HashAlgorithm>) -> bool {
        algorithm.is_none_or(|algorithm| {
            self.hashes()
                .iter()
                .any(|hash| hash.algorithm() == algorithm)
        })
    }

    /// Whether the archive passes every expected hash check.
    fn satisfies(&self, expected: &ExpectedHashes) -> bool {
        expected.matches(self.hashes())
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

    use super::ExpectedHashes;

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

        let expected = [sha256.clone(), sha512.clone()];
        let policy = ExpectedHashes::all(&expected);
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

        let expected = [sha256.clone(), sha512];
        let policy = ExpectedHashes::one_of(&expected);
        assert!(policy.matches(&[sha256]));
        assert!(!policy.matches(&[wrong_sha512]));
    }
}
