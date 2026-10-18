use std::fmt;
use std::ops::Bound;

use arcstr::ArcStr;
use itertools::Itertools;
use version_ranges::Ranges;

use uv_pep440::{Version, VersionSpecifier};

use crate::marker::algebra::INTERNER;
use crate::marker::algebra::isop::{Cover, Guard};
use crate::marker::tree::{ContainerOperator, MarkerTree};
use crate::{ExtraOperator, MarkerExpression, MarkerOperator, MarkerTreeKind};

/// Returns a simplified DNF expression for a given marker tree.
///
/// Extract a shared cover before expanding it to clauses for user-facing output.
pub(crate) fn to_dnf(tree: MarkerTree) -> Vec<Vec<MarkerExpression>> {
    let cover = INTERNER.lock().isop(tree.0);
    let mut dnf = Vec::new();
    collect_cover(&cover, &mut dnf, &mut Vec::new());
    sort(&mut dnf);
    dnf
}

/// Expand a shared cover, rendering each single-variable guard with the marker range rules.
fn collect_cover(
    cover: &Cover,
    dnf: &mut Vec<Vec<MarkerExpression>>,
    path: &mut Vec<MarkerExpression>,
) {
    match cover {
        Cover::False => {}
        Cover::True => {
            if !path.is_empty() {
                dnf.push(path.clone());
            }
        }
        Cover::Guard(guard) => collect_guard(Guard::Node(*guard), &Cover::True, dnf, path),
        Cover::Branches(branches) => {
            for (guard, suffix) in branches.iter() {
                collect_guard(*guard, suffix, dnf, path);
            }
        }
    }
}

/// Render a single-variable guard directly into the path before expanding its suffix.
fn collect_guard(
    guard: Guard,
    suffix: &Cover,
    dnf: &mut Vec<Vec<MarkerExpression>>,
    path: &mut Vec<MarkerExpression>,
) {
    let (tree, child) = match guard {
        Guard::Node(node) => (MarkerTree(node), MarkerTree::TRUE),
        Guard::Edge { node, child } => (MarkerTree(node), MarkerTree(child)),
    };
    match tree.kind() {
        // Reached a `false` node, meaning the conjunction is irrelevant for DNF.
        MarkerTreeKind::False => {}
        MarkerTreeKind::True => collect_cover(suffix, dnf, path),
        MarkerTreeKind::Version(marker) => {
            let range = guard_range(marker.edges(), child);
            // Detect whether the range can be simplified as an inequality.
            if let Some(excluded) = range_inequality(&range) {
                let current = path.len();
                for version in excluded {
                    path.push(MarkerExpression::Version {
                        key: marker.key().into(),
                        specifier: VersionSpecifier::not_equals_version(version.clone()),
                    });
                }

                collect_cover(suffix, dnf, path);
                path.truncate(current);
                return;
            }

            // Detect whether the range can be simplified as a star specifier.
            if let Some(specifier) = star_range_specifier(&range) {
                path.push(MarkerExpression::Version {
                    key: marker.key().into(),
                    specifier,
                });

                collect_cover(suffix, dnf, path);
                path.pop();
                return;
            }

            for bounds in range.iter() {
                let current = path.len();
                for specifier in VersionSpecifier::from_release_only_bounds(bounds) {
                    path.push(MarkerExpression::Version {
                        key: marker.key().into(),
                        specifier,
                    });
                }

                collect_cover(suffix, dnf, path);
                path.truncate(current);
            }
        }
        MarkerTreeKind::VersionString(marker) => {
            let range = guard_range(marker.edges(), child);
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
                collect_cover(suffix, dnf, path);
                path.truncate(current);
            }
        }
        MarkerTreeKind::String(marker) => {
            let range = guard_range(marker.children(), child);
            // Detect whether the range can be simplified as an inequality.
            if let Some(excluded) = range_inequality(&range) {
                let current = path.len();
                for value in excluded {
                    path.push(MarkerExpression::String {
                        key: marker.key().into(),
                        operator: MarkerOperator::NotEqual,
                        value: value.clone(),
                    });
                }

                collect_cover(suffix, dnf, path);
                path.truncate(current);
                return;
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

                collect_cover(suffix, dnf, path);
                path.truncate(current);
            }
        }
        MarkerTreeKind::In(marker) => {
            for (value, _) in marker.children().filter(|(_, tree)| *tree == child) {
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
                collect_cover(suffix, dnf, path);
                path.pop();
            }
        }
        MarkerTreeKind::Contains(marker) => {
            for (value, _) in marker.children().filter(|(_, tree)| *tree == child) {
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
                collect_cover(suffix, dnf, path);
                path.pop();
            }
        }
        MarkerTreeKind::List(marker) => {
            for (is_high, _) in marker.children().filter(|(_, tree)| *tree == child) {
                let expr = MarkerExpression::List {
                    pair: marker.pair().clone(),
                    operator: if is_high {
                        ContainerOperator::In
                    } else {
                        ContainerOperator::NotIn
                    },
                };

                path.push(expr);
                collect_cover(suffix, dnf, path);
                path.pop();
            }
        }
        MarkerTreeKind::Extra(marker) => {
            for (value, _) in marker.children().filter(|(_, tree)| *tree == child) {
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
                collect_cover(suffix, dnf, path);
                path.pop();
            }
        }
    }
}

/// Collect only the accepting ranges of a single-variable guard in one pass.
fn guard_range<'a, T: Clone + Ord + 'a>(
    edges: impl Iterator<Item = (&'a Ranges<T>, MarkerTree)>,
    child: MarkerTree,
) -> Ranges<T> {
    edges
        .filter(|(_, tree)| *tree == child)
        .flat_map(|(range, _)| range.iter())
        .map(|(start, end)| (start.cloned(), end.cloned()))
        .collect()
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
