use uv_distribution_types::ArchiveHashPolicy;
use uv_pypi_types::HashAlgorithm;

/// Return the algorithms to compute for an HTTP distribution.
pub(crate) fn http_hash_algorithms(hashes: ArchiveHashPolicy<'_>) -> Vec<HashAlgorithm> {
    let mut algorithms = hashes.algorithms();
    algorithms.push(HashAlgorithm::Sha256);
    algorithms.sort();
    algorithms.dedup();
    algorithms
}
