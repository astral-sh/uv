use std::fmt::{Display, Formatter};

use pep440_rs::VersionSpecifiers;
use platform_tags::{IncompatibleTag, TagPriority};
use pypi_types::{HashDigest, Yanked};

use crate::{Dist, InstalledDist, ResolvedDistRef};

/// A collection of distributions that have been filtered by relevance.
#[derive(Debug, Default, Clone)]
pub struct PrioritizedDist(Box<PrioritizedDistInner>);

/// [`PrioritizedDist`] is boxed because [`Dist`] is large.
#[derive(Debug, Default, Clone)]
struct PrioritizedDistInner {
    /// The highest-priority source distribution. Between compatible source distributions this priority is arbitrary.
    source: Option<(Dist, SourceDistCompatibility)>,
    /// The highest-priority wheel.
    wheel: Option<(Dist, WheelCompatibility)>,
    /// The hashes for each distribution.
    hashes: Vec<HashDigest>,
}

/// A distribution that can be used for both resolution and installation.
#[derive(Debug, Clone)]
pub enum CompatibleDist<'a> {
    /// The distribution is already installed and can be used.
    InstalledDist(&'a InstalledDist),
    /// The distribution should be resolved and installed using a source distribution.
    SourceDist(&'a Dist),
    /// The distribution should be resolved and installed using a wheel distribution.
    CompatibleWheel(&'a Dist, TagPriority),
    /// The distribution should be resolved using an incompatible wheel distribution, but
    /// installed using a source distribution.
    IncompatibleWheel {
        source_dist: &'a Dist,
        wheel: &'a Dist,
    },
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum IncompatibleDist {
    /// An incompatible wheel is available.
    Wheel(IncompatibleWheel),
    /// An incompatible source distribution is available.
    Source(IncompatibleSource),
    /// No distributions are available
    Unavailable,
}

impl Display for IncompatibleDist {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wheel(incompatibility) => match incompatibility {
                IncompatibleWheel::NoBinary => {
                    f.write_str("has no available source distribution and using wheels is disabled")
                }
                IncompatibleWheel::Tag(tag) => match tag {
                    IncompatibleTag::Invalid => {
                        f.write_str("has no wheels are available with valid tags")
                    }
                    IncompatibleTag::Python => f.write_str(
                        "has no wheels are available with a matching Python implementation",
                    ),
                    IncompatibleTag::Abi => {
                        f.write_str("has no wheels are available with a matching Python ABI")
                    }
                    IncompatibleTag::Platform => {
                        f.write_str("has no wheels are available with a matching platform")
                    }
                },
                IncompatibleWheel::Yanked(yanked) => match yanked {
                    Yanked::Bool(_) => f.write_str("was yanked"),
                    Yanked::Reason(reason) => write!(
                        f,
                        "was yanked (reason: {})",
                        reason.trim().trim_end_matches('.')
                    ),
                },
                IncompatibleWheel::ExcludeNewer(ts) => match ts {
                    Some(_) => f.write_str("was published after the exclude newer time"),
                    None => f.write_str("has no publish time"),
                },
                IncompatibleWheel::RequiresPython(python) => {
                    write!(f, "requires at python {python}")
                }
            },
            Self::Source(incompatibility) => match incompatibility {
                IncompatibleSource::NoBuild => {
                    f.write_str("has no usable wheels and building from source is disabled")
                }
                IncompatibleSource::Yanked(yanked) => match yanked {
                    Yanked::Bool(_) => f.write_str("was yanked"),
                    Yanked::Reason(reason) => write!(
                        f,
                        "was yanked (reason: {})",
                        reason.trim().trim_end_matches('.')
                    ),
                },
                IncompatibleSource::ExcludeNewer(ts) => match ts {
                    Some(_) => f.write_str("was published after the exclude newer time"),
                    None => f.write_str("has no publish time"),
                },
                IncompatibleSource::RequiresPython(python) => {
                    write!(f, "requires python {python}")
                }
            },
            Self::Unavailable => f.write_str("has no available distributions"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WheelCompatibility {
    Incompatible(IncompatibleWheel),
    Compatible(Hash, TagPriority),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum IncompatibleWheel {
    ExcludeNewer(Option<i64>),
    Tag(IncompatibleTag),
    RequiresPython(VersionSpecifiers),
    Yanked(Yanked),
    NoBinary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceDistCompatibility {
    Incompatible(IncompatibleSource),
    Compatible(Hash),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum IncompatibleSource {
    ExcludeNewer(Option<i64>),
    RequiresPython(VersionSpecifiers),
    Yanked(Yanked),
    NoBuild,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Hash {
    /// The hash is present, but does not match the expected value.
    Mismatched,
    /// The hash is missing.
    Missing,
    /// The hash matches the expected value.
    Matched,
}

impl PrioritizedDist {
    /// Create a new [`PrioritizedDist`] from the given wheel distribution.
    pub fn from_built(
        dist: Dist,
        hashes: Vec<HashDigest>,
        compatibility: WheelCompatibility,
    ) -> Self {
        Self(Box::new(PrioritizedDistInner {
            wheel: Some((dist, compatibility)),
            source: None,
            hashes,
        }))
    }

    /// Create a new [`PrioritizedDist`] from the given source distribution.
    pub fn from_source(
        dist: Dist,
        hashes: Vec<HashDigest>,
        compatibility: SourceDistCompatibility,
    ) -> Self {
        Self(Box::new(PrioritizedDistInner {
            wheel: None,
            source: Some((dist, compatibility)),
            hashes,
        }))
    }

    /// Insert the given built distribution into the [`PrioritizedDist`].
    pub fn insert_built(
        &mut self,
        dist: Dist,
        hashes: Vec<HashDigest>,
        compatibility: WheelCompatibility,
    ) {
        // Track the highest-priority wheel.
        if let Some((.., existing_compatibility)) = &self.0.wheel {
            if compatibility.is_more_compatible(existing_compatibility) {
                self.0.wheel = Some((dist, compatibility));
            }
        } else {
            self.0.wheel = Some((dist, compatibility));
        }

        self.0.hashes.extend(hashes);
    }

    /// Insert the given source distribution into the [`PrioritizedDist`].
    pub fn insert_source(
        &mut self,
        dist: Dist,
        hashes: Vec<HashDigest>,
        compatibility: SourceDistCompatibility,
    ) {
        // Track the highest-priority source.
        if let Some((.., existing_compatibility)) = &self.0.source {
            if compatibility.is_more_compatible(existing_compatibility) {
                self.0.source = Some((dist, compatibility));
            }
        } else {
            self.0.source = Some((dist, compatibility));
        }

        self.0.hashes.extend(hashes);
    }

    /// Return the highest-priority distribution for the package version, if any.
    pub fn get(&self) -> Option<CompatibleDist> {
        match (&self.0.wheel, &self.0.source) {
            // If both are compatible, break ties based on the hash.
            (
                Some((wheel, WheelCompatibility::Compatible(wheel_hash, tag_priority))),
                Some((source_dist, SourceDistCompatibility::Compatible(source_hash))),
            ) => {
                if source_hash > wheel_hash {
                    Some(CompatibleDist::SourceDist(source_dist))
                } else {
                    Some(CompatibleDist::CompatibleWheel(wheel, *tag_priority))
                }
            }
            // Prefer the highest-priority, platform-compatible wheel.
            (Some((wheel, WheelCompatibility::Compatible(_, tag_priority))), _) => {
                Some(CompatibleDist::CompatibleWheel(wheel, *tag_priority))
            }
            // If we have a compatible source distribution and an incompatible wheel, return the
            // wheel. We assume that all distributions have the same metadata for a given package
            // version. If a compatible source distribution exists, we assume we can build it, but
            // using the wheel is faster.
            (
                Some((wheel, WheelCompatibility::Incompatible(_))),
                Some((source_dist, SourceDistCompatibility::Compatible(_))),
            ) => Some(CompatibleDist::IncompatibleWheel { source_dist, wheel }),
            // Otherwise, if we have a source distribution, return it.
            (None, Some((source_dist, SourceDistCompatibility::Compatible(_)))) => {
                Some(CompatibleDist::SourceDist(source_dist))
            }
            _ => None,
        }
    }

    /// Return the incompatible source distribution, if any.
    pub fn incompatible_source(&self) -> Option<(&Dist, &IncompatibleSource)> {
        self.0
            .source
            .as_ref()
            .and_then(|(dist, compatibility)| match compatibility {
                SourceDistCompatibility::Compatible(_) => None,
                SourceDistCompatibility::Incompatible(incompatibility) => {
                    Some((dist, incompatibility))
                }
            })
    }

    /// Return the incompatible built distribution, if any.
    pub fn incompatible_wheel(&self) -> Option<(&Dist, &IncompatibleWheel)> {
        self.0
            .wheel
            .as_ref()
            .and_then(|(dist, compatibility)| match compatibility {
                WheelCompatibility::Compatible(_, _) => None,
                WheelCompatibility::Incompatible(incompatibility) => Some((dist, incompatibility)),
            })
    }

    /// Return the hashes for each distribution.
    pub fn hashes(&self) -> &[HashDigest] {
        &self.0.hashes
    }

    /// Returns true if and only if this distribution does not contain any
    /// source distributions or wheels.
    pub fn is_empty(&self) -> bool {
        self.0.source.is_none() && self.0.wheel.is_none()
    }
}

impl<'a> CompatibleDist<'a> {
    /// Return the [`ResolvedDistRef`] to use during resolution.
    pub fn for_resolution(&self) -> ResolvedDistRef<'a> {
        match *self {
            CompatibleDist::InstalledDist(dist) => ResolvedDistRef::Installed(dist),
            CompatibleDist::SourceDist(sdist) => ResolvedDistRef::Installable(sdist),
            CompatibleDist::CompatibleWheel(wheel, _) => ResolvedDistRef::Installable(wheel),
            CompatibleDist::IncompatibleWheel {
                source_dist: _,
                wheel,
            } => ResolvedDistRef::Installable(wheel),
        }
    }

    /// Return the [`ResolvedDistRef`] to use during installation.
    pub fn for_installation(&self) -> ResolvedDistRef<'a> {
        match *self {
            CompatibleDist::InstalledDist(dist) => ResolvedDistRef::Installed(dist),
            CompatibleDist::SourceDist(sdist) => ResolvedDistRef::Installable(sdist),
            CompatibleDist::CompatibleWheel(wheel, _) => ResolvedDistRef::Installable(wheel),
            CompatibleDist::IncompatibleWheel {
                source_dist,
                wheel: _,
            } => ResolvedDistRef::Installable(source_dist),
        }
    }

    /// Returns whether the distribution is a source distribution.
    ///
    /// Avoid building source distributions we don't need.
    pub fn prefetchable(&self) -> bool {
        match *self {
            CompatibleDist::SourceDist(_) => false,
            CompatibleDist::InstalledDist(_)
            | CompatibleDist::CompatibleWheel(_, _)
            | CompatibleDist::IncompatibleWheel { .. } => true,
        }
    }
}

impl WheelCompatibility {
    pub fn is_compatible(&self) -> bool {
        matches!(self, Self::Compatible(_, _))
    }

    /// Return `true` if the current compatibility is more compatible than another.
    ///
    /// Compatible wheels are always higher more compatible than incompatible wheels.
    /// Compatible wheel ordering is determined by tag priority.
    pub fn is_more_compatible(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Compatible(_, _), Self::Incompatible(_)) => true,
            (
                Self::Compatible(hash, tag_priority),
                Self::Compatible(other_hash, other_tag_priority),
            ) => (hash, tag_priority) > (other_hash, other_tag_priority),
            (Self::Incompatible(_), Self::Compatible(_, _)) => false,
            (Self::Incompatible(incompatibility), Self::Incompatible(other_incompatibility)) => {
                incompatibility.is_more_compatible(other_incompatibility)
            }
        }
    }
}

impl SourceDistCompatibility {
    /// Return the higher priority compatibility.
    ///
    /// Compatible source distributions are always higher priority than incompatible source distributions.
    /// Compatible source distribution priority is arbitrary.
    /// Incompatible source distribution priority selects a source distribution that was "closest" to being usable.
    pub fn is_more_compatible(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Compatible(_), Self::Incompatible(_)) => true,
            (Self::Compatible(compatibility), Self::Compatible(other_compatibility)) => {
                compatibility > other_compatibility
            }
            (Self::Incompatible(_), Self::Compatible(_)) => false,
            (Self::Incompatible(incompatibility), Self::Incompatible(other_incompatibility)) => {
                incompatibility.is_more_compatible(other_incompatibility)
            }
        }
    }
}

impl IncompatibleSource {
    fn is_more_compatible(&self, other: &Self) -> bool {
        match self {
            Self::ExcludeNewer(timestamp_self) => match other {
                // Smaller timestamps are closer to the cut-off time
                Self::ExcludeNewer(timestamp_other) => timestamp_other < timestamp_self,
                Self::NoBuild | Self::RequiresPython(_) | Self::Yanked(_) => true,
            },
            Self::RequiresPython(_) => match other {
                Self::ExcludeNewer(_) => false,
                // Version specifiers cannot be reasonably compared
                Self::RequiresPython(_) => false,
                Self::NoBuild | Self::Yanked(_) => true,
            },
            Self::Yanked(_) => match other {
                Self::ExcludeNewer(_) | Self::RequiresPython(_) => false,
                // Yanks with a reason are more helpful for errors
                Self::Yanked(yanked_other) => matches!(yanked_other, Yanked::Reason(_)),
                Self::NoBuild => true,
            },
            Self::NoBuild => false,
        }
    }
}

impl IncompatibleWheel {
    fn is_more_compatible(&self, other: &Self) -> bool {
        match self {
            Self::ExcludeNewer(timestamp_self) => match other {
                // Smaller timestamps are closer to the cut-off time
                Self::ExcludeNewer(timestamp_other) => match (timestamp_self, timestamp_other) {
                    (None, _) => true,
                    (_, None) => false,
                    (Some(timestamp_self), Some(timestamp_other)) => {
                        timestamp_other < timestamp_self
                    }
                },
                Self::NoBinary | Self::RequiresPython(_) | Self::Tag(_) | Self::Yanked(_) => true,
            },
            Self::Tag(tag_self) => match other {
                Self::ExcludeNewer(_) => false,
                Self::Tag(tag_other) => tag_other > tag_self,
                Self::NoBinary | Self::RequiresPython(_) | Self::Yanked(_) => true,
            },
            Self::RequiresPython(_) => match other {
                Self::ExcludeNewer(_) | Self::Tag(_) => false,
                // Version specifiers cannot be reasonably compared
                Self::RequiresPython(_) => false,
                Self::NoBinary | Self::Yanked(_) => true,
            },
            Self::Yanked(_) => match other {
                Self::ExcludeNewer(_) | Self::Tag(_) | Self::RequiresPython(_) => false,
                // Yanks with a reason are more helpful for errors
                Self::Yanked(yanked_other) => matches!(yanked_other, Yanked::Reason(_)),
                Self::NoBinary => true,
            },
            Self::NoBinary => false,
        }
    }
}
