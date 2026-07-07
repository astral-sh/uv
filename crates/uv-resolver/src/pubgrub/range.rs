use std::fmt::{Debug, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::{Bound, Deref, RangeBounds};

use pubgrub::{Ranges, SetRelation, VersionSet};

use uv_pep440::{
    Operator, Version, VersionSpecifiers, canonicalize_version_ranges,
    version_singleton_needs_canonicalization,
};

/// A PubGrub version range with a bounded pre-release opt-in region.
///
/// `logical_versions` folds internal PEP 440 boundary sentinels for PubGrub set relations, while
/// `raw_versions` retains the original bounds for candidate iteration and diagnostics when they
/// differ. `prerelease_region` is candidate-selection metadata: a pre-release inside this region
/// participates in normal version ordering, while one outside it is considered only after stable
/// candidates are exhausted. The region is always clipped to the raw versions.
#[derive(Clone, Debug)]
pub struct Range<T> {
    logical_versions: Ranges<T>,
    raw_versions: Option<Box<Ranges<T>>>,
    prerelease_region: Option<Box<PrereleaseRegion<T>>>,
}

/// The raw and canonical forms of a pre-release admission region.
///
/// Keeping these behind one pointer avoids making every PubGrub term larger for metadata that is
/// absent from almost all ranges.
#[derive(Clone, Debug)]
struct PrereleaseRegion<T> {
    logical_versions: Ranges<T>,
    raw_versions: Option<Box<Ranges<T>>>,
}

impl<T> PrereleaseRegion<T> {
    fn raw_versions(&self) -> &Ranges<T> {
        self.raw_versions
            .as_deref()
            .unwrap_or(&self.logical_versions)
    }
}

impl<T> Range<T> {
    /// Return the canonical set used for identity and set relationships.
    ///
    /// Ranges that do not need canonicalization reuse their raw representation.
    pub(crate) fn logical_versions(&self) -> &Ranges<T> {
        &self.logical_versions
    }

    /// Return the original range bounds before logical canonicalization.
    pub(crate) fn raw_versions(&self) -> &Ranges<T> {
        self.raw_versions
            .as_deref()
            .unwrap_or(&self.logical_versions)
    }

    fn selection_region(&self) -> Option<&Ranges<T>> {
        self.prerelease_region
            .as_deref()
            .map(|region| &region.logical_versions)
    }
}

// PubGrub defines equality and hashing in terms of version membership. The admission region affects
// candidate ordering instead, so [`VersionSet::selection_eq`] compares it separately.
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
        self.raw_versions()
    }
}

impl Range<Version> {
    fn from_parts(versions: Ranges<Version>, prerelease_region: Option<Ranges<Version>>) -> Self {
        let prerelease_region = prerelease_region
            .map(|prereleases| prereleases.intersection(&versions))
            .filter(|prereleases| !prereleases.is_empty());
        let canonical_versions = canonicalize_version_ranges(&versions);
        let canonical_prerelease_region = prerelease_region
            .as_ref()
            .and_then(canonicalize_version_ranges);
        Self::from_canonical_parts(
            versions,
            canonical_versions,
            prerelease_region,
            canonical_prerelease_region,
        )
    }
    /// Create a range from raw bounds and their already-computed canonical representations.
    fn from_canonical_parts(
        versions: Ranges<Version>,
        canonical_versions: Option<Ranges<Version>>,
        prerelease_region: Option<Ranges<Version>>,
        canonical_prerelease_region: Option<Ranges<Version>>,
    ) -> Self {
        let (logical_versions, raw_versions) = match canonical_versions {
            Some(canonical) if canonical != versions => (canonical, Some(Box::new(versions))),
            Some(_) | None => (versions, None),
        };
        let prerelease_region = match (prerelease_region, canonical_prerelease_region) {
            (Some(_), Some(canonical)) if canonical.is_empty() => None,
            (Some(prerelease_region), Some(canonical)) if canonical != prerelease_region => {
                Some(Box::new(PrereleaseRegion {
                    logical_versions: canonical,
                    raw_versions: Some(Box::new(prerelease_region)),
                }))
            }
            (Some(prerelease_region), Some(_) | None) if !prerelease_region.is_empty() => {
                Some(Box::new(PrereleaseRegion {
                    logical_versions: prerelease_region,
                    raw_versions: None,
                }))
            }
            (Some(_) | None, Some(_) | None) => None,
        };
        Self {
            logical_versions,
            raw_versions,
            prerelease_region,
        }
    }

    /// Construct a [`Range`] whose raw representation is already canonical.
    fn from_canonical_versions(versions: Ranges<Version>) -> Self {
        Self {
            logical_versions: versions,
            raw_versions: None,
            prerelease_region: None,
        }
    }

    /// Create a range with no pre-release opt-in.
    pub(crate) fn from_versions(versions: Ranges<Version>) -> Self {
        Self::from_parts(versions, None)
    }

    pub(crate) fn empty() -> Self {
        Self::from_canonical_versions(Ranges::empty())
    }

    pub(crate) fn full() -> Self {
        Self::from_canonical_versions(Ranges::full())
    }

    pub(crate) fn singleton(version: Version) -> Self {
        let needs_canonicalization = version_singleton_needs_canonicalization(&version);
        let versions = Ranges::singleton(version);
        if needs_canonicalization {
            Self::from_versions(versions)
        } else {
            Self::from_canonical_versions(versions)
        }
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

    pub(crate) fn contains(&self, version: &Version) -> bool {
        self.logical_versions.contains(version)
    }

    /// Return the bounded region in which pre-releases are explicitly enabled.
    pub(crate) fn prerelease_region(&self) -> Option<&Ranges<Version>> {
        self.prerelease_region
            .as_deref()
            .map(PrereleaseRegion::raw_versions)
    }

    /// Replace the pre-release admission region while preserving logical version membership.
    ///
    /// The region is clipped to this range's versions.
    pub(crate) fn with_prerelease_region(&self, prerelease_region: &Ranges<Version>) -> Self {
        let prerelease_region = prerelease_region.intersection(self.raw_versions());
        let canonical_prerelease_region = canonicalize_version_ranges(&prerelease_region);
        Self::from_canonical_parts(
            self.raw_versions().clone(),
            self.raw_versions
                .as_ref()
                .map(|_| self.logical_versions.clone()),
            Some(prerelease_region),
            canonical_prerelease_region,
        )
    }

    pub(crate) fn complement(&self) -> Self {
        // A complement is an exclusion, so it does not grant pre-release admission.
        Self::from_canonical_parts(
            self.raw_versions().complement(),
            self.raw_versions
                .as_ref()
                .map(|_| self.logical_versions.complement()),
            None,
            None,
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
    /// The raw result retains sentinel bounds for iteration and diagnostics, while the logical
    /// result keeps subsequent PubGrub operations congruent with canonical equality.
    fn binary_op(
        &self,
        other: &Self,
        operation: impl Fn(&Ranges<Version>, &Ranges<Version>) -> Ranges<Version>,
    ) -> Self {
        let versions = operation(self.raw_versions(), other.raw_versions());
        // When the raw operation collapses to an operand, its already-computed logical form is
        // also the logical result. This avoids repeating the range operation for common subset
        // intersections and unions.
        let canonical_versions = if self.raw_versions.is_none() && other.raw_versions.is_none() {
            None
        } else if &versions == other.raw_versions() {
            other
                .raw_versions
                .as_ref()
                .map(|_| other.logical_versions.clone())
        } else if &versions == self.raw_versions() {
            self.raw_versions
                .as_ref()
                .map(|_| self.logical_versions.clone())
        } else {
            Some(operation(self.logical_versions(), other.logical_versions()))
        };
        let prerelease_region =
            combine_regions(self.prerelease_region(), other.prerelease_region())
                .map(|region| region.intersection(&versions));
        let canonical_prerelease_region = if prerelease_region.is_some()
            && (canonical_versions.is_some()
                || self
                    .prerelease_region
                    .as_ref()
                    .is_some_and(|region| region.raw_versions.is_some())
                || other
                    .prerelease_region
                    .as_ref()
                    .is_some_and(|region| region.raw_versions.is_some()))
        {
            combine_regions(self.selection_region(), other.selection_region())
                .map(|region| region.intersection(canonical_versions.as_ref().unwrap_or(&versions)))
        } else {
            None
        };
        Self::from_canonical_parts(
            versions,
            canonical_versions,
            prerelease_region,
            canonical_prerelease_region,
        )
    }

    fn difference(&self, other: &Self) -> Self {
        if other.raw_versions().is_empty() {
            return self.clone();
        }
        let versions = self.raw_versions().difference(other.raw_versions());
        if versions.is_empty() {
            return Self::empty();
        }
        if &versions == self.raw_versions() {
            return self.clone();
        }
        let canonical_versions = if self.raw_versions.is_none() && other.raw_versions.is_none() {
            None
        } else {
            Some(self.logical_versions().difference(other.logical_versions()))
        };
        let prerelease_region = self
            .prerelease_region()
            .map(|region| region.intersection(&versions));
        let canonical_prerelease_region = if prerelease_region.is_some()
            && (canonical_versions.is_some()
                || self
                    .prerelease_region
                    .as_ref()
                    .is_some_and(|region| region.raw_versions.is_some()))
        {
            self.selection_region()
                .map(|region| region.intersection(canonical_versions.as_ref().unwrap_or(&versions)))
        } else {
            None
        };
        Self::from_canonical_parts(
            versions,
            canonical_versions,
            prerelease_region,
            canonical_prerelease_region,
        )
    }
}

fn combine_regions<T: Clone + Ord>(
    left: Option<&Ranges<T>>,
    right: Option<&Ranges<T>>,
) -> Option<Ranges<T>> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.union(right)),
        (Some(region), None) | (None, Some(region)) => Some(region.clone()),
        (None, None) => None,
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
        let prerelease_region = specifiers
            .iter()
            .filter(|specifier| {
                !matches!(
                    specifier.operator(),
                    Operator::NotEqual | Operator::NotEqualStar
                ) && specifier.any_prerelease()
            })
            .map(|specifier| Ranges::from(specifier.clone()))
            .reduce(|left, right| left.union(&right));
        let versions = Ranges::from(specifiers);
        Self::from_parts(versions, prerelease_region)
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

    fn difference(&self, other: &Self) -> Self {
        Self::difference(self, other)
    }

    fn contains(&self, version: &Self::V) -> bool {
        Self::contains(self, version)
    }

    fn selection_eq(&self, other: &Self) -> bool {
        self == other && self.selection_region() == other.selection_region()
    }

    fn may_refine_selection(&self) -> bool {
        self.prerelease_region.is_some()
    }

    fn selection_refinement(&self, requirement: &Self) -> Option<Self> {
        let refined = self.intersection(requirement);
        (self == &refined && !self.selection_eq(&refined)).then_some(refined)
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
        Display::fmt(self.raw_versions(), formatter)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::convert::Infallible;
    use std::hash::{Hash, Hasher};
    use std::mem::size_of;
    use std::str::FromStr;

    use pubgrub::{
        Dependencies, DependencyConstraints, DependencyProvider, Incompatibility,
        PackageResolutionStatistics, Ranges, SetRelation, State, VersionSet,
    };

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

    #[test]
    fn range_metadata_uses_two_pointers() {
        assert_eq!(
            size_of::<Range<Version>>(),
            size_of::<Ranges<Version>>() + 2 * size_of::<usize>()
        );
    }

    struct TestDependencyProvider;

    impl DependencyProvider for TestDependencyProvider {
        type P = &'static str;
        type V = Version;
        type VS = Range<Version>;
        type Priority = ();
        type M = String;
        type Err = Infallible;

        fn prioritize(
            &self,
            _package: &Self::P,
            _range: &Self::VS,
            _package_conflicts_counts: &PackageResolutionStatistics,
        ) -> Self::Priority {
        }

        fn choose_version(
            &self,
            _package: &Self::P,
            _range: &Self::VS,
        ) -> Result<Option<Self::V>, Self::Err> {
            Ok(None)
        }

        fn get_dependencies(
            &self,
            _package: &Self::P,
            _version: &Self::V,
        ) -> Result<Dependencies<Self::P, Self::VS, Self::M>, Self::Err> {
            Ok(Dependencies::Available(DependencyConstraints::default()))
        }
    }

    #[test]
    fn prerelease_region_is_clipped_to_its_specifier_bounds() {
        let range = range(">=2.0b1,<3").union(&range(">=3.5,<4"));

        assert!(range.contains(&Version::from_str("3.6b1").expect("valid version")));
        assert!(!range.prerelease_region().is_some_and(|region| {
            region.contains(&Version::from_str("3.6b1").expect("valid version"))
        }));
        assert!(range.prerelease_region().is_some_and(|region| {
            region.contains(&Version::from_str("2.5b1").expect("valid version"))
        }));
    }

    #[test]
    fn union_keeps_prerelease_admission_with_its_originating_range() {
        let range = range(">=1.0").union(&range(">=2.0b1"));

        let prereleases = range
            .prerelease_region()
            .expect("the pre-release specifier should create an opt-in region");
        assert!(!prereleases.contains(&Version::from_str("1.5b1").expect("valid version")));
        assert!(prereleases.contains(&Version::from_str("2.0b1").expect("valid version")));
    }

    #[test]
    fn narrowing_permanently_sheds_prerelease_admission() {
        let narrowed = range(">=2.0b1").intersection(&range("<3"));
        let widened = narrowed.union(&range(">=3.5,<4"));

        assert!(widened.contains(&Version::from_str("3.6b1").expect("valid version")));
        assert!(!widened.prerelease_region().is_some_and(|region| {
            region.contains(&Version::from_str("3.6b1").expect("valid version"))
        }));
    }

    #[test]
    fn difference_does_not_inherit_prerelease_admission() {
        let requirement = range(">=1.0");
        let exclusion = range(">=2.0b1");
        let range = requirement.difference(&exclusion);
        let via_complement = exclusion.complement().intersection(&requirement);

        assert_eq!(range, via_complement);
        assert_eq!(range.raw_versions(), via_complement.raw_versions());
        assert!(range.selection_eq(&via_complement));
        assert!(range.prerelease_region().is_none());
        assert!(via_complement.prerelease_region().is_none());
        assert!(range.contains(&Version::from_str("1.5a1").expect("valid version")));
    }

    #[test]
    fn difference_preserves_prerelease_admission_from_the_left() {
        let requirement = range(">=1.0a1,<4");
        let exclusion = range(">=2,<3");
        let range = requirement.difference(&exclusion);
        let via_complement = exclusion.complement().intersection(&requirement);

        assert_eq!(range.raw_versions(), via_complement.raw_versions());
        assert!(range.selection_eq(&via_complement));
        let prereleases = range
            .prerelease_region()
            .expect("the requirement should retain pre-release admission");
        assert!(prereleases.contains(&version("1.5a1")));
        assert!(!prereleases.contains(&version("2.5a1")));
        assert!(prereleases.contains(&version("3.5a1")));
    }

    #[test]
    fn complement_erases_prerelease_admission() {
        let range = range(">=2.0b1");
        let complemented = range.complement().complement();

        assert_eq!(range, complemented);
        assert!(!range.selection_eq(&complemented));
        assert!(complemented.prerelease_region().is_none());
    }

    #[test]
    fn set_relations_ignore_prerelease_admission() {
        let plain = range(">=1.0");
        let opted_in = plain.intersection(&range(">=1.0a1"));

        assert_eq!(plain.relation(&opted_in), SetRelation::Subset);
        assert!(plain.subset_of(&opted_in));
        assert!(!plain.is_disjoint(&opted_in));
    }

    #[test]
    fn range_identity_ignores_prerelease_admission() {
        let plain = range(">=1.0");
        let opted_in = plain.intersection(&range(">=1.0a1"));

        assert_eq!(plain, opted_in);
        assert!(!plain.selection_eq(&opted_in));
    }

    #[test]
    fn equivalent_prerelease_bounds_have_same_selection_metadata() {
        let exclusive = range(">1.0a1");
        let successor = range(">=1.0a2.dev0");
        let upper_bound = range("<2");

        assert!(exclusive.selection_eq(&successor));
        assert!(
            exclusive
                .intersection(&upper_bound)
                .selection_eq(&successor.intersection(&upper_bound))
        );
        assert!(exclusive.complement().selection_eq(&successor.complement()));
    }

    #[test]
    fn equivalent_membership_can_have_distinct_selection_metadata() {
        let plain = range("<=1.0");
        let opted_in = range("<1.0.post0.dev0");

        assert_eq!(plain, opted_in);
        assert!(!plain.selection_eq(&opted_in));
        assert_eq!(plain.complement(), opted_in.complement());
    }

    #[test]
    fn range_algebra_preserves_canonical_representations() {
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
                let difference = left.difference(right);
                let via_complement = right.complement().intersection(left);
                assert_eq!(difference.raw_versions(), via_complement.raw_versions());
                assert!(difference.selection_eq(&via_complement));
                assert_matches_recanonicalized(&difference);
            }
        }
    }

    #[test]
    fn operand_collapse_preserves_canonical_representations() {
        let range = range("<=1.0");
        let full = Range::full();
        let empty = Range::empty();

        for collapsed in [
            range.intersection(&full),
            full.intersection(&range),
            range.union(&empty),
            empty.union(&range),
        ] {
            assert_eq!(collapsed, range);
            assert_eq!(collapsed.raw_versions(), range.raw_versions());
            assert_matches_recanonicalized(&collapsed);
        }
    }

    #[test]
    fn raw_ranges_remain_visible() {
        let candidates = [
            "0.dev0",
            "0.0.dev0",
            "1.0.0.dev0",
            "1.0.0a1",
            "1.0.0a2.dev0",
            "1.0.0",
            "1.0.0.post0.dev0",
            "1.0.0+local",
        ]
        .map(version);
        for specifiers in ["<=1.0", "<1.0", ">1.0a1", "==0.dev0", "==1.0"] {
            let parsed = VersionSpecifiers::from_str(specifiers).expect("valid specifiers");
            let expected = Ranges::from(parsed.clone());
            let range = Range::from(parsed);

            assert!(range.raw_versions.is_some());
            assert_eq!(range.raw_versions(), &expected);
            assert_eq!(&*range, &expected);
            assert_eq!(range.to_string(), expected.to_string());
            for candidate in &candidates {
                assert_eq!(
                    range.contains(candidate),
                    expected.contains(candidate),
                    "{candidate} in {specifiers}"
                );
            }
        }
    }

    fn assert_matches_recanonicalized(range: &Range<Version>) {
        let canonical_versions = canonicalize_version_ranges(range.raw_versions())
            .unwrap_or_else(|| range.raw_versions().clone());
        assert_eq!(range.logical_versions(), &canonical_versions);

        let canonical_prerelease_region = range
            .prerelease_region()
            .map(|region| canonicalize_version_ranges(region).unwrap_or_else(|| region.clone()))
            .filter(|region| !region.is_empty());
        assert_eq!(
            range.selection_region(),
            canonical_prerelease_region.as_ref()
        );
    }

    #[test]
    fn canonical_empty_admission_remains_empty_after_intersection() {
        let equivalent = range("<1.0.post0.dev0");
        let admission = equivalent
            .prerelease_region()
            .expect("the pre-release bound should grant admission");
        let enriched = range("<=1.0").with_prerelease_region(admission);
        assert!(enriched.selection_eq(&equivalent));

        let complement = range("<=1.0").complement();
        assert!(
            enriched
                .intersection(&complement)
                .selection_eq(&equivalent.intersection(&complement))
        );
    }

    #[test]
    fn pep440_floor_is_not_logically_empty() {
        let floor = version("0.dev0");
        let specifier_range = range("==0.dev0");
        let singleton = Range::singleton(floor.clone());
        let canonical_singleton = Range::from_versions(Ranges::singleton(floor.clone()));

        assert!(singleton.raw_versions.is_some());
        assert!(specifier_range.contains(&floor));
        assert!(!specifier_range.is_disjoint(&singleton));
        assert_ne!(specifier_range, Range::empty());
        assert_eq!(singleton, canonical_singleton);
        assert_eq!(hash(&singleton), hash(&canonical_singleton));
        assert_eq!(singleton.complement(), canonical_singleton.complement());
        let upper_bound = range("<1");
        assert_eq!(
            singleton.intersection(&upper_bound),
            canonical_singleton.intersection(&upper_bound)
        );
        assert_eq!(
            singleton.union(&upper_bound),
            canonical_singleton.union(&upper_bound)
        );
    }

    #[test]
    fn ordinary_singletons_skip_canonicalization() {
        for version in [
            version("0a0.dev0"),
            version("1.0.dev0"),
            version("1.0a1"),
            version("1.0"),
            version("1.0.post1"),
            version("1.0+local"),
        ] {
            let singleton = Range::singleton(version.clone());
            let canonical_singleton = Range::from_versions(Ranges::singleton(version));

            assert!(singleton.raw_versions.is_none());
            assert_eq!(singleton, canonical_singleton);
            assert_eq!(hash(&singleton), hash(&canonical_singleton));
        }
    }

    fn hash(range: &Range<Version>) -> u64 {
        let mut hasher = DefaultHasher::new();
        range.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn replacing_prerelease_region_preserves_membership_and_clips_admission() {
        let plain = range(">=1,<3");
        let admission = range(">=0.5a1");
        let prerelease_region = admission
            .prerelease_region()
            .expect("the explicit range should admit pre-releases");
        let enriched = plain.with_prerelease_region(prerelease_region);
        let prerelease_region = enriched
            .prerelease_region()
            .expect("the enriched range should admit pre-releases");

        assert_eq!(plain, enriched);
        assert!(!plain.selection_eq(&enriched));
        assert!(prerelease_region.contains(&version("2.0a1")));
        assert!(!prerelease_region.contains(&version("0.9a1")));
        assert!(!prerelease_region.contains(&version("3.5a1")));
    }

    #[test]
    fn dependency_merging_keeps_prerelease_admission_distinct() {
        let mut state = State::<TestDependencyProvider>::init("root", version("0"));
        let dependent = state.package_store.alloc("a");
        let dependency = state.package_store.alloc("c");

        state.add_incompatibility(Incompatibility::from_dependency(
            dependent,
            Range::singleton(version("1")),
            (dependency, range(">=1")),
        ));
        state.add_incompatibility(Incompatibility::from_dependency(
            dependent,
            Range::singleton(version("2")),
            (dependency, range(">=1,>0a1")),
        ));

        assert_eq!(
            state
                .incompatibilities
                .get(&dependency)
                .expect("dependency incompatibilities should be registered")
                .len(),
            2
        );
    }
}
