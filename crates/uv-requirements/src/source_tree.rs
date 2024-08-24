use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use futures::stream::FuturesOrdered;
use futures::TryStreamExt;
use url::Url;

use distribution_types::{BuildableSource, DirectorySourceUrl, HashPolicy, SourceUrl, VersionId};
use pep508_rs::RequirementOrigin;
use pypi_types::Requirement;
use uv_configuration::ExtrasSpecification;
use uv_distribution::{DistributionDatabase, Reporter, RequiresDist};
use uv_fs::Simplified;
use uv_normalize::{ExtraName, PackageName};
use uv_resolver::{InMemoryIndex, MetadataResponse};
use uv_types::{BuildContext, HashStrategy};

#[derive(Debug, Clone)]
pub struct SourceTreeResolution {
    /// The requirements sourced from the source trees.
    pub requirements: Vec<Requirement>,
    /// The names of the projects that were resolved.
    pub project: PackageName,
    /// The extras used when resolving the requirements.
    pub extras: Vec<ExtraName>,
}

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
    pub async fn resolve(self) -> Result<Vec<SourceTreeResolution>> {
        let resolutions: Vec<_> = self
            .source_trees
            .iter()
            .map(|source_tree| async { self.resolve_source_tree(source_tree).await })
            .collect::<FuturesOrdered<_>>()
            .try_collect()
            .await?;
        Ok(resolutions)
    }

    /// Infer the dependencies for a directory dependency.
    async fn resolve_source_tree(&self, path: &Path) -> Result<SourceTreeResolution> {
        let metadata = self.resolve_requires_dist(path).await?;

        let origin = RequirementOrigin::Project(path.to_path_buf(), metadata.name.clone());

        // Determine the extras to include when resolving the requirements.
        let extras = match self.extras {
            ExtrasSpecification::All => metadata.provides_extras.as_slice(),
            ExtrasSpecification::None => &[],
            ExtrasSpecification::Some(extras) => extras,
        };

        // Determine the appropriate requirements to return based on the extras. This involves
        // evaluating the `extras` expression in any markers, but preserving the remaining marker
        // conditions.
        let mut requirements: Vec<Requirement> = metadata
            .requires_dist
            .into_iter()
            .map(|requirement| Requirement {
                origin: Some(origin.clone()),
                marker: requirement.marker.simplify_extras(extras),
                ..requirement
            })
            .collect();

        // Resolve any recursive extras.
        loop {
            // Find the first recursive requirement.
            // TODO(charlie): Respect markers on recursive extras.
            let Some(index) = requirements.iter().position(|requirement| {
                requirement.name == metadata.name && requirement.marker.is_true()
            }) else {
                break;
            };

            // Remove the requirement that points to us.
            let recursive = requirements.remove(index);

            // Re-simplify the requirements.
            for requirement in &mut requirements {
                requirement.marker = requirement
                    .marker
                    .clone()
                    .simplify_extras(&recursive.extras);
            }
        }

        let project = metadata.name;
        let extras = metadata.provides_extras;

        Ok(SourceTreeResolution {
            requirements,
            project,
            extras,
        })
    }

    /// Resolve the [`RequiresDist`] metadata for a given source tree. Attempts to resolve the
    /// requirements without building the distribution, even if the project contains (e.g.) a
    /// dynamic version since, critically, we don't need to install the package itself; only its
    /// dependencies.
    async fn resolve_requires_dist(&self, path: &Path) -> Result<RequiresDist> {
        // Convert to a buildable source.
        let source_tree = fs_err::canonicalize(path).with_context(|| {
            format!(
                "Failed to canonicalize path to source tree: {}",
                path.user_display()
            )
        })?;
        let source_tree = source_tree.parent().ok_or_else(|| {
            anyhow::anyhow!(
                "The file `{}` appears to be a `pyproject.toml`, `setup.py`, or `setup.cfg` file, which must be in a directory",
                path.user_display()
            )
        })?;

        // If the path is a `pyproject.toml`, attempt to extract the requirements statically.
        if let Ok(metadata) = self.database.requires_dist(source_tree).await {
            return Ok(metadata);
        }

        let Ok(url) = Url::from_directory_path(source_tree) else {
            return Err(anyhow::anyhow!("Failed to convert path to URL"));
        };
        let source = SourceUrl::Directory(DirectorySourceUrl {
            url: &url,
            install_path: Cow::Borrowed(source_tree),
            editable: false,
        });

        // Determine the hash policy. Since we don't have a package name, we perform a
        // manual match.
        let hashes = match self.hasher {
            HashStrategy::None => HashPolicy::None,
            HashStrategy::Generate => HashPolicy::Generate,
            HashStrategy::Verify(_) => HashPolicy::Generate,
            HashStrategy::Require(_) => {
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

        Ok(RequiresDist::from(metadata))
    }
}
