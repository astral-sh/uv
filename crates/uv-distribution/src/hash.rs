use uv_distribution_types::ArchiveHashPolicy;
use uv_pypi_types::{HashAlgorithm, HashDigests, Hashes};
use uv_redacted::DisplaySafeUrl;

/// Read a URL's declared hash, excluding MD5, which `--require-hashes` rejects.
pub(crate) fn parse_url_hashes(url: &DisplaySafeUrl) -> Option<HashDigests> {
    let hashes = url
        .fragment()?
        .split('&')
        .find_map(|fragment| Hashes::parse_fragment(fragment).ok())?;
    hashes.md5.is_none().then(|| HashDigests::from(hashes))
}

/// Return the algorithms to compute for an HTTP distribution.
pub(crate) fn http_hash_algorithms(hashes: ArchiveHashPolicy<'_>) -> Vec<HashAlgorithm> {
    let mut algorithms = hashes.algorithms();
    algorithms.push(HashAlgorithm::Sha256);
    algorithms.sort();
    algorithms.dedup();
    algorithms
}
