use std::cmp::Reverse;
use std::iter;

use hashbrown::hash_map::EntryRef;
use pubgrub::{DependencyProvider, Id, Range};
use smallvec::{smallvec, SmallVec};

use uv_normalize::PackageName;
use uv_pep440::Version;

use crate::dependency_provider::UvDependencyProvider;
use crate::fork_urls::ForkUrls;
use crate::pubgrub::PubGrubPackage;
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
///
/// Our main priority is the package name, the earlier we encounter a package, the higher its
/// priority. This way, all virtual packages of the same name will be applied in a batch. To ensure
/// determinism, we also track the discovery order of virtual packages as secondary order.
///
/// Lookups are in constant time (vec indexing).
#[derive(Clone, Debug)]
pub(crate) struct PubGrubPriorities {
    // In pubgrub, packages are interned into an arena backed by a vec. We reuse those indices
    // into the arena to build a matching vec `lookup` that returns priorities as a vec lookup.
    //
    // `package_priority` has two jobs:
    // a) Priorities are per package name, so when updating the priorities for a single virtual
    // package, we need to update the priorities for all virtual packages associated with it.
    // `Id` is half a word on 64-bit platforms.
    // b) Keep the order of virtual packages deterministic, using their index in the `SmallVec`.
    package_priority: FxHashbrownMap<PackageName, SmallVec<[Id<PubGrubPackage>; 4]>>,
    lookup: Vec<(PubGrubPriority, PubGrubPriority)>,
}

impl PubGrubPriorities {
    pub(crate) fn new(
        root: Id<PubGrubPackage>,
        target_python: Id<PubGrubPackage>,
        installed_python: Id<PubGrubPackage>,
    ) -> Self {
        let mut slf = Self {
            package_priority: FxHashbrownMap::default(),
            lookup: vec![],
        };
        // Insert the virtual packages not linked to any real package, so indexing them doesn't
        // error.
        Self::insert_lookup(
            &mut slf.lookup,
            root,
            PubGrubPriority::Root,
            PubGrubPriority::Singleton(Reverse(2)),
        );
        Self::insert_lookup(
            &mut slf.lookup,
            target_python,
            PubGrubPriority::Root,
            PubGrubPriority::Singleton(Reverse(1)),
        );
        Self::insert_lookup(
            &mut slf.lookup,
            installed_python,
            PubGrubPriority::Root,
            PubGrubPriority::Singleton(Reverse(0)),
        );
        slf
    }

    /// Add a [`PubGrubPackage`] to the priority map.
    pub(crate) fn insert<'a>(
        &'a mut self,
        package_id: Id<PubGrubPackage>,
        package: &'a PubGrubPackage,
        version: &Range<Version>,
        urls: &ForkUrls,
    ) -> Option<(
        PubGrubPriority,
        impl IntoIterator<Item = Id<PubGrubPackage>>,
    )> {
        // The root package and Python constraints have no explicit priority, the root package is
        // always first and the Python version (range) is fixed.
        let name = package.name_no_root()?;

        let len = self.package_priority.len();
        match self.package_priority.entry_ref(name) {
            EntryRef::Occupied(mut entry) => {
                // Preserve the original index.
                let old_priority = self.lookup[entry.get()[0].into_raw()].0;
                let index = Self::get_index(&old_priority).unwrap_or(len);

                // Compute the priority.
                let new_priority = if urls.get(name).is_some() {
                    PubGrubPriority::DirectUrl(Reverse(index))
                } else if version.as_singleton().is_some()
                    || SentinelRange::from(version).is_sentinel()
                {
                    PubGrubPriority::Singleton(Reverse(index))
                } else {
                    // Keep the conflict-causing packages to avoid loops where we seesaw between
                    // `Unspecified` and `Conflict*`.
                    if matches!(
                        old_priority,
                        PubGrubPriority::ConflictEarly(_) | PubGrubPriority::ConflictLate(_)
                    ) {
                        // If not present, register the virtual package
                        if !entry.get().contains(&package_id) {
                            Self::insert_lookup(
                                &mut self.lookup,
                                package_id,
                                old_priority,
                                old_priority,
                            );
                            entry.get_mut().push(package_id);
                        }

                        return None;
                    }
                    PubGrubPriority::Unspecified(Reverse(index))
                };

                // If not present, register the virtual package
                if !entry.get().contains(&package_id) {
                    Self::insert_lookup(&mut self.lookup, package_id, new_priority, new_priority);
                    entry.get_mut().push(package_id);
                }

                // If necessary, update the priorities for all virtual packages of this package name
                if new_priority > old_priority {
                    for virtual_package in entry.get() {
                        self.lookup[virtual_package.into_raw()].0 = new_priority;
                    }
                    // TODO(konsti): Avoid the clone
                    return Some((new_priority, entry.get().clone()));
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

                // Insert the virtual package
                entry.insert(smallvec![package_id]);
                Self::insert_lookup(&mut self.lookup, package_id, priority, priority);
            }
        }
        None
    }

    /// The virtual package
    fn insert_lookup(
        lookup: &mut Vec<(PubGrubPriority, PubGrubPriority)>,
        package_id: Id<PubGrubPackage>,
        priority: PubGrubPriority,
        virtual_priority: PubGrubPriority,
    ) {
        if lookup.len() <= package_id.into_raw() {
            lookup.reserve(package_id.into_raw() + 1);
            lookup.extend(iter::repeat_n(
                (PubGrubPriority::Sentinel, PubGrubPriority::Sentinel),
                package_id.into_raw() - lookup.len(),
            ));
            lookup.push((priority, virtual_priority));
            debug_assert!(lookup.len() == package_id.into_raw() + 1);
        } else {
            lookup[package_id.into_raw()] = (priority, virtual_priority);
        }
    }

    fn get_index(priority: &PubGrubPriority) -> Option<usize> {
        match priority {
            PubGrubPriority::ConflictLate(Reverse(index))
            | PubGrubPriority::Unspecified(Reverse(index))
            | PubGrubPriority::ConflictEarly(Reverse(index))
            | PubGrubPriority::Singleton(Reverse(index))
            | PubGrubPriority::DirectUrl(Reverse(index)) => Some(*index),
            PubGrubPriority::Root => None,
            PubGrubPriority::Sentinel => {
                panic!("priority not set")
            }
        }
    }

    /// Return the [`PubGrubPriority`] of the given package, if it exists.
    pub(crate) fn get(
        &self,
        package_id: Id<PubGrubPackage>,
    ) -> <UvDependencyProvider as DependencyProvider>::Priority {
        self.lookup[package_id.into_raw()]
    }

    /// Mark a package as prioritized by setting it to [`PubGrubPriority::ConflictEarly`], if it
    /// doesn't have a higher priority already.
    ///
    /// Returns whether the priority was changed, i.e., it's the first time we hit this condition
    /// for the package.
    pub(crate) fn mark_conflict_early(
        &mut self,
        package_id: Id<PubGrubPackage>,
        package: &PubGrubPackage,
    ) -> bool {
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
                let priority = PubGrubPriority::ConflictEarly(Reverse(len));
                entry.insert(smallvec![package_id]);
                Self::insert_lookup(&mut self.lookup, package_id, priority, priority);
                true
            }
            EntryRef::Occupied(entry) => {
                if matches!(
                    self.lookup[entry.get()[0].into_raw()].0,
                    PubGrubPriority::ConflictEarly(_)
                ) {
                    // Already in the right category
                    return false;
                };
                let index = Self::get_index(&self.lookup[package_id.into_raw()].0).unwrap_or(len);
                let priority = PubGrubPriority::ConflictEarly(Reverse(index));
                for virtual_package in entry.get() {
                    self.lookup[virtual_package.into_raw()].0 = priority;
                }
                true
            }
        }
    }

    /// Mark a package as prioritized by setting it to [`PubGrubPriority::ConflictLate`], if it
    /// doesn't have a higher priority already.
    ///
    /// Returns whether the priority was changed, i.e., it's the first time this package was
    /// marked as conflicting above the threshold.
    pub(crate) fn mark_conflict_late(
        &mut self,
        package_id: Id<PubGrubPackage>,
        package: &PubGrubPackage,
    ) -> bool {
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
                let priority = PubGrubPriority::ConflictLate(Reverse(len));
                entry.insert(smallvec![package_id]);
                Self::insert_lookup(&mut self.lookup, package_id, priority, priority);
                true
            }
            EntryRef::Occupied(entry) => {
                // The ConflictEarly` match avoids infinite loops.
                if matches!(
                    self.lookup[entry.get()[0].into_raw()].0,
                    PubGrubPriority::ConflictLate(_) | PubGrubPriority::ConflictEarly(_)
                ) {
                    // Already in the right category
                    return false;
                };
                let index = Self::get_index(&self.lookup[package_id.into_raw()].0).unwrap_or(len);
                let priority = PubGrubPriority::ConflictLate(Reverse(index));
                for virtual_package in entry.get() {
                    self.lookup[virtual_package.into_raw()].0 = priority;
                }
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

    /// The value is not yet determined and must not be read.
    Sentinel,
}
