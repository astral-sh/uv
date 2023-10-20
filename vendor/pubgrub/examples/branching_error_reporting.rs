// SPDX-License-Identifier: MPL-2.0

use pubgrub::error::PubGrubError;
use pubgrub::range::Range;
use pubgrub::report::{DefaultStringReporter, Reporter};
use pubgrub::solver::{resolve, OfflineDependencyProvider};
use pubgrub::version::SemanticVersion;

type SemVS = Range<SemanticVersion>;

// https://github.com/dart-lang/pub/blob/master/doc/solver.md#branching-error-reporting
fn main() {
    let mut dependency_provider = OfflineDependencyProvider::<&str, SemVS>::new();
    #[rustfmt::skip]
    // root 1.0.0 depends on foo ^1.0.0
        dependency_provider.add_dependencies(
        "root", (1, 0, 0),
        [("foo", Range::from_range_bounds((1, 0, 0)..(2, 0, 0)))],
    );
    #[rustfmt::skip]
    // foo 1.0.0 depends on a ^1.0.0 and b ^1.0.0
        dependency_provider.add_dependencies(
        "foo", (1, 0, 0),
        [
            ("a", Range::from_range_bounds((1, 0, 0)..(2, 0, 0))),
            ("b", Range::from_range_bounds((1, 0, 0)..(2, 0, 0))),
        ],
    );
    #[rustfmt::skip]
    // foo 1.1.0 depends on x ^1.0.0 and y ^1.0.0
        dependency_provider.add_dependencies(
        "foo", (1, 1, 0),
        [
            ("x", Range::from_range_bounds((1, 0, 0)..(2, 0, 0))),
            ("y", Range::from_range_bounds((1, 0, 0)..(2, 0, 0))),
        ],
    );
    #[rustfmt::skip]
    // a 1.0.0 depends on b ^2.0.0
        dependency_provider.add_dependencies(
        "a", (1, 0, 0),
        [("b", Range::from_range_bounds((2, 0, 0)..(3, 0, 0)))],
    );
    // b 1.0.0 and 2.0.0 have no dependencies.
    dependency_provider.add_dependencies("b", (1, 0, 0), []);
    dependency_provider.add_dependencies("b", (2, 0, 0), []);
    #[rustfmt::skip]
    // x 1.0.0 depends on y ^2.0.0.
        dependency_provider.add_dependencies(
        "x", (1, 0, 0),
        [("y", Range::from_range_bounds((2, 0, 0)..(3, 0, 0)))],
    );
    // y 1.0.0 and 2.0.0 have no dependencies.
    dependency_provider.add_dependencies("y", (1, 0, 0), []);
    dependency_provider.add_dependencies("y", (2, 0, 0), []);

    // Run the algorithm.
    match resolve(&dependency_provider, "root", (1, 0, 0)) {
        Ok(sol) => println!("{:?}", sol),
        Err(PubGrubError::NoSolution(mut derivation_tree)) => {
            derivation_tree.collapse_no_versions();
            eprintln!("{}", DefaultStringReporter::report(&derivation_tree));
            std::process::exit(1);
        }
        Err(err) => panic!("{:?}", err),
    };
}
