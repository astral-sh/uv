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
    if dnf.len() >= 8 && simplify_indexed(dnf) {
        return;
    }
    simplify_linear(dnf);
}

/// Simplify a DNF using interned terms and bit sets, following [`simplify_linear`]'s removal order.
/// Each removal affects later comparisons, so reordering them can change the resulting clauses.
/// Returns `false` without modifying the DNF if a clause repeats a term or the index is too large.
fn simplify_indexed(dnf: &mut Vec<Vec<MarkerExpression>>) -> bool {
    const MAX_INDEX_WORDS: usize = 32 * 1024 * 1024 / size_of::<u64>();

    let mut terms = IndexSet::<_, FxBuildHasher>::default();
    let mut clauses: Vec<Vec<_>> = dnf
        .iter()
        .map(|clause| {
            clause
                .iter()
                .map(|term| terms.insert_full(term).0)
                .collect()
        })
        .collect();

    let words = terms.len().div_ceil(64);
    // A sparse expression can have far more distinct terms than terms per clause.
    // Bound the dense index to 32 MiB and use the linear algorithm beyond that.
    let index_words = dnf.len().saturating_mul(words);
    if index_words > MAX_INDEX_WORDS {
        return false;
    }
    let negated: Vec<_> = terms
        .iter()
        .map(|expression| negate_expression(expression).and_then(|term| terms.get_index_of(&term)))
        .collect();
    drop(terms);

    // Every clause is empty, so at most one is needed.
    if words == 0 {
        dnf.truncate(1);
        return true;
    }

    // Candidate setup pays off when many clauses reuse relatively few terms. Smaller or sparse
    // expressions retain direct comparisons, which can stop as soon as a redundant term is found.
    if clauses.len() >= 256 && clauses.len() > negated.len().saturating_mul(2) {
        return simplify_candidates(dnf, clauses, &negated, words);
    }

    // Store each word across all clauses together so membership checks scan contiguous memory.
    let clause_count = clauses.len();
    let mut sets = vec![0u64; index_words];
    for (i, clause) in clauses.iter().enumerate() {
        for &term in clause {
            let index = (term / 64) * clause_count + i;
            // The linear algorithm tracks the first occurrence of a repeated term.
            // A set cannot represent that distinction.
            if sets[index] & (1 << (term % 64)) != 0 {
                return false;
            }
            sets[index] |= 1 << (term % 64);
        }
    }
    // One or two words are cheap to scan without consulting clause lengths.
    let sparse_comparisons = words > 2;

    // A term is redundant when another clause is a subset of this clause with that term negated.
    for i in 0..clause_count {
        let clause = &clauses[i];
        for &skipped in clause {
            let offset = (skipped / 64) * clause_count;
            let skipped_word = &sets[offset..offset + clause_count];
            let skipped_mask = 1 << (skipped % 64);
            let redundant = clauses.iter().enumerate().any(|(j, other)| {
                if i == j || skipped_word[j] & skipped_mask != 0 {
                    return false;
                }
                // Short clauses use term lookups instead of scanning mostly empty words.
                if sparse_comparisons && words > other.len().min(clause.len()) {
                    other.iter().all(|&term| {
                        negated[term] == Some(skipped)
                            || sets[(term / 64) * clause_count + i] & (1 << (term % 64)) != 0
                    })
                } else {
                    sets.chunks_exact(clause_count)
                        .enumerate()
                        .all(|(word, column)| {
                            let mut missing = column[j] & !column[i];
                            while missing != 0 {
                                let term = word * 64 + missing.trailing_zeros() as usize;
                                if negated[term] != Some(skipped) {
                                    return false;
                                }
                                missing &= missing - 1;
                            }
                            true
                        })
                }
            });
            if redundant {
                sets[offset + i] &= !skipped_mask;
            }
        }
        let mut position = 0;
        dnf[i].retain(|_| {
            let term = clause[position];
            position += 1;
            sets[(term / 64) * clause_count + i] & (1 << (term % 64)) != 0
        });
        // Later comparisons use the surviving terms, including when a dense clause becomes sparse.
        clauses[i].retain(|&term| sets[(term / 64) * clause_count + i] & (1 << (term % 64)) != 0);
    }

    // After removing terms, eliminate clauses that contain another surviving clause.
    let mut redundant_clauses = vec![false; clause_count];
    for (i, clause) in clauses.iter().enumerate() {
        redundant_clauses[i] = clauses.iter().enumerate().any(|(j, other)| {
            i != j
                && !redundant_clauses[j]
                && (!sparse_comparisons || other.len() <= clause.len())
                && if sparse_comparisons && words > other.len() {
                    other
                        .iter()
                        .all(|&term| sets[(term / 64) * clause_count + i] & (1 << (term % 64)) != 0)
                } else {
                    sets.chunks_exact(clause_count)
                        .all(|column| column[j] & !column[i] == 0)
                }
        });
    }

    let mut position = 0;
    dnf.retain(|_| {
        let keep = !redundant_clauses[position];
        position += 1;
        keep
    });
    true
}

/// Cache viable clause differences when many clauses reuse relatively few terms.
/// Update the differences after each removal so later checks cannot rely on removed terms.
/// Returns `false` without modifying the DNF if repeated terms require [`simplify_linear`].
fn simplify_candidates(
    dnf: &mut Vec<Vec<MarkerExpression>>,
    mut clauses: Vec<Vec<usize>>,
    negated: &[Option<usize>],
    words: usize,
) -> bool {
    // Store each clause contiguously so candidate differences scan adjacent words.
    let clause_count = clauses.len();
    let mut sets = vec![0u64; clause_count * words];
    for (i, clause) in clauses.iter().enumerate() {
        for &term in clause {
            let index = i * words + term / 64;
            // The linear algorithm tracks the first occurrence of a repeated term.
            // A set cannot represent that distinction.
            if sets[index] & (1 << (term % 64)) != 0 {
                return false;
            }
            sets[index] |= 1 << (term % 64);
        }
    }
    // One or two words are cheap to scan without consulting clause lengths.
    let sparse_comparisons = words > 2;

    // Terms in a candidate clause that are missing from the current clause must be
    // negations of the term being removed.
    let mut negation_counts = vec![0usize; negated.len()];
    for &term in negated.iter().flatten() {
        negation_counts[term] += 1;
    }
    let max_negations = negation_counts.into_iter().max().unwrap_or(0);

    // A term is redundant when another clause is a subset of this clause with that term negated.
    for i in 0..clause_count {
        let clause = &clauses[i];
        let mut candidates: Vec<(usize, SmallVec<[usize; 1]>)> = clauses
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .filter_map(|(j, other)| {
                let mut missing = SmallVec::new();
                if sparse_comparisons && words > other.len() {
                    for &term in other {
                        if sets[i * words + term / 64] & (1 << (term % 64)) == 0 {
                            if missing.len() == max_negations {
                                return None;
                            }
                            missing.push(term);
                        }
                    }
                } else {
                    let differences = sets[j * words..(j + 1) * words]
                        .iter()
                        .zip(&sets[i * words..(i + 1) * words])
                        .map(|(&other, &this)| other & !this);
                    let mut count = 0;
                    for difference in differences.clone() {
                        count += difference.count_ones() as usize;
                        if count > max_negations {
                            return None;
                        }
                    }
                    for (word, mut difference) in differences.enumerate() {
                        while difference != 0 {
                            missing.push(word * 64 + difference.trailing_zeros() as usize);
                            difference &= difference - 1;
                        }
                    }
                }
                Some((j, missing))
            })
            .collect();
        for &skipped in clause {
            let offset = skipped / 64;
            let skipped_mask = 1 << (skipped % 64);
            let redundant = candidates.iter().any(|(j, missing)| {
                sets[*j * words + offset] & skipped_mask == 0
                    && missing.iter().all(|&term| negated[term] == Some(skipped))
            });
            if redundant {
                sets[i * words + offset] &= !skipped_mask;
                // Removed terms are no longer available to justify a later removal.
                candidates.retain_mut(|(j, missing)| {
                    if sets[*j * words + offset] & skipped_mask != 0 {
                        if missing.len() == max_negations {
                            return false;
                        }
                        missing.push(skipped);
                    }
                    true
                });
            }
        }
        let mut position = 0;
        dnf[i].retain(|_| {
            let term = clause[position];
            position += 1;
            sets[i * words + term / 64] & (1 << (term % 64)) != 0
        });
        // Later comparisons use the surviving terms, including when a dense clause becomes sparse.
        clauses[i].retain(|&term| sets[i * words + term / 64] & (1 << (term % 64)) != 0);
    }

    // After removing terms, eliminate clauses that contain another surviving clause.
    let mut redundant_clauses = vec![false; clause_count];
    for (i, clause) in clauses.iter().enumerate() {
        redundant_clauses[i] = clauses.iter().enumerate().any(|(j, other)| {
            i != j
                && !redundant_clauses[j]
                && (!sparse_comparisons || other.len() <= clause.len())
                && if sparse_comparisons && words > other.len() {
                    other
                        .iter()
                        .all(|&term| sets[i * words + term / 64] & (1 << (term % 64)) != 0)
                } else {
                    sets[j * words..(j + 1) * words]
                        .iter()
                        .zip(&sets[i * words..(i + 1) * words])
                        .all(|(&other, &this)| other & !this == 0)
                }
        });
    }

    let mut position = 0;
    dnf.retain(|_| {
        let keep = !redundant_clauses[position];
        position += 1;
        keep
    });
    true
}

/// Remove redundant terms, then redundant clauses, in encounter order without allocating an index.
/// Repeated terms are matched by their first position, which a bit set cannot represent.
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
    use uv_pep440::{Operator, Version, VersionSpecifier};

    use super::{
        collect_dnf, is_negation, negate_expression, simplify, simplify_indexed, simplify_linear,
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
            simplify_linear(&mut expected);
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
