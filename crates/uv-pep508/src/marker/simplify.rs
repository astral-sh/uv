use std::fmt;
use std::ops::Bound;

use arcstr::ArcStr;
use indexmap::{IndexMap, IndexSet};
use itertools::Itertools;
use rustc_hash::FxBuildHasher;
use smallvec::SmallVec;
use version_ranges::Ranges;

use uv_pep440::{Version, VersionSpecifier};

use crate::marker::tree::ContainerOperator;
use crate::{ExtraOperator, MarkerExpression, MarkerOperator, MarkerTree, MarkerTreeKind};

/// Returns a simplified DNF expression for a given marker tree.
///
/// Marker trees are represented as decision diagrams that cannot be directly serialized to.
/// a boolean expression. Instead, you must traverse and collect all possible solutions to the
/// diagram, which can be used to create a DNF expression, or all non-solutions to the diagram,
/// which can be used to create a CNF expression.
///
/// We choose DNF as it is easier to simplify for user-facing output.
pub(crate) fn to_dnf(tree: MarkerTree) -> Vec<Vec<MarkerExpression>> {
    let mut dnf = Vec::new();
    collect_dnf(tree, &mut dnf, &mut Vec::new());
    simplify(&mut dnf);
    sort(&mut dnf);
    dnf
}

/// Walk a [`MarkerTree`] recursively and construct a DNF expression.
///
/// A decision diagram can be converted to DNF form by performing a depth-first traversal of
/// the tree and collecting all paths to a `true` terminal node.
///
/// `path` is the list of marker expressions traversed on the current path.
fn collect_dnf(
    tree: MarkerTree,
    dnf: &mut Vec<Vec<MarkerExpression>>,
    path: &mut Vec<MarkerExpression>,
) {
    match tree.kind() {
        // Reached a `false` node, meaning the conjunction is irrelevant for DNF.
        MarkerTreeKind::False => {}
        // Reached a solution, store the conjunction.
        MarkerTreeKind::True => {
            if !path.is_empty() {
                dnf.push(path.clone());
            }
        }
        MarkerTreeKind::Version(marker) => {
            for (tree, range) in collect_edges(marker.edges()) {
                // Detect whether the range for this edge can be simplified as an inequality.
                if let Some(excluded) = range_inequality(&range) {
                    let current = path.len();
                    for version in excluded {
                        path.push(MarkerExpression::Version {
                            key: marker.key().into(),
                            specifier: VersionSpecifier::not_equals_version(version.clone()),
                        });
                    }

                    collect_dnf(tree, dnf, path);
                    path.truncate(current);
                    continue;
                }

                // Detect whether the range for this edge can be simplified as a star specifier.
                if let Some(specifier) = star_range_specifier(&range) {
                    path.push(MarkerExpression::Version {
                        key: marker.key().into(),
                        specifier,
                    });

                    collect_dnf(tree, dnf, path);
                    path.pop();
                    continue;
                }

                for bounds in range.iter() {
                    let current = path.len();
                    for specifier in VersionSpecifier::from_release_only_bounds(bounds) {
                        path.push(MarkerExpression::Version {
                            key: marker.key().into(),
                            specifier,
                        });
                    }

                    collect_dnf(tree, dnf, path);
                    path.truncate(current);
                }
            }
        }
        MarkerTreeKind::VersionString(marker) => {
            for (tree, range) in collect_edges(marker.edges()) {
                for (lower, upper) in range.iter() {
                    let current = path.len();
                    let lower = lower.map(|version| ArcStr::from(version.to_string()));
                    let upper = upper.map(|version| ArcStr::from(version.to_string()));
                    for (operator, value) in
                        MarkerOperator::from_bounds((lower.as_ref(), upper.as_ref()))
                    {
                        path.push(MarkerExpression::String {
                            key: marker.key().into(),
                            operator,
                            value,
                        });
                    }
                    collect_dnf(tree, dnf, path);
                    path.truncate(current);
                }
            }
        }
        MarkerTreeKind::String(marker) => {
            for (tree, range) in collect_edges(marker.children()) {
                // Detect whether the range for this edge can be simplified as an inequality.
                if let Some(excluded) = range_inequality(&range) {
                    let current = path.len();
                    for value in excluded {
                        path.push(MarkerExpression::String {
                            key: marker.key().into(),
                            operator: MarkerOperator::NotEqual,
                            value: value.clone(),
                        });
                    }

                    collect_dnf(tree, dnf, path);
                    path.truncate(current);
                    continue;
                }

                for bounds in range.iter() {
                    let current = path.len();
                    for (operator, value) in MarkerOperator::from_bounds(bounds) {
                        path.push(MarkerExpression::String {
                            key: marker.key().into(),
                            operator,
                            value: value.clone(),
                        });
                    }

                    collect_dnf(tree, dnf, path);
                    path.truncate(current);
                }
            }
        }
        MarkerTreeKind::In(marker) => {
            for (value, tree) in marker.children() {
                let operator = if value {
                    MarkerOperator::In
                } else {
                    MarkerOperator::NotIn
                };

                let expr = MarkerExpression::String {
                    key: marker.key().into(),
                    value: ArcStr::from(marker.value()),
                    operator,
                };

                path.push(expr);
                collect_dnf(tree, dnf, path);
                path.pop();
            }
        }
        MarkerTreeKind::Contains(marker) => {
            for (value, tree) in marker.children() {
                let operator = if value {
                    MarkerOperator::Contains
                } else {
                    MarkerOperator::NotContains
                };

                let expr = MarkerExpression::String {
                    key: marker.key().into(),
                    value: ArcStr::from(marker.value()),
                    operator,
                };

                path.push(expr);
                collect_dnf(tree, dnf, path);
                path.pop();
            }
        }
        MarkerTreeKind::List(marker) => {
            for (is_high, tree) in marker.children() {
                let expr = MarkerExpression::List {
                    pair: marker.pair().clone(),
                    operator: if is_high {
                        ContainerOperator::In
                    } else {
                        ContainerOperator::NotIn
                    },
                };

                path.push(expr);
                collect_dnf(tree, dnf, path);
                path.pop();
            }
        }
        MarkerTreeKind::Extra(marker) => {
            for (value, tree) in marker.children() {
                let operator = if value {
                    ExtraOperator::Equal
                } else {
                    ExtraOperator::NotEqual
                };

                let expr = MarkerExpression::Extra {
                    name: marker.name().clone().into(),
                    operator,
                };

                path.push(expr);
                collect_dnf(tree, dnf, path);
                path.pop();
            }
        }
    }
}

/// Simplifies a DNF expression.
///
/// A decision diagram is canonical, but only for a given variable order. Depending on the
/// pre-defined order, the DNF expression produced by a decision tree can still be further
/// simplified.
///
/// For example, the decision diagram for the expression `A or B` will be represented as
/// `A or (not A and B)` or `B or (not B and A)`, depending on the variable order. In both
/// cases, the negation in the second clause is redundant.
///
/// Completely simplifying a DNF expression is NP-hard and amounts to the set cover problem.
/// Additionally, marker expressions can contain complex expressions involving version ranges
/// that are not trivial to simplify. Instead, we choose to simplify at the boolean variable
/// level without any truth table expansion. Combined with the normalization applied by decision
/// trees, this seems to be sufficient in practice.
///
/// The number of clause comparisons is quadratic. Most user-facing markers are simple; the
/// indexed path reduces the cost of comparing larger ones.
fn simplify(dnf: &mut Vec<Vec<MarkerExpression>>) {
    if dnf.len() <= 1 || (dnf.len() >= 8 && simplify_indexed(dnf)) {
        return;
    }
    simplify_with_queries(dnf, ClauseQueries::<false>::Linear(SmallVec::new()));
}

/// Remove terms and clauses in encounter order using the chosen clause comparisons.
/// Each removal affects later comparisons, so this order determines the rendered expression.
fn simplify_with_queries<const CACHE: bool>(
    dnf: &mut Vec<Vec<MarkerExpression>>,
    mut queries: ClauseQueries<'_, CACHE>,
) {
    for clause in 0..dnf.len() {
        queries.prepare_clause(clause);
        for term in 0..dnf[clause].len() {
            if queries.has_subset_with_negated(dnf, clause, term) {
                queries.remove_term(clause, term);
            }
        }
        queries.finish_clause(dnf, clause);
    }

    let mut redundant = SmallVec::<[usize; 8]>::new();
    for clause in 0..dnf.len() {
        if queries.has_subset(dnf, clause, &redundant) {
            redundant.push(clause);
        }
    }
    if !redundant.is_empty() {
        let mut redundant = redundant.into_iter().peekable();
        let mut position = 0;
        dnf.retain(|_| {
            let keep = redundant.next_if_eq(&position).is_none();
            position += 1;
            keep
        });
    }
}

/// Clause comparisons and removal state used by the ordered simplification pass.
enum ClauseQueries<'a, const CACHE: bool> {
    Linear(SmallVec<[usize; 8]>),
    Indexed(IndexedClauses<'a, CACHE>),
}

impl<const CACHE: bool> ClauseQueries<'_, CACHE> {
    /// Reset term removals or prepare cached differences for the next clause.
    fn prepare_clause(&mut self, clause: usize) {
        match self {
            Self::Linear(removed) => removed.clear(),
            Self::Indexed(index) => index.prepare_clause(clause),
        }
    }

    /// Test whether another clause is a subset after negating one term of the current clause.
    /// Previously removed terms cannot justify another removal.
    #[inline]
    fn has_subset_with_negated(
        &self,
        dnf: &[Vec<MarkerExpression>],
        clause: usize,
        term: usize,
    ) -> bool {
        match self {
            Self::Linear(removed) => {
                let skipped = &dnf[clause][term];
                dnf.iter().enumerate().any(|(other, expressions)| {
                    other != clause
                        && expressions.iter().all(|expression| {
                            if expression == skipped {
                                return false;
                            }
                            if is_negation(expression, skipped) {
                                return true;
                            }
                            // Repeated terms match their first occurrence, even when a later one survives.
                            dnf[clause]
                                .iter()
                                .position(|other| other == expression)
                                .is_some_and(|position| !removed.contains(&position))
                        })
                })
            }
            Self::Indexed(index) => index.has_subset_with_negated(clause, term),
        }
    }

    /// Update membership and cached differences after removing a term.
    fn remove_term(&mut self, clause: usize, term: usize) {
        match self {
            Self::Linear(removed) => removed.push(term),
            Self::Indexed(index) => index.remove_term(clause, term),
        }
    }

    /// Apply the marked removals to the clause and its index.
    fn finish_clause(&mut self, dnf: &mut [Vec<MarkerExpression>], clause: usize) {
        match self {
            Self::Linear(removed) => {
                for term in removed.drain(..).rev() {
                    dnf[clause].remove(term);
                }
            }
            Self::Indexed(index) => index.finish_clause(dnf, clause),
        }
    }

    /// Test whether another surviving clause is a subset of this clause.
    fn has_subset(
        &self,
        dnf: &[Vec<MarkerExpression>],
        clause: usize,
        redundant: &[usize],
    ) -> bool {
        let mut others = (0..dnf.len())
            .filter(|&other| other != clause && redundant.binary_search(&other).is_err());
        match self {
            Self::Linear(_) => {
                others.any(|other| dnf[other].iter().all(|term| dnf[clause].contains(term)))
            }
            Self::Indexed(index) => others.any(|other| index.is_subset(other, clause)),
        }
    }
}

/// Use interned terms when the clauses have no repeated terms and the index fits its budget.
/// Returns `false` without changing the DNF when direct comparisons are needed.
fn simplify_indexed(dnf: &mut Vec<Vec<MarkerExpression>>) -> bool {
    const MAX_INDEX_WORDS: usize = 32 * 1024 * 1024 / size_of::<u64>();
    let mut terms = IndexSet::<_, FxBuildHasher>::default();
    let clauses: Vec<Vec<_>> = dnf
        .iter()
        .map(|clause| {
            clause
                .iter()
                .map(|term| terms.insert_full(term).0)
                .collect()
        })
        .collect();
    let words = terms.len().div_ceil(64);
    if dnf.len().saturating_mul(words) > MAX_INDEX_WORDS {
        return false;
    }
    let negated: Vec<_> = terms
        .iter()
        .map(|expression| negate_expression(expression).and_then(|term| terms.get_index_of(&term)))
        .collect();
    drop(terms);
    if words == 0 {
        dnf.truncate(1);
        return true;
    }

    // Cache differences only when many clauses reuse relatively few terms.
    if clauses.len() >= 256 && clauses.len() > negated.len().saturating_mul(2) {
        if let Some(index) = IndexedClauses::<true>::new(clauses, &negated, words) {
            simplify_with_queries(dnf, ClauseQueries::Indexed(index));
            return true;
        }
    } else if let Some(index) = IndexedClauses::<false>::new(clauses, &negated, words) {
        simplify_with_queries(dnf, ClauseQueries::Indexed(index));
        return true;
    }
    false
}

/// Interned clauses with optional cached differences. Cached comparisons store each clause
/// contiguously; direct comparisons store the same word across clauses contiguously.
struct IndexedClauses<'a, const CACHE: bool> {
    clauses: Vec<Vec<usize>>,
    negated: &'a [Option<usize>],
    sets: Vec<u64>,
    words: usize,
    max_negations: usize,
    candidates: Vec<(usize, SmallVec<[usize; 1]>)>,
}

impl<'a, const CACHE: bool> IndexedClauses<'a, CACHE> {
    /// Build membership sets, rejecting clauses with repeated terms.
    fn new(clauses: Vec<Vec<usize>>, negated: &'a [Option<usize>], words: usize) -> Option<Self> {
        let mut index = Self {
            sets: vec![0; clauses.len() * words],
            clauses,
            negated,
            words,
            max_negations: 0,
            candidates: Vec::new(),
        };
        for (clause, terms) in index.clauses.iter().enumerate() {
            for &term in terms {
                let offset = index.offset(clause, term / 64);
                let mask = 1 << (term % 64);
                if index.sets[offset] & mask != 0 {
                    return None;
                }
                index.sets[offset] |= mask;
            }
        }
        if CACHE {
            let mut counts = vec![0usize; negated.len()];
            for &term in negated.iter().flatten() {
                counts[term] += 1;
            }
            index.max_negations = counts.into_iter().max().unwrap_or(0);
        }
        Some(index)
    }

    /// Locate a word in the layout selected for these comparisons.
    fn offset(&self, clause: usize, word: usize) -> usize {
        if CACHE {
            clause * self.words + word
        } else {
            word * self.clauses.len() + clause
        }
    }

    /// Test membership without comparing marker expressions.
    fn contains(&self, clause: usize, term: usize) -> bool {
        self.sets[self.offset(clause, term / 64)] & (1 << (term % 64)) != 0
    }

    /// Visit terms present in `left` but absent from `right`, stopping on failure.
    fn all_missing_from(
        &self,
        left: usize,
        right: usize,
        mut predicate: impl FnMut(usize) -> bool,
    ) -> bool {
        if self.words > 2 && self.words > self.clauses[left].len().min(self.clauses[right].len()) {
            return self.clauses[left]
                .iter()
                .all(|&term| self.contains(right, term) || predicate(term));
        }
        let left = self.offset(left, 0);
        let right = self.offset(right, 0);
        let stride = if CACHE { 1 } else { self.clauses.len() };
        for word in 0..self.words {
            let mut missing = self.sets[left + word * stride] & !self.sets[right + word * stride];
            while missing != 0 {
                if !predicate(word * 64 + missing.trailing_zeros() as usize) {
                    return false;
                }
                missing &= missing - 1;
            }
        }
        true
    }

    /// Cache viable clause differences before considering term removals.
    fn prepare_clause(&mut self, clause: usize) {
        if CACHE {
            self.candidates = (0..self.clauses.len())
                .filter(|&other| other != clause)
                .filter_map(|other| {
                    let mut missing = SmallVec::new();
                    self.all_missing_from(other, clause, |term| {
                        if missing.len() == self.max_negations {
                            return false;
                        }
                        missing.push(term);
                        true
                    })
                    .then_some((other, missing))
                })
                .collect();
        }
    }

    /// Test clause membership using cached differences or direct bitset comparisons.
    #[inline]
    fn has_subset_with_negated(&self, clause: usize, term: usize) -> bool {
        let skipped = self.clauses[clause][term];
        let mask = 1 << (skipped % 64);
        if CACHE {
            self.candidates.iter().any(|(other, missing)| {
                self.sets[*other * self.words + skipped / 64] & mask == 0
                    && missing
                        .iter()
                        .all(|&term| self.negated[term] == Some(skipped))
            })
        } else {
            let offset = (skipped / 64) * self.clauses.len();
            let column = &self.sets[offset..offset + self.clauses.len()];
            (0..self.clauses.len()).any(|other| {
                other != clause
                    && column[other] & mask == 0
                    && self
                        .all_missing_from(other, clause, |term| self.negated[term] == Some(skipped))
            })
        }
    }

    /// Remove a term from membership sets and update candidate differences.
    fn remove_term(&mut self, clause: usize, term: usize) {
        let skipped = self.clauses[clause][term];
        let offset = self.offset(clause, skipped / 64);
        self.sets[offset] &= !(1 << (skipped % 64));
        if CACHE {
            self.candidates.retain_mut(|(other, missing)| {
                if self.sets[*other * self.words + skipped / 64] & (1 << (skipped % 64)) != 0 {
                    if missing.len() == self.max_negations {
                        return false;
                    }
                    missing.push(skipped);
                }
                true
            });
        }
    }

    /// Retain surviving terms in both the expressions and their interned IDs.
    fn finish_clause(&mut self, dnf: &mut [Vec<MarkerExpression>], clause: usize) {
        let mut position = 0;
        dnf[clause].retain(|_| {
            let keep = self.contains(clause, self.clauses[clause][position]);
            position += 1;
            keep
        });
        let clause_count = self.clauses.len();
        self.clauses[clause].retain(|&term| {
            let offset = if CACHE {
                clause * self.words + term / 64
            } else {
                (term / 64) * clause_count + clause
            };
            self.sets[offset] & (1 << (term % 64)) != 0
        });
    }

    /// Test whether all surviving terms in `left` occur in `right`.
    fn is_subset(&self, left: usize, right: usize) -> bool {
        (self.words <= 2 || self.clauses[left].len() <= self.clauses[right].len())
            && self.all_missing_from(left, right, |_| false)
    }
}

/// Remove redundant terms, then redundant clauses, in encounter order without allocating an index.
/// Repeated terms are matched by their first position, which a bit set cannot represent.
#[cfg(test)]
fn simplify_linear(dnf: &mut Vec<Vec<MarkerExpression>>) {
    for i in 0..dnf.len() {
        let clause = &dnf[i];

        // Find redundant terms in this clause.
        let mut redundant_terms = Vec::new();
        'term: for (skipped, skipped_term) in clause.iter().enumerate() {
            for (j, other_clause) in dnf.iter().enumerate() {
                if i == j {
                    continue;
                }

                // Let X be this clause with a given term A set to it's negation.
                // If there exists another clause that is a subset of X, the term A is
                // redundant in this clause.
                //
                // For example, `A or (not A and B)` can be simplified to `A or B`,
                // eliminating the `not A` term.
                if other_clause.iter().all(|term| {
                    // For the term to be redundant in this clause, the other clause can
                    // contain the negation of the term but not the term itself.
                    if term == skipped_term {
                        return false;
                    }
                    if is_negation(term, skipped_term) {
                        return true;
                    }

                    clause
                        .iter()
                        .position(|x| x == term)
                        // If the term was already removed from this one, we cannot
                        // depend on it for further simplification.
                        .is_some_and(|i| !redundant_terms.contains(&i))
                }) {
                    redundant_terms.push(skipped);
                    continue 'term;
                }
            }
        }

        // Eliminate any redundant terms.
        redundant_terms.sort_by(|a, b| b.cmp(a));
        for term in redundant_terms {
            dnf[i].remove(term);
        }
    }

    // Once we have eliminated redundant terms, there may also be redundant clauses.
    // For example, `(A and B) or (not A and B)` would have been simplified above to
    // `(A and B) or B` and can now be further simplified to just `B`.
    let mut redundant_clauses = Vec::new();
    'clause: for i in 0..dnf.len() {
        let clause = &dnf[i];

        for (j, other_clause) in dnf.iter().enumerate() {
            // Ignore clauses that are going to be eliminated.
            if i == j || redundant_clauses.contains(&j) {
                continue;
            }

            // There is another clause that is a subset of this one, thus this clause is redundant.
            if other_clause.iter().all(|term| clause.contains(term)) {
                redundant_clauses.push(i);
                continue 'clause;
            }
        }
    }

    // Eliminate any redundant clauses.
    for i in redundant_clauses.into_iter().rev() {
        dnf.remove(i);
    }
}

/// Sort the clauses in a DNF expression, for backwards compatibility. The goal is to avoid
/// unnecessary churn in the display output of the marker expressions, e.g., when modifying the
/// internal representations used in the marker algebra.
fn sort(dnf: &mut [Vec<MarkerExpression>]) {
    // Sort each clause.
    for clause in dnf.iter_mut() {
        clause.sort_by_key(MarkerExpression::kind);
    }
    // Sort the clauses.
    dnf.sort_by(|a, b| {
        a.iter()
            .map(MarkerExpression::kind)
            .cmp(b.iter().map(MarkerExpression::kind))
    });
}

/// Merge any edges that lead to identical subtrees into a single range.
pub(crate) fn collect_edges<'a, T>(
    map: impl ExactSizeIterator<Item = (&'a Ranges<T>, MarkerTree)>,
) -> IndexMap<MarkerTree, Ranges<T>, FxBuildHasher>
where
    T: Ord + Clone + 'a,
{
    let mut paths: IndexMap<_, Ranges<_>, FxBuildHasher> = IndexMap::default();
    for (range, tree) in map {
        // OK because all ranges are guaranteed to be non-empty.
        let (start, end) = range.bounding_range().unwrap();
        // Combine the ranges.
        let range = Ranges::from_range_bounds((start.cloned(), end.cloned()));
        paths
            .entry(tree)
            .and_modify(|union| *union = union.union(&range))
            .or_insert_with(|| range.clone());
    }

    paths
}

/// Returns `Some` if the expression can be simplified as an inequality consisting
/// of the given values.
///
/// For example, `os_name < 'Linux' or os_name > 'Linux'` can be simplified to
/// `os_name != 'Linux'`.
fn range_inequality<T>(range: &Ranges<T>) -> Option<Vec<&T>>
where
    T: Ord + Clone + fmt::Debug,
{
    if range.is_empty() || range.bounding_range() != Some((Bound::Unbounded, Bound::Unbounded)) {
        return None;
    }

    let mut excluded = Vec::new();
    for ((_, end), (start, _)) in range.iter().tuple_windows() {
        match (end, start) {
            (Bound::Excluded(v1), Bound::Excluded(v2)) if v1 == v2 => excluded.push(v1),
            _ => return None,
        }
    }

    Some(excluded)
}

/// Returns `Some` if the version range can be simplified as a star specifier.
///
/// Only for the two bounds case not covered by [`VersionSpecifier::from_release_only_bounds`].
///
/// For negative ranges like `python_full_version < '3.8' or python_full_version >= '3.9'`,
/// returns `!= '3.8.*'`.
fn star_range_specifier(range: &Ranges<Version>) -> Option<VersionSpecifier> {
    if range.iter().count() != 2 {
        return None;
    }
    // Check for negative star range: two segments [(Unbounded, Excluded(v1)), (Included(v2), Unbounded)]
    let (b1, b2) = range.iter().collect_tuple()?;
    if let ((Bound::Unbounded, Bound::Excluded(v1)), (Bound::Included(v2), Bound::Unbounded)) =
        (b1, b2)
    {
        match *v1.only_release_trimmed().release() {
            [major] if *v2.release() == [major, 1] => {
                Some(VersionSpecifier::not_equals_star_version(Version::new([
                    major, 0,
                ])))
            }
            [major, minor] if *v2.release() == [major, minor + 1] => {
                Some(VersionSpecifier::not_equals_star_version(v1.clone()))
            }
            _ => None,
        }
    } else {
        None
    }
}

/// Returns `true` if the LHS is the negation of the RHS, or vice versa.
fn is_negation(left: &MarkerExpression, right: &MarkerExpression) -> bool {
    match left {
        MarkerExpression::Version { key, specifier } => {
            let MarkerExpression::Version {
                key: key2,
                specifier: specifier2,
            } = right
            else {
                return false;
            };

            key == key2
                && specifier.version() == specifier2.version()
                && specifier
                    .operator()
                    .negate()
                    .is_some_and(|negated| negated == *specifier2.operator())
        }
        MarkerExpression::VersionIn {
            key,
            versions,
            operator,
        } => {
            let MarkerExpression::VersionIn {
                key: key2,
                versions: versions2,
                operator: operator2,
            } = right
            else {
                return false;
            };

            key == key2 && versions == versions2 && operator != operator2
        }
        MarkerExpression::String {
            key,
            operator,
            value,
        } => {
            let MarkerExpression::String {
                key: key2,
                operator: operator2,
                value: value2,
            } = right
            else {
                return false;
            };

            key == key2
                && value == value2
                && operator
                    .negate()
                    .is_some_and(|negated| negated == *operator2)
        }
        MarkerExpression::Extra { operator, name } => {
            let MarkerExpression::Extra {
                name: name2,
                operator: operator2,
            } = right
            else {
                return false;
            };

            name == name2 && operator.negate() == *operator2
        }
        MarkerExpression::List { pair, operator } => {
            let MarkerExpression::List {
                pair: pair2,
                operator: operator2,
            } = right
            else {
                return false;
            };

            pair == pair2 && operator != operator2
        }
    }
}

/// Construct the single expression accepted by [`is_negation`] for this left operand.
/// Returns `None` if the negation cannot be represented as one valid marker expression.
fn negate_expression(expression: &MarkerExpression) -> Option<MarkerExpression> {
    Some(match expression {
        MarkerExpression::Version { key, specifier } => MarkerExpression::Version {
            key: *key,
            specifier: VersionSpecifier::from_version(
                specifier.operator().negate()?,
                specifier.version().clone(),
            )
            .ok()?,
        },
        MarkerExpression::VersionIn {
            key,
            versions,
            operator,
        } => MarkerExpression::VersionIn {
            key: *key,
            versions: versions.clone(),
            operator: operator.negate(),
        },
        MarkerExpression::String {
            key,
            operator,
            value,
        } => MarkerExpression::String {
            key: *key,
            operator: operator.negate()?,
            value: value.clone(),
        },
        MarkerExpression::List { pair, operator } => MarkerExpression::List {
            pair: pair.clone(),
            operator: operator.negate(),
        },
        MarkerExpression::Extra { name, operator } => MarkerExpression::Extra {
            name: name.clone(),
            operator: operator.negate(),
        },
    })
}

#[cfg(test)]
mod tests {
    use itertools::iproduct;
    use smallvec::SmallVec;
    use uv_pep440::{Operator, Version, VersionSpecifier};

    use super::{
        ClauseQueries, collect_dnf, is_negation, negate_expression, simplify, simplify_indexed,
        simplify_linear, simplify_with_queries,
    };
    use crate::{MarkerExpression, MarkerTree, MarkerValueVersion};

    fn expression(value: &str) -> MarkerExpression {
        MarkerExpression::from_str(value)
            .expect("valid marker expression")
            .expect("nontrivial marker expression")
    }

    #[test]
    fn negate_expression_matches_is_negation() {
        let expressions: Vec<_> = [
            "extra == 'a'",
            "extra != 'a'",
            "extra == 'b'",
            "extra != 'b'",
            "python_version == '3.10'",
            "python_version != '3.10'",
            "python_version >= '3.9'",
            "python_version < '3.9'",
            "python_version ~= '3.9'",
            "python_version in '3.9 3.10'",
            "python_version not in '3.9 3.10'",
            "sys_platform == 'linux'",
            "sys_platform != 'linux'",
            "'linux' in sys_platform",
            "'linux' not in sys_platform",
            "'test' in extras",
            "'test' not in extras",
        ]
        .into_iter()
        .map(expression)
        .chain([MarkerExpression::Version {
            key: MarkerValueVersion::PythonVersion,
            specifier: VersionSpecifier::from_version(Operator::ExactEqual, Version::new([3, 10]))
                .expect("valid exact-equality version specifier"),
        }])
        .collect();
        for left in &expressions {
            for right in &expressions {
                assert_eq!(
                    is_negation(left, right),
                    negate_expression(left).as_ref() == Some(right)
                );
            }
        }
    }

    #[test]
    fn indexed_simplification_preserves_removal_order() {
        let [a, not_a, b, not_b] = [
            "extra == 'a'",
            "extra != 'a'",
            "extra == 'b'",
            "extra != 'b'",
        ]
        .map(expression);
        // After removing `a` from the first clause, `b` cannot be removed using the third.
        let mut dnf = vec![
            vec![a.clone(), b.clone()],
            vec![not_a, b.clone()],
            vec![a.clone(), not_b],
        ];

        assert!(simplify_indexed(&mut dnf));
        assert_eq!(dnf, vec![vec![b], vec![a]]);
    }

    #[test]
    fn indexed_simplification_matches_linear() {
        let a = ["extra == 'a'", "extra != 'a'"].map(expression);
        let b = ["extra == 'b'", "extra != 'b'"].map(expression);
        // All nonempty clauses over two variables, with at most one term per variable.
        let clauses: Vec<_> = a
            .iter()
            .chain(&b)
            .map(|term| vec![term.clone()])
            .chain(iproduct!(&a, &b).map(|(a, b)| vec![a.clone(), b.clone()]))
            .collect();
        for (first, second, third) in iproduct!(&clauses, &clauses, &clauses) {
            let mut actual = vec![first.clone(), second.clone(), third.clone()];
            let mut expected = actual.clone();
            let mut linear = actual.clone();
            simplify_linear(&mut expected);
            simplify_with_queries(&mut linear, ClauseQueries::<false>::Linear(SmallVec::new()));
            assert_eq!(linear, expected, "{first:?} or {second:?} or {third:?}");
            assert!(simplify_indexed(&mut actual));
            assert_eq!(actual, expected, "{first:?} or {second:?} or {third:?}");
        }
    }

    #[test]
    fn indexed_simplification_spans_multiple_words() {
        // The 63 shared terms put the complementary pair on opposite sides of a word boundary.
        let shared: Vec<_> = (0..63)
            .map(|index| expression(&format!("extra == 'extra-{index}'")))
            .collect();
        let mut dnf = vec![shared.clone(), shared.clone()];
        dnf[0].push(expression("extra == 'boundary'"));
        dnf[1].push(expression("extra != 'boundary'"));

        assert!(simplify_indexed(&mut dnf));
        assert_eq!(dnf, vec![shared]);
    }

    #[test]
    fn indexed_simplification_uses_surviving_terms_in_sparse_clauses() {
        let [a, not_a, b, c, not_c] = [
            "extra == 'a'",
            "extra != 'a'",
            "extra == 'b'",
            "extra == 'c'",
            "extra != 'c'",
        ]
        .map(expression);
        let mut dnf = vec![
            vec![a, b.clone(), c.clone()],
            vec![not_a, b.clone(), c],
            vec![b.clone(), not_c],
        ];
        // Five terms above plus 124 singleton terms require three words. The first clause
        // becomes sparse after removing `a` and `c`, and the third clause must see only `b`.
        let singletons: Vec<_> = (0..124)
            .map(|index| vec![expression(&format!("platform_machine == 'arch-{index}'"))])
            .collect();
        dnf.extend(singletons.clone());

        assert!(simplify_indexed(&mut dnf));
        let mut expected = vec![vec![b]];
        expected.extend(singletons);
        assert_eq!(dnf, expected);
    }

    #[test]
    fn indexed_simplification_eliminates_empty_clauses() {
        let mut dnf = vec![Vec::new(), Vec::new()];
        assert!(simplify_indexed(&mut dnf));
        assert_eq!(dnf, vec![Vec::new()]);
    }

    #[test]
    fn indexed_simplification_falls_back_for_repeated_terms() {
        let term = expression("extra == 'a'");
        // Eight clauses trigger indexing; a duplicate in the last one must leave the input intact.
        let mut dnf = vec![vec![term.clone()]; 8];
        dnf[7].push(term.clone());
        let original = dnf.clone();

        assert!(!simplify_indexed(&mut dnf));
        assert_eq!(dnf, original);
        simplify(&mut dnf);
        assert_eq!(dnf, vec![vec![term.clone(), term]]);
    }

    #[test]
    fn indexed_candidates_match_linear() {
        for size in [24, 32] {
            let extras: Vec<MarkerTree> = (0..size)
                .map(|index| {
                    format!("extra == 'extra-{index:03}'")
                        .parse()
                        .expect("valid extra marker")
                })
                .collect();
            let mut marker: MarkerTree = "python_full_version < '3.11'"
                .parse()
                .expect("valid environment marker");
            for left in 0..size {
                for right in left + 1..size {
                    marker = marker.or(extras[left].and(extras[right]));
                }
            }
            let mut actual = Vec::new();
            collect_dnf(marker, &mut actual, &mut Vec::new());
            assert!(actual.len() >= 256);
            let mut expected = actual.clone();
            simplify_linear(&mut expected);
            assert!(simplify_indexed(&mut actual));
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn indexed_candidates_match_linear_across_words() {
        let expressions: Vec<_> = (0..70)
            .flat_map(|index| {
                [
                    expression(&format!("extra == 'candidate-{index}'")),
                    expression(&format!("extra != 'candidate-{index}'")),
                ]
            })
            .collect();
        let mut state = 17u64;
        for case in 0..40 {
            let mut next = || {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                (state >> 32) as usize
            };
            let mut actual = Vec::new();
            for _ in 0..512 {
                let mut clause = Vec::new();
                for _ in 0..2 + next() % 6 {
                    let term = expressions[next() % expressions.len()].clone();
                    if !clause.contains(&term) {
                        clause.push(term);
                    }
                }
                actual.push(clause);
            }
            let mut expected = actual.clone();
            simplify_linear(&mut expected);
            assert!(simplify_indexed(&mut actual));
            assert_eq!(actual, expected, "case {case}");
        }
    }

    #[test]
    fn indexed_simplification_limits_dense_storage() {
        // 16,384 singleton terms fill exactly 32 MiB; one more exceeds the index limit.
        let mut actual: Vec<_> = (0..16_385)
            .map(|index| vec![expression(&format!("extra == 'extra-{index}'"))])
            .collect();
        let expected = actual.clone();

        assert!(!simplify_indexed(&mut actual));
        assert_eq!(actual, expected);
    }
}
