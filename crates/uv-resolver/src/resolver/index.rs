use std::rc::Rc;

use distribution_types::VersionId;
use once_map::OnceMap;
use uv_normalize::PackageName;

use crate::resolver::provider::{MetadataResponse, VersionsResponse};

/// In-memory index of package metadata.
#[derive(Default)]
pub struct InMemoryIndex {
    /// A map from package name to the metadata for that package and the index where the metadata
    /// came from.
    pub(crate) packages: OnceMap<PackageName, Rc<VersionsResponse>>,

    /// A map from package ID to metadata for that distribution.
    pub(crate) distributions: OnceMap<VersionId, Rc<MetadataResponse>>,
}

impl InMemoryIndex {
    /// Insert a [`VersionsResponse`] into the index.
    pub fn insert_package(&self, package_name: PackageName, response: VersionsResponse) {
        self.packages.done(package_name, Rc::new(response));
    }

    /// Insert a [`Metadata23`] into the index.
    pub fn insert_metadata(&self, version_id: VersionId, response: MetadataResponse) {
        self.distributions.done(version_id, Rc::new(response));
    }

    /// Get the [`VersionsResponse`] for a given package name, without waiting.
    pub fn get_package(&self, package_name: &PackageName) -> Option<Rc<VersionsResponse>> {
        self.packages.get(package_name)
    }

    /// Get the [`MetadataResponse`] for a given package ID, without waiting.
    pub fn get_metadata(&self, version_id: &VersionId) -> Option<Rc<MetadataResponse>> {
        self.distributions.get(version_id)
    }
}
