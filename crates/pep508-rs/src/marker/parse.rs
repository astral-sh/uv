use std::str::FromStr;

use pep440_rs::{Version, VersionPattern, VersionSpecifier};
use uv_normalize::ExtraName;

use crate::cursor::Cursor;
use crate::{
    ExtraOperator, MarkerExpression, MarkerOperator, MarkerTree, MarkerValue, MarkerValueVersion,
    MarkerWarningKind, Pep508Error, Pep508ErrorSource, Pep508Url, Reporter,
};

/// ```text
/// version_cmp   = wsp* <'<=' | '<' | '!=' | '==' | '>=' | '>' | '~=' | '==='>
/// marker_op     = version_cmp | (wsp* 'in') | (wsp* 'not' wsp+ 'in')
/// ```
/// The `wsp*` has already been consumed by the caller.
fn parse_marker_operator<T: Pep508Url>(
    cursor: &mut Cursor,
) -> Result<MarkerOperator, Pep508Error<T>> {
    let (start, len) = if cursor.peek_char().is_some_and(char::is_alphabetic) {
        // "in" or "not"
        cursor.take_while(|char| !char.is_whitespace() && char != '\'' && char != '"')
    } else {
        // A mathematical operator
        cursor.take_while(|char| matches!(char, '<' | '=' | '>' | '~' | '!'))
    };
    let operator = cursor.slice(start, len);
    if operator == "not" {
        // 'not' wsp+ 'in'
        match cursor.next() {
            None => {
                return Err(Pep508Error {
                    message: Pep508ErrorSource::String(
                        "Expected whitespace after 'not', found end of input".to_string(),
                    ),
                    start: cursor.pos(),
                    len: 1,
                    input: cursor.to_string(),
                });
            }
            Some((_, whitespace)) if whitespace.is_whitespace() => {}
            Some((pos, other)) => {
                return Err(Pep508Error {
                    message: Pep508ErrorSource::String(format!(
                        "Expected whitespace after 'not', found '{other}'"
                    )),
                    start: pos,
                    len: other.len_utf8(),
                    input: cursor.to_string(),
                });
            }
        };
        cursor.eat_whitespace();
        cursor.next_expect_char('i', cursor.pos())?;
        cursor.next_expect_char('n', cursor.pos())?;
        return Ok(MarkerOperator::NotIn);
    }
    MarkerOperator::from_str(operator).map_err(|_| Pep508Error {
        message: Pep508ErrorSource::String(format!(
            "Expected a valid marker operator (such as '>=' or 'not in'), found '{operator}'"
        )),
        start,
        len,
        input: cursor.to_string(),
    })
}

/// Either a single or double quoted string or one of '`python_version`', '`python_full_version`',
/// '`os_name`', '`sys_platform`', '`platform_release`', '`platform_system`', '`platform_version`',
/// '`platform_machine`', '`platform_python_implementation`', '`implementation_name`',
/// '`implementation_version`', 'extra'
pub(crate) fn parse_marker_value<T: Pep508Url>(
    cursor: &mut Cursor,
) -> Result<MarkerValue, Pep508Error<T>> {
    // > User supplied constants are always encoded as strings with either ' or " quote marks. Note
    // > that backslash escapes are not defined, but existing implementations do support them. They
    // > are not included in this specification because they add complexity and there is no observable
    // > need for them today. Similarly we do not define non-ASCII character support: all the runtime
    // > variables we are referencing are expected to be ASCII-only.
    match cursor.peek() {
        None => Err(Pep508Error {
            message: Pep508ErrorSource::String(
                "Expected marker value, found end of dependency specification".to_string(),
            ),
            start: cursor.pos(),
            len: 1,
            input: cursor.to_string(),
        }),
        // It can be a string ...
        Some((start_pos, quotation_mark @ ('"' | '\''))) => {
            cursor.next();
            let (start, len) = cursor.take_while(|c| c != quotation_mark);
            let value = cursor.slice(start, len).to_string();
            cursor.next_expect_char(quotation_mark, start_pos)?;
            Ok(MarkerValue::QuotedString(value))
        }
        // ... or it can be a keyword
        Some(_) => {
            let (start, len) = cursor.take_while(|char| {
                !char.is_whitespace() && !['>', '=', '<', '!', '~', ')'].contains(&char)
            });
            let key = cursor.slice(start, len);
            MarkerValue::from_str(key).map_err(|_| Pep508Error {
                message: Pep508ErrorSource::String(format!(
                    "Expected a valid marker name, found '{key}'"
                )),
                start,
                len,
                input: cursor.to_string(),
            })
        }
    }
}

/// ```text
/// marker_var:l marker_op:o marker_var:r
/// ```
pub(crate) fn parse_marker_key_op_value<T: Pep508Url>(
    cursor: &mut Cursor,
    reporter: &mut impl Reporter,
) -> Result<MarkerExpression, Pep508Error<T>> {
    cursor.eat_whitespace();
    let l_value = parse_marker_value(cursor)?;
    cursor.eat_whitespace();
    // "not in" and "in" must be preceded by whitespace. We must already have matched a whitespace
    // when we're here because other `parse_marker_key` would have pulled the characters in and
    // errored
    let operator = parse_marker_operator(cursor)?;
    cursor.eat_whitespace();
    let r_value = parse_marker_value(cursor)?;

    // Convert a `<marker_value> <marker_op> <marker_value>` expression into it's
    // typed equivalent.
    let expr = match l_value {
        // The only sound choice for this is `<version key> <version op> <quoted PEP 440 version>`
        MarkerValue::MarkerEnvVersion(key) => {
            let MarkerValue::QuotedString(value) = r_value else {
                reporter.report(
                    MarkerWarningKind::Pep440Error,
                    format!(
                        "Expected double quoted PEP 440 version to compare with {key},
                        found {r_value}, will evaluate to false"
                    ),
                );

                return Ok(MarkerExpression::arbitrary(
                    MarkerValue::MarkerEnvVersion(key),
                    operator,
                    r_value,
                ));
            };

            match parse_version_expr(key.clone(), operator, &value, reporter) {
                Some(expr) => expr,
                None => MarkerExpression::arbitrary(
                    MarkerValue::MarkerEnvVersion(key),
                    operator,
                    MarkerValue::QuotedString(value),
                ),
            }
        }
        // The only sound choice for this is `<env key> <op> <string>`
        MarkerValue::MarkerEnvString(key) => {
            let value = match r_value {
                MarkerValue::Extra
                | MarkerValue::MarkerEnvVersion(_)
                | MarkerValue::MarkerEnvString(_) => {
                    reporter.report(
                        MarkerWarningKind::MarkerMarkerComparison,
                        "Comparing two markers with each other doesn't make any sense,
                            will evaluate to false"
                            .to_string(),
                    );

                    return Ok(MarkerExpression::arbitrary(
                        MarkerValue::MarkerEnvString(key),
                        operator,
                        r_value,
                    ));
                }
                MarkerValue::QuotedString(r_string) => r_string,
            };

            MarkerExpression::String {
                key,
                operator,
                value,
            }
        }
        // `extra == '...'`
        MarkerValue::Extra => {
            let value = match r_value {
                MarkerValue::MarkerEnvVersion(_)
                | MarkerValue::MarkerEnvString(_)
                | MarkerValue::Extra => {
                    reporter.report(
                        MarkerWarningKind::ExtraInvalidComparison,
                        "Comparing extra with something other than a quoted string is wrong,
                            will evaluate to false"
                            .to_string(),
                    );

                    return Ok(MarkerExpression::arbitrary(l_value, operator, r_value));
                }
                MarkerValue::QuotedString(value) => value,
            };

            match parse_extra_expr(operator, &value, reporter) {
                Some(expr) => expr,
                None => MarkerExpression::arbitrary(
                    MarkerValue::Extra,
                    operator,
                    MarkerValue::QuotedString(value),
                ),
            }
        }
        // This is either MarkerEnvVersion, MarkerEnvString or Extra inverted
        MarkerValue::QuotedString(l_string) => {
            match r_value {
                // The only sound choice for this is `<quoted PEP 440 version> <version op>` <version key>
                MarkerValue::MarkerEnvVersion(key) => {
                    match parse_inverted_version_expr(&l_string, operator, key.clone(), reporter) {
                        Some(expr) => expr,
                        None => MarkerExpression::arbitrary(
                            MarkerValue::QuotedString(l_string),
                            operator,
                            MarkerValue::MarkerEnvVersion(key),
                        ),
                    }
                }
                // '...' == <env key>
                MarkerValue::MarkerEnvString(key) => MarkerExpression::String {
                    key,
                    // Invert the operator to normalize the expression order.
                    operator: operator.invert(),
                    value: l_string,
                },
                // `'...' == extra`
                MarkerValue::Extra => match parse_extra_expr(operator, &l_string, reporter) {
                    Some(expr) => expr,
                    None => MarkerExpression::arbitrary(
                        MarkerValue::QuotedString(l_string),
                        operator,
                        MarkerValue::Extra,
                    ),
                },
                // `'...' == '...'`, doesn't make much sense
                MarkerValue::QuotedString(_) => {
                    // Not even pypa/packaging 22.0 supports this
                    // https://github.com/pypa/packaging/issues/632
                    let expr = MarkerExpression::arbitrary(
                        MarkerValue::QuotedString(l_string),
                        operator,
                        r_value,
                    );

                    reporter.report(
                        MarkerWarningKind::StringStringComparison,
                        format!(
                            "Comparing two quoted strings with each other doesn't make sense:
                            {expr}, will evaluate to false"
                        ),
                    );

                    expr
                }
            }
        }
    };

    Ok(expr)
}

/// Creates an instance of [`MarkerExpression::Version`] with the given values.
///
/// Reports a warning on failure, and returns `None`.
fn parse_version_expr(
    key: MarkerValueVersion,
    marker_operator: MarkerOperator,
    value: &str,
    reporter: &mut impl Reporter,
) -> Option<MarkerExpression> {
    let pattern = match value.parse::<VersionPattern>() {
        Ok(pattern) => pattern,
        Err(err) => {
            reporter.report(
                MarkerWarningKind::Pep440Error,
                format!(
                    "Expected PEP 440 version to compare with {key}, found {value},
                    will evaluate to false: {err}"
                ),
            );

            return None;
        }
    };

    let Some(operator) = marker_operator.to_pep440_operator() else {
        reporter.report(
            MarkerWarningKind::Pep440Error,
            format!(
                "Expected PEP 440 version operator to compare {key} with '{version}',
                    found '{marker_operator}', will evaluate to false",
                version = pattern.version()
            ),
        );

        return None;
    };

    let specifier = match VersionSpecifier::from_pattern(operator, pattern) {
        Ok(specifier) => specifier,
        Err(err) => {
            reporter.report(
                MarkerWarningKind::Pep440Error,
                format!("Invalid operator/version combination: {err}"),
            );
            return None;
        }
    };

    Some(MarkerExpression::Version { key, specifier })
}

/// Creates an instance of [`MarkerExpression::Version`] from an inverted expression.
///
/// Reports a warning on failure, and returns `None`.
fn parse_inverted_version_expr(
    value: &str,
    marker_operator: MarkerOperator,
    key: MarkerValueVersion,
    reporter: &mut impl Reporter,
) -> Option<MarkerExpression> {
    // Invert the operator to normalize the expression order.
    let marker_operator = marker_operator.invert();

    // Not star allowed here, `'3.*' == python_version` is not a valid PEP 440 comparison.
    let version = match value.parse::<Version>() {
        Ok(version) => version,
        Err(err) => {
            reporter.report(
                MarkerWarningKind::Pep440Error,
                format!(
                    "Expected PEP 440 version to compare with {key}, found {value},
                    will evaluate to false: {err}"
                ),
            );

            return None;
        }
    };

    let Some(operator) = marker_operator.to_pep440_operator() else {
        reporter.report(
            MarkerWarningKind::Pep440Error,
            format!(
                "Expected PEP 440 version operator to compare {key} with '{version}',
                    found '{marker_operator}', will evaluate to false"
            ),
        );

        return None;
    };

    let specifier = match VersionSpecifier::from_version(operator, version) {
        Ok(specifier) => specifier,
        Err(err) => {
            reporter.report(
                MarkerWarningKind::Pep440Error,
                format!("Invalid operator/version combination: {err}"),
            );
            return None;
        }
    };

    Some(MarkerExpression::Version { key, specifier })
}

/// Creates an instance of [`MarkerExpression::Extra`] with the given values, falling back to
/// [`MarkerExpression::Arbitrary`] on failure.
fn parse_extra_expr(
    operator: MarkerOperator,
    value: &str,
    reporter: &mut impl Reporter,
) -> Option<MarkerExpression> {
    let name = match ExtraName::from_str(value) {
        Ok(name) => name,
        Err(err) => {
            reporter.report(
                MarkerWarningKind::ExtraInvalidComparison,
                format!("Expected extra name, found '{value}', will evaluate to false: {err}"),
            );

            return None;
        }
    };

    if let Some(operator) = ExtraOperator::from_marker_operator(operator) {
        return Some(MarkerExpression::Extra { operator, name });
    }

    reporter.report(
        MarkerWarningKind::ExtraInvalidComparison,
        "Comparing extra with something other than a quoted string is wrong,
        will evaluate to false"
            .to_string(),
    );
    None
}

/// ```text
/// marker_expr   = marker_var:l marker_op:o marker_var:r -> (o, l, r)
///               | wsp* '(' marker:m wsp* ')' -> m
/// ```
fn parse_marker_expr<T: Pep508Url>(
    cursor: &mut Cursor,
    reporter: &mut impl Reporter,
) -> Result<MarkerTree, Pep508Error<T>> {
    cursor.eat_whitespace();
    if let Some(start_pos) = cursor.eat_char('(') {
        let marker = parse_marker_or(cursor, reporter)?;
        cursor.next_expect_char(')', start_pos)?;
        Ok(marker)
    } else {
        Ok(MarkerTree::Expression(parse_marker_key_op_value(
            cursor, reporter,
        )?))
    }
}

/// ```text
/// marker_and    = marker_expr:l wsp* 'and' marker_expr:r -> ('and', l, r)
///               | marker_expr:m -> m
/// ```
fn parse_marker_and<T: Pep508Url>(
    cursor: &mut Cursor,
    reporter: &mut impl Reporter,
) -> Result<MarkerTree, Pep508Error<T>> {
    parse_marker_op(cursor, "and", MarkerTree::And, parse_marker_expr, reporter)
}

/// ```text
/// marker_or     = marker_and:l wsp* 'or' marker_and:r -> ('or', l, r)
///                   | marker_and:m -> m
/// ```
fn parse_marker_or<T: Pep508Url>(
    cursor: &mut Cursor,
    reporter: &mut impl Reporter,
) -> Result<MarkerTree, Pep508Error<T>> {
    parse_marker_op(cursor, "or", MarkerTree::Or, parse_marker_and, reporter)
}

/// Parses both `marker_and` and `marker_or`
fn parse_marker_op<T: Pep508Url, R: Reporter>(
    cursor: &mut Cursor,
    op: &str,
    op_constructor: fn(Vec<MarkerTree>) -> MarkerTree,
    parse_inner: fn(&mut Cursor, &mut R) -> Result<MarkerTree, Pep508Error<T>>,
    reporter: &mut R,
) -> Result<MarkerTree, Pep508Error<T>> {
    // marker_and or marker_expr
    let first_element = parse_inner(cursor, reporter)?;
    // wsp*
    cursor.eat_whitespace();
    // Check if we're done here instead of invoking the whole vec allocating loop
    if matches!(cursor.peek_char(), None | Some(')')) {
        return Ok(first_element);
    }

    let mut expressions = Vec::with_capacity(1);
    expressions.push(first_element);
    loop {
        // wsp*
        cursor.eat_whitespace();
        // ('or' marker_and) or ('and' marker_or)
        let (start, len) = cursor.peek_while(|c| !c.is_whitespace());
        match cursor.slice(start, len) {
            value if value == op => {
                cursor.take_while(|c| !c.is_whitespace());
                let expression = parse_inner(cursor, reporter)?;
                expressions.push(expression);
            }
            _ => {
                // Build minimal trees
                return if expressions.len() == 1 {
                    Ok(expressions.remove(0))
                } else {
                    Ok(op_constructor(expressions))
                };
            }
        }
    }
}

/// ```text
/// marker        = marker_or^
/// ```
pub(crate) fn parse_markers_cursor<T: Pep508Url>(
    cursor: &mut Cursor,
    reporter: &mut impl Reporter,
) -> Result<MarkerTree, Pep508Error<T>> {
    let marker = parse_marker_or(cursor, reporter)?;
    cursor.eat_whitespace();
    if let Some((pos, unexpected)) = cursor.next() {
        // If we're here, both parse_marker_or and parse_marker_and returned because the next
        // character was neither "and" nor "or"
        return Err(Pep508Error {
            message: Pep508ErrorSource::String(format!(
                "Unexpected character '{unexpected}', expected 'and', 'or' or end of input"
            )),
            start: pos,
            len: cursor.remaining(),
            input: cursor.to_string(),
        });
    };
    Ok(marker)
}

/// Parses markers such as `python_version < '3.8'` or
/// `python_version == "3.10" and (sys_platform == "win32" or (os_name == "linux" and implementation_name == 'cpython'))`
pub(crate) fn parse_markers<T: Pep508Url>(
    markers: &str,
    reporter: &mut impl Reporter,
) -> Result<MarkerTree, Pep508Error<T>> {
    let mut chars = Cursor::new(markers);
    parse_markers_cursor(&mut chars, reporter)
}
