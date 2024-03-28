use std::collections::VecDeque;
use std::sync::Arc;

use anyhow::Result;
use futures::stream::FuturesUnordered;
use futures::StreamExt;

use distribution_types::{BuildableSource, Dist};
use pep508_rs::{MarkerEnvironment, Requirement, VersionOrUrl};
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
pub struct LookaheadResolver {
    /// The requirements for the project.
    requirements: Vec<Requirement>,
    /// The reporter to use when building source distributions.
    reporter: Option<Arc<dyn Reporter>>,
}

impl LookaheadResolver {
    /// Instantiate a new [`LookaheadResolver`] for a given set of requirements.
    pub fn new(requirements: Vec<Requirement>) -> Self {
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
        markers: &MarkerEnvironment,
        client: &RegistryClient,
    ) -> Result<Vec<RequestedRequirements>> {
        let mut queue = VecDeque::from(self.requirements.clone());
        let mut results = Vec::new();
        let mut futures = FuturesUnordered::new();

        while !queue.is_empty() || !futures.is_empty() {
            while let Some(requirement) = queue.pop_front() {
                futures.push(self.lookahead(requirement, context, client));
            }

            while let Some(result) = futures.next().await {
                if let Some(lookahead) = result? {
                    for requirement in lookahead.requirements() {
                        if requirement.evaluate_markers(markers, lookahead.extras()) {
                            queue.push_back(requirement.clone());
                        }
                    }
                    results.push(lookahead);
                }
            }
        }

        Ok(results)
    }

    /// Infer the package name for a given "unnamed" requirement.
    async fn lookahead<T: BuildContext>(
        &self,
        requirement: Requirement,
        context: &T,
        client: &RegistryClient,
    ) -> Result<Option<RequestedRequirements>> {
        // Determine whether the requirement represents a local distribution.
        let Some(VersionOrUrl::Url(url)) = requirement.version_or_url.as_ref() else {
            return Ok(None);
        };

        // Convert to a buildable distribution.
        let dist = Dist::from_url(requirement.name, url.clone())?;

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
            .await?;

        // Return the requirements from the metadata.
        Ok(Some(RequestedRequirements::new(
            requirement.extras,
            metadata.requires_dist,
        )))
    }
}
