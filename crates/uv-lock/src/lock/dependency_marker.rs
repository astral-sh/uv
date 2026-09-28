use std::fmt;

use uv_resolver_types::UniversalMarker;

/// A dependency edge marker relative to the PEP 508 reachability of its parent package.
///
/// Restoring `requires-python` does not restore the parent conditions removed when serializing
/// the edge. Standalone reachability must therefore be computed with [`Self::within`].
#[derive(Clone, Copy, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub(super) struct RelativeDependencyMarker(UniversalMarker);

impl RelativeDependencyMarker {
    pub(super) fn new(marker: UniversalMarker) -> Self {
        Self(marker)
    }

    /// Restore the parent context before using this marker as standalone reachability.
    pub(super) fn within(self, parent: UniversalMarker) -> UniversalMarker {
        let mut marker = self.0;
        marker.and(parent);
        marker
    }

    /// Inspect or evaluate the edge under the assumption that its parent has been reached.
    ///
    /// This is also used when comparing edges from the same parent, or displaying an edge's
    /// conditions. The result must not be treated as standalone package reachability.
    pub(super) fn in_parent_context(self) -> UniversalMarker {
        self.0
    }
}

impl fmt::Debug for RelativeDependencyMarker {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}
