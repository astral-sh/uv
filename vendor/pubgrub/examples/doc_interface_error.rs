// SPDX-License-Identifier: MPL-2.0

use pubgrub::error::PubGrubError;
use pubgrub::range::Range;
use pubgrub::report::{DefaultStringReporter, Reporter};
use pubgrub::solver::{resolve, OfflineDependencyProvider};
use pubgrub::version::SemanticVersion;

type SemVS = Range<SemanticVersion>;

// `root` depends on `menu`, `icons 1.0.0` and `intl 5.0.0`
// `menu 1.0.0` depends on `dropdown < 2.0.0`
// `menu >= 1.1.0` depends on `dropdown >= 2.0.0`
// `dropdown 1.8.0` depends on `intl 3.0.0`
// `dropdown >= 2.0.0` depends on `icons 2.0.0`
// `icons` has no dependency
// `intl` has no dependency
#[rustfmt::skip]
fn main() {
    let mut dependency_provider = OfflineDependencyProvider::<&str, SemVS>::new();
    // Direct dependencies: menu and icons.
    dependency_provider.add_dependencies("root", (1, 0, 0), [
        ("menu", Range::full()),
        ("icons", Range::singleton((1, 0, 0))),
        ("intl", Range::singleton((5, 0, 0))),
    ]);

    // Dependencies of the menu lib.
    dependency_provider.add_dependencies("menu", (1, 0, 0), [
        ("dropdown", Range::from_range_bounds(..(2, 0, 0))),
    ]);
    dependency_provider.add_dependencies("menu", (1, 1, 0), [
        ("dropdown", Range::from_range_bounds((2, 0, 0)..)),
    ]);
    dependency_provider.add_dependencies("menu", (1, 2, 0), [
        ("dropdown", Range::from_range_bounds((2, 0, 0)..)),
    ]);
    dependency_provider.add_dependencies("menu", (1, 3, 0), [
        ("dropdown", Range::from_range_bounds((2, 0, 0)..)),
    ]);
    dependency_provider.add_dependencies("menu", (1, 4, 0), [
        ("dropdown", Range::from_range_bounds((2, 0, 0)..)),
    ]);
    dependency_provider.add_dependencies("menu", (1, 5, 0), [
        ("dropdown", Range::from_range_bounds((2, 0, 0)..)),
    ]);

    // Dependencies of the dropdown lib.
    dependency_provider.add_dependencies("dropdown", (1, 8, 0), [
        ("intl", Range::singleton((3, 0, 0))),
    ]);
    dependency_provider.add_dependencies("dropdown", (2, 0, 0), [
        ("icons", Range::singleton((2, 0, 0))),
    ]);
    dependency_provider.add_dependencies("dropdown", (2, 1, 0), [
        ("icons", Range::singleton((2, 0, 0))),
    ]);
    dependency_provider.add_dependencies("dropdown", (2, 2, 0), [
        ("icons", Range::singleton((2, 0, 0))),
    ]);
    dependency_provider.add_dependencies("dropdown", (2, 3, 0), [
        ("icons", Range::singleton((2, 0, 0))),
    ]);

    // Icons have no dependencies.
    dependency_provider.add_dependencies("icons", (1, 0, 0), []);
    dependency_provider.add_dependencies("icons", (2, 0, 0), []);

    // Intl have no dependencies.
    dependency_provider.add_dependencies("intl", (3, 0, 0), []);
    dependency_provider.add_dependencies("intl", (4, 0, 0), []);
    dependency_provider.add_dependencies("intl", (5, 0, 0), []);

    // Run the algorithm.
    match resolve(&dependency_provider, "root", (1, 0, 0)) {
        Ok(sol) => println!("{:?}", sol),
        Err(PubGrubError::NoSolution(mut derivation_tree)) => {
            derivation_tree.collapse_no_versions();
            eprintln!("{}", DefaultStringReporter::report(&derivation_tree));
        }
        Err(err) => panic!("{:?}", err),
    };
}
