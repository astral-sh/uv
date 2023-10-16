use std::ops::Range;

use anyhow::Result;

use pep440_rs::{Operator, VersionSpecifier};

use crate::pubgrub::version::{PubGrubVersion, MAX_VERSION, MIN_VERSION};

/// A range of versions that can be used to satisfy a requirement.
#[derive(Debug)]
pub(crate) struct PubGrubSpecifier(Vec<Range<PubGrubVersion>>);

impl IntoIterator for PubGrubSpecifier {
    type Item = Range<PubGrubVersion>;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl TryFrom<&VersionSpecifier> for PubGrubSpecifier {
    type Error = anyhow::Error;

    /// Convert a PEP 508 specifier to a `PubGrub`-compatible version range.
    fn try_from(specifier: &VersionSpecifier) -> Result<Self> {
        let ranges = match specifier.operator() {
            Operator::Equal => {
                let version = PubGrubVersion::from(specifier.version().clone());
                vec![version.clone()..version.next()]
            }
            Operator::ExactEqual => {
                let version = PubGrubVersion::from(specifier.version().clone());
                vec![version.clone()..version.next()]
            }
            Operator::NotEqual => {
                let version = PubGrubVersion::from(specifier.version().clone());
                vec![
                    MIN_VERSION.clone()..version.clone(),
                    version.next()..MAX_VERSION.clone(),
                ]
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
                vec![lower..upper]
            }
            Operator::LessThan => {
                // Per PEP 440: "The exclusive ordered comparison <V MUST NOT allow a pre-release of
                // the specified version unless the specified version is itself a pre-release."
                if specifier.version().any_prerelease() {
                    let version = PubGrubVersion::from(specifier.version().clone());
                    vec![MIN_VERSION.clone()..version.clone()]
                } else {
                    let max_version = pep440_rs::Version {
                        post: None,
                        dev: Some(0),
                        local: None,
                        ..specifier.version().clone()
                    };
                    let version = PubGrubVersion::from(max_version);
                    vec![MIN_VERSION.clone()..version.clone()]
                }
            }
            Operator::LessThanEqual => {
                let version = PubGrubVersion::from(specifier.version().clone());
                vec![MIN_VERSION.clone()..version.next()]
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
                vec![version..MAX_VERSION.clone()]
            }
            Operator::GreaterThanEqual => {
                let version = PubGrubVersion::from(specifier.version().clone());
                vec![version..MAX_VERSION.clone()]
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

                vec![PubGrubVersion::from(low)..PubGrubVersion::from(high)]
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

                vec![
                    MIN_VERSION.clone()..PubGrubVersion::from(low),
                    PubGrubVersion::from(high)..MAX_VERSION.clone(),
                ]
            }
        };

        Ok(Self(ranges))
    }
}
