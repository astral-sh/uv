use uv_distribution_types::HashPolicy;
use uv_pypi_types::{HashAlgorithm, HashDigests, Hashes};
use uv_redacted::DisplaySafeUrl;

/// Return URL-provided hashes that can be reused instead of generating a hash.
///
/// Ignore MD5, which cannot satisfy `--require-hashes`.
pub(crate) fn url_hashes(url: &DisplaySafeUrl) -> Option<HashDigests> {
    let hashes = url
        .fragment()?
        .split('&')
        .find_map(|fragment| Hashes::parse_fragment(fragment).ok())?;
    hashes.md5.is_none().then(|| HashDigests::from(hashes))
}

/// Return the algorithms to compute for an HTTP distribution.
pub(crate) fn http_hash_algorithms(hashes: HashPolicy<'_>) -> Vec<HashAlgorithm> {
    let mut algorithms = hashes.algorithms();
    algorithms.push(HashAlgorithm::Sha256);
    algorithms.sort();
    algorithms.dedup();
    algorithms
}
