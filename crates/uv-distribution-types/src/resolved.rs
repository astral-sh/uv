use std::fmt::{Display, Formatter};
use std::path::Path;
use std::sync::Arc;

use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pypi_types::Yanked;

use crate::{
    BuiltDist, Dist, DistributionId, DistributionMetadata, Identifier, IndexUrl, InstalledDist,
    Name, ResourceId, SourceDist, VersionId, VersionOrUrlRef,
};

/// A distribution that can be used for resolution and installation.
///
/// Either an already-installed distribution or a distribution that can be installed.
#[derive(Debug, Clone, Hash)]
pub enum ResolvedDist {
    Installed {
        dist: Arc<InstalledDist>,
    },
    Installable {
        dist: Arc<Dist>,
        version: Option<Version>,
    },
}

impl ResolvedDist {
    /// Return true if the distribution is editable.
    pub fn is_editable(&self) -> bool {
        match self {
            Self::Installable { dist, .. } => dist.is_editable(),
            Self::Installed { dist } => dist.is_editable(),
        }
    }

    /// Return true if the distribution refers to a local file or directory.
    pub fn is_local(&self) -> bool {
        match self {
            Self::Installable { dist, .. } => dist.is_local(),
            Self::Installed { dist } => dist.is_local(),
        }
    }

    /// Returns the [`IndexUrl`], if the distribution is from a registry.
    pub fn index(&self) -> Option<&IndexUrl> {
        match self {
            Self::Installable { dist, .. } => dist.index(),
            Self::Installed { .. } => None,
        }
    }

    /// Returns the [`Yanked`] status of the distribution, if available.
    pub fn yanked(&self) -> Option<&Yanked> {
        match self {
            Self::Installable { dist, .. } => match dist.as_ref() {
                Dist::Source(SourceDist::Registry(sdist)) => sdist.file.yanked.as_deref(),
                Dist::Built(BuiltDist::Registry(wheel)) => {
                    wheel.best_wheel().file.yanked.as_deref()
                }
                _ => None,
            },
            Self::Installed { .. } => None,
        }
    }

    /// Returns the version of the distribution, if available.
    pub fn version(&self) -> Option<&Version> {
        match self {
            Self::Installable { version, dist } => dist.version().or(version.as_ref()),
            Self::Installed { dist } => Some(dist.version()),
        }
    }

    /// Return the source tree of the distribution, if available.
    pub fn source_tree(&self) -> Option<&Path> {
        match self {
            Self::Installable { dist, .. } => dist.source_tree(),
            Self::Installed { .. } => None,
        }
    }
}

impl Name for ResolvedDist {
    fn name(&self) -> &PackageName {
        match self {
            Self::Installable { dist, .. } => dist.name(),
            Self::Installed { dist } => dist.name(),
        }
    }
}

impl DistributionMetadata for ResolvedDist {
    fn version_or_url(&self) -> VersionOrUrlRef<'_> {
        match self {
            Self::Installed { dist } => dist.version_or_url(),
            Self::Installable { dist, .. } => dist.version_or_url(),
        }
    }

    fn version_id(&self) -> VersionId {
        match self {
            Self::Installed { dist } => dist.version_id(),
            Self::Installable { dist, .. } => dist.version_id(),
        }
    }
}

impl Identifier for ResolvedDist {
    fn distribution_id(&self) -> DistributionId {
        match self {
            Self::Installed { dist } => dist.distribution_id(),
            Self::Installable { dist, .. } => dist.distribution_id(),
        }
    }

    fn resource_id(&self) -> ResourceId {
        match self {
            Self::Installed { dist } => dist.resource_id(),
            Self::Installable { dist, .. } => dist.resource_id(),
        }
    }
}

impl Display for ResolvedDist {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Installed { dist } => dist.fmt(f),
            Self::Installable { dist, .. } => dist.fmt(f),
        }
    }
}
