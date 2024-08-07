use std::fmt;
use std::ops::Bound;

use indexmap::IndexMap;
use itertools::Itertools;
use pep440_rs::VersionSpecifier;
use pubgrub::Range;
use rustc_hash::FxBuildHasher;

use crate::{ExtraOperator, MarkerExpression, MarkerOperator, MarkerTree, MarkerTreeKind};

pub(crate) fn to_dnf(tree: &MarkerTree) -> Vec<Vec<MarkerExpression>> {
    let mut dnf = Vec::new();
    collect_dnf(tree, &mut dnf, &mut Vec::new());

    let mut redundant_solutions = Vec::new();
    'redundant: for i in 0..dnf.len() {
        let solution = &dnf[i];

        let mut redundant_clauses = Vec::new();
        for (j, other_solution) in dnf.iter().enumerate() {
            if i == j || redundant_solutions.contains(&j) {
                continue;
            }

            if other_solution
                .iter()
                .all(|clause| solution.contains(&clause))
            {
                redundant_solutions.push(i);
                continue 'redundant;
            }

            for (i, skip_clause) in solution.iter().enumerate() {
                if redundant_clauses.contains(&i) {
                    continue;
                }

                let negation = skip_clause.negate();
                if other_solution.iter().all(|clause| {
                    if clause == skip_clause {
                        return false;
                    }

                    solution.contains(clause)
                        || negation
                            .as_ref()
                            .is_some_and(|negation| *negation == *clause)
                }) {
                    redundant_clauses.push(i);
                }
            }
        }

        redundant_clauses.sort_by(|a, b| b.cmp(a));
        for clause in redundant_clauses {
            dnf[i].remove(clause);
        }
    }

    for i in redundant_solutions.into_iter().rev() {
        dnf.remove(i);
    }

    dnf
}

fn collect_dnf(
    tree: &MarkerTree,
    dnf: &mut Vec<Vec<MarkerExpression>>,
    path: &mut Vec<MarkerExpression>,
) {
    match tree.kind() {
        MarkerTreeKind::False => return,
        MarkerTreeKind::True => {
            if !path.is_empty() {
                dnf.push(path.clone())
            }
        }
        MarkerTreeKind::Version(marker) => {
            for (tree, range) in collect_paths(marker.children()) {
                if let Some(excluded) = range_inequality(&range) {
                    let current = path.len();
                    for version in excluded {
                        path.push(MarkerExpression::Version {
                            key: marker.key().clone(),
                            specifier: VersionSpecifier::not_equals_version(version.clone()),
                        });
                    }

                    collect_dnf(&tree, dnf, path);
                    path.truncate(current);
                    continue;
                }

                for bounds in range.iter() {
                    let current = path.len();
                    for specifier in VersionSpecifier::from_bounds(bounds) {
                        path.push(MarkerExpression::Version {
                            key: marker.key().clone(),
                            specifier,
                        });
                    }

                    collect_dnf(&tree, dnf, path);
                    path.truncate(current);
                }
            }
        }
        MarkerTreeKind::String(marker) => {
            for (tree, range) in collect_paths(marker.children()) {
                if let Some(excluded) = range_inequality(&range) {
                    let current = path.len();
                    for value in excluded {
                        path.push(MarkerExpression::String {
                            key: marker.key().clone(),
                            operator: MarkerOperator::NotEqual,
                            value: value.clone(),
                        });
                    }

                    collect_dnf(&tree, dnf, path);
                    path.truncate(current);
                    continue;
                }

                for bounds in range.iter() {
                    let current = path.len();
                    for (operator, value) in MarkerOperator::from_bounds(bounds) {
                        path.push(MarkerExpression::String {
                            key: marker.key().clone(),
                            operator,
                            value: value.clone(),
                        });
                    }

                    collect_dnf(&tree, dnf, path);
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
                    key: marker.key().clone(),
                    value: marker.value().to_owned(),
                    operator,
                };

                path.push(expr);
                collect_dnf(&tree, dnf, path);
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
                    key: marker.key().clone(),
                    value: marker.value().to_owned(),
                    operator,
                };

                path.push(expr);
                collect_dnf(&tree, dnf, path);
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
                    name: marker.name().clone(),
                    operator,
                };

                path.push(expr);
                collect_dnf(&tree, dnf, path);
                path.pop();
            }
        }
    }
}

fn collect_paths<'a, T>(
    map: impl ExactSizeIterator<Item = (&'a Range<T>, MarkerTree)>,
) -> IndexMap<MarkerTree, Range<T>, FxBuildHasher>
where
    T: Ord + Clone + 'a,
{
    let len = map.len();

    let mut paths: IndexMap<_, Range<_>, FxBuildHasher> = IndexMap::default();
    for (i, (range, tree)) in map.enumerate() {
        let (mut start, mut end) = range.bounding_range().unwrap();
        match (start, end) {
            (Bound::Included(v1), Bound::Included(v2)) if v1 == v2 => {}
            _ => {
                if i == 0 {
                    start = Bound::Unbounded;
                }
                if i == len - 1 {
                    end = Bound::Unbounded;
                }
            }
        }
        let range = Range::from_range_bounds((start.cloned(), end.cloned()));

        paths
            .entry(tree)
            .and_modify(|union| *union = union.union(&range))
            .or_insert_with(|| range.clone());
    }

    paths
}

fn range_inequality<T>(range: &Range<T>) -> Option<Vec<&T>>
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
