use rustc_hash::FxHashMap;

use uv_distribution_types::{Requirement, RequirementSource};
use uv_normalize::{GroupName, PackageName};
use uv_pep508::{MarkerTree, RequirementOrigin};
use uv_pypi_types::{ConflictItem, ConflictItemRef};

use crate::ResolverEnvironment;
use crate::universal_marker::UniversalMarker;

/// A set of package names associated with a given fork.
pub(crate) type ForkSet = ForkMap<()>;

/// A map from package names to their values for a given fork.
#[derive(Debug, Clone)]
pub(crate) struct ForkMap<T>(FxHashMap<PackageName, Vec<Entry<T>>>);

/// An entry in a [`ForkMap`].
#[derive(Debug, Clone)]
struct Entry<T> {
    value: T,
    scope: ForkScope,
}

/// The fork visibility of an entry.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct ForkScope {
    marker: MarkerTree,
    conflict: Option<ConflictItem>,
}

impl ForkScope {
    /// Derives the fork scope implied by a requirement's marker and conflict state.
    pub(super) fn from_requirement(requirement: &Requirement) -> Self {
        let conflict = Self::conflict_for_requirement(requirement);
        let marker = conflict.as_ref().map_or(requirement.marker, |conflict| {
            Self::marker_with_conflict(requirement.marker, conflict)
        });
        Self { marker, conflict }
    }

    /// Derives a fork scope for a dependency-group requirement.
    pub(super) fn from_group(
        marker: MarkerTree,
        project_name: &PackageName,
        group: &GroupName,
    ) -> Self {
        let conflict = ConflictItem::from((project_name.clone(), group.clone()));
        let marker = Self::marker_with_conflict(marker, &conflict);
        Self {
            marker,
            conflict: Some(conflict),
        }
    }

    /// Returns the marker under which the entry is visible.
    pub(super) fn marker(&self) -> MarkerTree {
        self.marker
    }

    /// Returns the conflict item that must remain enabled for this scope to match, if any.
    fn conflict(&self) -> Option<ConflictItemRef<'_>> {
        self.conflict.as_ref().map(ConflictItem::as_ref)
    }

    fn conflict_for_requirement(requirement: &Requirement) -> Option<ConflictItem> {
        let conflict = match &requirement.source {
            RequirementSource::Registry { conflict, .. } => conflict.clone(),
            RequirementSource::Url { .. }
            | RequirementSource::Git { .. }
            | RequirementSource::Path { .. }
            | RequirementSource::Directory { .. } => None,
        };
        conflict.or_else(|| match requirement.origin.as_ref() {
            Some(RequirementOrigin::Group(_, Some(project_name), group)) => {
                Some(ConflictItem::from((project_name.clone(), group.clone())))
            }
            _ => None,
        })
    }

    fn marker_with_conflict(marker: MarkerTree, conflict: &ConflictItem) -> MarkerTree {
        UniversalMarker::from_marker_and_conflict_item(marker, conflict).combined()
    }

    fn matches(&self, env: &ResolverEnvironment) -> bool {
        env.included_by_marker(self.marker)
            && self
                .conflict()
                .is_none_or(|conflict| env.included_by_group(conflict))
    }
}

impl<T> Default for ForkMap<T> {
    fn default() -> Self {
        Self(FxHashMap::default())
    }
}

impl<T> ForkMap<T> {
    /// Associate a value with the [`Requirement`] in a given fork.
    pub(crate) fn add(&mut self, requirement: &Requirement, value: T) {
        self.0
            .entry(requirement.name.clone())
            .or_default()
            .push(Entry {
                value,
                scope: ForkScope::from_requirement(requirement),
            });
    }

    /// Returns `true` if the map contains any values for a package that are compatible with the
    /// given fork.
    pub(crate) fn contains(&self, package_name: &PackageName, env: &ResolverEnvironment) -> bool {
        !self.get(package_name, env).is_empty()
    }

    /// Returns `true` if the map contains any values for a package.
    pub(crate) fn contains_key(&self, package_name: &PackageName) -> bool {
        self.0.contains_key(package_name)
    }

    /// Returns a list of values associated with a package that are compatible with the given fork.
    ///
    /// Compatibility implies that the markers on the requirement that contained this value
    /// are not disjoint with the given fork. Note that this does not imply that the requirement
    /// diverged in the given fork - values from overlapping forks may be combined.
    pub(crate) fn get(&self, package_name: &PackageName, env: &ResolverEnvironment) -> Vec<&T> {
        let Some(values) = self.0.get(package_name) else {
            return Vec::new();
        };
        values
            .iter()
            .filter(|entry| entry.scope.matches(env))
            .map(|entry| &entry.value)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::str::FromStr;

    use uv_distribution_types::RequirementSource;
    use uv_normalize::{GroupName, PackageName};
    use uv_pep508::VerbatimUrl;

    use super::*;

    #[test]
    fn add_scopes_non_registry_requirements_to_group_origin() {
        let project_name = PackageName::from_str("workspace-root").unwrap();
        let group = GroupName::from_str("dev").unwrap();
        let package_name = PackageName::from_str("demo").unwrap();
        let conflict = ConflictItem::from((project_name.clone(), group.clone()));
        let requirement = Requirement {
            name: package_name.clone(),
            extras: Box::default(),
            groups: Box::default(),
            marker: MarkerTree::TRUE,
            source: RequirementSource::Directory {
                install_path: PathBuf::from("/tmp/demo").into_boxed_path(),
                editable: None,
                r#virtual: None,
                url: VerbatimUrl::parse_url("file:///tmp/demo").unwrap(),
            },
            origin: Some(RequirementOrigin::Group(
                PathBuf::from("pyproject.toml"),
                Some(project_name),
                group,
            )),
        };

        let mut map = ForkMap::default();
        map.add(&requirement, ());

        assert!(map.contains(&package_name, &ResolverEnvironment::universal(vec![])));

        let env = ResolverEnvironment::universal(vec![])
            .filter_by_group([Err(conflict)])
            .unwrap();
        assert!(!map.contains(&package_name, &env));
    }
}
