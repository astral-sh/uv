use std::{collections::VecDeque, sync::Arc};

use futures::stream::FuturesUnordered;
use futures::StreamExt;
use rustc_hash::FxHashSet;
use tracing::trace;

use uv_configuration::{Constraints, Overrides};
use uv_distribution::{DistributionDatabase, Reporter};
use uv_distribution_types::{Dist, DistributionMetadata};
use uv_pypi_types::{Requirement, RequirementSource};
use uv_resolver::{InMemoryIndex, MetadataResponse, ResolverEnvironment};
use uv_types::{BuildContext, HashStrategy, RequestedRequirements};

use crate::{required_dist, Error};

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
/// the resolver will (correctly) reject it later on with a conflict error.) Further, it's only
/// possible because a direct URL points to a _specific_ version of a package, and so we know that
/// any correct resolution will _have_ to include it (unlike with PyPI dependencies, which may
/// require a range of versions and backtracking).
pub struct LookaheadResolver<'a, Context: BuildContext> {
    /// The direct requirements for the project.
    requirements: &'a [Requirement],
    /// The constraints for the project.
    constraints: &'a Constraints,
    /// The overrides for the project.
    overrides: &'a Overrides,
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
        hasher: &'a HashStrategy,
        index: &'a InMemoryIndex,
        database: DistributionDatabase<'a, Context>,
    ) -> Self {
        Self {
            requirements,
            constraints,
            overrides,
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
    ) -> Result<Vec<RequestedRequirements>, Error> {
        let mut results = Vec::new();
        let mut futures = FuturesUnordered::new();
        let mut seen = FxHashSet::default();

        // Queue up the initial requirements.
        let mut queue: VecDeque<_> = self
            .constraints
            .apply(self.overrides.apply(self.requirements))
            .filter(|requirement| requirement.evaluate_markers(env.marker_environment(), &[]))
            .map(|requirement| (*requirement).clone())
            .collect();

        while !queue.is_empty() || !futures.is_empty() {
            while let Some(requirement) = queue.pop_front() {
                if !matches!(requirement.source, RequirementSource::Registry { .. }) {
                    if seen.insert(requirement.clone()) {
                        futures.push(self.lookahead(requirement));
                    }
                }
            }

            while let Some(result) = futures.next().await {
                if let Some(lookahead) = result? {
                    for requirement in self
                        .constraints
                        .apply(self.overrides.apply(lookahead.requirements()))
                    {
                        if requirement
                            .evaluate_markers(env.marker_environment(), lookahead.extras())
                        {
                            queue.push_back((*requirement).clone());
                        }
                    }
                    results.push(lookahead);
                }
            }
        }

        Ok(results)
    }

    /// Infer the package name for a given "unnamed" requirement.
    async fn lookahead(
        &self,
        requirement: Requirement,
    ) -> Result<Option<RequestedRequirements>, Error> {
        trace!("Performing lookahead for {requirement}");

        // Determine whether the requirement represents a local distribution and convert to a
        // buildable distribution.
        let Some(dist) = required_dist(&requirement)? else {
            return Ok(None);
        };

        // Consider the dependencies to be "direct" if the requirement is a local source tree.
        let direct = if let Dist::Source(source_dist) = &dist {
            source_dist.as_path().is_some_and(std::path::Path::is_dir)
        } else {
            false
        };

        // Fetch the metadata for the distribution.
        let metadata = {
            let id = dist.version_id();
            if self.index.distributions().register(id.clone()) {
                // Run the PEP 517 build process to extract metadata from the source distribution.
                let archive = self
                    .database
                    .get_or_build_wheel_metadata(&dist, self.hasher.get(&dist))
                    .await
                    .map_err(|err| Error::from_dist(dist, err))?;

                let metadata = archive.metadata.clone();

                // Insert the metadata into the index.
                self.index
                    .distributions()
                    .done(id, Arc::new(MetadataResponse::Found(archive)));

                metadata
            } else {
                let response = self
                    .index
                    .distributions()
                    .wait(&id)
                    .await
                    .expect("missing value for registered task");
                let MetadataResponse::Found(archive) = &*response else {
                    panic!("Failed to find metadata for: {requirement}");
                };
                archive.metadata.clone()
            }
        };

        // Respect recursive extras by propagating the source extras to the dependencies.
        let requires_dist = metadata
            .requires_dist
            .into_iter()
            .chain(
                metadata
                    .dependency_groups
                    .into_iter()
                    .filter_map(|(group, dependencies)| {
                        if requirement.groups.contains(&group) {
                            Some(dependencies)
                        } else {
                            None
                        }
                    })
                    .flatten(),
            )
            .map(|dependency| {
                if dependency.name == requirement.name {
                    Requirement {
                        source: requirement.source.clone(),
                        ..dependency
                    }
                } else {
                    dependency
                }
            })
            .collect();

        // Return the requirements from the metadata.
        Ok(Some(RequestedRequirements::new(
            requirement.extras,
            requires_dist,
            direct,
        )))
    }
}
