use std::fmt;
use std::ops::Bound;

use arcstr::ArcStr;
use indexmap::IndexMap;
use itertools::Itertools;
use rustc_hash::FxBuildHasher;
use version_ranges::Ranges;

use uv_pep440::{Version, VersionSpecifier, release_specifier_to_range};

use crate::marker::tree::ContainerOperator;
use crate::{
    ExtraOperator, MarkerExpression, MarkerOperator, MarkerTree, MarkerTreeKind, MarkerValueString,
    VersionStringMarkerTree,
};

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
    if dnf
        .iter()
        .flatten()
        .any(|expression| matches!(expression, MarkerExpression::VersionStringDomain { .. }))
    {
        simplify_domains(tree, &mut dnf);
    }
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
            let mut edges: IndexMap<_, (Ranges<Version>, Ranges<ArcStr>), FxBuildHasher> =
                IndexMap::default();
            for (tree, versions) in collect_edges(marker.edges()) {
                edges
                    .entry(tree)
                    .or_insert_with(|| (Ranges::empty(), Ranges::empty()))
                    .0 = versions;
            }
            for (tree, strings) in collect_edges(marker.string_edges()) {
                edges
                    .entry(tree)
                    .or_insert_with(|| (Ranges::empty(), Ranges::empty()))
                    .1 = strings;
            }
            for (&tree, (versions, strings)) in &edges {
                if tree.is_false() {
                    continue;
                }
                // A branch may also cover values whose child is implied by this child.
                // This avoids introducing an opaque-only complement for expressions such
                // as `A or B`, where the B branch can cover A's values too.
                let mut versions = versions.clone();
                let mut strings = strings.clone();
                for (&other, (other_versions, other_strings)) in &edges {
                    if other != tree && tree.is_disjoint(other.negate()) {
                        versions = versions.union(other_versions);
                        strings = strings.union(other_strings);
                    }
                }
                for expressions in version_string_clauses(&marker, &versions, &strings) {
                    let current = path.len();
                    path.extend(expressions);
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

/// Expresses an edge over version and opaque-string values without changing either domain.
///
/// Prefer standard comparisons when they reproduce both maps. Otherwise, guard each domain
/// explicitly: the opaque-domain guard requires the lockfile's extended marker syntax.
fn version_string_clauses(
    marker: &VersionStringMarkerTree<'_>,
    versions: &Ranges<Version>,
    strings: &Ranges<ArcStr>,
) -> Vec<Vec<MarkerExpression>> {
    let key = marker.key().into();
    let numeric_bounds: Vec<_> = versions
        .iter()
        .map(|(lower, upper)| {
            let lower = lower.map(|version| ArcStr::from(version.to_string()));
            let upper = upper.map(|version| ArcStr::from(version.to_string()));
            MarkerOperator::from_bounds((lower.as_ref(), upper.as_ref()))
                .map(|(operator, value)| MarkerExpression::String {
                    key,
                    operator,
                    value,
                })
                .collect()
        })
        .collect();
    // A union of excluded points and version prefixes can be printed as inequalities.
    // Recover raw wildcard spellings from the string map before inferring numeric bounds.
    let numeric_exclusions = (|| {
        let mut missing = versions.complement();
        let mut excluded = Vec::new();
        for value in range_inequality(strings).into_iter().flatten() {
            let Some(prefix) = value.strip_suffix(".*") else {
                continue;
            };
            let Ok(version) = prefix.parse::<Version>() else {
                continue;
            };
            if version.release().last().copied() == Some(u64::MAX) {
                continue;
            }
            let range =
                release_specifier_to_range(VersionSpecifier::equals_star_version(version), true);
            if !range.is_disjoint(&missing) {
                missing = missing.intersection(&range.complement());
                excluded.push(value.clone());
            }
        }
        let mut inferred_prefixes = 0u64;
        for (lower, upper) in missing.iter() {
            let (Bound::Included(lower) | Bound::Excluded(lower)) = lower else {
                return None;
            };
            let (Bound::Included(upper) | Bound::Excluded(upper)) = upper else {
                return None;
            };
            if lower != upper {
                let mut release = lower.release().to_vec();
                release.resize(upper.release().len().max(release.len()), 0);
                let version = Version::new(release);
                if version.release().last().copied() == Some(u64::MAX) {
                    return None;
                }
                let range = release_specifier_to_range(
                    VersionSpecifier::equals_star_version(version.clone()),
                    true,
                );
                if range == Ranges::from_range_bounds(lower.clone()..upper.clone()) {
                    excluded.push(ArcStr::from(format!("{version}.*")));
                } else {
                    // Adjacent prefixes can merge into one numeric gap. Cover it with major
                    // prefixes, then restore any desired numeric values below. Bound the
                    // serialization expansion when distant endpoints imply a large cover.
                    let first = lower.release()[0];
                    let mut last = upper.release()[0];
                    if !missing.contains(upper) && *upper == Version::new([last]) {
                        last = last.checked_sub(1)?;
                    }
                    inferred_prefixes =
                        inferred_prefixes.checked_add(last.checked_sub(first)?.checked_add(1)?)?;
                    if inferred_prefixes > 64 || last == u64::MAX {
                        return None;
                    }
                    excluded.extend((first..=last).map(|major| ArcStr::from(format!("{major}.*"))));
                }
            }
            if missing.contains(upper) {
                excluded.push(ArcStr::from(upper.to_string()));
            }
        }
        Some(excluded)
    })();
    let numeric_clauses = numeric_exclusions.as_ref().map_or_else(
        || numeric_bounds.clone(),
        |excluded| {
            vec![
                excluded
                    .iter()
                    .map(|value| MarkerExpression::String {
                        key,
                        operator: MarkerOperator::NotEqual,
                        value: value.clone(),
                    })
                    .collect(),
            ]
        },
    );
    let string_clauses: Vec<Vec<_>> = if let Some(excluded) = range_inequality(strings) {
        vec![
            excluded
                .into_iter()
                .map(|value| MarkerExpression::String {
                    key,
                    operator: MarkerOperator::NotEqual,
                    value: value.clone(),
                })
                .collect(),
        ]
    } else {
        strings
            .iter()
            .map(|bounds| {
                MarkerOperator::from_bounds(bounds)
                    .map(|(operator, value)| MarkerExpression::String {
                        key,
                        operator,
                        value,
                    })
                    .collect()
            })
            .collect()
    };

    // String comparisons receive version semantics on Darwin. Compare candidates within that
    // platform, which is also the scope of every version-string node produced by the parser.
    let darwin = MarkerTree::expression(MarkerExpression::String {
        key: MarkerValueString::SysPlatform,
        operator: MarkerOperator::Equal,
        value: ArcStr::from("darwin"),
    });
    let expected = marker.condition(versions, strings).and(darwin);
    let matches = |clauses: &[Vec<MarkerExpression>]| {
        clauses
            .iter()
            .fold(MarkerTree::FALSE, |tree, clause| {
                tree.or(clause.iter().fold(MarkerTree::TRUE, |tree, expression| {
                    tree.and(MarkerTree::expression(expression.clone()))
                }))
            })
            .and(darwin)
            == expected
    };
    for candidate in [&string_clauses, &numeric_clauses, &numeric_bounds] {
        if matches(candidate) {
            return candidate.clone();
        }
    }
    // Equality with a wildcard has version-prefix semantics, while inclusive ordering with
    // that same invalid version specifier falls back to exact string equality.
    let inclusive_strings: Vec<Vec<_>> = string_clauses
        .iter()
        .map(|clause| {
            clause
                .iter()
                .map(|expression| match expression {
                    MarkerExpression::String {
                        key,
                        operator: MarkerOperator::Equal,
                        value,
                    } => MarkerExpression::String {
                        key: *key,
                        operator: MarkerOperator::GreaterEqual,
                        value: value.clone(),
                    },
                    expression => expression.clone(),
                })
                .collect()
        })
        .collect();
    if matches(&inclusive_strings) {
        return inclusive_strings;
    }
    let inclusive_union: Vec<_> = numeric_clauses
        .iter()
        .chain(&inclusive_strings)
        .cloned()
        .collect();
    if matches(&inclusive_union) {
        return inclusive_union;
    }
    let union: Vec<_> = numeric_clauses
        .iter()
        .chain(&string_clauses)
        .cloned()
        .collect();
    if matches(&union) {
        return union;
    }
    let intersection: Vec<_> = numeric_clauses
        .iter()
        .cartesian_product(&string_clauses)
        .map(|(numeric, strings)| numeric.iter().chain(strings).cloned().collect())
        .collect();
    if matches(&intersection) {
        return intersection;
    }

    // Numeric prefix exclusions also exclude their raw wildcard strings. Add those strings
    // back when the opaque map includes them; inclusive ordering has no numeric matches here.
    let mut restored = intersection;
    for value in numeric_exclusions.into_iter().flatten() {
        if value.ends_with(".*") && strings.contains(&value) {
            restored.push(vec![MarkerExpression::String {
                key,
                operator: MarkerOperator::GreaterEqual,
                value,
            }]);
        }
    }
    if matches(&restored) {
        return restored;
    }

    let mut clauses = Vec::new();
    for mut clause in numeric_bounds {
        clause.insert(
            0,
            MarkerExpression::VersionStringDomain { key, valid: true },
        );
        clauses.push(clause);
    }
    // A wildcard inequality may also exclude desired boundary versions. Restore those
    // numeric values without changing the opaque branch.
    let restored_numeric: Vec<_> = clauses.iter().chain(&restored).cloned().collect();
    if matches(&restored_numeric) {
        return restored_numeric;
    }
    let finite_strings: Vec<_> = clauses.iter().chain(&inclusive_strings).cloned().collect();
    if matches(&finite_strings) {
        return finite_strings;
    }
    for mut clause in string_clauses {
        clause.insert(
            0,
            MarkerExpression::VersionStringDomain { key, valid: false },
        );
        clauses.push(clause);
    }
    clauses
}

/// Removes domain guards introduced by decision-diagram paths when the complete expression
/// does not need them. For example, `A or B` must not require serializing `not A and B`.
fn simplify_domains(tree: MarkerTree, dnf: &mut [Vec<MarkerExpression>]) {
    for clause in dnf {
        // Prefer removing domain guards, since an opaque guard needs extended syntax.
        let mut indices: Vec<_> = (0..clause.len()).collect();
        indices.sort_by_key(|&index| {
            !matches!(clause[index], MarkerExpression::VersionStringDomain { .. })
        });
        let mut removed = vec![false; clause.len()];
        for skipped in indices {
            let candidate = clause
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != skipped && !removed[*index])
                .fold(MarkerTree::TRUE, |tree, (_, expression)| {
                    tree.and(MarkerTree::expression(expression.clone()))
                });
            if candidate.is_disjoint(tree.negate()) {
                removed[skipped] = true;
            }
        }
        let mut index = 0;
        clause.retain(|_| {
            let retain = !removed[index];
            index += 1;
            retain
        });
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
/// Note: This function has quadratic time complexity. However, it is not applied on every marker
/// operation, only to user facing output, which are typically very simple.
fn simplify(dnf: &mut Vec<Vec<MarkerExpression>>) {
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
                key: other_key,
                operator: other_operator,
                value: other_value,
            } = right
            else {
                return false;
            };
            key == other_key
                && value == other_value
                && operator
                    .negate()
                    .is_some_and(|negated| negated == *other_operator)
                && MarkerTree::expression(left.clone()).negate()
                    == MarkerTree::expression(right.clone())
        }
        MarkerExpression::VersionStringDomain { key, valid } => {
            let MarkerExpression::VersionStringDomain {
                key: other_key,
                valid: other_valid,
            } = right
            else {
                return false;
            };
            key == other_key && valid != other_valid
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
