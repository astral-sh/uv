use uv_pypi_types::{HashAlgorithm, HashDigest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashPolicy<'a> {
    /// No hash policy is specified.
    None,
    /// Hashes should be generated (specifically, a SHA-256 hash), but not validated.
    Generate(HashGeneration),
    /// Hashes should be validated against a pre-defined list of hashes. If necessary, hashes should
    /// be generated so as to ensure that the archive is valid.
    Validate(&'a [HashDigest]),
}

impl HashPolicy<'_> {
    /// Returns `true` if the hash policy is `None`.
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Returns `true` if the hash policy is `Validate`.
    pub fn is_validate(&self) -> bool {
        matches!(self, Self::Validate(_))
    }

    /// Returns `true` if the hash policy indicates that hashes should be generated.
    pub fn is_generate(&self, dist: &crate::BuiltDist) -> bool {
        match self {
            HashPolicy::Generate(HashGeneration::Url) => dist.file().is_none(),
            HashPolicy::Generate(HashGeneration::All) => {
                dist.file().is_none_or(|file| file.hashes.is_empty())
            }
            HashPolicy::Validate(_) => false,
            HashPolicy::None => false,
        }
    }

    /// Return the algorithms used in the hash policy.
    pub fn algorithms(&self) -> Vec<HashAlgorithm> {
        match self {
            Self::None => vec![],
            Self::Generate(_) => vec![HashAlgorithm::Sha256],
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
            Self::Generate(_) => &[],
            Self::Validate(hashes) => hashes,
        }
    }
}

/// The context in which hashes should be generated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashGeneration {
    /// Generate hashes for direct URL distributions.
    Url,
    /// Generate hashes for direct URL distributions, along with any distributions that are hosted
    /// on a registry that does _not_ provide hashes.
    All,
}

pub trait Hashed {
    /// Return the [`HashDigest`]s for the archive.
    fn hashes(&self) -> &[HashDigest];

    /// Returns `true` if the archive satisfies the given hash policy.
    fn satisfies(&self, hashes: HashPolicy) -> bool {
        match hashes {
            HashPolicy::None => true,
            HashPolicy::Generate(_) => self
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
            HashPolicy::Generate(_) => self
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
