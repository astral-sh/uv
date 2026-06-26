use std::borrow::Cow;
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
pub(crate) enum Range<T> {
    /// The same releases are available in both preference dimensions.
    Both(Ranges<T>),
    /// Releases are only available in the stable-preferring dimension.
    PreferStable(Ranges<T>),
    /// Releases are only available in the pre-release-enabled dimension.
    Allow(Ranges<T>),
    /// The preference dimensions contain different, non-empty release sets.
    Split {
        prefer_stable: Ranges<T>,
        allow: Ranges<T>,
    },
}

impl<T: Debug + Display + Clone + Eq + Ord> Range<T> {
    /// Construct the canonical representation of two preference dimensions.
    fn from_parts(prefer_stable: Ranges<T>, allow: Ranges<T>) -> Self {
        if prefer_stable == allow {
            Self::Both(prefer_stable)
        } else if prefer_stable.is_empty() {
            Self::Allow(allow)
        } else if allow.is_empty() {
            Self::PreferStable(prefer_stable)
        } else {
            Self::Split {
                prefer_stable,
                allow,
            }
        }
    }

    pub(crate) fn singleton(version: PubGrubVersion<T>) -> Self {
        match version.preference {
            PrereleasePreference::PreferStable => {
                Self::PreferStable(Ranges::singleton(version.version))
            }
            PrereleasePreference::Allow => Self::allow(Ranges::singleton(version.version)),
        }
    }

    pub(crate) fn prefer_stable(versions: Ranges<T>) -> Self {
        Self::from_parts(versions, Ranges::empty())
    }

    pub(crate) fn allow(versions: Ranges<T>) -> Self {
        Self::from_parts(Ranges::empty(), versions)
    }

    pub(crate) fn both(versions: Ranges<T>) -> Self {
        Self::Both(versions)
    }

    pub(crate) fn preference(&self, preference: PrereleasePreference) -> Option<&Ranges<T>> {
        match (self, preference) {
            (Self::Both(versions), _) => Some(versions),
            (Self::PreferStable(versions), PrereleasePreference::PreferStable)
            | (Self::Allow(versions), PrereleasePreference::Allow) => Some(versions),
            (Self::Split { prefer_stable, .. }, PrereleasePreference::PreferStable) => {
                Some(prefer_stable)
            }
            (Self::Split { allow, .. }, PrereleasePreference::Allow) => Some(allow),
            (Self::PreferStable(_), PrereleasePreference::Allow)
            | (Self::Allow(_), PrereleasePreference::PreferStable) => None,
        }
    }

    /// Return `true` if the set admits the same releases in both preference dimensions.
    pub(crate) fn is_preference_agnostic(&self) -> bool {
        matches!(self, Self::Both(_))
    }

    pub(crate) fn versions(&self) -> Cow<'_, Ranges<T>> {
        match self {
            Self::Both(versions) | Self::PreferStable(versions) | Self::Allow(versions) => {
                Cow::Borrowed(versions)
            }
            Self::Split {
                prefer_stable,
                allow,
            } => Cow::Owned(prefer_stable.union(allow)),
        }
    }

    pub(crate) fn select(&self, version: &T) -> Option<PubGrubVersion<T>> {
        if self
            .preference(PrereleasePreference::PreferStable)
            .is_some_and(|versions| versions.contains(version))
        {
            Some(PubGrubVersion::new(
                PrereleasePreference::PreferStable,
                version.clone(),
            ))
        } else if self
            .preference(PrereleasePreference::Allow)
            .is_some_and(|versions| versions.contains(version))
        {
            Some(PubGrubVersion::new(
                PrereleasePreference::Allow,
                version.clone(),
            ))
        } else {
            None
        }
    }

    pub(crate) fn as_singleton(&self) -> Option<PubGrubVersion<T>> {
        match self {
            Self::PreferStable(versions) => versions.as_singleton().map(|version| {
                PubGrubVersion::new(PrereleasePreference::PreferStable, version.clone())
            }),
            Self::Allow(versions) => versions
                .as_singleton()
                .map(|version| PubGrubVersion::new(PrereleasePreference::Allow, version.clone())),
            Self::Both(_) | Self::Split { .. } => None,
        }
    }

    pub(crate) fn restrict(&self, versions: &Ranges<T>) -> Self {
        match self {
            Self::Both(current) => Self::both(current.intersection(versions)),
            Self::PreferStable(current) => Self::prefer_stable(current.intersection(versions)),
            Self::Allow(current) => Self::allow(current.intersection(versions)),
            Self::Split {
                prefer_stable,
                allow,
            } => Self::from_parts(
                prefer_stable.intersection(versions),
                allow.intersection(versions),
            ),
        }
    }
}

impl<T: Debug + Display + Clone + Eq + Ord> VersionSet for Range<T> {
    type V = PubGrubVersion<T>;

    fn empty() -> Self {
        Self::both(Ranges::empty())
    }

    fn singleton(version: Self::V) -> Self {
        Self::singleton(version)
    }

    fn complement(&self) -> Self {
        match self {
            Self::Both(versions) => Self::both(versions.complement()),
            Self::PreferStable(versions) => Self::from_parts(versions.complement(), Ranges::full()),
            Self::Allow(versions) => Self::from_parts(Ranges::full(), versions.complement()),
            Self::Split {
                prefer_stable,
                allow,
            } => Self::from_parts(prefer_stable.complement(), allow.complement()),
        }
    }

    fn intersection(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Both(left), Self::Both(right)) => {
                return Self::both(left.intersection(right));
            }
            (Self::Both(left), Self::PreferStable(right))
            | (Self::PreferStable(right), Self::Both(left)) => {
                return Self::prefer_stable(left.intersection(right));
            }
            (Self::Both(left), Self::Allow(right)) | (Self::Allow(right), Self::Both(left)) => {
                return Self::allow(left.intersection(right));
            }
            (Self::PreferStable(left), Self::PreferStable(right)) => {
                return Self::prefer_stable(left.intersection(right));
            }
            (Self::Allow(left), Self::Allow(right)) => {
                return Self::allow(left.intersection(right));
            }
            (Self::PreferStable(_), Self::Allow(_)) | (Self::Allow(_), Self::PreferStable(_)) => {
                return Self::empty();
            }
            (Self::Split { .. }, _) | (_, Self::Split { .. }) => {}
        }

        let intersection =
            |preference| match (self.preference(preference), other.preference(preference)) {
                (Some(left), Some(right)) => left.intersection(right),
                _ => Ranges::empty(),
            };
        Self::from_parts(
            intersection(PrereleasePreference::PreferStable),
            intersection(PrereleasePreference::Allow),
        )
    }

    fn contains(&self, version: &Self::V) -> bool {
        match (self, version.preference) {
            (Self::Both(versions), _)
            | (Self::PreferStable(versions), PrereleasePreference::PreferStable)
            | (Self::Allow(versions), PrereleasePreference::Allow) => {
                versions.contains(&version.version)
            }
            (Self::Split { prefer_stable, .. }, PrereleasePreference::PreferStable) => {
                prefer_stable.contains(&version.version)
            }
            (Self::Split { allow, .. }, PrereleasePreference::Allow) => {
                allow.contains(&version.version)
            }
            (Self::PreferStable(_), PrereleasePreference::Allow)
            | (Self::Allow(_), PrereleasePreference::PreferStable) => false,
        }
    }

    fn full() -> Self {
        Self::both(Ranges::full())
    }

    fn union(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Both(left), Self::Both(right)) => return Self::both(left.union(right)),
            (Self::Both(left), Self::PreferStable(right))
            | (Self::PreferStable(right), Self::Both(left)) => {
                return Self::from_parts(left.union(right), left.clone());
            }
            (Self::Both(left), Self::Allow(right)) | (Self::Allow(right), Self::Both(left)) => {
                return Self::from_parts(left.clone(), left.union(right));
            }
            (Self::PreferStable(left), Self::PreferStable(right)) => {
                return Self::prefer_stable(left.union(right));
            }
            (Self::Allow(left), Self::Allow(right)) => {
                return Self::allow(left.union(right));
            }
            (Self::PreferStable(left), Self::Allow(right)) => {
                return Self::from_parts(left.clone(), right.clone());
            }
            (Self::Allow(left), Self::PreferStable(right)) => {
                return Self::from_parts(right.clone(), left.clone());
            }
            (Self::Split { .. }, _) | (_, Self::Split { .. }) => {}
        }

        let union = |preference| match (self.preference(preference), other.preference(preference)) {
            (Some(left), Some(right)) => left.union(right),
            (Some(versions), None) | (None, Some(versions)) => versions.clone(),
            (None, None) => Ranges::empty(),
        };
        Self::from_parts(
            union(PrereleasePreference::PreferStable),
            union(PrereleasePreference::Allow),
        )
    }

    fn is_disjoint(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Both(left) | Self::PreferStable(left) | Self::Allow(left),
                Self::Both(right),
            )
            | (Self::Both(left) | Self::PreferStable(left), Self::PreferStable(right))
            | (Self::Both(left) | Self::Allow(left), Self::Allow(right)) => {
                return left.is_disjoint(right);
            }
            (Self::PreferStable(_), Self::Allow(_)) | (Self::Allow(_), Self::PreferStable(_)) => {
                return true;
            }
            (Self::Split { .. }, _) | (_, Self::Split { .. }) => {}
        }

        let is_disjoint =
            |preference| match (self.preference(preference), other.preference(preference)) {
                (Some(left), Some(right)) => left.is_disjoint(right),
                _ => true,
            };
        is_disjoint(PrereleasePreference::PreferStable) && is_disjoint(PrereleasePreference::Allow)
    }

    fn subset_of(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Both(left) | Self::PreferStable(left) | Self::Allow(left),
                Self::Both(right),
            )
            | (Self::PreferStable(left), Self::PreferStable(right))
            | (Self::Allow(left), Self::Allow(right)) => return left.subset_of(right),
            (Self::PreferStable(_), Self::Allow(_)) | (Self::Allow(_), Self::PreferStable(_)) => {
                return false;
            }
            (Self::Both(_), Self::PreferStable(_) | Self::Allow(_))
            | (Self::Split { .. }, _)
            | (_, Self::Split { .. }) => {}
        }

        let subset_of =
            |preference| match (self.preference(preference), other.preference(preference)) {
                (Some(left), Some(right)) => left.subset_of(right),
                (Some(left), None) => left.is_empty(),
                (None, _) => true,
            };
        subset_of(PrereleasePreference::PreferStable) && subset_of(PrereleasePreference::Allow)
    }
}

impl<T: Debug + Display + Clone + Eq + Ord> Display for Range<T> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self.versions().as_ref(), formatter)
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
                .is_none()
        );
        assert_eq!(
            intersection.preference(PrereleasePreference::Allow),
            Some(&Ranges::between(2, 5))
        );
    }

    #[test]
    fn preference_agnostic_ranges_use_compact_representation() {
        assert!(matches!(
            Range::<i32>::both(Ranges::between(1, 5)),
            Range::Both(_)
        ));
        assert!(matches!(
            Range::<i32>::prefer_stable(Ranges::empty()),
            Range::Both(_)
        ));
    }

    #[test]
    fn complement_and_intersection_obey_product_set_semantics() {
        let range: Range<i32> = Range::both(Ranges::between(1, 5));
        assert_eq!(range.intersection(&range.complement()), Range::empty());
        assert_eq!(range.union(&range.complement()), Range::full());
        assert_eq!(range.complement().complement(), range);
    }

    #[test]
    fn compact_operations_match_product_set_semantics() {
        fn dimensions(range: &Range<i32>) -> [Ranges<i32>; 2] {
            [
                range
                    .preference(PrereleasePreference::PreferStable)
                    .cloned()
                    .unwrap_or_else(Ranges::empty),
                range
                    .preference(PrereleasePreference::Allow)
                    .cloned()
                    .unwrap_or_else(Ranges::empty),
            ]
        }

        let ranges = [
            Range::empty(),
            Range::full(),
            Range::both(Ranges::between(1, 5)),
            Range::prefer_stable(Ranges::between(2, 6)),
            Range::allow(Ranges::between(3, 7)),
            Range::from_parts(Ranges::between(1, 4), Ranges::between(4, 8)),
        ];

        for left in &ranges {
            let left_dimensions = dimensions(left);
            let complement = dimensions(&left.complement());
            assert_eq!(complement[0], left_dimensions[0].complement());
            assert_eq!(complement[1], left_dimensions[1].complement());

            for right in &ranges {
                let right_dimensions = dimensions(right);
                let intersection = dimensions(&left.intersection(right));
                assert_eq!(
                    intersection[0],
                    left_dimensions[0].intersection(&right_dimensions[0])
                );
                assert_eq!(
                    intersection[1],
                    left_dimensions[1].intersection(&right_dimensions[1])
                );

                let union = dimensions(&left.union(right));
                assert_eq!(union[0], left_dimensions[0].union(&right_dimensions[0]));
                assert_eq!(union[1], left_dimensions[1].union(&right_dimensions[1]));
                assert_eq!(
                    left.is_disjoint(right),
                    left_dimensions[0].is_disjoint(&right_dimensions[0])
                        && left_dimensions[1].is_disjoint(&right_dimensions[1])
                );
                assert_eq!(
                    left.subset_of(right),
                    left_dimensions[0].subset_of(&right_dimensions[0])
                        && left_dimensions[1].subset_of(&right_dimensions[1])
                );
            }
        }
    }

    #[test]
    fn singleton_selects_only_its_preference_dimension() {
        let version = PubGrubVersion::new(PrereleasePreference::Allow, 3);
        let range = Range::singleton(version.clone());

        assert!(range.contains(&version));
        assert!(!range.contains(&PubGrubVersion::new(PrereleasePreference::PreferStable, 3)));
    }
}
