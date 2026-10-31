use pubgrub::{Id, State};
use rustc_hash::{FxHashMap, FxHashSet};

use uv_distribution_types::{DerivationChain, DerivationStep};
use uv_pep440::Version;

use crate::dependency_provider::UvDependencyProvider;
use crate::pubgrub::PubGrubPackage;
use crate::resolver::ForkState;

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
        version: Option<&Version>,
        state: &ForkState<'_>,
    ) -> Option<DerivationChain> {
        /// Find a path from the current package to the root package.
        fn find_path(
            id: Id<PubGrubPackage>,
            version: Option<&Version>,
            state: &State<UvDependencyProvider>,
            solution: &FxHashMap<Id<PubGrubPackage>, Option<Version>>,
            seen: &mut FxHashSet<Id<PubGrubPackage>>,
            path: &mut Vec<DerivationStep>,
        ) -> bool {
            if !seen.insert(id) {
                return false;
            }
            // Find a dependency from a package to the current package.
            for dependency in state.dependencies(id) {
                if id != dependency.dependency {
                    continue;
                }
                let Some(dependency_versions) = dependency.dependency_versions else {
                    continue;
                };
                if !dependency_versions.contains_selection(version) {
                    continue;
                }
                let Some(version) = solution.get(&dependency.dependent) else {
                    continue;
                };
                if !dependency
                    .dependent_versions
                    .contains_selection(version.as_ref())
                {
                    continue;
                }
                let parent = &state.package_store[dependency.dependent];
                let child = &state.package_store[dependency.dependency];

                let Some(name) = parent.name_no_root() else {
                    return true;
                };
                if find_path(
                    dependency.dependent,
                    version.as_ref(),
                    state,
                    solution,
                    seen,
                    path,
                ) {
                    // Proxy packages share the name of the package they represent.
                    if Some(name) != child.name_no_root() {
                        path.push(DerivationStep::new(
                            name.clone(),
                            parent.extra().cloned(),
                            parent.group().cloned(),
                            version.clone(),
                            dependency_versions.encoded_versions().clone(),
                        ));
                    }
                    return true;
                }
            }
            false
        }

        let mut path = Vec::new();
        find_path(
            id,
            version,
            &state.pubgrub,
            &state.selected_packages(),
            &mut FxHashSet::default(),
            &mut path,
        )
        .then(|| path.into_iter().collect())
    }
}
