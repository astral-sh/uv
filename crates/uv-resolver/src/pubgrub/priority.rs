use std::cmp::Reverse;

use hashbrown::hash_map::{EntryRef, OccupiedEntry};
use pubgrub::Range;
use rustc_hash::FxBuildHasher;

use uv_normalize::PackageName;
use uv_pep440::Version;

use crate::fork_urls::ForkUrls;
use crate::pubgrub::package::PubGrubPackage;
use crate::pubgrub::PubGrubPackageInner;
use crate::{FxHashbrownMap, SentinelRange};

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
pub(crate) struct PubGrubPriorities {
    package_priority: FxHashbrownMap<PackageName, PubGrubPriority>,
    virtual_package_tiebreaker: FxHashbrownMap<PubGrubPackage, PubGrubTiebreaker>,
}

impl PubGrubPriorities {
    /// Add a [`PubGrubPackage`] to the priority map.
    pub(crate) fn insert(
        &mut self,
        package: &PubGrubPackage,
        version: &Range<Version>,
        urls: &ForkUrls,
    ) {
        let len = self.virtual_package_tiebreaker.len();
        self.virtual_package_tiebreaker
            .entry_ref(package)
            .or_insert_with(|| {
                PubGrubTiebreaker::from(u32::try_from(len).expect("Less than 2**32 packages"))
            });

        // The root package and Python constraints have no explicit priority, the root package is
        // always first and the Python version (range) is fixed.
        let Some(name) = package.name_no_root() else {
            return;
        };

        let len = self.package_priority.len();
        match self.package_priority.entry_ref(name) {
            EntryRef::Occupied(mut entry) => {
                // Preserve the original index.
                let index = Self::get_index(&entry).unwrap_or(len);

                // Compute the priority.
                let priority = if urls.get(name).is_some() {
                    PubGrubPriority::DirectUrl(Reverse(index))
                } else if version.as_singleton().is_some()
                    || SentinelRange::from(version).is_sentinel()
                {
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
            EntryRef::Vacant(entry) => {
                // Compute the priority.
                let priority = if urls.get(name).is_some() {
                    PubGrubPriority::DirectUrl(Reverse(len))
                } else if version.as_singleton().is_some()
                    || SentinelRange::from(version).is_sentinel()
                {
                    PubGrubPriority::Singleton(Reverse(len))
                } else {
                    PubGrubPriority::Unspecified(Reverse(len))
                };

                // Insert the priority.
                entry.insert(priority);
            }
        }
    }

    fn get_index(
        entry: &OccupiedEntry<'_, PackageName, PubGrubPriority, FxBuildHasher>,
    ) -> Option<usize> {
        match entry.get() {
            PubGrubPriority::ConflictLate(Reverse(index))
            | PubGrubPriority::Unspecified(Reverse(index))
            | PubGrubPriority::ConflictEarly(Reverse(index))
            | PubGrubPriority::Singleton(Reverse(index))
            | PubGrubPriority::DirectUrl(Reverse(index)) => Some(*index),
            PubGrubPriority::Root => None,
        }
    }

    /// Return the [`PubGrubPriority`] of the given package, if it exists.
    pub(crate) fn get(
        &self,
        package: &PubGrubPackage,
    ) -> (Option<PubGrubPriority>, Option<PubGrubTiebreaker>) {
        let package_priority = match &**package {
            PubGrubPackageInner::Root(_) => Some(PubGrubPriority::Root),
            PubGrubPackageInner::Python(_) => Some(PubGrubPriority::Root),
            PubGrubPackageInner::Marker { name, .. } => self.package_priority.get(name).copied(),
            PubGrubPackageInner::Extra { name, .. } => self.package_priority.get(name).copied(),
            PubGrubPackageInner::Dev { name, .. } => self.package_priority.get(name).copied(),
            PubGrubPackageInner::Package { name, .. } => self.package_priority.get(name).copied(),
        };
        let package_tiebreaker = self.virtual_package_tiebreaker.get(package).copied();
        (package_priority, package_tiebreaker)
    }

    /// Mark a package as prioritized by setting it to [`PubGrubPriority::ConflictEarly`], if it
    /// doesn't have a higher priority already.
    ///
    /// Returns whether the priority was changed, i.e., it's the first time we hit this condition
    /// for the package.
    pub(crate) fn mark_conflict_early(&mut self, package: &PubGrubPackage) -> bool {
        let Some(name) = package.name_no_root() else {
            // Not a correctness bug
            if cfg!(debug_assertions) {
                panic!("URL packages must not be involved in conflict handling")
            } else {
                return false;
            }
        };

        let len = self.package_priority.len();
        match self.package_priority.entry_ref(name) {
            EntryRef::Vacant(entry) => {
                entry.insert(PubGrubPriority::ConflictEarly(Reverse(len)));
                true
            }
            EntryRef::Occupied(mut entry) => {
                if matches!(entry.get(), PubGrubPriority::ConflictEarly(_)) {
                    // Already in the right category
                    return false;
                };
                let index = Self::get_index(&entry).unwrap_or(len);
                entry.insert(PubGrubPriority::ConflictEarly(Reverse(index)));
                true
            }
        }
    }

    /// Mark a package as prioritized by setting it to [`PubGrubPriority::ConflictLate`], if it
    /// doesn't have a higher priority already.
    ///
    /// Returns whether the priority was changed, i.e., it's the first time this package was
    /// marked as conflicting above the threshold.
    pub(crate) fn mark_conflict_late(&mut self, package: &PubGrubPackage) -> bool {
        let Some(name) = package.name_no_root() else {
            // Not a correctness bug
            if cfg!(debug_assertions) {
                panic!("URL packages must not be involved in conflict handling")
            } else {
                return false;
            }
        };

        let len = self.package_priority.len();
        match self.package_priority.entry_ref(name) {
            EntryRef::Vacant(entry) => {
                entry.insert(PubGrubPriority::ConflictLate(Reverse(len)));
                true
            }
            EntryRef::Occupied(mut entry) => {
                // The ConflictEarly` match avoids infinite loops.
                if matches!(
                    entry.get(),
                    PubGrubPriority::ConflictLate(_) | PubGrubPriority::ConflictEarly(_)
                ) {
                    // Already in the right category
                    return false;
                };
                let index = Self::get_index(&entry).unwrap_or(len);
                entry.insert(PubGrubPriority::ConflictLate(Reverse(index)));
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
    /// [`ForkUrls`].
    DirectUrl(Reverse<usize>),

    /// The package is the root package.
    Root,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PubGrubTiebreaker(Reverse<u32>);

impl From<u32> for PubGrubTiebreaker {
    fn from(value: u32) -> Self {
        Self(Reverse(value))
    }
}
