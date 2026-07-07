//! Convert [`VersionSpecifiers`] to [`Ranges`].

use std::cmp::Ordering;
use std::collections::Bound;
use std::ops::Deref;
use std::sync::LazyLock;
use version_ranges::Ranges;

use crate::{
    LocalVersion, LocalVersionSlice, Operator, Prerelease, Version, VersionSpecifier,
    VersionSpecifiers,
};

/// The smallest valid PEP 440 version.
static PEP440_MIN_VERSION: LazyLock<Version> =
    LazyLock::new(|| Version::new([0]).with_dev(Some(0)));

/// Canonicalize the internal sentinel bounds in a version range over the PEP 440 version universe.
///
/// [`Ranges`] treats its coordinate type as continuous, while PEP 440 has known least successors
/// for some otherwise-impossible internal boundary versions. Folding those boundaries onto their
/// successor gives membership-equivalent ranges the same equality and hash representation.
///
/// Returns `None` when the range is already canonical.
pub fn canonicalize_version_ranges(ranges: &Ranges<Version>) -> Option<Ranges<Version>> {
    if !ranges.iter().any(|(lower, upper)| {
        lower_bound_needs_canonicalization(lower) || upper_bound_needs_canonicalization(upper)
    }) {
        return None;
    }

    Some(
        ranges
            .clone()
            .into_iter()
            .filter_map(|(lower, upper)| {
                let mut lower = canonicalize_lower_bound(lower);
                let upper = canonicalize_upper_bound(upper);

                match &lower {
                    Bound::Included(version) if version <= &*PEP440_MIN_VERSION => {
                        lower = Bound::Unbounded;
                    }
                    Bound::Excluded(version) if version < &*PEP440_MIN_VERSION => {
                        lower = Bound::Unbounded;
                    }
                    Bound::Included(_) | Bound::Excluded(_) | Bound::Unbounded => {}
                }

                let below_floor = match &upper {
                    Bound::Included(version) => version < &*PEP440_MIN_VERSION,
                    Bound::Excluded(version) => version <= &*PEP440_MIN_VERSION,
                    Bound::Unbounded => false,
                };
                (!below_floor).then_some((lower, upper))
            })
            .collect(),
    )
}

fn lower_bound_needs_canonicalization(bound: Bound<&Version>) -> bool {
    match bound {
        Bound::Included(version) => {
            version <= &*PEP440_MIN_VERSION || sentinel_successor(version).is_some()
        }
        Bound::Excluded(version) => {
            version < &*PEP440_MIN_VERSION || sentinel_successor(version).is_some()
        }
        Bound::Unbounded => false,
    }
}

fn upper_bound_needs_canonicalization(bound: Bound<&Version>) -> bool {
    match bound {
        Bound::Included(version) => {
            version < &*PEP440_MIN_VERSION || sentinel_successor(version).is_some()
        }
        Bound::Excluded(version) => {
            version <= &*PEP440_MIN_VERSION || sentinel_successor(version).is_some()
        }
        Bound::Unbounded => false,
    }
}

fn canonicalize_lower_bound(bound: Bound<Version>) -> Bound<Version> {
    match bound {
        Bound::Included(version) => {
            sentinel_successor(&version).map_or(Bound::Included(version), Bound::Included)
        }
        Bound::Excluded(version) => {
            sentinel_successor(&version).map_or(Bound::Excluded(version), Bound::Included)
        }
        Bound::Unbounded => Bound::Unbounded,
    }
}

fn canonicalize_upper_bound(bound: Bound<Version>) -> Bound<Version> {
    match bound {
        Bound::Included(version) => {
            sentinel_successor(&version).map_or(Bound::Included(version), Bound::Excluded)
        }
        Bound::Excluded(version) => {
            sentinel_successor(&version).map_or(Bound::Excluded(version), Bound::Excluded)
        }
        Bound::Unbounded => Bound::Unbounded,
    }
}

/// Return the least real PEP 440 version above an internal sentinel boundary.
fn sentinel_successor(version: &Version) -> Option<Version> {
    if version.local() == LocalVersionSlice::Max {
        let version = version.clone().without_local();
        return if let Some(dev) = version.dev() {
            Some(version.with_dev(Some(dev.checked_add(1)?)))
        } else if let Some(post) = version.post() {
            Some(
                version
                    .with_post(Some(post.checked_add(1)?))
                    .with_dev(Some(0)),
            )
        } else {
            Some(version.with_post(Some(0)).with_dev(Some(0)))
        };
    }

    if version.min().is_some() {
        return Some(version.clone().with_min(None).with_dev(Some(0)));
    }

    if version.max().is_some()
        && let Some(prerelease) = version.pre()
    {
        return Some(
            version
                .clone()
                .with_max(None)
                .with_pre(Some(Prerelease {
                    kind: prerelease.kind,
                    number: prerelease.number.checked_add(1)?,
                }))
                .with_dev(Some(0)),
        );
    }

    None
}

impl From<VersionSpecifiers> for Ranges<Version> {
    /// Convert [`VersionSpecifiers`] to a PubGrub-compatible version range, using PEP 440
    /// semantics.
    fn from(specifiers: VersionSpecifiers) -> Self {
        let mut range = Self::full();
        for specifier in specifiers {
            range = range.intersection(&Self::from(specifier));
        }
        range
    }
}

impl From<VersionSpecifier> for Ranges<Version> {
    /// Convert the [`VersionSpecifier`] to a PubGrub-compatible version range, using PEP 440
    /// semantics.
    fn from(specifier: VersionSpecifier) -> Self {
        let VersionSpecifier { operator, version } = specifier;
        match operator {
            Operator::Equal => match version.local() {
                LocalVersionSlice::Segments(&[]) => {
                    let low = version;
                    let high = low.clone().with_local(LocalVersion::Max);
                    Self::between(low, high)
                }
                LocalVersionSlice::Segments(_) => Self::singleton(version),
                LocalVersionSlice::Max => unreachable!(
                    "found `LocalVersionSlice::Sentinel`, which should be an internal-only value"
                ),
            },
            Operator::ExactEqual => Self::singleton(version),
            Operator::NotEqual => Self::from(VersionSpecifier {
                operator: Operator::Equal,
                version,
            })
            .complement(),
            Operator::TildeEqual => {
                let release = version.release();
                let [rest @ .., last, _] = &*release else {
                    unreachable!("~= must have at least two segments");
                };
                let upper = Version::new(rest.iter().chain([&(last + 1)]))
                    .with_epoch(version.epoch())
                    .with_dev(Some(0));

                Self::from_range_bounds(version..upper)
            }
            Operator::LessThan => {
                // Per PEP 440: "The exclusive ordered comparison <V MUST NOT allow a
                // pre-release of the specified version unless the specified version is itself a
                // pre-release."
                if version.any_prerelease() {
                    // If V is a pre-release, we allow pre-releases of the same version.
                    Self::strictly_lower_than(version)
                } else if let Some(post) = version.post() {
                    // If V is a post-release (e.g., `<0.12.0.post2`), we want to:
                    // - Exclude pre-releases of the base version (e.g., `0.12.0a1`)
                    // - Include the final release (e.g., `0.12.0`)
                    // - Include earlier post-releases (e.g., `0.12.0.post1`)
                    //
                    // The range is: `(-∞, base.min0) ∪ [base, V.post)`
                    // where `base` is the version without the post-release component.
                    let base = version.clone().with_post(None);
                    // Everything below the base version's pre-releases
                    let lower = Self::strictly_lower_than(base.clone().with_min(Some(0)));
                    // From base (inclusive) up to but not including V
                    let upper = Self::from_range_bounds(base..version.with_post(Some(post)));
                    lower.union(&upper)
                } else {
                    // V is not a pre-release or post-release, so exclude pre-releases of the
                    // specified version by using a "min" sentinel that sorts before all
                    // pre-releases.
                    Self::strictly_lower_than(version.with_min(Some(0)))
                }
            }
            Operator::LessThanEqual => Self::lower_than(version.with_local(LocalVersion::Max)),
            Operator::GreaterThan => {
                // Per PEP 440: "The exclusive ordered comparison >V MUST NOT allow a post-release of
                // the given version unless V itself is a post release."

                if let Some(dev) = version.dev() {
                    Self::higher_than(version.with_dev(Some(dev + 1)))
                } else if let Some(post) = version.post() {
                    Self::higher_than(version.with_post(Some(post + 1)))
                } else {
                    Self::strictly_higher_than(version.with_max(Some(0)))
                }
            }
            Operator::GreaterThanEqual => Self::higher_than(version),
            Operator::EqualStar => {
                let low = version.with_dev(Some(0));
                let mut high = low.clone();
                if let Some(post) = high.post() {
                    high = high.with_post(Some(post + 1));
                } else if let Some(pre) = high.pre() {
                    high = high.with_pre(Some(Prerelease {
                        kind: pre.kind,
                        number: pre.number + 1,
                    }));
                } else {
                    let mut release = high.release().to_vec();
                    *release.last_mut().unwrap() += 1;
                    high = high.with_release(release);
                }
                Self::from_range_bounds(low..high)
            }
            Operator::NotEqualStar => {
                let low = version.with_dev(Some(0));
                let mut high = low.clone();
                if let Some(post) = high.post() {
                    high = high.with_post(Some(post + 1));
                } else if let Some(pre) = high.pre() {
                    high = high.with_pre(Some(Prerelease {
                        kind: pre.kind,
                        number: pre.number + 1,
                    }));
                } else {
                    let mut release = high.release().to_vec();
                    *release.last_mut().unwrap() += 1;
                    high = high.with_release(release);
                }
                Self::from_range_bounds(low..high).complement()
            }
        }
    }
}

/// Convert the [`VersionSpecifiers`] to a PubGrub-compatible version range, using release-only
/// semantics.
///
/// Assumes that the range will only be tested against versions that consist solely of release
/// segments (e.g., `3.12.0`, but not `3.12.0b1`).
///
/// These semantics are used for testing Python compatibility (e.g., `requires-python` against
/// the user's installed Python version). In that context, it's more intuitive that `3.13.0b0`
/// is allowed for projects that declare `requires-python = ">=3.13"`.
///
/// See: <https://github.com/pypa/pip/blob/a432c7f4170b9ef798a15f035f5dfdb4cc939f35/src/pip/_internal/resolution/resolvelib/candidates.py#L540>
pub fn release_specifiers_to_ranges(specifiers: VersionSpecifiers) -> Ranges<Version> {
    let mut range = Ranges::full();
    for specifier in specifiers {
        range = range.intersection(&release_specifier_to_range(specifier, false));
    }
    range
}

/// Convert the [`VersionSpecifier`] to a PubGrub-compatible version range, using release-only
/// semantics.
///
/// Assumes that the range will only be tested against versions that consist solely of release
/// segments (e.g., `3.12.0`, but not `3.12.0b1`).
///
/// These semantics are used for testing Python compatibility (e.g., `requires-python` against
/// the user's installed Python version). In that context, it's more intuitive that `3.13.0b0`
/// is allowed for projects that declare `requires-python = ">3.13"`.
///
/// See: <https://github.com/pypa/pip/blob/a432c7f4170b9ef798a15f035f5dfdb4cc939f35/src/pip/_internal/resolution/resolvelib/candidates.py#L540>
pub fn release_specifier_to_range(specifier: VersionSpecifier, trim: bool) -> Ranges<Version> {
    let VersionSpecifier { operator, version } = specifier;
    // Note(konsti): We switched strategies to trimmed for the markers, but we don't want to cause
    // churn in lockfile requires-python, so we only trim for markers.
    let version_trimmed = if trim {
        version.only_release_trimmed()
    } else {
        version.only_release()
    };
    match operator {
        // Trailing zeroes are not semantically relevant.
        Operator::Equal => Ranges::singleton(version_trimmed),
        Operator::ExactEqual => Ranges::singleton(version_trimmed),
        Operator::NotEqual => Ranges::singleton(version_trimmed).complement(),
        Operator::LessThan => Ranges::strictly_lower_than(version_trimmed),
        Operator::LessThanEqual => Ranges::lower_than(version_trimmed),
        Operator::GreaterThan => Ranges::strictly_higher_than(version_trimmed),
        Operator::GreaterThanEqual => Ranges::higher_than(version_trimmed),

        // Trailing zeroes are semantically relevant.
        Operator::TildeEqual => {
            let release = version.release();
            let [rest @ .., last, _] = &*release else {
                unreachable!("~= must have at least two segments");
            };
            let upper = Version::new(rest.iter().chain([&(last + 1)]));
            Ranges::from_range_bounds(version_trimmed..upper)
        }
        Operator::EqualStar => {
            // For (not-)equal-star, trailing zeroes are still before the star.
            let low_full = version.only_release();
            let high = {
                let mut high = low_full.clone();
                let mut release = high.release().to_vec();
                *release.last_mut().unwrap() += 1;
                high = high.with_release(release);
                high
            };
            Ranges::from_range_bounds(version..high)
        }
        Operator::NotEqualStar => {
            // For (not-)equal-star, trailing zeroes are still before the star.
            let low_full = version.only_release();
            let high = {
                let mut high = low_full.clone();
                let mut release = high.release().to_vec();
                *release.last_mut().unwrap() += 1;
                high = high.with_release(release);
                high
            };
            Ranges::from_range_bounds(version..high).complement()
        }
    }
}

/// A lower bound for a version range.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct LowerBound(pub Bound<Version>);

impl LowerBound {
    /// Initialize a [`LowerBound`] with the given bound.
    ///
    /// These bounds use release-only semantics when comparing versions.
    pub fn new(bound: Bound<Version>) -> Self {
        Self(match bound {
            Bound::Included(version) => Bound::Included(version.only_release_trimmed()),
            Bound::Excluded(version) => Bound::Excluded(version.only_release_trimmed()),
            Bound::Unbounded => Bound::Unbounded,
        })
    }

    /// Return the [`LowerBound`] truncated to the major and minor version.
    #[must_use]
    pub fn major_minor(&self) -> Self {
        match &self.0 {
            // Ex) `>=3.10.1` -> `>=3.10`
            Bound::Included(version) => Self(Bound::Included(Version::new(
                version.release().iter().take(2),
            ))),
            // Ex) `>3.10.1` -> `>=3.10`.
            Bound::Excluded(version) => Self(Bound::Included(Version::new(
                version.release().iter().take(2),
            ))),
            Bound::Unbounded => Self(Bound::Unbounded),
        }
    }

    /// Returns `true` if the lower bound contains the given version.
    pub fn contains(&self, version: &Version) -> bool {
        match self.0 {
            Bound::Included(ref bound) => bound <= version,
            Bound::Excluded(ref bound) => bound < version,
            Bound::Unbounded => true,
        }
    }

    /// Returns the [`VersionSpecifier`] for the lower bound.
    pub fn specifier(&self) -> Option<VersionSpecifier> {
        match &self.0 {
            Bound::Included(version) => Some(VersionSpecifier::greater_than_equal_version(
                version.clone(),
            )),
            Bound::Excluded(version) => {
                Some(VersionSpecifier::greater_than_version(version.clone()))
            }
            Bound::Unbounded => None,
        }
    }
}

impl PartialOrd for LowerBound {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// See: <https://github.com/pubgrub-rs/pubgrub/blob/4b4b44481c5f93f3233221dc736dd23e67e00992/src/range.rs#L324>
impl Ord for LowerBound {
    fn cmp(&self, other: &Self) -> Ordering {
        let left = self.0.as_ref();
        let right = other.0.as_ref();

        match (left, right) {
            // left:   ∞-----
            // right:  ∞-----
            (Bound::Unbounded, Bound::Unbounded) => Ordering::Equal,
            // left:     [---
            // right:  ∞-----
            (Bound::Included(_left), Bound::Unbounded) => Ordering::Greater,
            // left:     ]---
            // right:  ∞-----
            (Bound::Excluded(_left), Bound::Unbounded) => Ordering::Greater,
            // left:   ∞-----
            // right:    [---
            (Bound::Unbounded, Bound::Included(_right)) => Ordering::Less,
            // left:   [----- OR [----- OR   [-----
            // right:    [--- OR [----- OR [---
            (Bound::Included(left), Bound::Included(right)) => left.cmp(right),
            (Bound::Excluded(left), Bound::Included(right)) => match left.cmp(right) {
                // left:   ]-----
                // right:    [---
                Ordering::Less => Ordering::Less,
                // left:   ]-----
                // right:  [---
                Ordering::Equal => Ordering::Greater,
                // left:     ]---
                // right:  [-----
                Ordering::Greater => Ordering::Greater,
            },
            // left:   ∞-----
            // right:    ]---
            (Bound::Unbounded, Bound::Excluded(_right)) => Ordering::Less,
            (Bound::Included(left), Bound::Excluded(right)) => match left.cmp(right) {
                // left:   [-----
                // right:    ]---
                Ordering::Less => Ordering::Less,
                // left:   [-----
                // right:  ]---
                Ordering::Equal => Ordering::Less,
                // left:     [---
                // right:  ]-----
                Ordering::Greater => Ordering::Greater,
            },
            // left:   ]----- OR ]----- OR   ]---
            // right:    ]--- OR ]----- OR ]-----
            (Bound::Excluded(left), Bound::Excluded(right)) => left.cmp(right),
        }
    }
}

impl Default for LowerBound {
    fn default() -> Self {
        Self(Bound::Unbounded)
    }
}

impl Deref for LowerBound {
    type Target = Bound<Version>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<LowerBound> for Bound<Version> {
    fn from(bound: LowerBound) -> Self {
        bound.0
    }
}

/// An upper bound for a version range.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct UpperBound(pub Bound<Version>);

impl UpperBound {
    /// Initialize a [`UpperBound`] with the given bound.
    ///
    /// These bounds use release-only semantics when comparing versions.
    pub fn new(bound: Bound<Version>) -> Self {
        Self(match bound {
            Bound::Included(version) => Bound::Included(version.only_release_trimmed()),
            Bound::Excluded(version) => Bound::Excluded(version.only_release_trimmed()),
            Bound::Unbounded => Bound::Unbounded,
        })
    }

    /// Return the [`UpperBound`] truncated to the major and minor version.
    #[must_use]
    pub fn major_minor(&self) -> Self {
        match &self.0 {
            // Ex) `<=3.10.1` -> `<=3.10`
            Bound::Included(version) => Self(Bound::Included(Version::new(
                version.release().iter().take(2),
            ))),
            // Ex) `<3.10.1` -> `<=3.10` (but `<3.10.0` is `<3.10`)
            Bound::Excluded(version) => {
                if version.release().get(2).is_some_and(|patch| *patch > 0) {
                    Self(Bound::Included(Version::new(
                        version.release().iter().take(2),
                    )))
                } else {
                    Self(Bound::Excluded(Version::new(
                        version.release().iter().take(2),
                    )))
                }
            }
            Bound::Unbounded => Self(Bound::Unbounded),
        }
    }

    /// Returns `true` if the upper bound contains the given version.
    pub fn contains(&self, version: &Version) -> bool {
        match self.0 {
            Bound::Included(ref bound) => bound >= version,
            Bound::Excluded(ref bound) => bound > version,
            Bound::Unbounded => true,
        }
    }

    /// Returns the [`VersionSpecifier`] for the upper bound.
    pub fn specifier(&self) -> Option<VersionSpecifier> {
        match &self.0 {
            Bound::Included(version) => {
                Some(VersionSpecifier::less_than_equal_version(version.clone()))
            }
            Bound::Excluded(version) => Some(VersionSpecifier::less_than_version(version.clone())),
            Bound::Unbounded => None,
        }
    }
}

impl PartialOrd for UpperBound {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// See: <https://github.com/pubgrub-rs/pubgrub/blob/4b4b44481c5f93f3233221dc736dd23e67e00992/src/range.rs#L324>
impl Ord for UpperBound {
    fn cmp(&self, other: &Self) -> Ordering {
        let left = self.0.as_ref();
        let right = other.0.as_ref();

        match (left, right) {
            // left:   -----∞
            // right:  -----∞
            (Bound::Unbounded, Bound::Unbounded) => Ordering::Equal,
            // left:   ---]
            // right:  -----∞
            (Bound::Included(_left), Bound::Unbounded) => Ordering::Less,
            // left:   ---[
            // right:  -----∞
            (Bound::Excluded(_left), Bound::Unbounded) => Ordering::Less,
            // left:  -----∞
            // right: ---]
            (Bound::Unbounded, Bound::Included(_right)) => Ordering::Greater,
            // left:   -----] OR -----] OR ---]
            // right:    ---] OR -----] OR -----]
            (Bound::Included(left), Bound::Included(right)) => left.cmp(right),
            (Bound::Excluded(left), Bound::Included(right)) => match left.cmp(right) {
                // left:   ---[
                // right:  -----]
                Ordering::Less => Ordering::Less,
                // left:   -----[
                // right:  -----]
                Ordering::Equal => Ordering::Less,
                // left:   -----[
                // right:  ---]
                Ordering::Greater => Ordering::Greater,
            },
            (Bound::Unbounded, Bound::Excluded(_right)) => Ordering::Greater,
            (Bound::Included(left), Bound::Excluded(right)) => match left.cmp(right) {
                // left:   ---]
                // right:  -----[
                Ordering::Less => Ordering::Less,
                // left:   -----]
                // right:  -----[
                Ordering::Equal => Ordering::Greater,
                // left:   -----]
                // right:  ---[
                Ordering::Greater => Ordering::Greater,
            },
            // left:   -----[ OR -----[ OR ---[
            // right:  ---[   OR -----[ OR -----[
            (Bound::Excluded(left), Bound::Excluded(right)) => left.cmp(right),
        }
    }
}

impl Default for UpperBound {
    fn default() -> Self {
        Self(Bound::Unbounded)
    }
}

impl Deref for UpperBound {
    type Target = Bound<Version>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<UpperBound> for Bound<Version> {
    fn from(bound: UpperBound) -> Self {
        bound.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_known_pep440_successor_boundaries() {
        let range = |specifier: &str| {
            let range = Ranges::from(specifier.parse::<VersionSpecifiers>().unwrap());
            canonicalize_version_ranges(&range).unwrap_or(range)
        };

        assert_eq!(range(">1.0a1"), range(">=1.0a2.dev0"));
        assert_eq!(range("<=1.0"), range("<1.0.post0.dev0"));
        assert_eq!(range("==1.0"), range(">=1.0,<1.0.post0.dev0"));
        assert_eq!(range(">=0.dev0"), Ranges::full());
        assert_eq!(range("<0.dev0"), Ranges::empty());
    }

    #[test]
    fn canonicalization_preserves_pep440_floor_membership() {
        let versions = ["0.dev0", "0a0.dev0"].map(|version| {
            version
                .parse::<Version>()
                .expect("valid version for floor test")
        });

        for specifiers in ["==0.dev0", "!=0.dev0", ">=0a0.dev0", "<0a0.dev0"] {
            let range = Ranges::from(
                specifiers
                    .parse::<VersionSpecifiers>()
                    .expect("valid specifiers for floor test"),
            );
            let canonical = canonicalize_version_ranges(&range).unwrap_or_else(|| range.clone());

            for version in &versions {
                assert_eq!(
                    range.contains(version),
                    canonical.contains(version),
                    "canonicalizing `{specifiers}` changed membership for `{version}`"
                );
            }
        }
    }

    #[test]
    fn canonicalization_preserves_real_version_membership() {
        let versions = [
            "0.dev0",
            "0a0.dev0",
            "0.9",
            "1.0.dev0",
            "1.0a1.dev0",
            "1.0a1",
            "1.0a2.dev0",
            "1.0",
            "1.0+local",
            "1.0.post0.dev0",
            "1.0.post1.dev0",
            "2.0",
        ]
        .map(|version| {
            version
                .parse::<Version>()
                .expect("valid version for canonicalization test")
        });

        for specifiers in [
            "==1.0",
            "!=1.0",
            "<1.0",
            "<=1.0",
            ">1.0a1",
            "<1.0.post1",
        ] {
            let range = Ranges::from(
                specifiers
                    .parse::<VersionSpecifiers>()
                    .expect("valid specifiers for canonicalization test"),
            );
            let canonical = canonicalize_version_ranges(&range)
                .expect("the specifier should contain an internal sentinel");

            for version in &versions {
                assert_eq!(
                    range.contains(version),
                    canonical.contains(version),
                    "canonicalizing `{specifiers}` changed membership for `{version}`"
                );
            }
        }
    }

    #[test]
    fn skips_ranges_without_internal_sentinels() {
        let range = Ranges::singleton("1.0".parse::<Version>().expect("valid version"));

        assert!(canonicalize_version_ranges(&range).is_none());
    }

    /// Test that `<V.postN` excludes pre-releases of the base version but includes
    /// earlier post-releases and the final release.
    ///
    /// See: <https://github.com/astral-sh/uv/issues/16868>
    #[test]
    fn less_than_post_release() {
        let specifier: VersionSpecifier = "<0.12.0.post2".parse().unwrap();
        let range = Ranges::<Version>::from(specifier);

        // Should include versions less than base release.
        let v = "0.11.0".parse::<Version>().unwrap();
        assert!(range.contains(&v), "should include 0.11.0");

        // Should exclude pre-releases of the base release.
        let v = "0.12.0a1".parse::<Version>().unwrap();
        assert!(!range.contains(&v), "should exclude 0.12.0a1");

        let v = "0.12.0b1".parse::<Version>().unwrap();
        assert!(!range.contains(&v), "should exclude 0.12.0b1");

        let v = "0.12.0rc1".parse::<Version>().unwrap();
        assert!(!range.contains(&v), "should exclude 0.12.0rc1");

        let v = "0.12.0.dev0".parse::<Version>().unwrap();
        assert!(!range.contains(&v), "should exclude 0.12.0.dev0");

        // Should also exclude post-releases of pre-releases.
        let v = "0.12.0a1.post1".parse::<Version>().unwrap();
        assert!(!range.contains(&v), "should exclude 0.12.0a1.post1");

        let v = "0.12.0b1.post1".parse::<Version>().unwrap();
        assert!(!range.contains(&v), "should exclude 0.12.0b1.post1");

        // Should include the final release.
        let v = "0.12.0".parse::<Version>().unwrap();
        assert!(range.contains(&v), "should include 0.12.0");

        // Should include earlier post-releases.
        let v = "0.12.0.post1".parse::<Version>().unwrap();
        assert!(range.contains(&v), "should include 0.12.0.post1");

        // Should exclude the specified post-release.
        let v = "0.12.0.post2".parse::<Version>().unwrap();
        assert!(!range.contains(&v), "should exclude 0.12.0.post2");

        // Should exclude later versions.
        let v = "0.13.0".parse::<Version>().unwrap();
        assert!(!range.contains(&v), "should exclude 0.13.0");
    }

    /// Test that `<V` (non-post-release) correctly excludes pre-releases.
    #[test]
    fn less_than_final_release() {
        let specifier: VersionSpecifier = "<0.12.0".parse().unwrap();
        let range = Ranges::<Version>::from(specifier);

        // Should include versions less than base release.
        let v = "0.11.0".parse::<Version>().unwrap();
        assert!(range.contains(&v), "should include 0.11.0");

        // Should exclude pre-releases of the specified version.
        let v = "0.12.0a1".parse::<Version>().unwrap();
        assert!(!range.contains(&v), "should exclude 0.12.0a1");

        let v = "0.12.0.dev0".parse::<Version>().unwrap();
        assert!(!range.contains(&v), "should exclude 0.12.0.dev0");

        // Should exclude the specified version.
        let v = "0.12.0".parse::<Version>().unwrap();
        assert!(!range.contains(&v), "should exclude 0.12.0");

        // Should exclude post-releases of the specified version.
        let v = "0.12.0.post1".parse::<Version>().unwrap();
        assert!(!range.contains(&v), "should exclude 0.12.0.post1");
    }

    /// Test that `<V.preN` allows earlier pre-releases of the same version.
    #[test]
    fn less_than_pre_release() {
        let specifier: VersionSpecifier = "<0.12.0b1".parse().unwrap();
        let range = Ranges::<Version>::from(specifier);

        // Should include earlier pre-releases.
        let v = "0.12.0a1".parse::<Version>().unwrap();
        assert!(range.contains(&v), "should include 0.12.0a1");

        let v = "0.12.0.dev0".parse::<Version>().unwrap();
        assert!(range.contains(&v), "should include 0.12.0.dev0");

        // Should exclude the specified pre-release and later.
        let v = "0.12.0b1".parse::<Version>().unwrap();
        assert!(!range.contains(&v), "should exclude 0.12.0b1");

        let v = "0.12.0".parse::<Version>().unwrap();
        assert!(!range.contains(&v), "should exclude 0.12.0");
    }

    /// Test the edge case where `<V.post0` still includes the final release.
    #[test]
    fn less_than_post_zero() {
        let specifier: VersionSpecifier = "<0.12.0.post0".parse().unwrap();
        let range = Ranges::<Version>::from(specifier);

        // Should include versions less than base release.
        let v = "0.11.0".parse::<Version>().unwrap();
        assert!(range.contains(&v), "should include 0.11.0");

        // Should exclude pre-releases of the base release.
        let v = "0.12.0a1".parse::<Version>().unwrap();
        assert!(!range.contains(&v), "should exclude 0.12.0a1");

        // Should include the final release (0.12.0 < 0.12.0.post0).
        let v = "0.12.0".parse::<Version>().unwrap();
        assert!(range.contains(&v), "should include 0.12.0");

        // Should exclude post0 and later.
        let v = "0.12.0.post0".parse::<Version>().unwrap();
        assert!(!range.contains(&v), "should exclude 0.12.0.post0");

        let v = "0.12.0.post1".parse::<Version>().unwrap();
        assert!(!range.contains(&v), "should exclude 0.12.0.post1");
    }

    /// Do not panic with `u64::MAX` causing an `u64::MAX + 1` overflow.
    #[test]
    fn u64_max_version_segments_rejected_at_parse_time() {
        assert!(
            "~=18446744073709551615.0"
                .parse::<VersionSpecifier>()
                .is_err()
        );
        assert!(
            "==18446744073709551615.*"
                .parse::<VersionSpecifier>()
                .is_err()
        );

        // u64::MAX - 1 is still accepted.
        assert!(
            "~=18446744073709551614.0"
                .parse::<VersionSpecifier>()
                .is_ok()
        );
        assert!(
            "==18446744073709551614.*"
                .parse::<VersionSpecifier>()
                .is_ok()
        );
    }
}
