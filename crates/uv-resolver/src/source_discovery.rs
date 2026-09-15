use std::collections::VecDeque;

use rustc_hash::FxHashSet;

use uv_configuration::{Constraints, Excludes, Overrides};
use uv_distribution::Metadata;
use uv_distribution_types::{DependencyMetadata, Requirement, RequirementSource};
use uv_types::{HashStrategy, HashStrategyError, HashVerification, RequestedRequirements};

use crate::ResolverEnvironment;

/// A source declaration and the metadata inspected while discovering its dependencies.
///
/// Source discovery can inspect packages that are absent from the resolved dependency graph.
/// Their metadata remains an input to resolution because it can declare sources for other packages.
#[derive(Debug, Clone)]
pub struct SourceInput {
    /// The source and extras or groups whose dependencies were inspected.
    pub requirement: Requirement,
    /// The complete metadata, before selecting groups or rewriting recursive extras.
    pub metadata: Metadata,
}

/// Traverse first-party requirements independently of how their metadata is loaded.
///
/// Resolution can fetch or build metadata, while lock validation can refresh local metadata and
/// reuse recorded remote metadata. Both apply the same constraints, overrides, exclusions, and
/// extra selection when discovering additional sources.
pub struct SourceDiscovery<'a> {
    constraints: &'a Constraints,
    overrides: &'a Overrides,
    excludes: &'a Excludes,
    dependency_metadata: &'a DependencyMetadata,
    env: &'a ResolverEnvironment,
    hasher: HashStrategy,
    queue: VecDeque<Requirement>,
    seen: FxHashSet<Requirement>,
    inputs: Vec<SourceInput>,
}

impl<'a> SourceDiscovery<'a> {
    /// Start discovery from the project's direct requirements.
    pub fn new(
        requirements: &[Requirement],
        constraints: &'a Constraints,
        overrides: &'a Overrides,
        excludes: &'a Excludes,
        dependency_metadata: &'a DependencyMetadata,
        hasher: &HashStrategy,
        env: &'a ResolverEnvironment,
    ) -> Self {
        let queue = constraints
            .apply(overrides.apply(requirements))
            .filter(|requirement| !excludes.contains(&requirement.name))
            .filter(|requirement| requirement.evaluate_markers(env.marker_environment(), &[]))
            .map(|requirement| (*requirement).clone())
            .collect();

        Self {
            constraints,
            overrides,
            excludes,
            dependency_metadata,
            env,
            hasher: hasher.clone(),
            queue,
            seen: FxHashSet::default(),
            inputs: Vec::new(),
        }
    }

    /// Return the next source whose metadata must be inspected.
    ///
    /// Each source and requested extras or groups is inspected once. Returning `None` means no
    /// requirements are queued; visiting metadata that is still loading can enqueue more.
    pub fn next_requirement(&mut self) -> Option<Requirement> {
        while let Some(requirement) = self.queue.pop_front() {
            match requirement.source {
                RequirementSource::Registry { .. } => continue,
                RequirementSource::Url { .. }
                | RequirementSource::GitDirectory { .. }
                | RequirementSource::GitPath { .. }
                | RequirementSource::Path { .. }
                | RequirementSource::Directory { .. } => {}
            }
            if self.seen.insert(requirement.clone()) {
                return Some(requirement);
            }
        }
        None
    }

    /// Record a source's metadata and queue the sources declared by its selected dependencies.
    pub fn visit(
        &mut self,
        requirement: Requirement,
        metadata: Metadata,
    ) -> Result<RequestedRequirements, HashStrategyError> {
        // Consider the dependencies to be direct if the requirement is a local source tree.
        let direct = match &requirement.source {
            RequirementSource::Directory { install_path, .. } => install_path.is_dir(),
            RequirementSource::Registry { .. }
            | RequirementSource::Url { .. }
            | RequirementSource::GitDirectory { .. }
            | RequirementSource::GitPath { .. }
            | RequirementSource::Path { .. } => false,
        };

        // Respect recursive extras by propagating the source to self-dependencies.
        let requires_dist = metadata
            .requires_dist
            .iter()
            .chain(
                metadata
                    .dependency_groups
                    .iter()
                    .filter(|(group, _)| requirement.groups.contains(group))
                    .flat_map(|(_, dependencies)| dependencies.iter()),
            )
            .cloned()
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
        let lookahead = RequestedRequirements::new(
            metadata.name.clone(),
            metadata.version.clone(),
            requirement.extras.clone(),
            requires_dist,
            direct,
        );

        // User-provided metadata can authorize dependencies even under required hashes.
        // An override may only match after the source's version has been discovered.
        // Read its hashes directly; the lookahead requirements may come from the archive.
        let trusted_requirements = match self.hasher.verification() {
            HashVerification::Required(_) => self
                .dependency_metadata
                .get(lookahead.package(), Some(lookahead.version()))
                .map(|metadata| {
                    Box::into_iter(metadata.requires_dist)
                        .map(Requirement::from)
                        .collect::<Vec<_>>()
                }),
            HashVerification::None | HashVerification::IfPresent(_) => None,
        };
        let requirements = trusted_requirements
            .as_deref()
            .unwrap_or_else(|| lookahead.requirements())
            .iter()
            .filter(|requirement| {
                !self.excludes.contains_for(
                    lookahead.package(),
                    lookahead.version(),
                    &requirement.name,
                )
            });
        self.hasher = if trusted_requirements.is_some() {
            self.hasher
                .clone()
                .augment_with_requirements(requirements)?
        } else {
            self.hasher
                .clone()
                .augment_with_metadata_requirements(requirements)?
        };
        for requirement in self.constraints.apply(self.overrides.apply_for(
            lookahead.package(),
            lookahead.version(),
            lookahead.requirements(),
        )) {
            if !self.excludes.contains_for(
                lookahead.package(),
                lookahead.version(),
                &requirement.name,
            ) && requirement.evaluate_markers(self.env.marker_environment(), lookahead.extras())
            {
                self.queue.push_back((*requirement).clone());
            }
        }

        self.inputs.push(SourceInput {
            requirement,
            metadata,
        });
        Ok(lookahead)
    }

    /// Return the hash policy including hashes discovered in source declarations.
    pub fn hasher(&self) -> &HashStrategy {
        &self.hasher
    }

    /// Return the inspected source inputs and the resulting hash policy.
    pub fn into_parts(self) -> (Vec<SourceInput>, HashStrategy) {
        (self.inputs, self.hasher)
    }
}
