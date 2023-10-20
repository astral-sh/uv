// SPDX-License-Identifier: MPL-2.0

//! Publicly exported type aliases.

/// Map implementation used by the library.
pub type Map<K, V> = rustc_hash::FxHashMap<K, V>;

/// Concrete dependencies picked by the library during [resolve](crate::solver::resolve)
/// from [DependencyConstraints].
pub type SelectedDependencies<P, V> = Map<P, V>;

/// Holds information about all possible versions a given package can accept.
/// There is a difference in semantics between an empty map
/// inside [DependencyConstraints] and [Dependencies::Unknown](crate::solver::Dependencies::Unknown):
/// the former means the package has no dependency and it is a known fact,
/// while the latter means they could not be fetched by the [DependencyProvider](crate::solver::DependencyProvider).
pub type DependencyConstraints<P, VS> = Map<P, VS>;
