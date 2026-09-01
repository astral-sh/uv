use std::fmt::Display;

use uv_distribution_types::{HashPolicy, Hashed};
use uv_pypi_types::{HashAlgorithm, HashDigest};

use crate::Error;

/// Hash requirements for downloading and caching a wheel or source distribution.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ArtifactHashPolicy<'a> {
    /// Hashes the caller requires on the returned artifact.
    pub(crate) required: HashPolicy<'a>,
    /// Hashes downloaded or cached bytes must match before entering the cache.
    cache_verification: HashPolicy<'a>,
}

impl<'a> ArtifactHashPolicy<'a> {
    pub(crate) const fn new(required: HashPolicy<'a>, cache_verification: HashPolicy<'a>) -> Self {
        Self {
            required,
            cache_verification,
        }
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

impl<'a> From<HashPolicy<'a>> for ArtifactHashPolicy<'a> {
    fn from(required: HashPolicy<'a>) -> Self {
        Self::new(required, HashPolicy::None)
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
            HashPolicy::None,
            HashPolicy::Any(cache_verification.as_slice()),
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
            HashPolicy::None,
            HashPolicy::Any(cache_verification.as_slice()),
        );

        assert!(!hashes.admits_cached_artifact(&cached_hashes));
        Ok(())
    }
}
