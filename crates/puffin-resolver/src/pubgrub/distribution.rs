use distribution_types::{Metadata, VersionOrUrl};
use pep508_rs::VerbatimUrl;
use puffin_normalize::PackageName;

use crate::pubgrub::PubGrubVersion;

#[derive(Debug)]
pub(crate) enum PubGrubDistribution<'a> {
    Registry(&'a PackageName, &'a PubGrubVersion),
    Url(&'a PackageName, &'a VerbatimUrl),
}

impl<'a> PubGrubDistribution<'a> {
    pub(crate) fn from_registry(name: &'a PackageName, version: &'a PubGrubVersion) -> Self {
        Self::Registry(name, version)
    }

    pub(crate) fn from_url(name: &'a PackageName, url: &'a VerbatimUrl) -> Self {
        Self::Url(name, url)
    }
}

impl Metadata for PubGrubDistribution<'_> {
    fn name(&self) -> &PackageName {
        match self {
            Self::Registry(name, _) => name,
            Self::Url(name, _) => name,
        }
    }

    fn version_or_url(&self) -> VersionOrUrl {
        match self {
            Self::Registry(_, version) => VersionOrUrl::Version((*version).into()),
            Self::Url(_, url) => VersionOrUrl::Url(url),
        }
    }
}
