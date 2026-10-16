use petgraph::graph::NodeIndex;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep508::MarkerTree;
use uv_pypi_types::{ConflictItem, ConflictSet, Conflicts};

use crate::{ConflictMarker, UniversalMarker};

use super::{ActivationGraph, ActivationNode, SelectionError, conflict_variable};

fn item(package: &str, extra: &str) -> ConflictItem {
    ConflictItem::from((
        package.parse::<PackageName>().expect("package"),
        extra.parse::<ExtraName>().expect("extra"),
    ))
}

fn conflicts(items: &[ConflictItem]) -> Conflicts {
    let mut conflicts = Conflicts::empty();
    conflicts.push(ConflictSet::try_from(items.to_vec()).expect("conflict set"));
    conflicts
}

fn request(graph: &mut ActivationGraph, items: &[ConflictItem]) -> NodeIndex {
    graph.graph.add_node(ActivationNode {
        requests: items.to_vec(),
        requires_choice: false,
    })
}

/// The request for `example[bar]` remains observable even if no valid installation edge can
/// coexist with `example[foo]`. This is the transitive conflict currently diagnosed by `imbibe`.
#[test]
fn transitive_conflict_survives_impossible_installation() {
    let foo = item("example", "foo");
    let bar = item("example", "bar");
    let mut graph = ActivationGraph::default();
    let root = request(&mut graph, std::slice::from_ref(&foo));
    let indirection = request(&mut graph, &[]);
    let bar_request = request(&mut graph, std::slice::from_ref(&bar));
    graph.graph.add_edge(root, indirection, MarkerTree::TRUE);
    graph
        .graph
        .add_edge(indirection, bar_request, MarkerTree::TRUE);

    assert_eq!(
        graph
            .select(
                [(root, MarkerTree::TRUE)],
                MarkerTree::TRUE,
                &conflicts(&[foo.clone(), bar.clone()])
            )
            .expect_err("transitive request violates the conflict"),
        SelectionError::Conflict {
            first: bar,
            second: foo,
            environment: MarkerTree::TRUE
        }
    );
}

/// Extra discovery must not activate dependencies of a package excluded by an extra found later.
#[test]
fn activation_does_not_follow_an_excluded_parent() {
    let foo = item("x", "foo");
    let bar = item("q", "bar");
    let mut graph = ActivationGraph::default();
    let root = request(&mut graph, &[]);
    let parent = request(&mut graph, &[]);
    let bar_request = request(&mut graph, std::slice::from_ref(&bar));
    let indirect = request(&mut graph, &[]);
    let foo_request = request(&mut graph, std::slice::from_ref(&foo));
    graph
        .graph
        .add_edge(root, parent, conflict_variable(&foo).negate());
    graph.graph.add_edge(parent, bar_request, MarkerTree::TRUE);
    graph.graph.add_edge(root, indirect, MarkerTree::TRUE);
    graph
        .graph
        .add_edge(indirect, foo_request, MarkerTree::TRUE);

    let selection = graph
        .select(
            [(root, MarkerTree::TRUE)],
            MarkerTree::TRUE,
            &conflicts(&[foo, bar]),
        )
        .expect("the excluded parent does not request bar");
    assert_eq!(selection.marker(parent), MarkerTree::FALSE);
    assert_eq!(selection.marker(bar_request), MarkerTree::FALSE);
    assert_eq!(selection.marker(foo_request), MarkerTree::TRUE);
}

#[test]
fn distinct_versions_keep_their_own_requests() {
    let foo = item("project", "foo");
    let bar = item("project", "bar");
    let mut graph = ActivationGraph::default();
    let dependency = request(&mut graph, std::slice::from_ref(&foo));
    graph.graph[dependency].requires_choice = true;
    let version_one = request(&mut graph, std::slice::from_ref(&bar));
    let version_two = request(&mut graph, &[]);
    graph
        .graph
        .add_edge(dependency, version_one, conflict_variable(&foo).negate());
    graph
        .graph
        .add_edge(dependency, version_two, conflict_variable(&foo));

    let selection = graph
        .select(
            [(dependency, MarkerTree::TRUE)],
            MarkerTree::TRUE,
            &conflicts(&[foo, bar]),
        )
        .expect("only the chosen version contributes requests");
    assert_eq!(selection.marker(version_one), MarkerTree::FALSE);
    assert_eq!(selection.marker(version_two), MarkerTree::TRUE);
}

/// Every alternative requests the extra, so its activation does not depend on first choosing a
/// version. Propagating positive guards one edge at a time would miss this justification.
#[test]
fn all_versions_can_jointly_justify_a_request() {
    let foo = item("project", "foo");
    let mut graph = ActivationGraph::default();
    let dependency = request(&mut graph, &[]);
    graph.graph[dependency].requires_choice = true;
    let version_one = request(&mut graph, std::slice::from_ref(&foo));
    let version_two = request(&mut graph, std::slice::from_ref(&foo));
    graph
        .graph
        .add_edge(dependency, version_one, conflict_variable(&foo).negate());
    graph
        .graph
        .add_edge(dependency, version_two, conflict_variable(&foo));

    let selection = graph
        .select(
            [(dependency, MarkerTree::TRUE)],
            MarkerTree::TRUE,
            &Conflicts::empty(),
        )
        .expect("both versions request foo");
    assert_eq!(selection.marker(version_one), MarkerTree::FALSE);
    assert_eq!(selection.marker(version_two), MarkerTree::TRUE);
}

#[test]
fn an_extra_cannot_activate_itself() {
    let foo = item("project", "foo");
    let mut graph = ActivationGraph::default();
    let root = request(&mut graph, &[]);
    let optional = request(&mut graph, std::slice::from_ref(&foo));
    graph
        .graph
        .add_edge(root, optional, conflict_variable(&foo));
    graph.graph.add_edge(optional, root, MarkerTree::TRUE);

    let selection = graph
        .select(
            [(root, MarkerTree::TRUE)],
            MarkerTree::TRUE,
            &Conflicts::empty(),
        )
        .expect("a cycle without an initial request stays inactive");
    assert_eq!(selection.marker(optional), MarkerTree::FALSE);
}

#[test]
fn disconnected_cycles_do_not_become_roots() {
    let foo = item("project", "foo");
    let bar = item("project", "bar");
    let mut graph = ActivationGraph::default();
    let root = request(&mut graph, &[]);
    let first = request(&mut graph, std::slice::from_ref(&foo));
    let second = request(&mut graph, std::slice::from_ref(&bar));
    graph.graph.add_edge(first, second, MarkerTree::TRUE);
    graph.graph.add_edge(second, first, MarkerTree::TRUE);

    let selection = graph
        .select(
            [(root, MarkerTree::TRUE)],
            MarkerTree::TRUE,
            &conflicts(&[foo, bar]),
        )
        .expect("the conflicting cycle is unreachable");
    assert_eq!(selection.marker(first), MarkerTree::FALSE);
    assert_eq!(selection.marker(second), MarkerTree::FALSE);
}

#[test]
fn missing_version_choice_cannot_drop_a_requirement() {
    let mut graph = ActivationGraph::default();
    let root = request(&mut graph, &[]);
    graph.graph[root].requires_choice = true;
    assert_eq!(
        graph
            .select(
                [(root, MarkerTree::TRUE)],
                MarkerTree::TRUE,
                &Conflicts::empty()
            )
            .expect_err("a required dependency has no target"),
        SelectionError::Unsatisfiable(MarkerTree::TRUE)
    );
}

#[test]
fn overlapping_version_choices_are_rejected() {
    let mut graph = ActivationGraph::default();
    let root = request(&mut graph, &[]);
    graph.graph[root].requires_choice = true;
    let first = request(&mut graph, &[]);
    let second = request(&mut graph, &[]);
    graph.graph.add_edge(root, first, MarkerTree::TRUE);
    graph.graph.add_edge(root, second, MarkerTree::TRUE);
    assert_eq!(
        graph
            .select(
                [(root, MarkerTree::TRUE)],
                MarkerTree::TRUE,
                &Conflicts::empty()
            )
            .expect_err("two versions cannot satisfy the same choice"),
        SelectionError::Unsatisfiable(MarkerTree::TRUE)
    );
}

#[test]
fn conflicts_retain_their_environment_conditions() {
    let one = item("proxy", "one");
    let two = item("proxy", "two");
    let python: MarkerTree = "python_full_version >= '3.12'".parse().expect("marker");
    let mut graph = ActivationGraph::default();
    let root = request(&mut graph, std::slice::from_ref(&one));
    let conditional = request(&mut graph, std::slice::from_ref(&two));
    graph.graph.add_edge(root, conditional, python);
    let conflicts = conflicts(&[one.clone(), two.clone()]);
    assert_eq!(
        graph
            .select([(root, MarkerTree::TRUE)], MarkerTree::TRUE, &conflicts)
            .expect_err("only newer Python versions activate both extras"),
        SelectionError::Conflict {
            first: one,
            second: two,
            environment: python
        }
    );
    let selection = graph
        .select([(root, MarkerTree::TRUE)], python.negate(), &conflicts)
        .expect("older Python versions have a valid selection");
    assert_eq!(selection.marker(root), python.negate());
    assert_eq!(selection.marker(conditional), MarkerTree::FALSE);
}

#[test]
fn negative_activation_cycle_is_not_silently_dropped() {
    let foo = item("project", "foo");
    let mut graph = ActivationGraph::default();
    let root = request(&mut graph, &[]);
    let conditional = request(&mut graph, std::slice::from_ref(&foo));
    graph
        .graph
        .add_edge(root, conditional, conflict_variable(&foo).negate());
    assert_eq!(
        graph
            .select(
                [(root, MarkerTree::TRUE)],
                MarkerTree::TRUE,
                &Conflicts::empty()
            )
            .expect_err("neither activation state satisfies the requests"),
        SelectionError::Unsatisfiable(MarkerTree::TRUE)
    );
}

#[test]
fn incomparable_activation_choices_are_ambiguous() {
    let foo = item("project", "foo");
    let bar = item("project", "bar");
    let mut graph = ActivationGraph::default();
    let root = request(&mut graph, &[]);
    let first = request(&mut graph, std::slice::from_ref(&foo));
    let second = request(&mut graph, std::slice::from_ref(&bar));
    graph
        .graph
        .add_edge(root, first, conflict_variable(&bar).negate());
    graph
        .graph
        .add_edge(root, second, conflict_variable(&foo).negate());
    assert_eq!(
        graph
            .select(
                [(root, MarkerTree::TRUE)],
                MarkerTree::TRUE,
                &Conflicts::empty()
            )
            .expect_err("either extra is a supported solution"),
        SelectionError::Ambiguous(MarkerTree::TRUE)
    );
}

#[test]
fn projects_and_groups_use_the_same_validation() {
    let package: PackageName = "project".parse().expect("package");
    let production = ConflictItem::from(package.clone());
    let group = ConflictItem::from((package, "dev".parse::<GroupName>().expect("group")));
    let mut graph = ActivationGraph::default();
    let root = request(&mut graph, std::slice::from_ref(&production));
    let dev = request(&mut graph, std::slice::from_ref(&group));
    graph.graph.add_edge(root, dev, MarkerTree::TRUE);
    let error = graph
        .select(
            [(root, MarkerTree::TRUE)],
            MarkerTree::TRUE,
            &conflicts(&[production.clone(), group.clone()]),
        )
        .expect_err("production requests a conflicting group");
    let mut expected = [production, group];
    expected.sort();
    let [first, second] = expected;
    assert_eq!(
        error,
        SelectionError::Conflict {
            first,
            second,
            environment: MarkerTree::TRUE
        }
    );
}

/// There are 2^128 possible activation assignments; selection operates on their shared diagram.
#[test]
fn large_conflict_set_selects_last_version() {
    let items: Vec<_> = (0..128)
        .map(|index| item("project", &format!("e{index:03}")))
        .collect();
    let mut graph = ActivationGraph::default();
    let root = request(&mut graph, &items[127..]);
    graph.graph[root].requires_choice = true;
    let mut versions = Vec::new();
    for item in &items {
        let version = request(&mut graph, &[]);
        graph.graph.add_edge(root, version, conflict_variable(item));
        versions.push(version);
    }
    let selection = graph
        .select(
            [(root, MarkerTree::TRUE)],
            MarkerTree::TRUE,
            &conflicts(&items),
        )
        .expect("one selected extra determines one version");
    for version in &versions[..127] {
        assert_eq!(selection.marker(*version), MarkerTree::FALSE);
    }
    assert_eq!(selection.marker(versions[127]), MarkerTree::TRUE);
}

/// Independent pairs have a compact diagram but would expand to 2^32 DNF clauses.
#[test]
fn independent_pairs_do_not_expand_to_dnf() {
    let mut graph = ActivationGraph::default();
    let mut requested = Vec::new();
    let mut declared = Conflicts::empty();
    let mut condition = MarkerTree::TRUE;
    for index in 0..32 {
        let first = item("project", &format!("pair-{index:02}-a"));
        let second = item("project", &format!("pair-{index:02}-b"));
        condition = condition.and(conflict_variable(&first).or(conflict_variable(&second)));
        requested.push(first.clone());
        declared.push(ConflictSet::try_from(vec![first, second]).expect("conflict pair"));
    }
    let root = request(&mut graph, &requested);
    let dependency = request(&mut graph, &[]);
    graph.graph.add_edge(root, dependency, condition);
    let selection = graph
        .select([(root, MarkerTree::TRUE)], MarkerTree::TRUE, &declared)
        .expect("one extra in every pair is requested");
    assert_eq!(selection.marker(dependency), MarkerTree::TRUE);
}

/// Compare symbolic propagation with an enumerating reference over every pair of two-variable
/// Boolean request conditions, root selection, optional cycle, and mutual-exclusion constraint.
#[test]
fn exhaustive_two_item_selections() {
    let first_item = item("project", "first");
    let second_item = item("project", "second");
    let variables = [
        conflict_variable(&first_item),
        conflict_variable(&second_item),
    ];
    let functions: Vec<_> = (0_u8..16)
        .map(|table| {
            (0..4).fold(MarkerTree::FALSE, |marker, assignment| {
                if table & (1 << assignment) == 0 {
                    return marker;
                }
                let term = variables.iter().enumerate().fold(
                    MarkerTree::TRUE,
                    |term, (index, variable)| {
                        term.and(if assignment & (1 << index) == 0 {
                            variable.negate()
                        } else {
                            *variable
                        })
                    },
                );
                marker.or(term)
            })
        })
        .collect();
    for first_table in 0_u8..16 {
        for second_table in 0_u8..16 {
            for cycle in 0_u8..4 {
                for selected in 0_u8..4 {
                    for conflicting in [false, true] {
                        let mut graph = ActivationGraph::default();
                        let root = request(&mut graph, &[]);
                        let first = request(&mut graph, std::slice::from_ref(&first_item));
                        let second = request(&mut graph, std::slice::from_ref(&second_item));
                        graph
                            .graph
                            .add_edge(root, first, functions[usize::from(first_table)]);
                        graph
                            .graph
                            .add_edge(root, second, functions[usize::from(second_table)]);
                        if cycle & 1 != 0 {
                            graph.graph.add_edge(first, second, MarkerTree::TRUE);
                        }
                        if cycle & 2 != 0 {
                            graph.graph.add_edge(second, first, MarkerTree::TRUE);
                        }
                        let mut roots = vec![(root, MarkerTree::TRUE)];
                        if selected & 1 != 0 {
                            roots.push((first, MarkerTree::TRUE));
                        }
                        if selected & 2 != 0 {
                            roots.push((second, MarkerTree::TRUE));
                        }
                        let declared = if conflicting {
                            conflicts(&[first_item.clone(), second_item.clone()])
                        } else {
                            Conflicts::empty()
                        };
                        // Compute ordinary graph reachability for each complete assignment.
                        let mut reachable = [0_u8; 4];
                        for assignment in 0_u8..4 {
                            let mut reached = selected;
                            if first_table & (1 << assignment) != 0 {
                                reached |= 1;
                            }
                            if second_table & (1 << assignment) != 0 {
                                reached |= 2;
                            }
                            loop {
                                let previous = reached;
                                if cycle & 1 != 0 && reached & 1 != 0 {
                                    reached |= 2;
                                }
                                if cycle & 2 != 0 && reached & 2 != 0 {
                                    reached |= 1;
                                }
                                if reached == previous {
                                    break;
                                }
                            }
                            reachable[usize::from(assignment)] = reached;
                        }
                        let mut solutions = Vec::new();
                        for candidate in 0_u8..4 {
                            let mut justified = 0_u8;
                            loop {
                                let previous = justified;
                                let completions: Vec<_> = (0_u8..4)
                                    .filter(|completion| {
                                        completion & justified == justified
                                            && completion & !candidate == 0
                                    })
                                    .collect();
                                if completions.is_empty() {
                                    break;
                                }
                                for bit in [1, 2] {
                                    if completions.iter().all(|completion| {
                                        reachable[usize::from(*completion)] & bit != 0
                                    }) {
                                        justified |= bit;
                                    }
                                }
                                if justified == previous {
                                    break;
                                }
                            }
                            if justified == candidate
                                && reachable[usize::from(candidate)] == candidate
                                && !(conflicting && candidate == 3)
                            {
                                solutions.push(candidate);
                            }
                        }
                        let expected = match solutions.as_slice() {
                            [] => Err(SelectionError::Unsatisfiable(MarkerTree::TRUE)),
                            [selection] => Ok(*selection),
                            [_, _, ..] => Err(SelectionError::Ambiguous(MarkerTree::TRUE)),
                        };
                        // Restriction may change invalid assignments, but must not change
                        // which selections pass validation or which nodes they reach.
                        for restricted in [false, true] {
                            if restricted {
                                let constraint = UniversalMarker::new(
                                    MarkerTree::TRUE,
                                    ConflictMarker::from_conflicts(&declared),
                                )
                                .combined();
                                for marker in graph.graph.edge_weights_mut() {
                                    *marker = marker.restrict(constraint);
                                }
                            }
                            let actual = graph
                                .select(roots.clone(), MarkerTree::TRUE, &declared)
                                .map_err(|error| match error {
                                    SelectionError::Conflict { environment, .. } => {
                                        SelectionError::Unsatisfiable(environment)
                                    }
                                    SelectionError::Unsatisfiable(_)
                                    | SelectionError::Ambiguous(_)
                                    | SelectionError::UnknownConflictItem(_) => error,
                                })
                                .map(|selection| {
                                    u8::from(selection.marker(first).is_true())
                                        | (u8::from(selection.marker(second).is_true()) << 1)
                                });
                            assert_eq!(
                                actual, expected,
                                "tables={first_table:04b},{second_table:04b} cycle={cycle:02b} roots={selected:02b} conflict={conflicting} restricted={restricted}"
                            );
                        }
                    }
                }
            }
        }
    }
}
