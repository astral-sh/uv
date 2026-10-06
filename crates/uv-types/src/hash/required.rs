use std::fmt::Display;
use std::sync::Arc;

use rustc_hash::FxHashMap;
use uv_configuration::HashCheckingMode;
use uv_distribution_types::VersionId;
use uv_pypi_types::HashDigest;

use super::{HashStrategyError, HashVerification, TrustedHashDigests, merge_digests};

/// Digests that can authorize a distribution when hashes are required.
///
/// MD5 hashes cannot be included. The digests are only exposed as a shared slice, so callers
/// cannot add an insecure hash after construction.
#[derive(Debug, Default, Clone)]
pub struct RequiredHashDigests(Vec<HashDigest>);

impl AsRef<[HashDigest]> for RequiredHashDigests {
    fn as_ref(&self) -> &[HashDigest] {
        &self.0
    }
}

impl TrustedHashDigests for RequiredHashDigests {
    const MODE: HashCheckingMode = HashCheckingMode::Require;

    fn from_digests(mut digests: Vec<HashDigest>) -> Self {
        digests.retain(|digest| match digest {
            HashDigest::Md5(_) => false,
            HashDigest::Sha256(_)
            | HashDigest::Sha384(_)
            | HashDigest::Sha512(_)
            | HashDigest::Blake2b256(_) => true,
        });
        Self(digests)
    }

    fn retain(&mut self, predicate: impl FnMut(&HashDigest) -> bool) {
        self.0.retain(predicate);
    }

    fn merge(
        &mut self,
        incoming: &Self,
        requirement: impl Display,
    ) -> Result<(), HashStrategyError> {
        merge_digests(&mut self.0, &incoming.0, requirement)
    }

    fn verification(hashes: FxHashMap<VersionId, Self>) -> HashVerification {
        HashVerification::Required(Arc::new(hashes))
    }
}
