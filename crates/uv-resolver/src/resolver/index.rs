use std::hash::BuildHasherDefault;
use std::sync::Arc;

use distribution_types::VersionId;
use once_map::OnceMap;
use rustc_hash::FxHasher;
use uv_normalize::PackageName;

use crate::resolver::provider::{MetadataResponse, VersionsResponse};

/// In-memory index of package metadata.
#[derive(Default, Clone)]
pub struct InMemoryIndex(Arc<SharedInMemoryIndex>);

#[derive(Default)]
struct SharedInMemoryIndex {
    /// A map from package name to the metadata for that package and the index where the metadata
    /// came from.
    packages: FxOnceMap<PackageName, Arc<VersionsResponse>>,

    /// A map from package ID to metadata for that distribution.
    distributions: FxOnceMap<VersionId, Arc<MetadataResponse>>,
}

pub(crate) type FxOnceMap<K, V> = OnceMap<K, V, BuildHasherDefault<FxHasher>>;

impl InMemoryIndex {
    /// Create an `InMemoryIndex` with pre-filled packages and distributions.
    pub fn with(
        packages: FxOnceMap<PackageName, Arc<VersionsResponse>>,
        distributions: FxOnceMap<VersionId, Arc<MetadataResponse>>,
    ) -> InMemoryIndex {
        InMemoryIndex(Arc::new(SharedInMemoryIndex {
            packages,
            distributions,
        }))
    }

    /// Returns a reference to the package metadata map.
    pub fn packages(&self) -> &FxOnceMap<PackageName, Arc<VersionsResponse>> {
        &self.0.packages
    }

    /// Returns a reference to the distribution metadata map.
    pub fn distributions(&self) -> &FxOnceMap<VersionId, Arc<MetadataResponse>> {
        &self.0.distributions
    }
}
