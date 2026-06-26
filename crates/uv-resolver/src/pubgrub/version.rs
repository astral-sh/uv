use std::fmt::{Debug, Display, Formatter};

use pubgrub::{Ranges, VersionSet};

/// The pre-release ordering attached to a solver version.
///
/// The two variants form distinct dimensions in the PubGrub version universe. An ordinary
/// requirement can admit both dimensions, while a requirement that explicitly names a
/// pre-release admits only [`PrereleasePreference::Allow`]. This lets intersection and
/// backtracking carry the preference through PubGrub's normal set algebra.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) enum PrereleasePreference {
    /// Prefer stable candidates, falling back to pre-releases when necessary.
    PreferStable,
    /// Consider stable and pre-release candidates in normal version order.
    Allow,
}

/// A version in PubGrub's expanded, pre-release-aware version universe.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) struct PubGrubVersion<T> {
    preference: PrereleasePreference,
    version: T,
}

impl<T> PubGrubVersion<T> {
    pub(crate) fn new(preference: PrereleasePreference, version: T) -> Self {
        Self {
            preference,
            version,
        }
    }

    pub(crate) fn preference(&self) -> PrereleasePreference {
        self.preference
    }

    pub(crate) fn version(&self) -> &T {
        &self.version
    }

    pub(crate) fn into_version(self) -> T {
        self.version
    }
}

impl<T: Display> Display for PubGrubVersion<T> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        self.version.fmt(formatter)
    }
}

/// A version set with separate stable-first and pre-release-enabled dimensions.
///
/// Unlike an ordering flag attached to a normal range, this is a genuine set over
/// [`PubGrubVersion`] values. All [`VersionSet`] operations are component-wise, preserving the set
/// laws required by PubGrub's conflict resolution.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct Range<T> {
    prefer_stable: Ranges<T>,
    allow: Ranges<T>,
}

impl<T> Range<T> {
    pub(crate) fn singleton(version: PubGrubVersion<T>) -> Self
    where
        T: Clone,
    {
        match version.preference {
            PrereleasePreference::PreferStable => {
                Self::prefer_stable(Ranges::singleton(version.version))
            }
            PrereleasePreference::Allow => Self::allow(Ranges::singleton(version.version)),
        }
    }

    pub(crate) fn prefer_stable(versions: Ranges<T>) -> Self {
        Self {
            prefer_stable: versions,
            allow: Ranges::empty(),
        }
    }

    pub(crate) fn allow(versions: Ranges<T>) -> Self {
        Self {
            prefer_stable: Ranges::empty(),
            allow: versions,
        }
    }

    pub(crate) fn both(versions: Ranges<T>) -> Self
    where
        T: Clone,
    {
        Self {
            prefer_stable: versions.clone(),
            allow: versions,
        }
    }

    pub(crate) fn preference(&self, preference: PrereleasePreference) -> &Ranges<T> {
        match preference {
            PrereleasePreference::PreferStable => &self.prefer_stable,
            PrereleasePreference::Allow => &self.allow,
        }
    }

    /// Return `true` if the set admits the same releases in both preference dimensions.
    pub(crate) fn is_preference_agnostic(&self) -> bool
    where
        T: Eq,
    {
        self.prefer_stable == self.allow
    }

    pub(crate) fn versions(&self) -> Ranges<T>
    where
        T: Clone + Ord,
    {
        self.prefer_stable.union(&self.allow)
    }

    pub(crate) fn select(&self, version: &T) -> Option<PubGrubVersion<T>>
    where
        T: Clone + Ord,
    {
        if self.prefer_stable.contains(version) {
            Some(PubGrubVersion::new(
                PrereleasePreference::PreferStable,
                version.clone(),
            ))
        } else if self.allow.contains(version) {
            Some(PubGrubVersion::new(
                PrereleasePreference::Allow,
                version.clone(),
            ))
        } else {
            None
        }
    }

    pub(crate) fn as_singleton(&self) -> Option<PubGrubVersion<T>>
    where
        T: Clone + Ord,
    {
        match (self.prefer_stable.as_singleton(), self.allow.as_singleton()) {
            (Some(version), None) => Some(PubGrubVersion::new(
                PrereleasePreference::PreferStable,
                version.clone(),
            )),
            (None, Some(version)) => Some(PubGrubVersion::new(
                PrereleasePreference::Allow,
                version.clone(),
            )),
            _ => None,
        }
    }

    pub(crate) fn restrict(&self, versions: &Ranges<T>) -> Self
    where
        T: Clone + Ord,
    {
        Self {
            prefer_stable: self.prefer_stable.intersection(versions),
            allow: self.allow.intersection(versions),
        }
    }
}

impl<T: Debug + Display + Clone + Eq + Ord> VersionSet for Range<T> {
    type V = PubGrubVersion<T>;

    fn empty() -> Self {
        Self {
            prefer_stable: Ranges::empty(),
            allow: Ranges::empty(),
        }
    }

    fn singleton(version: Self::V) -> Self {
        Self::singleton(version)
    }

    fn complement(&self) -> Self {
        Self {
            prefer_stable: self.prefer_stable.complement(),
            allow: self.allow.complement(),
        }
    }

    fn intersection(&self, other: &Self) -> Self {
        Self {
            prefer_stable: self.prefer_stable.intersection(&other.prefer_stable),
            allow: self.allow.intersection(&other.allow),
        }
    }

    fn contains(&self, version: &Self::V) -> bool {
        self.preference(version.preference)
            .contains(&version.version)
    }

    fn full() -> Self {
        Self {
            prefer_stable: Ranges::full(),
            allow: Ranges::full(),
        }
    }

    fn union(&self, other: &Self) -> Self {
        Self {
            prefer_stable: self.prefer_stable.union(&other.prefer_stable),
            allow: self.allow.union(&other.allow),
        }
    }

    fn is_disjoint(&self, other: &Self) -> bool {
        self.prefer_stable.is_disjoint(&other.prefer_stable) && self.allow.is_disjoint(&other.allow)
    }

    fn subset_of(&self, other: &Self) -> bool {
        self.prefer_stable.subset_of(&other.prefer_stable) && self.allow.subset_of(&other.allow)
    }
}

impl<T: Debug + Display + Clone + Eq + Ord> Display for Range<T> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.versions(), formatter)
    }
}

#[cfg(test)]
mod tests {
    use super::{PrereleasePreference, PubGrubVersion, Range};
    use pubgrub::{Ranges, VersionSet};

    #[test]
    fn explicit_intersection_removes_stable_preference() {
        let ordinary: Range<i32> = Range::both(Ranges::between(1, 5));
        let explicit: Range<i32> = Range::allow(Ranges::between(2, 6));
        let intersection = ordinary.intersection(&explicit);

        assert!(
            intersection
                .preference(PrereleasePreference::PreferStable)
                .is_empty()
        );
        assert_eq!(
            intersection.preference(PrereleasePreference::Allow),
            &Ranges::between(2, 5)
        );
    }

    #[test]
    fn complement_and_intersection_obey_product_set_semantics() {
        let range: Range<i32> = Range::both(Ranges::between(1, 5));
        assert_eq!(range.intersection(&range.complement()), Range::empty());
        assert_eq!(range.union(&range.complement()), Range::full());
        assert_eq!(range.complement().complement(), range);
    }

    #[test]
    fn singleton_selects_only_its_preference_dimension() {
        let version = PubGrubVersion::new(PrereleasePreference::Allow, 3);
        let range = Range::singleton(version.clone());

        assert!(range.contains(&version));
        assert!(!range.contains(&PubGrubVersion::new(PrereleasePreference::PreferStable, 3)));
    }
}
