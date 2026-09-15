use std::sync::Arc;

use futures::StreamExt;
use futures::stream::FuturesUnordered;
use tracing::trace;

use uv_configuration::{Constraints, Excludes, Overrides};
use uv_distribution::{DistributionDatabase, Metadata, Reporter};
use uv_distribution_types::{DependencyMetadata, Identifier, Requirement};
use uv_resolver::{
    InMemoryIndex, MetadataResponse, ResolverEnvironment, SourceDiscovery, SourceInput,
};
use uv_types::{BuildContext, HashStrategy, RequestedRequirements};

use crate::{Error, required_dist};

/// A resolver for resolving lookahead requirements from direct URLs.
///
/// The resolver extends certain privileges to "first-party" requirements. For example, first-party
/// requirements are allowed to contain direct URL references.
///
/// The lookahead resolver resolves requirements recursively for direct URLs, so that the resolver
/// can treat them as first-party dependencies for the purpose of analyzing their specifiers.
/// Namely, this enables transitive direct URL dependencies, since we can tell the resolver all of
/// the known URLs upfront.
///
/// This strategy relies on the assumption that direct URLs are only introduced by other direct
/// URLs, and not by PyPI dependencies. (If a direct URL _is_ introduced by a PyPI dependency, then
/// the resolver will (correctly) reject it later on with a conflict error.) A direct URL identifies
/// a specific version whose metadata can be inspected before solving. Conditional dependencies can
/// still exclude that package from the solution, so the inspected metadata is retained separately
/// from the selected dependency graph.
pub struct LookaheadResolver<'a, Context: BuildContext> {
    /// The direct requirements for the project.
    requirements: &'a [Requirement],
    /// The constraints for the project.
    constraints: &'a Constraints,
    /// The overrides for the project.
    overrides: &'a Overrides,
    /// The dependency exclusions for the project.
    excludes: &'a Excludes,
    /// The metadata explicitly provided by the user.
    dependency_metadata: &'a DependencyMetadata,
    /// The required hashes for the project.
    hasher: &'a HashStrategy,
    /// The in-memory index for resolving dependencies.
    index: &'a InMemoryIndex,
    /// The database for fetching and building distributions.
    database: DistributionDatabase<'a, Context>,
}

impl<'a, Context: BuildContext> LookaheadResolver<'a, Context> {
    /// Instantiate a new [`LookaheadResolver`] for a given set of requirements.
    pub fn new(
        requirements: &'a [Requirement],
        constraints: &'a Constraints,
        overrides: &'a Overrides,
        excludes: &'a Excludes,
        dependency_metadata: &'a DependencyMetadata,
        hasher: &'a HashStrategy,
        index: &'a InMemoryIndex,
        database: DistributionDatabase<'a, Context>,
    ) -> Self {
        Self {
            requirements,
            constraints,
            overrides,
            excludes,
            dependency_metadata,
            hasher,
            index,
            database,
        }
    }

    /// Set the [`Reporter`] to use for this resolver.
    #[must_use]
    pub fn with_reporter(self, reporter: Arc<dyn Reporter>) -> Self {
        Self {
            database: self.database.with_reporter(reporter),
            ..self
        }
    }

    /// Resolve the requirements from the provided source trees.
    ///
    /// When the environment is not given, this treats all marker expressions
    /// that reference the environment as true. In other words, it does
    /// environment independent expression evaluation. (Which in turn devolves
    /// to "only evaluate marker expressions that reference an extra name.")
    pub async fn resolve(
        self,
        env: &ResolverEnvironment,
    ) -> Result<(Vec<RequestedRequirements>, Vec<SourceInput>, HashStrategy), Error> {
        let mut results = Vec::new();
        let mut futures = FuturesUnordered::new();
        let mut discovery = SourceDiscovery::new(
            self.requirements,
            self.constraints,
            self.overrides,
            self.excludes,
            self.dependency_metadata,
            self.hasher,
            env,
        );

        loop {
            while let Some(requirement) = discovery.next_requirement() {
                futures.push(self.lookahead(requirement, discovery.hasher().clone()));
            }
            if futures.is_empty() {
                break;
            }
            while let Some(result) = futures.next().await {
                if let Some((requirement, metadata)) = result? {
                    results.push(discovery.visit(requirement, metadata)?);
                }
            }
        }

        let (inputs, hasher) = discovery.into_parts();
        Ok((results, inputs, hasher))
    }

    /// Load the metadata for a direct source requirement.
    async fn lookahead(
        &self,
        requirement: Requirement,
        hasher: HashStrategy,
    ) -> Result<Option<(Requirement, Metadata)>, Error> {
        trace!("Performing lookahead for {requirement}");

        // Determine whether the requirement represents a local distribution and convert to a
        // buildable distribution.
        let Some(dist) = required_dist(&requirement)? else {
            return Ok(None);
        };

        // Fetch the metadata for the distribution.
        let metadata = {
            let id = dist.distribution_id();
            if let Some(response) = self.index.distributions().register_or_wait(&id).await {
                let MetadataResponse::Found(archive) = &*response else {
                    panic!("Failed to find metadata for: {requirement}");
                };
                archive.metadata.clone()
            } else {
                // Run the PEP 517 build process to extract metadata from the source distribution.
                let archive = self
                    .database
                    .get_or_build_wheel_metadata(&dist, hasher.metadata_policy(&dist))
                    .await
                    .map_err(|err| Error::from_dist(dist, err))?;

                let metadata = archive.metadata.clone();

                // Insert the metadata into the index.
                self.index
                    .distributions()
                    .done(id, Arc::new(MetadataResponse::Found(archive)));

                metadata
            }
        };

        Ok(Some((requirement, metadata)))
    }
}
