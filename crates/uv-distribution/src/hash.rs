use std::fmt::Display;

use uv_distribution_types::{ExpectedHashes, Hashed};
use uv_pypi_types::{HashAlgorithm, HashDigest};

use crate::Error;

/// Hash algorithms to compute for every HTTP archive.
pub(crate) const HTTP_DEFAULT_HASH_ALGORITHMS: [HashAlgorithm; 1] = [HashAlgorithm::Sha256];

/// Hash requirements for downloading and caching a wheel or source distribution.
#[derive(Debug, Clone)]
pub(crate) struct ArtifactHashes {
    /// Hash checks requested by the caller.
    required: ExpectedHashes,
    /// Additional hash checks for admitting or reusing an archive in the cache.
    cache: ExpectedHashes,
}

impl ArtifactHashes {
    fn new(required: ExpectedHashes, cache: ExpectedHashes) -> Self {
        Self { required, cache }
    }

    /// Use the caller's checks with the standard index hash fallback.
    pub(crate) fn required(required: ExpectedHashes) -> Self {
        Self::new(required, ExpectedHashes::none())
    }

    /// Proxy downloads must match the selected index file as well as the caller's hashes.
    pub(crate) fn for_registry(
        required: ExpectedHashes,
        is_proxy: bool,
        index_hashes: &[HashDigest],
    ) -> Self {
        let cache = if is_proxy {
            if index_hashes.is_empty() {
                required.clone()
            } else {
                ExpectedHashes::all(index_hashes)
            }
        } else {
            ExpectedHashes::none()
        };
        Self::new(required, cache)
    }

    /// A cache hit must match the applicable index and cache hashes and record the caller's algorithms.
    pub(crate) fn can_reuse(&self, artifact: &impl Hashed, index_hashes: &[HashDigest]) -> bool {
        self.index_fallback(index_hashes).matches(artifact.hashes())
            && self.cache.matches(artifact.hashes())
            && self.required.has_algorithms(artifact.hashes())
    }

    /// Check the caller's required hashes before using a cached artifact.
    pub(crate) fn verify_required(
        &self,
        artifact: impl Display,
        hashes: &[HashDigest],
    ) -> Result<(), Error> {
        verify_hashes(&self.required, &artifact, hashes)
    }

    pub(crate) fn for_download(&self, index_hashes: &[HashDigest]) -> DownloadHashVerifier {
        DownloadHashVerifier {
            checks: [
                self.index_fallback(index_hashes),
                self.cache.clone(),
                self.required.clone(),
            ],
        }
    }

    /// Index hashes are a fallback unless the caller or proxy supplies its own check.
    fn index_fallback(&self, index_hashes: &[HashDigest]) -> ExpectedHashes {
        if !self.cache.is_empty() || !self.required.is_empty() {
            ExpectedHashes::none()
        } else {
            ExpectedHashes::none().with_index_hashes(index_hashes)
        }
    }
}

/// Independently check the applicable index, cache, and caller hashes.
#[derive(Debug, Clone)]
pub(crate) struct DownloadHashVerifier {
    checks: [ExpectedHashes; 3],
}

impl DownloadHashVerifier {
    pub(crate) fn algorithms(
        &self,
        extra: impl IntoIterator<Item = HashAlgorithm>,
    ) -> Vec<HashAlgorithm> {
        let mut algorithms: Vec<_> = self
            .checks
            .iter()
            .flat_map(ExpectedHashes::iter)
            .map(HashDigest::algorithm)
            .chain(extra)
            .collect();
        algorithms.sort_unstable();
        algorithms.dedup();
        algorithms
    }

    pub(crate) fn verify(
        &self,
        artifact: impl Display,
        hashes: &[HashDigest],
    ) -> Result<(), Error> {
        for request in &self.checks {
            verify_hashes(request, &artifact, hashes)?;
        }
        Ok(())
    }
}

pub(crate) fn verify_hashes(
    expected: &ExpectedHashes,
    artifact: impl Display,
    hashes: &[HashDigest],
) -> Result<(), Error> {
    if let Some(expected) = expected.first_mismatch(hashes) {
        return Err(Error::hash_mismatch(artifact.to_string(), expected, hashes));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use uv_pypi_types::Digest;

    use super::*;

    #[test]
    fn cache_reuse_checks_index_hashes_unless_explicit_hashes_take_precedence() {
        let advertised = [HashDigest::Sha256(Digest::from_bytes([0xaa; 32]))];
        let cached_hashes = vec![HashDigest::Sha256(Digest::from_bytes([0xbb; 32]))];
        let direct = ArtifactHashes::required(ExpectedHashes::none());
        assert!(direct.can_reuse(&cached_hashes, &cached_hashes));
        assert!(!direct.can_reuse(&cached_hashes, &advertised));
        assert!(!direct.can_reuse(&Vec::new(), &advertised));

        let caller = ArtifactHashes::required(ExpectedHashes::one_of(&advertised));
        assert!(caller.can_reuse(&cached_hashes, &advertised));
        assert!(caller.verify_required("archive", &cached_hashes).is_err());

        let cache = ArtifactHashes::new(
            ExpectedHashes::none(),
            ExpectedHashes::one_of(&cached_hashes),
        );
        assert!(cache.can_reuse(&cached_hashes, &advertised));
        let cache =
            ArtifactHashes::new(ExpectedHashes::none(), ExpectedHashes::one_of(&advertised));
        assert!(!cache.can_reuse(&cached_hashes, &cached_hashes));
    }

    #[test]
    fn proxy_file_hashes_are_checked_even_when_the_caller_supplies_a_hash() {
        let sha256 = HashDigest::Sha256(Digest::from_bytes([0xaa; 32]));
        let sha512 = HashDigest::Sha512(Digest::from_bytes([0xbb; 64]));
        let file_hashes = [sha256.clone(), sha512];
        let incomplete = [sha256];
        let caller = ExpectedHashes::one_of(&incomplete);
        let proxy = ArtifactHashes::for_registry(caller, true, &file_hashes);
        assert!(!proxy.can_reuse(&incomplete.as_slice(), &file_hashes));
        assert!(
            proxy
                .for_download(&file_hashes)
                .verify("archive", &incomplete)
                .is_err()
        );
        assert!(
            proxy
                .for_download(&file_hashes)
                .verify("archive", &file_hashes)
                .is_ok()
        );
    }

    #[test]
    fn download_verifier_requires_both_caller_and_cache_hashes() {
        let caller = [HashDigest::Sha256(Digest::from_bytes([0xaa; 32]))];
        let cache = [HashDigest::Sha512(Digest::from_bytes([0xbb; 64]))];
        let verifier =
            ArtifactHashes::new(ExpectedHashes::all(&caller), ExpectedHashes::one_of(&cache))
                .for_download(&[]);
        assert_eq!(
            verifier.algorithms(HTTP_DEFAULT_HASH_ALGORITHMS),
            vec![HashAlgorithm::Sha256, HashAlgorithm::Sha512]
        );

        let matching = [caller[0].clone(), cache[0].clone()];
        assert!(verifier.verify("archive", &matching).is_ok());

        let wrong_caller = [
            HashDigest::Sha256(Digest::from_bytes([0xcc; 32])),
            cache[0].clone(),
        ];
        assert!(matches!(
            verifier.verify("archive", &wrong_caller),
            Err(Error::MismatchedHashes { expected, .. }) if expected.trim() == caller[0].to_string()
        ));

        let wrong_cache = [
            caller[0].clone(),
            HashDigest::Sha512(Digest::from_bytes([0xdd; 64])),
        ];
        assert!(matches!(
            verifier.verify("archive", &wrong_cache),
            Err(Error::MismatchedHashes { expected, .. }) if expected.trim() == cache[0].to_string()
        ));
    }
}
