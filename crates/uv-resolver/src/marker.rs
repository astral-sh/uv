#![allow(clippy::enum_glob_use)]

use std::collections::HashMap;
use std::ops::Bound::{self, *};
use std::ops::RangeBounds;

use pep440_rs::{Operator, Version, VersionSpecifier};
use pep508_rs::{
    ExtraName, ExtraOperator, MarkerExpression, MarkerOperator, MarkerTree, MarkerValueString,
    MarkerValueVersion,
};

use crate::pubgrub::PubGrubSpecifier;
use pubgrub::range::Range as PubGrubRange;

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
        (MarkerExpression::Version { .. } | MarkerExpression::VersionInverted { .. }, expr2) => {
            version_is_disjoint(expr1, expr2)
        }
        (MarkerExpression::String { .. } | MarkerExpression::StringInverted { .. }, expr2) => {
            string_is_disjoint(expr1, expr2)
        }
        (MarkerExpression::Extra { operator, name }, expr2) => {
            extra_is_disjoint(operator, name, expr2)
        }
    }
}

/// Returns `true` if this string expression does not intersect with the given expression.
fn string_is_disjoint(this: &MarkerExpression, other: &MarkerExpression) -> bool {
    use MarkerOperator::*;

    let (key, operator, value) = extract_string_expression(this).unwrap();
    let Some((key2, operator2, value2)) = extract_string_expression(other) else {
        return false;
    };

    // distinct string expressions are not disjoint
    if key != key2 {
        return false;
    }

    match (operator, operator2) {
        // the only disjoint expressions involving strict inequality are `key != value` and `key == value`
        (NotEqual, Equal) | (Equal, NotEqual) => return value == value2,
        (NotEqual, _) | (_, NotEqual) => return false,
        // similarly for `in` and `not in`
        (In, NotIn) | (NotIn, In) => return value == value2,
        (In | NotIn, _) | (_, In | NotIn) => return false,
        _ => {}
    }

    let bounds = string_bounds(value, operator);
    let bounds2 = string_bounds(value2, operator2);

    // make sure the ranges do not intersection
    if range_exists::<&str>(&bounds2.start_bound(), &bounds.end_bound())
        && range_exists::<&str>(&bounds.start_bound(), &bounds2.end_bound())
    {
        return false;
    }

    true
}

/// Normalizes this marker tree.
///
/// This function does a number of operations to normalize a marker tree recursively:
/// - Sort all nested expressions.
/// - Simplify expressions. This includes combining overlapping version ranges and removing duplicate
///   expressions at the same level of precedence. For example, `(a == 'a' and a == 'a') or b == 'b'` can
///   be reduced, but `a == 'a' and (a == 'a' or b == 'b')` cannot.
/// - Normalize the order of version expressions to the form `<version key> <version op> <version>`
///  (i.e. not the reverse).
///
/// This is useful in cases where creating conjunctions or disjunctions might occur in a non-deterministic
/// order. This routine will attempt to erase the distinction created by such a construction.
pub(crate) fn normalize(tree: MarkerTree) -> Option<MarkerTree> {
    match tree {
        MarkerTree::And(trees) => {
            let mut reduced = Vec::new();
            let mut versions: HashMap<_, Vec<_>> = HashMap::new();

            for subtree in trees {
                // Simplify nested expressions as much as possible first.
                //
                // If the expression gets normalized out (e.g., `version < '3.8' and version >= '3.8'`), omit it.
                let Some(subtree) = normalize(subtree) else {
                    continue;
                };

                // Extract expressions we may be able to simplify more.
                if let MarkerTree::Expression(ref expr) = subtree {
                    if let Some((key, range)) = keyed_range(expr) {
                        versions.entry(key.clone()).or_default().push(range);
                        continue;
                    }
                }
                reduced.push(subtree);
            }

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
            let mut versions: HashMap<_, Vec<_>> = HashMap::new();

            for subtree in trees {
                // Simplify nested expressions as much as possible first.
                //
                // If the expression gets normalized out (e.g., `version < '3.8' and version >= '3.8'`), return `true`.
                let subtree = normalize(subtree)?;

                // Extract expressions we may be able to simplify more.
                if let MarkerTree::Expression(ref expr) = subtree {
                    if let Some((key, range)) = keyed_range(expr) {
                        versions.entry(key.clone()).or_default().push(range);
                        continue;
                    }
                }

                reduced.push(subtree);
            }

            simplify_ranges(&mut reduced, versions, |ranges| {
                ranges
                    .iter()
                    .fold(PubGrubRange::empty(), |acc, range| acc.union(range))
            });

            reduced.sort();
            reduced.dedup();

            match reduced.len() {
                0 => None,
                1 => Some(reduced.remove(0)),
                _ => Some(MarkerTree::Or(reduced)),
            }
        }

        MarkerTree::Expression(_) => Some(tree),
    }
}

// Simplify version expressions.
fn simplify_ranges(
    reduced: &mut Vec<MarkerTree>,
    versions: HashMap<MarkerValueVersion, Vec<PubGrubRange<Version>>>,
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
        MarkerExpression::StringInverted {
            value,
            operator,
            key,
        } => {
            // if the expression was inverted, we have to reverse the operator
            Some((key, reverse_marker_operator(*operator), value))
        }
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
        NotEqual | In | NotIn => unreachable!(),
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

/// Returns the key and version range for a version expression.
fn keyed_range(expr: &MarkerExpression) -> Option<(&MarkerValueVersion, PubGrubRange<Version>)> {
    let (key, specifier) = match expr {
        MarkerExpression::Version { key, specifier } => (key, specifier.clone()),
        MarkerExpression::VersionInverted {
            version,
            operator,
            key,
        } => {
            // if the expression was inverted, we have to reverse the operator before constructing
            // a version specifier
            let operator = reverse_operator(*operator);
            let specifier = VersionSpecifier::from_version(operator, version.clone()).ok()?;

            (key, specifier)
        }
        _ => return None,
    };

    let pubgrub_specifier = PubGrubSpecifier::try_from(&specifier).ok()?;

    Some((key, pubgrub_specifier.into()))
}

/// Reverses a binary operator.
fn reverse_operator(operator: Operator) -> Operator {
    use Operator::*;
    match operator {
        LessThan => GreaterThan,
        LessThanEqual => GreaterThanEqual,
        GreaterThan => LessThan,
        GreaterThanEqual => LessThanEqual,
        _ => operator,
    }
}

/// Reverses a marker operator.
fn reverse_marker_operator(operator: MarkerOperator) -> MarkerOperator {
    use MarkerOperator::*;
    match operator {
        LessThan => GreaterThan,
        LessEqual => GreaterEqual,
        GreaterThan => LessThan,
        GreaterEqual => LessEqual,
        _ => operator,
    }
}

#[cfg(test)]
mod tests {
    use pep508_rs::TracingReporter;

    use super::*;

    #[test]
    fn simplify() {
        assert_marker_equal(
            "python_version == '3.1' or python_version == '3.1'",
            "python_version == '3.1'",
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
            "python_version >= '3.17.post5'",
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

        // cannot simplify nested complex expressions
        assert_marker_equal(
            "extra == 'a' and (extra == 'a' or extra == 'b')",
            "extra == 'a' and (extra == 'a' or extra == 'b')",
        );

        assert_normalizes_out("python_version < '3.12.0rc1' or python_version >= '3.12.0rc1'");

        assert_normalizes_out(
            "extra == 'a' or (python_version < '3.12.0rc1' or python_version >= '3.12.0rc1')",
        );

        assert_normalizes_to(
            "extra == 'a' and (python_version < '3.12.0rc1' or python_version >= '3.12.0rc1')",
            "extra == 'a'",
        );
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
        assert!(is_disjoint("'Linux' in os_name", "os_name not in 'Linux'"));
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

    fn is_disjoint(one: impl AsRef<str>, two: impl AsRef<str>) -> bool {
        let one = MarkerTree::parse_reporter(one.as_ref(), &mut TracingReporter).unwrap();
        let two = MarkerTree::parse_reporter(two.as_ref(), &mut TracingReporter).unwrap();
        super::is_disjoint(&one, &two) && super::is_disjoint(&two, &one)
    }

    fn assert_marker_equal(one: impl AsRef<str>, two: impl AsRef<str>) {
        let tree1 = MarkerTree::parse_reporter(one.as_ref(), &mut TracingReporter).unwrap();
        let tree1 = normalize(tree1).unwrap();
        let tree2 = MarkerTree::parse_reporter(two.as_ref(), &mut TracingReporter).unwrap();
        assert_eq!(tree1.to_string(), tree2.to_string());
    }

    fn assert_normalizes_to(before: impl AsRef<str>, after: impl AsRef<str>) {
        let normalized = MarkerTree::parse_reporter(before.as_ref(), &mut TracingReporter)
            .unwrap()
            .clone();
        let normalized = normalize(normalized).unwrap();
        assert_eq!(normalized.to_string(), after.as_ref());
    }

    fn assert_normalizes_out(before: impl AsRef<str>) {
        let normalized = MarkerTree::parse_reporter(before.as_ref(), &mut TracingReporter)
            .unwrap()
            .clone();
        assert!(normalize(normalized).is_none());
    }
}
