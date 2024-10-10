use std::sync::Arc;

use futures::{stream::FuturesOrdered, TryStreamExt};
use thiserror::Error;

use uv_distribution::{DistributionDatabase, Reporter};
use uv_distribution_types::{BuiltDist, Dist, DistributionMetadata, SourceDist};
use uv_pypi_types::Requirement;
use uv_resolver::{InMemoryIndex, MetadataResponse};
use uv_types::{BuildContext, HashStrategy};

use crate::required_dist;

#[derive(Debug, Error)]
pub enum ExtrasError {
    #[error("Failed to download: `{0}`")]
    Download(BuiltDist, #[source] uv_distribution::Error),
    #[error("Failed to download and build: `{0}`")]
    DownloadAndBuild(SourceDist, #[source] uv_distribution::Error),
    #[error("Failed to build: `{0}`")]
    Build(SourceDist, #[source] uv_distribution::Error),
    #[error(transparent)]
    UnsupportedUrl(#[from] uv_distribution_types::Error),
}

/// A resolver to expand the requested extras for a set of requirements to include all defined
/// extras.
pub struct ExtrasResolver<'a, Context: BuildContext> {
    /// Whether to check hashes for distributions.
    hasher: &'a HashStrategy,
    /// The in-memory index for resolving dependencies.
    index: &'a InMemoryIndex,
    /// The database for fetching and building distributions.
    database: DistributionDatabase<'a, Context>,
}

impl<'a, Context: BuildContext> ExtrasResolver<'a, Context> {
    /// Instantiate a new [`ExtrasResolver`] for a given set of requirements.
    pub fn new(
        hasher: &'a HashStrategy,
        index: &'a InMemoryIndex,
        database: DistributionDatabase<'a, Context>,
    ) -> Self {
        Self {
            hasher,
            index,
            database,
        }
    }

    /// Set the [`Reporter`] to use for this resolver.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        Self {
            database: self.database.with_reporter(reporter),
            ..self
        }
    }

    /// Expand the set of available extras for a given set of requirements.
    pub async fn resolve(
        self,
        requirements: impl Iterator<Item = Requirement>,
    ) -> Result<Vec<Requirement>, ExtrasError> {
        let Self {
            hasher,
            index,
            database,
        } = self;
        requirements
            .map(|requirement| async {
                Self::resolve_requirement(requirement, hasher, index, &database)
                    .await
                    .map(Requirement::from)
            })
            .collect::<FuturesOrdered<_>>()
            .try_collect()
            .await
    }

    /// Expand the set of available extras for a given [`Requirement`].
    async fn resolve_requirement(
        requirement: Requirement,
        hasher: &HashStrategy,
        index: &InMemoryIndex,
        database: &DistributionDatabase<'a, Context>,
    ) -> Result<Requirement, ExtrasError> {
        // Determine whether the requirement represents a local distribution and convert to a
        // buildable distribution.
        let Some(dist) = required_dist(&requirement)? else {
            return Ok(requirement);
        };

        // Fetch the metadata for the distribution.
        let metadata = {
            let id = dist.version_id();
            if let Some(archive) = index
                .distributions()
                .get(&id)
                .as_deref()
                .and_then(|response| {
                    if let MetadataResponse::Found(archive, ..) = response {
                        Some(archive)
                    } else {
                        None
                    }
                })
            {
                // If the metadata is already in the index, return it.
                archive.metadata.clone()
            } else {
                // Run the PEP 517 build process to extract metadata from the source distribution.
                let archive = database
                    .get_or_build_wheel_metadata(&dist, hasher.get(&dist))
                    .await
                    .map_err(|err| match &dist {
                        Dist::Built(built) => ExtrasError::Download(built.clone(), err),
                        Dist::Source(source) => {
                            if source.is_local() {
                                ExtrasError::Build(source.clone(), err)
                            } else {
                                ExtrasError::DownloadAndBuild(source.clone(), err)
                            }
                        }
                    })?;

                let metadata = archive.metadata.clone();

                // Insert the metadata into the index.
                index
                    .distributions()
                    .done(id, Arc::new(MetadataResponse::Found(archive)));

                metadata
            }
        };

        // Sort extras for consistency.
        let extras = {
            let mut extras = metadata.provides_extras;
            extras.sort_unstable();
            extras
        };

        Ok(Requirement {
            extras,
            ..requirement
        })
    }
}
