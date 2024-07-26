#![allow(clippy::enum_glob_use, clippy::single_match_else)]

use std::ops::Bound::{self, *};
use std::ops::RangeBounds;

use pubgrub::range::{Range as PubGrubRange, Range};
use rustc_hash::FxHashMap;

use pep440_rs::{Version, VersionSpecifier};
use pep508_rs::{
    ExtraName, ExtraOperator, MarkerExpression, MarkerOperator, MarkerTree, MarkerValueString,
    MarkerValueVersion,
};

use crate::pubgrub::PubGrubSpecifier;
use crate::RequiresPythonBound;

/// Returns true when it can be proven that the given marker expression
/// evaluates to true for precisely zero marker environments.
///
/// When this returns false, it *may* be the case that is evaluates to
/// true for precisely zero marker environments. That is, this routine
/// never has false positives but may have false negatives.
pub(crate) fn is_definitively_empty_set(tree: &MarkerTree) -> bool {
    match *tree {
        // A conjunction is definitively empty when it is known that
        // *any* two of its conjuncts are disjoint. Since this would
        // imply that the entire conjunction could never be true.
        MarkerTree::And(ref trees) => {
            // Since this is quadratic in the case where the
            // expression is *not* empty, we limit ourselves
            // to a small number of conjuncts. In practice,
            // this should hopefully cover most cases.
            if trees.len() > 10 {
                return false;
            }
            for (i, tree1) in trees.iter().enumerate() {
                for tree2 in &trees[i..] {
                    if is_disjoint(tree1, tree2) {
                        return true;
                    }
                }
            }
            false
        }
        // A disjunction is definitively empty when all of its
        // disjuncts are definitively empty.
        MarkerTree::Or(ref trees) => trees.iter().all(is_definitively_empty_set),
        // An "arbitrary" expression is always false, so we
        // at least know it is definitively empty.
        MarkerTree::Expression(MarkerExpression::Arbitrary { .. }) => true,
        // Can't really do much with a single expression. There are maybe
        // trivial cases we could check (like `python_version < '0'`), but I'm
        // not sure it's worth doing?
        MarkerTree::Expression(_) => false,
    }
}

/// Returns `true` if there is no environment in which both marker trees can both apply, i.e.
/// the expression `first and second` is always false.
pub(crate) fn is_disjoint(first: &MarkerTree, second: &MarkerTree) -> bool {
    let (expr1, expr2) = match (first, second) {
        (MarkerTree::Expression(expr1), MarkerTree::Expression(expr2)) => (expr1, expr2),
        // `Or` expressions are disjoint if all clauses are disjoint.
        (other, MarkerTree::Or(exprs)) | (MarkerTree::Or(exprs), other) => {
            return exprs.iter().all(|tree1| is_disjoint(tree1, other))
        }
        // `And` expressions are disjoint if any clause is disjoint.
        (other, MarkerTree::And(exprs)) | (MarkerTree::And(exprs), other) => {
            return exprs.iter().any(|tree1| is_disjoint(tree1, other));
        }
    };

    match (expr1, expr2) {
        // `Arbitrary` expressions always evaluate to `false`, and are thus always disjoint.
        (MarkerExpression::Arbitrary { .. }, _) | (_, MarkerExpression::Arbitrary { .. }) => true,
        (MarkerExpression::Version { .. }, expr2) => version_is_disjoint(expr1, expr2),
        (MarkerExpression::String { .. }, expr2) => string_is_disjoint(expr1, expr2),
        (MarkerExpression::Extra { operator, name }, expr2) => {
            extra_is_disjoint(operator, name, expr2)
        }
    }
}

/// Returns `true` if this string expression does not intersect with the given expression.
fn string_is_disjoint(this: &MarkerExpression, other: &MarkerExpression) -> bool {
    use MarkerOperator::*;

    // Extract the normalized string expressions.
    let Some((key, operator, value)) = extract_string_expression(this) else {
        return false;
    };
    let Some((key2, operator2, value2)) = extract_string_expression(other) else {
        return false;
    };

    // Distinct string expressions are not disjoint.
    if key != key2 {
        return false;
    }

    match (operator, operator2) {
        // The only disjoint expressions involving strict inequality are `key != value` and `key == value`.
        (NotEqual, Equal) | (Equal, NotEqual) => return value == value2,
        (NotEqual, _) | (_, NotEqual) => return false,
        // Similarly for `in` and `not in`.
        (In, NotIn) | (NotIn, In) => return value == value2,
        (In | NotIn, _) | (_, In | NotIn) => return false,
        // As well as the inverse.
        (Contains, NotContains) | (NotContains, Contains) => return value == value2,
        (Contains | NotContains, _) | (_, Contains | NotContains) => return false,
        _ => {}
    }

    let bounds = string_bounds(value, operator);
    let bounds2 = string_bounds(value2, operator2);

    // Make sure the ranges do not intersect.
    if range_exists::<&str>(&bounds2.start_bound(), &bounds.end_bound())
        && range_exists::<&str>(&bounds.start_bound(), &bounds2.end_bound())
    {
        return false;
    }

    true
}

pub(crate) fn python_range(expr: &MarkerExpression) -> Option<Range<Version>> {
    match expr {
        MarkerExpression::Version {
            key: MarkerValueVersion::PythonFullVersion,
            specifier,
        } => {
            // Simplify using PEP 440 semantics.
            let specifier = PubGrubSpecifier::from_pep440_specifier(specifier).ok()?;

            // Convert to PubGrub.
            Some(PubGrubRange::from(specifier))
        }
        MarkerExpression::Version {
            key: MarkerValueVersion::PythonVersion,
            specifier,
        } => {
            // Simplify using release-only semantics, since `python_version` is always `major.minor`.
            let specifier = PubGrubSpecifier::from_release_specifier(specifier).ok()?;

            // Convert to PubGrub.
            Some(PubGrubRange::from(specifier))
        }
        _ => None,
    }
}

/// Returns the minimum Python version that can satisfy the [`MarkerTree`], if it's constrained.
pub(crate) fn requires_python_marker(tree: &MarkerTree) -> Option<RequiresPythonBound> {
    match tree {
        MarkerTree::Expression(expr) => {
            // Extract the supported Python range.
            let range = python_range(expr)?;

            // Extract the lower bound.
            let (lower, _) = range.iter().next()?;
            Some(RequiresPythonBound::new(lower.clone()))
        }
        MarkerTree::And(trees) => {
            // Take the maximum of any nested expressions.
            trees.iter().filter_map(requires_python_marker).max()
        }
        MarkerTree::Or(trees) => {
            // If all subtrees have a bound, take the minimum.
            let mut min_version = None;
            for tree in trees {
                let version = requires_python_marker(tree)?;
                min_version = match min_version {
                    Some(min_version) => Some(std::cmp::min(min_version, version)),
                    None => Some(version),
                };
            }
            min_version
        }
    }
}

/// Normalizes this marker tree.
///
/// This function does a number of operations to normalize a marker tree recursively:
/// - Sort and flatten all nested expressions.
/// - Simplify expressions. This includes combining overlapping version ranges, removing duplicate
///   expressions, and removing redundant expressions.
/// - Normalize the order of version expressions to the form `<version key> <version op> <version>`
///   (i.e., not the reverse).
///
/// This is useful in cases where creating conjunctions or disjunctions might occur in a non-deterministic
/// order. This routine will attempt to erase the distinction created by such a construction.
///
/// This returns `None` in the case of normalization turning a `MarkerTree`
/// into an expression that is known to match all possible marker
/// environments. Note though that this may return an empty disjunction, e.g.,
/// `MarkerTree::Or(vec![])`, which matches precisely zero marker environments.
pub(crate) fn normalize(
    mut tree: MarkerTree,
    bound: Option<&RequiresPythonBound>,
) -> Option<MarkerTree> {
    // Filter out redundant expressions that show up before and after normalization.
    filter_all(&mut tree);
    let mut tree = normalize_all(tree, bound)?;
    filter_all(&mut tree);
    Some(tree)
}

/// Normalize the marker tree recursively.
pub(crate) fn normalize_all(
    tree: MarkerTree,
    bound: Option<&RequiresPythonBound>,
) -> Option<MarkerTree> {
    match tree {
        MarkerTree::And(trees) => {
            let mut reduced = Vec::new();
            let mut versions: FxHashMap<_, Vec<_>> = FxHashMap::default();

            for subtree in trees {
                // Normalize nested expressions as much as possible first.
                //
                // If the expression gets normalized out (e.g., `version < '3.8' and version >= '3.8'`),
                // omit it.
                let Some(subtree) = normalize_all(subtree, bound) else {
                    continue;
                };

                match subtree {
                    MarkerTree::Or(_) => reduced.push(subtree),
                    // Flatten nested `And` expressions.
                    MarkerTree::And(subtrees) => reduced.extend(subtrees),
                    // Extract expressions we may be able to simplify more.
                    MarkerTree::Expression(ref expr) => {
                        if let Some((key, range)) = keyed_range(expr) {
                            versions.entry(key.clone()).or_default().push(range);
                            continue;
                        }

                        reduced.push(subtree);
                    }
                }
            }

            // Combine version ranges.
            simplify_ranges(&mut reduced, versions, |ranges| {
                ranges
                    .iter()
                    .fold(PubGrubRange::full(), |acc, range| acc.intersection(range))
            });

            reduced.sort();
            reduced.dedup();

            match reduced.len() {
                0 => None,
                1 => Some(reduced.remove(0)),
                _ => Some(MarkerTree::And(reduced)),
            }
        }

        MarkerTree::Or(trees) => {
            let mut reduced = Vec::new();
            let mut versions: FxHashMap<_, Vec<_>> = FxHashMap::default();

            for subtree in trees {
                // Normalize nested expressions as much as possible first.
                //
                // If the expression gets normalized out (e.g., `version < '3.8' and version >= '3.8'`),
                // return `true`.
                let subtree = normalize_all(subtree, bound)?;

                match subtree {
                    MarkerTree::And(_) => reduced.push(subtree),
                    // Flatten nested `Or` expressions.
                    MarkerTree::Or(subtrees) => {
                        for subtree in subtrees {
                            match subtree {
                                // Look one level deeper for expressions to simplify, as
                                // `normalize_all` can return `MarkerTree::Or` for some expressions.
                                MarkerTree::Expression(ref expr) => {
                                    if let Some((key, range)) = keyed_range(expr) {
                                        versions.entry(key.clone()).or_default().push(range);
                                        continue;
                                    }
                                    reduced.push(subtree);
                                }
                                _ => reduced.push(subtree),
                            }
                        }
                    }
                    // Extract expressions we may be able to simplify more.
                    MarkerTree::Expression(ref expr) => {
                        if let Some((key, range)) = keyed_range(expr) {
                            versions.entry(key.clone()).or_default().push(range);
                            continue;
                        }

                        reduced.push(subtree);
                    }
                }
            }

            // Combine version ranges.
            simplify_ranges(&mut reduced, versions, |ranges| {
                ranges
                    .iter()
                    .fold(PubGrubRange::empty(), |acc, range| acc.union(range))
            });

            reduced.sort();
            reduced.dedup();

            // If the reduced trees contain complementary terms (e.g., `sys_platform == 'linux' or sys_platform != 'linux'`),
            // the expression is always true and can be removed.
            if contains_complements(&reduced) {
                return None;
            }

            match reduced.len() {
                0 => None,
                1 => Some(reduced.remove(0)),
                _ => Some(MarkerTree::Or(reduced)),
            }
        }

        // If the marker is redundant given the supported Python range, remove it.
        //
        // For example, `python_version >= '3.7'` is redundant with `requires-python: '>=3.8'`.
        MarkerTree::Expression(expr)
            if bound.is_some_and(|bound| {
                python_range(&expr).is_some_and(|supported_range| {
                    Range::from(bound.clone()).subset_of(&supported_range)
                })
            }) =>
        {
            None
        }

        MarkerTree::Expression(ref expr) => {
            if let Some((key, range)) = keyed_range(expr) {
                // If multiple terms are required to express the range, flatten them into an `Or`
                // expression.
                let mut iter = range.iter().flat_map(VersionSpecifier::from_bounds);
                let first = iter.next().unwrap();
                if let Some(second) = iter.next() {
                    Some(MarkerTree::Or(
                        std::iter::once(first)
                            .chain(std::iter::once(second))
                            .chain(iter)
                            .map(|specifier| {
                                MarkerTree::Expression(MarkerExpression::Version {
                                    key: key.clone(),
                                    specifier,
                                })
                            })
                            .collect(),
                    ))
                } else {
                    Some(MarkerTree::Expression(MarkerExpression::Version {
                        key: key.clone(),
                        specifier: first,
                    }))
                }
            } else {
                Some(tree)
            }
        }
    }
}

/// Removes redundant expressions from the tree recursively.
///
/// This function does not attempt to flatten or clean the tree and may leave it in a denormalized state.
pub(crate) fn filter_all(tree: &mut MarkerTree) {
    match tree {
        MarkerTree::And(trees) => {
            for subtree in &mut *trees {
                filter_all(subtree);
            }

            for conjunct in collect_expressions(trees) {
                // Filter out redundant disjunctions (by the Absorption Law).
                trees.retain_mut(|tree| !filter_disjunctions(tree, &conjunct));

                // Filter out redundant expressions in this conjunction.
                for tree in &mut *trees {
                    filter_conjunct_exprs(tree, &conjunct);
                }
            }
        }

        MarkerTree::Or(trees) => {
            for subtree in &mut *trees {
                filter_all(subtree);
            }

            for disjunct in collect_expressions(trees) {
                // Filter out redundant conjunctions (by the Absorption Law).
                trees.retain_mut(|tree| !filter_conjunctions(tree, &disjunct));

                // Filter out redundant expressions in this disjunction.
                for tree in &mut *trees {
                    filter_disjunct_exprs(tree, &disjunct);
                }
            }
        }

        MarkerTree::Expression(_) => {}
    }
}

/// Collects all direct leaf expressions from a list of marker trees.
///
/// The expressions that are directly present within a conjunction or disjunction
/// can be used to filter out redundant expressions recursively in sibling trees. Importantly,
/// this function only returns expressions present at the top-level and does not search
/// recursively.
fn collect_expressions(trees: &[MarkerTree]) -> Vec<MarkerExpression> {
    trees
        .iter()
        .filter_map(|tree| match tree {
            MarkerTree::Expression(expr) => Some(expr.clone()),
            _ => None,
        })
        .collect()
}

/// Filters out the given expression recursively from any disjunctions in a marker tree.
///
/// If a given expression is directly present in an outer disjunction, the tree can be satisfied
/// by the singular expression and thus we can filter it out from any disjunctions in sibling trees.
/// For example, `a or (b or a)` can be simplified to `a or b`.
fn filter_disjunct_exprs(tree: &mut MarkerTree, disjunct: &MarkerExpression) {
    match tree {
        MarkerTree::Or(trees) => {
            trees.retain_mut(|tree| match tree {
                MarkerTree::Expression(expr) => expr != disjunct,
                _ => {
                    filter_disjunct_exprs(tree, disjunct);
                    true
                }
            });
        }

        MarkerTree::And(trees) => {
            for tree in trees {
                filter_disjunct_exprs(tree, disjunct);
            }
        }

        MarkerTree::Expression(_) => {}
    }
}

/// Filters out the given expression recursively from any conjunctions in a marker tree.
///
/// If a given expression is directly present in an outer conjunction, we can assume it is
/// true in all sibling trees and thus filter it out from any nested conjunctions. For example,
/// `a and (b and a)` can be simplified to `a and b`.
fn filter_conjunct_exprs(tree: &mut MarkerTree, conjunct: &MarkerExpression) {
    match tree {
        MarkerTree::And(trees) => {
            trees.retain_mut(|tree| match tree {
                MarkerTree::Expression(expr) => expr != conjunct,
                _ => {
                    filter_conjunct_exprs(tree, conjunct);
                    true
                }
            });
        }

        MarkerTree::Or(trees) => {
            for tree in trees {
                filter_conjunct_exprs(tree, conjunct);
            }
        }

        MarkerTree::Expression(_) => {}
    }
}

/// Filters out disjunctions recursively from a marker tree that contain the given expression.
///
/// If a given expression is directly present in an outer conjunction, we can assume it is
/// true in all sibling trees and thus filter out any disjunctions that contain it. For example,
/// `a and (b or a)` can be simplified to `a`.
///
/// Returns `true` if the outer tree should be removed.
fn filter_disjunctions(tree: &mut MarkerTree, conjunct: &MarkerExpression) -> bool {
    let disjunction = match tree {
        MarkerTree::Or(trees) => trees,
        // Recurse because the tree might not have been flattened.
        MarkerTree::And(trees) => {
            trees.retain_mut(|tree| !filter_disjunctions(tree, conjunct));
            return trees.is_empty();
        }
        MarkerTree::Expression(_) => return false,
    };

    let mut filter = Vec::new();
    for (i, tree) in disjunction.iter_mut().enumerate() {
        match tree {
            // Found a matching expression, filter out this entire tree.
            MarkerTree::Expression(expr) if expr == conjunct => {
                return true;
            }
            // Filter subtrees.
            MarkerTree::Or(_) => {
                if filter_disjunctions(tree, conjunct) {
                    filter.push(i);
                }
            }
            _ => {}
        }
    }

    for i in filter.into_iter().rev() {
        disjunction.remove(i);
    }

    false
}

/// Filters out conjunctions recursively from a marker tree that contain the given expression.
///
/// If a given expression is directly present in an outer disjunction, the tree can be satisfied
/// by the singular expression and thus we can filter out any conjunctions in sibling trees that
/// contain it. For example, `a or (b and a)` can be simplified to `a`.
///
/// Returns `true` if the outer tree should be removed.
fn filter_conjunctions(tree: &mut MarkerTree, disjunct: &MarkerExpression) -> bool {
    let conjunction = match tree {
        MarkerTree::And(trees) => trees,
        // Recurse because the tree might not have been flattened.
        MarkerTree::Or(trees) => {
            trees.retain_mut(|tree| !filter_conjunctions(tree, disjunct));
            return trees.is_empty();
        }
        MarkerTree::Expression(_) => return false,
    };

    let mut filter = Vec::new();
    for (i, tree) in conjunction.iter_mut().enumerate() {
        match tree {
            // Found a matching expression, filter out this entire tree.
            MarkerTree::Expression(expr) if expr == disjunct => {
                return true;
            }
            // Filter subtrees.
            MarkerTree::And(_) => {
                if filter_conjunctions(tree, disjunct) {
                    filter.push(i);
                }
            }
            _ => {}
        }
    }

    for i in filter.into_iter().rev() {
        conjunction.remove(i);
    }

    false
}

/// Simplify version expressions.
fn simplify_ranges(
    reduced: &mut Vec<MarkerTree>,
    versions: FxHashMap<MarkerValueVersion, Vec<PubGrubRange<Version>>>,
    combine: impl Fn(&Vec<PubGrubRange<Version>>) -> PubGrubRange<Version>,
) {
    for (key, ranges) in versions {
        let simplified = combine(&ranges);

        // If this is a meaningless expressions with no valid intersection, add back
        // the original ranges.
        if simplified.is_empty() {
            for specifier in ranges
                .iter()
                .flat_map(PubGrubRange::iter)
                .flat_map(VersionSpecifier::from_bounds)
            {
                reduced.push(MarkerTree::Expression(MarkerExpression::Version {
                    specifier,
                    key: key.clone(),
                }));
            }
        }

        // Add back the simplified segments.
        for specifier in simplified.iter().flat_map(VersionSpecifier::from_bounds) {
            reduced.push(MarkerTree::Expression(MarkerExpression::Version {
                key: key.clone(),
                specifier,
            }));
        }
    }
}

/// Extracts the key, value, and string from a string expression, reversing the operator if necessary.
fn extract_string_expression(
    expr: &MarkerExpression,
) -> Option<(&MarkerValueString, MarkerOperator, &str)> {
    match expr {
        MarkerExpression::String {
            key,
            operator,
            value,
        } => Some((key, *operator, value)),
        _ => None,
    }
}

/// Returns `true` if the range formed by an upper and lower bound is non-empty.
fn range_exists<T: PartialOrd>(lower: &Bound<T>, upper: &Bound<T>) -> bool {
    match (lower, upper) {
        (Included(s), Included(e)) => s <= e,
        (Included(s), Excluded(e)) => s < e,
        (Excluded(s), Included(e)) => s < e,
        (Excluded(s), Excluded(e)) => s < e,
        (Unbounded, _) | (_, Unbounded) => true,
    }
}

/// Returns the lower and upper bounds of a string inequality.
///
/// Panics if called on the `!=`, `in`, or `not in` operators.
fn string_bounds(value: &str, operator: MarkerOperator) -> (Bound<&str>, Bound<&str>) {
    use MarkerOperator::*;
    match operator {
        Equal => (Included(value), Included(value)),
        // TODO: not really sure what this means for strings
        TildeEqual => (Included(value), Included(value)),
        GreaterThan => (Excluded(value), Unbounded),
        GreaterEqual => (Included(value), Unbounded),
        LessThan => (Unbounded, Excluded(value)),
        LessEqual => (Unbounded, Included(value)),
        NotEqual | In | NotIn | Contains | NotContains => unreachable!(),
    }
}

/// Returns `true` if this extra expression does not intersect with the given expression.
fn extra_is_disjoint(operator: &ExtraOperator, name: &ExtraName, other: &MarkerExpression) -> bool {
    let MarkerExpression::Extra {
        operator: operator2,
        name: name2,
    } = other
    else {
        return false;
    };

    // extra expressions are only disjoint if they require existence and non-existence of the same extra
    operator != operator2 && name == name2
}

/// Returns `true` if this version expression does not intersect with the given expression.
fn version_is_disjoint(this: &MarkerExpression, other: &MarkerExpression) -> bool {
    let Some((key, range)) = keyed_range(this) else {
        return false;
    };

    // if this is not a version expression it may intersect
    let Some((key2, range2)) = keyed_range(other) else {
        return false;
    };

    // distinct version expressions are not disjoint
    if key != key2 {
        return false;
    }

    // there is no version that is contained in both ranges
    range.is_disjoint(&range2)
}

/// Return `true` if the tree contains complementary terms (e.g., `sys_platform == 'linux' or sys_platform != 'linux'`).
fn contains_complements(trees: &[MarkerTree]) -> bool {
    let mut terms = FxHashMap::default();
    for tree in trees {
        let MarkerTree::Expression(MarkerExpression::String {
            key,
            operator,
            value,
        }) = tree
        else {
            continue;
        };

        match operator {
            MarkerOperator::Equal => {
                if let Some(MarkerOperator::NotEqual) = terms.insert((key, value), operator) {
                    return true;
                }
            }
            MarkerOperator::NotEqual => {
                if let Some(MarkerOperator::Equal) = terms.insert((key, value), operator) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Returns the key and version range for a version expression.
fn keyed_range(expr: &MarkerExpression) -> Option<(&MarkerValueVersion, PubGrubRange<Version>)> {
    let (key, specifier) = match expr {
        MarkerExpression::Version { key, specifier } => (key, specifier.clone()),
        _ => return None,
    };

    // Simplify using either PEP 440 or release-only semantics.
    let pubgrub_specifier = match expr {
        MarkerExpression::Version {
            key: MarkerValueVersion::PythonVersion,
            ..
        } => PubGrubSpecifier::from_release_specifier(&specifier).ok()?,
        MarkerExpression::Version {
            key: MarkerValueVersion::PythonFullVersion,
            ..
        } => PubGrubSpecifier::from_pep440_specifier(&specifier).ok()?,
        _ => return None,
    };

    Some((key, pubgrub_specifier.into()))
}

#[cfg(test)]
mod tests {
    use pep508_rs::TracingReporter;

    use super::*;

    #[test]
    fn simplify() {
        assert_marker_equal(
            "python_version == '3.9' or python_version == '3.9'",
            "python_version == '3.9'",
        );

        assert_marker_equal(
            "python_version < '3.17' or python_version < '3.18'",
            "python_version < '3.18'",
        );

        assert_marker_equal(
            "python_version > '3.17' or python_version > '3.18' or python_version > '3.12'",
            "python_version > '3.12'",
        );

        // a quirk of how pubgrub works, but this is considered part of normalization
        assert_marker_equal(
            "python_version > '3.17.post4' or python_version > '3.18.post4'",
            "python_version > '3.17'",
        );

        assert_marker_equal(
            "python_version < '3.17' and python_version < '3.18'",
            "python_version < '3.17'",
        );

        assert_marker_equal(
            "python_version <= '3.18' and python_version == '3.18'",
            "python_version == '3.18'",
        );

        assert_marker_equal(
            "python_version <= '3.18' or python_version == '3.18'",
            "python_version <= '3.18'",
        );

        assert_marker_equal(
            "python_version <= '3.15' or (python_version <= '3.17' and python_version < '3.16')",
            "python_version < '3.16'",
        );

        assert_marker_equal(
            "(python_version > '3.17' or python_version > '3.16') and python_version > '3.15'",
            "python_version > '3.16'",
        );

        assert_marker_equal(
            "(python_version > '3.17' or python_version > '3.16') and python_version > '3.15' and implementation_version == '1'",
            "implementation_version == '1' and python_version > '3.16'",
        );

        assert_marker_equal(
            "('3.17' < python_version or '3.16' < python_version) and '3.15' < python_version and implementation_version == '1'",
            "implementation_version == '1' and python_version > '3.16'",
        );

        assert_marker_equal("extra == 'a' or extra == 'a'", "extra == 'a'");
        assert_marker_equal(
            "extra == 'a' and extra == 'a' or extra == 'b'",
            "extra == 'a' or extra == 'b'",
        );

        // bogus expressions are retained but still normalized
        assert_marker_equal(
            "python_version < '3.17' and '3.18' == python_version",
            "python_version == '3.18' and python_version < '3.17'",
        );

        // flatten nested expressions
        assert_marker_equal(
            "((extra == 'a' and extra == 'b') and extra == 'c') and extra == 'b'",
            "extra == 'a' and extra == 'b' and extra == 'c'",
        );

        assert_marker_equal(
            "((extra == 'a' or extra == 'b') or extra == 'c') or extra == 'b'",
            "extra == 'a' or extra == 'b' or extra == 'c'",
        );

        // complex expressions
        assert_marker_equal(
            "extra == 'a' or (extra == 'a' and extra == 'b')",
            "extra == 'a'",
        );

        assert_marker_equal(
            "extra == 'a' and (extra == 'a' or extra == 'b')",
            "extra == 'a'",
        );

        assert_marker_equal(
            "(extra == 'a' and (extra == 'a' or extra == 'b')) or extra == 'd'",
            "extra == 'a' or extra == 'd'",
        );

        assert_marker_equal(
            "((extra == 'a' and extra == 'b') or extra == 'c') or extra == 'b'",
            "extra == 'b' or extra == 'c'",
        );

        assert_marker_equal(
            "((extra == 'a' or extra == 'b') and extra == 'c') and extra == 'b'",
            "extra == 'b' and extra == 'c'",
        );

        assert_marker_equal(
            "((extra == 'a' or extra == 'b') and extra == 'c') or extra == 'b'",
            "extra == 'b' or (extra == 'a' and extra == 'c')",
        );

        // post-normalization filtering
        assert_marker_equal(
            "(python_version < '3.1' or python_version < '3.2') and (python_version < '3.2' or python_version == '3.3')",
            "python_version < '3.2'",
        );

        // normalize out redundant ranges
        assert_normalizes_out("python_version < '3.12.0rc1' or python_version >= '3.12.0rc1'");

        assert_normalizes_out(
            "extra == 'a' or (python_version < '3.12.0rc1' or python_version >= '3.12.0rc1')",
        );

        assert_normalizes_to(
            "extra == 'a' and (python_version < '3.12.0rc1' or python_version >= '3.12.0rc1')",
            "extra == 'a'",
        );

        // normalize `!=` operators
        assert_normalizes_out("python_version != '3.10' or python_version < '3.12'");

        assert_normalizes_to(
            "python_version != '3.10' or python_version > '3.12'",
            "python_version < '3.10' or python_version > '3.10'",
        );

        assert_normalizes_to(
            "python_version != '3.8' and python_version < '3.10'",
            "python_version < '3.10' and (python_version < '3.8' or python_version > '3.8')",
        );

        assert_normalizes_to(
            "python_version != '3.8' and python_version != '3.9'",
            "(python_version < '3.8' or python_version > '3.8') and (python_version < '3.9' or python_version > '3.9')",
        );

        // normalize out redundant expressions
        assert_normalizes_out("sys_platform == 'win32' or sys_platform != 'win32'");

        assert_normalizes_out("'win32' == sys_platform or sys_platform != 'win32'");

        assert_normalizes_out(
            "sys_platform == 'win32' or sys_platform == 'win32' or sys_platform != 'win32'",
        );

        assert_normalizes_to(
            "sys_platform == 'win32' and sys_platform != 'win32'",
            "sys_platform == 'win32' and sys_platform != 'win32'",
        );
    }

    #[test]
    fn requires_python() {
        assert_normalizes_out("python_version >= '3.8'");
        assert_normalizes_out("python_version >= '3.8' or sys_platform == 'win32'");

        assert_normalizes_to(
            "python_version >= '3.8' and sys_platform == 'win32'",
            "sys_platform == 'win32'",
        );

        assert_normalizes_to("python_version == '3.8'", "python_version == '3.8'");

        assert_normalizes_to("python_version <= '3.10'", "python_version <= '3.10'");
    }

    #[test]
    fn extra_disjointness() {
        assert!(!is_disjoint("extra == 'a'", "python_version == '1'"));

        assert!(!is_disjoint("extra == 'a'", "extra == 'a'"));
        assert!(!is_disjoint("extra == 'a'", "extra == 'b'"));
        assert!(!is_disjoint("extra == 'b'", "extra == 'a'"));
        assert!(!is_disjoint("extra == 'b'", "extra != 'a'"));
        assert!(!is_disjoint("extra != 'b'", "extra == 'a'"));
        assert!(is_disjoint("extra != 'b'", "extra == 'b'"));
        assert!(is_disjoint("extra == 'b'", "extra != 'b'"));
    }

    #[test]
    fn arbitrary_disjointness() {
        assert!(is_disjoint(
            "python_version == 'Linux'",
            "python_version == '3.7.1'"
        ));
    }

    #[test]
    fn version_disjointness() {
        assert!(!is_disjoint(
            "os_name == 'Linux'",
            "python_version == '3.7.1'"
        ));

        test_version_bounds_disjointness("python_version");

        assert!(!is_disjoint(
            "python_version == '3.7.*'",
            "python_version == '3.7.1'"
        ));
    }

    #[test]
    fn string_disjointness() {
        assert!(!is_disjoint(
            "os_name == 'Linux'",
            "platform_version == '3.7.1'"
        ));
        assert!(!is_disjoint(
            "implementation_version == '3.7.0'",
            "python_version == '3.7.1'"
        ));

        // basic version bounds checking should still work with lexicographical comparisons
        test_version_bounds_disjointness("platform_version");

        assert!(is_disjoint("os_name == 'Linux'", "os_name == 'OSX'"));
        assert!(is_disjoint("os_name <= 'Linux'", "os_name == 'OSX'"));

        assert!(!is_disjoint(
            "os_name in 'OSXLinuxWindows'",
            "os_name == 'OSX'"
        ));
        assert!(!is_disjoint("'OSX' in os_name", "'Linux' in os_name"));

        // complicated `in` intersections are not supported
        assert!(!is_disjoint("os_name in 'OSX'", "os_name in 'Linux'"));
        assert!(!is_disjoint(
            "os_name in 'OSXLinux'",
            "os_name == 'Windows'"
        ));

        assert!(is_disjoint(
            "os_name in 'Windows'",
            "os_name not in 'Windows'"
        ));
        assert!(is_disjoint(
            "'Windows' in os_name",
            "'Windows' not in os_name"
        ));

        assert!(!is_disjoint("'Windows' in os_name", "'Windows' in os_name"));
        assert!(!is_disjoint("'Linux' in os_name", "os_name not in 'Linux'"));
        assert!(!is_disjoint("'Linux' not in os_name", "os_name in 'Linux'"));
    }

    #[test]
    fn combined_disjointness() {
        assert!(!is_disjoint(
            "os_name == 'a' and platform_version == '1'",
            "os_name == 'a'"
        ));
        assert!(!is_disjoint(
            "os_name == 'a' or platform_version == '1'",
            "os_name == 'a'"
        ));

        assert!(is_disjoint(
            "os_name == 'a' and platform_version == '1'",
            "os_name == 'a' and platform_version == '2'"
        ));
        assert!(is_disjoint(
            "os_name == 'a' and platform_version == '1'",
            "'2' == platform_version and os_name == 'a'"
        ));
        assert!(!is_disjoint(
            "os_name == 'a' or platform_version == '1'",
            "os_name == 'a' or platform_version == '2'"
        ));

        assert!(is_disjoint(
            "sys_platform == 'darwin' and implementation_name == 'pypy'",
            "sys_platform == 'bar' or implementation_name == 'foo'",
        ));
        assert!(is_disjoint(
            "sys_platform == 'bar' or implementation_name == 'foo'",
            "sys_platform == 'darwin' and implementation_name == 'pypy'",
        ));
    }

    #[test]
    fn is_definitively_empty_set() {
        assert!(is_empty("'wat' == 'wat'"));
        assert!(is_empty(
            "python_version < '3.10' and python_version >= '3.10'"
        ));
        assert!(is_empty(
            "(python_version < '3.10' and python_version >= '3.10') \
             or (python_version < '3.9' and python_version >= '3.9')",
        ));

        assert!(!is_empty("python_version < '3.10'"));
        assert!(!is_empty("python_version < '0'"));
        assert!(!is_empty(
            "python_version < '3.10' and python_version >= '3.9'"
        ));
        assert!(!is_empty(
            "python_version < '3.10' or python_version >= '3.11'"
        ));
    }

    fn test_version_bounds_disjointness(version: &str) {
        assert!(!is_disjoint(
            format!("{version} > '2.7.0'"),
            format!("{version} == '3.6.0'")
        ));
        assert!(!is_disjoint(
            format!("{version} >= '3.7.0'"),
            format!("{version} == '3.7.1'")
        ));
        assert!(!is_disjoint(
            format!("{version} >= '3.7.0'"),
            format!("'3.7.1' == {version}")
        ));

        assert!(is_disjoint(
            format!("{version} >= '3.7.1'"),
            format!("{version} == '3.7.0'")
        ));
        assert!(is_disjoint(
            format!("'3.7.1' <= {version}"),
            format!("{version} == '3.7.0'")
        ));

        assert!(is_disjoint(
            format!("{version} < '3.7.0'"),
            format!("{version} == '3.7.0'")
        ));
        assert!(is_disjoint(
            format!("'3.7.0' > {version}"),
            format!("{version} == '3.7.0'")
        ));
        assert!(is_disjoint(
            format!("{version} < '3.7.0'"),
            format!("{version} == '3.7.1'")
        ));

        assert!(is_disjoint(
            format!("{version} == '3.7.0'"),
            format!("{version} == '3.7.1'")
        ));
        assert!(is_disjoint(
            format!("{version} == '3.7.0'"),
            format!("{version} != '3.7.0'")
        ));
    }

    fn is_empty(tree: &str) -> bool {
        let tree = MarkerTree::parse_reporter(tree, &mut TracingReporter).unwrap();
        super::is_definitively_empty_set(&tree)
    }

    fn is_disjoint(one: impl AsRef<str>, two: impl AsRef<str>) -> bool {
        let one = MarkerTree::parse_reporter(one.as_ref(), &mut TracingReporter).unwrap();
        let two = MarkerTree::parse_reporter(two.as_ref(), &mut TracingReporter).unwrap();
        super::is_disjoint(&one, &two) && super::is_disjoint(&two, &one)
    }

    fn assert_marker_equal(one: impl AsRef<str>, two: impl AsRef<str>) {
        let bound = RequiresPythonBound::new(Included(Version::new([3, 8])));
        let tree1 = MarkerTree::parse_reporter(one.as_ref(), &mut TracingReporter).unwrap();
        let tree1 = normalize(tree1, Some(&bound)).unwrap();
        let tree2 = MarkerTree::parse_reporter(two.as_ref(), &mut TracingReporter).unwrap();
        assert_eq!(
            tree1.to_string(),
            tree2.to_string(),
            "failed to normalize {}",
            one.as_ref()
        );
    }

    fn assert_normalizes_to(before: impl AsRef<str>, after: impl AsRef<str>) {
        let bound = RequiresPythonBound::new(Included(Version::new([3, 8])));
        let normalized = MarkerTree::parse_reporter(before.as_ref(), &mut TracingReporter)
            .unwrap()
            .clone();
        let normalized = normalize(normalized, Some(&bound)).unwrap();
        assert_eq!(normalized.to_string(), after.as_ref());
    }

    fn assert_normalizes_out(before: impl AsRef<str>) {
        let bound = RequiresPythonBound::new(Included(Version::new([3, 8])));
        let normalized = MarkerTree::parse_reporter(before.as_ref(), &mut TracingReporter)
            .unwrap()
            .clone();
        assert!(normalize(normalized, Some(&bound)).is_none());
    }
}
