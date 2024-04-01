use std::collections::VecDeque;
use std::sync::Arc;

use anyhow::{Context, Result};
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use rustc_hash::FxHashSet;

use distribution_types::{Dist, LocalEditable};
use pep508_rs::{MarkerEnvironment, Requirement, VersionOrUrl};
use pypi_types::Metadata23;
use uv_client::RegistryClient;
use uv_distribution::{DistributionDatabase, Reporter};
use uv_types::{BuildContext, Constraints, Overrides, RequestedRequirements};

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
    /// The direct requirements for the project.
    requirements: &'a [Requirement],
    /// The constraints for the project.
    constraints: &'a Constraints,
    /// The overrides for the project.
    overrides: &'a Overrides,
    /// The editable requirements for the project.
    editables: &'a [(LocalEditable, Metadata23)],
    /// The reporter to use when building source distributions.
    reporter: Option<Arc<dyn Reporter>>,
}

impl<'a> LookaheadResolver<'a> {
    /// Instantiate a new [`LookaheadResolver`] for a given set of requirements.
    pub fn new(
        requirements: &'a [Requirement],
        constraints: &'a Constraints,
        overrides: &'a Overrides,
        editables: &'a [(LocalEditable, Metadata23)],
    ) -> Self {
        Self {
            requirements,
            constraints,
            overrides,
            editables,
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
    pub async fn resolve<T: BuildContext + Send + Sync>(
        self,
        context: &T,
        markers: &MarkerEnvironment,
        client: &RegistryClient,
    ) -> Result<Vec<RequestedRequirements>> {
        let mut results = Vec::new();
        let mut futures = FuturesUnordered::new();
        let mut seen = FxHashSet::default();

        // Queue up the initial requirements.
        let mut queue: VecDeque<Requirement> = self
            .constraints
            .apply(self.overrides.apply(self.requirements))
            .filter(|requirement| requirement.evaluate_markers(markers, &[]))
            .chain(self.editables.iter().flat_map(|(editable, metadata)| {
                self.constraints
                    .apply(self.overrides.apply(&metadata.requires_dist))
                    .filter(|requirement| requirement.evaluate_markers(markers, &editable.extras))
            }))
            .cloned()
            .collect();

        while !queue.is_empty() || !futures.is_empty() {
            while let Some(requirement) = queue.pop_front() {
                // Ignore duplicates. If we have conflicting URLs, we'll catch that later.
                if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                    if seen.insert(requirement.name.clone()) {
                        futures.push(self.lookahead(requirement, context, client));
                    }
                }
            }

            while let Some(result) = futures.next().await {
                if let Some(lookahead) = result? {
                    for requirement in self
                        .constraints
                        .apply(self.overrides.apply(lookahead.requirements()))
                    {
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
    async fn lookahead<T: BuildContext + Send + Sync>(
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

        // Consider the dependencies to be "direct" if the requirement is a local source tree.
        let direct = if let Dist::Source(source_dist) = &dist {
            source_dist.as_path().is_some_and(std::path::Path::is_dir)
        } else {
            false
        };

        // Run the PEP 517 build process to extract metadata from the source distribution.
        let database = if let Some(reporter) = self.reporter.clone() {
            DistributionDatabase::new(client, context).with_reporter(reporter)
        } else {
            DistributionDatabase::new(client, context)
        };

        let (metadata, _precise) = database
            .get_or_build_wheel_metadata(&dist)
            .await
            .with_context(|| match dist {
                Dist::Built(built) => format!("Failed to download: {built}"),
                Dist::Source(source) => format!("Failed to download and build: {source}"),
            })?;

        // Return the requirements from the metadata.
        Ok(Some(RequestedRequirements::new(
            requirement.extras,
            metadata.requires_dist,
            direct,
        )))
    }
}
