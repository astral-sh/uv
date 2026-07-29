use std::collections::VecDeque;
use std::collections::hash_map::Entry;
use std::sync::Arc;

use futures::StreamExt;
use futures::stream::FuturesUnordered;
use rustc_hash::FxHashMap;
use tracing::trace;

use uv_configuration::{Constraints, Excludes, Overrides};
use uv_distribution::{DistributionDatabase, Reporter};
use uv_distribution_types::{Dist, Identifier, Requirement, RequirementSource};
use uv_normalize::ExtraName;
use uv_pep508::MarkerTree;
use uv_pypi_types::{ConflictItem, ConflictKindRef, Conflicts};
use uv_resolver::{
    InMemoryIndex, MetadataResponse, PythonRequirement, ResolverEnvironment, UniversalMarker,
};
use uv_types::{BuildContext, HashStrategy, RequestedRequirements};

use crate::{Error, required_dist};

/// A requirement and the project scope in which it was discovered.
///
/// Dependency extras are local to their declaring package, so project extras and groups must be
/// tracked separately from the environment markers inherited through transitive dependencies.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ScopedRequirement {
    /// The dependency being activated, including its package-local extra marker.
    requirement: Requirement,
    /// The package-qualified project, extra, and group predicates inherited from its ancestors.
    scope: MarkerTree,
    /// The platform and Python-version conditions inherited from its ancestors.
    environment: MarkerTree,
}

/// A source activation independent of the environments that can reach it.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ScopedRequirementKey {
    /// The source and dependency with its marker normalized away.
    requirement: Requirement,
    /// Conflict scopes must remain distinct even when the source is otherwise identical.
    scope: MarkerTree,
}

impl ScopedRequirement {
    /// Return a marker-independent source key while preserving its conflict scope.
    fn key(&self) -> ScopedRequirementKey {
        ScopedRequirementKey {
            requirement: Requirement {
                marker: MarkerTree::TRUE,
                ..self.requirement.clone()
            },
            scope: self.scope,
        }
    }

    /// Merge another activation of the same source and report whether its marker coverage grew.
    fn merge(&mut self, other: &Self) -> bool {
        let environment = self.environment.or(other.environment);
        let marker = self.requirement.marker.or(other.requirement.marker);
        if environment == self.environment && marker == self.requirement.marker {
            return false;
        }

        self.environment = environment;
        self.requirement.marker = marker;
        true
    }
}

/// A FIFO work queue with indexed coalescing for equivalent source activations.
#[derive(Default)]
struct ScopedRequirementQueue {
    /// Preserve discovery order without scanning queued requirements on every insertion.
    order: VecDeque<ScopedRequirementKey>,
    /// Keep the activation associated with each queued source key.
    requirements: FxHashMap<ScopedRequirementKey, ScopedRequirement>,
}

impl ScopedRequirementQueue {
    /// Schedule a requirement, merging environments when its source is already queued.
    fn push(&mut self, requirement: ScopedRequirement) {
        match self.requirements.entry(requirement.key()) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().merge(&requirement);
            }
            Entry::Vacant(entry) => {
                self.order.push_back(entry.key().clone());
                entry.insert(requirement);
            }
        }
    }

    /// Return the next queued source in insertion order.
    fn pop(&mut self) -> Option<ScopedRequirement> {
        let key = self.order.pop_front()?;
        self.requirements.remove(&key)
    }

    /// Return whether the activation queue has been drained.
    fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

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
    /// The dependency exclusions for the project.
    excludes: &'a Excludes,
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
        hasher: &'a HashStrategy,
        index: &'a InMemoryIndex,
        database: DistributionDatabase<'a, Context>,
    ) -> Self {
        Self {
            requirements,
            constraints,
            overrides,
            excludes,
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

    /// Resolve direct sources and every transitively activated source before dependency resolution.
    ///
    /// Registry-form requirements can request extras on an already-known direct source. Replay
    /// those requirements only in compatible Python, platform, and conflict scopes so their URLs,
    /// hashes, and candidate-selection requirements are available before the main resolver starts.
    pub async fn resolve(
        self,
        env: &ResolverEnvironment,
        python_requirement: &PythonRequirement,
        conflicts: &Conflicts,
    ) -> Result<(Vec<RequestedRequirements>, HashStrategy), Error> {
        let mut results = Vec::new();
        let mut futures = FuturesUnordered::new();
        let mut seen: FxHashMap<ScopedRequirementKey, MarkerTree> = FxHashMap::default();
        let mut hasher = self.hasher.clone();
        let conflict_markers = UniversalMarker::from_conflicts(conflicts).combined();

        // Queue up the initial requirements.
        let mut queue = ScopedRequirementQueue::default();
        for requirement in self
            .constraints
            .apply(self.overrides.apply(self.requirements))
            .filter(|requirement| !self.excludes.contains(&requirement.name))
            .filter(|requirement| requirement.evaluate_markers(env.marker_environment(), &[]))
            .filter(|requirement| env.supports_marker(requirement.marker, python_requirement))
        {
            let requirement = (*requirement).clone();
            let environment = requirement.marker.without_extras();

            if requirement.groups.len() <= 1 {
                queue.push(ScopedRequirement {
                    scope: Self::activation_scope(&requirement, conflicts),
                    requirement,
                    environment,
                });
                continue;
            }

            for group in &requirement.groups {
                let requirement = Requirement {
                    groups: Box::new([group.clone()]),
                    ..requirement.clone()
                };
                queue.push(ScopedRequirement {
                    scope: Self::activation_scope(&requirement, conflicts),
                    requirement,
                    environment,
                });
            }
        }

        // Track direct sources and registry requirements separately so either can be discovered
        // first and still replay against all compatible activations of the other.
        let mut direct: FxHashMap<_, Vec<_>> = FxHashMap::default();
        let mut pending: FxHashMap<_, Vec<_>> = FxHashMap::default();

        while !queue.is_empty() || !futures.is_empty() {
            while let Some(scoped_requirement) = queue.pop() {
                let requirement = &scoped_requirement.requirement;
                if !matches!(requirement.source, RequirementSource::Registry { .. }) {
                    let direct_requirements = direct.entry(requirement.name.clone()).or_default();
                    Self::remember(direct_requirements, scoped_requirement.clone());

                    if !Self::visit(&mut seen, &scoped_requirement) {
                        continue;
                    }

                    futures.push(self.lookahead(
                        scoped_requirement.clone(),
                        hasher.clone(),
                        conflicts,
                    ));

                    if let Some(pending_requirements) = pending.get(&requirement.name) {
                        for pending_requirement in pending_requirements {
                            let Some(candidate) = Self::replay_requirement(
                                pending_requirement,
                                &scoped_requirement,
                                env,
                                python_requirement,
                                conflict_markers,
                            ) else {
                                continue;
                            };
                            queue.push(candidate);
                        }
                    }
                } else {
                    let pending_requirements = pending.entry(requirement.name.clone()).or_default();
                    let Some(scoped_requirement) =
                        Self::remember(pending_requirements, scoped_requirement.clone())
                    else {
                        continue;
                    };

                    if let Some(direct_requirements) = direct.get(&requirement.name) {
                        for direct_requirement in direct_requirements {
                            let Some(candidate) = Self::replay_requirement(
                                &scoped_requirement,
                                direct_requirement,
                                env,
                                python_requirement,
                                conflict_markers,
                            ) else {
                                continue;
                            };
                            queue.push(candidate);
                        }
                    }
                }
            }

            while let Some(result) = futures.next().await {
                if let Some((lookahead, scope, environment)) = result? {
                    hasher = hasher.augment_with_requirements(
                        lookahead.requirements().iter().filter(|requirement| {
                            !self.excludes.contains_for(
                                lookahead.package(),
                                lookahead.version(),
                                &requirement.name,
                            ) && requirement
                                .evaluate_markers(env.marker_environment(), lookahead.extras())
                                && env.supports_marker(
                                    environment.and(Self::selected_marker(
                                        requirement.marker,
                                        lookahead.extras(),
                                    )),
                                    python_requirement,
                                )
                        }),
                    )?;
                    if !env.supports_marker(environment, python_requirement) {
                        results.push(lookahead);
                        continue;
                    }

                    for requirement in self.constraints.apply(self.overrides.apply_for(
                        lookahead.package(),
                        lookahead.version(),
                        lookahead.requirements(),
                    )) {
                        if !self.excludes.contains_for(
                            lookahead.package(),
                            lookahead.version(),
                            &requirement.name,
                        ) && requirement
                            .evaluate_markers(env.marker_environment(), lookahead.extras())
                        {
                            let local_environment =
                                Self::selected_marker(requirement.marker, lookahead.extras());
                            if !env.supports_marker(local_environment, python_requirement) {
                                continue;
                            }
                            let environment = environment.and(local_environment);

                            let extra_scope = requirement
                                .marker
                                .simplify_not_extras_with(|extra| {
                                    !lookahead.extras().contains(extra)
                                })
                                .only_extras();
                            let scope = scope.and(
                                UniversalMarker::from_package_extras(
                                    lookahead.package(),
                                    extra_scope,
                                    conflicts,
                                )
                                .combined(),
                            );
                            if scope.and(conflict_markers).is_false() {
                                continue;
                            }

                            queue.push(ScopedRequirement {
                                requirement: (*requirement).clone(),
                                scope,
                                environment,
                            });
                        }
                    }
                    results.push(lookahead);
                }
            }
        }

        Ok((results, hasher))
    }

    /// Merge equivalent source records, returning only newly expanded activation contexts.
    fn remember(
        requirements: &mut Vec<ScopedRequirement>,
        requirement: ScopedRequirement,
    ) -> Option<ScopedRequirement> {
        let key = requirement.key();
        if let Some(previous) = requirements
            .iter_mut()
            .find(|previous| previous.key() == key)
        {
            previous.merge(&requirement).then(|| previous.clone())
        } else {
            requirements.push(requirement.clone());
            Some(requirement)
        }
    }

    /// Return whether a source is reachable in any environment not traversed previously.
    ///
    /// An impossible environment is still visited once to register its immediate nested URLs;
    /// later visits must add a genuinely reachable environment.
    fn visit(
        seen: &mut FxHashMap<ScopedRequirementKey, MarkerTree>,
        requirement: &ScopedRequirement,
    ) -> bool {
        match seen.entry(requirement.key()) {
            Entry::Occupied(mut entry) => {
                if requirement.environment.and(entry.get().negate()).is_false() {
                    return false;
                }

                *entry.get_mut() = entry.get().or(requirement.environment);
                true
            }
            Entry::Vacant(entry) => {
                entry.insert(requirement.environment);
                true
            }
        }
    }

    /// Evaluate package-local extras before projecting a marker onto its supported environments.
    ///
    /// This keeps the selected branch of markers that combine extras with platform conditions.
    fn selected_marker(marker: MarkerTree, extras: &[ExtraName]) -> MarkerTree {
        marker
            .simplify_extras(extras)
            .simplify_not_extras_with(|extra| !extras.contains(extra))
    }

    /// Replays a registry requirement against a compatible direct source.
    ///
    /// Source markers and dependency markers originate from different packages, so compare their
    /// project-extra scopes separately before intersecting their environment markers.
    fn replay_requirement(
        scoped_requirement: &ScopedRequirement,
        scoped_direct_requirement: &ScopedRequirement,
        env: &ResolverEnvironment,
        python_requirement: &PythonRequirement,
        conflicts: MarkerTree,
    ) -> Option<ScopedRequirement> {
        let requirement = &scoped_requirement.requirement;
        let direct_requirement = &scoped_direct_requirement.requirement;

        let scope = scoped_requirement
            .scope
            .and(scoped_direct_requirement.scope);
        if scope.and(conflicts).is_false() {
            return None;
        }

        let marker = scoped_requirement
            .environment
            .and(scoped_direct_requirement.environment)
            .and(requirement.marker.without_extras())
            .and(direct_requirement.marker.without_extras());

        if !env.supports_marker(marker, python_requirement) {
            return None;
        }

        Some(ScopedRequirement {
            requirement: Requirement {
                source: direct_requirement.source.clone(),
                marker,
                ..requirement.clone()
            },
            scope,
            environment: marker.without_extras(),
        })
    }

    /// Return the package-qualified project or dependency-group conflicts activated by a source.
    fn activation_scope(requirement: &Requirement, conflicts: &Conflicts) -> MarkerTree {
        if requirement.groups.is_empty() {
            if !conflicts.contains(&requirement.name, ConflictKindRef::Project) {
                return MarkerTree::TRUE;
            }

            return UniversalMarker::from_conflict_item(&ConflictItem::from(
                requirement.name.clone(),
            ))
            .combined();
        }

        requirement
            .groups
            .iter()
            .filter(|group| conflicts.contains(&requirement.name, *group))
            .fold(MarkerTree::TRUE, |scope, group| {
                scope.and(
                    UniversalMarker::from_conflict_item(&ConflictItem::from((
                        requirement.name.clone(),
                        group.clone(),
                    )))
                    .combined(),
                )
            })
    }

    /// Fetch a direct source's metadata while retaining the scope in which it was activated.
    async fn lookahead(
        &self,
        scoped_requirement: ScopedRequirement,
        hasher: HashStrategy,
        conflicts: &Conflicts,
    ) -> Result<Option<(RequestedRequirements, MarkerTree, MarkerTree)>, Error> {
        let ScopedRequirement {
            requirement,
            scope,
            environment,
        } = scoped_requirement;
        trace!("Performing lookahead for {requirement}");

        let scope = scope.and(Self::activation_scope(&requirement, conflicts));

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
                    .get_or_build_wheel_metadata(&dist, hasher.get(&dist))
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

        // Respect recursive extras by propagating the source extras to the dependencies.
        let package = metadata.name.clone();
        let version = metadata.version.clone();
        let requires_dist = Box::into_iter(metadata.requires_dist)
            // Dependency groups are independent of the project's production dependencies.
            .filter(|_| requirement.groups.is_empty())
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
        let activation = Requirement {
            marker: scope.and(environment),
            ..requirement.clone()
        };

        Ok(Some((
            RequestedRequirements::new(
                package,
                version,
                requirement.extras,
                requires_dist,
                activation,
                direct,
            ),
            scope,
            environment,
        )))
    }
}
