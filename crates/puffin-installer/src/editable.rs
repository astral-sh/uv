use distribution_types::{CachedDist, InstalledDist, LocalEditable, Metadata, VersionOrUrl};
use puffin_normalize::PackageName;
use pypi_types::Metadata21;

/// An editable distribution that has been built.
#[derive(Debug, Clone)]
pub struct BuiltEditable {
    pub editable: LocalEditable,
    pub wheel: CachedDist,
    pub metadata: Metadata21,
}

/// An editable distribution that has been resolved to a concrete distribution.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum ResolvedEditable {
    /// The editable is already installed in the environment.
    Installed(InstalledDist),
    /// The editable has been built and is ready to be installed.
    Built(BuiltEditable),
}

impl Metadata for BuiltEditable {
    fn name(&self) -> &PackageName {
        &self.metadata.name
    }

    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Version(&self.metadata.version)
    }
}

impl Metadata for ResolvedEditable {
    fn name(&self) -> &PackageName {
        match self {
            Self::Installed(dist) => dist.name(),
            Self::Built(dist) => dist.name(),
        }
    }

    fn version_or_url(&self) -> VersionOrUrl {
        match self {
            Self::Installed(dist) => dist.version_or_url(),
            Self::Built(dist) => dist.version_or_url(),
        }
    }
}

impl std::fmt::Display for BuiltEditable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for ResolvedEditable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}
