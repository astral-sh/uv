use std::sync::Arc;

use distribution_types::VersionId;
use once_map::OnceMap;
use uv_normalize::PackageName;

use crate::resolver::provider::{MetadataResponse, VersionsResponse};

/// In-memory index of package metadata.
#[derive(Default, Clone)]
pub struct InMemoryIndex(Arc<InMemoryIndexState>);

#[derive(Default)]
struct InMemoryIndexState {
    /// A map from package name to the metadata for that package and the index where the metadata
    /// came from.
    pub(crate) packages: OnceMap<PackageName, Arc<VersionsResponse>>,

    /// A map from package ID to metadata for that distribution.
    pub(crate) distributions: OnceMap<VersionId, Arc<MetadataResponse>>,
}

impl InMemoryIndex {
    /// Insert a [`VersionsResponse`] into the index.
    pub fn packages(&self) -> &OnceMap<PackageName, Arc<VersionsResponse>> {
        &self.0.packages
    }

    /// Insert a [`Metadata23`] into the index.
    pub fn distributions(&self) -> &OnceMap<VersionId, Arc<MetadataResponse>> {
        &self.0.distributions
    }
}
