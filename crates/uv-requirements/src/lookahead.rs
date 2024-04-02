use std::collections::VecDeque;

use anyhow::{Context, Result};
use cache_key::CanonicalUrl;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use rustc_hash::FxHashSet;

use distribution_types::{Dist, DistributionMetadata, LocalEditable};
use pep508_rs::{MarkerEnvironment, Requirement, VersionOrUrl};
use pypi_types::Metadata23;
use uv_client::RegistryClient;
use uv_distribution::{DistributionDatabase, Reporter};
use uv_resolver::InMemoryIndex;
use uv_types::{BuildContext, Constraints, Overrides, RequestedRequirements};

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
    requirements: &'a [Requirement],
    /// The constraints for the project.
    constraints: &'a Constraints,
    /// The overrides for the project.
    overrides: &'a Overrides,
    /// The editable requirements for the project.
    editables: &'a [(LocalEditable, Metadata23)],
    /// The in-memory index for resolving dependencies.
    index: &'a InMemoryIndex,
    /// The database for fetching and building distributions.
    database: DistributionDatabase<'a, Context>,
}

impl<'a, Context: BuildContext + Send + Sync> LookaheadResolver<'a, Context> {
    /// Instantiate a new [`LookaheadResolver`] for a given set of requirements.
    pub fn new(
        requirements: &'a [Requirement],
        constraints: &'a Constraints,
        overrides: &'a Overrides,
        editables: &'a [(LocalEditable, Metadata23)],
        context: &'a Context,
        client: &'a RegistryClient,
        index: &'a InMemoryIndex,
    ) -> Self {
        Self {
            requirements,
            constraints,
            overrides,
            editables,
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
    pub async fn resolve(self, markers: &MarkerEnvironment) -> Result<Vec<RequestedRequirements>> {
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
    async fn lookahead(&self, requirement: Requirement) -> Result<Option<RequestedRequirements>> {
        // Determine whether the requirement represents a local distribution.
        let Some(VersionOrUrl::Url(url)) = requirement.version_or_url.as_ref() else {
            return Ok(None);
        };

        // Convert to a buildable distribution.
        let dist = Dist::from_url(requirement.name, url.clone())?;

        // Fetch the metadata for the distribution.
        let requires_dist = {
            let id = dist.package_id();
            if let Some(metadata) = self.index.get_metadata(&id) {
                // If the metadata is already in the index, return it.
                metadata.requires_dist.clone()
            } else {
                // Run the PEP 517 build process to extract metadata from the source distribution.
                let (metadata, precise) = self
                    .database
                    .get_or_build_wheel_metadata(&dist)
                    .await
                    .with_context(|| match &dist {
                        Dist::Built(built) => format!("Failed to download: {built}"),
                        Dist::Source(source) => format!("Failed to download and build: {source}"),
                    })?;

                let requires_dist = metadata.requires_dist.clone();

                // Insert the metadata into the index.
                self.index.insert_metadata(id, metadata);

                // Insert the redirect into the index.
                if let Some(precise) = precise {
                    self.index.insert_redirect(CanonicalUrl::new(url), precise);
                }

                requires_dist
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
