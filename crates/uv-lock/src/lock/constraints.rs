use std::borrow::Cow;
use std::collections::{BTreeSet, VecDeque};
use std::iter;
use std::path::Path;

use rustc_hash::FxHashMap;
use tracing::debug;

use uv_configuration::{Constraints, Overrides};
use uv_distribution_types::{Requirement, RequirementSource};
use uv_pep440::VersionSpecifiers;
use uv_pep508::MarkerTree;

use super::{
    DependencyContext, Lock, LockError, Package, SatisfiesResult, implicit_constraints_marker,
};

impl Lock {
    /// Establish current constraints from the locked graph, or require resolution when uncertain.
    /// Locked sources and pre-releases remain valid when the current constraints admit them.
    /// Collect dynamic version bounds for validation after the dependency structure is checked.
    pub(super) fn satisfies_constraints<'lock>(
        &'lock self,
        constraints: &BTreeSet<Requirement>,
        root_requirements: &[Cow<'_, Requirement>],
        overrides: &Overrides,
        root: &Path,
        dynamic_constraints: &mut Vec<(&'lock Package, VersionSpecifiers)>,
    ) -> Result<SatisfiesResult<'lock>, LockError> {
        if constraints.is_empty() {
            return Ok(SatisfiesResult::Satisfied);
        }
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
            if marker.is_false() {
                continue;
            }
            // Recursive extras are flattened into their callers' dependency sections, so the
            // lock cannot establish a constraint's original extra context. Clear extra predicates
            // as in the resolver's package lowering, conservatively retaining every potentially
            // applicable bound. Incompatible bounds require resolution to determine applicability.
            for constraint in constraints.get(&package.id.name).into_iter().flatten() {
                let mut marker = marker.and(constraint.marker.simplify_extras_with(|_| true));
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

                if !Self::package_satisfies_requirement(package, constraint, root)? {
                    debug!(
                        "Cannot validate constraint `{constraint}` against locked package `{}`",
                        package.id
                    );
                    return Ok(SatisfiesResult::UnvalidatedConstraint(&package.id.name));
                }
                if package.id.version.is_none()
                    && let Some(specifiers) = constraint.source.version_specifiers()
                    && !specifiers.is_empty()
                {
                    dynamic_constraints.push((package, specifiers.clone()));
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
                        queue.push_back((package, DependencyContext::Group(group), marker));
                    }
                }
                DependencyContext::Extra(_) => {
                    queue.push_back((package, DependencyContext::Production, marker));
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
                    queue.push_back((package, child_context, marker));
                }
            }
        }
        Ok(SatisfiesResult::Satisfied)
    }
}
