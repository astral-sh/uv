use std::fmt;
use std::ops::Bound;

use arcstr::ArcStr;
use indexmap::IndexMap;
use itertools::Itertools;
use rustc_hash::{FxBuildHasher, FxHashMap};
use version_ranges::Ranges;

use uv_pep440::{Version, VersionSpecifier};

use crate::marker::tree::{ContainerOperator, MarkerExpressionKind};
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
/// Note: This function is quadratic in the number of clauses. Markers for large conflict sets can
/// have thousands of clauses, so larger expressions are simplified on bit sets.
fn simplify(dnf: &mut Vec<Vec<MarkerExpression>>) {
    // Indexing only pays off for larger expressions.
    if dnf.len() >= 8 && simplify_indexed(dnf) {
        return;
    }
    simplify_linear(dnf);
}

/// The maximum size of the bit sets used by [`simplify_indexed`] (32 MiB).
const MAX_INDEXED_WORDS: usize = 4 * 1024 * 1024;

/// Equivalent to [`simplify_linear`], but compares clauses as bit sets of interned terms.
///
/// Returns `false` without modifying the expression if a clause repeats a term or the bit sets
/// would exceed [`MAX_INDEXED_WORDS`].
fn simplify_indexed(dnf: &mut Vec<Vec<MarkerExpression>>) -> bool {
    let mut terms: FxHashMap<&MarkerExpression, usize> = FxHashMap::default();
    let mut expressions = Vec::new();
    let clauses: Vec<Vec<usize>> = dnf
        .iter()
        .map(|clause| {
            clause
                .iter()
                .map(|term| {
                    *terms.entry(term).or_insert_with(|| {
                        expressions.push(term);
                        expressions.len() - 1
                    })
                })
                .collect()
        })
        .collect();

    let words = expressions.len().div_ceil(64);
    let Some(size) = (clauses.len() + expressions.len()).checked_mul(words) else {
        return false;
    };
    if size > MAX_INDEXED_WORDS {
        return false;
    }

    let mut sets = BitSets::new(clauses.len(), words);
    for (i, clause) in clauses.iter().enumerate() {
        for &term in clause {
            if sets.contains(i, term) {
                return false;
            }
            sets.insert(i, term);
        }
    }

    // For each term, the terms that negate it. Only terms of the same kind can.
    let mut negations = BitSets::new(expressions.len(), words);
    let mut kinds: FxHashMap<MarkerExpressionKind, Vec<usize>> = FxHashMap::default();
    for (term, expression) in expressions.iter().enumerate() {
        kinds.entry(expression.kind()).or_default().push(term);
    }
    for group in kinds.values() {
        for &term in group {
            for &other in group {
                if is_negation(expressions[other], expressions[term]) {
                    negations.insert(term, other);
                }
            }
        }
    }

    // Find redundant terms, removing each immediately as in `simplify_linear`.
    let mut redundant_terms = vec![Vec::new(); clauses.len()];
    for (i, clause) in clauses.iter().enumerate() {
        for (position, &skipped) in clause.iter().enumerate() {
            let redundant = (0..clauses.len()).any(|j| {
                i != j
                    && !sets.contains(j, skipped)
                    && sets
                        .get(j)
                        .iter()
                        .zip(sets.get(i))
                        .zip(negations.get(skipped))
                        .all(|((other, this), negation)| other & !this & !negation == 0)
            });
            if redundant {
                redundant_terms[i].push(position);
                sets.remove(i, skipped);
            }
        }
    }

    // Find redundant clauses.
    let mut redundant_clauses = vec![false; clauses.len()];
    for i in 0..clauses.len() {
        redundant_clauses[i] = (0..clauses.len()).any(|j| {
            i != j
                && !redundant_clauses[j]
                && sets
                    .get(j)
                    .iter()
                    .zip(sets.get(i))
                    .all(|(other, this)| other & !this == 0)
        });
    }

    for (clause, positions) in dnf.iter_mut().zip(redundant_terms) {
        for position in positions.into_iter().rev() {
            clause.remove(position);
        }
    }
    let mut redundant_clauses = redundant_clauses.into_iter();
    dnf.retain(|_| redundant_clauses.next() == Some(false));

    true
}

/// A fixed number of equally sized bit sets in contiguous storage.
struct BitSets {
    words: usize,
    bits: Vec<u64>,
}

impl BitSets {
    fn new(len: usize, words: usize) -> Self {
        Self {
            words,
            bits: vec![0; len * words],
        }
    }

    fn get(&self, set: usize) -> &[u64] {
        &self.bits[set * self.words..(set + 1) * self.words]
    }

    fn contains(&self, set: usize, bit: usize) -> bool {
        self.bits[set * self.words + bit / 64] & (1 << (bit % 64)) != 0
    }

    fn insert(&mut self, set: usize, bit: usize) {
        self.bits[set * self.words + bit / 64] |= 1 << (bit % 64);
    }

    fn remove(&mut self, set: usize, bit: usize) {
        self.bits[set * self.words + bit / 64] &= !(1 << (bit % 64));
    }
}

/// Simplify a DNF expression by comparing every pair of clauses term by term.
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

                    // TODO(ibraheem): if we intern variables we could reduce this
                    // from a linear search to an integer `HashSet` lookup
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
            if other_clause.iter().all(|term| {
                // TODO(ibraheem): if we intern variables we could reduce this
                // from a linear search to an integer `HashSet` lookup
                clause.contains(term)
            }) {
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

#[cfg(test)]
mod tests {
    use super::{simplify_indexed, simplify_linear};
    use crate::MarkerExpression;

    /// The indexed and linear simplifications must produce identical lockfile output.
    #[test]
    fn indexed_matches_linear() {
        let expressions: Vec<MarkerExpression> = [
            "extra == 'a'",
            "extra != 'a'",
            "extra == 'b'",
            "extra != 'b'",
            "extra == 'c'",
            "extra != 'c'",
            "python_full_version == '3.10'",
            "python_full_version != '3.10'",
            "python_full_version == '3.10.0'",
            "python_full_version >= '3.11'",
            "python_full_version < '3.11'",
            "python_full_version ~= '3.9'",
            "python_version in '3.9 3.10'",
            "python_version not in '3.9 3.10'",
            "sys_platform == 'linux'",
            "sys_platform != 'linux'",
            "'test' in extras",
            "'test' not in extras",
        ]
        .into_iter()
        .map(|expression| MarkerExpression::from_str(expression).unwrap().unwrap())
        .collect();

        // A deterministic pseudo-random generator.
        let mut seed = 17u64;
        let mut next = || {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            usize::try_from(seed >> 33).unwrap()
        };

        let mut indexed = 0;
        for case in 0..2000 {
            let mut dnf = Vec::new();
            for _ in 0..8 + next() % 32 {
                let mut clause = Vec::new();
                for _ in 0..=next() % 5 {
                    let term = expressions[next() % expressions.len()].clone();
                    // Exercise the repeated-term fallback.
                    if case % 4 == 0 || !clause.contains(&term) {
                        clause.push(term);
                    }
                }
                dnf.push(clause);
            }

            let mut expected = dnf.clone();
            simplify_linear(&mut expected);

            let mut actual = dnf.clone();
            if simplify_indexed(&mut actual) {
                indexed += 1;
                assert_eq!(actual, expected, "case {case}");
            } else {
                assert_eq!(actual, dnf, "case {case}");
            }
        }
        assert!(indexed >= 1000, "only {indexed} cases were indexed");
    }
}
