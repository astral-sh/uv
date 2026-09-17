//! Expand requirements before lowering them into PubGrub dependencies.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::{iter, slice};

use either::Either;
use rustc_hash::FxHashSet;
use tracing::trace;

use uv_configuration::{Constraints, Excludes, Overrides};
use uv_distribution_types::Requirement;
use uv_normalize::{ExtraName, PackageName};
use uv_pep440::Version;
use uv_pep508::MarkerTree;
use uv_pypi_types::ConflictItemRef;

use crate::python_requirement::PythonRequirement;
use crate::resolver::environment::ResolverEnvironment;

/// The dependency scope whose requirements are being expanded.
#[derive(Clone, Copy)]
pub(super) enum RequirementContext<'a> {
    Root,
    Package {
        name: &'a PackageName,
        version: &'a Version,
    },
    Extra {
        name: &'a PackageName,
        version: &'a Version,
        extra: &'a ExtraName,
    },
    /// Groups use global overrides and package-scoped exclusions, and may depend on the project.
    Group {
        name: &'a PackageName,
        version: &'a Version,
    },
}

impl<'a> RequirementContext<'a> {
    fn package(self) -> Option<(&'a PackageName, &'a Version)> {
        match self {
            Self::Root => None,
            Self::Package { name, version }
            | Self::Extra { name, version, .. }
            | Self::Group { name, version } => Some((name, version)),
        }
    }

    fn override_package(self) -> Option<(&'a PackageName, &'a Version)> {
        match self {
            Self::Root | Self::Group { .. } => None,
            Self::Package { name, version } | Self::Extra { name, version, .. } => {
                Some((name, version))
            }
        }
    }

    fn extra(self) -> Option<&'a ExtraName> {
        match self {
            Self::Extra { extra, .. } => Some(extra),
            Self::Root | Self::Package { .. } | Self::Group { .. } => None,
        }
    }
}

/// Applies overrides, exclusions, extra activation, and constraints within a resolver fork.
pub(super) struct RequirementExpander<'a> {
    constraints: &'a Constraints,
    overrides: &'a Overrides,
    excludes: &'a Excludes,
    env: &'a ResolverEnvironment,
    python_requirement: &'a PythonRequirement,
    python_marker: MarkerTree,
}

impl<'a> RequirementExpander<'a> {
    pub(super) fn new(
        constraints: &'a Constraints,
        overrides: &'a Overrides,
        excludes: &'a Excludes,
        env: &'a ResolverEnvironment,
        python_requirement: &'a PythonRequirement,
    ) -> Self {
        Self {
            constraints,
            overrides,
            excludes,
            env,
            python_requirement,
            python_marker: python_requirement.to_marker_tree(),
        }
    }

    /// Expand applicable dependencies, including recursively activated extras and constraints.
    pub(super) fn expand<'data>(
        &'data self,
        dependencies: &'data [Requirement],
        context: RequirementContext<'data>,
    ) -> impl Iterator<Item = Cow<'data, Requirement>> {
        let requirements = self.requirements_for_context(dependencies, context);
        let (name, version) = match context {
            // Dependency groups can include the project itself, so they do not flatten recursive
            // dependencies.
            RequirementContext::Root | RequirementContext::Group { .. } => {
                return Either::Left(requirements);
            }
            RequirementContext::Package { name, version }
            | RequirementContext::Extra { name, version, .. } => (name, version),
        };
        if !dependencies
            .iter()
            .any(|requirement| name == &requirement.name && !requirement.extras.is_empty())
        {
            // If the project doesn't define any recursive dependencies, take the fast path.
            return Either::Left(requirements);
        }

        let python_marker = self.python_marker;
        let env = self.env;
        let mut requirements = requirements.collect::<Vec<_>>();

        // Transitively process all extras that are recursively included, starting with the current
        // extra.
        let mut seen = FxHashSet::<(ExtraName, MarkerTree)>::default();
        let mut queue: VecDeque<_> = requirements
            .iter()
            .filter(|req| name == &req.name)
            .flat_map(|req| req.extras.iter().cloned().map(|extra| (extra, req.marker)))
            .collect();
        while let Some((extra, marker)) = queue.pop_front() {
            if !seen.insert((extra.clone(), marker)) {
                continue;
            }
            for requirement in self.requirements_for_context(
                dependencies,
                RequirementContext::Extra {
                    name,
                    version,
                    extra: &extra,
                },
            ) {
                let requirement = match requirement {
                    Cow::Owned(mut requirement) => {
                        requirement.marker = requirement.marker.and(marker);
                        requirement
                    }
                    Cow::Borrowed(requirement) => {
                        let mut marker = marker;
                        marker = marker.and(requirement.marker);
                        Requirement {
                            name: requirement.name.clone(),
                            extras: requirement.extras.clone(),
                            groups: requirement.groups.clone(),
                            source: requirement.source.clone(),
                            scope: requirement.scope.clone(),
                            origin: requirement.origin.clone(),
                            marker: marker.simplify_extras(slice::from_ref(&extra)),
                        }
                    }
                };
                // Filter out unreachable unsatisfiable requirements before they reach the
                // unsatisfiability check.
                let applicable_marker = requirement
                    .marker
                    .simplify_extras(slice::from_ref(&extra))
                    .simplify_not_extras_with(|candidate| candidate != &extra);
                if python_marker.is_disjoint(applicable_marker)
                    || !env.included_by_marker(applicable_marker)
                {
                    continue;
                }
                if name == &requirement.name {
                    // Add each transitively included extra.
                    queue.extend(
                        requirement
                            .extras
                            .iter()
                            .cloned()
                            .map(|extra| (extra, requirement.marker)),
                    );
                }

                // Retain the requirement, including any recursively reached self-constraint.
                requirements.push(Cow::Owned(requirement));
            }
        }

        // Retain any self-constraints for that extra, e.g., if `project[foo]` includes
        // `project[bar]>1.0`, as a dependency, we need to propagate `project>1.0`, in addition to
        // transitively expanding `project[bar]`.
        let mut self_constraints = vec![];
        for req in &requirements {
            if name == &req.name && !req.extras.is_empty() && !req.source.is_empty() {
                self_constraints.push(Requirement {
                    name: req.name.clone(),
                    extras: Box::new([]),
                    groups: req.groups.clone(),
                    source: req.source.clone(),
                    scope: req.scope.clone(),
                    origin: req.origin.clone(),
                    marker: req.marker,
                });
            }
        }

        // Drop all the self-requirements now that we flattened them out.
        requirements.retain(|req| name != &req.name || req.extras.is_empty());
        requirements.extend(self_constraints.into_iter().map(Cow::Owned));

        Either::Right(requirements.into_iter())
    }

    /// The set of the regular and dev dependencies, filtered by Python version,
    /// the markers of this fork and the requested extra.
    fn requirements_for_context<'data, 'parameters>(
        &'data self,
        dependencies: impl IntoIterator<Item = &'data Requirement> + 'parameters,
        context: RequirementContext<'parameters>,
    ) -> impl Iterator<Item = Cow<'data, Requirement>> + 'parameters
    where
        'data: 'parameters,
    {
        let extra = context.extra();
        self.overrides
            .apply_for_package(context.override_package(), dependencies)
            .filter(move |requirement| {
                !self
                    .excludes
                    .contains_for_package(context.package(), &requirement.name)
            })
            .map(move |mut requirement| {
                // Split the marker into production and optional components. If we have e.g.
                // `foo; sys_platform == 'win32' or extra == 'feature'`
                // we split it into
                // `foo; sys_platform == 'win32'` (production) when `extra` is `None`,
                // `foo; extra == 'feature'` (optional) when `extra` is `Some("feature")`.
                // The requirements are then separately tracked in production and optional
                // dependencies respectively.

                let marker = match extra {
                    Some(extra) => requirement
                        .marker
                        .simplify_extras(slice::from_ref(extra))
                        .simplify_not_extras_with(|candidate| candidate != extra)
                        .and(
                            requirement
                                .marker
                                .simplify_not_extras_with(|_| true)
                                .negate(),
                        ),
                    None => requirement.marker.simplify_not_extras_with(|_| true),
                };

                if requirement.marker != marker {
                    requirement.to_mut().marker = marker;
                }

                requirement
            })
            .filter(move |requirement| self.is_requirement_applicable(requirement, extra))
            .flat_map(move |requirement| {
                iter::once(requirement.clone())
                    .chain(self.constraints_for_requirement(requirement, extra))
            })
    }

    /// Whether a requirement is applicable for the Python version, the markers of this fork and the
    /// requested extra.
    fn is_requirement_applicable(
        &self,
        requirement: &Requirement,
        extra: Option<&ExtraName>,
    ) -> bool {
        let env = self.env;
        let python_marker = self.python_marker;
        let python_requirement = self.python_requirement;
        // If the requirement isn't relevant for the current platform, skip it.
        match extra {
            Some(source_extra) => {
                if !requirement.evaluate_markers(env.marker_environment(), &[]) {
                    return false;
                }

                if !env.included_by_group(ConflictItemRef::from((&requirement.name, source_extra)))
                {
                    return false;
                }
            }
            None => {
                if !requirement.evaluate_markers(env.marker_environment(), &[]) {
                    return false;
                }
            }
        }

        // If the requirement would not be selected with any Python version
        // supported by the root, skip it.
        if python_marker.is_disjoint(requirement.marker) {
            trace!(
                "Skipping {requirement} because of Requires-Python: {requires_python}",
                requires_python = python_requirement.target(),
            );
            return false;
        }

        // If we're in a fork in universal mode, ignore any dependency that isn't part of
        // this fork (but will be part of another fork).
        if !env.included_by_marker(requirement.marker) {
            trace!("Skipping {requirement} because of {env}");
            return false;
        }

        true
    }

    /// The constraints applicable to the requirement, filtered by Python version, the markers of
    /// this fork and the requested extra.
    fn constraints_for_requirement<'data, 'parameters>(
        &'data self,
        requirement: Cow<'data, Requirement>,
        extra: Option<&'parameters ExtraName>,
    ) -> impl Iterator<Item = Cow<'data, Requirement>> + 'parameters
    where
        'data: 'parameters,
    {
        let env = self.env;
        let python_marker = self.python_marker;
        let python_requirement = self.python_requirement;
        self.constraints
            .get(&requirement.name)
            .into_iter()
            .flatten()
            .filter_map(move |constraint| {
                // If the requirement would not be selected with any Python version
                // supported by the root, skip it.
                let constraint = if constraint.marker.is_true() {
                    // Additionally, if the requirement is `requests ; sys_platform == 'darwin'`
                    // and the constraint is `requests ; python_version == '3.6'`, the
                    // constraint should only apply when _both_ markers are true.
                    if requirement.marker.is_true() {
                        Cow::Borrowed(constraint)
                    } else {
                        let mut marker = constraint.marker;
                        marker = marker.and(requirement.marker);

                        if marker.is_false() {
                            trace!(
                                "Skipping {constraint} because of disjoint markers: `{}` vs. `{}`",
                                constraint.marker.try_to_string().unwrap(),
                                requirement.marker.try_to_string().unwrap(),
                            );
                            return None;
                        }

                        Cow::Owned(Requirement {
                            name: constraint.name.clone(),
                            extras: constraint.extras.clone(),
                            groups: constraint.groups.clone(),
                            source: constraint.source.clone(),
                            scope: constraint.scope.clone(),
                            origin: constraint.origin.clone(),
                            marker,
                        })
                    }
                } else {
                    let requires_python = python_requirement.target();

                    let mut marker = constraint.marker;
                    marker = marker.and(requirement.marker);

                    if marker.is_false() {
                        trace!(
                            "Skipping {constraint} because of disjoint markers: `{}` vs. `{}`",
                            constraint.marker.try_to_string().unwrap(),
                            requirement.marker.try_to_string().unwrap(),
                        );
                        return None;
                    }

                    // Additionally, if the requirement is `requests ; sys_platform == 'darwin'`
                    // and the constraint is `requests ; python_version == '3.6'`, the
                    // constraint should only apply when _both_ markers are true.
                    if python_marker.is_disjoint(marker) {
                        trace!(
                            "Skipping constraint {requirement} because of Requires-Python: {requires_python}"
                        );
                        return None;
                    }

                    if marker == constraint.marker {
                        Cow::Borrowed(constraint)
                    } else {
                        Cow::Owned(Requirement {
                            name: constraint.name.clone(),
                            extras: constraint.extras.clone(),
                            groups: constraint.groups.clone(),
                            source: constraint.source.clone(),
                            scope: constraint.scope.clone(),
                            origin: constraint.origin.clone(),
                            marker,
                        })
                    }
                };

                // If we're in a fork in universal mode, ignore any dependency that isn't part of
                // this fork (but will be part of another fork).
                if !env.included_by_marker(constraint.marker) {
                    trace!("Skipping {constraint} because of {env}");
                    return None;
                }

                // If the constraint isn't relevant for the current platform, skip it.
                match extra {
                    Some(source_extra) => {
                        if !constraint
                            .evaluate_markers(env.marker_environment(), slice::from_ref(source_extra))
                        {
                            return None;
                        }
                        if !env.included_by_group(ConflictItemRef::from((&requirement.name, source_extra)))
                        {
                            return None;
                        }
                    }
                    None => {
                        if !constraint.evaluate_markers(env.marker_environment(), &[]) {
                            return None;
                        }
                    }
                }

                Some(constraint)
            })
    }
}
