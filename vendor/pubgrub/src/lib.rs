// SPDX-License-Identifier: MPL-2.0

//! PubGrub version solving algorithm.
//!
//! Version solving consists in efficiently finding a set of packages and versions
//! that satisfy all the constraints of a given project dependencies.
//! In addition, when that is not possible,
//! we should try to provide a very human-readable and clear
//! explanation as to why that failed.
//!
//! # Package and Version traits
//!
//! All the code in this crate is manipulating packages and versions, and for this to work
//! we defined a [Package](package::Package) and [Version](version::Version) traits
//! that are used as bounds on most of the exposed types and functions.
//!
//! Package identifiers needs to implement our [Package](package::Package) trait,
//! which is automatic if the type already implements
//! [Clone] + [Eq] + [Hash] + [Debug] + [Display](std::fmt::Display).
//! So things like [String] will work out of the box.
//!
//! Our [Version](version::Version) trait requires
//! [Clone] + [Ord] + [Debug] + [Display](std::fmt::Display)
//! and also the definition of two methods,
//! [lowest() -> Self](version::Version::lowest) which returns the lowest version existing,
//! and [bump(&self) -> Self](version::Version::bump) which returns the next smallest version
//! strictly higher than the current one.
//! For convenience, this library already provides
//! two implementations of [Version](version::Version).
//! The first one is [NumberVersion](version::NumberVersion), basically a newtype for [u32].
//! The second one is [SemanticVersion](version::NumberVersion)
//! that implements semantic versioning rules.
//!
//! # Basic example
//!
//! Let's imagine that we are building a user interface
//! with a menu containing dropdowns with some icons,
//! icons that we are also directly using in other parts of the interface.
//! For this scenario our direct dependencies are `menu` and `icons`,
//! but the complete set of dependencies looks like follows:
//!
//! - `root` depends on `menu` and `icons`
//! - `menu` depends on `dropdown`
//! - `dropdown` depends on `icons`
//! - `icons` has no dependency
//!
//! We can model that scenario with this library as follows
//! ```
//! # use pubgrub::solver::{OfflineDependencyProvider, resolve};
//! # use pubgrub::version::NumberVersion;
//! # use pubgrub::range::Range;
//!
//! type NumVS = Range<NumberVersion>;
//!
//! let mut dependency_provider = OfflineDependencyProvider::<&str, NumVS>::new();
//!
//! dependency_provider.add_dependencies(
//!     "root", 1, [("menu", Range::full()), ("icons", Range::full())],
//! );
//! dependency_provider.add_dependencies("menu", 1, [("dropdown", Range::full())]);
//! dependency_provider.add_dependencies("dropdown", 1, [("icons", Range::full())]);
//! dependency_provider.add_dependencies("icons", 1, []);
//!
//! // Run the algorithm.
//! let solution = resolve(&dependency_provider, "root", 1).unwrap();
//! ```
//!
//! # DependencyProvider trait
//!
//! In our previous example we used the
//! [OfflineDependencyProvider](solver::OfflineDependencyProvider),
//! which is a basic implementation of the [DependencyProvider](solver::DependencyProvider) trait.
//!
//! But we might want to implement the [DependencyProvider](solver::DependencyProvider)
//! trait for our own type.
//! Let's say that we will use [String] for packages,
//! and [SemanticVersion](version::SemanticVersion) for versions.
//! This may be done quite easily by implementing the three following functions.
//! ```
//! # use pubgrub::solver::{DependencyProvider, Dependencies};
//! # use pubgrub::version::SemanticVersion;
//! # use pubgrub::range::Range;
//! # use pubgrub::type_aliases::Map;
//! # use std::error::Error;
//! # use std::borrow::Borrow;
//! #
//! # struct MyDependencyProvider;
//! #
//! type SemVS = Range<SemanticVersion>;
//!
//! impl DependencyProvider<String, SemVS> for MyDependencyProvider {
//!     fn choose_version(&self, package: &String, range: &SemVS) -> Result<Option<SemanticVersion>, Box<dyn Error + Send + Sync>> {
//!         unimplemented!()
//!     }
//!
//!     type Priority = usize;
//!     fn prioritize(&self, package: &String, range: &SemVS) -> Self::Priority {
//!         unimplemented!()
//!     }
//!
//!     fn get_dependencies(
//!         &self,
//!         package: &String,
//!         version: &SemanticVersion,
//!     ) -> Result<Dependencies<impl IntoIterator<Item = (String, SemVS)> + Clone>, Box<dyn Error + Send + Sync>>
//!     {
//!         unimplemented!();
//!         Ok(Dependencies::Known([]))
//!     }
//! }
//! ```
//!
//! The first method
//! [choose_version](crate::solver::DependencyProvider::choose_version)
//! chooses a version compatible with the provided range for a package.
//! The second method
//! [prioritize](crate::solver::DependencyProvider::prioritize)
//! in which order different packages should be chosen.
//! Usually prioritizing packages
//! with the fewest number of compatible versions speeds up resolution.
//! But in general you are free to employ whatever strategy suits you best
//! to pick a package and a version.
//!
//! The third method [get_dependencies](crate::solver::DependencyProvider::get_dependencies)
//! aims at retrieving the dependencies of a given package at a given version.
//! Returns [None] if dependencies are unknown.
//!
//! In a real scenario, these two methods may involve reading the file system
//! or doing network request, so you may want to hold a cache in your
//! [DependencyProvider](solver::DependencyProvider) implementation.
//! How exactly this could be achieved is shown in `CachingDependencyProvider`
//! (see `examples/caching_dependency_provider.rs`).
//! You could also use the [OfflineDependencyProvider](solver::OfflineDependencyProvider)
//! type defined by the crate as guidance,
//! but you are free to use whatever approach makes sense in your situation.
//!
//! # Solution and error reporting
//!
//! When everything goes well, the algorithm finds and returns the complete
//! set of direct and indirect dependencies satisfying all the constraints.
//! The packages and versions selected are returned as
//! [SelectedDepedencies<P, V>](type_aliases::SelectedDependencies).
//! But sometimes there is no solution because dependencies are incompatible.
//! In such cases, [resolve(...)](solver::resolve) returns a
//! [PubGrubError::NoSolution(derivation_tree)](error::PubGrubError::NoSolution),
//! where the provided derivation tree is a custom binary tree
//! containing the full chain of reasons why there is no solution.
//!
//! All the items in the tree are called incompatibilities
//! and may be of two types, either "external" or "derived".
//! Leaves of the tree are external incompatibilities,
//! and nodes are derived.
//! External incompatibilities have reasons that are independent
//! of the way this algorithm is implemented such as
//!  - dependencies: "package_a" at version 1 depends on "package_b" at version 4
//!  - missing dependencies: dependencies of "package_a" are unknown
//!  - absence of version: there is no version of "package_a" in the range [3.1.0  4.0.0[
//!
//! Derived incompatibilities are obtained during the algorithm execution by deduction,
//! such as if "a" depends on "b" and "b" depends on "c", "a" depends on "c".
//!
//! This crate defines a [Reporter](crate::report::Reporter) trait, with an associated
//! [Output](crate::report::Reporter::Output) type and a single method.
//! ```
//! # use pubgrub::package::Package;
//! # use pubgrub::version_set::VersionSet;
//! # use pubgrub::report::DerivationTree;
//! #
//! pub trait Reporter<P: Package, VS: VersionSet> {
//!     type Output;
//!
//!     fn report(derivation_tree: &DerivationTree<P, VS>) -> Self::Output;
//! }
//! ```
//! Implementing a [Reporter](crate::report::Reporter) may involve a lot of heuristics
//! to make the output human-readable and natural.
//! For convenience, we provide a default implementation
//! [DefaultStringReporter](crate::report::DefaultStringReporter)
//! that outputs the report as a [String].
//! You may use it as follows:
//! ```
//! # use pubgrub::solver::{resolve, OfflineDependencyProvider};
//! # use pubgrub::report::{DefaultStringReporter, Reporter};
//! # use pubgrub::error::PubGrubError;
//! # use pubgrub::version::NumberVersion;
//! # use pubgrub::range::Range;
//! #
//! # type NumVS = Range<NumberVersion>;
//! #
//! # let dependency_provider = OfflineDependencyProvider::<&str, NumVS>::new();
//! # let root_package = "root";
//! # let root_version = 1;
//! #
//! match resolve(&dependency_provider, root_package, root_version) {
//!     Ok(solution) => println!("{:?}", solution),
//!     Err(PubGrubError::NoSolution(mut derivation_tree)) => {
//!         derivation_tree.collapse_no_versions();
//!         eprintln!("{}", DefaultStringReporter::report(&derivation_tree));
//!     }
//!     Err(err) => panic!("{:?}", err),
//! };
//! ```
//! Notice that we also used
//! [collapse_no_versions()](crate::report::DerivationTree::collapse_no_versions) above.
//! This method simplifies the derivation tree to get rid of the
//! [NoVersions](crate::report::External::NoVersions)
//! external incompatibilities in the derivation tree.
//! So instead of seeing things like this in the report:
//! ```txt
//! Because there is no version of foo in 1.0.1 <= v < 2.0.0
//! and foo 1.0.0 depends on bar 2.0.0 <= v < 3.0.0,
//! foo 1.0.0 <= v < 2.0.0 depends on bar 2.0.0 <= v < 3.0.0.
//! ```
//! you may have directly:
//! ```txt
//! foo 1.0.0 <= v < 2.0.0 depends on bar 2.0.0 <= v < 3.0.0.
//! ```
//! Beware though that if you are using some kind of offline mode
//! with a cache, you may want to know that some versions
//! do not exist in your cache.

#![allow(clippy::rc_buffer)]
#![warn(missing_docs)]

pub mod error;
pub mod package;
pub mod range;
pub mod report;
pub mod solver;
pub mod term;
pub mod type_aliases;
pub mod version;
pub mod version_set;

mod internal;
