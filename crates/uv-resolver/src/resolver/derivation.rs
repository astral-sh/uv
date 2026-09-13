use pubgrub::{Id, State};
use rustc_hash::FxHashMap;

use uv_distribution_types::{DerivationChain, DerivationStep};
use uv_pep440::Version;

use crate::dependency_provider::UvDependencyProvider;
use crate::pubgrub::PubGrubPackage;

/// Build a [`DerivationChain`] from the pubgrub state, which is available in `uv-resolver`, but not
/// in `uv-distribution-types`.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub(crate) struct DerivationChainBuilder;

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
            // Find a dependency from a package to the current package.
            for dependency in state.dependencies(id) {
                if id == dependency.dependency
                    && let Some(dependency_versions) = dependency.dependency_versions
                    && dependency_versions.contains(version)
                    && let Some(version) = solution.get(&dependency.dependent)
                {
                    let parent = &state.package_store[dependency.dependent];
                    let child = &state.package_store[dependency.dependency];

                    if parent.name_no_root() == child.name_no_root() {
                        // Skip proxied dependencies.
                        if find_path(dependency.dependent, version, state, solution, path) {
                            return true;
                        }
                    } else if let Some(name) = parent.name_no_root() {
                        // Add to the current path.
                        path.push(DerivationStep::new(
                            name.clone(),
                            parent.extra().cloned(),
                            parent.group().cloned(),
                            Some(version.clone()),
                            dependency_versions.encoded_versions().clone(),
                        ));

                        // Recursively search the next package.
                        if find_path(dependency.dependent, version, state, solution, path) {
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
