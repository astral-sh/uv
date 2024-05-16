use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use futures::stream::FuturesOrdered;
use futures::TryStreamExt;
use url::Url;

use distribution_types::{
    BuildableSource, DirectorySourceUrl, HashPolicy, Requirement, SourceUrl, VersionId,
};
use pep508_rs::RequirementOrigin;
use uv_distribution::{DistributionDatabase, Reporter};
use uv_fs::Simplified;
use uv_resolver::{InMemoryIndex, MetadataResponse};
use uv_types::{BuildContext, HashStrategy};

use crate::ExtrasSpecification;

/// A resolver for requirements specified via source trees.
///
/// Used, e.g., to determine the input requirements when a user specifies a `pyproject.toml`
/// file, which may require running PEP 517 build hooks to extract metadata.
pub struct SourceTreeResolver<'a, Context: BuildContext> {
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

impl<'a, Context: BuildContext> SourceTreeResolver<'a, Context> {
    /// Instantiate a new [`SourceTreeResolver`] for a given set of `source_trees`.
    pub fn new(
        source_trees: Vec<PathBuf>,
        extras: &'a ExtrasSpecification,
        hasher: &'a HashStrategy,
        index: &'a InMemoryIndex,
        database: DistributionDatabase<'a, Context>,
    ) -> Self {
        Self {
            source_trees,
            extras,
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

    /// Resolve the requirements from the provided source trees.
    pub async fn resolve(self) -> Result<Vec<Requirement>> {
        let requirements: Vec<_> = self
            .source_trees
            .iter()
            .map(|source_tree| async { self.resolve_source_tree(source_tree).await })
            .collect::<FuturesOrdered<_>>()
            .try_collect()
            .await?;
        Ok(requirements
            .into_iter()
            .flatten()
            .map(Requirement::from_pep508)
            .collect::<Result<_, _>>()?)
    }

    /// Infer the package name for a given "unnamed" requirement.
    async fn resolve_source_tree(&self, path: &Path) -> Result<Vec<pep508_rs::Requirement>> {
        // Convert to a buildable source.
        let source_tree = fs_err::canonicalize(path).with_context(|| {
            format!(
                "Failed to canonicalize path to source tree: {}",
                path.user_display()
            )
        })?;
        let source_tree = source_tree.parent().ok_or_else(|| {
            anyhow::anyhow!(
                "The file `{}` appears to be a `setup.py` or `setup.cfg` file, which must be in a directory",
                path.user_display()
            )
        })?;

        let Ok(url) = Url::from_directory_path(source_tree) else {
            return Err(anyhow::anyhow!("Failed to convert path to URL"));
        };
        let source = SourceUrl::Directory(DirectorySourceUrl {
            url: &url,
            path: Cow::Borrowed(source_tree),
        });

        // Determine the hash policy. Since we don't have a package name, we perform a
        // manual match.
        let hashes = match self.hasher {
            HashStrategy::None => HashPolicy::None,
            HashStrategy::Generate => HashPolicy::Generate,
            HashStrategy::Validate { .. } => {
                return Err(anyhow::anyhow!(
                    "Hash-checking is not supported for local directories: {}",
                    path.user_display()
                ));
            }
        };

        // Fetch the metadata for the distribution.
        let metadata = {
            let id = VersionId::from_url(source.url());
            if let Some(archive) =
                self.index
                    .distributions()
                    .get(&id)
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
                    .distributions()
                    .done(id, Arc::new(MetadataResponse::Found(archive.clone())));

                archive.metadata
            }
        };

        // Extract the origin.
        let origin = RequirementOrigin::Project(path.to_path_buf(), metadata.name.clone());

        // Determine the appropriate requirements to return based on the extras. This involves
        // evaluating the `extras` expression in any markers, but preserving the remaining marker
        // conditions.
        match self.extras {
            ExtrasSpecification::None => Ok(metadata
                .requires_dist
                .into_iter()
                .map(|requirement| requirement.with_origin(origin.clone()))
                .collect()),
            ExtrasSpecification::All => Ok(metadata
                .requires_dist
                .into_iter()
                .map(|requirement| pep508_rs::Requirement {
                    origin: Some(origin.clone()),
                    marker: requirement
                        .marker
                        .and_then(|marker| marker.simplify_extras(&metadata.provides_extras)),
                    ..requirement
                })
                .collect()),
            ExtrasSpecification::Some(extras) => Ok(metadata
                .requires_dist
                .into_iter()
                .map(|requirement| pep508_rs::Requirement {
                    origin: Some(origin.clone()),
                    marker: requirement
                        .marker
                        .and_then(|marker| marker.simplify_extras(extras)),
                    ..requirement
                })
                .collect()),
        }
    }
}
