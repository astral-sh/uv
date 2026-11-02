use std::fmt::{Display, Formatter};
use std::sync::Arc;

use uv_distribution_types::{
    BuiltDist, Dist, DistributionId, DistributionMetadata, Identifier, IndexUrl, InstalledDist,
    Name, RegistryBuiltWheel, RegistrySourceDist, ResolvedDist, ResourceId, SourceDist,
    VersionOrUrlRef,
};
use uv_normalize::PackageName;

use crate::prioritized_distribution::PrioritizedDist;

/// A variant of [`ResolvedDist`] with borrowed inner distributions.
#[derive(Debug, Clone)]
pub(crate) enum ResolvedDistRef<'a> {
    Installed {
        dist: &'a InstalledDist,
    },
    InstallableRegistrySourceDist {
        /// The source distribution that should be used.
        sdist: &'a RegistrySourceDist,
        /// The prioritized distribution that the wheel came from.
        prioritized: &'a PrioritizedDist,
    },
    InstallableRegistryBuiltDist {
        /// The wheel that should be used.
        wheel: &'a RegistryBuiltWheel,
        /// The prioritized distribution that the wheel came from.
        prioritized: &'a PrioritizedDist,
    },
}

impl ResolvedDistRef<'_> {
    pub(crate) fn to_owned(&self) -> ResolvedDist {
        match self {
            Self::InstallableRegistrySourceDist { sdist, prioritized } => {
                // This is okay because we're only here if the prioritized dist
                // has an sdist, so this always succeeds.
                let source = prioritized.source_dist().expect("a source distribution");
                assert_eq!(
                    (&sdist.name, &sdist.version),
                    (&source.name, &source.version),
                    "expected chosen sdist to match prioritized sdist"
                );
                ResolvedDist::Installable {
                    dist: Arc::new(Dist::Source(SourceDist::Registry(source))),
                    version: Some(sdist.version.clone()),
                }
            }
            Self::InstallableRegistryBuiltDist {
                wheel, prioritized, ..
            } => {
                assert_eq!(
                    Some(&wheel.filename),
                    prioritized.best_wheel().map(|(wheel, _)| &wheel.filename),
                    "expected chosen wheel to match best wheel"
                );
                // This is okay because we're only here if the prioritized dist
                // has at least one wheel, so this always succeeds.
                let built = prioritized.built_dist().expect("at least one wheel");
                ResolvedDist::Installable {
                    dist: Arc::new(Dist::Built(BuiltDist::Registry(built))),
                    version: Some(wheel.filename.version.clone()),
                }
            }
            Self::Installed { dist } => ResolvedDist::Installed {
                dist: Arc::new((*dist).clone()),
            },
        }
    }

    /// Returns the [`IndexUrl`], if the distribution is from a registry.
    pub(crate) fn index(&self) -> Option<&IndexUrl> {
        match self {
            Self::InstallableRegistrySourceDist { sdist, .. } => Some(&sdist.index),
            Self::InstallableRegistryBuiltDist { wheel, .. } => Some(&wheel.index),
            Self::Installed { .. } => None,
        }
    }
}

impl Display for ResolvedDistRef<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InstallableRegistrySourceDist { sdist, .. } => Display::fmt(sdist, f),
            Self::InstallableRegistryBuiltDist { wheel, .. } => Display::fmt(wheel, f),
            Self::Installed { dist } => Display::fmt(dist, f),
        }
    }
}

impl Name for ResolvedDistRef<'_> {
    fn name(&self) -> &PackageName {
        match self {
            Self::InstallableRegistrySourceDist { sdist, .. } => sdist.name(),
            Self::InstallableRegistryBuiltDist { wheel, .. } => wheel.name(),
            Self::Installed { dist } => dist.name(),
        }
    }
}

impl DistributionMetadata for ResolvedDistRef<'_> {
    fn version_or_url(&self) -> VersionOrUrlRef<'_> {
        match self {
            Self::Installed { dist } => VersionOrUrlRef::Version(dist.version()),
            Self::InstallableRegistrySourceDist { sdist, .. } => sdist.version_or_url(),
            Self::InstallableRegistryBuiltDist { wheel, .. } => wheel.version_or_url(),
        }
    }
}

impl Identifier for ResolvedDistRef<'_> {
    fn distribution_id(&self) -> DistributionId {
        match self {
            Self::Installed { dist } => dist.distribution_id(),
            Self::InstallableRegistrySourceDist { sdist, .. } => sdist.distribution_id(),
            Self::InstallableRegistryBuiltDist { wheel, .. } => wheel.distribution_id(),
        }
    }

    fn resource_id(&self) -> ResourceId {
        match self {
            Self::Installed { dist } => dist.resource_id(),
            Self::InstallableRegistrySourceDist { sdist, .. } => sdist.resource_id(),
            Self::InstallableRegistryBuiltDist { wheel, .. } => wheel.resource_id(),
        }
    }
}
