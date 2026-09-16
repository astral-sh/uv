use uv_distribution_types::IndexMetadata;
use uv_pypi_types::VerbatimParsedUrl;

/// The source used to select a package in the current fork.
///
/// Unlike an edge's [`crate::pubgrub::DependencySource`], this incorporates source constraints
/// from other dependencies in the fork. A URL takes precedence over a registry requirement.
#[derive(Debug, Clone, Copy)]
pub(super) enum PackageSource<'a> {
    Url(&'a VerbatimParsedUrl),
    /// Use version-based selection, optionally restricting registry lookups to an explicit
    /// index. Candidates can also include installed distributions and, without an explicit
    /// index, flat-index entries.
    Registry(Option<&'a IndexMetadata>),
}
