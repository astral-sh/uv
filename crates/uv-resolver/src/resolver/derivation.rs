use pubgrub::{Id, Kind, State};
use rustc_hash::FxHashMap;

use uv_distribution_types::{DerivationChain, DerivationStep};
use uv_pep440::Version;

use crate::dependency_provider::UvDependencyProvider;
use crate::pubgrub::PubGrubPackage;

/// Build a [`DerivationChain`] from the pubgrub state, which is available in `uv-resolver`, but not
/// in `uv-distribution-types`.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct DerivationChainBuilder;

impl DerivationChainBuilder {
    /// Compute a [`DerivationChain`] from the current PubGrub state.
    ///
    /// This is used to construct a derivation chain upon resolution failure.
    pub(crate) fn from_state(
        id: Id<PubGrubPackage>,
        version: &Version,
        state: &State<UvDependencyProvider>,
    ) -> Option<DerivationChain> {
        /// Find a path from the current package to the root package.
        fn find_path(
            id: Id<PubGrubPackage>,
            version: &Version,
            state: &State<UvDependencyProvider>,
            solution: &FxHashMap<Id<PubGrubPackage>, Version>,
            path: &mut Vec<DerivationStep>,
        ) -> bool {
            // Retrieve the incompatibilities for the current package.
            let Some(incompatibilities) = state.incompatibilities.get(&id) else {
                return false;
            };
            for index in incompatibilities {
                let incompat = &state.incompatibility_store[*index];

                // Find a dependency from a package to the current package.
                if let Kind::FromDependencyOf(id1, _, id2, v2) = &incompat.kind {
                    if id == *id2 && v2.contains(version) {
                        if let Some(version) = solution.get(id1) {
                            let p1 = &state.package_store[*id1];
                            let p2 = &state.package_store[*id2];

                            if p1.name_no_root() == p2.name_no_root() {
                                // Skip proxied dependencies.
                                if find_path(*id1, version, state, solution, path) {
                                    return true;
                                }
                            } else if let Some(name) = p1.name_no_root() {
                                // Add to the current path.
                                path.push(DerivationStep::new(
                                    name.clone(),
                                    p1.extra().cloned(),
                                    p1.dev().cloned(),
                                    Some(version.clone()),
                                    v2.clone(),
                                ));

                                // Recursively search the next package.
                                if find_path(*id1, version, state, solution, path) {
                                    return true;
                                }

                                // Backtrack if the path didn't lead to the root.
                                path.pop();
                            } else {
                                // If we've reached the root, return.
                                return true;
                            }
                        }
                    }
                }
            }
            false
        }

        let solution: FxHashMap<_, _> = state.partial_solution.extract_solution().collect();
        let path = {
            let mut path = vec![];
            if !find_path(id, version, state, &solution, &mut path) {
                return None;
            }
            path.reverse();
            path
        };

        Some(path.into_iter().collect())
    }
}
