use std::cmp::Reverse;

use pubgrub::range::Range;
use rustc_hash::FxHashMap;

use crate::fork_urls::ForkUrls;
use pep440_rs::Version;
use uv_normalize::PackageName;

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
                let index = match entry.get() {
                    PubGrubPriority::Unspecified(Reverse(index)) => *index,
                    PubGrubPriority::Singleton(Reverse(index)) => *index,
                    PubGrubPriority::DirectUrl(Reverse(index)) => *index,
                    PubGrubPriority::Root => next,
                };

                // Compute the priority.
                let priority = if urls.get(name).is_some() {
                    PubGrubPriority::DirectUrl(Reverse(index))
                } else if version.as_singleton().is_some() {
                    PubGrubPriority::Singleton(Reverse(index))
                } else {
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
