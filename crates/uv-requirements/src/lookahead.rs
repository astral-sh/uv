use std::collections::VecDeque;
use std::ops::Deref;

use futures::stream::FuturesUnordered;
use futures::StreamExt;
use rustc_hash::FxHashSet;
use thiserror::Error;
use url::Url;

use distribution_types::{
    BuiltDist, Dist, DistributionMetadata, LocalEditable, SourceDist, UvRequirement,
    UvRequirements, UvSource,
};
use pep508_rs::{MarkerEnvironment, VerbatimUrl};
use pypi_types::Metadata23;
use uv_client::RegistryClient;
use uv_configuration::{Constraints, Overrides};
use uv_distribution::{DistributionDatabase, Reporter};
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
    #[error(transparent)]
    InvalidRequirement(#[from] distribution_types::ParsedUrlError),
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
pub struct LookaheadResolver<'a, Context: BuildContext + Send + Sync> {
    /// The direct requirements for the project.
    requirements: &'a [UvRequirement],
    /// The constraints for the project.
    constraints: &'a Constraints,
    /// The overrides for the project.
    overrides: &'a Overrides,
    /// The editable requirements for the project.
    editables: &'a [(LocalEditable, Metadata23, UvRequirements)],
    /// The required hashes for the project.
    hasher: &'a HashStrategy,
    /// The in-memory index for resolving dependencies.
    index: &'a InMemoryIndex,
    /// The database for fetching and building distributions.
    database: DistributionDatabase<'a, Context>,
}

impl<'a, Context: BuildContext + Send + Sync> LookaheadResolver<'a, Context> {
    /// Instantiate a new [`LookaheadResolver`] for a given set of requirements.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        requirements: &'a [UvRequirement],
        constraints: &'a Constraints,
        overrides: &'a Overrides,
        editables: &'a [(LocalEditable, Metadata23, UvRequirements)],
        hasher: &'a HashStrategy,
        context: &'a Context,
        client: &'a RegistryClient,
        index: &'a InMemoryIndex,
    ) -> Self {
        Self {
            requirements,
            constraints,
            overrides,
            editables,
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
    pub async fn resolve(
        self,
        markers: &MarkerEnvironment,
    ) -> Result<Vec<RequestedRequirements>, LookaheadError> {
        let mut results = Vec::new();
        let mut futures = FuturesUnordered::new();
        let mut seen = FxHashSet::default();

        // Queue up the initial requirements.
        let mut queue: VecDeque<_> = self
            .constraints
            .apply(self.overrides.apply(self.requirements))
            .filter(|requirement| requirement.evaluate_markers(markers, &[]))
            .chain(
                self.editables
                    .iter()
                    .flat_map(|(editable, _metadata, requirements)| {
                        self.constraints
                            .apply(self.overrides.apply(&requirements.dependencies))
                            .filter(|requirement| {
                                requirement.evaluate_markers(markers, &editable.extras)
                            })
                    }),
            )
            .cloned()
            .collect();

        while !queue.is_empty() || !futures.is_empty() {
            while let Some(requirement) = queue.pop_front() {
                // TODO(konsti): Git and path too
                if !matches!(requirement.source, UvSource::Registry { .. }) {
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
        requirement: UvRequirement,
    ) -> Result<Option<RequestedRequirements>, LookaheadError> {
        // Determine whether the requirement represents a local distribution and convert to a
        // buildable distribution.
        let dist = match requirement.source {
            UvSource::Registry { .. } => return Ok(None),
            UvSource::Url { url, subdirectory } => {
                let mut merged_url: Url = url.deref().clone();
                if let Some(subdirectory) = subdirectory {
                    merged_url
                        .set_fragment(Some(&format!("subdirectory={}", subdirectory.display())));
                }
                let mut merged_url = VerbatimUrl::from_url(merged_url);
                if let Some(given) = url.given() {
                    merged_url = merged_url.with_given(given);
                }
                Dist::from_https_url(requirement.name, merged_url)?
            }
            UvSource::Git { url, .. } => Dist::from_git_url(requirement.name, url)?,
            UvSource::Path {
                path: _,
                url,
                editable: _,
            } => Dist::from_file_url(requirement.name, url, false)?,
        };

        // Fetch the metadata for the distribution.
        let requires_dist = {
            let id = dist.version_id();
            if let Some(archive) = self
                .index
                .get_metadata(&id)
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
                    .map(UvRequirement::from_requirement)
                    .collect::<Result<_, _>>()?
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
                    .insert_metadata(id, MetadataResponse::Found(archive));

                requires_dist
                    .into_iter()
                    .map(UvRequirement::from_requirement)
                    .collect::<Result<_, _>>()?
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
