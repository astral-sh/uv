use pypi_types::HashDigest;

pub trait Hashed {
    /// Return the [`HashDigest`]s for the archive.
    fn hashes(&self) -> &[HashDigest];

    /// Returns `true` if the archive satisfies the given hashes.
    fn satisfies(&self, hashes: &[HashDigest]) -> bool {
        if hashes.is_empty() {
            true
        } else {
            self.hashes().iter().any(|hash| hashes.contains(hash))
        }
    }

    /// Returns `true` if the archive includes a hash for at least one of the given algorithms.
    fn has_digests(&self, hashes: &[HashDigest]) -> bool {
        if hashes.is_empty() {
            true
        } else {
            hashes
                .iter()
                .map(HashDigest::algorithm)
                .any(|algorithm| self.hashes().iter().any(|hash| hash.algorithm == algorithm))
        }
    }
}
