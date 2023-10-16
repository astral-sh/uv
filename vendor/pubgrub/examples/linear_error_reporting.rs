// SPDX-License-Identifier: MPL-2.0

use pubgrub::error::PubGrubError;
use pubgrub::range::Range;
use pubgrub::report::{DefaultStringReporter, Reporter};
use pubgrub::solver::{resolve, OfflineDependencyProvider};
use pubgrub::version::SemanticVersion;

// https://github.com/dart-lang/pub/blob/master/doc/solver.md#linear-error-reporting
fn main() {
    let mut dependency_provider = OfflineDependencyProvider::<&str, SemanticVersion>::new();
    #[rustfmt::skip]
    // root 1.0.0 depends on foo ^1.0.0 and baz ^1.0.0
        dependency_provider.add_dependencies(
        "root", (1, 0, 0),
        vec![
            ("foo", Range::between((1, 0, 0), (2, 0, 0))),
            ("baz", Range::between((1, 0, 0), (2, 0, 0))),
        ],
    );
    #[rustfmt::skip]
    // foo 1.0.0 depends on bar ^2.0.0
        dependency_provider.add_dependencies(
        "foo", (1, 0, 0),
        vec![("bar", Range::between((2, 0, 0), (3, 0, 0)))],
    );
    #[rustfmt::skip]
    // bar 2.0.0 depends on baz ^3.0.0
        dependency_provider.add_dependencies(
        "bar", (2, 0, 0),
        vec![("baz", Range::between((3, 0, 0), (4, 0, 0)))],
    );
    // baz 1.0.0 and 3.0.0 have no dependencies
    dependency_provider.add_dependencies("baz", (1, 0, 0), vec![]);
    dependency_provider.add_dependencies("baz", (3, 0, 0), vec![]);

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
