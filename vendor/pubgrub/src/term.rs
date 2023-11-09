// SPDX-License-Identifier: MPL-2.0

//! A term is the fundamental unit of operation of the PubGrub algorithm.
//! It is a positive or negative expression regarding a set of versions.

use crate::version_set::VersionSet;
use std::fmt::{self, Display};

///  A positive or negative expression regarding a set of versions.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Term<VS: VersionSet> {
    /// For example, "1.0.0 <= v < 2.0.0" is a positive expression
    /// that is evaluated true if a version is selected
    /// and comprised between version 1.0.0 and version 2.0.0.
    Positive(VS),
    /// The term "not v < 3.0.0" is a negative expression
    /// that is evaluated true if a version is selected >= 3.0.0
    /// or if no version is selected at all.
    Negative(VS),
}

/// Base methods.
impl<VS: VersionSet> Term<VS> {
    /// A term that is always true.
    pub(crate) fn any() -> Self {
        Self::Negative(VS::empty())
    }

    /// A term that is never true.
    pub(crate) fn empty() -> Self {
        Self::Positive(VS::empty())
    }

    /// A positive term containing exactly that version.
    pub(crate) fn exact(version: VS::V) -> Self {
        Self::Positive(VS::singleton(version))
    }

    /// Simply check if a term is positive.
    pub(crate) fn is_positive(&self) -> bool {
        match self {
            Self::Positive(_) => true,
            Self::Negative(_) => false,
        }
    }

    /// Negate a term.
    /// Evaluation of a negated term always returns
    /// the opposite of the evaluation of the original one.
    pub(crate) fn negate(&self) -> Self {
        match self {
            Self::Positive(set) => Self::Negative(set.clone()),
            Self::Negative(set) => Self::Positive(set.clone()),
        }
    }

    /// Evaluate a term regarding a given choice of version.
    pub(crate) fn contains(&self, v: &VS::V) -> bool {
        match self {
            Self::Positive(set) => set.contains(v),
            Self::Negative(set) => !(set.contains(v)),
        }
    }

    /// Unwrap the set contained in a positive term.
    /// Will panic if used on a negative set.
    pub(crate) fn unwrap_positive(&self) -> &VS {
        match self {
            Self::Positive(set) => set,
            _ => panic!("Negative term cannot unwrap positive set"),
        }
    }
}

/// Set operations with terms.
impl<VS: VersionSet> Term<VS> {
    /// Compute the intersection of two terms.
    /// If at least one term is positive, the intersection is also positive.
    pub(crate) fn intersection(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Positive(r1), Self::Positive(r2)) => Self::Positive(r1.intersection(r2)),
            (Self::Positive(r1), Self::Negative(r2)) => {
                Self::Positive(r1.intersection(&r2.complement()))
            }
            (Self::Negative(r1), Self::Positive(r2)) => {
                Self::Positive(r1.complement().intersection(r2))
            }
            (Self::Negative(r1), Self::Negative(r2)) => Self::Negative(r1.union(r2)),
        }
    }

    /// Compute the union of two terms.
    /// If at least one term is negative, the union is also negative.
    pub(crate) fn union(&self, other: &Self) -> Self {
        (self.negate().intersection(&other.negate())).negate()
    }

    /// Indicate if this term is a subset of another term.
    /// Just like for sets, we say that t1 is a subset of t2
    /// if and only if t1 ∩ t2 = t1.
    pub(crate) fn subset_of(&self, other: &Self) -> bool {
        self == &self.intersection(other)
    }
}

/// Describe a relation between a set of terms S and another term t.
///
/// As a shorthand, we say that a term v
/// satisfies or contradicts a term t if {v} satisfies or contradicts it.
pub(crate) enum Relation {
    /// We say that a set of terms S "satisfies" a term t
    /// if t must be true whenever every term in S is true.
    Satisfied,
    /// Conversely, S "contradicts" t if t must be false
    /// whenever every term in S is true.
    Contradicted,
    /// If neither of these is true we say that S is "inconclusive" for t.
    Inconclusive,
}

/// Relation between terms.
impl<VS: VersionSet> Term<VS> {
    /// Check if a set of terms satisfies this term.
    ///
    /// We say that a set of terms S "satisfies" a term t
    /// if t must be true whenever every term in S is true.
    ///
    /// It turns out that this can also be expressed with set operations:
    ///    S satisfies t if and only if  ⋂ S ⊆ t
    #[cfg(test)]
    fn satisfied_by(&self, terms_intersection: &Self) -> bool {
        terms_intersection.subset_of(self)
    }

    /// Check if a set of terms contradicts this term.
    ///
    /// We say that a set of terms S "contradicts" a term t
    /// if t must be false whenever every term in S is true.
    ///
    /// It turns out that this can also be expressed with set operations:
    ///    S contradicts t if and only if ⋂ S is disjoint with t
    ///    S contradicts t if and only if  (⋂ S) ⋂ t = ∅
    #[cfg(test)]
    fn contradicted_by(&self, terms_intersection: &Self) -> bool {
        terms_intersection.intersection(self) == Self::empty()
    }

    /// Check if a set of terms satisfies or contradicts a given term.
    /// Otherwise the relation is inconclusive.
    pub(crate) fn relation_with(&self, other_terms_intersection: &Self) -> Relation {
        let full_intersection = self.intersection(other_terms_intersection);
        if &full_intersection == other_terms_intersection {
            Relation::Satisfied
        } else if full_intersection == Self::empty() {
            Relation::Contradicted
        } else {
            Relation::Inconclusive
        }
    }
}

impl<VS: VersionSet> AsRef<Self> for Term<VS> {
    fn as_ref(&self) -> &Self {
        self
    }
}

// REPORT ######################################################################

impl<VS: VersionSet + Display> Display for Term<VS> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Positive(set) => write!(f, "{}", set),
            Self::Negative(set) => write!(f, "Not ( {} )", set),
        }
    }
}

// TESTS #######################################################################

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::range::Range;
    use proptest::prelude::*;

    pub fn strategy() -> impl Strategy<Value = Term<Range<u32>>> {
        prop_oneof![
            crate::range::tests::strategy().prop_map(Term::Positive),
            crate::range::tests::strategy().prop_map(Term::Negative),
        ]
    }

    proptest! {

        // Testing relation --------------------------------

        #[test]
        fn relation_with(term1 in strategy(), term2 in strategy()) {
            match term1.relation_with(&term2) {
                Relation::Satisfied => assert!(term1.satisfied_by(&term2)),
                Relation::Contradicted => assert!(term1.contradicted_by(&term2)),
                Relation::Inconclusive => {
                    assert!(!term1.satisfied_by(&term2));
                    assert!(!term1.contradicted_by(&term2));
                }
            }
        }

    }
}
