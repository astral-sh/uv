use uv_distribution_types::{BuiltDist, HashPolicy};
use uv_pypi_types::{HashAlgorithm, HashDigests, Hashes};

/// Return the algorithms to compute for an HTTP distribution.
pub(crate) fn http_hash_algorithms(hashes: HashPolicy<'_>) -> Vec<HashAlgorithm> {
    let mut algorithms = hashes.algorithms();
    algorithms.push(HashAlgorithm::Sha256);
    algorithms.sort();
    algorithms.dedup();
    algorithms
}

/// Return the URL hash that must match before reusing a shared wheel cache entry during generation.
pub(crate) fn url_hashes_for_generation(
    dist: &BuiltDist,
    hashes: HashPolicy<'_>,
) -> Option<HashDigests> {
    if !hashes.is_generate(dist) {
        return None;
    }
    let BuiltDist::DirectUrl(wheel) = dist else {
        return None;
    };
    wheel
        .url
        .fragment()?
        .split('&')
        .find_map(|fragment| Hashes::parse_fragment(fragment).ok())
        .map(HashDigests::from)
}

/// Include the URL's hash algorithm so subsequent generation can reuse the canonical wheel entry.
pub(crate) fn http_wheel_hash_algorithms(
    dist: &BuiltDist,
    hashes: HashPolicy<'_>,
) -> Vec<HashAlgorithm> {
    let url_hashes = url_hashes_for_generation(dist, hashes);
    http_hash_algorithms(
        url_hashes
            .as_ref()
            .map_or(hashes, |hashes| HashPolicy::All(hashes.as_slice())),
    )
}
