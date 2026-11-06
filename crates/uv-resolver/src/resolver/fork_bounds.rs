use std::sync::Arc;

use pubgrub::{Id, State, Term};
use rustc_hash::FxHashMap;
use uv_distribution_types::IndexUrl;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pep508::MarkerTree;

use crate::dependency_provider::UvDependencyProvider;
use crate::preferences::Entry;
use crate::pubgrub::PubGrubPackage;

/// A registry release that imposed an upper bound on a dependency in a completed fork.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BoundParent {
    pub(super) name: PackageName,
    pub(super) version: Version,
    /// The explicit index requested for the parent, or the implicit registry search.
    pub(crate) index: Option<IndexUrl>,
}

/// Parent releases this fork actually upgraded after escaping a sibling's upper bound.
#[derive(Debug, Clone, Default)]
pub(crate) struct ForkUpgrades(FxHashMap<PackageName, ParentUpgrade>);

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParentUpgrade {
    id: Id<PubGrubPackage>,
    version: Version,
    previous: Vec<Arc<BoundParent>>,
}

impl ForkUpgrades {
    pub(crate) fn replaced(&self, parent: &BoundParent) -> bool {
        self.0.get(&parent.name).is_some_and(|upgrade| {
            upgrade
                .previous
                .iter()
                .any(|previous| **previous == *parent)
        })
    }

    /// Drop evidence when PubGrub backtracks past a parent's selection.
    pub(super) fn retain_decisions(&mut self, pubgrub: &State<UvDependencyProvider>) -> bool {
        let len = self.0.len();
        self.0.retain(|_, upgrade| {
            matches!(pubgrub.partial_solution.term_intersection_for_package(upgrade.id),
                Some(Term::Positive(range)) if range.as_singleton() == Some(&upgrade.version))
        });
        self.0.len() != len
    }

    /// Record which sibling releases the selected version supersedes. An upper bound
    /// that admits both versions cannot explain an upgrade or release a child's pin.
    pub(super) fn record(
        &mut self,
        name: &PackageName,
        id: Id<PubGrubPackage>,
        version: &Version,
        index: Option<&IndexUrl>,
        entries: &[Entry],
        fork: MarkerTree,
    ) -> bool {
        let mut previous = Vec::new();
        for entry in entries {
            if let Some(parent) = entry.bounded_parent(index)
                && !previous.contains(parent)
                && !entry.applies_in(fork, || Some(version), self)
            {
                previous.push(Arc::clone(parent));
            }
        }
        if previous.is_empty() {
            return self.0.remove(name).is_some();
        }
        let upgrade = ParentUpgrade {
            id,
            version: version.clone(),
            previous,
        };
        if self.0.get(name) == Some(&upgrade) {
            return false;
        }
        self.0.insert(name.clone(), upgrade);
        true
    }
}
