#![allow(clippy::enum_glob_use)]

use std::ops::Bound::{self, *};
use std::ops::RangeBounds;

use pep440_rs::{Operator, Version, VersionSpecifier};
use pep508_rs::{
    ExtraName, ExtraOperator, MarkerExpression, MarkerOperator, MarkerTree, MarkerValueString,
    MarkerValueVersion,
};

use crate::pubgrub::PubGrubSpecifier;

/// Returns `true` if there is no environment in which both marker trees can both apply, i.e.
/// the expression `first and second` is always false.
#[allow(dead_code)]
pub(crate) fn is_disjoint(first: &MarkerTree, second: &MarkerTree) -> bool {
    let (expr1, expr2) = match (first, second) {
        (MarkerTree::Expression(expr1), MarkerTree::Expression(expr2)) => (expr1, expr2),
        // `And` expressions are disjoint if any clause is disjoint.
        (other, MarkerTree::And(exprs)) | (MarkerTree::And(exprs), other) => {
            return exprs.iter().any(|tree1| is_disjoint(tree1, other))
        }
        // `Or` expressions are disjoint if all clauses are disjoint.
        (other, MarkerTree::Or(exprs)) | (MarkerTree::Or(exprs), other) => {
            return exprs.iter().all(|tree1| is_disjoint(tree1, other))
        }
    };

    match expr1 {
        MarkerExpression::Version { .. } | MarkerExpression::VersionInverted { .. } => {
            version_is_disjoint(expr1, expr2)
        }
        MarkerExpression::String { .. } | MarkerExpression::StringInverted { .. } => {
            string_is_disjoint(expr1, expr2)
        }
        MarkerExpression::Extra { operator, name } => extra_is_disjoint(operator, name, expr2),
        MarkerExpression::Arbitrary { .. } => false,
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
    let Some((key, range)) = keyed_range(this).unwrap() else {
        return false;
    };

    // if this is not a version expression it may intersect
    let Ok(Some((key2, range2))) = keyed_range(other) else {
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
fn keyed_range(
    expr: &MarkerExpression,
) -> Result<Option<(&MarkerValueVersion, pubgrub::range::Range<Version>)>, ()> {
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
            let Ok(specifier) = VersionSpecifier::from_version(operator, version.clone()) else {
                return Ok(None);
            };

            (key, specifier)
        }
        _ => return Err(()),
    };

    let Ok(pubgrub_specifier) = PubGrubSpecifier::try_from(&specifier) else {
        return Ok(None);
    };

    Ok(Some((key, pubgrub_specifier.into())))
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

    fn is_disjoint(one: impl AsRef<str>, two: impl AsRef<str>) -> bool {
        let one = MarkerTree::parse_reporter(one.as_ref(), &mut TracingReporter).unwrap();
        let two = MarkerTree::parse_reporter(two.as_ref(), &mut TracingReporter).unwrap();
        super::is_disjoint(&one, &two) && super::is_disjoint(&two, &one)
    }

    #[test]
    fn extra() {
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
    fn invalid() {
        assert!(!is_disjoint(
            "python_version == 'Linux'",
            "python_version == '3.7.1'"
        ));
    }

    #[test]
    fn version() {
        assert!(!is_disjoint(
            "os_name == 'Linux'",
            "python_version == '3.7.1'"
        ));

        test_version_bounds("python_version");

        assert!(!is_disjoint(
            "python_version == '3.7.*'",
            "python_version == '3.7.1'"
        ));
    }

    #[test]
    fn string() {
        assert!(!is_disjoint(
            "os_name == 'Linux'",
            "platform_version == '3.7.1'"
        ));
        assert!(!is_disjoint(
            "implementation_version == '3.7.0'",
            "python_version == '3.7.1'"
        ));

        // basic version bounds checking should still work with lexicographical comparisons
        test_version_bounds("platform_version");

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
    fn combined() {
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
    }

    fn test_version_bounds(version: &str) {
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
}
