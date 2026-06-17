use std::collections::{BTreeSet, VecDeque};

use rustc_hash::FxHashSet;
use uv_normalize::{ExtraName, PackageName};
use uv_pep508::MarkerTree;

use crate::lock::{Dependency, Lock, Package, PackageId};
use crate::{ConflictMarker, UniversalMarker};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TraversalState<'lock> {
    package_id: &'lock PackageId,
    extras: BTreeSet<&'lock ExtraName>,
    marker: UniversalMarker,
}

impl Lock {
    /// Return the workspace members that depend on `target` in at least one valid environment.
    ///
    /// The traversal considers every production dependency, optional dependency, and dependency
    /// group on each member as a root context. Once it leaves the member, it follows production
    /// dependencies and explicitly activated extras, but not dependency groups owned by transitive
    /// workspace packages.
    ///
    /// Returns `None` if `target` does not occur in the lockfile.
    pub fn workspace_members_depending_on(
        &self,
        target: &PackageName,
    ) -> Option<BTreeSet<PackageName>> {
        if !self
            .packages
            .iter()
            .any(|package| &package.id.name == target)
        {
            return None;
        }

        let members: BTreeSet<&PackageId> = if self.members().is_empty() {
            self.root().into_iter().map(|package| &package.id).collect()
        } else {
            self.packages
                .iter()
                .filter(|package| self.members().contains(&package.id.name))
                .map(|package| &package.id)
                .collect()
        };
        let conflict_marker = UniversalMarker::new(
            MarkerTree::TRUE,
            ConflictMarker::from_conflicts(self.conflicts()),
        );

        Some(
            members
                .into_iter()
                .filter(|package_id| &package_id.name != target)
                .filter(|package_id| {
                    self.package_depends_on(self.find_by_id(package_id), target, conflict_marker)
                })
                .map(|package_id| package_id.name.clone())
                .collect(),
        )
    }

    fn package_depends_on(
        &self,
        member: &Package,
        target: &PackageName,
        conflict_marker: UniversalMarker,
    ) -> bool {
        let mut queue = VecDeque::new();
        let mut visited = FxHashSet::default();

        for dependency in member
            .dependencies
            .iter()
            .chain(member.optional_dependencies.values().flatten())
            .chain(member.dependency_groups.values().flatten())
        {
            enqueue(&mut queue, dependency, conflict_marker);
        }

        while let Some(state) = queue.pop_front() {
            if &state.package_id.name == target {
                return true;
            }
            if !visited.insert(state.clone()) {
                continue;
            }

            let package = self.find_by_id(state.package_id);
            for dependency in &package.dependencies {
                enqueue(&mut queue, dependency, state.marker);
            }
            for extra in &state.extras {
                if let Some(dependencies) = package.optional_dependencies.get(*extra) {
                    for dependency in dependencies {
                        enqueue(&mut queue, dependency, state.marker);
                    }
                }
            }
        }

        false
    }
}

fn enqueue<'lock>(
    queue: &mut VecDeque<TraversalState<'lock>>,
    dependency: &'lock Dependency,
    mut marker: UniversalMarker,
) {
    marker.and(dependency.complexified_marker);
    if marker.is_false() {
        return;
    }
    queue.push_back(TraversalState {
        package_id: &dependency.package_id,
        extras: dependency.extra.iter().collect(),
        marker,
    });
}
