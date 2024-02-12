use pep440_rs::VersionSpecifiers;
use platform_tags::TagPriority;
use pypi_types::{Hashes, Yanked};

use crate::Dist;

/// Attach its requires-python to a [`Dist`], since downstream needs this information to filter
/// [`PrioritizedDistribution`].
#[derive(Debug, Clone)]
pub struct DistRequiresPython {
    pub dist: Dist,
    pub requires_python: Option<VersionSpecifiers>,
}

// Boxed because `Dist` is large.
#[derive(Debug, Default, Clone)]
pub struct PrioritizedDistribution(Box<PrioritizedDistributionInner>);

#[derive(Debug, Default, Clone)]
struct PrioritizedDistributionInner {
    /// An arbitrary source distribution for the package version.
    source: Option<DistRequiresPython>,
    /// The highest-priority, platform-compatible wheel for the package version.
    compatible_wheel: Option<(DistRequiresPython, TagPriority)>,
    /// An arbitrary, platform-incompatible wheel for the package version.
    incompatible_wheel: Option<DistRequiresPython>,
    /// Is the distribution yanked
    yanked: Yanked,
    /// The hashes for each distribution.
    hashes: Vec<Hashes>,
}

impl PrioritizedDistribution {
    /// Create a new [`PrioritizedDistribution`] from the given wheel distribution.
    pub fn from_built(
        dist: Dist,
        requires_python: Option<VersionSpecifiers>,
        hash: Option<Hashes>,
        priority: Option<TagPriority>,
        yanked: Yanked,
    ) -> Self {
        if let Some(priority) = priority {
            Self(Box::new(PrioritizedDistributionInner {
                source: None,
                compatible_wheel: Some((
                    DistRequiresPython {
                        dist,
                        requires_python,
                    },
                    priority,
                )),
                incompatible_wheel: None,
                hashes: hash.map(|hash| vec![hash]).unwrap_or_default(),
                yanked,
            }))
        } else {
            Self(Box::new(PrioritizedDistributionInner {
                source: None,
                compatible_wheel: None,
                incompatible_wheel: Some(DistRequiresPython {
                    dist,
                    requires_python,
                }),
                hashes: hash.map(|hash| vec![hash]).unwrap_or_default(),
                yanked,
            }))
        }
    }

    /// Create a new [`PrioritizedDistribution`] from the given source distribution.
    pub fn from_source(
        dist: Dist,
        requires_python: Option<VersionSpecifiers>,
        hash: Option<Hashes>,
        yanked: Yanked,
    ) -> Self {
        Self(Box::new(PrioritizedDistributionInner {
            source: Some(DistRequiresPython {
                dist,
                requires_python,
            }),
            compatible_wheel: None,
            incompatible_wheel: None,
            hashes: hash.map(|hash| vec![hash]).unwrap_or_default(),
            yanked,
        }))
    }

    /// Insert the given built distribution into the [`PrioritizedDistribution`].
    pub fn insert_built(
        &mut self,
        dist: Dist,
        requires_python: Option<VersionSpecifiers>,
        hash: Option<Hashes>,
        priority: Option<TagPriority>,
        yanked: Yanked,
    ) {
        if yanked.is_yanked() {
            self.0.yanked = yanked;
        }

        // Prefer the highest-priority, platform-compatible wheel.
        if let Some(priority) = priority {
            if let Some((.., existing_priority)) = &self.0.compatible_wheel {
                if priority > *existing_priority {
                    self.0.compatible_wheel = Some((
                        DistRequiresPython {
                            dist,
                            requires_python,
                        },
                        priority,
                    ));
                }
            } else {
                self.0.compatible_wheel = Some((
                    DistRequiresPython {
                        dist,
                        requires_python,
                    },
                    priority,
                ));
            }
        } else if self.0.incompatible_wheel.is_none() {
            self.0.incompatible_wheel = Some(DistRequiresPython {
                dist,
                requires_python,
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
        hash: Option<Hashes>,
        yanked: Yanked,
    ) {
        if yanked.is_yanked() {
            self.0.yanked = yanked;
        }

        if self.0.source.is_none() {
            self.0.source = Some(DistRequiresPython {
                dist,
                requires_python,
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
            (Some((wheel, tag_priority)), _, _) => Some(ResolvableDist::CompatibleWheel(
                wheel,
                &self.0.yanked,
                *tag_priority,
            )),
            // If we have a compatible source distribution and an incompatible wheel, return the
            // wheel. We assume that all distributions have the same metadata for a given package
            // version. If a compatible source distribution exists, we assume we can build it, but
            // using the wheel is faster.
            (_, Some(source_dist), Some(wheel)) => Some(ResolvableDist::IncompatibleWheel {
                source_dist,
                yanked: &self.0.yanked,
                wheel,
            }),
            // Otherwise, if we have a source distribution, return it.
            (_, Some(source_dist), _) => {
                Some(ResolvableDist::SourceDist(source_dist, &self.0.yanked))
            }
            _ => None,
        }
    }

    /// Return the source distribution, if any.
    pub fn source(&self) -> Option<&DistRequiresPython> {
        self.0.source.as_ref()
    }

    /// Return the compatible built distribution, if any.
    pub fn compatible_wheel(&self) -> Option<&(DistRequiresPython, TagPriority)> {
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

#[derive(Debug, Clone)]
pub enum ResolvableDist<'a> {
    /// The distribution should be resolved and installed using a source distribution.
    SourceDist(&'a DistRequiresPython, &'a Yanked),
    /// The distribution should be resolved and installed using a wheel distribution.
    CompatibleWheel(&'a DistRequiresPython, &'a Yanked, TagPriority),
    /// The distribution should be resolved using an incompatible wheel distribution, but
    /// installed using a source distribution.
    IncompatibleWheel {
        source_dist: &'a DistRequiresPython,
        yanked: &'a Yanked,
        wheel: &'a DistRequiresPython,
    },
}

impl<'a> ResolvableDist<'a> {
    /// Return the [`DistRequiresPython`] to use during resolution.
    pub fn resolution_python(&self) -> &DistRequiresPython {
        match *self {
            ResolvableDist::SourceDist(sdist, _) => sdist,
            ResolvableDist::CompatibleWheel(wheel, _, _) => wheel,
            ResolvableDist::IncompatibleWheel {
                source_dist: _,
                yanked: _,
                wheel,
            } => wheel,
        }
    }

    /// Return the [`DistRequiresPython`] to use during installation.
    pub fn installation_python(&self) -> &DistRequiresPython {
        match *self {
            ResolvableDist::SourceDist(sdist, _) => sdist,
            ResolvableDist::CompatibleWheel(wheel, _, _) => wheel,
            ResolvableDist::IncompatibleWheel {
                source_dist,
                yanked: _,
                wheel: _,
            } => source_dist,
        }
    }

    pub fn yanked(&self) -> &Yanked {
        match *self {
            ResolvableDist::SourceDist(_, yanked) => yanked,
            ResolvableDist::CompatibleWheel(_, yanked, _) => yanked,
            ResolvableDist::IncompatibleWheel {
                source_dist: _,
                yanked,
                wheel: _,
            } => yanked,
        }
    }
}
