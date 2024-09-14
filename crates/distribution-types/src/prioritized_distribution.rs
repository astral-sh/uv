use distribution_filename::BuildTag;
use std::fmt::{Display, Formatter};

use pep440_rs::VersionSpecifiers;
use platform_tags::{IncompatibleTag, TagPriority};
use pypi_types::{HashDigest, Yanked};

use crate::{
    InstalledDist, RegistryBuiltDist, RegistryBuiltWheel, RegistrySourceDist, ResolvedDistRef,
};

/// A collection of distributions that have been filtered by relevance.
#[derive(Debug, Default, Clone)]
pub struct PrioritizedDist(Box<PrioritizedDistInner>);

/// [`PrioritizedDist`] is boxed because [`Dist`] is large.
#[derive(Debug, Default, Clone)]
struct PrioritizedDistInner {
    /// The highest-priority source distribution. Between compatible source distributions this priority is arbitrary.
    source: Option<(RegistrySourceDist, SourceDistCompatibility)>,
    /// The highest-priority wheel index. When present, it is
    /// guaranteed to be a valid index into `wheels`.
    best_wheel_index: Option<usize>,
    /// The set of all wheels associated with this distribution.
    wheels: Vec<(RegistryBuiltWheel, WheelCompatibility)>,
    /// The hashes for each distribution.
    hashes: Vec<HashDigest>,
}

/// A distribution that can be used for both resolution and installation.
#[derive(Debug, Clone)]
pub enum CompatibleDist<'a> {
    /// The distribution is already installed and can be used.
    InstalledDist(&'a InstalledDist),
    /// The distribution should be resolved and installed using a source distribution.
    SourceDist {
        /// The source distribution that should be used.
        sdist: &'a RegistrySourceDist,
        /// The prioritized distribution that the sdist came from.
        prioritized: &'a PrioritizedDist,
    },
    /// The distribution should be resolved and installed using a wheel distribution.
    CompatibleWheel {
        /// The wheel that should be used.
        wheel: &'a RegistryBuiltWheel,
        /// The platform priority associated with the wheel.
        priority: Option<TagPriority>,
        /// The prioritized distribution that the wheel came from.
        prioritized: &'a PrioritizedDist,
    },
    /// The distribution should be resolved using an incompatible wheel distribution, but
    /// installed using a source distribution.
    IncompatibleWheel {
        /// The sdist to be used during installation.
        sdist: &'a RegistrySourceDist,
        /// The wheel to be used during resolution.
        wheel: &'a RegistryBuiltWheel,
        /// The prioritized distribution that the wheel and sdist came from.
        prioritized: &'a PrioritizedDist,
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

impl IncompatibleDist {
    pub fn singular_message(&self) -> String {
        match self {
            Self::Wheel(incompatibility) => match incompatibility {
                IncompatibleWheel::NoBinary => format!("has {self}"),
                IncompatibleWheel::Tag(_) => format!("has {self}"),
                IncompatibleWheel::Yanked(_) => format!("was {self}"),
                IncompatibleWheel::ExcludeNewer(ts) => match ts {
                    Some(_) => format!("was {self}"),
                    None => format!("has {self}"),
                },
                IncompatibleWheel::RequiresPython(..) => format!("requires {self}"),
            },
            Self::Source(incompatibility) => match incompatibility {
                IncompatibleSource::NoBuild => format!("has {self}"),
                IncompatibleSource::Yanked(_) => format!("was {self}"),
                IncompatibleSource::ExcludeNewer(ts) => match ts {
                    Some(_) => format!("was {self}"),
                    None => format!("has {self}"),
                },
                IncompatibleSource::RequiresPython(..) => {
                    format!("requires {self}")
                }
            },
            Self::Unavailable => format!("has {self}"),
        }
    }

    pub fn plural_message(&self) -> String {
        match self {
            Self::Wheel(incompatibility) => match incompatibility {
                IncompatibleWheel::NoBinary => format!("have {self}"),
                IncompatibleWheel::Tag(_) => format!("have {self}"),
                IncompatibleWheel::Yanked(_) => format!("were {self}"),
                IncompatibleWheel::ExcludeNewer(ts) => match ts {
                    Some(_) => format!("were {self}"),
                    None => format!("have {self}"),
                },
                IncompatibleWheel::RequiresPython(..) => format!("require {self}"),
            },
            Self::Source(incompatibility) => match incompatibility {
                IncompatibleSource::NoBuild => format!("have {self}"),
                IncompatibleSource::Yanked(_) => format!("were {self}"),
                IncompatibleSource::ExcludeNewer(ts) => match ts {
                    Some(_) => format!("were {self}"),
                    None => format!("have {self}"),
                },
                IncompatibleSource::RequiresPython(..) => {
                    format!("require {self}")
                }
            },
            Self::Unavailable => format!("have {self}"),
        }
    }
}

impl Display for IncompatibleDist {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wheel(incompatibility) => match incompatibility {
                IncompatibleWheel::NoBinary => {
                    f.write_str("no source distribution and using wheels is disabled")
                }
                IncompatibleWheel::Tag(tag) => match tag {
                    IncompatibleTag::Invalid => f.write_str("no wheels with valid tags"),
                    IncompatibleTag::Python => {
                        f.write_str("no wheels with a matching Python implementation tag")
                    }
                    IncompatibleTag::Abi => f.write_str("no wheels with a matching Python ABI tag"),
                    IncompatibleTag::Platform => {
                        f.write_str("no wheels with a matching platform tag")
                    }
                },
                IncompatibleWheel::Yanked(yanked) => match yanked {
                    Yanked::Bool(_) => f.write_str("yanked"),
                    Yanked::Reason(reason) => write!(
                        f,
                        "yanked (reason: {})",
                        reason.trim().trim_end_matches('.')
                    ),
                },
                IncompatibleWheel::ExcludeNewer(ts) => match ts {
                    Some(_) => f.write_str("published after the exclude newer time"),
                    None => f.write_str("no publish time"),
                },
                IncompatibleWheel::RequiresPython(python, _) => {
                    write!(f, "Python {python}")
                }
            },
            Self::Source(incompatibility) => match incompatibility {
                IncompatibleSource::NoBuild => {
                    f.write_str("no usable wheels and building from source is disabled")
                }
                IncompatibleSource::Yanked(yanked) => match yanked {
                    Yanked::Bool(_) => f.write_str("yanked"),
                    Yanked::Reason(reason) => write!(
                        f,
                        "yanked (reason: {})",
                        reason.trim().trim_end_matches('.')
                    ),
                },
                IncompatibleSource::ExcludeNewer(ts) => match ts {
                    Some(_) => f.write_str("published after the exclude newer time"),
                    None => f.write_str("no publish time"),
                },
                IncompatibleSource::RequiresPython(python, _) => {
                    write!(f, "Python {python}")
                }
            },
            Self::Unavailable => f.write_str("no available distributions"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum PythonRequirementKind {
    /// The installed version of Python.
    Installed,
    /// The target version of Python; that is, the version of Python for which we are resolving
    /// dependencies. This is typically the same as the installed version, but may be different
    /// when specifying an alternate Python version for the resolution.
    Target,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WheelCompatibility {
    Incompatible(IncompatibleWheel),
    Compatible(HashComparison, Option<TagPriority>, Option<BuildTag>),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum IncompatibleWheel {
    /// The wheel was published after the exclude newer time.
    ExcludeNewer(Option<i64>),
    /// The wheel tags do not match those of the target Python platform.
    Tag(IncompatibleTag),
    /// The required Python version is not a superset of the target Python version range.
    RequiresPython(VersionSpecifiers, PythonRequirementKind),
    /// The wheel was yanked.
    Yanked(Yanked),
    /// The use of binary wheels is disabled.
    NoBinary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceDistCompatibility {
    Incompatible(IncompatibleSource),
    Compatible(HashComparison),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum IncompatibleSource {
    ExcludeNewer(Option<i64>),
    RequiresPython(VersionSpecifiers, PythonRequirementKind),
    Yanked(Yanked),
    NoBuild,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum HashComparison {
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
        dist: RegistryBuiltWheel,
        hashes: Vec<HashDigest>,
        compatibility: WheelCompatibility,
    ) -> Self {
        Self(Box::new(PrioritizedDistInner {
            best_wheel_index: Some(0),
            wheels: vec![(dist, compatibility)],
            source: None,
            hashes,
        }))
    }

    /// Create a new [`PrioritizedDist`] from the given source distribution.
    pub fn from_source(
        dist: RegistrySourceDist,
        hashes: Vec<HashDigest>,
        compatibility: SourceDistCompatibility,
    ) -> Self {
        Self(Box::new(PrioritizedDistInner {
            best_wheel_index: None,
            wheels: vec![],
            source: Some((dist, compatibility)),
            hashes,
        }))
    }

    /// Insert the given built distribution into the [`PrioritizedDist`].
    pub fn insert_built(
        &mut self,
        dist: RegistryBuiltWheel,
        hashes: impl IntoIterator<Item = HashDigest>,
        compatibility: WheelCompatibility,
    ) {
        // Track the highest-priority wheel.
        if let Some((.., existing_compatibility)) = self.best_wheel() {
            if compatibility.is_more_compatible(existing_compatibility) {
                self.0.best_wheel_index = Some(self.0.wheels.len());
            }
        } else {
            self.0.best_wheel_index = Some(self.0.wheels.len());
        }
        self.0.wheels.push((dist, compatibility));
        self.0.hashes.extend(hashes);
    }

    /// Insert the given source distribution into the [`PrioritizedDist`].
    pub fn insert_source(
        &mut self,
        dist: RegistrySourceDist,
        hashes: impl IntoIterator<Item = HashDigest>,
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
        let best_wheel = self.0.best_wheel_index.map(|i| &self.0.wheels[i]);
        match (&best_wheel, &self.0.source) {
            // If both are compatible, break ties based on the hash outcome. For example, prefer a
            // source distribution with a matching hash over a wheel with a mismatched hash. When
            // the outcomes are equivalent (e.g., both have a matching hash), prefer the wheel.
            (
                Some((wheel, WheelCompatibility::Compatible(wheel_hash, tag_priority, ..))),
                Some((sdist, SourceDistCompatibility::Compatible(sdist_hash))),
            ) => {
                if sdist_hash > wheel_hash {
                    Some(CompatibleDist::SourceDist {
                        sdist,
                        prioritized: self,
                    })
                } else {
                    Some(CompatibleDist::CompatibleWheel {
                        wheel,
                        priority: *tag_priority,
                        prioritized: self,
                    })
                }
            }
            // Prefer the highest-priority, platform-compatible wheel.
            (Some((wheel, WheelCompatibility::Compatible(_, tag_priority, ..))), _) => {
                Some(CompatibleDist::CompatibleWheel {
                    wheel,
                    priority: *tag_priority,
                    prioritized: self,
                })
            }
            // If we have a compatible source distribution and an incompatible wheel, return the
            // wheel. We assume that all distributions have the same metadata for a given package
            // version. If a compatible source distribution exists, we assume we can build it, but
            // using the wheel is faster.
            (
                Some((wheel, WheelCompatibility::Incompatible(_))),
                Some((sdist, SourceDistCompatibility::Compatible(_))),
            ) => Some(CompatibleDist::IncompatibleWheel {
                sdist,
                wheel,
                prioritized: self,
            }),
            // Otherwise, if we have a source distribution, return it.
            (None, Some((sdist, SourceDistCompatibility::Compatible(_)))) => {
                Some(CompatibleDist::SourceDist {
                    sdist,
                    prioritized: self,
                })
            }
            _ => None,
        }
    }

    /// Return the incompatibility for the best source distribution, if any.
    pub fn incompatible_source(&self) -> Option<&IncompatibleSource> {
        self.0
            .source
            .as_ref()
            .and_then(|(_, compatibility)| match compatibility {
                SourceDistCompatibility::Compatible(_) => None,
                SourceDistCompatibility::Incompatible(incompatibility) => Some(incompatibility),
            })
    }

    /// Return the incompatibility for the best wheel, if any.
    pub fn incompatible_wheel(&self) -> Option<&IncompatibleWheel> {
        self.0
            .best_wheel_index
            .map(|i| &self.0.wheels[i])
            .and_then(|(_, compatibility)| match compatibility {
                WheelCompatibility::Compatible(_, _, _) => None,
                WheelCompatibility::Incompatible(incompatibility) => Some(incompatibility),
            })
    }

    /// Return the hashes for each distribution.
    pub fn hashes(&self) -> &[HashDigest] {
        &self.0.hashes
    }

    /// Returns true if and only if this distribution does not contain any
    /// source distributions or wheels.
    pub fn is_empty(&self) -> bool {
        self.0.source.is_none() && self.0.wheels.is_empty()
    }

    /// If this prioritized dist has at least one wheel, then this creates
    /// a built distribution with the best wheel in this prioritized dist.
    pub fn built_dist(&self) -> Option<RegistryBuiltDist> {
        let best_wheel_index = self.0.best_wheel_index?;
        let wheels = self
            .0
            .wheels
            .iter()
            .map(|(wheel, _)| wheel.clone())
            .collect();
        let sdist = self.0.source.as_ref().map(|(sdist, _)| sdist.clone());
        Some(RegistryBuiltDist {
            wheels,
            best_wheel_index,
            sdist,
        })
    }

    /// If this prioritized dist has an sdist, then this creates a source
    /// distribution.
    pub fn source_dist(&self) -> Option<RegistrySourceDist> {
        let mut sdist = self.0.source.as_ref().map(|(sdist, _)| sdist.clone())?;
        assert!(
            sdist.wheels.is_empty(),
            "source distribution should not have any wheels yet"
        );
        sdist.wheels = self
            .0
            .wheels
            .iter()
            .map(|(wheel, _)| wheel.clone())
            .collect();
        Some(sdist)
    }

    /// Returns the "best" wheel in this prioritized distribution, if one
    /// exists.
    pub fn best_wheel(&self) -> Option<&(RegistryBuiltWheel, WheelCompatibility)> {
        self.0.best_wheel_index.map(|i| &self.0.wheels[i])
    }
}

impl<'a> CompatibleDist<'a> {
    /// Return the [`ResolvedDistRef`] to use during resolution.
    pub fn for_resolution(&self) -> ResolvedDistRef<'a> {
        match *self {
            CompatibleDist::InstalledDist(dist) => ResolvedDistRef::Installed(dist),
            CompatibleDist::SourceDist { sdist, prioritized } => {
                ResolvedDistRef::InstallableRegistrySourceDist { sdist, prioritized }
            }
            CompatibleDist::CompatibleWheel {
                wheel, prioritized, ..
            } => ResolvedDistRef::InstallableRegistryBuiltDist { wheel, prioritized },
            CompatibleDist::IncompatibleWheel {
                wheel, prioritized, ..
            } => ResolvedDistRef::InstallableRegistryBuiltDist { wheel, prioritized },
        }
    }

    /// Return the [`ResolvedDistRef`] to use during installation.
    pub fn for_installation(&self) -> ResolvedDistRef<'a> {
        match *self {
            CompatibleDist::InstalledDist(dist) => ResolvedDistRef::Installed(dist),
            CompatibleDist::SourceDist { sdist, prioritized } => {
                ResolvedDistRef::InstallableRegistrySourceDist { sdist, prioritized }
            }
            CompatibleDist::CompatibleWheel {
                wheel, prioritized, ..
            } => ResolvedDistRef::InstallableRegistryBuiltDist { wheel, prioritized },
            CompatibleDist::IncompatibleWheel {
                sdist, prioritized, ..
            } => ResolvedDistRef::InstallableRegistrySourceDist { sdist, prioritized },
        }
    }

    /// Returns a [`RegistryBuiltWheel`] if the distribution includes a compatible or incompatible
    /// wheel.
    pub fn wheel(&self) -> Option<&RegistryBuiltWheel> {
        match self {
            CompatibleDist::InstalledDist(_) => None,
            CompatibleDist::SourceDist { .. } => None,
            CompatibleDist::CompatibleWheel { wheel, .. } => Some(wheel),
            CompatibleDist::IncompatibleWheel { wheel, .. } => Some(wheel),
        }
    }
}

impl WheelCompatibility {
    pub fn is_compatible(&self) -> bool {
        matches!(self, Self::Compatible(_, _, _))
    }

    /// Return `true` if the current compatibility is more compatible than another.
    ///
    /// Compatible wheels are always higher more compatible than incompatible wheels.
    /// Compatible wheel ordering is determined by tag priority.
    pub fn is_more_compatible(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Compatible(_, _, _), Self::Incompatible(_)) => true,
            (
                Self::Compatible(hash, tag_priority, build_tag),
                Self::Compatible(other_hash, other_tag_priority, other_build_tag),
            ) => {
                (hash, tag_priority, build_tag) > (other_hash, other_tag_priority, other_build_tag)
            }
            (Self::Incompatible(_), Self::Compatible(_, _, _)) => false,
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
                Self::NoBuild | Self::RequiresPython(_, _) | Self::Yanked(_) => true,
            },
            Self::RequiresPython(_, _) => match other {
                Self::ExcludeNewer(_) => false,
                // Version specifiers cannot be reasonably compared
                Self::RequiresPython(_, _) => false,
                Self::NoBuild | Self::Yanked(_) => true,
            },
            Self::Yanked(_) => match other {
                Self::ExcludeNewer(_) | Self::RequiresPython(_, _) => false,
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
                Self::NoBinary | Self::RequiresPython(_, _) | Self::Tag(_) | Self::Yanked(_) => {
                    true
                }
            },
            Self::Tag(tag_self) => match other {
                Self::ExcludeNewer(_) => false,
                Self::Tag(tag_other) => tag_other > tag_self,
                Self::NoBinary | Self::RequiresPython(_, _) | Self::Yanked(_) => true,
            },
            Self::RequiresPython(_, _) => match other {
                Self::ExcludeNewer(_) | Self::Tag(_) => false,
                // Version specifiers cannot be reasonably compared
                Self::RequiresPython(_, _) => false,
                Self::NoBinary | Self::Yanked(_) => true,
            },
            Self::Yanked(_) => match other {
                Self::ExcludeNewer(_) | Self::Tag(_) | Self::RequiresPython(_, _) => false,
                // Yanks with a reason are more helpful for errors
                Self::Yanked(yanked_other) => matches!(yanked_other, Yanked::Reason(_)),
                Self::NoBinary => true,
            },
            Self::NoBinary => false,
        }
    }
}
