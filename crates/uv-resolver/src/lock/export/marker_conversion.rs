use std::str::FromStr;

use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep508::{
    CanonicalMarkerListPair, ContainerOperator, ExtraOperator, MarkerExpression, MarkerTree,
};

/// Converts a universal marker (with encoded extras/groups) to PEP 751 marker syntax.
///
/// This function transforms markers that contain encoded extras and dependency groups
/// into the PEP 751 format. For example:
/// - `extra == 'extra-3-pkg-cpu'` → `'cpu' in extras`
/// - `extra == 'group-5-myapp-dev'` → `'dev' in dependency_groups`
/// - `extra == 'project-5-myapp'` → (removed, project activation is implicit)
/// - Regular PEP 508 markers → unchanged
///
/// The encoding scheme uses the format: `{kind}-{length}-{package}-{name}` where:
/// - `kind` is "extra", "group", or "project"
/// - `length` is the byte length of the package name
/// - `package` is the package name
/// - `name` is the extra or group name (not present for "project")
pub(crate) fn to_pep751_marker(marker: MarkerTree) -> MarkerTree {
    // Short-circuit for trivial cases
    if marker.is_true() {
        return MarkerTree::TRUE;
    }
    if marker.is_false() {
        return MarkerTree::FALSE;
    }

    // Convert the marker to DNF for easier processing
    let dnf = marker.to_dnf();

    // If DNF is empty after conversion, the original marker was TRUE
    if dnf.is_empty() {
        return MarkerTree::TRUE;
    }

    let mut result = MarkerTree::FALSE;

    // Process each clause in the DNF
    for clause in dnf {
        let mut clause_result = MarkerTree::TRUE;

        for expr in clause {
            let converted = convert_expression(expr);

            // If conversion returns None, it means the marker should be removed (project activation)
            if let Some(converted_expr) = converted {
                clause_result.and(MarkerTree::expression(converted_expr));
            }
        }

        result.or(clause_result);
    }

    result
}

/// Convert a single marker expression from universal to PEP 751 format.
///
/// Returns `None` if the expression is a project activation marker (which should be removed).
fn convert_expression(expr: MarkerExpression) -> Option<MarkerExpression> {
    match &expr {
        MarkerExpression::Extra { name, operator } => {
            // Check if this is an encoded extra/group/project
            if let Some(extra_name) = name.as_extra() {
                if let Some(parsed) = parse_encoded_marker(extra_name.as_ref()) {
                    match parsed {
                        EncodedMarker::Extra { extra, .. } => {
                            // Convert to PEP 751 list syntax: 'extra_name' in extras
                            Some(MarkerExpression::List {
                                pair: CanonicalMarkerListPair::Extras(extra),
                                operator: match operator {
                                    ExtraOperator::Equal => ContainerOperator::In,
                                    ExtraOperator::NotEqual => ContainerOperator::NotIn,
                                },
                            })
                        }
                        EncodedMarker::Group { group, .. } => {
                            // Convert to PEP 751 list syntax: 'group_name' in dependency_groups
                            Some(MarkerExpression::List {
                                pair: CanonicalMarkerListPair::DependencyGroup(group),
                                operator: match operator {
                                    ExtraOperator::Equal => ContainerOperator::In,
                                    ExtraOperator::NotEqual => ContainerOperator::NotIn,
                                },
                            })
                        }
                        EncodedMarker::Project { .. } => {
                            // Project activation is implicit in PEP 751, so we remove this marker
                            // Return None to indicate it should be removed
                            None
                        }
                    }
                } else {
                    // Not an encoded marker, keep as-is
                    Some(expr)
                }
            } else {
                // Invalid extra name (MarkerValueExtra::Arbitrary), keep as-is
                Some(expr)
            }
        }
        // All other expression types pass through unchanged
        _ => Some(expr),
    }
}

/// Represents a parsed encoded marker from the universal marker system.
enum EncodedMarker {
    Extra {
        #[allow(dead_code)]
        package: PackageName,
        extra: ExtraName,
    },
    Group {
        #[allow(dead_code)]
        package: PackageName,
        group: GroupName,
    },
    Project {
        #[allow(dead_code)]
        package: PackageName,
    },
}

/// Parse an encoded marker string like "extra-3-pkg-cpu" into its components.
///
/// Returns `None` if the string is not an encoded marker.
fn parse_encoded_marker(s: &str) -> Option<EncodedMarker> {
    // Split on the first dash to get the kind
    let (kind, rest) = s.split_once('-')?;

    match kind {
        "extra" | "group" => {
            // Format: {kind}-{len}-{package}-{name}
            let (len_str, rest) = rest.split_once('-')?;
            let len: usize = len_str.parse().ok()?;

            // Split at the package length boundary
            if rest.len() < len + 1 {
                // Need at least len chars for package + 1 dash + name
                return None;
            }

            let package_str = &rest[..len];
            let rest = &rest[len..];

            // Should start with a dash
            if !rest.starts_with('-') {
                return None;
            }

            let name_str = &rest[1..]; // Skip the dash

            // Parse the package name
            let package = PackageName::from_str(package_str).ok()?;

            if kind == "extra" {
                let extra = ExtraName::from_str(name_str).ok()?;
                Some(EncodedMarker::Extra { package, extra })
            } else {
                let group = GroupName::from_str(name_str).ok()?;
                Some(EncodedMarker::Group { package, group })
            }
        }
        "project" => {
            // Format: project-{len}-{package}
            let (len_str, rest) = rest.split_once('-')?;
            let len: usize = len_str.parse().ok()?;

            // The rest should be exactly the package name
            if rest.len() != len {
                return None;
            }

            let package = PackageName::from_str(rest).ok()?;
            Some(EncodedMarker::Project { package })
        }
        _ => None,
    }
}
