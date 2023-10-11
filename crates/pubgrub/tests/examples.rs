// SPDX-License-Identifier: MPL-2.0

use pubgrub::range::Range;
use pubgrub::solver::{resolve, OfflineDependencyProvider};
use pubgrub::type_aliases::Map;
use pubgrub::version::{NumberVersion, SemanticVersion};

#[test]
/// https://github.com/dart-lang/pub/blob/master/doc/solver.md#no-conflicts
fn no_conflict() {
    let mut dependency_provider = OfflineDependencyProvider::<&str, SemanticVersion>::new();
    #[rustfmt::skip]
        dependency_provider.add_dependencies(
        "root", (1, 0, 0),
        vec![("foo", Range::between((1, 0, 0), (2, 0, 0)))],
    );
    #[rustfmt::skip]
        dependency_provider.add_dependencies(
        "foo", (1, 0, 0),
        vec![("bar", Range::between((1, 0, 0), (2, 0, 0)))],
    );
    dependency_provider.add_dependencies("bar", (1, 0, 0), vec![]);
    dependency_provider.add_dependencies("bar", (2, 0, 0), vec![]);

    // Run the algorithm.
    let computed_solution = resolve(&dependency_provider, "root", (1, 0, 0)).unwrap();

    // Solution.
    let mut expected_solution = Map::default();
    expected_solution.insert("root", (1, 0, 0).into());
    expected_solution.insert("foo", (1, 0, 0).into());
    expected_solution.insert("bar", (1, 0, 0).into());

    // Comparing the true solution with the one computed by the algorithm.
    assert_eq!(expected_solution, computed_solution);
}

#[test]
/// https://github.com/dart-lang/pub/blob/master/doc/solver.md#avoiding-conflict-during-decision-making
fn avoiding_conflict_during_decision_making() {
    let mut dependency_provider = OfflineDependencyProvider::<&str, SemanticVersion>::new();
    #[rustfmt::skip]
        dependency_provider.add_dependencies(
        "root", (1, 0, 0),
        vec![
            ("foo", Range::between((1, 0, 0), (2, 0, 0))),
            ("bar", Range::between((1, 0, 0), (2, 0, 0))),
        ],
    );
    #[rustfmt::skip]
        dependency_provider.add_dependencies(
        "foo", (1, 1, 0),
        vec![("bar", Range::between((2, 0, 0), (3, 0, 0)))],
    );
    dependency_provider.add_dependencies("foo", (1, 0, 0), vec![]);
    dependency_provider.add_dependencies("bar", (1, 0, 0), vec![]);
    dependency_provider.add_dependencies("bar", (1, 1, 0), vec![]);
    dependency_provider.add_dependencies("bar", (2, 0, 0), vec![]);

    // Run the algorithm.
    let computed_solution = resolve(&dependency_provider, "root", (1, 0, 0)).unwrap();

    // Solution.
    let mut expected_solution = Map::default();
    expected_solution.insert("root", (1, 0, 0).into());
    expected_solution.insert("foo", (1, 0, 0).into());
    expected_solution.insert("bar", (1, 1, 0).into());

    // Comparing the true solution with the one computed by the algorithm.
    assert_eq!(expected_solution, computed_solution);
}

#[test]
/// https://github.com/dart-lang/pub/blob/master/doc/solver.md#performing-conflict-resolution
fn conflict_resolution() {
    let mut dependency_provider = OfflineDependencyProvider::<&str, SemanticVersion>::new();
    #[rustfmt::skip]
        dependency_provider.add_dependencies(
        "root", (1, 0, 0),
        vec![("foo", Range::higher_than((1, 0, 0)))],
    );
    #[rustfmt::skip]
        dependency_provider.add_dependencies(
        "foo", (2, 0, 0),
        vec![("bar", Range::between((1, 0, 0), (2, 0, 0)))],
    );
    dependency_provider.add_dependencies("foo", (1, 0, 0), vec![]);
    #[rustfmt::skip]
        dependency_provider.add_dependencies(
        "bar", (1, 0, 0),
        vec![("foo", Range::between((1, 0, 0), (2, 0, 0)))],
    );

    // Run the algorithm.
    let computed_solution = resolve(&dependency_provider, "root", (1, 0, 0)).unwrap();

    // Solution.
    let mut expected_solution = Map::default();
    expected_solution.insert("root", (1, 0, 0).into());
    expected_solution.insert("foo", (1, 0, 0).into());

    // Comparing the true solution with the one computed by the algorithm.
    assert_eq!(expected_solution, computed_solution);
}

#[test]
/// https://github.com/dart-lang/pub/blob/master/doc/solver.md#conflict-resolution-with-a-partial-satisfier
fn conflict_with_partial_satisfier() {
    let mut dependency_provider = OfflineDependencyProvider::<&str, SemanticVersion>::new();
    #[rustfmt::skip]
    // root 1.0.0 depends on foo ^1.0.0 and target ^2.0.0
        dependency_provider.add_dependencies(
        "root", (1, 0, 0),
        vec![
            ("foo", Range::between((1, 0, 0), (2, 0, 0))),
            ("target", Range::between((2, 0, 0), (3, 0, 0))),
        ],
    );
    #[rustfmt::skip]
    // foo 1.1.0 depends on left ^1.0.0 and right ^1.0.0
        dependency_provider.add_dependencies(
        "foo", (1, 1, 0),
        vec![
            ("left", Range::between((1, 0, 0), (2, 0, 0))),
            ("right", Range::between((1, 0, 0), (2, 0, 0))),
        ],
    );
    dependency_provider.add_dependencies("foo", (1, 0, 0), vec![]);
    #[rustfmt::skip]
    // left 1.0.0 depends on shared >=1.0.0
        dependency_provider.add_dependencies(
        "left", (1, 0, 0),
        vec![("shared", Range::higher_than((1, 0, 0)))],
    );
    #[rustfmt::skip]
    // right 1.0.0 depends on shared <2.0.0
        dependency_provider.add_dependencies(
        "right", (1, 0, 0),
        vec![("shared", Range::strictly_lower_than((2, 0, 0)))],
    );
    dependency_provider.add_dependencies("shared", (2, 0, 0), vec![]);
    #[rustfmt::skip]
    // shared 1.0.0 depends on target ^1.0.0
        dependency_provider.add_dependencies(
        "shared", (1, 0, 0),
        vec![("target", Range::between((1, 0, 0), (2, 0, 0)))],
    );
    dependency_provider.add_dependencies("target", (2, 0, 0), vec![]);
    dependency_provider.add_dependencies("target", (1, 0, 0), vec![]);

    // Run the algorithm.
    let computed_solution = resolve(&dependency_provider, "root", (1, 0, 0)).unwrap();

    // Solution.
    let mut expected_solution = Map::default();
    expected_solution.insert("root", (1, 0, 0).into());
    expected_solution.insert("foo", (1, 0, 0).into());
    expected_solution.insert("target", (2, 0, 0).into());

    // Comparing the true solution with the one computed by the algorithm.
    assert_eq!(expected_solution, computed_solution);
}

#[test]
/// a0 dep on b and c
/// b0 dep on d0
/// b1 dep on d1 (not existing)
/// c0 has no dep
/// c1 dep on d2 (not existing)
/// d0 has no dep
///
/// Solution: a0, b0, c0, d0
fn double_choices() {
    let mut dependency_provider = OfflineDependencyProvider::<&str, NumberVersion>::new();
    dependency_provider.add_dependencies("a", 0, vec![("b", Range::any()), ("c", Range::any())]);
    dependency_provider.add_dependencies("b", 0, vec![("d", Range::exact(0))]);
    dependency_provider.add_dependencies("b", 1, vec![("d", Range::exact(1))]);
    dependency_provider.add_dependencies("c", 0, vec![]);
    dependency_provider.add_dependencies("c", 1, vec![("d", Range::exact(2))]);
    dependency_provider.add_dependencies("d", 0, vec![]);

    // Solution.
    let mut expected_solution = Map::default();
    expected_solution.insert("a", 0.into());
    expected_solution.insert("b", 0.into());
    expected_solution.insert("c", 0.into());
    expected_solution.insert("d", 0.into());

    // Run the algorithm.
    let computed_solution = resolve(&dependency_provider, "a", 0).unwrap();
    assert_eq!(expected_solution, computed_solution);
}
