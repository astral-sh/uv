use std::sync::Arc;

use distribution_types::PackageId;
use once_map::OnceMap;
use uv_normalize::PackageName;

use crate::resolver::provider::{MetadataResponse, VersionsResponse};

/// In-memory index of package metadata.
#[derive(Default)]
pub struct InMemoryIndex {
    /// A map from package name to the metadata for that package and the index where the metadata
    /// came from.
    pub(crate) packages: OnceMap<PackageName, VersionsResponse>,

    /// A map from package ID to metadata for that distribution.
    pub(crate) distributions: OnceMap<PackageId, MetadataResponse>,
}

impl InMemoryIndex {
    /// Insert a [`VersionsResponse`] into the index.
    pub fn insert_package(&self, package_name: PackageName, response: VersionsResponse) {
        self.packages.done(package_name, response);
    }

    /// Insert a [`Metadata23`] into the index.
    pub fn insert_metadata(&self, package_id: PackageId, response: MetadataResponse) {
        self.distributions.done(package_id, response);
    }

    /// Get the [`VersionsResponse`] for a given package name, without waiting.
    pub fn get_package(&self, package_name: &PackageName) -> Option<Arc<VersionsResponse>> {
        self.packages.get(package_name)
    }

    /// Get the [`MetadataResponse`] for a given package ID, without waiting.
    pub fn get_metadata(&self, package_id: &PackageId) -> Option<Arc<MetadataResponse>> {
        self.distributions.get(package_id)
    }
}
