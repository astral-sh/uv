use std::fmt::Display;

use uv_distribution_types::{ArchiveHashPolicy, Hashed, RegistryFile};
use uv_pypi_types::{HashAlgorithm, HashDigest};

use crate::Error;

/// Hash requirements for downloading and caching a wheel or source distribution.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ArtifactHashPolicy<'a> {
    /// Hashes the caller requires on the returned artifact.
    pub(crate) required: ArchiveHashPolicy<'a>,
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

    /// Return hash checks for a cached registry artifact.
    ///
    /// Caller hash requirements are already enforced when building the registry wheel index.
    /// Return `None` if a proxied artifact has no matching file in the current distribution.
    /// Direct indexes can reuse cached artifacts without one.
    pub(crate) fn for_cached_registry(
        is_proxy: bool,
        file: Option<&'a RegistryFile>,
    ) -> Option<Self> {
        let cache_verification = if is_proxy {
            let hashes = &file?.hashes;
            if hashes.is_empty() {
                ArchiveHashPolicy::None
            } else {
                ArchiveHashPolicy::Any(hashes.as_slice())
            }
        } else {
            ArchiveHashPolicy::None
        };

        Some(Self::new(ArchiveHashPolicy::None, cache_verification))
    }

    /// Apply index hashes when the route has no separate cache verification policy.
    pub(crate) fn with_index_hashes(self, hashes: &'a [HashDigest]) -> Self {
        let cache_verification = if self.cache_verification.requires_validation() {
            self.cache_verification
        } else {
            self.required.with_index_hashes(hashes)
        };
        Self::new(self.required, cache_verification)
    }

    pub(crate) fn algorithms(self) -> Vec<HashAlgorithm> {
        let mut algorithms = self.required.algorithms();
        algorithms.extend(self.cache_verification.algorithms());
        algorithms.sort_unstable();
        algorithms.dedup();
        algorithms
    }

    pub(crate) fn http_algorithms(self) -> Vec<HashAlgorithm> {
        let mut algorithms = self.algorithms();
        algorithms.push(HashAlgorithm::Sha256);
        algorithms.sort_unstable();
        algorithms.dedup();
        algorithms
    }

    pub(crate) fn admits_cached_artifact(self, artifact: &impl Hashed) -> bool {
        artifact.satisfies(self.cache_verification) && artifact.has_digests(self.required)
    }

    pub(crate) fn validate_download(
        self,
        artifact: &impl Display,
        hashes: &[HashDigest],
    ) -> Result<(), Error> {
        if !self.cache_verification.matches(hashes) {
            return Err(Error::hash_mismatch(
                artifact.to_string(),
                self.cache_verification.digests(),
                hashes,
            ));
        }

        Ok(())
    }

    pub(crate) fn validate_artifact(
        self,
        artifact: &impl Display,
        hashes: &impl Hashed,
    ) -> Result<(), Error> {
        self.validate_download(artifact, hashes.hashes())?;
        if !hashes.satisfies(self.required) {
            return Err(Error::hash_mismatch(
                artifact.to_string(),
                self.required.digests(),
                hashes.hashes(),
            ));
        }

        Ok(())
    }
}

impl<'a> From<ArchiveHashPolicy<'a>> for ArtifactHashPolicy<'a> {
    fn from(required: ArchiveHashPolicy<'a>) -> Self {
        Self::new(required, ArchiveHashPolicy::None)
    }
}

#[cfg(test)]
mod tests {
    use uv_pypi_types::HashDigests;

    use super::*;

    #[test]
    fn artifact_hash_policy_preserves_cache_verification_algorithms()
    -> Result<(), Box<dyn std::error::Error>> {
        let cache_verification = HashDigests::from(vec![
            "sha512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .parse()?,
        ]);
        let hashes = ArtifactHashPolicy::new(
            ArchiveHashPolicy::None,
            ArchiveHashPolicy::Any(cache_verification.as_slice()),
        );

        assert_eq!(
            hashes.http_algorithms(),
            vec![HashAlgorithm::Sha256, HashAlgorithm::Sha512]
        );
        Ok(())
    }

    #[test]
    fn artifact_hash_policy_rejects_wrong_cached_digest() -> Result<(), Box<dyn std::error::Error>>
    {
        let cache_verification = HashDigests::from(vec![
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".parse()?,
        ]);
        let cached_hashes = vec![
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".parse()?,
        ];
        let hashes = ArtifactHashPolicy::new(
            ArchiveHashPolicy::None,
            ArchiveHashPolicy::Any(cache_verification.as_slice()),
        );

        assert!(!hashes.admits_cached_artifact(&cached_hashes));
        Ok(())
    }
}
