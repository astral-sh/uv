use std::fmt::Display;

use uv_distribution_types::{ArchiveHashPolicy, Hashed};
use uv_pypi_types::{HashAlgorithm, HashDigest};

use crate::Error;

/// Hash requirements for downloading and caching a wheel or source distribution.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ArtifactHashPolicy<'a> {
    /// Hashes the caller requires on the returned artifact.
    required: ArchiveHashPolicy<'a>,
    /// Hashes downloaded or cached bytes must match before entering the cache.
    cache_verification: ArchiveHashPolicy<'a>,
}

impl<'a> ArtifactHashPolicy<'a> {
    pub(crate) const fn new(
        required: ArchiveHashPolicy<'a>,
        cache_verification: ArchiveHashPolicy<'a>,
    ) -> Self {
        Self {
            required,
            cache_verification,
        }
    }

    /// Proxy downloads must match the selected index file as well as the caller's hashes.
    pub(crate) fn for_registry(
        required: ArchiveHashPolicy<'a>,
        is_proxy: bool,
        index_hashes: &'a [HashDigest],
    ) -> Self {
        let cache_verification = if is_proxy {
            if index_hashes.is_empty() {
                required
            } else {
                ArchiveHashPolicy::None.with_index_hashes(index_hashes)
            }
        } else {
            ArchiveHashPolicy::None
        };
        Self::new(required, cache_verification)
    }

    /// A cache hit must match the applicable index and cache hashes and record the caller's algorithms.
    pub(crate) fn can_reuse(self, artifact: &impl Hashed, index_hashes: &'a [HashDigest]) -> bool {
        artifact.satisfies(self.index_fallback(index_hashes))
            && artifact.satisfies(self.cache_verification)
            && artifact.has_digests(self.required)
    }

    /// Check the caller's required hashes before using a cached artifact.
    pub(crate) fn verify_required(
        self,
        artifact: impl Display,
        hashes: &[HashDigest],
    ) -> Result<(), Error> {
        verify_hashes(self.required, &artifact, hashes)
    }

    pub(crate) fn for_download(self, index_hashes: &'a [HashDigest]) -> DownloadHashVerifier<'a> {
        DownloadHashVerifier {
            checks: [
                self.index_fallback(index_hashes),
                self.cache_verification,
                self.required,
            ],
        }
    }

    /// Use the selected index file's hashes when neither policy specifies digests.
    fn index_fallback(self, index_hashes: &'a [HashDigest]) -> ArchiveHashPolicy<'a> {
        if self.cache_verification.requires_validation() || self.required.requires_validation() {
            ArchiveHashPolicy::None
        } else {
            ArchiveHashPolicy::None.with_index_hashes(index_hashes)
        }
    }
}

impl<'a> From<ArchiveHashPolicy<'a>> for ArtifactHashPolicy<'a> {
    fn from(required: ArchiveHashPolicy<'a>) -> Self {
        Self::new(required, ArchiveHashPolicy::None)
    }
}

/// Hash checks for newly read archive bytes, including any applicable index hashes.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DownloadHashVerifier<'a> {
    checks: [ArchiveHashPolicy<'a>; 3],
}

impl DownloadHashVerifier<'_> {
    pub(crate) fn algorithms(
        self,
        extra: impl IntoIterator<Item = HashAlgorithm>,
    ) -> Vec<HashAlgorithm> {
        let mut algorithms: Vec<_> = self
            .checks
            .iter()
            .flat_map(ArchiveHashPolicy::algorithms)
            .chain(extra)
            .collect();
        algorithms.sort_unstable();
        algorithms.dedup();
        algorithms
    }

    pub(crate) fn verify(self, artifact: impl Display, hashes: &[HashDigest]) -> Result<(), Error> {
        for policy in self.checks {
            verify_hashes(policy, &artifact, hashes)?;
        }
        Ok(())
    }
}

fn verify_hashes(
    policy: ArchiveHashPolicy<'_>,
    artifact: &impl Display,
    hashes: &[HashDigest],
) -> Result<(), Error> {
    if !policy.matches(hashes) {
        return Err(Error::hash_mismatch(
            artifact.to_string(),
            policy.digests(),
            hashes,
        ));
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
        let direct = ArtifactHashPolicy::from(ArchiveHashPolicy::None);
        assert!(direct.can_reuse(&cached_hashes, &cached_hashes));
        assert!(!direct.can_reuse(&cached_hashes, &advertised));
        assert!(!direct.can_reuse(&Vec::new(), &advertised));

        let caller = ArtifactHashPolicy::from(ArchiveHashPolicy::Any(&advertised));
        assert!(caller.can_reuse(&cached_hashes, &advertised));
        assert!(caller.verify_required("archive", &cached_hashes).is_err());

        let cache = ArtifactHashPolicy::new(
            ArchiveHashPolicy::None,
            ArchiveHashPolicy::Any(&cached_hashes),
        );
        assert!(cache.can_reuse(&cached_hashes, &advertised));
        let cache =
            ArtifactHashPolicy::new(ArchiveHashPolicy::None, ArchiveHashPolicy::Any(&advertised));
        assert!(!cache.can_reuse(&cached_hashes, &cached_hashes));
    }

    #[test]
    fn proxy_file_hashes_are_checked_even_when_the_caller_supplies_a_hash() {
        let sha256 = HashDigest::Sha256(Digest::from_bytes([0xaa; 32]));
        let sha512 = HashDigest::Sha512(Digest::from_bytes([0xbb; 64]));
        let file_hashes = [sha256.clone(), sha512];
        let caller = ArchiveHashPolicy::Any(std::slice::from_ref(&sha256));
        let proxy = ArtifactHashPolicy::for_registry(caller, true, &file_hashes);
        let incomplete = vec![sha256.clone()];
        assert!(!proxy.can_reuse(&incomplete, &file_hashes));
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
        let verifier = ArtifactHashPolicy::new(
            ArchiveHashPolicy::All(&caller),
            ArchiveHashPolicy::Any(&cache),
        )
        .for_download(&[]);
        assert_eq!(
            verifier.algorithms([HashAlgorithm::Sha256]),
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
