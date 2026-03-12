use rustc_hash::FxHashMap;

use uv_distribution_types::Requirement;
use uv_normalize::PackageName;
use uv_pep508::MarkerTree;
use uv_pypi_types::{ConflictItem, ConflictItemRef, ConflictKind};

use crate::ResolverEnvironment;
use crate::universal_marker::{ConflictMarker, UniversalMarker};

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
pub(crate) struct ForkScope {
    marker: MarkerTree,
    conflict: Option<ConflictItem>,
}

impl ForkScope {
    /// Create a scope from a marker plus an optional conflict item.
    pub(crate) fn new(marker: MarkerTree, conflict: Option<ConflictItem>) -> Self {
        Self { marker, conflict }
    }

    /// Derive the scope under which a requirement should be visible in forked resolution.
    ///
    /// Group conflicts are folded into the marker so group-scoped entries only appear in forks
    /// where that group is active.
    pub(crate) fn from_requirement(requirement: &Requirement) -> Self {
        let conflict = match &requirement.source {
            uv_distribution_types::RequirementSource::Registry { conflict, .. } => conflict.clone(),
            uv_distribution_types::RequirementSource::Url { .. }
            | uv_distribution_types::RequirementSource::Git { .. }
            | uv_distribution_types::RequirementSource::Path { .. }
            | uv_distribution_types::RequirementSource::Directory { .. } => None,
        };
        let marker = conflict
            .as_ref()
            .filter(|conflict_item| matches!(conflict_item.kind(), ConflictKind::Group(_)))
            .map_or(requirement.marker, |conflict_item| {
                UniversalMarker::new(
                    requirement.marker.without_extras(),
                    ConflictMarker::from_conflict_item(conflict_item),
                )
                .combined()
            });
        Self::new(marker, conflict)
    }

    /// Return the conflict item that further restricts this scope, if any.
    pub(crate) fn conflict(&self) -> Option<ConflictItemRef<'_>> {
        self.conflict.as_ref().map(ConflictItem::as_ref)
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
        self.add_with_scope(
            &requirement.name,
            ForkScope::from_requirement(requirement),
            value,
        );
    }

    /// Associate a value with a package name and scope in a given fork.
    pub(crate) fn add_with_scope(
        &mut self,
        package_name: &PackageName,
        scope: ForkScope,
        value: T,
    ) {
        let entry = Entry { value, scope };

        self.0.entry(package_name.clone()).or_default().push(entry);
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
