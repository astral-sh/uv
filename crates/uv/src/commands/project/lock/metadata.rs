use std::path::Path;
use std::sync::Arc;

use uv_distribution::{DistributionDatabase, DistributionMetadataIndex, MetadataResponse};
use uv_distribution_types::{
    ArchiveHashPolicy, Dist, HashValidation, Identifier, Metadata, MetadataHashPolicy, RequiresDist,
};
use uv_lock::LockMetadataProvider;
use uv_pypi_types::PyProjectToml;
use uv_types::{BuildContext, HashStrategy};

/// Reuse the resolver's metadata cache and hash policy during lock validation.
pub(super) struct MetadataProvider<'a, 'context, Context: BuildContext> {
    pub(super) database: &'a DistributionDatabase<'context, Context>,
    pub(super) index: &'a DistributionMetadataIndex,
    pub(super) hasher: &'a HashStrategy,
}

impl<Context: BuildContext> LockMetadataProvider for MetadataProvider<'_, '_, Context> {
    type Error = uv_distribution::Error;

    fn is_offline(&self) -> bool {
        self.database.client().unmanaged.connectivity().is_offline()
    }

    async fn metadata(
        &self,
        dist: &Dist,
        locked_hashes: Option<HashValidation<'_>>,
    ) -> Result<Metadata, Self::Error> {
        let id = dist.distribution_id();
        if let Some(archive) = self.index.get(&id).as_deref().and_then(|response| {
            if let MetadataResponse::Found(archive, ..) = response {
                Some(archive)
            } else {
                None
            }
        }) && locked_hashes.is_none_or(|validation| {
            ArchiveHashPolicy::from(validation).matches(archive.hashes.as_slice())
        }) {
            return Ok(archive.metadata.clone());
        }

        let metadata_hashes = if let Some(validation) = locked_hashes {
            MetadataHashPolicy {
                collection: self.hasher.collection(),
                validation,
            }
        } else {
            self.hasher.metadata_policy(dist)
        };
        let archive = self
            .database
            .get_or_build_wheel_metadata(dist, metadata_hashes)
            .await?;
        let metadata = archive.metadata.clone();
        self.index
            .done(id, Arc::new(MetadataResponse::Found(archive)));
        Ok(metadata)
    }

    async fn requires_dist(
        &self,
        path: &Path,
        pyproject: &PyProjectToml,
    ) -> Result<Option<RequiresDist>, Self::Error> {
        self.database.requires_dist(path, pyproject).await
    }
}
