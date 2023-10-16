// SPDX-License-Identifier: MPL-2.0

//! A Memory acts like a structured partial solution
//! where terms are regrouped by package in a [Map](crate::type_aliases::Map).

use crate::internal::arena::Arena;
use crate::internal::incompatibility::{IncompId, Incompatibility, Relation};
use crate::internal::small_map::SmallMap;
use crate::package::Package;
use crate::range::Range;
use crate::term::Term;
use crate::type_aliases::{Map, SelectedDependencies};
use crate::version::Version;

use super::small_vec::SmallVec;

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct DecisionLevel(pub u32);

impl DecisionLevel {
    pub fn increment(self) -> Self {
        Self(self.0 + 1)
    }
}

/// The partial solution contains all package assignments,
/// organized by package and historically ordered.
#[derive(Clone, Debug)]
pub struct PartialSolution<P: Package, V: Version> {
    next_global_index: u32,
    current_decision_level: DecisionLevel,
    package_assignments: Map<P, PackageAssignments<P, V>>,
}

/// Package assignments contain the potential decision and derivations
/// that have already been made for a given package,
/// as well as the intersection of terms by all of these.
#[derive(Clone, Debug)]
struct PackageAssignments<P: Package, V: Version> {
    smallest_decision_level: DecisionLevel,
    highest_decision_level: DecisionLevel,
    dated_derivations: SmallVec<DatedDerivation<P, V>>,
    assignments_intersection: AssignmentsIntersection<V>,
}

#[derive(Clone, Debug)]
pub struct DatedDerivation<P: Package, V: Version> {
    global_index: u32,
    decision_level: DecisionLevel,
    cause: IncompId<P, V>,
}

#[derive(Clone, Debug)]
enum AssignmentsIntersection<V: Version> {
    Decision((u32, V, Term<V>)),
    Derivations(Term<V>),
}

#[derive(Clone, Debug)]
pub enum SatisfierSearch<P: Package, V: Version> {
    DifferentDecisionLevels {
        previous_satisfier_level: DecisionLevel,
    },
    SameDecisionLevels {
        satisfier_cause: IncompId<P, V>,
    },
}

impl<P: Package, V: Version> PartialSolution<P, V> {
    /// Initialize an empty PartialSolution.
    pub fn empty() -> Self {
        Self {
            next_global_index: 0,
            current_decision_level: DecisionLevel(0),
            package_assignments: Map::default(),
        }
    }

    /// Add a decision.
    pub fn add_decision(&mut self, package: P, version: V) {
        // Check that add_decision is never used in the wrong context.
        if cfg!(debug_assertions) {
            match self.package_assignments.get_mut(&package) {
                None => panic!("Derivations must already exist"),
                Some(pa) => match &pa.assignments_intersection {
                    // Cannot be called when a decision has already been taken.
                    AssignmentsIntersection::Decision(_) => panic!("Already existing decision"),
                    // Cannot be called if the versions is not contained in the terms intersection.
                    AssignmentsIntersection::Derivations(term) => {
                        debug_assert!(term.contains(&version))
                    }
                },
            }
        }
        self.current_decision_level = self.current_decision_level.increment();
        let mut pa = self
            .package_assignments
            .get_mut(&package)
            .expect("Derivations must already exist");
        pa.highest_decision_level = self.current_decision_level;
        pa.assignments_intersection = AssignmentsIntersection::Decision((
            self.next_global_index,
            version.clone(),
            Term::exact(version),
        ));
        self.next_global_index += 1;
    }

    /// Add a derivation.
    pub fn add_derivation(
        &mut self,
        package: P,
        cause: IncompId<P, V>,
        store: &Arena<Incompatibility<P, V>>,
    ) {
        use std::collections::hash_map::Entry;
        let term = store[cause].get(&package).unwrap().negate();
        let dated_derivation = DatedDerivation {
            global_index: self.next_global_index,
            decision_level: self.current_decision_level,
            cause,
        };
        self.next_global_index += 1;
        match self.package_assignments.entry(package) {
            Entry::Occupied(mut occupied) => {
                let mut pa = occupied.get_mut();
                pa.highest_decision_level = self.current_decision_level;
                match &mut pa.assignments_intersection {
                    // Check that add_derivation is never called in the wrong context.
                    AssignmentsIntersection::Decision(_) => {
                        panic!("add_derivation should not be called after a decision")
                    }
                    AssignmentsIntersection::Derivations(t) => {
                        *t = t.intersection(&term);
                    }
                }
                pa.dated_derivations.push(dated_derivation);
            }
            Entry::Vacant(v) => {
                v.insert(PackageAssignments {
                    smallest_decision_level: self.current_decision_level,
                    highest_decision_level: self.current_decision_level,
                    dated_derivations: SmallVec::One([dated_derivation]),
                    assignments_intersection: AssignmentsIntersection::Derivations(term),
                });
            }
        }
    }

    /// Extract potential packages for the next iteration of unit propagation.
    /// Return `None` if there is no suitable package anymore, which stops the algorithm.
    /// A package is a potential pick if there isn't an already
    /// selected version (no "decision")
    /// and if it contains at least one positive derivation term
    /// in the partial solution.
    pub fn potential_packages(&self) -> Option<impl Iterator<Item = (&P, &Range<V>)>> {
        let mut iter = self
            .package_assignments
            .iter()
            .filter_map(|(p, pa)| pa.assignments_intersection.potential_package_filter(p))
            .peekable();
        if iter.peek().is_some() {
            Some(iter)
        } else {
            None
        }
    }

    /// If a partial solution has, for every positive derivation,
    /// a corresponding decision that satisfies that assignment,
    /// it's a total solution and version solving has succeeded.
    pub fn extract_solution(&self) -> Option<SelectedDependencies<P, V>> {
        let mut solution = Map::default();
        for (p, pa) in &self.package_assignments {
            match &pa.assignments_intersection {
                AssignmentsIntersection::Decision((_, v, _)) => {
                    solution.insert(p.clone(), v.clone());
                }
                AssignmentsIntersection::Derivations(term) => {
                    if term.is_positive() {
                        return None;
                    }
                }
            }
        }
        Some(solution)
    }

    /// Backtrack the partial solution to a given decision level.
    pub fn backtrack(
        &mut self,
        decision_level: DecisionLevel,
        store: &Arena<Incompatibility<P, V>>,
    ) {
        self.current_decision_level = decision_level;
        self.package_assignments.retain(|p, pa| {
            if pa.smallest_decision_level > decision_level {
                // Remove all entries that have a smallest decision level higher than the backtrack target.
                false
            } else if pa.highest_decision_level <= decision_level {
                // Do not change entries older than the backtrack decision level target.
                true
            } else {
                // smallest_decision_level <= decision_level < highest_decision_level
                //
                // Since decision_level < highest_decision_level,
                // We can be certain that there will be no decision in this package assignments
                // after backtracking, because such decision would have been the last
                // assignment and it would have the "highest_decision_level".

                // Truncate the history.
                while pa.dated_derivations.last().map(|dd| dd.decision_level) > Some(decision_level)
                {
                    pa.dated_derivations.pop();
                }
                debug_assert!(!pa.dated_derivations.is_empty());

                // Update highest_decision_level.
                pa.highest_decision_level = pa.dated_derivations.last().unwrap().decision_level;

                // Recompute the assignments intersection.
                pa.assignments_intersection = AssignmentsIntersection::Derivations(
                    pa.dated_derivations
                        .iter()
                        .fold(Term::any(), |acc, dated_derivation| {
                            let term = store[dated_derivation.cause].get(&p).unwrap().negate();
                            acc.intersection(&term)
                        }),
                );
                true
            }
        });
    }

    /// We can add the version to the partial solution as a decision
    /// if it doesn't produce any conflict with the new incompatibilities.
    /// In practice I think it can only produce a conflict if one of the dependencies
    /// (which are used to make the new incompatibilities)
    /// is already in the partial solution with an incompatible version.
    pub fn add_version(
        &mut self,
        package: P,
        version: V,
        new_incompatibilities: std::ops::Range<IncompId<P, V>>,
        store: &Arena<Incompatibility<P, V>>,
    ) {
        let exact = Term::exact(version.clone());
        let not_satisfied = |incompat: &Incompatibility<P, V>| {
            incompat.relation(|p| {
                if p == &package {
                    Some(&exact)
                } else {
                    self.term_intersection_for_package(p)
                }
            }) != Relation::Satisfied
        };

        // Check none of the dependencies (new_incompatibilities)
        // would create a conflict (be satisfied).
        if store[new_incompatibilities].iter().all(not_satisfied) {
            self.add_decision(package, version);
        }
    }

    /// Check if the terms in the partial solution satisfy the incompatibility.
    pub fn relation(&self, incompat: &Incompatibility<P, V>) -> Relation<P> {
        incompat.relation(|package| self.term_intersection_for_package(package))
    }

    /// Retrieve intersection of terms related to package.
    pub fn term_intersection_for_package(&self, package: &P) -> Option<&Term<V>> {
        self.package_assignments
            .get(package)
            .map(|pa| pa.assignments_intersection.term())
    }

    /// Figure out if the satisfier and previous satisfier are of different decision levels.
    pub fn satisfier_search(
        &self,
        incompat: &Incompatibility<P, V>,
        store: &Arena<Incompatibility<P, V>>,
    ) -> (P, SatisfierSearch<P, V>) {
        let satisfied_map = Self::find_satisfier(incompat, &self.package_assignments, store);
        let (satisfier_package, &(satisfier_index, _, satisfier_decision_level)) = satisfied_map
            .iter()
            .max_by_key(|(_p, (_, global_index, _))| global_index)
            .unwrap();
        let satisfier_package = satisfier_package.clone();
        let previous_satisfier_level = Self::find_previous_satisfier(
            incompat,
            &satisfier_package,
            satisfied_map,
            &self.package_assignments,
            store,
        );
        if previous_satisfier_level < satisfier_decision_level {
            let search_result = SatisfierSearch::DifferentDecisionLevels {
                previous_satisfier_level,
            };
            (satisfier_package, search_result)
        } else {
            let satisfier_pa = self.package_assignments.get(&satisfier_package).unwrap();
            let dd = &satisfier_pa.dated_derivations[satisfier_index];
            let search_result = SatisfierSearch::SameDecisionLevels {
                satisfier_cause: dd.cause,
            };
            (satisfier_package, search_result)
        }
    }

    /// A satisfier is the earliest assignment in partial solution such that the incompatibility
    /// is satisfied by the partial solution up to and including that assignment.
    ///
    /// Returns a map indicating for each package term, when that was first satisfied in history.
    /// If we effectively found a satisfier, the returned map must be the same size that incompat.
    ///
    /// Question: This is possible since we added a "global_index" to every dated_derivation.
    /// It would be nice if we could get rid of it, but I don't know if then it will be possible
    /// to return a coherent previous_satisfier_level.
    fn find_satisfier(
        incompat: &Incompatibility<P, V>,
        package_assignments: &Map<P, PackageAssignments<P, V>>,
        store: &Arena<Incompatibility<P, V>>,
    ) -> SmallMap<P, (usize, u32, DecisionLevel)> {
        let mut satisfied = SmallMap::Empty;
        for (package, incompat_term) in incompat.iter() {
            let pa = package_assignments.get(package).expect("Must exist");
            satisfied.insert(
                package.clone(),
                pa.satisfier(package, incompat_term, Term::any(), store),
            );
        }
        satisfied
    }

    /// Earliest assignment in the partial solution before satisfier
    /// such that incompatibility is satisfied by the partial solution up to
    /// and including that assignment plus satisfier.
    fn find_previous_satisfier(
        incompat: &Incompatibility<P, V>,
        satisfier_package: &P,
        mut satisfied_map: SmallMap<P, (usize, u32, DecisionLevel)>,
        package_assignments: &Map<P, PackageAssignments<P, V>>,
        store: &Arena<Incompatibility<P, V>>,
    ) -> DecisionLevel {
        // First, let's retrieve the previous derivations and the initial accum_term.
        let satisfier_pa = package_assignments.get(satisfier_package).unwrap();
        let (satisfier_index, _gidx, _dl) = satisfied_map.get_mut(satisfier_package).unwrap();

        let accum_term = if *satisfier_index == satisfier_pa.dated_derivations.len() {
            match &satisfier_pa.assignments_intersection {
                AssignmentsIntersection::Derivations(_) => panic!("must be a decision"),
                AssignmentsIntersection::Decision((_, _, term)) => term.clone(),
            }
        } else {
            let dd = &satisfier_pa.dated_derivations[*satisfier_index];
            store[dd.cause].get(satisfier_package).unwrap().negate()
        };

        let incompat_term = incompat
            .get(satisfier_package)
            .expect("satisfier package not in incompat");

        satisfied_map.insert(
            satisfier_package.clone(),
            satisfier_pa.satisfier(satisfier_package, incompat_term, accum_term, store),
        );

        // Finally, let's identify the decision level of that previous satisfier.
        let (_, &(_, _, decision_level)) = satisfied_map
            .iter()
            .max_by_key(|(_p, (_, global_index, _))| global_index)
            .unwrap();
        decision_level.max(DecisionLevel(1))
    }
}

impl<P: Package, V: Version> PackageAssignments<P, V> {
    fn satisfier(
        &self,
        package: &P,
        incompat_term: &Term<V>,
        start_term: Term<V>,
        store: &Arena<Incompatibility<P, V>>,
    ) -> (usize, u32, DecisionLevel) {
        // Term where we accumulate intersections until incompat_term is satisfied.
        let mut accum_term = start_term;
        // Indicate if we found a satisfier in the list of derivations, otherwise it will be the decision.
        for (idx, dated_derivation) in self.dated_derivations.iter().enumerate() {
            let this_term = store[dated_derivation.cause].get(package).unwrap().negate();
            accum_term = accum_term.intersection(&this_term);
            if accum_term.subset_of(incompat_term) {
                // We found the derivation causing satisfaction.
                return (
                    idx,
                    dated_derivation.global_index,
                    dated_derivation.decision_level,
                );
            }
        }
        // If it wasn't found in the derivations,
        // it must be the decision which is last (if called in the right context).
        match self.assignments_intersection {
            AssignmentsIntersection::Decision((global_index, _, _)) => (
                self.dated_derivations.len(),
                global_index,
                self.highest_decision_level,
            ),
            AssignmentsIntersection::Derivations(_) => {
                panic!("This must be a decision")
            }
        }
    }
}

impl<V: Version> AssignmentsIntersection<V> {
    /// Returns the term intersection of all assignments (decision included).
    fn term(&self) -> &Term<V> {
        match self {
            Self::Decision((_, _, term)) => term,
            Self::Derivations(term) => term,
        }
    }

    /// A package is a potential pick if there isn't an already
    /// selected version (no "decision")
    /// and if it contains at least one positive derivation term
    /// in the partial solution.
    fn potential_package_filter<'a, P: Package>(
        &'a self,
        package: &'a P,
    ) -> Option<(&'a P, &'a Range<V>)> {
        match self {
            Self::Decision(_) => None,
            Self::Derivations(term_intersection) => {
                if term_intersection.is_positive() {
                    Some((package, term_intersection.unwrap_positive()))
                } else {
                    None
                }
            }
        }
    }
}
