use uv_distribution_types::{ArchiveHashPolicy, BuiltDist};
use uv_pypi_types::{HashAlgorithm, HashDigests, Hashes};
use uv_redacted::DisplaySafeUrl;

/// Return the algorithms to compute for an HTTP distribution.
fn http_hash_algorithms(hashes: ArchiveHashPolicy<'_>) -> Vec<HashAlgorithm> {
    let mut algorithms = hashes.algorithms();
    algorithms.push(HashAlgorithm::Sha256);
    algorithms.sort();
    algorithms.dedup();
    algorithms
}

/// Use a URL hash to select cached contents unless explicit hash requirements take precedence.
pub(crate) fn url_hashes_for_cache(
    url: &DisplaySafeUrl,
    hashes: ArchiveHashPolicy<'_>,
) -> Option<HashDigests> {
    match hashes {
        ArchiveHashPolicy::None | ArchiveHashPolicy::Generate => {}
        ArchiveHashPolicy::Any(_) | ArchiveHashPolicy::All(_) => return None,
    }
    url.fragment()?
        .split('&')
        .find_map(|fragment| Hashes::parse_fragment(fragment).ok())
        .map(HashDigests::from)
}

/// Compute URL digests needed to recognize the same wheel in subsequent cache lookups.
pub(crate) fn http_wheel_hash_algorithms(
    dist: &BuiltDist,
    hashes: ArchiveHashPolicy<'_>,
) -> Vec<HashAlgorithm> {
    let url_hashes = match dist {
        BuiltDist::DirectUrl(wheel) => url_hashes_for_cache(&wheel.url, hashes),
        BuiltDist::Registry(_) | BuiltDist::Path(_) | BuiltDist::GitPath(_) => None,
    };
    http_hash_algorithms(
        url_hashes
            .as_ref()
            .map_or(hashes, |hashes| ArchiveHashPolicy::All(hashes.as_slice())),
    )
}
