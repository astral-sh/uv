use distribution_types::{DistributionMetadata, Name, VersionOrUrl};
use pep440_rs::Version;
use pep508_rs::VerbatimUrl;
use uv_normalize::PackageName;

#[derive(Debug)]
pub(crate) enum PubGrubDistribution<'a> {
    Registry(&'a PackageName, &'a Version),
    Url(&'a PackageName, &'a VerbatimUrl),
}

impl<'a> PubGrubDistribution<'a> {
    pub(crate) fn from_registry(name: &'a PackageName, version: &'a Version) -> Self {
        Self::Registry(name, version)
    }

    pub(crate) fn from_url(name: &'a PackageName, url: &'a VerbatimUrl) -> Self {
        Self::Url(name, url)
    }
}

impl Name for PubGrubDistribution<'_> {
    fn name(&self) -> &PackageName {
        match self {
            Self::Registry(name, _) => name,
            Self::Url(name, _) => name,
        }
    }
}

impl DistributionMetadata for PubGrubDistribution<'_> {
    fn version_or_url(&self) -> VersionOrUrl {
        match self {
            Self::Registry(_, version) => VersionOrUrl::Version(version),
            Self::Url(_, url) => VersionOrUrl::Url(url),
        }
    }
}
