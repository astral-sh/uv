// SPDX-License-Identifier: MPL-2.0

//! PubGrub version solving algorithm.
//!
//! It consists in efficiently finding a set of packages and versions
//! that satisfy all the constraints of a given project dependencies.
//! In addition, when that is not possible,
//! PubGrub tries to provide a very human-readable and clear
//! explanation as to why that failed.
//! Below is an example of explanation present in
//! the introductory blog post about PubGrub
//!
//! ```txt
//! Because dropdown >=2.0.0 depends on icons >=2.0.0 and
//!   root depends on icons <2.0.0, dropdown >=2.0.0 is forbidden.
//!
//! And because menu >=1.1.0 depends on dropdown >=2.0.0,
//!   menu >=1.1.0 is forbidden.
//!
//! And because menu <1.1.0 depends on dropdown >=1.0.0 <2.0.0
//!   which depends on intl <4.0.0, every version of menu
//!   requires intl <4.0.0.
//!
//! So, because root depends on both menu >=1.0.0 and intl >=5.0.0,
//!   version solving failed.
//! ```
//!
//! The algorithm is generic and works for any type of dependency system
//! as long as packages (P) and versions (V) implement
//! the [Package] and [Version](crate::version::Version) traits.
//! [Package] is strictly equivalent and automatically generated
//! for any type that implement [Clone] + [Eq] + [Hash] + [Debug] + [Display](std::fmt::Display).
//! [Version](crate::version::Version) simply states that versions are ordered,
//! that there should be
//! a minimal [lowest](crate::version::Version::lowest) version (like 0.0.0 in semantic versions),
//! and that for any version, it is possible to compute
//! what the next version closest to this one is ([bump](crate::version::Version::bump)).
//! For semantic versions, [bump](crate::version::Version::bump) corresponds to
//! an increment of the patch number.
//!
//! ## API
//!
//! ```
//! # use pubgrub::solver::{resolve, OfflineDependencyProvider};
//! # use pubgrub::version::NumberVersion;
//! # use pubgrub::error::PubGrubError;
//! # use pubgrub::range::Range;
//! #
//! # type NumVS = Range<NumberVersion>;
//! #
//! # fn try_main() -> Result<(), PubGrubError<&'static str, NumVS>> {
//! #     let dependency_provider = OfflineDependencyProvider::<&str, NumVS>::new();
//! #     let package = "root";
//! #     let version = 1;
//! let solution = resolve(&dependency_provider, package, version)?;
//! #     Ok(())
//! # }
//! # fn main() {
//! #     assert!(matches!(try_main(), Err(PubGrubError::NoSolution(_))));
//! # }
//! ```
//!
//! Where `dependency_provider` supplies the list of available packages and versions,
//! as well as the dependencies of every available package
//! by implementing the [DependencyProvider] trait.
//! The call to [resolve] for a given package at a given version
//! will compute the set of packages and versions needed
//! to satisfy the dependencies of that package and version pair.
//! If there is no solution, the reason will be provided as clear as possible.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet as Set};
use std::error::Error;

use crate::error::PubGrubError;
pub use crate::internal::core::State;
pub use crate::internal::incompatibility::{Incompatibility, Kind};
use crate::package::Package;
use crate::type_aliases::{DependencyConstraints, Map, SelectedDependencies};
use crate::version_set::VersionSet;
use log::{debug, info};

/// Main function of the library.
/// Finds a set of packages satisfying dependency bounds for a given package + version pair.
pub fn resolve<P: Package, VS: VersionSet>(
    dependency_provider: &impl DependencyProvider<P, VS>,
    package: P,
    version: impl Into<VS::V>,
) -> Result<SelectedDependencies<P, VS::V>, PubGrubError<P, VS>> {
    let mut state = State::init(package.clone(), version.into());
    let mut added_dependencies: Map<P, Set<VS::V>> = Map::default();
    let mut next = package;
    loop {
        dependency_provider
            .should_cancel()
            .map_err(|err| PubGrubError::ErrorInShouldCancel(err))?;

        info!("unit_propagation: {}", &next);
        state.unit_propagation(next)?;

        debug!(
            "Partial solution after unit propagation: {}",
            state.partial_solution
        );

        let Some(highest_priority_pkg) = state
            .partial_solution
            .pick_highest_priority_pkg(|p, r| dependency_provider.prioritize(p, r))
        else {
            return Ok(state.partial_solution.extract_solution());
        };
        next = highest_priority_pkg;

        let term_intersection = state
            .partial_solution
            .term_intersection_for_package(&next)
            .ok_or_else(|| {
                PubGrubError::Failure("a package was chosen but we don't have a term.".into())
            })?;
        let decision = dependency_provider
            .choose_version(&next, term_intersection.unwrap_positive())
            .map_err(PubGrubError::ErrorChoosingPackageVersion)?;
        info!("DP chose: {} @ {:?}", next, decision);

        // Pick the next compatible version.
        let v = match decision {
            None => {
                let inc = Incompatibility::no_versions(next.clone(), term_intersection.clone());
                state.add_incompatibility(inc);
                continue;
            }
            Some(x) => x,
        };

        if !term_intersection.contains(&v) {
            return Err(PubGrubError::ErrorChoosingPackageVersion(
                "choose_package_version picked an incompatible version".into(),
            ));
        }

        let is_new_dependency = added_dependencies
            .entry(next.clone())
            .or_default()
            .insert(v.clone());

        if is_new_dependency {
            // Retrieve that package dependencies.
            let p = &next;
            let dependencies = dependency_provider.get_dependencies(p, &v).map_err(|err| {
                PubGrubError::ErrorRetrievingDependencies {
                    package: p.clone(),
                    version: v.clone(),
                    source: err,
                }
            })?;

            let known_dependencies = match dependencies {
                Dependencies::Unknown => {
                    state.add_incompatibility(Incompatibility::unavailable_dependencies(
                        p.clone(),
                        v.clone(),
                    ));
                    continue;
                }
                Dependencies::Known(x) if x.contains_key(p) => {
                    return Err(PubGrubError::SelfDependency {
                        package: p.clone(),
                        version: v,
                    });
                }
                Dependencies::Known(x) => x,
            };

            // Add that package and version if the dependencies are not problematic.
            let dep_incompats = state.add_incompatibility_from_dependencies(
                p.clone(),
                v.clone(),
                &known_dependencies,
            );

            state.partial_solution.add_version(
                p.clone(),
                v,
                dep_incompats,
                &state.incompatibility_store,
            );
        } else {
            // `dep_incompats` are already in `incompatibilities` so we know there are not satisfied
            // terms and can add the decision directly.
            info!("add_decision (not first time): {} @ {}", &next, v);
            state.partial_solution.add_decision(next.clone(), v);
        }
    }
}

/// An enum used by [DependencyProvider] that holds information about package dependencies.
/// For each [Package] there is a set of versions allowed as a dependency.
#[derive(Clone)]
pub enum Dependencies<P: Package, VS: VersionSet> {
    /// Package dependencies are unavailable.
    Unknown,
    /// Container for all available package versions.
    Known(DependencyConstraints<P, VS>),
}

/// Trait that allows the algorithm to retrieve available packages and their dependencies.
/// An implementor needs to be supplied to the [resolve] function.
pub trait DependencyProvider<P: Package, VS: VersionSet> {
    /// [Decision making](https://github.com/dart-lang/pub/blob/master/doc/solver.md#decision-making)
    /// is the process of choosing the next package
    /// and version that will be appended to the partial solution.
    ///
    /// Every time such a decision must be made, the resolver looks at all the potential valid
    /// packages that have changed, and a asks the dependency provider how important each one is.
    /// For each one it calls `prioritize` with the name of the package and the current set of
    /// acceptable versions.
    /// The resolver will then pick the package with the highes priority from all the potential valid
    /// packages.
    ///
    /// The strategy employed to prioritize packages
    /// cannot change the existence of a solution or not,
    /// but can drastically change the performances of the solver,
    /// or the properties of the solution.
    /// The documentation of Pub (PubGrub implementation for the dart programming language)
    /// states the following:
    ///
    /// > Pub chooses the latest matching version of the package
    /// > with the fewest versions that match the outstanding constraint.
    /// > This tends to find conflicts earlier if any exist,
    /// > since these packages will run out of versions to try more quickly.
    /// > But there's likely room for improvement in these heuristics.
    ///
    /// Note: the resolver may call this even when the range has not change,
    /// if it is more efficient for the resolveres internal data structures.
    fn prioritize(&self, package: &P, range: &VS) -> Self::Priority;
    /// The type returned from `prioritize`. The resolver does not care what type this is
    /// as long as it can pick a largest one and clone it.
    ///
    /// [std::cmp::Reverse] can be useful if you want to pick the package with
    /// the fewest versions that match the outstanding constraint.
    type Priority: Ord + Clone;

    /// Once the resolver has found the highest `Priority` package from all potential valid
    /// packages, it needs to know what vertion of that package to use. The most common pattern
    /// is to select the largest vertion that the range contains.
    fn choose_version(
        &self,
        package: &P,
        range: &VS,
    ) -> Result<Option<VS::V>, Box<dyn Error + Send + Sync>>;

    /// Retrieves the package dependencies.
    /// Return [Dependencies::Unknown] if its dependencies are unknown.
    fn get_dependencies(
        &self,
        package: &P,
        version: &VS::V,
    ) -> Result<Dependencies<P, VS>, Box<dyn Error + Send + Sync>>;

    /// This is called fairly regularly during the resolution,
    /// if it returns an Err then resolution will be terminated.
    /// This is helpful if you want to add some form of early termination like a timeout,
    /// or you want to add some form of user feedback if things are taking a while.
    /// If not provided the resolver will run as long as needed.
    fn should_cancel(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }
}

/// A basic implementation of [DependencyProvider].
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(bound(
        serialize = "VS::V: serde::Serialize, VS: serde::Serialize, P: serde::Serialize",
        deserialize = "VS::V: serde::Deserialize<'de>, VS: serde::Deserialize<'de>, P: serde::Deserialize<'de>"
    ))
)]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct OfflineDependencyProvider<P: Package, VS: VersionSet> {
    dependencies: Map<P, BTreeMap<VS::V, DependencyConstraints<P, VS>>>,
}

impl<P: Package, VS: VersionSet> OfflineDependencyProvider<P, VS> {
    /// Creates an empty OfflineDependencyProvider with no dependencies.
    pub fn new() -> Self {
        Self {
            dependencies: Map::default(),
        }
    }

    /// Registers the dependencies of a package and version pair.
    /// Dependencies must be added with a single call to
    /// [add_dependencies](OfflineDependencyProvider::add_dependencies).
    /// All subsequent calls to
    /// [add_dependencies](OfflineDependencyProvider::add_dependencies) for a given
    /// package version pair will replace the dependencies by the new ones.
    ///
    /// The API does not allow to add dependencies one at a time to uphold an assumption that
    /// [OfflineDependencyProvider.get_dependencies(p, v)](OfflineDependencyProvider::get_dependencies)
    /// provides all dependencies of a given package (p) and version (v) pair.
    pub fn add_dependencies<I: IntoIterator<Item = (P, VS)>>(
        &mut self,
        package: P,
        version: impl Into<VS::V>,
        dependencies: I,
    ) {
        let package_deps = dependencies.into_iter().collect();
        let v = version.into();
        *self
            .dependencies
            .entry(package)
            .or_default()
            .entry(v)
            .or_default() = package_deps;
    }

    /// Lists packages that have been saved.
    pub fn packages(&self) -> impl Iterator<Item = &P> {
        self.dependencies.keys()
    }

    /// Lists versions of saved packages in sorted order.
    /// Returns [None] if no information is available regarding that package.
    pub fn versions(&self, package: &P) -> Option<impl Iterator<Item = &VS::V>> {
        self.dependencies.get(package).map(|k| k.keys())
    }

    /// Lists dependencies of a given package and version.
    /// Returns [None] if no information is available regarding that package and version pair.
    fn dependencies(&self, package: &P, version: &VS::V) -> Option<DependencyConstraints<P, VS>> {
        self.dependencies.get(package)?.get(version).cloned()
    }
}

/// An implementation of [DependencyProvider] that
/// contains all dependency information available in memory.
/// Currently packages are picked with the fewest versions contained in the constraints first.
/// But, that may change in new versions if better heuristics are found.
/// Versions are picked with the newest versions first.
impl<P: Package, VS: VersionSet> DependencyProvider<P, VS> for OfflineDependencyProvider<P, VS> {
    fn choose_version(
        &self,
        package: &P,
        range: &VS,
    ) -> Result<Option<VS::V>, Box<dyn Error + Send + Sync>> {
        Ok(self
            .dependencies
            .get(package)
            .and_then(|versions| versions.keys().rev().find(|v| range.contains(v)).cloned()))
    }

    type Priority = Reverse<usize>;
    fn prioritize(&self, package: &P, range: &VS) -> Self::Priority {
        Reverse(
            self.dependencies
                .get(package)
                .map(|versions| versions.keys().filter(|v| range.contains(v)).count())
                .unwrap_or(0),
        )
    }

    fn get_dependencies(
        &self,
        package: &P,
        version: &VS::V,
    ) -> Result<Dependencies<P, VS>, Box<dyn Error + Send + Sync>> {
        Ok(match self.dependencies(package, version) {
            None => Dependencies::Unknown,
            Some(dependencies) => Dependencies::Known(dependencies),
        })
    }
}
