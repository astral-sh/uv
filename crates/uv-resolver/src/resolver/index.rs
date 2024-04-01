use cache_key::CanonicalUrl;
use dashmap::DashMap;
use std::sync::Arc;
use url::Url;

use distribution_types::PackageId;
use once_map::OnceMap;
use pypi_types::Metadata23;
use uv_normalize::PackageName;

use super::provider::VersionsResponse;

/// In-memory index of package metadata.
#[derive(Default)]
pub struct InMemoryIndex {
    /// A map from package name to the metadata for that package and the index where the metadata
    /// came from.
    pub(crate) packages: OnceMap<PackageName, VersionsResponse>,

    /// A map from package ID to metadata for that distribution.
    pub(crate) distributions: OnceMap<PackageId, Metadata23>,

    /// A map from source URL to precise URL. For example, the source URL
    /// `git+https://github.com/pallets/flask.git` could be redirected to
    /// `git+https://github.com/pallets/flask.git@c2f65dd1cfff0672b902fd5b30815f0b4137214c`.
    pub(crate) redirects: DashMap<CanonicalUrl, Url>,
}

impl InMemoryIndex {
    /// Insert a [`VersionsResponse`] into the index.
    pub fn insert_package(&self, package_name: PackageName, metadata: VersionsResponse) {
        self.packages.done(package_name, metadata);
    }

    /// Insert a [`Metadata23`] into the index.
    pub fn insert_metadata(&self, package_id: PackageId, metadata: Metadata23) {
        self.distributions.done(package_id, metadata);
    }

    /// Insert a redirect from a source URL to a target URL.
    pub fn insert_redirect(&self, source: CanonicalUrl, target: Url) {
        self.redirects.insert(source, target);
    }

    /// Get the [`VersionsResponse`] for a given package name, without waiting.
    pub fn get_package(&self, package_name: &PackageName) -> Option<Arc<VersionsResponse>> {
        self.packages.get(package_name)
    }

    /// Get the [`Metadata23`] for a given package ID, without waiting.
    pub fn get_metadata(&self, package_id: &PackageId) -> Option<Arc<Metadata23>> {
        self.distributions.get(package_id)
    }
}
