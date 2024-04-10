use pypi_types::{HashAlgorithm, HashDigest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashPolicy<'a> {
    /// No hash policy is specified.
    None,
    /// Hashes should be generated (specifically, a SHA-256 hash), but not validated.
    Generate,
    /// Hashes should be validated against a pre-defined list of hashes. If necessary, hashes should
    /// be generated so as to ensure that the archive is valid.
    Validate(&'a [HashDigest]),
}

impl<'a> HashPolicy<'a> {
    /// Returns `true` if the hash policy is `None`.
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Returns `true` if the hash policy is `Generate`.
    pub fn is_generate(&self) -> bool {
        matches!(self, Self::Generate)
    }

    /// Returns `true` if the hash policy is `Validate`.
    pub fn is_validate(&self) -> bool {
        matches!(self, Self::Validate(_))
    }

    /// Return the algorithms used in the hash policy.
    pub fn algorithms(&self) -> Vec<HashAlgorithm> {
        match self {
            Self::None => vec![],
            Self::Generate => vec![HashAlgorithm::Sha256],
            Self::Validate(hashes) => {
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
            Self::Generate => &[],
            Self::Validate(hashes) => hashes,
        }
    }
}

pub trait Hashed {
    /// Return the [`HashDigest`]s for the archive.
    fn hashes(&self) -> &[HashDigest];

    /// Returns `true` if the archive satisfies the given hash policy.
    fn satisfies(&self, hashes: HashPolicy) -> bool {
        match hashes {
            HashPolicy::None => true,
            HashPolicy::Generate => self
                .hashes()
                .iter()
                .any(|hash| hash.algorithm == HashAlgorithm::Sha256),
            HashPolicy::Validate(hashes) => self.hashes().iter().any(|hash| hashes.contains(hash)),
        }
    }

    /// Returns `true` if the archive includes a hash for at least one of the given algorithms.
    fn has_digests(&self, hashes: HashPolicy) -> bool {
        match hashes {
            HashPolicy::None => true,
            HashPolicy::Generate => self
                .hashes()
                .iter()
                .any(|hash| hash.algorithm == HashAlgorithm::Sha256),
            HashPolicy::Validate(hashes) => hashes
                .iter()
                .map(HashDigest::algorithm)
                .any(|algorithm| self.hashes().iter().any(|hash| hash.algorithm == algorithm)),
        }
    }
}
