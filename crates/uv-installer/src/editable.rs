use distribution_types::{
    CachedDist, InstalledDist, InstalledMetadata, InstalledVersion, LocalEditable, Name,
};
use pypi_types::Metadata21;
use uv_normalize::PackageName;

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

impl Name for BuiltEditable {
    fn name(&self) -> &PackageName {
        &self.metadata.name
    }
}

impl Name for ResolvedEditable {
    fn name(&self) -> &PackageName {
        match self {
            Self::Installed(dist) => dist.name(),
            Self::Built(dist) => dist.name(),
        }
    }
}

impl InstalledMetadata for BuiltEditable {
    fn installed_version(&self) -> InstalledVersion {
        self.wheel.installed_version()
    }
}

impl InstalledMetadata for ResolvedEditable {
    fn installed_version(&self) -> InstalledVersion {
        match self {
            Self::Installed(dist) => dist.installed_version(),
            Self::Built(dist) => dist.installed_version(),
        }
    }
}

impl std::fmt::Display for BuiltEditable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.installed_version())
    }
}

impl std::fmt::Display for ResolvedEditable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.installed_version())
    }
}
