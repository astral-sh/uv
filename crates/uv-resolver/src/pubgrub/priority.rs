use pubgrub::Range;
use rustc_hash::FxHashMap;
use std::cmp::Reverse;
use std::collections::hash_map::OccupiedEntry;

use crate::fork_urls::ForkUrls;
use uv_normalize::PackageName;
use uv_pep440::Version;

use crate::pubgrub::package::PubGrubPackage;
use crate::pubgrub::PubGrubPackageInner;

/// A prioritization map to guide the PubGrub resolution process.
///
/// During resolution, PubGrub needs to decide which package to consider next. The priorities
/// encoded here are used to guide that decision.
///
/// Like `pip`, we prefer packages that are pinned to direct URLs over packages pinned to a single
/// version over packages that are constrained in some way over packages that are unconstrained.
///
/// See: <https://github.com/pypa/pip/blob/ef78c129b1a966dbbbdb8ebfffc43723e89110d1/src/pip/_internal/resolution/resolvelib/provider.py#L120>
#[derive(Clone, Debug, Default)]
pub(crate) struct PubGrubPriorities(FxHashMap<PackageName, PubGrubPriority>);

impl PubGrubPriorities {
    /// Add a [`PubGrubPackage`] to the priority map.
    pub(crate) fn insert(
        &mut self,
        package: &PubGrubPackage,
        version: &Range<Version>,
        urls: &ForkUrls,
    ) {
        let next = self.0.len();
        // The root package and Python constraints have no explicit priority, the root package is
        // always first and the Python version (range) is fixed.
        let Some(name) = package.name_no_root() else {
            return;
        };

        match self.0.entry(name.clone()) {
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                // Preserve the original index.
                let index = Self::get_index(next, &mut entry);

                // Compute the priority.
                let priority = if urls.get(name).is_some() {
                    PubGrubPriority::DirectUrl(Reverse(index))
                } else if version.as_singleton().is_some() {
                    PubGrubPriority::Singleton(Reverse(index))
                } else {
                    // Keep the conflict-causing packages to avoid loops where we seesaw between
                    // `Unspecified` and `Conflict*`.
                    if matches!(
                        entry.get(),
                        PubGrubPriority::ConflictEarly(_) | PubGrubPriority::ConflictLate(_)
                    ) {
                        return;
                    }
                    PubGrubPriority::Unspecified(Reverse(index))
                };

                // Take the maximum of the new and existing priorities.
                if priority > *entry.get() {
                    entry.insert(priority);
                }
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                // Compute the priority.
                let priority = if urls.get(name).is_some() {
                    PubGrubPriority::DirectUrl(Reverse(next))
                } else if version.as_singleton().is_some() {
                    PubGrubPriority::Singleton(Reverse(next))
                } else {
                    PubGrubPriority::Unspecified(Reverse(next))
                };

                // Insert the priority.
                entry.insert(priority);
            }
        }
    }

    fn get_index(next: usize, entry: &mut OccupiedEntry<PackageName, PubGrubPriority>) -> usize {
        match entry.get() {
            PubGrubPriority::ConflictLate(Reverse(index)) => *index,
            PubGrubPriority::Unspecified(Reverse(index)) => *index,
            PubGrubPriority::ConflictEarly(Reverse(index)) => *index,
            PubGrubPriority::Singleton(Reverse(index)) => *index,
            PubGrubPriority::DirectUrl(Reverse(index)) => *index,
            PubGrubPriority::Root => next,
        }
    }

    /// Return the [`PubGrubPriority`] of the given package, if it exists.
    pub(crate) fn get(&self, package: &PubGrubPackage) -> Option<PubGrubPriority> {
        match &**package {
            PubGrubPackageInner::Root(_) => Some(PubGrubPriority::Root),
            PubGrubPackageInner::Python(_) => Some(PubGrubPriority::Root),
            PubGrubPackageInner::Marker { name, .. } => self.0.get(name).copied(),
            PubGrubPackageInner::Extra { name, .. } => self.0.get(name).copied(),
            PubGrubPackageInner::Dev { name, .. } => self.0.get(name).copied(),
            PubGrubPackageInner::Package { name, .. } => self.0.get(name).copied(),
        }
    }

    /// Returns whether the priority was changed, i.e., it's the first time we hit this condition
    /// for the package.
    pub(crate) fn make_conflict_early(&mut self, package: &PubGrubPackage) -> bool {
        let next = self.0.len();
        let Some(name) = package.name_no_root() else {
            // Not a correctness bug
            assert!(
                !cfg!(debug_assertions),
                "URL packages must not be involved in conflict handling"
            );
            return false;
        };
        match self.0.entry(name.clone()) {
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                if matches!(entry.get(), PubGrubPriority::ConflictEarly(_)) {
                    // Already in the right category
                    return false;
                };
                let index = Self::get_index(next, &mut entry);
                entry.insert(PubGrubPriority::ConflictEarly(Reverse(index)));
                true
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(PubGrubPriority::ConflictEarly(Reverse(next)));
                true
            }
        }
    }

    pub(crate) fn make_conflict_late(&mut self, package: &PubGrubPackage) -> bool {
        let next = self.0.len();
        let Some(name) = package.name_no_root() else {
            // Not a correctness bug
            assert!(
                !cfg!(debug_assertions),
                "URL packages must not be involved in conflict handling"
            );
            return false;
        };
        match self.0.entry(name.clone()) {
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                // The ConflictEarly` match avoids infinite loops.
                if matches!(
                    entry.get(),
                    PubGrubPriority::ConflictLate(_) | PubGrubPriority::ConflictEarly(_)
                ) {
                    // Already in the right category
                    return false;
                };
                let index = Self::get_index(next, &mut entry);
                entry.insert(PubGrubPriority::ConflictLate(Reverse(index)));
                true
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(PubGrubPriority::ConflictLate(Reverse(next)));
                true
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PubGrubPriority {
    /// The package has no specific priority.
    ///
    /// As such, its priority is based on the order in which the packages were added (FIFO), such
    /// that the first package we visit is prioritized over subsequent packages.
    ///
    /// TODO(charlie): Prefer constrained over unconstrained packages, if they're at the same depth
    /// in the dependency graph.
    Unspecified(Reverse<usize>),

    /// Selected version of this package were often the culprit of rejecting another package, so
    /// it's deprioritized behind `ConflictEarly`. It's still the higher than `Unspecified` to
    /// conflict before selecting unrelated packages.
    ConflictLate(Reverse<usize>),

    /// Selected version of this package were often rejected, so it's prioritized over
    /// `ConflictLate`.
    ConflictEarly(Reverse<usize>),

    /// The version range is constrained to a single version (e.g., with the `==` operator).
    Singleton(Reverse<usize>),

    /// The package was specified via a direct URL.
    ///
    /// N.B.: URLs need to have priority over registry distributions for correctly matching registry
    /// distributions to URLs, see [`PubGrubPackage::from_package`] an
    /// [`crate::fork_urls::ForkUrls`].
    DirectUrl(Reverse<usize>),

    /// The package is the root package.
    Root,
}
