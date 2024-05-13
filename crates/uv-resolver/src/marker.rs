use std::ops::Bound::{self, *};
use std::ops::RangeBounds;

use pep440_rs::{Operator, Version, VersionSpecifier};
use pep508_rs::{
    ExtraName, ExtraOperator, MarkerExpression, MarkerOperator, MarkerTree, MarkerValueString,
    MarkerValueVersion,
};

use crate::pubgrub::PubGrubSpecifier;

/// Returns `true` if there is no environment in which both marker trees can both apply, i.e.
/// the expression `this and other` is always false.
#[allow(dead_code)]
pub(crate) fn is_disjoint(this: &MarkerTree, other: &MarkerTree) -> bool {
    let (expr1, expr2) = match (this, other) {
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
        MarkerExpression::Arbitrary { .. } => return false,
    }
}

/// Returns `true` if this string expression does not intersect with the given expression.
fn string_is_disjoint(this: &MarkerExpression, other: &MarkerExpression) -> bool {
    let (key, operator, value) = extract_string_expression(this).unwrap();
    let Some((key2, operator2, value2)) = extract_string_expression(other) else {
        return false;
    };

    // distinct string expressions are not disjoint
    if key != key2 {
        return false;
    }

    use MarkerOperator::*;
    match (operator, operator2) {
        // the only disjoint expressions involving strict inequality are `key != value` and `key == value`
        (NotEqual, Equal) | (Equal, NotEqual) => return value == value2,
        (NotEqual, _) | (_, NotEqual) => return false,
        // similarly for `in` and `not in`
        (In, NotIn) | (NotIn, In) => return value == value2,
        (In | NotIn, _) | (_, In | NotIn) => return false,
        _ => {}
    }

    let bounds = string_bounds(value, &operator);
    let bounds2 = string_bounds(value2, &operator);

    // make sure the ranges do not intersection
    if range_exists::<&str>(&bounds2.start_bound(), &bounds.end_bound())
        || range_exists::<&str>(&bounds.start_bound(), &bounds2.end_bound())
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
            Some((key, reverse_marker_operator(operator), value))
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
fn string_bounds<'a>(
    value: &'a str,
    operator: &MarkerOperator,
) -> (Bound<&'a str>, Bound<&'a str>) {
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

    // if this is not a version expression it may interesect
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
            let operator = reverse_operator(operator);
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
fn reverse_operator(operator: &Operator) -> Operator {
    use Operator::*;
    match operator {
        LessThan => GreaterThanEqual,
        LessThanEqual => GreaterThan,
        GreaterThan => LessThanEqual,
        GreaterThanEqual => LessThan,
        _ => *operator,
    }
}

/// Reverses a marker operator.
fn reverse_marker_operator(operator: &MarkerOperator) -> MarkerOperator {
    use MarkerOperator::*;
    match operator {
        LessThan => GreaterEqual,
        LessEqual => GreaterThan,
        GreaterThan => LessEqual,
        GreaterEqual => LessThan,
        _ => *operator,
    }
}
