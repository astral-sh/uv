// SPDX-License-Identifier: MPL-2.0

//! Handling pubgrub errors.

use thiserror::Error;

use crate::package::Package;
use crate::report::DerivationTree;
use crate::version::Version;

/// Errors that may occur while solving dependencies.
#[derive(Error, Debug)]
pub enum PubGrubError<P: Package, V: Version> {
    /// There is no solution for this set of dependencies.
    #[error("No solution")]
    NoSolution(DerivationTree<P, V>),

    /// Error arising when the implementer of
    /// [DependencyProvider](crate::solver::DependencyProvider)
    /// returned an error in the method
    /// [get_dependencies](crate::solver::DependencyProvider::get_dependencies).
    #[error("Retrieving dependencies of {package} {version} failed")]
    ErrorRetrievingDependencies {
        /// Package whose dependencies we want.
        package: P,
        /// Version of the package for which we want the dependencies.
        version: V,
        /// Error raised by the implementer of
        /// [DependencyProvider](crate::solver::DependencyProvider).
        source: Box<dyn std::error::Error>,
    },

    /// Error arising when the implementer of
    /// [DependencyProvider](crate::solver::DependencyProvider)
    /// returned a dependency on an empty range.
    /// This technically means that the package can not be selected,
    /// but is clearly some kind of mistake.
    #[error("Package {dependent} required by {package} {version} depends on the empty set")]
    DependencyOnTheEmptySet {
        /// Package whose dependencies we want.
        package: P,
        /// Version of the package for which we want the dependencies.
        version: V,
        /// The dependent package that requires us to pick from the empty set.
        dependent: P,
    },

    /// Error arising when the implementer of
    /// [DependencyProvider](crate::solver::DependencyProvider)
    /// returned a dependency on the requested package.
    /// This technically means that the package directly depends on itself,
    /// and is clearly some kind of mistake.
    #[error("{package} {version} depends on itself")]
    SelfDependency {
        /// Package whose dependencies we want.
        package: P,
        /// Version of the package for which we want the dependencies.
        version: V,
    },

    /// Error arising when the implementer of
    /// [DependencyProvider](crate::solver::DependencyProvider)
    /// returned an error in the method
    /// [choose_package_version](crate::solver::DependencyProvider::choose_package_version).
    #[error("Decision making failed")]
    ErrorChoosingPackageVersion(Box<dyn std::error::Error>),

    /// Error arising when the implementer of [DependencyProvider](crate::solver::DependencyProvider)
    /// returned an error in the method [should_cancel](crate::solver::DependencyProvider::should_cancel).
    #[error("We should cancel")]
    ErrorInShouldCancel(Box<dyn std::error::Error>),

    /// Something unexpected happened.
    #[error("{0}")]
    Failure(String),
}
