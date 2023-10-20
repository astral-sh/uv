// SPDX-License-Identifier: MPL-2.0

use pubgrub::range::Range;
use pubgrub::solver::{resolve, OfflineDependencyProvider};
use pubgrub::version::NumberVersion;

type NumVS = Range<NumberVersion>;

// `root` depends on `menu` and `icons`
// `menu` depends on `dropdown`
// `dropdown` depends on `icons`
// `icons` has no dependency
#[rustfmt::skip]
fn main() {
    let mut dependency_provider = OfflineDependencyProvider::<&str, NumVS>::new();
    dependency_provider.add_dependencies(
        "root", 1, [("menu", Range::full()), ("icons", Range::full())],
    );
    dependency_provider.add_dependencies("menu", 1, [("dropdown", Range::full())]);
    dependency_provider.add_dependencies("dropdown", 1, [("icons", Range::full())]);
    dependency_provider.add_dependencies("icons", 1, []);

    // Run the algorithm.
    let solution = resolve(&dependency_provider, "root", 1);
    println!("Solution: {:?}", solution);
}
