use uv_distribution_types::{DistributionMetadata, Name, VersionOrUrlRef};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pypi_types::VerbatimParsedUrl;

#[derive(Debug)]
pub(crate) enum PubGrubDistribution<'a> {
    Registry(&'a PackageName, &'a Version),
    Url(&'a PackageName, &'a VerbatimParsedUrl),
}

impl<'a> PubGrubDistribution<'a> {
    pub(crate) fn from_registry(name: &'a PackageName, version: &'a Version) -> Self {
        Self::Registry(name, version)
    }

    pub(crate) fn from_url(name: &'a PackageName, url: &'a VerbatimParsedUrl) -> Self {
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
    fn version_or_url(&self) -> VersionOrUrlRef {
        match self {
            Self::Registry(_, version) => VersionOrUrlRef::Version(version),
            Self::Url(_, url) => VersionOrUrlRef::Url(&url.verbatim),
        }
    }
}
