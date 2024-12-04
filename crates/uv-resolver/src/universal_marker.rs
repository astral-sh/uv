use uv_normalize::ExtraName;
use uv_pep508::{MarkerEnvironment, MarkerTree};

/// A representation of a marker for use in universal resolution.
///
/// (This also degrades gracefully to a standard PEP 508 marker in the case of
/// non-universal resolution.)
///
/// This universal marker is meant to combine both a PEP 508 marker and a
/// marker for conflicting extras/groups. The latter specifically expresses
/// whether a particular edge in a dependency graph should be followed
/// depending on the activated extras and groups.
///
/// A universal marker evaluates to true only when *both* its PEP 508 marker
/// and its conflict marker evaluate to true.
#[derive(Debug, Default, Copy, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct UniversalMarker {
    pep508_marker: MarkerTree,
    conflict_marker: MarkerTree,
}

impl UniversalMarker {
    /// A constant universal marker that always evaluates to `true`.
    pub(crate) const TRUE: UniversalMarker = UniversalMarker {
        pep508_marker: MarkerTree::TRUE,
        conflict_marker: MarkerTree::TRUE,
    };

    /// A constant universal marker that always evaluates to `false`.
    pub(crate) const FALSE: UniversalMarker = UniversalMarker {
        pep508_marker: MarkerTree::FALSE,
        conflict_marker: MarkerTree::FALSE,
    };

    /// Creates a new universal marker from its constituent pieces.
    pub(crate) fn new(pep508_marker: MarkerTree, conflict_marker: MarkerTree) -> UniversalMarker {
        UniversalMarker {
            pep508_marker,
            conflict_marker,
        }
    }

    /// Combine this universal marker with the one given in a way that unions
    /// them. That is, the updated marker will evaluate to `true` if `self` or
    /// `other` evaluate to `true`.
    pub(crate) fn or(&mut self, other: UniversalMarker) {
        self.pep508_marker.or(other.pep508_marker);
        self.conflict_marker.or(other.conflict_marker);
    }

    /// Combine this universal marker with the one given in a way that
    /// intersects them. That is, the updated marker will evaluate to `true` if
    /// `self` and `other` evaluate to `true`.
    pub(crate) fn and(&mut self, other: UniversalMarker) {
        self.pep508_marker.and(other.pep508_marker);
        self.conflict_marker.and(other.conflict_marker);
    }

    /// Returns true if this universal marker will always evaluate to `true`.
    pub(crate) fn is_true(&self) -> bool {
        self.pep508_marker.is_true() && self.conflict_marker.is_true()
    }

    /// Returns true if this universal marker will always evaluate to `false`.
    pub(crate) fn is_false(&self) -> bool {
        self.pep508_marker.is_false() || self.conflict_marker.is_false()
    }

    /// Returns true if this universal marker is disjoint with the one given.
    ///
    /// Two universal markers are disjoint when it is impossible for them both
    /// to evaluate to `true` simultaneously.
    pub(crate) fn is_disjoint(self, other: &UniversalMarker) -> bool {
        self.pep508_marker.is_disjoint(other.pep508_marker)
            || self.conflict_marker.is_disjoint(other.conflict_marker)
    }

    /// Returns true if this universal marker is satisfied by the given
    /// marker environment and list of activated extras.
    ///
    /// FIXME: This also needs to accept a list of groups.
    pub(crate) fn evaluate(self, env: &MarkerEnvironment, extras: &[ExtraName]) -> bool {
        self.pep508_marker.evaluate(env, extras) && self.conflict_marker.evaluate(env, extras)
    }

    /// Returns the PEP 508 marker for this universal marker.
    ///
    /// One should be cautious using this. Generally speaking, it should only
    /// be used when one knows universal resolution isn't in effect. When
    /// universal resolution is enabled (i.e., there may be multiple forks
    /// producing different versions of the same package), then one should
    /// always use a universal marker since it accounts for all possible ways
    /// for a package to be installed.
    pub fn pep508(self) -> MarkerTree {
        self.pep508_marker
    }

    /// Returns the non-PEP 508 marker expression that represents conflicting
    /// extras/groups.
    ///
    /// Like with `UniversalMarker::pep508`, one should be cautious when using
    /// this. It is generally always wrong to consider conflicts in isolation
    /// from PEP 508 markers. But this can be useful for detecting failure
    /// cases. For example, the code for emitting a `ResolverOutput` (even a
    /// universal one) in a `requirements.txt` format checks for the existence
    /// of non-trivial conflict markers and fails if any are found. (Because
    /// conflict markers cannot be represented in the `requirements.txt`
    /// format.)
    pub fn conflict(self) -> MarkerTree {
        self.conflict_marker
    }
}

impl std::fmt::Display for UniversalMarker {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.pep508_marker.is_false() || self.conflict_marker.is_false() {
            return write!(f, "`false`");
        }
        match (
            self.pep508_marker.contents(),
            self.conflict_marker.contents(),
        ) {
            (None, None) => write!(f, "`true`"),
            (Some(pep508), None) => write!(f, "`{pep508}`"),
            (None, Some(conflict)) => write!(f, "`true` (conflict marker: `{conflict}`)"),
            (Some(pep508), Some(conflict)) => {
                write!(f, "`{pep508}` (conflict marker: `{conflict}`)")
            }
        }
    }
}
