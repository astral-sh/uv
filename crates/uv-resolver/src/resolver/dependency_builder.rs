use std::borrow::Cow;
use std::collections::BTreeMap;
use std::slice;

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

use super::ResolverState;

/// A requirement that should be represented as a complementary source-aware base dependency.
///
/// This captures both the source that should be attached to the complementary dependency edge and
/// the source identity used to find an already-flattened sibling dependency in the root path.
struct ComplementarySourceRequirement<'a> {
    requirement: &'a Requirement,
    marker: MarkerTree,
    version: Ranges<Version>,
    attached_source: DependencySource,
    flattened_marker: MarkerTree,
    flattened_source: DependencySource,
}

impl ComplementarySourceRequirement<'_> {
    /// Returns the package name of the underlying requirement.
    fn name(&self) -> &PackageName {
        &self.requirement.name
    }
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
        group_name: Option<&'req GroupName>,
    ) where
        'a: 'req,
    {
        self.deps
            .extend(requirements.into_iter().flat_map(|requirement| {
                PubGrubDependency::from_requirement(
                    &self.state.conflicts,
                    requirement,
                    group_name,
                    Some(self.package),
                )
            }));
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
        let python_marker = self.python_requirement.to_marker_tree();

        for requirement in self.state.overrides.apply(self.state.requirements.iter()) {
            let requirement: &Requirement = requirement.as_ref();
            let marker = ForkScope::from_requirement(requirement).marker();

            for requirement in
                self.complementary_source_requirements(requirement, marker, false, python_marker)
            {
                self.apply_complementary_source_requirement(
                    requirement,
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
        requirements: &[Requirement],
        dependency_groups: &BTreeMap<GroupName, Box<[Requirement]>>,
    ) {
        let python_marker = self.python_requirement.to_marker_tree();

        for requirement in self.state.overrides.apply(requirements.iter()) {
            let raw_requirement = requirement.into_owned();
            if !self.can_synthesize_non_root_complementary_source(&raw_requirement.source) {
                continue;
            }
            let marker = ForkScope::from_requirement(&raw_requirement).marker();
            let complementary_requirements = self.complementary_source_requirements(
                &raw_requirement,
                marker,
                raw_requirement.evaluate_markers(self.env.marker_environment(), &[]),
                python_marker,
            );

            for requirement in complementary_requirements {
                let extra = Self::single_positive_extra(raw_requirement.marker);
                let constraints = self.constraints_for_complementary_extra_source(
                    &raw_requirement,
                    requirement.marker,
                    extra.as_ref(),
                    python_marker,
                );

                if self.apply_complementary_source_requirement(
                    requirement,
                    ComplementarySourceAction::AddDependency,
                ) {
                    self.extend_requirements(constraints.into_iter().map(Cow::Owned), None);
                }
            }
        }

        let Some(parent_name) = self.package.name_no_root() else {
            return;
        };

        for (group, requirements) in dependency_groups {
            for requirement in self.state.overrides.apply(requirements.iter()) {
                let raw_requirement = requirement.into_owned();
                if !self.can_synthesize_non_root_complementary_source(&raw_requirement.source) {
                    continue;
                }
                let marker =
                    ForkScope::from_group(raw_requirement.marker, parent_name, group).marker();

                let complementary_requirements = self.complementary_source_requirements(
                    &raw_requirement,
                    marker,
                    false,
                    python_marker,
                );

                for requirement in complementary_requirements {
                    let split_requirement =
                        Self::requirement_with_marker(&raw_requirement, requirement.marker);
                    let constraints =
                        self.constraints_for_requirement(&split_requirement, None, python_marker);

                    if self.apply_complementary_source_requirement(
                        requirement,
                        ComplementarySourceAction::AddDependency,
                    ) {
                        self.extend_requirements(constraints.into_iter().map(Cow::Owned), None);
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
        requirement: ComplementarySourceRequirement<'_>,
        action: ComplementarySourceAction,
    ) -> bool {
        let name = requirement.name().clone();
        let parent = self.package.name_no_root().cloned();

        let Some(base_index) = self.find_unsourced_base_index(&name) else {
            if action == ComplementarySourceAction::RewriteFlattenedDependency {
                return self.add_root_unsourced_complement(requirement, name, parent);
            }
            return false;
        };

        if action == ComplementarySourceAction::RewriteFlattenedDependency {
            let Some(flattened_index) = self.find_source_index(
                &name,
                &requirement.flattened_source,
                requirement.flattened_marker,
            ) else {
                return false;
            };

            self.deps[flattened_index].package =
                PubGrubPackage::from_base_preserving_marker(name.clone(), requirement.marker);
        }

        if self.deps[base_index].package.marker().is_false() {
            self.deps[base_index].package = PubGrubPackage::from_base_preserving_marker(
                name.clone(),
                requirement.marker.negate(),
            );
        } else {
            Self::exclude_marker_from_base(&mut self.deps[base_index], &name, requirement.marker);
        }

        if action == ComplementarySourceAction::AddDependency {
            self.deps.push(PubGrubDependency {
                package: PubGrubPackage::from_base_preserving_marker(name, requirement.marker),
                version: requirement.version,
                parent,
                source: requirement.attached_source,
            });
        }

        true
    }

    /// Adds the unsourced side of a root complementary-source split when root flattening only
    /// emitted the sourced edge.
    fn add_root_unsourced_complement(
        &mut self,
        requirement: ComplementarySourceRequirement<'_>,
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

        let Some((base_marker, base_version)) =
            self.root_unsourced_complement(&name, requirement.marker)
        else {
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
    fn complementary_source_requirements<'req>(
        &self,
        requirement: &'req Requirement,
        marker: MarkerTree,
        included_in_fork: bool,
        python_marker: MarkerTree,
    ) -> Vec<ComplementarySourceRequirement<'req>> {
        // Already included via `flatten_requirements`.
        if included_in_fork {
            return Vec::new();
        }
        // Only explicit sources (URL or named index) have per-fork source
        // state that can leak.
        if matches!(
            requirement.source,
            RequirementSource::Registry { index: None, .. }
        ) {
            return Vec::new();
        }
        // Requirements with requested extras/groups are handled by the
        // existing Extra/Group machinery.
        if !requirement.extras.is_empty() || !requirement.groups.is_empty() {
            return Vec::new();
        }
        if self.state.excludes.contains(&requirement.name) {
            return Vec::new();
        }
        // This path is specifically for extra/group-gated source splits.
        if marker.only_extras().is_true() {
            return Vec::new();
        }
        Self::split_complementary_markers(marker)
            .into_iter()
            .filter(|marker| !python_marker.is_disjoint(*marker))
            .filter(|marker| self.env.included_by_marker(*marker))
            .map(|marker| ComplementarySourceRequirement {
                requirement,
                marker,
                version: Self::version_for_requirement(requirement),
                attached_source: DependencySource::from_source(&requirement.source),
                flattened_marker: requirement.marker,
                flattened_source: DependencySource::from_requirement(requirement),
            })
            .collect()
    }

    /// Returns the version range implied by a complementary requirement.
    fn version_for_requirement(requirement: &Requirement) -> Ranges<Version> {
        match &requirement.source {
            RequirementSource::Registry { specifier, .. } => Ranges::from(specifier.clone()),
            RequirementSource::Url { .. }
            | RequirementSource::Git { .. }
            | RequirementSource::Path { .. }
            | RequirementSource::Directory { .. } => Ranges::full(),
        }
    }

    /// Returns `true` when a non-root complementary dependency can be synthesized for `source`.
    ///
    /// Direct URL-like sources are validated against root requirements and constraints. Recreating
    /// them from package metadata would turn them into disallowed transitive URL dependencies.
    fn can_synthesize_non_root_complementary_source(&self, source: &RequirementSource) -> bool {
        if matches!(source, RequirementSource::Registry { index: Some(_), .. }) {
            return true;
        }

        let Some(package_name) = self.package.name_no_root() else {
            return false;
        };

        self.state.project.as_ref() == Some(package_name)
            || self.state.workspace_members.contains(package_name)
    }

    /// Returns the positive extra referenced by `marker`, if it names exactly one extra.
    fn single_positive_extra(marker: MarkerTree) -> Option<ExtraName> {
        let mut extra = None;
        let mut has_negative = false;
        let mut has_multiple = false;

        marker.visit_extras(|operator, candidate| match operator {
            MarkerOperator::Equal => match &extra {
                Some(extra) if extra != candidate => has_multiple = true,
                None => extra = Some(candidate.clone()),
                Some(_) => {}
            },
            MarkerOperator::NotEqual => has_negative = true,
            _ => {}
        });

        if has_negative || has_multiple {
            return None;
        }

        extra
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

    /// Returns the constraints that must be present in the sibling extra or group fork for
    /// `requirement`.
    fn constraints_for_requirement(
        &self,
        requirement: &Requirement,
        extra: Option<&ExtraName>,
        python_marker: MarkerTree,
    ) -> Vec<Requirement> {
        self.state
            .constraints_for_requirement(
                Cow::Borrowed(requirement),
                extra,
                self.env,
                python_marker,
                self.python_requirement,
            )
            .map(Cow::into_owned)
            .collect()
    }

    /// Returns constraints for an extra-gated complementary source dependency.
    ///
    /// Source extra markers are encoded as conflict markers on the synthesized dependency edge.
    /// Root constraints, however, are authored against the raw extra name. Select constraints using
    /// the raw requirement marker, then emit them under the encoded marker for this fork.
    fn constraints_for_complementary_extra_source(
        &self,
        raw_requirement: &Requirement,
        marker: MarkerTree,
        extra: Option<&ExtraName>,
        python_marker: MarkerTree,
    ) -> Vec<Requirement> {
        let Some(extra) = extra else {
            let split_requirement = Self::requirement_with_marker(raw_requirement, marker);
            return self.constraints_for_requirement(&split_requirement, None, python_marker);
        };

        let Some(constraints) = self.state.constraints.get(&raw_requirement.name) else {
            return Vec::new();
        };

        constraints
            .iter()
            .filter_map(|constraint| {
                let mut raw_marker = constraint.marker;
                raw_marker.and(raw_requirement.marker);
                if raw_marker.is_false() {
                    return None;
                }

                if !constraint
                    .evaluate_markers(self.env.marker_environment(), slice::from_ref(extra))
                {
                    return None;
                }

                let mut scoped_marker = raw_marker.without_extras();
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

    /// Returns the root source-agnostic requirement that covers the complement of a sourced edge.
    fn root_unsourced_complement(
        &self,
        name: &PackageName,
        source_marker: MarkerTree,
    ) -> Option<(MarkerTree, Ranges<Version>)> {
        let complement = source_marker.negate();
        let python_marker = self.python_requirement.to_marker_tree();

        self.state
            .overrides
            .apply(self.state.requirements.iter())
            .filter_map(|requirement| {
                let requirement: &Requirement = requirement.as_ref();

                if &requirement.name != name {
                    return None;
                }
                if !matches!(
                    requirement.source,
                    RequirementSource::Registry { index: None, .. }
                ) {
                    return None;
                }
                if !requirement.extras.is_empty() || !requirement.groups.is_empty() {
                    return None;
                }
                if self.state.excludes.contains(&requirement.name) {
                    return None;
                }

                let mut marker = requirement.marker;
                marker.and(complement);
                if marker.is_false()
                    || python_marker.is_disjoint(marker)
                    || !self.env.included_by_marker(marker)
                {
                    return None;
                }

                Some((marker, Self::version_for_requirement(requirement)))
            })
            .next()
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
