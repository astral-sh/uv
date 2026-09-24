use std::collections::{BTreeMap, VecDeque};
use std::slice;

use rustc_hash::FxHashSet;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep508::MarkerTree;

use crate::{Metadata, Requirement};

#[derive(Debug, Clone)]
pub struct RequiresDist {
    pub name: PackageName,
    pub requires_dist: Box<[Requirement]>,
    pub provides_extra: Box<[ExtraName]>,
    pub dependency_groups: BTreeMap<GroupName, Box<[Requirement]>>,
    pub dynamic: bool,
}

impl From<Metadata> for RequiresDist {
    fn from(metadata: Metadata) -> Self {
        Self {
            name: metadata.name,
            requires_dist: metadata.requires_dist,
            provides_extra: metadata.provides_extra,
            dependency_groups: metadata.dependency_groups,
            dynamic: metadata.dynamic,
        }
    }
}

/// Like [`uv_pypi_types::RequiresDist`], but with any recursive (or self-referential) dependencies
/// resolved.
///
/// For example, given:
/// ```toml
/// [project]
/// name = "example"
/// version = "0.1.0"
/// requires-python = ">=3.13.0"
/// dependencies = []
///
/// [project.optional-dependencies]
/// all = [
///     "example[async]",
/// ]
/// async = [
///     "fastapi",
/// ]
/// ```
///
/// A build backend could return:
/// ```txt
/// Metadata-Version: 2.2
/// Name: example
/// Version: 0.1.0
/// Requires-Python: >=3.13.0
/// Provides-Extra: all
/// Requires-Dist: example[async]; extra == "all"
/// Provides-Extra: async
/// Requires-Dist: fastapi; extra == "async"
/// ```
///
/// Or:
/// ```txt
/// Metadata-Version: 2.4
/// Name: example
/// Version: 0.1.0
/// Requires-Python: >=3.13.0
/// Provides-Extra: all
/// Requires-Dist: fastapi; extra == 'all'
/// Provides-Extra: async
/// Requires-Dist: fastapi; extra == 'async'
/// ```
///
/// The [`FlatRequiresDist`] struct is used to flatten out the recursive dependencies, i.e., convert
/// from the former to the latter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatRequiresDist(Box<[Requirement]>);

impl FlatRequiresDist {
    /// Flatten a set of requirements, resolving any self-references.
    pub fn from_requirements(requirements: Box<[Requirement]>, name: &PackageName) -> Self {
        // If there are no self-references, we can return early.
        if requirements.iter().all(|req| req.name != *name) {
            return Self(requirements);
        }

        // Transitively process all extras that are recursively included.
        let mut flattened = requirements.to_vec();
        let mut seen = FxHashSet::<(ExtraName, MarkerTree)>::default();
        let mut queue: VecDeque<_> = flattened
            .iter()
            .filter(|req| req.name == *name)
            .flat_map(|req| req.extras.iter().cloned().map(|extra| (extra, req.marker)))
            .collect();
        while let Some((extra, marker)) = queue.pop_front() {
            if !seen.insert((extra.clone(), marker)) {
                continue;
            }

            // Find the optional portion of each requirement for this extra. A requirement can
            // also apply in production, as in `sys_platform == 'win32' or extra == 'base'`.
            for requirement in &requirements {
                let production_marker = requirement.marker.simplify_not_extras_with(|_| true);
                let extra_marker = requirement
                    .marker
                    .simplify_extras(slice::from_ref(&extra))
                    .simplify_not_extras_with(|candidate| candidate != &extra)
                    .and(production_marker.negate());
                let marker = marker.and(extra_marker);
                if marker.is_false() {
                    continue;
                }
                let requirement = Requirement {
                    name: requirement.name.clone(),
                    extras: requirement.extras.clone(),
                    groups: requirement.groups.clone(),
                    source: requirement.source.clone(),
                    scope: requirement.scope.clone(),
                    origin: requirement.origin.clone(),
                    marker,
                };
                if requirement.name == *name {
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
                flattened.push(requirement);
            }
        }

        // Retain any self-constraints for that extra, e.g., if `project[foo]` includes
        // `project[bar]>1.0`, as a dependency, we need to propagate `project>1.0`, in addition to
        // transitively expanding `project[bar]`.
        let mut self_constraints = vec![];
        for req in &flattened {
            if req.name == *name && !req.source.is_empty() {
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

        // Drop all the self-references now that we've flattened them out.
        flattened.retain(|req| req.name != *name);
        flattened.extend(self_constraints);

        Self(flattened.into_boxed_slice())
    }
}

impl IntoIterator for FlatRequiresDist {
    type Item = Requirement;
    type IntoIter = <Box<[Requirement]> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        Box::into_iter(self.0)
    }
}

#[cfg(test)]
mod test {
    use super::FlatRequiresDist;
    use std::str::FromStr;
    use uv_normalize::PackageName;
    use uv_pep508::Requirement;

    #[test]
    fn test_flat_requires_dist_noop() {
        let name = PackageName::from_str("pkg").unwrap();
        let requirements = [
            Requirement::from_str("requests>=2.0.0").unwrap().into(),
            Requirement::from_str("pytest; extra == 'test'")
                .unwrap()
                .into(),
            Requirement::from_str("black; extra == 'dev'")
                .unwrap()
                .into(),
        ];

        let expected = FlatRequiresDist(
            [
                Requirement::from_str("requests>=2.0.0").unwrap().into(),
                Requirement::from_str("pytest; extra == 'test'")
                    .unwrap()
                    .into(),
                Requirement::from_str("black; extra == 'dev'")
                    .unwrap()
                    .into(),
            ]
            .into(),
        );

        let actual = FlatRequiresDist::from_requirements(requirements.into(), &name);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_flat_requires_dist_basic() {
        let name = PackageName::from_str("pkg").unwrap();
        let requirements = [
            Requirement::from_str("requests>=2.0.0").unwrap().into(),
            Requirement::from_str("pytest; extra == 'test'")
                .unwrap()
                .into(),
            Requirement::from_str("pkg[dev]; extra == 'test'")
                .unwrap()
                .into(),
            Requirement::from_str("black; extra == 'dev'")
                .unwrap()
                .into(),
        ];

        let expected = FlatRequiresDist(
            [
                Requirement::from_str("requests>=2.0.0").unwrap().into(),
                Requirement::from_str("pytest; extra == 'test'")
                    .unwrap()
                    .into(),
                Requirement::from_str("black; extra == 'dev'")
                    .unwrap()
                    .into(),
                Requirement::from_str("black; extra == 'test'")
                    .unwrap()
                    .into(),
            ]
            .into(),
        );

        let actual = FlatRequiresDist::from_requirements(requirements.into(), &name);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_flat_requires_dist_with_markers() {
        let name = PackageName::from_str("pkg").unwrap();
        let requirements = vec![
            Requirement::from_str("requests>=2.0.0").unwrap().into(),
            Requirement::from_str("pytest; extra == 'test'")
                .unwrap()
                .into(),
            Requirement::from_str("pkg[dev]; extra == 'test' and sys_platform == 'win32'")
                .unwrap()
                .into(),
            Requirement::from_str("black; extra == 'dev' and sys_platform == 'win32'")
                .unwrap()
                .into(),
        ];

        let expected = FlatRequiresDist(
            [
                Requirement::from_str("requests>=2.0.0").unwrap().into(),
                Requirement::from_str("pytest; extra == 'test'")
                    .unwrap()
                    .into(),
                Requirement::from_str("black; extra == 'dev' and sys_platform == 'win32'")
                    .unwrap()
                    .into(),
                Requirement::from_str("black; extra == 'test' and sys_platform == 'win32'")
                    .unwrap()
                    .into(),
            ]
            .into(),
        );

        let actual = FlatRequiresDist::from_requirements(requirements.into(), &name);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_flat_requires_dist_self_constraint() {
        let name = PackageName::from_str("pkg").unwrap();
        let requirements = [
            Requirement::from_str("requests>=2.0.0").unwrap().into(),
            Requirement::from_str("pytest; extra == 'test'")
                .unwrap()
                .into(),
            Requirement::from_str("black; extra == 'dev'")
                .unwrap()
                .into(),
            Requirement::from_str("pkg[async]==1.0.0").unwrap().into(),
        ];

        let expected = FlatRequiresDist(
            [
                Requirement::from_str("requests>=2.0.0").unwrap().into(),
                Requirement::from_str("pytest; extra == 'test'")
                    .unwrap()
                    .into(),
                Requirement::from_str("black; extra == 'dev'")
                    .unwrap()
                    .into(),
                Requirement::from_str("pkg==1.0.0").unwrap().into(),
            ]
            .into(),
        );

        let actual = FlatRequiresDist::from_requirements(requirements.into(), &name);

        assert_eq!(actual, expected);
    }
}
