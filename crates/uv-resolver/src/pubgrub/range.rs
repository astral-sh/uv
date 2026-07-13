use std::fmt::{Debug, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::{Bound, Deref, RangeBounds};

use pubgrub::{Ranges, SetRelation, VersionSet};

use uv_pep440::{
    LocalVersionSlice, Version, VersionSpecifiers, canonicalize_version_ranges,
    strip_local_version_sentinels,
};

/// A PEP 440 version range with encoded and canonical representations.
///
/// `encoded_versions` retains the internal sentinel bounds required for candidate membership and
/// as the source for a lossy diagnostic projection. `canonical_versions` folds those sentinels
/// onto their least valid successors. PubGrub uses the canonical representation for equality,
/// hashing, and set relations.
#[derive(Clone, Debug)]
pub struct Range<T> {
    encoded_versions: Ranges<T>,
    canonical_versions: Option<Box<Ranges<T>>>,
}

impl<T> Range<T> {
    /// Return the canonical set used for identity and set relationships.
    ///
    /// Ranges that do not need canonicalization reuse their encoded representation.
    fn logical_versions(&self) -> &Ranges<T> {
        self.canonical_versions
            .as_deref()
            .unwrap_or(&self.encoded_versions)
    }
}

impl<T: PartialEq> PartialEq for Range<T> {
    fn eq(&self, other: &Self) -> bool {
        self.logical_versions() == other.logical_versions()
    }
}

impl<T: Eq> Eq for Range<T> {}

impl<T: Hash> Hash for Range<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.logical_versions().hash(state);
    }
}

impl<T> Deref for Range<T> {
    type Target = Ranges<T>;

    fn deref(&self) -> &Self::Target {
        &self.encoded_versions
    }
}

impl Range<Version> {
    /// Construct a [`Range`], omitting a redundant canonical representation.
    fn from_parts(
        encoded_versions: Ranges<Version>,
        canonical_versions: Option<Ranges<Version>>,
    ) -> Self {
        let canonical_versions = canonical_versions
            .filter(|canonical| canonical != &encoded_versions)
            .map(Box::new);
        Self {
            encoded_versions,
            canonical_versions,
        }
    }

    /// Construct a [`Range`] whose encoded representation is already canonical.
    fn from_canonical_versions(versions: Ranges<Version>) -> Self {
        Self {
            encoded_versions: versions,
            canonical_versions: None,
        }
    }

    /// Create a range from its internal PEP 440 encoding.
    pub(crate) fn from_versions(versions: Ranges<Version>) -> Self {
        let canonical_versions = canonicalize_version_ranges(&versions);
        Self::from_parts(versions, canonical_versions)
    }

    pub(crate) fn empty() -> Self {
        Self::from_canonical_versions(Ranges::empty())
    }

    pub(crate) fn full() -> Self {
        Self::from_canonical_versions(Ranges::full())
    }

    pub(crate) fn singleton(version: Version) -> Self {
        Self::from_versions(Ranges::singleton(version))
    }

    pub(crate) fn from_range_bounds(range: impl RangeBounds<Version>) -> Self {
        Self::from_versions(Ranges::from_range_bounds(range))
    }

    pub(crate) fn strictly_lower_than(version: Version) -> Self {
        Self::from_versions(Ranges::strictly_lower_than(version))
    }

    pub(crate) fn strictly_higher_than(version: Version) -> Self {
        Self::from_versions(Ranges::strictly_higher_than(version))
    }

    /// Widen this range across gaps in the known versions while preserving canonical identity.
    pub(crate) fn widen_versions(&self, versions: &[Version]) -> Self {
        Self::from_versions(self.encoded_versions.widen_versions(versions))
    }

    /// Narrow this range onto the known versions while preserving canonical identity.
    pub(crate) fn narrow_versions(&self, versions: &[Version]) -> Self {
        Self::from_versions(self.encoded_versions.narrow_versions(versions))
    }

    /// Return the internal PEP 440 bounds used for candidate iteration.
    pub(crate) fn encoded_versions(&self) -> &Ranges<Version> {
        &self.encoded_versions
    }

    /// Return `true` if this range represents a single PEP 440 version constraint.
    ///
    /// A constraint like `==1.0` includes local versions such as `1.0+local`, so its encoded range
    /// is not a singleton even though it should be prioritized like a pinned requirement.
    #[inline]
    pub(crate) fn is_singleton_constraint(&self) -> bool {
        self.encoded_versions.as_singleton().is_some() || self.is_local_version_sentinel()
    }

    /// Return `true` if this range represents one or more exact public-version constraints.
    #[inline]
    fn is_local_version_sentinel(&self) -> bool {
        self.encoded_versions.iter().all(|(lower, upper)| {
            let (Bound::Included(lower), Bound::Excluded(upper)) = (lower, upper) else {
                return false;
            };
            if !lower.local().is_empty() {
                return false;
            }
            if upper.local() != LocalVersionSlice::Max {
                return false;
            }
            *lower == upper.clone().without_local()
        })
    }

    /// Return `true` if this range contains only local versions of the same public version.
    pub(crate) fn is_local_version_complement(&self) -> bool {
        self.encoded_versions.iter().all(|(lower, upper)| {
            let (Bound::Excluded(lower), Bound::Excluded(upper)) = (lower, upper) else {
                return false;
            };
            lower.local().is_empty()
                && upper.local() == LocalVersionSlice::Max
                && *lower == upper.clone().without_local()
        })
    }

    /// Rewrite internal local-version sentinels into bounds suitable for diagnostics.
    pub(crate) fn without_local_version_sentinels(&self) -> Self {
        Self::from_versions(strip_local_version_sentinels(&self.encoded_versions))
    }

    pub(crate) fn complement(&self) -> Self {
        Self::from_parts(
            self.encoded_versions.complement(),
            self.canonical_versions.as_deref().map(Ranges::complement),
        )
    }

    pub(crate) fn intersection(&self, other: &Self) -> Self {
        self.binary_op(other, Ranges::intersection)
    }

    pub(crate) fn union(&self, other: &Self) -> Self {
        self.binary_op(other, Ranges::union)
    }

    /// Apply a set operation to both representations while preserving their equivalence.
    ///
    /// The encoded result retains sentinel bounds for candidate membership and diagnostic
    /// projection, while the logical result keeps subsequent PubGrub operations congruent with
    /// canonical equality.
    fn binary_op(
        &self,
        other: &Self,
        operation: impl Fn(&Ranges<Version>, &Ranges<Version>) -> Ranges<Version>,
    ) -> Self {
        let encoded_versions = operation(&self.encoded_versions, &other.encoded_versions);
        let canonical_versions = (self.canonical_versions.is_some()
            || other.canonical_versions.is_some())
        .then(|| operation(self.logical_versions(), other.logical_versions()));
        Self::from_parts(encoded_versions, canonical_versions)
    }
}

impl From<Ranges<Version>> for Range<Version> {
    fn from(versions: Ranges<Version>) -> Self {
        Self::from_versions(versions)
    }
}

impl FromIterator<(Bound<Version>, Bound<Version>)> for Range<Version> {
    fn from_iter<I: IntoIterator<Item = (Bound<Version>, Bound<Version>)>>(iter: I) -> Self {
        Self::from_versions(iter.into_iter().collect())
    }
}

impl From<VersionSpecifiers> for Range<Version> {
    fn from(specifiers: VersionSpecifiers) -> Self {
        Self::from_versions(Ranges::from(specifiers))
    }
}

impl VersionSet for Range<Version> {
    type V = Version;

    fn empty() -> Self {
        Self::empty()
    }

    fn singleton(version: Self::V) -> Self {
        Self::singleton(version)
    }

    fn complement(&self) -> Self {
        Self::complement(self)
    }

    fn intersection(&self, other: &Self) -> Self {
        Self::intersection(self, other)
    }

    fn contains(&self, version: &Self::V) -> bool {
        self.encoded_versions.contains(version)
    }

    fn full() -> Self {
        Self::full()
    }

    fn union(&self, other: &Self) -> Self {
        Self::union(self, other)
    }

    fn is_disjoint(&self, other: &Self) -> bool {
        self.logical_versions()
            .is_disjoint(other.logical_versions())
    }

    fn subset_of(&self, other: &Self) -> bool {
        self.logical_versions().subset_of(other.logical_versions())
    }

    fn relation(&self, other: &Self) -> SetRelation {
        self.logical_versions().relation(other.logical_versions())
    }
}

impl<T: Debug + Display + Clone + Eq + Ord> Display for Range<T> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.encoded_versions, formatter)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::str::FromStr;

    use pubgrub::{Ranges, VersionSet};

    use super::Range;
    use uv_pep440::{Version, VersionSpecifiers, canonicalize_version_ranges};

    fn range(specifiers: &str) -> Range<Version> {
        Range::from(
            VersionSpecifiers::from_str(specifiers).expect("valid version specifiers for test"),
        )
    }

    fn version(version: &str) -> Version {
        Version::from_str(version).expect("valid version")
    }

    fn hash(range: &Range<Version>) -> u64 {
        let mut hasher = DefaultHasher::new();
        range.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn equivalent_pep440_ranges_have_canonical_identity() {
        for (left, right) in [
            (">1.0a1", ">=1.0a2.dev0"),
            ("<=1.0", "<1.0.post0.dev0"),
            ("==1.0", ">=1.0,<1.0.post0.dev0"),
        ] {
            let left = range(left);
            let right = range(right);

            assert_eq!(left, right);
            assert_eq!(hash(&left), hash(&right));
            assert!(left.subset_of(&right));
            assert!(right.subset_of(&left));
        }
    }

    #[test]
    fn canonical_identity_preserves_original_bounds() {
        let encoded = Ranges::from(
            VersionSpecifiers::from_str("<=1.0").expect("valid version specifiers for test"),
        );
        let range = Range::from(encoded.clone());

        assert_eq!(range.encoded_versions(), &encoded);
        assert_eq!(range.to_string(), encoded.to_string());
        assert_ne!(range.encoded_versions(), range.logical_versions());
    }

    #[test]
    fn diagnostic_ranges_hide_local_version_sentinels() {
        let equality = range("==1.0");
        assert_eq!(equality.encoded_versions().to_string(), ">=1.0, <1.0+");
        assert_eq!(
            equality.without_local_version_sentinels().to_string(),
            "==1.0"
        );

        let upper_bound = range("<=1.0");
        assert_eq!(upper_bound.encoded_versions().to_string(), "<=1.0+");
        assert_eq!(
            upper_bound.without_local_version_sentinels().to_string(),
            "<=1.0"
        );
    }

    #[test]
    fn public_version_equality_is_a_singleton_constraint() {
        assert!(range("==1.0").is_singleton_constraint());
        assert!(Range::singleton(version("1.0+local")).is_singleton_constraint());
        assert!(!range(">=1.0").is_singleton_constraint());
    }

    #[test]
    fn range_algebra_preserves_canonical_identity() {
        let ranges = [
            "==0.dev0",
            "!=0.dev0",
            "<=1.0",
            "<1.0.post0.dev0",
            "<1.0",
            "<1.0.dev0",
            ">1.0a1",
            ">=1.0a2.dev0",
            "==1.0",
            ">=2.0b1,<3",
            ">=3.5,<4",
        ]
        .map(range);

        for left in &ranges {
            assert_matches_recanonicalized(&left.complement());
            for right in &ranges {
                assert_matches_recanonicalized(&left.intersection(right));
                assert_matches_recanonicalized(&left.union(right));
            }
        }
    }

    #[test]
    fn known_version_projection_preserves_canonical_identity() {
        let versions = [version("0.dev0"), version("1.0"), version("2.0")];
        let encoded = range("<=1.0");
        let canonical = range("<1.0.post0.dev0");

        assert_eq!(
            encoded.widen_versions(&versions),
            canonical.widen_versions(&versions)
        );
        assert_eq!(
            encoded.narrow_versions(&versions),
            canonical.narrow_versions(&versions)
        );

        let empty = range("<0.dev0");
        assert_eq!(empty.widen_versions(&versions), Range::empty());
        assert_eq!(empty.narrow_versions(&versions), Range::empty());
    }

    fn assert_matches_recanonicalized(range: &Range<Version>) {
        let canonical_versions = canonicalize_version_ranges(&range.encoded_versions)
            .unwrap_or_else(|| range.encoded_versions.clone());
        assert_eq!(range.logical_versions(), &canonical_versions);
    }

    #[test]
    fn pep440_floor_is_not_logically_empty() {
        let floor = version("0.dev0");
        let range = range("==0.dev0");

        assert!(range.contains(&floor));
        assert!(!range.is_disjoint(&Range::singleton(floor)));
        assert_ne!(range, Range::empty());
    }
}
