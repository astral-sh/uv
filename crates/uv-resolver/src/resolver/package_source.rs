use uv_distribution_types::IndexMetadata;
use uv_pypi_types::VerbatimParsedUrl;

use crate::fork_indexes::ForkIndexes;
use crate::fork_urls::ForkUrls;
use crate::pubgrub::PubGrubPackage;
use crate::resolver::urls::Urls;

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

impl<'a> PackageSource<'a> {
    /// Look up the effective source after the package's source constraints have been applied.
    /// Packages without a name use the default registry source, but never request metadata.
    pub(super) fn from_fork(
        package: &PubGrubPackage,
        fork_urls: &'a ForkUrls,
        fork_indexes: &'a ForkIndexes,
    ) -> Self {
        if let Some(url) = package.name().and_then(|name| fork_urls.get(name)) {
            Self::Url(url)
        } else {
            Self::Registry(package.name().and_then(|name| fork_indexes.get(name)))
        }
    }

    /// Look up a source before selection, when some URL constraints may still be unresolved.
    ///
    /// `None` defers the request until selection determines the source. It is distinct from
    /// `Registry(None)`, which requests candidates without an explicit index.
    pub(super) fn for_prefetch(
        package: &PubGrubPackage,
        fork_urls: &'a ForkUrls,
        fork_indexes: &'a ForkIndexes,
        urls: &Urls,
    ) -> Option<Self> {
        let source = Self::from_fork(package, fork_urls, fork_indexes);
        match source {
            Self::Url(_) => Some(source),
            Self::Registry(_) => {
                if package.name().is_none_or(|name| urls.any_url(name)) {
                    None
                } else {
                    Some(source)
                }
            }
        }
    }
}
