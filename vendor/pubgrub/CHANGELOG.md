# Changelog

All notable changes to this project will be documented in this file.

## Unreleased [(diff)][unreleased-diff]

## [0.2.1] - 2021-06-30 - [(diff with 0.2.0)][0.2.0-diff]

This release is focused on performance improvements and code readability, without any change to the public API.

The code tends to be simpler around tricky parts of the algorithm such as conflict resolution.
Some data structures have been rewritten (with no unsafe) to lower memory usage.
Depending on scenarios, version 0.2.1 is 3 to 8 times faster than 0.2.0.
As an example, solving all elm package versions existing went from 580ms to 175ms on my laptop.
While solving a specific subset of packages from crates.io went from 2.5s to 320ms on my laptop.

Below are listed all the important changes in the internal parts of the API.

#### Added

- New `SmallVec` data structure (with no unsafe) using fixed size arrays for up to 2 entries.
- New `SmallMap` data structure (with no unsafe) using fixed size arrays for up to 2 entries.
- New `Arena` data structure (with no unsafe) backed by a `Vec` and indexed with `Id<T>` where `T` is phantom data.

#### Changed

- Updated the `large_case` benchmark to run with both u16 and string package identifiers in registries.
- Use the new `Arena` for the incompatibility store, and use its `Id<T>` identifiers to reference incompatibilities instead of full owned copies in the `incompatibilities` field of the solver `State`.
- Save satisfier indices of each package involved in an incompatibility when looking for its satisfier. This speeds up the search for the previous satisfier.
- Early unit propagation loop restart at the first conflict found instead of continuing evaluation for the current package.
- Index incompatibilities by package in a hash map instead of using a vec.
- Keep track of already contradicted incompatibilities in a `Set` until the next backtrack to speed up unit propagation.
- Unify `history` and `memory` in `partial_solution` under a unique hash map indexed by packages. This should speed up access to relevan terms in conflict resolution.

## [0.2.0] - 2020-11-19 - [(diff with 0.1.0)][0.1.0-diff]

This release brings many important improvements to PubGrub.
The gist of it is:

- A bug in the algorithm's implementation was [fixed](https://github.com/pubgrub-rs/pubgrub/pull/23).
- The solver is now implemented in a `resolve` function taking as argument
  an implementer of the `DependencyProvider` trait,
  which has more control over the decision making process.
- End-to-end property testing of large synthetic registries was added.
- More than 10x performance improvement.

### Changes affecting the public API

#### Added

- Links to code items in the code documentation.
- New `"serde"` feature that allows serializing some library types, useful for making simple reproducible bug reports.
- New variants for `error::PubGrubError` which are `DependencyOnTheEmptySet`,
  `SelfDependency`, `ErrorChoosingPackageVersion` and `ErrorInShouldCancel`.
- New `type_alias::Map` defined as `rustc_hash::FxHashMap`.
- New `type_alias::SelectedDependencies<P, V>` defined as `Map<P, V>`.
- The types `Dependencies` and `DependencyConstraints` were introduced to clarify intent.
- New function `choose_package_with_fewest_versions` to help implement
  the `choose_package_version` method of a `DependencyProvider`.
- Implement `FromStr` for `SemanticVersion`.
- Add the `VersionParseError` type for parsing of semantic versions.

#### Changed

- The `Solver` trait was replaced by a `DependencyProvider` trait
  which now must implement a `choose_package_version` method
  instead of `list_available_versions`.
  So it now has the ability to choose a package in addition to a version.
  The `DependencyProvider` also has a new optional method `should_cancel`
  that may be used to stop the solver if needed.
- The `choose_package_version` and `get_dependencies` methods of the
  `DependencyProvider` trait now take an immutable reference to `self`.
  Interior mutability can be used by implementor if mutability is needed.
- The `Solver.run` method was thus replaced by a free function `solver::resolve`
  taking a dependency provider as first argument.
- The `OfflineSolver` is thus replaced by an `OfflineDependencyProvider`.
- `SemanticVersion` now takes `u32` instead of `usize` for its 3 parts.
- `NumberVersion` now uses `u32` instead of `usize`.

#### Removed

- `ErrorRetrievingVersions` variant of `error::PubGrubError`.

### Changes in the internal parts of the API

#### Added

- `benches/large_case.rs` enables benchmarking of serialized registries of packages.
- `examples/caching_dependency_provider.rs` an example dependency provider caching dependencies.
- `PackageTerm<P, V> = (P, Term<V>)` new type alias for readability.
- `Memory.term_intersection_for_package(&mut self, package: &P) -> Option<&Term<V>>`
- New types were introduces for conflict resolution in `internal::partial_solution`
  to clarify the intent and return values of some functions.
  Those types are `DatedAssignment` and `SatisfierAndPreviousHistory`.
- `PartialSolution.term_intersection_for_package` calling the same function
  from its `memory`.
- New property tests for ranges: `negate_contains_opposite`, `intesection_contains_both`
  and `union_contains_either`.
- A large synthetic test case was added in `test-examples/`.
- A new test example `double_choices` was added
  for the detection of a bug (fixed) in the implementation.
- Property testing of big synthetic datasets was added in `tests/proptest.rs`.
- Comparison of PubGrub solver and a SAT solver
  was added with `tests/sat_dependency_provider.rs`.
- Other regression and unit tests were added to `tests/tests.rs`.

#### Changed

- CI workflow was improved (`./github/workflows/`), including a check for
  [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) and
  [Clippy](https://github.com/rust-lang/rust-clippy) for source code linting.
- Using SPDX license identifiers instead of MPL-2.0 classic file headers.
- `State.incompatibilities` is now wrapped inside a `Rc`.
- `DecisionLevel(u32)` is used in place of `usize` for partial solution decision levels.
- `State.conflict_resolution` now also returns the almost satisfied package
  to avoid an unnecessary call to `self.partial_solution.relation(...)` after conflict resolution.
- `Kind::NoVersion` renamed to `Kind::NoVersions` and all other usage of `noversion`
  has been changed to `no_versions`.
- Variants of the `incompatibility::Relation` enum have changed.
- Incompatibility now uses a deterministic hasher to store packages in its hash map.
- `incompatibility.relation(...)` now takes a function as argument to avoid computations
  of unnecessary terms intersections.
- `Memory` now uses a deterministic hasher instead of the default one.
- `memory::PackageAssignments` is now an enum instead of a struct.
- Derivations in a `PackageAssignments` keep a precomputed intersection of derivation terms.
- `potential_packages` method now returns a `Range`
  instead of a `Term` for the versions constraint of each package.
- `PartialSolution.relation` now takes `&mut self` instead of `&self`
  to be able to store computation of terms intersection.
- `Term.accept_version` was renamed `Term.contains`.
- The `satisfied_by` and `contradicted_by` methods of a `Term`
  now directly takes a reference to the intersection of other terms.
  Same for `relation_with`.

#### Removed

- `term` field of an `Assignment::Derivation` variant.
- `Memory.all_terms` method was removed.
- `Memory.remove_decision` method was removed in favor of a check before using `Memory.add_decision`.
- `PartialSolution` methods `pick_package` and `pick_version` have been removed
  since control was given back to the dependency provider to choose a package version.
- `PartialSolution` methods `remove_last_decision` and `satisfies_any_of` were removed
  in favor of a preventive check before calling `add_decision`.
- `Term.is_negative`.

#### Fixed

- Prior cause computation (`incompatibility::prior_cause`) now uses the intersection of package terms
  instead of their union, which was an implementation error.

## [0.1.0] - 2020-10-01

### Added

- `README.md` as the home page of this repository.
- `LICENSE`, code is provided under the MPL 2.0 license.
- `Cargo.toml` configuration of this Rust project.
- `src/` containing all the source code for this first implementation of PubGrub in Rust.
- `tests/` containing test end-to-end examples.
- `examples/` other examples, not in the form of tests.
- `.gitignore` configured for a Rust project.
- `.github/workflows/` CI to automatically build, test and document on push and pull requests.

[0.2.1]: https://github.com/pubgrub-rs/pubgrub/releases/tag/v0.2.1
[0.2.0]: https://github.com/pubgrub-rs/pubgrub/releases/tag/v0.2.0
[0.1.0]: https://github.com/pubgrub-rs/pubgrub/releases/tag/v0.1.0

[unreleased-diff]: https://github.com/pubgrub-rs/pubgrub/compare/release...dev
[0.2.0-diff]: https://github.com/pubgrub-rs/pubgrub/compare/v0.2.0...v0.2.1
[0.1.0-diff]: https://github.com/pubgrub-rs/pubgrub/compare/v0.1.0...v0.2.0
