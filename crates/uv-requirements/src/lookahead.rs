use std::sync::Arc;

use anyhow::{Context, Result};
use futures::{StreamExt, TryStreamExt};

use distribution_types::{BuildableSource, Dist};
use pep508_rs::{Requirement, VersionOrUrl};
use uv_client::RegistryClient;
use uv_distribution::{Reporter, SourceDistCachedBuilder};
use uv_types::{BuildContext, RequestedRequirements};

/// A resolver for resolving lookahead requirements from local dependencies.
///
/// The resolver extends certain privileges to "first-party" requirements. For example, first-party
/// requirements are allowed to contain direct URL references, local version specifiers, and more.
///
/// We make an exception for transitive requirements of _local_ dependencies. For example,
/// `pip install .` should treat the dependencies of `.` as if they were first-party dependencies.
/// This matches our treatment of editable installs (`pip install -e .`).
///
/// The lookahead resolver resolves requirements for local dependencies, so that the resolver can
/// treat them as first-party dependencies for the purpose of analyzing their specifiers.
pub struct LookaheadResolver<'a> {
    /// The requirements for the project.
    requirements: &'a [Requirement],
    /// The reporter to use when building source distributions.
    reporter: Option<Arc<dyn Reporter>>,
}

impl<'a> LookaheadResolver<'a> {
    /// Instantiate a new [`LookaheadResolver`] for a given set of `source_trees`.
    pub fn new(requirements: &'a [Requirement]) -> Self {
        Self {
            requirements,
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
    ) -> Result<Vec<RequestedRequirements>> {
        let requirements: Vec<_> = futures::stream::iter(self.requirements.iter())
            .map(|requirement| async { self.lookahead(requirement, context, client).await })
            .buffered(50)
            .try_collect()
            .await?;
        Ok(requirements.into_iter().flatten().collect())
    }

    /// Infer the package name for a given "unnamed" requirement.
    async fn lookahead<T: BuildContext>(
        &self,
        requirement: &Requirement,
        context: &T,
        client: &RegistryClient,
    ) -> Result<Option<RequestedRequirements>> {
        // Determine whether the requirement represents a local distribution.
        let Some(VersionOrUrl::Url(url)) = requirement.version_or_url.as_ref() else {
            return Ok(None);
        };

        // Convert to a buildable distribution.
        let dist = Dist::from_url(requirement.name.clone(), url.clone())?;

        // Only support source trees (and not, e.g., wheels).
        let Dist::Source(source_dist) = &dist else {
            return Ok(None);
        };
        if !source_dist.as_path().is_some_and(std::path::Path::is_dir) {
            return Ok(None);
        }

        // Run the PEP 517 build process to extract metadata from the source distribution.
        let builder = if let Some(reporter) = self.reporter.clone() {
            SourceDistCachedBuilder::new(context, client).with_reporter(reporter)
        } else {
            SourceDistCachedBuilder::new(context, client)
        };

        let metadata = builder
            .download_and_build_metadata(&BuildableSource::Dist(source_dist))
            .await
            .context("Failed to build source distribution")?;

        // Return the requirements from the metadata.
        Ok(Some(RequestedRequirements::new(
            requirement.extras.clone(),
            metadata.requires_dist,
        )))
    }
}
