use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use futures::{StreamExt, TryStreamExt};
use url::Url;

use distribution_types::{BuildableSource, PathSourceUrl, SourceUrl};
use pep508_rs::Requirement;
use uv_client::RegistryClient;
use uv_distribution::{Reporter, SourceDistCachedBuilder};
use uv_types::BuildContext;

use crate::ExtrasSpecification;

/// A resolver for requirements specified via source trees.
///
/// Used, e.g., to determine the the input requirements when a user specifies a `pyproject.toml`
/// file, which may require running PEP 517 build hooks to extract metadata.
pub struct SourceTreeResolver<'a> {
    /// The requirements for the project.
    source_trees: Vec<PathBuf>,
    /// The extras to include when resolving requirements.
    extras: &'a ExtrasSpecification<'a>,
    /// The reporter to use when building source distributions.
    reporter: Option<Arc<dyn Reporter>>,
}

impl<'a> SourceTreeResolver<'a> {
    /// Instantiate a new [`SourceTreeResolver`] for a given set of `source_trees`.
    pub fn new(source_trees: Vec<PathBuf>, extras: &'a ExtrasSpecification<'a>) -> Self {
        Self {
            source_trees,
            extras,
            reporter: None,
        }
    }

    /// Set the [`Reporter`] to use for this resolver.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        let reporter: Arc<dyn Reporter> = Arc::new(reporter);
        Self {
            reporter: Some(reporter),
            ..self
        }
    }

    /// Resolve the requirements from the provided source trees.
    pub async fn resolve<T: BuildContext>(
        self,
        context: &T,
        client: &RegistryClient,
    ) -> Result<Vec<Requirement>> {
        let requirements: Vec<_> = futures::stream::iter(self.source_trees.iter())
            .map(|source_tree| async {
                self.resolve_source_tree(source_tree, context, client).await
            })
            .buffered(50)
            .try_collect()
            .await?;
        Ok(requirements.into_iter().flatten().collect())
    }

    /// Infer the package name for a given "unnamed" requirement.
    async fn resolve_source_tree<T: BuildContext>(
        &self,
        source_tree: &Path,
        context: &T,
        client: &RegistryClient,
    ) -> Result<Vec<Requirement>> {
        // Convert to a buildable source.
        let path = fs_err::canonicalize(source_tree).with_context(|| {
            format!(
                "Failed to canonicalize path to source tree: {}",
                source_tree.display()
            )
        })?;
        let Ok(url) = Url::from_directory_path(&path) else {
            return Err(anyhow::anyhow!("Failed to convert path to URL"));
        };
        let source = BuildableSource::Url(SourceUrl::Path(PathSourceUrl {
            url: &url,
            path: Cow::Owned(path),
        }));

        // Run the PEP 517 build process to extract metadata from the source distribution.
        let builder = if let Some(reporter) = self.reporter.clone() {
            SourceDistCachedBuilder::new(context, client).with_reporter(reporter)
        } else {
            SourceDistCachedBuilder::new(context, client)
        };

        let metadata = builder.download_and_build_metadata(&source).await?;

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
