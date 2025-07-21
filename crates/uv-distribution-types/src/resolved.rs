use std::fmt::{Display, Formatter};
use std::path::Path;
use std::sync::Arc;

use uv_distribution_filename::WheelFilename;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pypi_types::Yanked;

use crate::{
    BuiltDist, Dist, DistributionId, DistributionMetadata, Identifier, IndexUrl, InstalledDist,
    Name, PrioritizedDist, RegistryBuiltWheel, RegistrySourceDist, RegistryVariantsJson,
    ResourceId, SourceDist, VersionOrUrlRef,
};

/// A distribution that can be used for resolution and installation.
///
/// Either an already-installed distribution or a distribution that can be installed.
#[derive(Debug, Clone, Hash)]
#[allow(clippy::large_enum_variant)]
pub enum ResolvedDist {
    Installed {
        dist: Arc<InstalledDist>,
    },
    Installable {
        dist: Arc<Dist>,
        variants_json: Option<Arc<RegistryVariantsJson>>,
        version: Option<Version>,
    },
}

/// A variant of [`ResolvedDist`] with borrowed inner distributions.
#[derive(Debug, Clone)]
pub enum ResolvedDistRef<'a> {
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
            Self::Installable { version, dist, .. } => dist.version().or(version.as_ref()),
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

    // TODO(konsti): That's the wrong "abstraction"
    pub fn wheel_filename(&self) -> Option<&WheelFilename> {
        match self {
            Self::Installed { .. } => {
                // TODO(konsti): Support installed dists too
                None
            }
            Self::Installable { dist, .. } => match &**dist {
                Dist::Built(dist) => Some(dist.wheel_filename()),
                Dist::Source(_) => None,
            },
        }
    }
}

impl ResolvedDistRef<'_> {
    pub fn to_owned(&self) -> ResolvedDist {
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
                    variants_json: prioritized
                        .variants_json()
                        .map(|variants_json| Arc::new(variants_json.clone())),
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
                    variants_json: prioritized
                        .variants_json()
                        .map(|variants_json| Arc::new(variants_json.clone())),
                    version: Some(wheel.filename.version.clone()),
                }
            }
            Self::Installed { dist } => ResolvedDist::Installed {
                dist: Arc::new((*dist).clone()),
            },
        }
    }

    /// Returns the [`IndexUrl`], if the distribution is from a registry.
    pub fn index(&self) -> Option<&IndexUrl> {
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
