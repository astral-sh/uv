use anyhow::Result;

use pep440_rs::{Operator, VersionSpecifier};
use std::ops::Range;

use crate::facade::{PubGrubVersion, VERSION_INFINITY, VERSION_ZERO};

/// Convert a PEP 508 specifier to a `PubGrub` range.
pub(crate) fn to_ranges(specifier: &VersionSpecifier) -> Result<Vec<Range<PubGrubVersion>>> {
    match specifier.operator() {
        Operator::Equal => {
            let version = PubGrubVersion::from(specifier.version().clone());
            Ok(vec![version.clone()..version.next()])
        }
        Operator::ExactEqual => {
            let version = PubGrubVersion::from(specifier.version().clone());
            Ok(vec![version.clone()..version.next()])
        }
        Operator::NotEqual => {
            let version = PubGrubVersion::from(specifier.version().clone());
            Ok(vec![
                VERSION_ZERO.clone()..version.clone(),
                version.next()..VERSION_INFINITY.clone(),
            ])
        }
        Operator::TildeEqual => {
            let [rest @ .., last, _] = specifier.version().release.as_slice() else {
                return Err(anyhow::anyhow!(
                    "~= operator requires at least two release segments"
                ));
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
            let lower = PubGrubVersion::from(specifier.version().clone());
            Ok(vec![lower..upper])
        }
        Operator::LessThan => {
            // Per PEP 440: "The exclusive ordered comparison <V MUST NOT allow a pre-release of
            // the specified version unless the specified version is itself a pre-release."
            if specifier.version().any_prerelease() {
                let version = PubGrubVersion::from(specifier.version().clone());
                Ok(vec![VERSION_ZERO.clone()..version.clone()])
            } else {
                let max_version = pep440_rs::Version {
                    post: None,
                    dev: Some(0),
                    local: None,
                    ..specifier.version().clone()
                };
                let version = PubGrubVersion::from(max_version);
                Ok(vec![VERSION_ZERO.clone()..version.clone()])
            }
        }
        Operator::LessThanEqual => {
            let version = PubGrubVersion::from(specifier.version().clone());
            Ok(vec![VERSION_ZERO.clone()..version.next()])
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

            let version = PubGrubVersion::from(low);
            Ok(vec![version..VERSION_INFINITY.clone()])
        }
        Operator::GreaterThanEqual => {
            let version = PubGrubVersion::from(specifier.version().clone());
            Ok(vec![version..VERSION_INFINITY.clone()])
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

            Ok(vec![PubGrubVersion::from(low)..PubGrubVersion::from(high)])
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

            Ok(vec![
                VERSION_ZERO.clone()..PubGrubVersion::from(low),
                PubGrubVersion::from(high)..VERSION_INFINITY.clone(),
            ])
        }
    }
}
