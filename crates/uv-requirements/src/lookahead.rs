use std::{collections::VecDeque, sync::Arc};

use futures::StreamExt;
use futures::stream::FuturesUnordered;
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::trace;

use uv_configuration::{Constraints, Excludes, Overrides};
use uv_distribution::{DistributionDatabase, Reporter};
use uv_distribution_types::{Dist, Identifier, Requirement, RequirementSource};
use uv_normalize::{GroupName, PackageName};
use uv_pep508::{ExtraOperator, MarkerExpression, MarkerTree, MarkerValueExtra};
use uv_pypi_types::{ConflictItem, ConflictKind, Conflicts};
use uv_resolver::{InMemoryIndex, MetadataResponse, PythonRequirement, ResolverEnvironment};
use uv_types::{BuildContext, HashStrategy, RequestedRequirements};

use crate::{Error, required_dist};

/// A requirement and the project scope in which it was discovered.
///
/// Dependency extras are local to their declaring package, so project extras and groups must be
/// tracked separately from the environment markers inherited through transitive dependencies.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ScopedRequirement {
    requirement: Requirement,
    scope: Option<ExtraScope>,
    environment: MarkerTree,
}

/// The package whose extras or dependency groups select the source.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ExtraScope {
    package: PackageName,
    marker: MarkerTree,
    groups: Box<[GroupName]>,
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

    /// Resolve the requirements from the provided source trees.
    ///
    /// When the environment is not given, this treats all marker expressions
    /// that reference the environment as true. In other words, it does
    /// environment independent expression evaluation. (Which in turn devolves
    /// to "only evaluate marker expressions that reference an extra name.")
    pub async fn resolve(
        self,
        env: &ResolverEnvironment,
        python_requirement: &PythonRequirement,
        conflicts: &Conflicts,
    ) -> Result<(Vec<RequestedRequirements>, HashStrategy), Error> {
        let mut results = Vec::new();
        let mut futures = FuturesUnordered::new();
        let mut seen = FxHashSet::default();
        let mut hasher = self.hasher.clone();

        // Queue up the initial requirements.
        let mut queue: VecDeque<_> = self
            .constraints
            .apply(self.overrides.apply(self.requirements))
            .filter(|requirement| !self.excludes.contains(&requirement.name))
            .filter(|requirement| requirement.evaluate_markers(env.marker_environment(), &[]))
            .flat_map(|requirement| {
                let requirement = (*requirement).clone();
                let environment = requirement.marker.without_extras();

                if requirement.groups.len() <= 1 {
                    return vec![ScopedRequirement {
                        requirement,
                        scope: None,
                        environment,
                    }];
                }

                requirement
                    .groups
                    .iter()
                    .cloned()
                    .map(|group| ScopedRequirement {
                        requirement: Requirement {
                            groups: Box::new([group]),
                            ..requirement.clone()
                        },
                        scope: None,
                        environment,
                    })
                    .collect()
            })
            .collect();

        // Track every direct source for each package, including sources that apply in disjoint
        // marker environments. Registry-form requirements can then activate extras on each
        // compatible source without depending on their discovery order.
        let mut direct: FxHashMap<_, Vec<_>> = FxHashMap::default();
        for scoped_requirement in &queue {
            let requirement = &scoped_requirement.requirement;
            if !matches!(requirement.source, RequirementSource::Registry { .. }) {
                let direct_requirements = direct.entry(requirement.name.clone()).or_default();
                if !direct_requirements.contains(scoped_requirement) {
                    direct_requirements.push(scoped_requirement.clone());
                }
            }
        }
        let mut pending: FxHashMap<_, FxHashSet<_>> = FxHashMap::default();

        while !queue.is_empty() || !futures.is_empty() {
            while let Some(scoped_requirement) = queue.pop_front() {
                let requirement = &scoped_requirement.requirement;
                if !matches!(requirement.source, RequirementSource::Registry { .. }) {
                    let direct_requirements = direct.entry(requirement.name.clone()).or_default();
                    if !direct_requirements.contains(&scoped_requirement) {
                        direct_requirements.push(scoped_requirement.clone());
                    }

                    if seen.insert(scoped_requirement.clone()) {
                        futures.push(self.lookahead(scoped_requirement.clone(), hasher.clone()));
                    }

                    if let Some(pending_requirements) = pending.get(&requirement.name) {
                        for pending_requirement in pending_requirements {
                            let Some(candidate) = Self::replay_requirement(
                                pending_requirement,
                                &scoped_requirement,
                                env,
                                python_requirement,
                                conflicts,
                            ) else {
                                continue;
                            };
                            if seen.insert(candidate.clone()) {
                                futures.push(self.lookahead(candidate, hasher.clone()));
                            }
                        }
                    }
                } else {
                    let pending_requirements = pending.entry(requirement.name.clone()).or_default();
                    if !pending_requirements.insert(scoped_requirement.clone()) {
                        continue;
                    }

                    if let Some(direct_requirements) = direct.get(&requirement.name) {
                        for direct_requirement in direct_requirements {
                            let Some(candidate) = Self::replay_requirement(
                                &scoped_requirement,
                                direct_requirement,
                                env,
                                python_requirement,
                                conflicts,
                            ) else {
                                continue;
                            };
                            if seen.insert(candidate.clone()) {
                                futures.push(self.lookahead(candidate, hasher.clone()));
                            }
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
                            )
                        }),
                    )?;
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
                            queue.push_back(ScopedRequirement {
                                requirement: (*requirement).clone(),
                                scope: Some(scope.clone().unwrap_or_else(|| ExtraScope {
                                    package: lookahead.package().clone(),
                                    marker: requirement.marker.only_extras(),
                                    groups: Box::default(),
                                })),
                                environment: environment.and(requirement.marker.without_extras()),
                            });
                        }
                    }
                    results.push(lookahead);
                }
            }
        }

        Ok((results, hasher))
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
        conflicts: &Conflicts,
    ) -> Option<ScopedRequirement> {
        let requirement = &scoped_requirement.requirement;
        let direct_requirement = &scoped_direct_requirement.requirement;

        let scope = match (&scoped_requirement.scope, &scoped_direct_requirement.scope) {
            (Some(scope), Some(direct_scope)) if scope.package == direct_scope.package => {
                let mut marker = scope.marker.and(direct_scope.marker);

                for conflict_set in conflicts.iter() {
                    for (index, left) in conflict_set.iter().enumerate() {
                        if left.package() != &scope.package {
                            continue;
                        }
                        let Some(left_marker) = Self::conflict_marker(left, scope, direct_scope)
                        else {
                            continue;
                        };

                        for right in conflict_set.iter().skip(index + 1) {
                            if right.package() != &scope.package {
                                continue;
                            }
                            let Some(right_marker) =
                                Self::conflict_marker(right, scope, direct_scope)
                            else {
                                continue;
                            };

                            let compatible = left_marker.negate().or(right_marker.negate());
                            marker = marker.and(compatible);
                        }
                    }
                }

                if marker.is_false() {
                    return None;
                }

                let mut groups = scope
                    .groups
                    .iter()
                    .chain(direct_scope.groups.iter())
                    .cloned()
                    .collect::<Vec<_>>();
                groups.sort_unstable();
                groups.dedup();

                Some(ExtraScope {
                    package: scope.package.clone(),
                    marker,
                    groups: groups.into_boxed_slice(),
                })
            }
            (Some(scope), _) | (None, Some(scope)) => Some(scope.clone()),
            (None, None) => None,
        };

        let marker = scoped_requirement
            .environment
            .and(scoped_direct_requirement.environment)
            .and(requirement.marker)
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

    /// Returns the marker indicating whether a conflicting project extra or group is active.
    fn conflict_marker(
        item: &ConflictItem,
        scope: &ExtraScope,
        direct_scope: &ExtraScope,
    ) -> Option<MarkerTree> {
        match item.kind() {
            ConflictKind::Extra(extra) => Some(MarkerTree::expression(MarkerExpression::Extra {
                operator: ExtraOperator::Equal,
                name: MarkerValueExtra::Extra(extra.clone()),
            })),
            ConflictKind::Group(group) => Some(
                if scope.groups.contains(group) || direct_scope.groups.contains(group) {
                    MarkerTree::TRUE
                } else {
                    MarkerTree::FALSE
                },
            ),
            ConflictKind::Project => None,
        }
    }

    /// Infer the package name for a given "unnamed" requirement.
    async fn lookahead(
        &self,
        scoped_requirement: ScopedRequirement,
        hasher: HashStrategy,
    ) -> Result<Option<(RequestedRequirements, Option<ExtraScope>, MarkerTree)>, Error> {
        let ScopedRequirement {
            requirement,
            scope,
            environment,
        } = scoped_requirement;
        trace!("Performing lookahead for {requirement}");

        let scope = scope.or_else(|| {
            requirement.groups.first().map(|group| ExtraScope {
                package: requirement.name.clone(),
                marker: MarkerTree::TRUE,
                groups: Box::new([group.clone()]),
            })
        });

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
        Ok(Some((
            RequestedRequirements::new(package, version, requirement.extras, requires_dist, direct),
            scope,
            environment,
        )))
    }
}
