use anyhow::Result;
use pubgrub::range::Range;

use pep440_rs::{Operator, VersionSpecifier};

use crate::pubgrub::version::PubGrubVersion;
use crate::ResolveError;

/// A range of versions that can be used to satisfy a requirement.
#[derive(Debug)]
pub(crate) struct PubGrubSpecifier(Range<PubGrubVersion>);

impl From<PubGrubSpecifier> for Range<PubGrubVersion> {
    /// Convert a `PubGrub` specifier to a range of versions.
    fn from(specifier: PubGrubSpecifier) -> Self {
        specifier.0
    }
}

impl TryFrom<&VersionSpecifier> for PubGrubSpecifier {
    type Error = ResolveError;

    /// Convert a PEP 508 specifier to a `PubGrub`-compatible version range.
    fn try_from(specifier: &VersionSpecifier) -> Result<Self, ResolveError> {
        let ranges = match specifier.operator() {
            Operator::Equal => {
                let version = PubGrubVersion::from(specifier.version().clone());
                Range::singleton(version)
            }
            Operator::ExactEqual => {
                let version = PubGrubVersion::from(specifier.version().clone());
                Range::singleton(version)
            }
            Operator::NotEqual => {
                let version = PubGrubVersion::from(specifier.version().clone());
                Range::singleton(version).complement()
            }
            Operator::TildeEqual => {
                let [rest @ .., last, _] = specifier.version().release() else {
                    return Err(ResolveError::InvalidTildeEquals(specifier.clone()));
                };
                let upper = PubGrubVersion::from(
                    pep440_rs::Version::new(rest.iter().chain([&(last + 1)]))
                        .with_epoch(specifier.version().epoch())
                        .with_dev(Some(0)),
                );
                let version = PubGrubVersion::from(specifier.version().clone());
                Range::from_range_bounds(version..upper)
            }
            Operator::LessThan => {
                let version = PubGrubVersion::from(specifier.version().clone());
                Range::strictly_lower_than(version)
            }
            Operator::LessThanEqual => {
                let version = PubGrubVersion::from(specifier.version().clone());
                Range::lower_than(version)
            }
            Operator::GreaterThan => {
                // Per PEP 440: "The exclusive ordered comparison >V MUST NOT allow a post-release of
                // the given version unless V itself is a post release."
                let version = PubGrubVersion::from(specifier.version().clone());
                Range::strictly_higher_than(version)
            }
            Operator::GreaterThanEqual => {
                let version = PubGrubVersion::from(specifier.version().clone());
                Range::higher_than(version)
            }
            Operator::EqualStar => {
                let low = specifier.version().clone().with_dev(Some(0));
                let mut high = low.clone();
                if let Some(post) = high.post() {
                    high = high.with_post(Some(post + 1));
                } else if let Some(pre) = high.pre() {
                    high = high.with_pre(Some(match pre {
                        (pep440_rs::PreRelease::Rc, n) => (pep440_rs::PreRelease::Rc, n + 1),
                        (pep440_rs::PreRelease::Alpha, n) => (pep440_rs::PreRelease::Alpha, n + 1),
                        (pep440_rs::PreRelease::Beta, n) => (pep440_rs::PreRelease::Beta, n + 1),
                    }));
                } else {
                    let mut release = high.release().to_vec();
                    *release.last_mut().unwrap() += 1;
                    high = high.with_release(release);
                }
                Range::from_range_bounds(PubGrubVersion::from(low)..PubGrubVersion::from(high))
            }
            Operator::NotEqualStar => {
                let low = specifier.version().clone().with_dev(Some(0));
                let mut high = low.clone();
                if let Some(post) = high.post() {
                    high = high.with_post(Some(post + 1));
                } else if let Some(pre) = high.pre() {
                    high = high.with_pre(Some(match pre {
                        (pep440_rs::PreRelease::Rc, n) => (pep440_rs::PreRelease::Rc, n + 1),
                        (pep440_rs::PreRelease::Alpha, n) => (pep440_rs::PreRelease::Alpha, n + 1),
                        (pep440_rs::PreRelease::Beta, n) => (pep440_rs::PreRelease::Beta, n + 1),
                    }));
                } else {
                    let mut release = high.release().to_vec();
                    *release.last_mut().unwrap() += 1;
                    high = high.with_release(release);
                }
                Range::from_range_bounds(PubGrubVersion::from(low)..PubGrubVersion::from(high))
                    .complement()
            }
        };

        Ok(Self(ranges))
    }
}
