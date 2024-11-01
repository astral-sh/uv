//! Convert [`VersionSpecifiers`] to [`version_ranges::Ranges`].

use version_ranges::Ranges;

use crate::{Operator, Prerelease, Version, VersionSpecifier, VersionSpecifiers};

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
            Operator::Equal => Ranges::singleton(version),
            Operator::ExactEqual => Ranges::singleton(version),
            Operator::NotEqual => Ranges::singleton(version).complement(),
            Operator::TildeEqual => {
                let [rest @ .., last, _] = version.release() else {
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
            Operator::LessThanEqual => Ranges::lower_than(version),
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
/// is allowed for projects that declare `requires-python = ">3.13"`.
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
            let [rest @ .., last, _] = version.release() else {
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
