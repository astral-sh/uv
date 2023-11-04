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
                let [rest @ .., last, _] = specifier.version().release.as_slice() else {
                    return Err(ResolveError::InvalidTildeEquals(specifier.clone()));
                };
                let upper = PubGrubVersion::from(pep440_rs::Version {
                    dev: Some(0),
                    epoch: specifier.version().epoch,
                    local: None,
                    post: None,
                    pre: None,
                    release: rest
                        .iter()
                        .chain(std::iter::once(&(last + 1)))
                        .copied()
                        .collect(),
                });
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
                let mut low = specifier.version().clone();
                if let Some(dev) = low.dev {
                    low.dev = Some(dev + 1);
                } else if let Some(post) = low.post {
                    low.post = Some(post + 1);
                } else {
                    low.post = Some(usize::MAX);
                }
                let version = PubGrubVersion::from(specifier.version().clone());
                Range::strictly_higher_than(version)
            }
            Operator::GreaterThanEqual => {
                let version = PubGrubVersion::from(specifier.version().clone());
                Range::higher_than(version)
            }
            Operator::EqualStar => {
                let low = pep440_rs::Version {
                    dev: Some(0),
                    ..specifier.version().clone()
                };
                let mut high = pep440_rs::Version {
                    dev: Some(0),
                    ..specifier.version().clone()
                };
                if let Some(post) = high.post {
                    high.post = Some(post + 1);
                } else if let Some(pre) = high.pre {
                    high.pre = Some(match pre {
                        (pep440_rs::PreRelease::Rc, n) => (pep440_rs::PreRelease::Rc, n + 1),
                        (pep440_rs::PreRelease::Alpha, n) => (pep440_rs::PreRelease::Alpha, n + 1),
                        (pep440_rs::PreRelease::Beta, n) => (pep440_rs::PreRelease::Beta, n + 1),
                    });
                } else {
                    *high.release.last_mut().unwrap() += 1;
                }
                Range::from_range_bounds(PubGrubVersion::from(low)..PubGrubVersion::from(high))
            }
            Operator::NotEqualStar => {
                let low = pep440_rs::Version {
                    dev: Some(0),
                    ..specifier.version().clone()
                };
                let mut high = pep440_rs::Version {
                    dev: Some(0),
                    ..specifier.version().clone()
                };
                if let Some(post) = high.post {
                    high.post = Some(post + 1);
                } else if let Some(pre) = high.pre {
                    high.pre = Some(match pre {
                        (pep440_rs::PreRelease::Rc, n) => (pep440_rs::PreRelease::Rc, n + 1),
                        (pep440_rs::PreRelease::Alpha, n) => (pep440_rs::PreRelease::Alpha, n + 1),
                        (pep440_rs::PreRelease::Beta, n) => (pep440_rs::PreRelease::Beta, n + 1),
                    });
                } else {
                    *high.release.last_mut().unwrap() += 1;
                }
                Range::from_range_bounds(PubGrubVersion::from(low)..PubGrubVersion::from(high))
                    .complement()
            }
        };

        Ok(Self(ranges))
    }
}
