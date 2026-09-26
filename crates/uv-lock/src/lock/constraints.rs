use std::borrow::Cow;
use std::collections::{BTreeSet, VecDeque};
use std::iter;
use std::path::Path;

use rustc_hash::FxHashMap;
use tracing::debug;

use uv_configuration::{BuildOptions, Excludes, Overrides, PrereleaseMode};
use uv_distribution::DistributionDatabase;
use uv_distribution_types::{Requirement, RequirementSource};
use uv_normalize::PackageName;
use uv_pep440::{Operator, Version};
use uv_pep508::{MarkerEnvironment, MarkerTree};
use uv_platform_tags::Tags;
use uv_resolver_types::DistributionMetadataIndex;
use uv_types::{BuildContext, HashStrategy};

use super::{
    DependencyContext, DependencySources, Lock, LockError, PackageId, PackageMarkers,
    SatisfiesResult, Source, SourceTreeRequiresDist, implicit_constraints_marker,
};

impl Lock {
    /// Validate current constraints against the locked graph, without comparing declarations.
    pub(super) async fn satisfies_constraints<Context: BuildContext>(
        &self,
        constraints: &BTreeSet<Requirement>,
        root_requirements: &[Cow<'_, Requirement>],
        overrides: &Overrides,
        sources: &DependencySources<'_>,
        source_tree_metadata: &FxHashMap<PackageId, Option<SourceTreeRequiresDist>>,
        root: &Path,
        tags: &Tags,
        marker_environment: &MarkerEnvironment,
        build_options: &BuildOptions,
        hasher: &HashStrategy,
        index: &DistributionMetadataIndex,
        database: &DistributionDatabase<'_, Context>,
    ) -> Result<SatisfiesResult<'_>, LockError> {
        let markers = self.constraint_markers(root_requirements);
        for constraint in constraints {
            if let RequirementSource::Registry { specifier, .. } = &constraint.source
                && specifier.is_empty()
            {
                continue;
            }
            for package in self.packages_for_name(&constraint.name) {
                let Some(marker) = markers.get(&package.id) else {
                    continue;
                };
                let mut marker = marker.and(constraint.marker);
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

                // Extra markers on constraints are evaluated in the requiring package's
                // context, which is not represented by the target's reachability marker.
                if constraint.marker != constraint.marker.without_extras()
                    || !version_matches
                    || !Self::package_satisfies_requirement(package, constraint, root)?
                {
                    debug!(
                        "Locked package `{}` does not establish constraint `{constraint}`",
                        package.id
                    );
                    return Ok(SatisfiesResult::UnvalidatedConstraints(&package.id.name));
                }
            }
        }

        for package in &self.packages {
            let Some(marker) = markers.get(&package.id) else {
                continue;
            };
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
                                let allowed = self.prerelease_constraint_markers(
                                    &package.id.name,
                                    constraints,
                                    root_requirements,
                                    overrides,
                                    &markers,
                                    source_tree_metadata,
                                );
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
                    if self.is_workspace_package(package) {
                        continue;
                    }
                    let mut authorized = false;
                    for requirement in sources.requirements.requirements() {
                        if requirement.name == package.id.name
                            && !matches!(requirement.source, RequirementSource::Registry { .. })
                            && !marker
                                .only_extras()
                                .is_disjoint(requirement.marker.only_extras())
                            && package
                                .id
                                .source
                                .satisfies_requirement_source(&requirement.source, root)?
                        {
                            authorized = true;
                            break;
                        }
                    }
                    if !authorized {
                        return Ok(SatisfiesResult::UnvalidatedConstraints(&package.id.name));
                    }
                }
            }
        }
        Ok(SatisfiesResult::Satisfied)
    }

    /// Reconstruct explicit pre-release opt-in from current constraints and validated declarations.
    fn prerelease_constraint_markers(
        &self,
        name: &PackageName,
        constraints: &BTreeSet<Requirement>,
        root_requirements: &[Cow<'_, Requirement>],
        overrides: &Overrides,
        markers: &PackageMarkers<'_>,
        source_tree_metadata: &FxHashMap<PackageId, Option<SourceTreeRequiresDist>>,
    ) -> MarkerTree {
        // Unretained exclusions for absent scopes intentionally take effect only on a fresh resolve.
        let excludes = Excludes::from_entries(self.manifest.excludes.iter().cloned());
        let mut allowed = MarkerTree::FALSE;
        let mut record = |requirement: &Requirement, marker: MarkerTree| {
            if requirement.name == *name
                && let RequirementSource::Registry { specifier, .. } = &requirement.source
                && specifier.iter().any(|specifier| {
                    !matches!(
                        specifier.operator(),
                        Operator::NotEqual | Operator::NotEqualStar
                    ) && specifier.any_prerelease()
                })
            {
                allowed = allowed.or(marker);
            }
        };
        for requirement in constraints
            .iter()
            .chain(root_requirements.iter().map(AsRef::as_ref))
            .chain(overrides.global_requirements())
            .filter(|requirement| !excludes.contains(&requirement.name))
        {
            record(
                requirement,
                DependencyContext::Production.requirement_marker(requirement.marker),
            );
        }
        for (package, version, requirement) in overrides.scoped_requirements() {
            if !excludes.contains_for_scope(overrides, package, version, &requirement.name) {
                record(
                    requirement,
                    DependencyContext::Production.requirement_marker(requirement.marker),
                );
            }
        }
        for package in &self.packages {
            if matches!(package.id.source, Source::Registry(_))
                || markers.get(&package.id).is_none()
            {
                continue;
            }
            let metadata = source_tree_metadata
                .get(&package.id)
                .and_then(Option::as_ref);
            let version = metadata
                .and_then(|metadata| metadata.version.as_ref())
                .or(package.id.version.as_ref());
            for context in iter::once(DependencyContext::Production)
                .chain(
                    package
                        .optional_dependencies
                        .keys()
                        .filter(|extra| markers.get_extra(&package.id, extra).is_some())
                        .map(DependencyContext::Extra),
                )
                .chain(
                    package
                        .dependency_groups
                        .keys()
                        .map(DependencyContext::Group),
                )
            {
                let requirements = if let Some(metadata) = metadata {
                    match context {
                        DependencyContext::Production | DependencyContext::Extra(_) => {
                            metadata.metadata.requires_dist.to_vec()
                        }
                        DependencyContext::Group(group) => metadata
                            .metadata
                            .dependency_groups
                            .get(group)
                            .map_or_else(Vec::new, |requirements| requirements.to_vec()),
                    }
                } else {
                    match context {
                        DependencyContext::Production | DependencyContext::Extra(_) => {
                            package.metadata.requires_dist.iter().cloned().collect()
                        }
                        DependencyContext::Group(group) => package
                            .metadata
                            .dependency_groups
                            .get(group)
                            .into_iter()
                            .flatten()
                            .cloned()
                            .collect(),
                    }
                };
                for requirement in Self::preprocess_requirements(
                    &package.id.name,
                    version,
                    &requirements,
                    context,
                    overrides,
                    &excludes,
                ) {
                    record(&requirement, context.requirement_marker(requirement.marker));
                }
            }
        }
        allowed
    }

    /// Restore the complete environments of locked packages from parent-relative edge markers.
    fn constraint_markers(&self, root_requirements: &[Cow<'_, Requirement>]) -> PackageMarkers<'_> {
        let mut markers = PackageMarkers::default();
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
            queue.push_back((package, DependencyContext::Production, root_marker));
            for extra in package.optional_dependencies.keys() {
                queue.push_back((package, DependencyContext::Extra(extra), root_marker));
            }
            for group in package.dependency_groups.keys() {
                queue.push_back((package, DependencyContext::Group(group), root_marker));
            }
        }
        for requirement in root_requirements {
            for package in self.packages_for_name(&requirement.name) {
                let Some(marker) = self.root_requirement_marker(requirement, package) else {
                    continue;
                };
                let marker = marker.and(root_marker);
                queue.push_back((package, DependencyContext::Production, marker));
                for extra in &requirement.extras {
                    if let Some((extra, _)) = package.optional_dependencies.get_key_value(extra) {
                        queue.push_back((package, DependencyContext::Extra(extra), marker));
                    }
                }
            }
        }
        while let Some((package, context, marker)) = queue.pop_front() {
            let mut marker = marker.and(context.conflict_marker(&package.id.name, &self.conflicts));
            if !package.fork_markers.is_empty() {
                let forks = package
                    .fork_markers
                    .iter()
                    .fold(MarkerTree::FALSE, |marker, fork| marker.or(fork.combined()));
                marker = marker.and(forks);
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
                    markers.merge(&package.id, None, marker);
                    for group in package.dependency_groups.keys() {
                        queue.push_back((package, DependencyContext::Group(group), marker));
                    }
                }
                DependencyContext::Extra(extra) => {
                    markers.merge(&package.id, Some(extra), marker);
                    queue.push_back((package, DependencyContext::Production, marker));
                }
                DependencyContext::Group(_) => {}
            }
            for dependency in context.dependencies(package) {
                let marker = marker.and(dependency.complexified_marker.combined());
                let package = self.package(dependency.index);
                for context in iter::once(DependencyContext::Production).chain(
                    dependency.extra.iter().filter_map(|extra| {
                        package
                            .optional_dependencies
                            .get_key_value(extra)
                            .map(|(extra, _)| DependencyContext::Extra(extra))
                    }),
                ) {
                    queue.push_back((package, context, marker));
                }
            }
        }
        markers
    }
}
