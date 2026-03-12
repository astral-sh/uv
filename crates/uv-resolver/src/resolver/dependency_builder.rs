use std::borrow::Cow;
use std::collections::BTreeMap;

use pubgrub::Ranges;

use uv_distribution_types::{Requirement, RequirementSource};
use uv_normalize::{GroupName, PackageName};
use uv_pep508::{MarkerTree, RequirementOrigin};
use uv_types::InstalledPackagesProvider;

use crate::pubgrub::{DependencySource, PubGrubDependency, PubGrubPackage};
use crate::python_requirement::PythonRequirement;
use crate::resolver::environment::ResolverEnvironment;
use crate::universal_marker::{ConflictMarker, UniversalMarker};

use super::ResolverState;

/// A small helper for querying and mutating the dependency list under construction.
struct DependencySet<'a> {
    deps: &'a mut Vec<PubGrubDependency>,
}

impl<'a> DependencySet<'a> {
    /// Wraps a mutable dependency list with search helpers used during normalization.
    fn new(deps: &'a mut Vec<PubGrubDependency>) -> Self {
        Self { deps }
    }

    /// Returns the dependency for `name` whose edge-local source exactly matches `source`.
    fn find_source_mut(
        &mut self,
        name: &PackageName,
        source: &DependencySource,
    ) -> Option<&mut PubGrubDependency> {
        self.deps
            .iter_mut()
            .find(|dep| dep.matches_name_and_source(name, source))
    }

    /// Returns the plain base dependency for `name`, if it exists.
    fn find_unsourced_base_mut(&mut self, name: &PackageName) -> Option<&mut PubGrubDependency> {
        self.deps
            .iter_mut()
            .find(|dep| dep.is_unsourced_base_for(name))
    }

    /// Appends a dependency to the underlying list.
    fn push(&mut self, dep: PubGrubDependency) {
        self.deps.push(dep);
    }
}

/// A requirement that should be represented as a complementary source-aware base dependency.
///
/// This captures both the source that should be attached to the synthesized dependency and the
/// source used to find an already-flattened sibling dependency in the root path.
struct ComplementarySourceRequirement<'a> {
    requirement: &'a Requirement,
    marker: MarkerTree,
    source: DependencySource,
    match_source: DependencySource,
}

impl ComplementarySourceRequirement<'_> {
    /// Returns the package name of the underlying requirement.
    fn name(&self) -> &PackageName {
        &self.requirement.name
    }
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
    pub(super) fn extend_requirements(
        &mut self,
        requirements: impl IntoIterator<Item = Cow<'a, Requirement>>,
        group_name: Option<&'a GroupName>,
    ) {
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

    /// Rewrites root dependencies whose source is only active behind a dependency group marker.
    ///
    /// Unlike non-root packages, root requirements have already been flattened from
    /// `ResolverState::requirements`, so this pass mutates the already-added dependencies in place
    /// instead of synthesizing new ones from raw metadata.
    pub(super) fn rewrite_root_complementary_sources(&mut self) {
        let python_marker = self.python_requirement.to_marker_tree();

        for requirement in self.state.overrides.apply(self.state.requirements.iter()) {
            let requirement: &Requirement = requirement.as_ref();

            let Some(RequirementOrigin::Group(_, Some(project_name), group)) =
                requirement.origin.as_ref()
            else {
                continue;
            };

            let mut marker = requirement.marker;
            marker.and(Self::group_conflict_marker(project_name, group));

            let Some(requirement) =
                self.complementary_source_requirement(requirement, marker, false, python_marker)
            else {
                continue;
            };

            let mut deps = DependencySet::new(&mut self.deps);
            let Some(group_dep) =
                deps.find_source_mut(requirement.name(), &requirement.match_source)
            else {
                continue;
            };

            group_dep.package =
                PubGrubPackage::from_base(requirement.name().clone(), requirement.marker);

            let Some(base) = deps.find_unsourced_base_mut(requirement.name()) else {
                continue;
            };

            Self::exclude_marker_from_base(base, requirement.name(), requirement.marker);
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
            let requirement: &Requirement = requirement.as_ref();
            let Some(requirement) = self.complementary_source_requirement(
                requirement,
                requirement.marker,
                requirement.evaluate_markers(self.env.marker_environment(), &[]),
                python_marker,
            ) else {
                continue;
            };

            self.add_complementary_source_dependency(requirement);
        }

        let Some(parent_name) = self.package.name_no_root() else {
            return;
        };

        for (group, requirements) in dependency_groups {
            let group_marker = Self::group_conflict_marker(parent_name, group);

            for requirement in self.state.overrides.apply(requirements.iter()) {
                let requirement: &Requirement = requirement.as_ref();
                let mut marker = requirement.marker;
                marker.and(group_marker);

                let Some(requirement) = self.complementary_source_requirement(
                    requirement,
                    marker,
                    false,
                    python_marker,
                ) else {
                    continue;
                };

                self.add_complementary_source_dependency(requirement);
            }
        }
    }

    /// Splits the existing unsourced base dependency and adds a complementary source-aware base
    /// dependency for `requirement`.
    fn add_complementary_source_dependency(
        &mut self,
        requirement: ComplementarySourceRequirement<'_>,
    ) {
        let parent = self.package.name_no_root().cloned();
        let mut deps = DependencySet::new(&mut self.deps);

        // A base dep (without a URL) must already exist; otherwise this is an
        // extra/group-only dependency and should remain as-is.
        let Some(base) = deps.find_unsourced_base_mut(requirement.name()) else {
            return;
        };

        Self::exclude_marker_from_base(base, requirement.name(), requirement.marker);

        deps.push(PubGrubDependency {
            package: PubGrubPackage::from_base(requirement.name().clone(), requirement.marker),
            version: Ranges::full(),
            parent,
            source: requirement.source,
        });
    }

    /// Returns the normalized complementary-source representation for `requirement`, if needed in
    /// the current fork.
    fn complementary_source_requirement<'req>(
        &self,
        requirement: &'req Requirement,
        marker: MarkerTree,
        included_in_fork: bool,
        python_marker: MarkerTree,
    ) -> Option<ComplementarySourceRequirement<'req>> {
        // Already included via `flatten_requirements`.
        if included_in_fork {
            return None;
        }
        // Only explicit sources (URL or named index) have per-fork source
        // state that can leak.
        if matches!(
            requirement.source,
            RequirementSource::Registry { index: None, .. }
        ) {
            return None;
        }
        // Requirements with requested extras/groups are handled by the
        // existing Extra/Group machinery.
        if !requirement.extras.is_empty() || !requirement.groups.is_empty() {
            return None;
        }
        if python_marker.is_disjoint(marker) {
            return None;
        }
        if !self.env.included_by_marker(marker) {
            return None;
        }
        if self.state.excludes.contains(&requirement.name) {
            return None;
        }
        // This path is specifically for extra/group-gated source splits.
        if marker.only_extras().is_true() {
            return None;
        }
        Some(ComplementarySourceRequirement {
            requirement,
            marker,
            source: DependencySource::from_requirement_source(&requirement.source, None),
            match_source: DependencySource::from_requirement(requirement),
        })
    }

    /// Returns the synthetic marker used to scope a dependency-group conflict to `parent_name`.
    fn group_conflict_marker(parent_name: &PackageName, group: &GroupName) -> MarkerTree {
        UniversalMarker::new(MarkerTree::TRUE, ConflictMarker::group(parent_name, group)).combined()
    }

    /// Removes `marker` from the existing unsourced base dependency for `name`.
    fn exclude_marker_from_base(
        base: &mut PubGrubDependency,
        name: &PackageName,
        marker: MarkerTree,
    ) {
        let mut base_marker = base.package.marker();
        base_marker.and(marker.negate());
        base.package = PubGrubPackage::from_base(name.clone(), base_marker);
    }

    /// Returns the accumulated dependency edges.
    pub(super) fn finish(self) -> Vec<PubGrubDependency> {
        self.deps
    }
}
