use std::str::FromStr;

use once_cell::sync::Lazy;

/// A PubGrub-compatible wrapper around a PEP 440 version.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PubGrubVersion(pep440_rs::Version);

impl PubGrubVersion {
    /// Returns the smallest PEP 440 version that is larger than `self`.
    pub fn next(&self) -> PubGrubVersion {
        let mut next = self.clone();
        if let Some(dev) = next.0.dev() {
            next.0 = next.0.with_dev(Some(dev + 1));
        } else if let Some(post) = next.0.post() {
            next.0 = next.0.with_post(Some(post + 1));
        } else {
            next.0 = next.0.with_post(Some(0)).with_dev(Some(0));
        }
        next
    }

    pub fn any_prerelease(&self) -> bool {
        self.0.any_prerelease()
    }
}

impl std::fmt::Display for PubGrubVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Debug for PubGrubVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl pubgrub::version::Version for PubGrubVersion {
    fn lowest() -> Self {
        MIN_VERSION.to_owned()
    }

    fn bump(&self) -> Self {
        self.next()
    }
}

impl<'a> From<&'a PubGrubVersion> for &'a pep440_rs::Version {
    fn from(version: &'a PubGrubVersion) -> Self {
        &version.0
    }
}

impl From<pep440_rs::Version> for PubGrubVersion {
    fn from(version: pep440_rs::Version) -> Self {
        Self(version)
    }
}

impl From<PubGrubVersion> for pep440_rs::Version {
    fn from(version: PubGrubVersion) -> Self {
        version.0
    }
}

pub(crate) static MIN_VERSION: Lazy<PubGrubVersion> =
    Lazy::new(|| PubGrubVersion::from(pep440_rs::Version::from_str("0a0.dev0").unwrap()));
