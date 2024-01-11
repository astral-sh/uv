use pep440_rs::VersionSpecifiers;
use platform_tags::TagPriority;

use crate::Dist;

/// Attach its requires-python to a [`Dist`], since downstream needs this information to filter
/// [`PrioritizedDistribution`].
#[derive(Debug, Clone)]
pub struct DistRequiresPython {
    pub dist: Dist,
    pub requires_python: Option<VersionSpecifiers>,
}

#[derive(Debug, Clone)]
pub struct PrioritizedDistribution {
    /// An arbitrary source distribution for the package version.
    pub source: Option<DistRequiresPython>,
    /// The highest-priority, platform-compatible wheel for the package version.
    pub compatible_wheel: Option<(DistRequiresPython, TagPriority)>,
    /// An arbitrary, platform-incompatible wheel for the package version.
    incompatible_wheel: Option<DistRequiresPython>,
}

impl PrioritizedDistribution {
    /// Create a new [`PrioritizedDistribution`] from the given wheel distribution.
    pub fn from_built(
        dist: Dist,
        requires_python: Option<VersionSpecifiers>,
        priority: Option<TagPriority>,
    ) -> Self {
        let dist_requires_python = DistRequiresPython {
            dist,
            requires_python,
        };
        if let Some(priority) = priority {
            Self {
                source: None,
                compatible_wheel: Some((dist_requires_python, priority)),
                incompatible_wheel: None,
            }
        } else {
            Self {
                source: None,
                compatible_wheel: None,
                incompatible_wheel: Some(dist_requires_python),
            }
        }
    }

    /// Create a new [`PrioritizedDistribution`] from the given source distribution.
    pub fn from_source(dist: Dist, requires_python: Option<VersionSpecifiers>) -> Self {
        Self {
            source: Some(DistRequiresPython {
                dist,
                requires_python,
            }),
            compatible_wheel: None,
            incompatible_wheel: None,
        }
    }

    /// Insert the given built distribution into the [`PrioritizedDistribution`].
    pub fn insert_built(
        &mut self,
        dist: Dist,
        requires_python: Option<VersionSpecifiers>,
        priority: Option<TagPriority>,
    ) {
        // Prefer the highest-priority, platform-compatible wheel.
        if let Some(priority) = priority {
            if let Some((.., existing_priority)) = &self.compatible_wheel {
                if priority > *existing_priority {
                    self.compatible_wheel = Some((
                        DistRequiresPython {
                            dist,
                            requires_python,
                        },
                        priority,
                    ));
                }
            } else {
                self.compatible_wheel = Some((
                    DistRequiresPython {
                        dist,
                        requires_python,
                    },
                    priority,
                ));
            }
        } else if self.incompatible_wheel.is_none() {
            self.incompatible_wheel = Some(DistRequiresPython {
                dist,
                requires_python,
            });
        }
    }

    /// Insert the given source distribution into the [`PrioritizedDistribution`].
    pub fn insert_source(&mut self, dist: Dist, requires_python: Option<VersionSpecifiers>) {
        if self.source.is_none() {
            self.source = Some(DistRequiresPython {
                dist,
                requires_python,
            });
        }
    }

    /// Return the highest-priority distribution for the package version, if any.
    pub fn get(&self) -> Option<ResolvableDist> {
        match (
            &self.compatible_wheel,
            &self.source,
            &self.incompatible_wheel,
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
}

#[derive(Debug, Clone)]
pub enum ResolvableDist<'a> {
    /// The distribution should be resolved and installed using a source distribution.
    SourceDist(&'a DistRequiresPython),
    /// The distribution should be resolved and installed using a wheel distribution.
    CompatibleWheel(&'a DistRequiresPython, TagPriority),
    /// The distribution should be resolved using an incompatible wheel distribution, but
    /// installed using a source distribution.
    IncompatibleWheel {
        source_dist: &'a DistRequiresPython,
        wheel: &'a DistRequiresPython,
    },
}

impl<'a> ResolvableDist<'a> {
    /// Return the [`DistRequiresPython`] to use during resolution.
    pub fn resolve(&self) -> &DistRequiresPython {
        match *self {
            ResolvableDist::SourceDist(sdist) => sdist,
            ResolvableDist::CompatibleWheel(wheel, _) => wheel,
            ResolvableDist::IncompatibleWheel {
                source_dist: _,
                wheel,
            } => wheel,
        }
    }

    /// Return the [`DistRequiresPython`] to use during installation.
    pub fn install(&self) -> &DistRequiresPython {
        match *self {
            ResolvableDist::SourceDist(sdist) => sdist,
            ResolvableDist::CompatibleWheel(wheel, _) => wheel,
            ResolvableDist::IncompatibleWheel {
                source_dist,
                wheel: _,
            } => source_dist,
        }
    }
}
