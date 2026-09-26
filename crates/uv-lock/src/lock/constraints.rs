use std::borrow::Cow;
use std::collections::{BTreeSet, VecDeque};
use std::iter;
use std::path::Path;

use rustc_hash::FxHashMap;
use tracing::debug;

use uv_configuration::{BuildOptions, Constraints, Overrides, PrereleaseMode};
use uv_distribution::DistributionDatabase;
use uv_distribution_types::{Requirement, RequirementSource};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pep508::{MarkerEnvironment, MarkerTree};
use uv_platform_tags::Tags;
use uv_resolver_types::DistributionMetadataIndex;
use uv_types::{BuildContext, HashStrategy};

use super::{
    DependencyContext, DependencySources, Lock, LockError, SatisfiesResult, Source,
    implicit_constraints_marker,
};

impl Lock {
    /// Validate current constraints against the locked graph, without comparing declarations.
    pub(super) async fn satisfies_constraints<Context: BuildContext>(
        &self,
        constraints: &BTreeSet<Requirement>,
        root_requirements: &[Cow<'_, Requirement>],
        overrides: &Overrides,
        sources: &DependencySources<'_>,
        root: &Path,
        tags: &Tags,
        marker_environment: &MarkerEnvironment,
        build_options: &BuildOptions,
        hasher: &HashStrategy,
        index: &DistributionMetadataIndex,
        database: &DistributionDatabase<'_, Context>,
    ) -> Result<SatisfiesResult<'_>, LockError> {
        let constraints = Constraints::from_requirements(constraints.iter().cloned());
        let mut contexts = FxHashMap::default();
        let mut queue = VecDeque::new();
        let root_marker = self.fork_markers_union().and(implicit_constraints_marker(
            self.requires_python.to_marker_tree(),
            &self.supported_environments,
        ));
        for package in self
            .packages
            .iter()
            .filter(|package| self.is_workspace_package(package))
        {
            queue.push_back((
                package,
                DependencyContext::Production,
                DependencyContext::Production,
                root_marker,
            ));
            for extra in package.optional_dependencies.keys() {
                queue.push_back((
                    package,
                    DependencyContext::Extra(extra),
                    DependencyContext::Production,
                    root_marker,
                ));
            }
            for group in package.dependency_groups.keys() {
                queue.push_back((
                    package,
                    DependencyContext::Group(group),
                    DependencyContext::Production,
                    root_marker,
                ));
            }
        }
        for requirement in root_requirements {
            for package in self.packages_for_name(&requirement.name) {
                let Some(marker) = self.root_requirement_marker(requirement, package) else {
                    continue;
                };
                let marker = marker.and(root_marker);
                queue.push_back((
                    package,
                    DependencyContext::Production,
                    DependencyContext::Production,
                    marker,
                ));
                for extra in &requirement.extras {
                    if let Some((extra, _)) = package.optional_dependencies.get_key_value(extra) {
                        queue.push_back((
                            package,
                            DependencyContext::Extra(extra),
                            DependencyContext::Production,
                            marker,
                        ));
                    }
                }
            }
        }
        while let Some((package, context, parent_context, marker)) = queue.pop_front() {
            let mut marker = marker.and(context.conflict_marker(&package.id.name, &self.conflicts));
            if !package.fork_markers.is_empty() {
                let forks = package
                    .fork_markers
                    .iter()
                    .fold(MarkerTree::FALSE, |marker, fork| marker.or(fork.combined()));
                marker = marker.and(forks);
            }
            if marker.is_false() {
                continue;
            }
            // Constraints apply in the incoming parent's extra context, before merging paths
            // that activate the same outgoing dependency section.
            for constraint in constraints.get(&package.id.name).into_iter().flatten() {
                let mut marker = marker.and(parent_context.constraint_marker(constraint.marker));
                if marker.is_false() {
                    continue;
                }
                // A global URL override replaces competing URL constraints.
                if !matches!(constraint.source, RequirementSource::Registry { .. }) {
                    for requirement in overrides.global_requirements().filter(|requirement| {
                        requirement.name == constraint.name
                            && !matches!(requirement.source, RequirementSource::Registry { .. })
                    }) {
                        marker = marker.and(requirement.marker.negate());
                    }
                    if marker.is_false() {
                        continue;
                    }
                }

                // Dynamic source-tree versions are absent from the lock, so obtain the current
                // metadata before checking their version bounds.
                let metadata;
                let version = if package.id.version.is_none()
                    && constraint
                        .source
                        .version_specifiers()
                        .is_some_and(|specifiers| !specifiers.is_empty())
                {
                    metadata = Self::package_metadata(
                        package,
                        root,
                        tags,
                        marker_environment,
                        build_options,
                        hasher,
                        index,
                        database,
                    )
                    .await?;
                    Some(&metadata.version)
                } else {
                    package.id.version.as_ref()
                };
                let version_matches = constraint
                    .source
                    .version_specifiers()
                    .zip(version)
                    .is_none_or(|(specifiers, version)| specifiers.contains(version));

                if !version_matches
                    || !Self::package_satisfies_requirement(package, constraint, root)?
                {
                    debug!(
                        "Locked package `{}` does not establish constraint `{constraint}`",
                        package.id
                    );
                    return Ok(SatisfiesResult::UnvalidatedConstraints(&package.id.name));
                }
            }

            match &package.id.source {
                Source::Registry(_) => {
                    if package
                        .id
                        .version
                        .as_ref()
                        .is_some_and(Version::any_prerelease)
                    {
                        match self.prerelease().mode(&package.id.name) {
                            PrereleaseMode::Disallow => {
                                return Ok(SatisfiesResult::UnvalidatedConstraints(
                                    &package.id.name,
                                ));
                            }
                            PrereleaseMode::Explicit => {
                                let allowed = sources.prereleases.get(&package.id.name);
                                if !marker.without_extras().implies(allowed).is_true() {
                                    return Ok(SatisfiesResult::UnvalidatedConstraints(
                                        &package.id.name,
                                    ));
                                }
                            }
                            #[expect(deprecated)]
                            PrereleaseMode::Allow
                            | PrereleaseMode::IfNecessary
                            | PrereleaseMode::IfNecessaryOrExplicit => {}
                        }
                    }
                }
                Source::Git(..)
                | Source::Direct(..)
                | Source::Path(..)
                | Source::Directory(..)
                | Source::Editable(..)
                | Source::Virtual(..) => {
                    // Removing a source constraint must not leave its selected source implicitly
                    // authorized by an inherited edge. Current declarations may still select it.
                    let mut authorized = if self.is_workspace_package(package) {
                        MarkerTree::TRUE
                    } else {
                        MarkerTree::FALSE
                    };
                    for requirement in sources.requirements.requirements() {
                        if requirement.name == package.id.name
                            && !matches!(requirement.source, RequirementSource::Registry { .. })
                            && package
                                .id
                                .source
                                .satisfies_requirement_source(&requirement.source, root)?
                        {
                            authorized = authorized.or(requirement.marker);
                        }
                    }
                    if !marker.implies(authorized).is_true() {
                        return Ok(SatisfiesResult::UnvalidatedConstraints(&package.id.name));
                    }
                }
            }

            let previous = contexts
                .entry((&package.id, context))
                .or_insert(MarkerTree::FALSE);
            let marker = previous.or(marker);
            if *previous == marker {
                continue;
            }
            *previous = marker;
            match context {
                DependencyContext::Production => {
                    for group in package.dependency_groups.keys() {
                        queue.push_back((
                            package,
                            DependencyContext::Group(group),
                            parent_context,
                            marker,
                        ));
                    }
                }
                DependencyContext::Extra(_) => {
                    queue.push_back((
                        package,
                        DependencyContext::Production,
                        parent_context,
                        marker,
                    ));
                }
                DependencyContext::Group(_) => {}
            }
            for dependency in context.dependencies(package) {
                let marker = marker.and(dependency.complexified_marker.combined());
                let package = self.package(dependency.index);
                for child_context in iter::once(DependencyContext::Production).chain(
                    dependency.extra.iter().filter_map(|extra| {
                        package
                            .optional_dependencies
                            .get_key_value(extra)
                            .map(|(extra, _)| DependencyContext::Extra(extra))
                    }),
                ) {
                    queue.push_back((package, child_context, context, marker));
                }
            }
        }
        Ok(SatisfiesResult::Satisfied)
    }
}

/// Explicit pre-release opt-ins collected alongside current source declarations.
#[derive(Default)]
pub(super) struct PrereleaseMarkers(FxHashMap<PackageName, MarkerTree>);

impl PrereleaseMarkers {
    pub(super) fn insert(&mut self, requirement: &Requirement, marker: MarkerTree) {
        if requirement.allows_prereleases() {
            self.0
                .entry(requirement.name.clone())
                .and_modify(|existing| *existing = existing.or(marker))
                .or_insert(marker);
        }
    }

    fn get(&self, name: &PackageName) -> MarkerTree {
        self.0.get(name).copied().unwrap_or(MarkerTree::FALSE)
    }
}
