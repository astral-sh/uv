use uv_normalize::PackageName;
use uv_pep508::{CanonicalMarkerListPair, ContainerOperator, MarkerExpression, MarkerTree};
use uv_pypi_types::{ConflictKind, Conflicts};

use crate::universal_marker::{ParsedRawExtra, conflict_marker_to_pep751};

/// Converts uv's internal conflict-encoded markers into PEP 751 marker syntax.
///
/// uv encodes extras and dependency groups as synthetic `extra == 'extra-{len}-{pkg}-{name}'` /
/// `extra == 'group-{len}-{pkg}-{name}'` expressions. This function decodes those back into PEP 751's native
/// `'name' in extras` / `'name' in dependency_groups` list membership syntax.
///
/// Algorithm:
///   1. Expand the marker into disjunctive normal form (DNF) — a list of clauses OR'd together, each clause being a
///      conjunction (AND) of expressions.
///   2. For each clause, convert every expression via [`convert_expression`]: decode conflict-encoded extras/groups into
///      PEP 751 list expressions; drop expressions that are valid conflict markers but belong to a different root
///      package (they carry no meaning in the exported file); pass all other expressions through unchanged.
///   3. Check for impossible clauses: if a clause contains two `'x' in extras` (or `dependency_groups`) expressions
///      where both `x` and `y` belong to the same `ConflictSet`, the clause is always FALSE (mutual exclusion) and is
///      dropped.
///   4. AND the surviving expressions back into a clause, then OR all surviving clauses into the final result.
///
/// Example — given conflicts `{cpu, gpu}` for package `pkg`, and input marker:
///
///   `(extra == 'extra-3-pkg-cpu' AND extra == 'extra-3-pkg-gpu')
///     OR extra == 'extra-3-pkg-cpu'
///     OR (python_version >= '3.10' AND extra == 'extra-3-pkg-gpu')`
///
///   Step 1 — DNF yields three clauses:
///     clause A: `extra == 'extra-3-pkg-cpu' AND extra == 'extra-3-pkg-gpu'`
///     clause B: `extra == 'extra-3-pkg-cpu'`
///     clause C: `python_version >= '3.10' AND extra == 'extra-3-pkg-gpu'`
///
///   Step 2 — convert each expression to PEP 751 syntax:
///     clause A: `'cpu' in extras AND 'gpu' in extras`
///     clause B: `'cpu' in extras`
///     clause C: `python_full_version >= '3.10' AND 'gpu' in extras`
///
///   Step 3 — clause A has `'cpu' in extras` and `'gpu' in extras`, both from conflict set `{cpu, gpu}` → impossible,
///     drop it. Clauses B and C survive.
///
///   Step 4 — reassemble: `'cpu' in extras OR (python_full_version >= '3.10' AND 'gpu' in extras)`
pub(crate) fn to_pep751_marker(
    marker: MarkerTree,
    root_name: Option<&PackageName>,
    conflicts: Option<&Conflicts>,
) -> MarkerTree {
    if marker.is_true() {
        return MarkerTree::TRUE;
    }
    if marker.is_false() {
        return MarkerTree::FALSE;
    }

    let dnf = marker.to_dnf();
    if dnf.is_empty() {
        return MarkerTree::TRUE;
    }

    let mut result = MarkerTree::FALSE;
    for clause in dnf {
        let converted: Vec<MarkerExpression> = clause
            .into_iter()
            .filter_map(|expr| convert_expression(expr, root_name))
            .collect();

        if let Some((root, conflicts)) = root_name.zip(conflicts) {
            if has_conflict_pair(&converted, root, conflicts) {
                continue;
            }
        }

        let mut clause_result = MarkerTree::TRUE;
        for expr in converted {
            clause_result.and(MarkerTree::expression(expr));
        }
        result.or(clause_result);
    }

    result
}

fn convert_expression(
    expr: MarkerExpression,
    root_name: Option<&PackageName>,
) -> Option<MarkerExpression> {
    match &expr {
        MarkerExpression::Extra { name, operator } => {
            if let Some(extra_name) = name.as_extra() {
                if let Some(pep751_expr) =
                    conflict_marker_to_pep751(extra_name, operator, root_name)
                {
                    Some(pep751_expr)
                } else if ParsedRawExtra::parse(extra_name).is_ok() {
                    None
                } else {
                    Some(expr)
                }
            } else {
                Some(expr)
            }
        }
        _ => Some(expr),
    }
}

fn has_conflict_pair(
    exprs: &[MarkerExpression],
    root_name: &PackageName,
    conflicts: &Conflicts,
) -> bool {
    let in_pairs: Vec<&CanonicalMarkerListPair> = exprs
        .iter()
        .filter_map(|expr| match expr {
            MarkerExpression::List {
                pair,
                operator: ContainerOperator::In,
            } => Some(pair),
            _ => None,
        })
        .collect();

    for i in 0..in_pairs.len() {
        for j in (i + 1)..in_pairs.len() {
            if conflict_set_contains_pair(conflicts, root_name, in_pairs[i], in_pairs[j]) {
                return true;
            }
        }
    }
    false
}

fn conflict_set_contains_pair(
    conflicts: &Conflicts,
    root_name: &PackageName,
    a: &CanonicalMarkerListPair,
    b: &CanonicalMarkerListPair,
) -> bool {
    conflicts.iter().any(|set| {
        let contains = |pair: &CanonicalMarkerListPair| -> bool {
            match pair {
                CanonicalMarkerListPair::Extras(extra) => set.iter().any(|item| {
                    item.package() == root_name
                        && matches!(item.kind(), ConflictKind::Extra(e) if e == extra)
                }),
                CanonicalMarkerListPair::DependencyGroup(group) => set.iter().any(|item| {
                    item.package() == root_name
                        && matches!(item.kind(), ConflictKind::Group(g) if g == group)
                }),
                CanonicalMarkerListPair::Arbitrary { .. } => false,
            }
        };
        contains(a) && contains(b)
    })
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use uv_normalize::{ExtraName, GroupName, PackageName};
    use uv_pypi_types::{ConflictItem, ConflictSet, Conflicts};

    use super::to_pep751_marker;

    fn create_package(name: &str) -> PackageName {
        PackageName::from_str(name).unwrap()
    }

    fn create_extra(name: &str) -> ExtraName {
        ExtraName::from_str(name).unwrap()
    }

    fn create_group(name: &str) -> GroupName {
        GroupName::from_str(name).unwrap()
    }

    fn create_conflicts(it: impl IntoIterator<Item = ConflictSet>) -> Conflicts {
        let mut conflicts = Conflicts::empty();
        for set in it {
            conflicts.push(set);
        }
        conflicts
    }

    fn create_set<'a>(it: impl IntoIterator<Item = &'a str>) -> ConflictSet {
        let items = it
            .into_iter()
            .map(|extra| (create_package("pkg"), create_extra(extra)))
            .map(ConflictItem::from)
            .collect::<Vec<ConflictItem>>();
        ConflictSet::try_from(items).unwrap()
    }

    fn create_group_set<'a>(it: impl IntoIterator<Item = &'a str>) -> ConflictSet {
        let items = it
            .into_iter()
            .map(|group| (create_package("pkg"), create_group(group)))
            .map(ConflictItem::from)
            .collect::<Vec<ConflictItem>>();
        ConflictSet::try_from(items).unwrap()
    }

    fn to_str(marker: uv_pep508::MarkerTree) -> String {
        marker.try_to_string().unwrap_or_else(|| "true".to_string())
    }

    #[test]
    fn drops_impossible_extra_clause() {
        let root = create_package("pkg");
        let conflicts = create_conflicts([create_set(["cpu", "gpu"])]);
        let marker = uv_pep508::MarkerTree::from_str(
            "(extra == 'extra-3-pkg-cpu' and extra == 'extra-3-pkg-gpu') \
             or extra == 'extra-3-pkg-cpu'",
        )
        .unwrap();

        let result = to_pep751_marker(marker, Some(&root), Some(&conflicts));
        insta::assert_snapshot!(to_str(result), @"'cpu' in extras");
    }

    #[test]
    fn preserves_clause_from_different_conflict_sets() {
        let root = create_package("pkg");
        let conflicts = create_conflicts([create_set(["cpu", "gpu"]), create_set(["dev", "test"])]);
        let marker = uv_pep508::MarkerTree::from_str(
            "extra == 'extra-3-pkg-cpu' and extra == 'extra-3-pkg-dev'",
        )
        .unwrap();

        let result = to_pep751_marker(marker, Some(&root), Some(&conflicts));
        insta::assert_snapshot!(to_str(result), @"'cpu' in extras and 'dev' in extras");
    }

    #[test]
    fn not_in_same_conflict_set_is_not_impossible() {
        let root = create_package("pkg");
        let conflicts = create_conflicts([create_set(["cpu", "gpu"])]);
        let marker = uv_pep508::MarkerTree::from_str(
            "extra != 'extra-3-pkg-cpu' and extra != 'extra-3-pkg-gpu'",
        )
        .unwrap();

        let result = to_pep751_marker(marker, Some(&root), Some(&conflicts));
        insta::assert_snapshot!(to_str(result), @"'cpu' not in extras and 'gpu' not in extras");
    }

    #[test]
    fn drops_impossible_group_clause() {
        let root = create_package("pkg");
        let conflicts = create_conflicts([create_group_set(["black22", "black23", "black24"])]);
        let marker = uv_pep508::MarkerTree::from_str(
            "(extra == 'group-3-pkg-black22' and extra == 'group-3-pkg-black23') \
             or extra == 'group-3-pkg-black22'",
        )
        .unwrap();

        let result = to_pep751_marker(marker, Some(&root), Some(&conflicts));
        insta::assert_snapshot!(to_str(result), @"'black22' in dependency_groups");
    }

    #[test]
    fn no_conflicts_preserves_all_clauses() {
        let root = create_package("pkg");
        let marker = uv_pep508::MarkerTree::from_str(
            "extra == 'extra-3-pkg-cpu' or extra == 'extra-3-pkg-gpu'",
        )
        .unwrap();

        let result = to_pep751_marker(marker, Some(&root), None);
        insta::assert_snapshot!(to_str(result), @"'cpu' in extras or 'gpu' in extras");
    }

    #[test]
    fn empty_conflicts_preserves_all_clauses() {
        let root = create_package("pkg");
        let conflicts = Conflicts::empty();
        let marker = uv_pep508::MarkerTree::from_str(
            "extra == 'extra-3-pkg-cpu' and extra == 'extra-3-pkg-gpu'",
        )
        .unwrap();

        let result = to_pep751_marker(marker, Some(&root), Some(&conflicts));
        insta::assert_snapshot!(to_str(result), @"'cpu' in extras and 'gpu' in extras");
    }
}
