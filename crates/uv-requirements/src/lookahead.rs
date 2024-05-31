use std::{collections::VecDeque, sync::Arc};

use futures::stream::FuturesUnordered;
use futures::StreamExt;
use rustc_hash::FxHashSet;
use thiserror::Error;
use tracing::trace;

use distribution_types::{BuiltDist, Dist, DistributionMetadata, GitSourceDist, SourceDist};
use pep508_rs::MarkerEnvironment;
use pypi_types::{Requirement, RequirementSource};
use uv_configuration::{Constraints, Overrides};
use uv_distribution::{DistributionDatabase, Reporter};
use uv_git::GitUrl;
use uv_resolver::{InMemoryIndex, MetadataResponse};
use uv_types::{BuildContext, HashStrategy, RequestedRequirements};

#[derive(Debug, Error)]
pub enum LookaheadError {
    #[error("Failed to download: `{0}`")]
    Download(BuiltDist, #[source] uv_distribution::Error),
    #[error("Failed to download and build: `{0}`")]
    DownloadAndBuild(SourceDist, #[source] uv_distribution::Error),
    #[error(transparent)]
    UnsupportedUrl(#[from] distribution_types::Error),
}

/// A resolver for resolving lookahead requirements from direct URLs.
///
/// The resolver extends certain privileges to "first-party" requirements. For example, first-party
/// requirements are allowed to contain direct URL references, local version specifiers, and more.
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
    #[allow(clippy::too_many_arguments)]
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
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
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
        markers: Option<&MarkerEnvironment>,
    ) -> Result<Vec<RequestedRequirements>, LookaheadError> {
        let mut results = Vec::new();
        let mut futures = FuturesUnordered::new();
        let mut seen = FxHashSet::default();

        // Queue up the initial requirements.
        let mut queue: VecDeque<_> = self
            .constraints
            .apply(self.overrides.apply(self.requirements))
            .filter(|requirement| requirement.evaluate_markers(markers, &[]))
            .cloned()
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
    async fn lookahead(
        &self,
        requirement: Requirement,
    ) -> Result<Option<RequestedRequirements>, LookaheadError> {
        trace!("Performing lookahead for {requirement}");
        // Determine whether the requirement represents a local distribution and convert to a
        // buildable distribution.
        let dist = match requirement.source {
            RequirementSource::Registry { .. } => return Ok(None),
            RequirementSource::Url {
                subdirectory,
                location,
                url,
            } => Dist::from_http_url(requirement.name, url, location, subdirectory)?,
            RequirementSource::Git {
                repository,
                reference,
                precise,
                subdirectory,
                url,
            } => {
                let mut git_url = GitUrl::new(repository, reference);
                if let Some(precise) = precise {
                    git_url = git_url.with_precise(precise);
                }
                Dist::Source(SourceDist::Git(GitSourceDist {
                    name: requirement.name,
                    git: Box::new(git_url),
                    subdirectory,
                    url,
                }))
            }
            RequirementSource::Path {
                path,
                url,
                editable,
            } => Dist::from_file_url(requirement.name, url, &path, editable)?,
        };

        // Fetch the metadata for the distribution.
        let requires_dist = {
            let id = dist.version_id();
            if let Some(archive) =
                self.index
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
                archive
                    .metadata
                    .requires_dist
                    .iter()
                    .cloned()
                    .map(Requirement::from)
                    .collect()
            } else {
                // Run the PEP 517 build process to extract metadata from the source distribution.
                let archive = self
                    .database
                    .get_or_build_wheel_metadata(&dist, self.hasher.get(&dist))
                    .await
                    .map_err(|err| match &dist {
                        Dist::Built(built) => LookaheadError::Download(built.clone(), err),
                        Dist::Source(source) => {
                            LookaheadError::DownloadAndBuild(source.clone(), err)
                        }
                    })?;

                let requires_dist = archive.metadata.requires_dist.clone();

                // Insert the metadata into the index.
                self.index
                    .distributions()
                    .done(id, Arc::new(MetadataResponse::Found(archive)));

                requires_dist.into_iter().map(Requirement::from).collect()
            }
        };

        // Consider the dependencies to be "direct" if the requirement is a local source tree.
        let direct = if let Dist::Source(source_dist) = &dist {
            source_dist.as_path().is_some_and(std::path::Path::is_dir)
        } else {
            false
        };

        // Return the requirements from the metadata.
        Ok(Some(RequestedRequirements::new(
            requirement.extras,
            requires_dist,
            direct,
        )))
    }
}
