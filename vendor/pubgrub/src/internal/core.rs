// SPDX-License-Identifier: MPL-2.0

//! Core model and functions
//! to write a functional PubGrub algorithm.

use std::collections::HashSet as Set;

use crate::error::PubGrubError;
use crate::internal::arena::Arena;
use crate::internal::incompatibility::{IncompId, Incompatibility, Relation};
use crate::internal::partial_solution::SatisfierSearch::{
    DifferentDecisionLevels, SameDecisionLevels,
};
use crate::internal::partial_solution::{DecisionLevel, PartialSolution};
use crate::internal::small_vec::SmallVec;
use crate::package::Package;
use crate::report::DerivationTree;
use crate::type_aliases::Map;
use crate::version_set::VersionSet;

/// Current state of the PubGrub algorithm.
#[derive(Clone)]
pub struct State<P: Package, VS: VersionSet, Priority: Ord + Clone> {
    root_package: P,
    root_version: VS::V,

    incompatibilities: Map<P, Vec<IncompId<P, VS>>>,

    /// Store the ids of incompatibilities that are already contradicted
    /// and will stay that way until the next conflict and backtrack is operated.
    contradicted_incompatibilities: rustc_hash::FxHashSet<IncompId<P, VS>>,

    /// Partial solution.
    /// TODO: remove pub.
    pub partial_solution: PartialSolution<P, VS, Priority>,

    /// The store is the reference storage for all incompatibilities.
    pub incompatibility_store: Arena<Incompatibility<P, VS>>,

    /// This is a stack of work to be done in `unit_propagation`.
    /// It can definitely be a local variable to that method, but
    /// this way we can reuse the same allocation for better performance.
    unit_propagation_buffer: SmallVec<P>,
}

impl<P: Package, VS: VersionSet, Priority: Ord + Clone> State<P, VS, Priority> {
    /// Initialization of PubGrub state.
    pub fn init(root_package: P, root_version: VS::V) -> Self {
        let mut incompatibility_store = Arena::new();
        let not_root_id = incompatibility_store.alloc(Incompatibility::not_root(
            root_package.clone(),
            root_version.clone(),
        ));
        let mut incompatibilities = Map::default();
        incompatibilities.insert(root_package.clone(), vec![not_root_id]);
        Self {
            root_package,
            root_version,
            incompatibilities,
            contradicted_incompatibilities: rustc_hash::FxHashSet::default(),
            partial_solution: PartialSolution::empty(),
            incompatibility_store,
            unit_propagation_buffer: SmallVec::Empty,
        }
    }

    /// Add an incompatibility to the state.
    pub fn add_incompatibility(&mut self, incompat: Incompatibility<P, VS>) {
        let id = self.incompatibility_store.alloc(incompat);
        self.merge_incompatibility(id);
    }

    /// Add an incompatibility to the state.
    pub fn add_incompatibility_from_dependencies(
        &mut self,
        package: P,
        version: VS::V,
        deps: impl IntoIterator<Item = (P, VS)>,
    ) -> std::ops::Range<IncompId<P, VS>> {
        // Create incompatibilities and allocate them in the store.
        let new_incompats_id_range = self
            .incompatibility_store
            .alloc_iter(deps.into_iter().map(|dep| {
                Incompatibility::from_dependency(package.clone(), version.clone(), dep)
            }));
        // Merge the newly created incompatibilities with the older ones.
        for id in IncompId::range_to_iter(new_incompats_id_range.clone()) {
            self.merge_incompatibility(id);
        }
        new_incompats_id_range
    }

    /// Unit propagation is the core mechanism of the solving algorithm.
    /// CF <https://github.com/dart-lang/pub/blob/master/doc/solver.md#unit-propagation>
    pub fn unit_propagation(&mut self, package: P) -> Result<(), PubGrubError<P, VS>> {
        self.unit_propagation_buffer.clear();
        self.unit_propagation_buffer.push(package);
        while let Some(current_package) = self.unit_propagation_buffer.pop() {
            // Iterate over incompatibilities in reverse order
            // to evaluate first the newest incompatibilities.
            let mut conflict_id = None;
            // We only care about incompatibilities if it contains the current package.
            for &incompat_id in self.incompatibilities[&current_package].iter().rev() {
                if self.contradicted_incompatibilities.contains(&incompat_id) {
                    continue;
                }
                let current_incompat = &self.incompatibility_store[incompat_id];
                match self.partial_solution.relation(current_incompat) {
                    // If the partial solution satisfies the incompatibility
                    // we must perform conflict resolution.
                    Relation::Satisfied => {
                        log::info!(
                            "Start conflict resolution because incompat satisfied:\n   {}",
                            current_incompat
                        );
                        conflict_id = Some(incompat_id);
                        break;
                    }
                    Relation::AlmostSatisfied(package_almost) => {
                        self.unit_propagation_buffer.push(package_almost.clone());
                        // Add (not term) to the partial solution with incompat as cause.
                        self.partial_solution.add_derivation(
                            package_almost,
                            incompat_id,
                            &self.incompatibility_store,
                        );
                        // With the partial solution updated, the incompatibility is now contradicted.
                        self.contradicted_incompatibilities.insert(incompat_id);
                    }
                    Relation::Contradicted(_) => {
                        self.contradicted_incompatibilities.insert(incompat_id);
                    }
                    _ => {}
                }
            }
            if let Some(incompat_id) = conflict_id {
                let (package_almost, root_cause) = self.conflict_resolution(incompat_id)?;
                self.unit_propagation_buffer.clear();
                self.unit_propagation_buffer.push(package_almost.clone());
                // Add to the partial solution with incompat as cause.
                self.partial_solution.add_derivation(
                    package_almost,
                    root_cause,
                    &self.incompatibility_store,
                );
                // After conflict resolution and the partial solution update,
                // the root cause incompatibility is now contradicted.
                self.contradicted_incompatibilities.insert(root_cause);
            }
        }
        // If there are no more changed packages, unit propagation is done.
        Ok(())
    }

    /// Return the root cause and the backtracked model.
    /// CF <https://github.com/dart-lang/pub/blob/master/doc/solver.md#unit-propagation>
    fn conflict_resolution(
        &mut self,
        incompatibility: IncompId<P, VS>,
    ) -> Result<(P, IncompId<P, VS>), PubGrubError<P, VS>> {
        let mut current_incompat_id = incompatibility;
        let mut current_incompat_changed = false;
        loop {
            if self.incompatibility_store[current_incompat_id]
                .is_terminal(&self.root_package, &self.root_version)
            {
                return Err(PubGrubError::NoSolution(
                    self.build_derivation_tree(current_incompat_id),
                ));
            } else {
                let (package, satisfier_search_result) = self.partial_solution.satisfier_search(
                    &self.incompatibility_store[current_incompat_id],
                    &self.incompatibility_store,
                );
                match satisfier_search_result {
                    DifferentDecisionLevels {
                        previous_satisfier_level,
                    } => {
                        self.backtrack(
                            current_incompat_id,
                            current_incompat_changed,
                            previous_satisfier_level,
                        );
                        log::info!("backtrack to {:?}", previous_satisfier_level);
                        return Ok((package, current_incompat_id));
                    }
                    SameDecisionLevels { satisfier_cause } => {
                        let prior_cause = Incompatibility::prior_cause(
                            current_incompat_id,
                            satisfier_cause,
                            &package,
                            &self.incompatibility_store,
                        );
                        log::info!("prior cause: {}", prior_cause);
                        current_incompat_id = self.incompatibility_store.alloc(prior_cause);
                        current_incompat_changed = true;
                    }
                }
            }
        }
    }

    /// Backtracking.
    fn backtrack(
        &mut self,
        incompat: IncompId<P, VS>,
        incompat_changed: bool,
        decision_level: DecisionLevel,
    ) {
        self.partial_solution
            .backtrack(decision_level, &self.incompatibility_store);
        self.contradicted_incompatibilities.clear();
        if incompat_changed {
            self.merge_incompatibility(incompat);
        }
    }

    /// Add this incompatibility into the set of all incompatibilities.
    ///
    /// Pub collapses identical dependencies from adjacent package versions
    /// into individual incompatibilities.
    /// This substantially reduces the total number of incompatibilities
    /// and makes it much easier for Pub to reason about multiple versions of packages at once.
    ///
    /// For example, rather than representing
    /// foo 1.0.0 depends on bar ^1.0.0 and
    /// foo 1.1.0 depends on bar ^1.0.0
    /// as two separate incompatibilities,
    /// they are collapsed together into the single incompatibility {foo ^1.0.0, not bar ^1.0.0}
    /// (provided that no other version of foo exists between 1.0.0 and 2.0.0).
    /// We could collapse them into { foo (1.0.0 ∪ 1.1.0), not bar ^1.0.0 }
    /// without having to check the existence of other versions though.
    ///
    /// Here we do the simple stupid thing of just growing the Vec.
    /// It may not be trivial since those incompatibilities
    /// may already have derived others.
    fn merge_incompatibility(&mut self, id: IncompId<P, VS>) {
        for (pkg, term) in self.incompatibility_store[id].iter() {
            if cfg!(debug_assertions) {
                assert_ne!(term, &crate::term::Term::any());
            }
            self.incompatibilities
                .entry(pkg.clone())
                .or_default()
                .push(id);
        }
    }

    // Error reporting #########################################################

    fn build_derivation_tree(&self, incompat: IncompId<P, VS>) -> DerivationTree<P, VS> {
        let shared_ids = self.find_shared_ids(incompat);
        Incompatibility::build_derivation_tree(incompat, &shared_ids, &self.incompatibility_store)
    }

    fn find_shared_ids(&self, incompat: IncompId<P, VS>) -> Set<IncompId<P, VS>> {
        let mut all_ids = Set::new();
        let mut shared_ids = Set::new();
        let mut stack = vec![incompat];
        while let Some(i) = stack.pop() {
            if let Some((id1, id2)) = self.incompatibility_store[i].causes() {
                if all_ids.contains(&i) {
                    shared_ids.insert(i);
                } else {
                    all_ids.insert(i);
                    stack.push(id1);
                    stack.push(id2);
                }
            }
        }
        shared_ids
    }
}
