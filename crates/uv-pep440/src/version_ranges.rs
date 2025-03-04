//! Convert [`VersionSpecifiers`] to [`Ranges`].

use std::cmp::Ordering;
use std::collections::Bound;
use std::ops::Deref;
use version_ranges::Ranges;

use crate::{
    LocalVersion, LocalVersionSlice, Operator, Prerelease, Version, VersionSpecifier,
    VersionSpecifiers,
};

impl From<VersionSpecifiers> for Ranges<Version> {
    /// Convert [`VersionSpecifiers`] to a PubGrub-compatible version range, using PEP 440
    /// semantics.
    fn from(specifiers: VersionSpecifiers) -> Self {
        let mut range = Ranges::full();
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
                    Ranges::between(low, high)
                }
                LocalVersionSlice::Segments(_) => Ranges::singleton(version),
                LocalVersionSlice::Max => unreachable!(
                    "found `LocalVersionSlice::Sentinel`, which should be an internal-only value"
                ),
            },
            Operator::ExactEqual => Ranges::singleton(version),
            Operator::NotEqual => Ranges::from(VersionSpecifier {
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

                Ranges::from_range_bounds(version..upper)
            }
            Operator::LessThan => {
                if version.any_prerelease() {
                    Ranges::strictly_lower_than(version)
                } else {
                    // Per PEP 440: "The exclusive ordered comparison <V MUST NOT allow a
                    // pre-release of the specified version unless the specified version is itself a
                    // pre-release."
                    Ranges::strictly_lower_than(version.with_min(Some(0)))
                }
            }
            Operator::LessThanEqual => Ranges::lower_than(version.with_local(LocalVersion::Max)),
            Operator::GreaterThan => {
                // Per PEP 440: "The exclusive ordered comparison >V MUST NOT allow a post-release of
                // the given version unless V itself is a post release."

                if let Some(dev) = version.dev() {
                    Ranges::higher_than(version.with_dev(Some(dev + 1)))
                } else if let Some(post) = version.post() {
                    Ranges::higher_than(version.with_post(Some(post + 1)))
                } else {
                    Ranges::strictly_higher_than(version.with_max(Some(0)))
                }
            }
            Operator::GreaterThanEqual => Ranges::higher_than(version),
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
                Ranges::from_range_bounds(low..high)
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
                Ranges::from_range_bounds(low..high).complement()
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
        range = range.intersection(&release_specifier_to_range(specifier));
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
pub fn release_specifier_to_range(specifier: VersionSpecifier) -> Ranges<Version> {
    let VersionSpecifier { operator, version } = specifier;
    match operator {
        Operator::Equal => {
            let version = version.only_release();
            Ranges::singleton(version)
        }
        Operator::ExactEqual => {
            let version = version.only_release();
            Ranges::singleton(version)
        }
        Operator::NotEqual => {
            let version = version.only_release();
            Ranges::singleton(version).complement()
        }
        Operator::TildeEqual => {
            let release = version.release();
            let [rest @ .., last, _] = &*release else {
                unreachable!("~= must have at least two segments");
            };
            let upper = Version::new(rest.iter().chain([&(last + 1)]));
            let version = version.only_release();
            Ranges::from_range_bounds(version..upper)
        }
        Operator::LessThan => {
            let version = version.only_release();
            Ranges::strictly_lower_than(version)
        }
        Operator::LessThanEqual => {
            let version = version.only_release();
            Ranges::lower_than(version)
        }
        Operator::GreaterThan => {
            let version = version.only_release();
            Ranges::strictly_higher_than(version)
        }
        Operator::GreaterThanEqual => {
            let version = version.only_release();
            Ranges::higher_than(version)
        }
        Operator::EqualStar => {
            let low = version.only_release();
            let high = {
                let mut high = low.clone();
                let mut release = high.release().to_vec();
                *release.last_mut().unwrap() += 1;
                high = high.with_release(release);
                high
            };
            Ranges::from_range_bounds(low..high)
        }
        Operator::NotEqualStar => {
            let low = version.only_release();
            let high = {
                let mut high = low.clone();
                let mut release = high.release().to_vec();
                *release.last_mut().unwrap() += 1;
                high = high.with_release(release);
                high
            };
            Ranges::from_range_bounds(low..high).complement()
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
            Bound::Included(version) => Bound::Included(version.only_release()),
            Bound::Excluded(version) => Bound::Excluded(version.only_release()),
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
            Bound::Included(version) => Bound::Included(version.only_release()),
            Bound::Excluded(version) => Bound::Excluded(version.only_release()),
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
