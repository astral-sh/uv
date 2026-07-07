use std::fmt::{Debug, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::{Bound, Deref, RangeBounds};

use pubgrub::{Ranges, SetRelation, VersionSet};

use uv_pep440::{Version, VersionSpecifiers, canonicalize_version_ranges};

/// A PEP 440 version range with separate representations for diagnostics and PubGrub reasoning.
///
/// `raw_versions` retains the original sentinel bounds used to format PEP 440 constraints, while
/// `canonical_versions` folds those sentinels onto their least valid successors. PubGrub uses the
/// canonical representation for equality, hashing, and set relations.
#[derive(Clone, Debug)]
pub struct Range<T> {
    raw_versions: Ranges<T>,
    canonical_versions: Option<Box<Ranges<T>>>,
}

impl<T> Range<T> {
    fn logical_versions(&self) -> &Ranges<T> {
        self.canonical_versions
            .as_deref()
            .unwrap_or(&self.raw_versions)
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
        &self.raw_versions
    }
}

impl Range<Version> {
    fn from_parts(
        raw_versions: Ranges<Version>,
        canonical_versions: Option<Ranges<Version>>,
    ) -> Self {
        let canonical_versions = canonical_versions
            .filter(|canonical| canonical != &raw_versions)
            .map(Box::new);
        Self {
            raw_versions,
            canonical_versions,
        }
    }

    fn from_canonical_versions(versions: Ranges<Version>) -> Self {
        Self {
            raw_versions: versions,
            canonical_versions: None,
        }
    }

    /// Create a range from its diagnostic representation.
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

    /// Return the original PEP 440 bounds used for candidate iteration and diagnostics.
    pub(crate) fn raw_versions(&self) -> &Ranges<Version> {
        &self.raw_versions
    }

    pub(crate) fn complement(&self) -> Self {
        Self::from_parts(
            self.raw_versions.complement(),
            self.canonical_versions.as_deref().map(Ranges::complement),
        )
    }

    pub(crate) fn intersection(&self, other: &Self) -> Self {
        self.binary_op(other, Ranges::intersection)
    }

    pub(crate) fn union(&self, other: &Self) -> Self {
        self.binary_op(other, Ranges::union)
    }

    fn binary_op(
        &self,
        other: &Self,
        operation: impl Fn(&Ranges<Version>, &Ranges<Version>) -> Ranges<Version>,
    ) -> Self {
        let raw_versions = operation(&self.raw_versions, &other.raw_versions);
        let canonical_versions = (self.canonical_versions.is_some()
            || other.canonical_versions.is_some())
        .then(|| operation(self.logical_versions(), other.logical_versions()));
        Self::from_parts(raw_versions, canonical_versions)
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
        self.raw_versions.contains(version)
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
        Display::fmt(&self.raw_versions, formatter)
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
        let raw = Ranges::from(
            VersionSpecifiers::from_str("<=1.0").expect("valid version specifiers for test"),
        );
        let range = Range::from(raw.clone());

        assert_eq!(range.raw_versions(), &raw);
        assert_eq!(range.to_string(), raw.to_string());
        assert_ne!(range.raw_versions(), range.logical_versions());
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

    fn assert_matches_recanonicalized(range: &Range<Version>) {
        let canonical_versions = canonicalize_version_ranges(&range.raw_versions)
            .unwrap_or_else(|| range.raw_versions.clone());
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
