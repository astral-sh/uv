use std::borrow::Cow;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use futures::{StreamExt, TryStreamExt};
use url::Url;

use distribution_types::{
    BuildableSource, HashPolicy, PathSourceUrl, SourceUrl, UvRequirement, VersionId,
};
use pep508_rs::Requirement;

use uv_client::RegistryClient;
use uv_distribution::{DistributionDatabase, Reporter};
use uv_fs::Simplified;
use uv_resolver::{InMemoryIndex, MetadataResponse};
use uv_types::{BuildContext, HashStrategy};

use crate::ExtrasSpecification;

/// A resolver for requirements specified via source trees.
///
/// Used, e.g., to determine the input requirements when a user specifies a `pyproject.toml`
/// file, which may require running PEP 517 build hooks to extract metadata.
pub struct SourceTreeResolver<'a, Context: BuildContext + Send + Sync> {
    /// The requirements for the project.
    source_trees: Vec<PathBuf>,
    /// The extras to include when resolving requirements.
    extras: &'a ExtrasSpecification,
    /// The hash policy to enforce.
    hasher: &'a HashStrategy,
    /// The in-memory index for resolving dependencies.
    index: &'a InMemoryIndex,
    /// The database for fetching and building distributions.
    database: DistributionDatabase<'a, Context>,
}

impl<'a, Context: BuildContext + Send + Sync> SourceTreeResolver<'a, Context> {
    /// Instantiate a new [`SourceTreeResolver`] for a given set of `source_trees`.
    pub fn new(
        source_trees: Vec<PathBuf>,
        extras: &'a ExtrasSpecification,
        hasher: &'a HashStrategy,
        context: &'a Context,
        client: &'a RegistryClient,
        index: &'a InMemoryIndex,
    ) -> Self {
        Self {
            source_trees,
            extras,
            hasher,
            index,
            database: DistributionDatabase::new(client, context),
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

    /// Resolve the requirements from the provided source trees.
    pub async fn resolve(self) -> Result<Vec<UvRequirement>> {
        let requirements: Vec<_> = futures::stream::iter(self.source_trees.iter())
            .map(|source_tree| async { self.resolve_source_tree(source_tree).await })
            .buffered(50)
            .try_collect()
            .await?;
        Ok(requirements
            .into_iter()
            .flatten()
            .map(UvRequirement::from_requirement)
            .collect::<Result<_, _>>()?)
    }

    /// Infer the package name for a given "unnamed" requirement.
    async fn resolve_source_tree(&self, source_tree: &Path) -> Result<Vec<Requirement>> {
        // Convert to a buildable source.
        let path = fs_err::canonicalize(source_tree).with_context(|| {
            format!(
                "Failed to canonicalize path to source tree: {}",
                source_tree.user_display()
            )
        })?;
        let Ok(url) = Url::from_directory_path(&path) else {
            return Err(anyhow::anyhow!("Failed to convert path to URL"));
        };
        let source = SourceUrl::Path(PathSourceUrl {
            url: &url,
            path: Cow::Owned(path),
        });

        // Determine the hash policy. Since we don't have a package name, we perform a
        // manual match.
        let hashes = match self.hasher {
            HashStrategy::None => HashPolicy::None,
            HashStrategy::Generate => HashPolicy::Generate,
            HashStrategy::Validate { .. } => {
                return Err(anyhow::anyhow!(
                    "Hash-checking is not supported for local directories: {}",
                    source_tree.user_display()
                ));
            }
        };

        // Fetch the metadata for the distribution.
        let metadata = {
            let id = VersionId::from_url(source.url());
            if let Some(archive) = self
                .index
                .get_metadata(&id)
                .as_deref()
                .and_then(|response| {
                    if let MetadataResponse::Found(archive) = response {
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
                let source = BuildableSource::Url(source);
                let archive = self.database.build_wheel_metadata(&source, hashes).await?;

                // Insert the metadata into the index.
                self.index
                    .insert_metadata(id, MetadataResponse::Found(archive.clone()));

                archive.metadata
            }
        };

        // Determine the appropriate requirements to return based on the extras. This involves
        // evaluating the `extras` expression in any markers, but preserving the remaining marker
        // conditions.
        match self.extras {
            ExtrasSpecification::None => Ok(metadata.requires_dist),
            ExtrasSpecification::All => Ok(metadata
                .requires_dist
                .into_iter()
                .map(|requirement| Requirement {
                    marker: requirement
                        .marker
                        .and_then(|marker| marker.simplify_extras(&metadata.provides_extras)),
                    ..requirement
                })
                .collect()),
            ExtrasSpecification::Some(extras) => Ok(metadata
                .requires_dist
                .into_iter()
                .map(|requirement| Requirement {
                    marker: requirement
                        .marker
                        .and_then(|marker| marker.simplify_extras(extras)),
                    ..requirement
                })
                .collect()),
        }
    }
}
