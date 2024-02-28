use pep440_rs::VersionSpecifiers;
use platform_tags::{IncompatibleTag, TagCompatibility, TagPriority};
use pypi_types::{Hashes, Yanked};

use crate::Dist;

/// A collection of distributions that have been filtered by relevance.
#[derive(Debug, Default, Clone)]
pub struct PrioritizedDist(Box<PrioritizedDistInner>);

/// [`PrioritizedDist`] is boxed because [`Dist`] is large.
#[derive(Debug, Default, Clone)]
struct PrioritizedDistInner {
    /// The most-compatible source distribution for the package version.
    source: Option<(DistMetadata, SourceCompatibility)>,
    /// The most-compatible wheel distribution for the package version.
    wheel: Option<(DistMetadata, WheelCompatibility)>,
    /// The hashes for each distribution.
    hashes: Vec<Hashes>,
    /// If exclude newer filtered files from this distribution
    exclude_newer: bool,
}

/// A distribution that can be used for both resolution and installation.
#[derive(Debug, Clone)]
pub enum CompatibleDist<'a> {
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

#[derive(Debug, PartialEq, Eq, Ord, PartialOrd, Clone, Copy)]
pub enum SourceCompatibility {
    Incompatible(IncompatibleSource),
    Compatible,
}

#[derive(Debug, PartialEq, Eq, Ord, PartialOrd, Clone, Copy)]
pub enum IncompatibleSource {
    NoBuild,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum WheelCompatibility {
    Incompatible(IncompatibleWheel),
    Compatible(TagPriority),
}

#[derive(Debug, PartialEq, Eq, Ord, PartialOrd, Clone, Copy)]
pub enum IncompatibleWheel {
    Tag(IncompatibleTag),
    RequiresPython,
    NoBinary,
}

#[derive(Debug, Clone, Copy)]
pub enum Incompatible {
    Source(IncompatibleSource),
    Wheel(IncompatibleWheel),
}

/// A [`Dist`] and metadata about it required for downstream filtering.
#[derive(Debug, Clone)]
pub struct DistMetadata {
    /// The distribution.
    pub dist: Dist,
    /// The version of Python required by the distribution.
    pub requires_python: Option<VersionSpecifiers>,
    /// If the distribution file is yanked.
    pub yanked: Yanked,
}

impl PrioritizedDist {
    /// Create a new [`PrioritizedDist`] from the given wheel distribution.
    pub fn from_built(
        dist: Dist,
        requires_python: Option<VersionSpecifiers>,
        yanked: Yanked,
        hash: Option<Hashes>,
        compatibility: WheelCompatibility,
    ) -> Self {
        Self(Box::new(PrioritizedDistInner {
            source: None,
            wheel: Some((
                DistMetadata {
                    dist,
                    requires_python,
                    yanked,
                },
                compatibility,
            )),
            hashes: hash.map(|hash| vec![hash]).unwrap_or_default(),
            exclude_newer: false,
        }))
    }

    /// Create a new [`PrioritizedDist`] from the given source distribution.
    pub fn from_source(
        dist: Dist,
        requires_python: Option<VersionSpecifiers>,
        yanked: Yanked,
        hash: Option<Hashes>,
        compatibility: SourceCompatibility,
    ) -> Self {
        Self(Box::new(PrioritizedDistInner {
            source: Some((
                DistMetadata {
                    dist,
                    requires_python,
                    yanked,
                },
                compatibility,
            )),
            wheel: None,
            hashes: hash.map(|hash| vec![hash]).unwrap_or_default(),
            exclude_newer: false,
        }))
    }

    /// Insert the given built distribution into the [`PrioritizedDist`].
    pub fn insert_built(
        &mut self,
        dist: Dist,
        requires_python: Option<VersionSpecifiers>,
        yanked: Yanked,
        hash: Option<Hashes>,
        compatibility: WheelCompatibility,
    ) {
        // Track the highest-priority wheel.
        if let Some((.., existing_compatibility)) = &self.0.wheel {
            if compatibility > *existing_compatibility {
                self.0.wheel = Some((
                    DistMetadata {
                        dist,
                        requires_python,
                        yanked,
                    },
                    compatibility,
                ));
            }
        } else {
            self.0.wheel = Some((
                DistMetadata {
                    dist,
                    requires_python,
                    yanked,
                },
                compatibility,
            ));
        }

        if let Some(hash) = hash {
            self.0.hashes.push(hash);
        }
    }

    /// Insert the given source distribution into the [`PrioritizedDist`].
    pub fn insert_source(
        &mut self,
        dist: Dist,
        requires_python: Option<VersionSpecifiers>,
        yanked: Yanked,
        hash: Option<Hashes>,
        compatibility: SourceCompatibility,
    ) {
        // Track the highest-priority source distribution.
        if let Some((.., existing_compatibility)) = &self.0.source {
            if compatibility > *existing_compatibility {
                self.0.source = Some((
                    DistMetadata {
                        dist,
                        requires_python,
                        yanked,
                    },
                    compatibility,
                ));
            }
        } else {
            self.0.source = Some((
                DistMetadata {
                    dist,
                    requires_python,
                    yanked,
                },
                compatibility,
            ));
        }

        if let Some(hash) = hash {
            self.0.hashes.push(hash);
        }
    }

    /// Return the highest-priority distribution for the package version, if any.
    pub fn get(&self) -> Option<CompatibleDist> {
        match (&self.0.wheel, &self.0.source) {
            // Prefer the highest-priority, platform-compatible wheel.
            (Some((wheel, WheelCompatibility::Compatible(tag_priority))), _) => {
                Some(CompatibleDist::CompatibleWheel(wheel, *tag_priority))
            }
            // If we have a compatible source distribution and an incompatible wheel, return the
            // wheel. We assume that all distributions have the same metadata for a given package
            // version. If a compatible source distribution exists, we assume we can build it, but
            // using the wheel is faster.
            (
                Some((wheel, WheelCompatibility::Incompatible(_))),
                Some((source_dist, SourceCompatibility::Compatible)),
            ) => Some(CompatibleDist::IncompatibleWheel { source_dist, wheel }),
            // If we have a compatible source distribution and no wheel, return the source
            // distribution.
            (None, Some((source_dist, SourceCompatibility::Compatible))) => {
                Some(CompatibleDist::SourceDist(source_dist))
            }
            // If we have an incompatible source distribution and no wheel, return `None`.
            _ => None,
        }
    }

    /// Return the compatible source distribution, if any.
    pub fn compatible_source(&self) -> Option<&DistMetadata> {
        self.0
            .source
            .as_ref()
            .and_then(|(dist, compatibility)| match compatibility {
                SourceCompatibility::Compatible => Some(dist),
                SourceCompatibility::Incompatible(_) => None,
            })
    }

    /// Return the compatible built distribution, if any.
    pub fn compatible_wheel(&self) -> Option<(&DistMetadata, TagPriority)> {
        self.0
            .wheel
            .as_ref()
            .and_then(|(dist, compatibility)| match compatibility {
                WheelCompatibility::Compatible(priority) => Some((dist, *priority)),
                WheelCompatibility::Incompatible(_) => None,
            })
    }

    /// Return the incompatible source distribution, if any.
    fn incompatible_source(&self) -> Option<(&DistMetadata, &IncompatibleSource)> {
        self.0
            .source
            .as_ref()
            .and_then(|(dist, compatibility)| match compatibility {
                SourceCompatibility::Compatible => None,
                SourceCompatibility::Incompatible(incompatibility) => Some((dist, incompatibility)),
            })
    }

    /// Return the incompatible built distribution, if any.
    fn incompatible_wheel(&self) -> Option<(&DistMetadata, &IncompatibleWheel)> {
        self.0
            .wheel
            .as_ref()
            .and_then(|(dist, compatibility)| match compatibility {
                WheelCompatibility::Compatible(_) => None,
                WheelCompatibility::Incompatible(incompatibility) => Some((dist, incompatibility)),
            })
    }

    /// Return the incompatible source or wheel distribution, if any.
    pub fn incompatible(&self) -> Option<Incompatible> {
        self.incompatible_source()
            .map(|(_, incompatibility)| Incompatible::Source(*incompatibility))
            .or_else(|| {
                self.incompatible_wheel()
                    .map(|(_, incompatibility)| Incompatible::Wheel(*incompatibility))
            })
    }

    /// Set the `exclude_newer` flag
    pub fn set_exclude_newer(&mut self) {
        self.0.exclude_newer = true;
    }

    /// Check if any distributions were excluded by the `exclude_newer` option
    pub fn exclude_newer(&self) -> bool {
        self.0.exclude_newer
    }

    /// Return the hashes for each distribution.
    pub fn hashes(&self) -> &[Hashes] {
        &self.0.hashes
    }

    /// Returns true if and only if this distribution does not contain any
    /// source distributions or wheels.
    pub fn is_empty(&self) -> bool {
        self.0.source.is_none() && self.0.wheel.is_none()
    }
}

impl<'a> CompatibleDist<'a> {
    /// Return the [`DistMetadata`] to use during resolution.
    pub fn for_resolution(&self) -> &DistMetadata {
        match *self {
            CompatibleDist::SourceDist(sdist) => sdist,
            CompatibleDist::CompatibleWheel(wheel, _) => wheel,
            CompatibleDist::IncompatibleWheel {
                source_dist: _,
                wheel,
            } => wheel,
        }
    }

    /// Return the [`DistMetadata`] to use during installation.
    pub fn for_installation(&self) -> &DistMetadata {
        match *self {
            CompatibleDist::SourceDist(sdist) => sdist,
            CompatibleDist::CompatibleWheel(wheel, _) => wheel,
            CompatibleDist::IncompatibleWheel {
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

impl Ord for WheelCompatibility {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (Self::Compatible(p_self), Self::Compatible(p_other)) => p_self.cmp(p_other),
            (Self::Incompatible(_), Self::Compatible(_)) => std::cmp::Ordering::Less,
            (Self::Compatible(_), Self::Incompatible(_)) => std::cmp::Ordering::Greater,
            (Self::Incompatible(t_self), Self::Incompatible(t_other)) => t_self.cmp(t_other),
        }
    }
}

impl PartialOrd for WheelCompatibility {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(Self::cmp(self, other))
    }
}

impl WheelCompatibility {
    pub fn is_compatible(&self) -> bool {
        matches!(self, Self::Compatible(_))
    }
}

impl From<TagCompatibility> for WheelCompatibility {
    fn from(value: TagCompatibility) -> Self {
        match value {
            TagCompatibility::Compatible(priority) => Self::Compatible(priority),
            TagCompatibility::Incompatible(tag) => Self::Incompatible(IncompatibleWheel::Tag(tag)),
        }
    }
}
