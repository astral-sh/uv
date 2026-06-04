use std::borrow::Cow;
use std::collections::BTreeMap;

use pubgrub::Ranges;

use uv_distribution_types::{Requirement, RequirementSource};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::Version;
use uv_pep508::{ExtraOperator, MarkerExpression, MarkerOperator, MarkerTree, MarkerValueExtra};
use uv_types::InstalledPackagesProvider;

use crate::pubgrub::{DependencySource, PubGrubDependency, PubGrubPackage};
use crate::python_requirement::PythonRequirement;
use crate::resolver::environment::ResolverEnvironment;
use crate::resolver::fork_map::ForkScope;
use crate::universal_marker::encode_package_extras;

use super::ResolverState;

/// A requirement that should be represented as a complementary source-aware base dependency.
///
/// This captures both the source that should be attached to the complementary dependency edge and
/// the source identity used to find an already-flattened sibling dependency in the root path.
struct ComplementarySourceRequirement {
    name: PackageName,
    marker: MarkerTree,
    version: Ranges<Version>,
    attached_source: DependencySource,
    flattened_marker: MarkerTree,
    flattened_source: DependencySource,
}

/// How a complementary source requirement should be applied to the dependency list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ComplementarySourceAction {
    /// Rewrite an already-flattened source-specific dependency into the complementary base edge.
    RewriteFlattenedDependency,
    /// Add a new complementary base dependency with the attached source constraint.
    AddDependency,
}

/// Builds the dependency edges emitted for a single package/fork pair.
///
/// The builder first collects the dependencies produced by the normal flattening path, then
/// adjusts them to preserve source-aware edges that only appear in sibling extra or group forks.
pub(super) struct DependencyBuilder<'a, InstalledPackages: InstalledPackagesProvider> {
    state: &'a ResolverState<InstalledPackages>,
    package: &'a PubGrubPackage,
    env: &'a ResolverEnvironment,
    python_requirement: &'a PythonRequirement,
    deps: Vec<PubGrubDependency>,
}

impl<'a, InstalledPackages: InstalledPackagesProvider> DependencyBuilder<'a, InstalledPackages> {
    /// Creates a builder for the dependency edges emitted while resolving `package` in `env`.
    pub(super) fn new(
        state: &'a ResolverState<InstalledPackages>,
        package: &'a PubGrubPackage,
        env: &'a ResolverEnvironment,
        python_requirement: &'a PythonRequirement,
    ) -> Self {
        Self {
            state,
            package,
            env,
            python_requirement,
            deps: Vec::new(),
        }
    }

    /// Flattens the given requirements into PubGrub dependencies and appends them to the builder.
    pub(super) fn extend_requirements<'req>(
        &mut self,
        requirements: impl IntoIterator<Item = Cow<'req, Requirement>>,
    ) where
        'a: 'req,
    {
        self.deps
            .extend(requirements.into_iter().flat_map(|requirement| {
                PubGrubDependency::from_requirement(
                    &self.state.conflicts,
                    requirement,
                    None,
                    Some(self.package),
                    None,
                )
            }));
    }

    /// Flattens synthesized requirements whose encoded activation marker must be preserved.
    fn extend_marker_preserving_requirements<'req>(
        &mut self,
        requirements: impl IntoIterator<Item = Cow<'req, Requirement>>,
    ) where
        'a: 'req,
    {
        self.deps
            .extend(requirements.into_iter().flat_map(|requirement| {
                let marker = requirement.marker;
                PubGrubDependency::from_requirement(
                    &self.state.conflicts,
                    requirement,
                    None,
                    Some(self.package),
                    Some(marker),
                )
            }));
    }

    /// Flattens dependency-group requirements while preserving markers for source-specific edges
    /// that have a complementary unsourced base requirement.
    pub(super) fn extend_group_requirements<'req>(
        &mut self,
        requirements: impl IntoIterator<Item = Cow<'req, Requirement>>,
        group: &'req GroupName,
        base_requirements: &'req [Requirement],
    ) where
        'a: 'req,
    {
        let requirements = requirements
            .into_iter()
            .map(Cow::into_owned)
            .collect::<Vec<_>>();
        let dependencies = requirements
            .iter()
            .flat_map(|requirement| {
                let marker = self.complementary_group_source_marker(
                    requirement,
                    group,
                    base_requirements,
                    &requirements,
                );
                PubGrubDependency::from_requirement(
                    &self.state.conflicts,
                    Cow::Borrowed(requirement),
                    Some(group),
                    Some(self.package),
                    marker,
                )
            })
            .collect::<Vec<_>>();
        self.deps.extend(dependencies);
    }

    /// Appends already-constructed dependencies to the builder.
    pub(super) fn extend_dependencies(
        &mut self,
        deps: impl IntoIterator<Item = PubGrubDependency>,
    ) {
        self.deps.extend(deps);
    }

    /// Rewrites root dependencies whose source is only active in a sibling extra or group fork.
    ///
    /// Unlike non-root packages, root requirements have already been flattened from
    /// `ResolverState::requirements`, so this pass mutates the already-added dependencies in place
    /// instead of synthesizing new ones from raw metadata.
    pub(super) fn rewrite_root_complementary_sources(&mut self) {
        if self.state.urls.is_empty()
            && self.state.indexes.is_empty()
            && !self
                .state
                .requirements
                .iter()
                .any(|requirement| self.is_source_specific_base_requirement(requirement))
        {
            return;
        }

        let python_marker = self.python_requirement.to_marker_tree();

        for requirement in self.state.overrides.apply(self.state.requirements.iter()) {
            let requirement: &Requirement = requirement.as_ref();
            let scope = self.complementary_source_scope(requirement);

            for requirement in
                self.complementary_source_requirements(requirement, &scope, false, python_marker)
            {
                self.apply_complementary_source_requirement(
                    &requirement,
                    ComplementarySourceAction::RewriteFlattenedDependency,
                );
            }
        }
    }

    /// Adds complementary source-aware base dependencies for requirements that are absent from the
    /// current fork but present in a sibling extra or dependency-group fork.
    ///
    /// Unlike the root path, non-root packages still have access to their raw metadata
    /// requirements, so this pass inspects that metadata and synthesizes any missing
    /// source-aware base dependencies.
    pub(super) fn add_complementary_source_dependencies(
        &mut self,
        base_requirements: &[Requirement],
        dependency_groups: &BTreeMap<GroupName, Box<[Requirement]>>,
    ) {
        if self.state.urls.is_empty()
            && self.state.indexes.is_empty()
            && !base_requirements
                .iter()
                .chain(dependency_groups.values().flatten())
                .any(|requirement| self.is_source_specific_base_requirement(requirement))
        {
            return;
        }

        let python_marker = self.python_requirement.to_marker_tree();

        for requirement in self.state.overrides.apply(base_requirements.iter()) {
            let raw_requirement = requirement.into_owned();
            if !self.can_synthesize_non_root_complementary_source(&raw_requirement) {
                continue;
            }
            let scope = self.complementary_source_scope(&raw_requirement);
            let included_in_fork = raw_requirement
                .evaluate_markers(self.env.marker_environment(), &[])
                && (scope.conflict().is_some() || scope.marker() == raw_requirement.marker);
            let complementary_requirements = self.complementary_source_requirements(
                &raw_requirement,
                &scope,
                included_in_fork,
                python_marker,
            );

            for requirement in complementary_requirements {
                let needs_unsourced_complement = raw_requirement
                    .evaluate_markers(self.env.marker_environment(), &[])
                    && self
                        .find_unsourced_base_index(&raw_requirement.name)
                        .is_none();
                let constraints = self.constraints_for_complementary_extra_source(
                    &raw_requirement,
                    requirement.marker,
                    python_marker,
                    None,
                );

                if self.apply_complementary_source_requirement(
                    &requirement,
                    ComplementarySourceAction::AddDependency,
                ) {
                    if needs_unsourced_complement
                        && let Some(dependency) =
                            self.unsourced_complement_dependency(base_requirements, &requirement)
                    {
                        self.deps.push(dependency);
                    }
                    self.extend_marker_preserving_requirements(
                        constraints.into_iter().map(Cow::Owned),
                    );
                }
            }
        }

        let Some(parent_name) = self.package.name_no_root() else {
            return;
        };
        // Dependency groups are not transitive, so groups from ordinary path
        // dependencies cannot participate in the lock.
        if !self.is_project_or_workspace_member(parent_name) {
            return;
        }

        for (group, requirements) in dependency_groups {
            let requirements = self
                .state
                .overrides
                .apply(requirements.iter())
                .map(Cow::into_owned)
                .collect::<Vec<_>>();
            for raw_requirement in &requirements {
                if !self.can_synthesize_non_root_complementary_source(raw_requirement) {
                    continue;
                }
                if Self::simplify_group_activated_extras(
                    raw_requirement.marker,
                    parent_name,
                    &requirements,
                    base_requirements,
                )
                .is_false()
                {
                    continue;
                }
                let scope = ForkScope::from_group(raw_requirement.marker, parent_name, group);

                let complementary_requirements = self.complementary_source_requirements(
                    raw_requirement,
                    &scope,
                    false,
                    python_marker,
                );

                for requirement in complementary_requirements {
                    let constraints = self.constraints_for_complementary_extra_source(
                        raw_requirement,
                        requirement.marker,
                        python_marker,
                        Some((&requirements, base_requirements)),
                    );

                    if self.apply_complementary_source_requirement(
                        &requirement,
                        ComplementarySourceAction::AddDependency,
                    ) {
                        self.extend_marker_preserving_requirements(
                            constraints.into_iter().map(Cow::Owned),
                        );
                    }
                }
            }
        }
    }

    /// Applies the complementary-source split for `requirement`.
    ///
    /// Both root and non-root packages narrow the existing unsourced base dependency by excluding
    /// `requirement.marker`. They differ only in whether the complementary dependency already
    /// exists in flattened form (`RewriteFlattenedDependency`) or must be synthesized
    /// (`AddDependency`).
    fn apply_complementary_source_requirement(
        &mut self,
        requirement: &ComplementarySourceRequirement,
        action: ComplementarySourceAction,
    ) -> bool {
        let name = requirement.name.clone();
        let parent = self.package.name_no_root().cloned();

        let Some(base_index) = self.find_unsourced_base_index(&name) else {
            if action == ComplementarySourceAction::RewriteFlattenedDependency {
                return self.add_root_unsourced_complement(requirement, name, parent);
            }
            if let Some(source_index) = self.find_source_index(
                &name,
                &requirement.flattened_source,
                requirement.flattened_marker,
            ) {
                self.deps[source_index].package =
                    PubGrubPackage::from_base_preserving_marker(name, requirement.marker);
                return true;
            }
            return false;
        };

        let flattened_index = self.find_source_index(
            &name,
            &requirement.flattened_source,
            requirement.flattened_marker,
        );
        if let Some(flattened_index) = flattened_index {
            self.deps[flattened_index].package =
                PubGrubPackage::from_base_preserving_marker(name.clone(), requirement.marker);
        } else if action == ComplementarySourceAction::RewriteFlattenedDependency {
            return false;
        }

        self.preserve_base_constraint_on_source(base_index, &name, requirement.marker);

        if self.deps[base_index].package.marker().is_false() {
            self.deps[base_index].package = PubGrubPackage::from_base_preserving_marker(
                name.clone(),
                requirement.marker.negate(),
            );
        } else {
            Self::exclude_marker_from_base(&mut self.deps[base_index], &name, requirement.marker);
        }

        if action == ComplementarySourceAction::AddDependency && flattened_index.is_none() {
            self.deps.push(PubGrubDependency {
                package: PubGrubPackage::from_base_preserving_marker(name, requirement.marker),
                version: requirement.version.clone(),
                parent,
                source: requirement.attached_source.clone(),
            });
        }

        true
    }

    /// Preserves an unsourced production constraint on the sourced side of a split.
    ///
    /// The source-specific dependency may come from an optional dependency or dependency group
    /// with a weaker version range than the production dependency. Keep the production range
    /// active wherever both requirements apply.
    fn preserve_base_constraint_on_source(
        &mut self,
        base_index: usize,
        name: &PackageName,
        source_marker: MarkerTree,
    ) {
        let mut marker = self.deps[base_index].package.marker();
        if marker.is_false() {
            return;
        }
        marker.and(source_marker);
        if marker.is_false() {
            return;
        }

        let mut constraint = self.deps[base_index].clone();
        constraint.package = PubGrubPackage::from_base_preserving_marker(name.clone(), marker);
        self.deps.push(constraint);
    }

    /// Adds the unsourced side of a root complementary-source split when root flattening only
    /// emitted the sourced edge.
    fn add_root_unsourced_complement(
        &mut self,
        requirement: &ComplementarySourceRequirement,
        name: PackageName,
        parent: Option<PackageName>,
    ) -> bool {
        let Some(flattened_index) = self.find_source_index(
            &name,
            &requirement.flattened_source,
            requirement.flattened_marker,
        ) else {
            return false;
        };

        let Some((base_marker, base_version)) = self.unsourced_complement(
            self.state.requirements.iter(),
            &name,
            requirement.marker,
            None,
        ) else {
            return false;
        };

        self.deps[flattened_index].package =
            PubGrubPackage::from_base_preserving_marker(name.clone(), requirement.marker);
        self.deps.push(PubGrubDependency {
            package: PubGrubPackage::from_base_preserving_marker(name, base_marker),
            version: base_version,
            parent,
            source: DependencySource::Unspecified,
        });

        true
    }

    /// Returns the normalized complementary-source representations for `requirement`, if needed in
    /// the current fork.
    fn complementary_source_requirements(
        &self,
        requirement: &Requirement,
        scope: &ForkScope,
        included_in_fork: bool,
        python_marker: MarkerTree,
    ) -> Vec<ComplementarySourceRequirement> {
        // Already included via `flatten_requirements`.
        if included_in_fork {
            return Vec::new();
        }
        if !self.is_source_specific_base_requirement(requirement) {
            return Vec::new();
        }
        if self.is_declared_conflict_scope(scope) {
            return Vec::new();
        }
        if !scope.matches(self.env) {
            return Vec::new();
        }
        let marker = scope.marker();
        // This path is specifically for extra/group-gated source splits.
        if !Self::has_extra(marker) {
            return Vec::new();
        }
        Self::split_complementary_markers(marker)
            .into_iter()
            .filter(|marker| !python_marker.is_disjoint(*marker))
            .filter(|marker| self.env.included_by_marker(*marker))
            .map(|marker| ComplementarySourceRequirement {
                name: requirement.name.clone(),
                marker,
                version: Self::version_for_requirement(requirement),
                attached_source: DependencySource::from_source(&requirement.source),
                flattened_marker: if !Self::required_extras(requirement.marker).is_empty()
                    || requirement.marker.simplify_extras_with(|_| true).is_true()
                {
                    requirement.marker
                } else {
                    requirement.marker.simplify_extras_with(|_| true)
                },
                flattened_source: DependencySource::from_requirement(requirement),
            })
            .collect()
    }

    /// Returns the fork scope for a source-specific requirement.
    ///
    /// URL-like sources do not carry a conflict item on their requirement source, so add one for
    /// extra-gated sources when the requirement does not already have a structured group scope.
    fn complementary_source_scope(&self, requirement: &Requirement) -> ForkScope {
        Self::complementary_source_scope_for_project(
            requirement,
            self.package.name_no_root().or(self.state.project.as_ref()),
        )
    }

    /// Returns the fork scope for a source-specific requirement declared by `project_name`.
    fn complementary_source_scope_for_project(
        requirement: &Requirement,
        project_name: Option<&PackageName>,
    ) -> ForkScope {
        let scope = ForkScope::from_requirement(requirement);
        if scope.conflict().is_some() {
            return scope;
        }
        let Some(project_name) = project_name else {
            return scope;
        };
        if matches!(requirement.source, RequirementSource::Registry { .. }) {
            return if Self::has_extra(requirement.marker) {
                ForkScope::from_package_marker(requirement.marker, project_name)
            } else {
                scope
            };
        }
        let Some(extra) = Self::single_required_extra(requirement.marker) else {
            return if Self::has_extra(requirement.marker) {
                ForkScope::from_package_marker(requirement.marker, project_name)
            } else {
                scope
            };
        };
        ForkScope::from_extra(requirement.marker, project_name, &extra)
    }

    /// Returns whether a source is already isolated by a declared conflict.
    ///
    /// The normal extra or group dependency carries the source into the corresponding resolver
    /// fork, so a complementary edge would only create a redundant conflict-only resolution fork.
    fn is_declared_conflict_scope(&self, scope: &ForkScope) -> bool {
        scope.conflict().is_some_and(|conflict| {
            self.state
                .conflicts
                .iter()
                .any(|set| set.contains(conflict.package(), conflict.kind()))
        })
    }

    /// Returns the marker that must be preserved on a source-specific edge with a complementary
    /// unsourced base requirement.
    fn complementary_group_source_marker(
        &self,
        requirement: &Requirement,
        group: &GroupName,
        base_requirements: &[Requirement],
        group_requirements: &[Requirement],
    ) -> Option<MarkerTree> {
        if !self.has_unsourced_base_requirement(base_requirements, &requirement.name) {
            return None;
        }
        if !self.is_source_specific_base_requirement(requirement) {
            return None;
        }

        let parent_name = self.package.name_no_root()?;
        let marker = Self::simplify_group_activated_extras(
            requirement.marker,
            parent_name,
            group_requirements,
            base_requirements,
        );
        Some(ForkScope::from_group(marker, parent_name, group).marker())
    }

    /// Returns the version range implied by a complementary requirement.
    fn version_for_requirement(requirement: &Requirement) -> Ranges<Version> {
        match &requirement.source {
            RequirementSource::Registry { specifier, .. } => Ranges::from(specifier.clone()),
            RequirementSource::Url { .. }
            | RequirementSource::GitDirectory { .. }
            | RequirementSource::GitPath { .. }
            | RequirementSource::Path { .. }
            | RequirementSource::Directory { .. } => Ranges::full(),
        }
    }

    /// Returns `true` when a non-root complementary dependency can be synthesized for
    /// `requirement`.
    ///
    /// Direct URL-like sources are validated against root requirements and constraints. Recreating
    /// them from package metadata would turn them into disallowed transitive URL dependencies.
    fn can_synthesize_non_root_complementary_source(&self, requirement: &Requirement) -> bool {
        if !self.is_source_specific_base_requirement(requirement) {
            return false;
        }
        if matches!(
            requirement.source,
            RequirementSource::Registry { index: Some(_), .. }
        ) || self.state.urls.any_url(&requirement.name)
        {
            return true;
        }

        let Some(package_name) = self.package.name_no_root() else {
            return false;
        };

        self.is_project_or_workspace_member(package_name)
    }

    /// Returns `true` if `package_name` belongs to the project or workspace being resolved.
    fn is_project_or_workspace_member(&self, package_name: &PackageName) -> bool {
        self.state.project.as_ref() == Some(package_name)
            || self.state.workspace_members.contains(package_name)
    }

    /// Returns the positive extra required by `marker`, if it requires exactly one extra.
    fn single_required_extra(marker: MarkerTree) -> Option<ExtraName> {
        let mut extra = None;
        let mut has_multiple = false;

        marker.visit_extras(|operator, candidate| match operator {
            MarkerOperator::Equal => match &extra {
                Some(extra) if extra != candidate => has_multiple = true,
                None => extra = Some(candidate.clone()),
                Some(_) => {}
            },
            MarkerOperator::NotEqual => {}
            _ => {}
        });

        if has_multiple {
            return None;
        }

        let extra = extra?;
        let mut implication = marker;
        implication.implies(Self::extra_marker(&extra));
        implication.is_true().then_some(extra)
    }

    /// Returns every positive extra required by `marker`.
    fn required_extras(marker: MarkerTree) -> Vec<ExtraName> {
        let mut candidates = Vec::new();

        marker.visit_extras(|operator, candidate| {
            if operator == MarkerOperator::Equal && !candidates.contains(candidate) {
                candidates.push(candidate.clone());
            }
        });

        candidates.retain(|extra| {
            let mut implication = marker;
            implication.implies(Self::extra_marker(extra));
            implication.is_true()
        });
        candidates
    }

    /// Returns whether `marker` references at least one extra.
    fn has_extra(marker: MarkerTree) -> bool {
        let mut has_extra = false;
        marker.visit_extras(|_, _| has_extra = true);
        has_extra
    }

    /// Returns the marker split for a complementary dependency.
    ///
    /// When the source applies to multiple sibling extra or group forks, emit one complementary
    /// edge per fork instead of a single marker spanning all of them.
    fn split_complementary_markers(marker: MarkerTree) -> Vec<MarkerTree> {
        let mut extras = Vec::new();

        marker.visit_extras(|operator, candidate| {
            if operator == MarkerOperator::Equal && !extras.contains(candidate) {
                extras.push(candidate.clone());
            }
        });

        if extras.len() <= 1 {
            return vec![marker];
        }

        let mut split_markers = Vec::new();
        for extra in extras {
            let mut split_marker = marker;
            split_marker.and(Self::extra_marker(&extra));
            if !split_marker.is_false() && !split_markers.contains(&split_marker) {
                split_markers.push(split_marker);
            }
        }

        split_markers
    }

    /// Returns a marker that activates only the given extra or encoded group conflict item.
    fn extra_marker(extra: &ExtraName) -> MarkerTree {
        MarkerTree::expression(MarkerExpression::Extra {
            operator: ExtraOperator::Equal,
            name: MarkerValueExtra::Extra(extra.clone()),
        })
    }

    /// Returns constraints for an extra-gated complementary source dependency.
    ///
    /// Source extra markers are encoded as conflict markers on the synthesized dependency edge.
    /// Root constraints, however, are authored against raw extra names. Encode every overlapping
    /// constraint predicate relative to the declaring package before combining it with the source
    /// fork marker.
    fn constraints_for_complementary_extra_source(
        &self,
        raw_requirement: &Requirement,
        marker: MarkerTree,
        python_marker: MarkerTree,
        group_activation_requirements: Option<(&[Requirement], &[Requirement])>,
    ) -> Vec<Requirement> {
        let Some(constraints) = self.state.constraints.get(&raw_requirement.name) else {
            return Vec::new();
        };
        let package = self.package.name_no_root().or(self.state.project.as_ref());

        constraints
            .iter()
            .filter_map(|constraint| {
                let mut raw_marker = constraint.marker;
                raw_marker.and(raw_requirement.marker);
                if let (Some(package), Some((group_requirements, recursive_requirements))) =
                    (package, group_activation_requirements)
                {
                    raw_marker = Self::simplify_group_activated_extras(
                        raw_marker,
                        package,
                        group_requirements,
                        recursive_requirements,
                    );
                }
                if raw_marker.is_false() {
                    return None;
                }

                let mut scoped_marker = package.map_or_else(
                    || raw_marker.without_extras(),
                    |package| encode_package_extras(raw_marker, package),
                );
                scoped_marker.and(marker);
                if scoped_marker.is_false()
                    || python_marker.is_disjoint(scoped_marker)
                    || !self.env.included_by_marker(scoped_marker)
                {
                    return None;
                }

                Some(Self::requirement_with_marker(constraint, scoped_marker))
            })
            .collect()
    }

    /// Simplifies `marker` using extras guaranteed to be activated by dependency-group edges.
    fn simplify_group_activated_extras(
        mut marker: MarkerTree,
        package: &PackageName,
        group_requirements: &[Requirement],
        recursive_requirements: &[Requirement],
    ) -> MarkerTree {
        let mut activated_extras = Vec::new();

        loop {
            let mut changed = false;
            for requirement in group_requirements.iter().chain(
                recursive_requirements
                    .iter()
                    .filter(|requirement| Self::has_extra(requirement.marker)),
            ) {
                if requirement.name != *package {
                    continue;
                }

                let requirement_marker = requirement.marker.simplify_extras_with(|extra| {
                    activated_extras.contains(extra) || requirement.extras.contains(extra)
                });
                let mut implication = marker;
                implication.implies(requirement_marker);
                if !implication.is_true() {
                    continue;
                }

                for extra in &requirement.extras {
                    if !activated_extras.contains(extra) {
                        activated_extras.push(extra.clone());
                        changed = true;
                    }
                }
            }

            if !changed {
                return marker;
            }
            marker = marker.simplify_extras(&activated_extras);
        }
    }

    /// Returns a source-agnostic dependency that covers the complement of a sourced edge.
    fn unsourced_complement_dependency(
        &self,
        requirements: &[Requirement],
        requirement: &ComplementarySourceRequirement,
    ) -> Option<PubGrubDependency> {
        let package = self.package.name_no_root()?;
        let (marker, version) = self.unsourced_complement(
            requirements.iter(),
            &requirement.name,
            requirement.marker,
            Some(package),
        )?;
        Some(PubGrubDependency {
            package: PubGrubPackage::from_base_preserving_marker(requirement.name.clone(), marker),
            version,
            parent: self.package.name_no_root().cloned(),
            source: DependencySource::Unspecified,
        })
    }

    /// Returns the source-agnostic requirement that covers the complement of a sourced edge.
    fn unsourced_complement<'req>(
        &self,
        requirements: impl IntoIterator<Item = &'req Requirement>,
        name: &PackageName,
        source_marker: MarkerTree,
        package: Option<&PackageName>,
    ) -> Option<(MarkerTree, Ranges<Version>)>
    where
        'a: 'req,
    {
        let complement = source_marker.negate();
        let python_marker = self.python_requirement.to_marker_tree();

        self.state
            .overrides
            .apply(requirements)
            .find_map(|requirement| {
                let requirement: &Requirement = requirement.as_ref();

                if !self.is_unsourced_base_requirement(requirement, name) {
                    return None;
                }

                let mut marker = package.map_or(requirement.marker, |package| {
                    encode_package_extras(requirement.marker, package)
                });
                marker.and(complement);
                if marker.is_false()
                    || python_marker.is_disjoint(marker)
                    || !self.env.included_by_marker(marker)
                {
                    return None;
                }

                Some((marker, Self::version_for_requirement(requirement)))
            })
    }

    /// Returns `true` if `requirements` contains an unsourced base requirement for `name` in the
    /// current fork.
    fn has_unsourced_base_requirement(
        &self,
        requirements: &[Requirement],
        name: &PackageName,
    ) -> bool {
        self.state
            .overrides
            .apply(requirements.iter())
            .any(|requirement| {
                let requirement: &Requirement = requirement.as_ref();
                self.is_unsourced_base_requirement(requirement, name)
                    && requirement.evaluate_markers(self.env.marker_environment(), &[])
            })
    }

    /// Returns `true` if `requirement` can participate in a source-specific base dependency split.
    ///
    /// Only explicit sources have per-fork source state that can leak. Requirements with requested
    /// extras or groups are handled by the existing proxy-package machinery.
    fn is_source_specific_base_requirement(&self, requirement: &Requirement) -> bool {
        !matches!(
            requirement.source,
            RequirementSource::Registry { index: None, .. }
        ) && self.is_base_requirement(requirement)
    }

    /// Returns `true` if `requirement` is an unsourced base requirement for `name`.
    fn is_unsourced_base_requirement(&self, requirement: &Requirement, name: &PackageName) -> bool {
        &requirement.name == name
            && matches!(
                requirement.source,
                RequirementSource::Registry { index: None, .. }
            )
            && self.is_base_requirement(requirement)
    }

    /// Returns `true` if `requirement` represents a base package dependency.
    fn is_base_requirement(&self, requirement: &Requirement) -> bool {
        requirement.extras.is_empty()
            && requirement.groups.is_empty()
            && !self.state.excludes.contains(&requirement.name)
    }

    /// Clones a requirement with a replacement marker.
    fn requirement_with_marker(requirement: &Requirement, marker: MarkerTree) -> Requirement {
        Requirement {
            name: requirement.name.clone(),
            extras: requirement.extras.clone(),
            groups: requirement.groups.clone(),
            source: requirement.source.clone(),
            origin: requirement.origin.clone(),
            marker,
        }
    }

    /// Removes `marker` from the existing unsourced base dependency for `name`.
    fn exclude_marker_from_base(
        base: &mut PubGrubDependency,
        name: &PackageName,
        marker: MarkerTree,
    ) {
        let mut base_marker = base.package.marker();
        base_marker.and(marker.negate());
        base.package = PubGrubPackage::from_base_preserving_marker(name.clone(), base_marker);
    }

    /// Returns the index of the dependency for `name` whose package marker and edge-local source
    /// exactly match.
    fn find_source_index(
        &self,
        name: &PackageName,
        source: &DependencySource,
        marker: MarkerTree,
    ) -> Option<usize> {
        self.deps.iter().position(|dep| {
            dep.package.name() == Some(name)
                && dep.package.marker() == marker
                && &dep.source == source
        })
    }

    /// Returns the index of the plain base dependency for `name`, if it exists.
    fn find_unsourced_base_index(&self, name: &PackageName) -> Option<usize> {
        self.deps.iter().position(|dep| {
            dep.package.name() == Some(name)
                && dep.package.extra().is_none()
                && dep.package.group().is_none()
                && matches!(&dep.source, DependencySource::Unspecified)
        })
    }

    /// Returns the accumulated dependency edges.
    pub(super) fn finish(self) -> Vec<PubGrubDependency> {
        self.deps
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::str::FromStr;

    use uv_pep508::{RequirementOrigin, VerbatimUrl};
    use uv_types::EmptyInstalledPackages;

    use super::*;

    #[test]
    fn complementary_source_scope_preserves_group_origin() {
        let project_name = PackageName::from_str("project").unwrap();
        let package_name = PackageName::from_str("demo").unwrap();
        let group = GroupName::from_str("alt").unwrap();
        let marker = MarkerTree::from_str("extra == 'foo'").unwrap();
        let requirement = Requirement {
            name: package_name,
            extras: Box::default(),
            groups: Box::default(),
            marker,
            source: RequirementSource::Directory {
                install_path: PathBuf::from("/tmp/demo").into_boxed_path(),
                editable: None,
                r#virtual: None,
                url: VerbatimUrl::parse_url("file:///tmp/demo").unwrap(),
            },
            origin: Some(RequirementOrigin::Group(
                PathBuf::from("pyproject.toml"),
                Some(project_name.clone()),
                group.clone(),
            )),
        };

        let scope =
            DependencyBuilder::<EmptyInstalledPackages>::complementary_source_scope_for_project(
                &requirement,
                Some(&project_name),
            );

        assert_eq!(scope, ForkScope::from_group(marker, &project_name, &group));
    }

    #[test]
    fn disjunctive_source_marker_has_no_required_extra() {
        let marker =
            MarkerTree::from_str("extra == 'alt' or extra != 'other'").expect("valid marker");

        assert_eq!(
            DependencyBuilder::<EmptyInstalledPackages>::single_required_extra(marker),
            None
        );
        assert!(DependencyBuilder::<EmptyInstalledPackages>::required_extras(marker).is_empty());
    }

    #[test]
    fn conjunctive_source_marker_has_required_extras() {
        let marker =
            MarkerTree::from_str("extra == 'alt' and extra == 'foo'").expect("valid marker");

        assert_eq!(
            DependencyBuilder::<EmptyInstalledPackages>::required_extras(marker),
            vec![
                ExtraName::from_str("alt").expect("valid extra"),
                ExtraName::from_str("foo").expect("valid extra"),
            ]
        );
    }
}
