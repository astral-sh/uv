use pep440_rs::VersionSpecifiers;
use platform_tags::TagPriority;
use pypi_types::{Hashes, Yanked};

use crate::Dist;

/// A [`Dist`] and metadata about it required for downstream filtering.
#[derive(Debug, Clone)]
pub struct DistMetadata {
    pub dist: Dist,

    /// The version of Python required by the distribution
    pub requires_python: Option<VersionSpecifiers>,

    /// Is the distribution file yanked?
    pub yanked: Yanked,
}

// Boxed because `Dist` is large.
#[derive(Debug, Default, Clone)]
pub struct PrioritizedDistribution(Box<PrioritizedDistributionInner>);

#[derive(Debug, Default, Clone)]
struct PrioritizedDistributionInner {
    /// An arbitrary source distribution for the package version.
    source: Option<DistMetadata>,
    /// The highest-priority, platform-compatible wheel for the package version.
    compatible_wheel: Option<(DistMetadata, TagPriority)>,
    /// An arbitrary, platform-incompatible wheel for the package version.
    incompatible_wheel: Option<DistMetadata>,
    /// The hashes for each distribution.
    hashes: Vec<Hashes>,
}

impl PrioritizedDistribution {
    /// Create a new [`PrioritizedDistribution`] from the given wheel distribution.
    pub fn from_built(
        dist: Dist,
        requires_python: Option<VersionSpecifiers>,
        yanked: Yanked,
        hash: Option<Hashes>,
        priority: Option<TagPriority>,
    ) -> Self {
        if let Some(priority) = priority {
            Self(Box::new(PrioritizedDistributionInner {
                source: None,
                compatible_wheel: Some((
                    DistMetadata {
                        dist,
                        requires_python,
                        yanked,
                    },
                    priority,
                )),
                incompatible_wheel: None,
                hashes: hash.map(|hash| vec![hash]).unwrap_or_default(),
            }))
        } else {
            Self(Box::new(PrioritizedDistributionInner {
                source: None,
                compatible_wheel: None,
                incompatible_wheel: Some(DistMetadata {
                    dist,
                    requires_python,
                    yanked,
                }),
                hashes: hash.map(|hash| vec![hash]).unwrap_or_default(),
            }))
        }
    }

    /// Create a new [`PrioritizedDistribution`] from the given source distribution.
    pub fn from_source(
        dist: Dist,
        requires_python: Option<VersionSpecifiers>,
        yanked: Yanked,
        hash: Option<Hashes>,
    ) -> Self {
        Self(Box::new(PrioritizedDistributionInner {
            source: Some(DistMetadata {
                dist,
                requires_python,
                yanked,
            }),
            compatible_wheel: None,
            incompatible_wheel: None,
            hashes: hash.map(|hash| vec![hash]).unwrap_or_default(),
        }))
    }

    /// Insert the given built distribution into the [`PrioritizedDistribution`].
    pub fn insert_built(
        &mut self,
        dist: Dist,
        requires_python: Option<VersionSpecifiers>,
        yanked: Yanked,
        hash: Option<Hashes>,
        priority: Option<TagPriority>,
    ) {
        // Prefer the highest-priority, platform-compatible wheel.
        if let Some(priority) = priority {
            if let Some((.., existing_priority)) = &self.0.compatible_wheel {
                if priority > *existing_priority {
                    self.0.compatible_wheel = Some((
                        DistMetadata {
                            dist,
                            requires_python,
                            yanked,
                        },
                        priority,
                    ));
                }
            } else {
                self.0.compatible_wheel = Some((
                    DistMetadata {
                        dist,
                        requires_python,
                        yanked,
                    },
                    priority,
                ));
            }
        } else if self.0.incompatible_wheel.is_none() {
            self.0.incompatible_wheel = Some(DistMetadata {
                dist,
                requires_python,
                yanked,
            });
        }

        if let Some(hash) = hash {
            self.0.hashes.push(hash);
        }
    }

    /// Insert the given source distribution into the [`PrioritizedDistribution`].
    pub fn insert_source(
        &mut self,
        dist: Dist,
        requires_python: Option<VersionSpecifiers>,
        yanked: Yanked,
        hash: Option<Hashes>,
    ) {
        if self.0.source.is_none() {
            self.0.source = Some(DistMetadata {
                dist,
                requires_python,
                yanked,
            });
        }

        if let Some(hash) = hash {
            self.0.hashes.push(hash);
        }
    }

    /// Return the highest-priority distribution for the package version, if any.
    pub fn get(&self) -> Option<ResolvableDist> {
        match (
            &self.0.compatible_wheel,
            &self.0.source,
            &self.0.incompatible_wheel,
        ) {
            // Prefer the highest-priority, platform-compatible wheel.
            (Some((wheel, tag_priority)), _, _) => {
                Some(ResolvableDist::CompatibleWheel(wheel, *tag_priority))
            }
            // If we have a compatible source distribution and an incompatible wheel, return the
            // wheel. We assume that all distributions have the same metadata for a given package
            // version. If a compatible source distribution exists, we assume we can build it, but
            // using the wheel is faster.
            (_, Some(source_dist), Some(wheel)) => {
                Some(ResolvableDist::IncompatibleWheel { source_dist, wheel })
            }
            // Otherwise, if we have a source distribution, return it.
            (_, Some(source_dist), _) => Some(ResolvableDist::SourceDist(source_dist)),
            _ => None,
        }
    }

    /// Return the source distribution, if any.
    pub fn source(&self) -> Option<&DistMetadata> {
        self.0.source.as_ref()
    }

    /// Return the compatible built distribution, if any.
    pub fn compatible_wheel(&self) -> Option<&(DistMetadata, TagPriority)> {
        self.0.compatible_wheel.as_ref()
    }

    /// Return the hashes for each distribution.
    pub fn hashes(&self) -> &[Hashes] {
        &self.0.hashes
    }

    /// Returns true if and only if this distribution does not contain any
    /// source distributions or wheels.
    pub fn is_empty(&self) -> bool {
        self.0.source.is_none()
            && self.0.compatible_wheel.is_none()
            && self.0.incompatible_wheel.is_none()
    }
}

/// A collection of distributions ([`Dist`]) that can be used for resolution and installation.
#[derive(Debug, Clone)]
pub enum ResolvableDist<'a> {
    /// The distribution should be resolved and installed using a source distribution.
    SourceDist(&'a DistMetadata),
    /// The distribution should be resolved and installed using a wheel distribution.
    CompatibleWheel(&'a DistMetadata, TagPriority),
    /// The distribution should be resolved using an incompatible wheel distribution, but
    /// installed using a source distribution.
    IncompatibleWheel {
        source_dist: &'a DistMetadata,
        wheel: &'a DistMetadata,
    },
}

impl<'a> ResolvableDist<'a> {
    /// Return the [`DistMetadata`] to use during resolution.
    pub fn for_resolution(&self) -> &DistMetadata {
        match *self {
            ResolvableDist::SourceDist(sdist) => sdist,
            ResolvableDist::CompatibleWheel(wheel, _) => wheel,
            ResolvableDist::IncompatibleWheel {
                source_dist: _,
                wheel,
            } => wheel,
        }
    }

    /// Return the [`DistMetadata`] to use during installation.
    pub fn for_installation(&self) -> &DistMetadata {
        match *self {
            ResolvableDist::SourceDist(sdist) => sdist,
            ResolvableDist::CompatibleWheel(wheel, _) => wheel,
            ResolvableDist::IncompatibleWheel {
                source_dist,
                wheel: _,
            } => source_dist,
        }
    }

    /// Return the [`Yanked`] status of the distribution.
    ///
    /// It is possible for files to have a different yank status per PEP 592 but in the official
    /// PyPI warehouse this cannot happen.
    ///
    /// Here, we will treat the distribution is yanked if the file we will install with
    /// is yanked.
    ///
    /// PEP 592: <https://peps.python.org/pep-0592/#warehouse-pypi-implementation-notes>
    pub fn yanked(&self) -> &Yanked {
        &self.for_installation().yanked
    }
}
