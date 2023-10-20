// SPDX-License-Identifier: MPL-2.0

//! An incompatibility is a set of terms for different packages
//! that should never be satisfied all together.

use std::collections::HashSet as Set;
use std::fmt;

use crate::internal::arena::{Arena, Id};
use crate::internal::small_map::SmallMap;
use crate::package::Package;
use crate::report::{DefaultStringReporter, DerivationTree, Derived, External};
use crate::term::{self, Term};
use crate::version_set::VersionSet;

/// An incompatibility is a set of terms for different packages
/// that should never be satisfied all together.
/// An incompatibility usually originates from a package dependency.
/// For example, if package A at version 1 depends on package B
/// at version 2, you can never have both terms `A = 1`
/// and `not B = 2` satisfied at the same time in a partial solution.
/// This would mean that we found a solution with package A at version 1
/// but not with package B at version 2.
/// Yet A at version 1 depends on B at version 2 so this is not possible.
/// Therefore, the set `{ A = 1, not B = 2 }` is an incompatibility,
/// defined from dependencies of A at version 1.
///
/// Incompatibilities can also be derived from two other incompatibilities
/// during conflict resolution. More about all this in
/// [PubGrub documentation](https://github.com/dart-lang/pub/blob/master/doc/solver.md#incompatibility).
#[derive(Debug, Clone)]
pub struct Incompatibility<P: Package, VS: VersionSet> {
    package_terms: SmallMap<P, Term<VS>>,
    kind: Kind<P, VS>,
}

/// Type alias of unique identifiers for incompatibilities.
pub type IncompId<P, VS> = Id<Incompatibility<P, VS>>;

#[derive(Debug, Clone)]
enum Kind<P: Package, VS: VersionSet> {
    /// Initial incompatibility aiming at picking the root package for the first decision.
    NotRoot(P, VS::V),
    /// There are no versions in the given range for this package.
    NoVersions(P, VS),
    /// Dependencies of the package are unavailable for versions in that range.
    UnavailableDependencies(P, VS),
    /// Incompatibility coming from the dependencies of a given package.
    FromDependencyOf(P, VS, P, VS),
    /// Derived from two causes. Stores cause ids.
    DerivedFrom(IncompId<P, VS>, IncompId<P, VS>),
}

/// A Relation describes how a set of terms can be compared to an incompatibility.
/// Typically, the set of terms comes from the partial solution.
#[derive(Eq, PartialEq, Debug)]
pub enum Relation<P: Package> {
    /// We say that a set of terms S satisfies an incompatibility I
    /// if S satisfies every term in I.
    Satisfied,
    /// We say that S contradicts I
    /// if S contradicts at least one term in I.
    Contradicted(P),
    /// If S satisfies all but one of I's terms and is inconclusive for the remaining term,
    /// we say S "almost satisfies" I and we call the remaining term the "unsatisfied term".
    AlmostSatisfied(P),
    /// Otherwise, we say that their relation is inconclusive.
    Inconclusive,
}

impl<P: Package, VS: VersionSet> Incompatibility<P, VS> {
    /// Create the initial "not Root" incompatibility.
    pub fn not_root(package: P, version: VS::V) -> Self {
        Self {
            package_terms: SmallMap::One([(
                package.clone(),
                Term::Negative(VS::singleton(version.clone())),
            )]),
            kind: Kind::NotRoot(package, version),
        }
    }

    /// Create an incompatibility to remember
    /// that a given set does not contain any version.
    pub fn no_versions(package: P, term: Term<VS>) -> Self {
        let set = match &term {
            Term::Positive(r) => r.clone(),
            Term::Negative(_) => panic!("No version should have a positive term"),
        };
        Self {
            package_terms: SmallMap::One([(package.clone(), term)]),
            kind: Kind::NoVersions(package, set),
        }
    }

    /// Create an incompatibility to remember
    /// that a package version is not selectable
    /// because its list of dependencies is unavailable.
    pub fn unavailable_dependencies(package: P, version: VS::V) -> Self {
        let set = VS::singleton(version);
        Self {
            package_terms: SmallMap::One([(package.clone(), Term::Positive(set.clone()))]),
            kind: Kind::UnavailableDependencies(package, set),
        }
    }

    /// Build an incompatibility from a given dependency.
    pub fn from_dependency(package: P, version: VS::V, dep: (&P, &VS)) -> Self {
        let set1 = VS::singleton(version);
        let (p2, set2) = dep;
        Self {
            package_terms: SmallMap::Two([
                (package.clone(), Term::Positive(set1.clone())),
                (p2.clone(), Term::Negative(set2.clone())),
            ]),
            kind: Kind::FromDependencyOf(package, set1, p2.clone(), set2.clone()),
        }
    }

    /// Prior cause of two incompatibilities using the rule of resolution.
    pub fn prior_cause(
        incompat: Id<Self>,
        satisfier_cause: Id<Self>,
        package: &P,
        incompatibility_store: &Arena<Self>,
    ) -> Self {
        let kind = Kind::DerivedFrom(incompat, satisfier_cause);
        let mut package_terms = incompatibility_store[incompat].package_terms.clone();
        let t1 = package_terms.remove(package).unwrap();
        let satisfier_cause_terms = &incompatibility_store[satisfier_cause].package_terms;
        package_terms.merge(
            satisfier_cause_terms.iter().filter(|(p, _)| p != &package),
            |t1, t2| Some(t1.intersection(t2)),
        );
        let term = t1.union(satisfier_cause_terms.get(package).unwrap());
        if term != Term::any() {
            package_terms.insert(package.clone(), term);
        }
        Self {
            package_terms,
            kind,
        }
    }

    /// Check if an incompatibility should mark the end of the algorithm
    /// because it satisfies the root package.
    pub fn is_terminal(&self, root_package: &P, root_version: &VS::V) -> bool {
        if self.package_terms.len() == 0 {
            true
        } else if self.package_terms.len() > 1 {
            false
        } else {
            let (package, term) = self.package_terms.iter().next().unwrap();
            (package == root_package) && term.contains(root_version)
        }
    }

    /// Get the term related to a given package (if it exists).
    pub fn get(&self, package: &P) -> Option<&Term<VS>> {
        self.package_terms.get(package)
    }

    /// Iterate over packages.
    pub fn iter(&self) -> impl Iterator<Item = (&P, &Term<VS>)> {
        self.package_terms.iter()
    }

    // Reporting ###############################################################

    /// Retrieve parent causes if of type DerivedFrom.
    pub fn causes(&self) -> Option<(Id<Self>, Id<Self>)> {
        match self.kind {
            Kind::DerivedFrom(id1, id2) => Some((id1, id2)),
            _ => None,
        }
    }

    /// Build a derivation tree for error reporting.
    pub fn build_derivation_tree(
        self_id: Id<Self>,
        shared_ids: &Set<Id<Self>>,
        store: &Arena<Self>,
    ) -> DerivationTree<P, VS> {
        match &store[self_id].kind {
            Kind::DerivedFrom(id1, id2) => {
                let cause1 = Self::build_derivation_tree(*id1, shared_ids, store);
                let cause2 = Self::build_derivation_tree(*id2, shared_ids, store);
                let derived = Derived {
                    terms: store[self_id].package_terms.as_map(),
                    shared_id: shared_ids.get(&self_id).map(|id| id.into_raw()),
                    cause1: Box::new(cause1),
                    cause2: Box::new(cause2),
                };
                DerivationTree::Derived(derived)
            }
            Kind::NotRoot(package, version) => {
                DerivationTree::External(External::NotRoot(package.clone(), version.clone()))
            }
            Kind::NoVersions(package, set) => {
                DerivationTree::External(External::NoVersions(package.clone(), set.clone()))
            }
            Kind::UnavailableDependencies(package, set) => DerivationTree::External(
                External::UnavailableDependencies(package.clone(), set.clone()),
            ),
            Kind::FromDependencyOf(package, set, dep_package, dep_set) => {
                DerivationTree::External(External::FromDependencyOf(
                    package.clone(),
                    set.clone(),
                    dep_package.clone(),
                    dep_set.clone(),
                ))
            }
        }
    }
}

impl<'a, P: Package, VS: VersionSet + 'a> Incompatibility<P, VS> {
    /// CF definition of Relation enum.
    pub fn relation(&self, terms: impl Fn(&P) -> Option<&'a Term<VS>>) -> Relation<P> {
        let mut relation = Relation::Satisfied;
        for (package, incompat_term) in self.package_terms.iter() {
            match terms(package).map(|term| incompat_term.relation_with(term)) {
                Some(term::Relation::Satisfied) => {}
                Some(term::Relation::Contradicted) => {
                    return Relation::Contradicted(package.clone());
                }
                None | Some(term::Relation::Inconclusive) => {
                    // If a package is not present, the intersection is the same as [Term::any].
                    // According to the rules of satisfactions, the relation would be inconclusive.
                    // It could also be satisfied if the incompatibility term was also [Term::any],
                    // but we systematically remove those from incompatibilities
                    // so we're safe on that front.
                    if relation == Relation::Satisfied {
                        relation = Relation::AlmostSatisfied(package.clone());
                    } else {
                        relation = Relation::Inconclusive;
                    }
                }
            }
        }
        relation
    }
}

impl<P: Package, VS: VersionSet> fmt::Display for Incompatibility<P, VS> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            DefaultStringReporter::string_terms(&self.package_terms.as_map())
        )
    }
}

// TESTS #######################################################################

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::range::Range;
    use crate::term::tests::strategy as term_strat;
    use crate::type_aliases::Map;
    use proptest::prelude::*;

    proptest! {

        /// For any three different packages p1, p2 and p3,
        /// for any three terms t1, t2 and t3,
        /// if we have the two following incompatibilities:
        ///    { p1: t1, p2: not t2 }
        ///    { p2: t2, p3: t3 }
        /// the rule of resolution says that we can deduce the following incompatibility:
        ///    { p1: t1, p3: t3 }
        #[test]
        fn rule_of_resolution(t1 in term_strat(), t2 in term_strat(), t3 in term_strat()) {
            let mut store = Arena::new();
            let i1 = store.alloc(Incompatibility {
                package_terms: SmallMap::Two([("p1", t1.clone()), ("p2", t2.negate())]),
                kind: Kind::UnavailableDependencies("0", Range::full())
            });

            let i2 = store.alloc(Incompatibility {
                package_terms: SmallMap::Two([("p2", t2), ("p3", t3.clone())]),
                kind: Kind::UnavailableDependencies("0", Range::full())
            });

            let mut i3 = Map::default();
            i3.insert("p1", t1);
            i3.insert("p3", t3);

            let i_resolution = Incompatibility::prior_cause(i1, i2, &"p2", &store);
            assert_eq!(i_resolution.package_terms.as_map(), i3);
        }

    }
}
