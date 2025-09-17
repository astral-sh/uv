use std::fmt::{Display, Formatter};

use arcstr::ArcStr;
use owo_colors::OwoColorize;
use tracing::debug;

use uv_distribution_filename::{BuildTag, WheelFilename};
use uv_pep440::{Version, VersionSpecifier, VersionSpecifiers};
use uv_pep508::{MarkerExpression, MarkerOperator, MarkerTree, MarkerValueString};
use uv_platform_tags::{AbiTag, IncompatibleTag, LanguageTag, PlatformTag, TagPriority, Tags};
use uv_pypi_types::{HashDigest, Yanked};
use uv_variants::VariantPriority;
use uv_variants::resolved_variants::ResolvedVariants;

use crate::{
    File, IndexUrl, InstalledDist, KnownPlatform, RegistryBuiltDist, RegistryBuiltWheel,
    RegistrySourceDist, RegistryVariantsJson, ResolvedDistRef,
};

/// A collection of distributions that have been filtered by relevance.
#[derive(Debug, Default, Clone)]
pub struct PrioritizedDist(Box<PrioritizedDistInner>);

/// [`PrioritizedDist`] is boxed because [`Dist`] is large.
#[derive(Debug, Clone)]
struct PrioritizedDistInner {
    /// The highest-priority source distribution. Between compatible source distributions this priority is arbitrary.
    source: Option<(RegistrySourceDist, SourceDistCompatibility)>,
    /// The highest-priority wheel index. When present, it is
    /// guaranteed to be a valid index into `wheels`.
    ///
    /// This wheel may still be incompatible.
    best_wheel_index: Option<usize>,
    /// The set of all wheels associated with this distribution.
    wheels: Vec<(RegistryBuiltWheel, WheelCompatibility)>,
    /// The `variants.json` file associated with the package version.
    variants_json: Option<RegistryVariantsJson>,
    /// The hashes for each distribution.
    hashes: Vec<HashDigest>,
    /// The set of supported platforms for the distribution, described in terms of their markers.
    markers: MarkerTree,
}

impl Default for PrioritizedDistInner {
    fn default() -> Self {
        Self {
            source: None,
            best_wheel_index: None,
            wheels: Vec::new(),
            variants_json: None,
            hashes: Vec::new(),
            markers: MarkerTree::FALSE,
        }
    }
}

/// A distribution that can be used for both resolution and installation.
#[derive(Debug, Copy, Clone)]
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

impl CompatibleDist<'_> {
    /// Return the `requires-python` specifier for the distribution, if any.
    pub fn requires_python(&self) -> Option<&VersionSpecifiers> {
        match self {
            Self::InstalledDist(_) => None,
            Self::SourceDist { sdist, .. } => sdist.file.requires_python.as_ref(),
            Self::CompatibleWheel { wheel, .. } => wheel.file.requires_python.as_ref(),
            Self::IncompatibleWheel { sdist, .. } => sdist.file.requires_python.as_ref(),
        }
    }

    /// Return the index URL for the distribution, if any.
    pub fn index(&self) -> Option<&IndexUrl> {
        match self {
            CompatibleDist::InstalledDist(_) => None,
            CompatibleDist::SourceDist { sdist, .. } => Some(&sdist.index),
            CompatibleDist::CompatibleWheel { wheel, .. } => Some(&wheel.index),
            CompatibleDist::IncompatibleWheel { sdist, .. } => Some(&sdist.index),
        }
    }

    // For installable distributions, return the prioritized distribution it was derived from.
    pub fn prioritized(&self) -> Option<&PrioritizedDist> {
        match self {
            Self::InstalledDist(_) => None,
            Self::SourceDist { prioritized, .. }
            | Self::CompatibleWheel { prioritized, .. }
            | Self::IncompatibleWheel { prioritized, .. } => Some(prioritized),
        }
    }

    /// Return the set of supported platform the distribution, in terms of their markers.
    pub fn implied_markers(&self) -> MarkerTree {
        match self.prioritized() {
            Some(prioritized) => prioritized.0.markers,
            None => MarkerTree::TRUE,
        }
    }
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
                IncompatibleWheel::Variant => format!("has {self}"),
                IncompatibleWheel::Tag(_) => format!("has {self}"),
                IncompatibleWheel::Yanked(_) => format!("was {self}"),
                IncompatibleWheel::ExcludeNewer(ts) => match ts {
                    Some(_) => format!("was {self}"),
                    None => format!("has {self}"),
                },
                IncompatibleWheel::RequiresPython(..) => format!("requires {self}"),
                IncompatibleWheel::MissingPlatform(_) => format!("has {self}"),
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
                IncompatibleWheel::Variant => format!("have {self}"),
                IncompatibleWheel::Tag(_) => format!("have {self}"),
                IncompatibleWheel::Yanked(_) => format!("were {self}"),
                IncompatibleWheel::ExcludeNewer(ts) => match ts {
                    Some(_) => format!("were {self}"),
                    None => format!("have {self}"),
                },
                IncompatibleWheel::RequiresPython(..) => format!("require {self}"),
                IncompatibleWheel::MissingPlatform(_) => format!("have {self}"),
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

    pub fn context_message(
        &self,
        tags: Option<&Tags>,
        requires_python: Option<AbiTag>,
    ) -> Option<String> {
        match self {
            Self::Wheel(incompatibility) => match incompatibility {
                IncompatibleWheel::Tag(IncompatibleTag::Python) => {
                    let tag = tags?.python_tag().as_ref().map(ToString::to_string)?;
                    Some(format!("(e.g., `{tag}`)", tag = tag.cyan()))
                }
                IncompatibleWheel::Tag(IncompatibleTag::Abi) => {
                    let tag = tags?.abi_tag().as_ref().map(ToString::to_string)?;
                    Some(format!("(e.g., `{tag}`)", tag = tag.cyan()))
                }
                IncompatibleWheel::Tag(IncompatibleTag::AbiPythonVersion) => {
                    let tag = requires_python?;
                    Some(format!("(e.g., `{tag}`)", tag = tag.cyan()))
                }
                IncompatibleWheel::Tag(IncompatibleTag::Platform) => {
                    let tag = tags?.platform_tag().map(ToString::to_string)?;
                    Some(format!("(e.g., `{tag}`)", tag = tag.cyan()))
                }
                IncompatibleWheel::Tag(IncompatibleTag::Invalid) => None,
                IncompatibleWheel::Variant => None,
                IncompatibleWheel::NoBinary => None,
                IncompatibleWheel::Yanked(..) => None,
                IncompatibleWheel::ExcludeNewer(..) => None,
                IncompatibleWheel::RequiresPython(..) => None,
                IncompatibleWheel::MissingPlatform(..) => None,
            },
            Self::Source(..) => None,
            Self::Unavailable => None,
        }
    }
}

impl Display for IncompatibleDist {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wheel(incompatibility) => match incompatibility {
                IncompatibleWheel::NoBinary => f.write_str("no source distribution"),
                IncompatibleWheel::Variant => {
                    f.write_str("no wheels with a variant supported on the current platform")
                }
                IncompatibleWheel::Tag(tag) => match tag {
                    IncompatibleTag::Invalid => f.write_str("no wheels with valid tags"),
                    IncompatibleTag::Python => {
                        f.write_str("no wheels with a matching Python implementation tag")
                    }
                    IncompatibleTag::Abi => f.write_str("no wheels with a matching Python ABI tag"),
                    IncompatibleTag::AbiPythonVersion => {
                        f.write_str("no wheels with a matching Python version tag")
                    }
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
                IncompatibleWheel::MissingPlatform(marker) => {
                    if let Some(platform) = KnownPlatform::from_marker(*marker) {
                        write!(f, "no {platform}-compatible wheels")
                    } else if let Some(marker) = marker.try_to_string() {
                        write!(f, "no `{marker}`-compatible wheels")
                    } else {
                        write!(f, "no compatible wheels")
                    }
                }
            },
            Self::Source(incompatibility) => match incompatibility {
                IncompatibleSource::NoBuild => f.write_str("no usable wheels"),
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

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
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
    Compatible {
        hash: HashComparison,
        variant_priority: VariantPriority,
        tag_priority: Option<TagPriority>,
        build_tag: Option<BuildTag>,
    },
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum IncompatibleWheel {
    /// The wheel was published after the exclude newer time.
    ExcludeNewer(Option<i64>),
    /// The wheel variant does not match the target platform.
    Variant,
    /// The wheel tags do not match those of the target Python platform.
    Tag(IncompatibleTag),
    /// The required Python version is not a superset of the target Python version range.
    RequiresPython(VersionSpecifiers, PythonRequirementKind),
    /// The wheel was yanked.
    Yanked(Yanked),
    /// The use of binary wheels is disabled.
    NoBinary,
    /// Wheels are not available for the current platform.
    MissingPlatform(MarkerTree),
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
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
            markers: implied_markers(&dist.filename),
            best_wheel_index: Some(0),
            wheels: vec![(dist, compatibility)],
            variants_json: None,
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
            markers: MarkerTree::TRUE,
            best_wheel_index: None,
            wheels: vec![],
            source: Some((dist, compatibility)),
            variants_json: None,
            hashes,
        }))
    }

    /// Create a new [`PrioritizedDist`] from the `variants.json`.
    pub fn from_variant_json(variant_json: RegistryVariantsJson) -> Self {
        Self(Box::new(PrioritizedDistInner {
            markers: MarkerTree::TRUE,
            best_wheel_index: Some(0),
            wheels: vec![],
            source: None,
            variants_json: Some(variant_json),
            hashes: vec![],
        }))
    }

    /// Insert the given built distribution into the [`PrioritizedDist`].
    pub fn insert_built(
        &mut self,
        dist: RegistryBuiltWheel,
        hashes: impl IntoIterator<Item = HashDigest>,
        compatibility: WheelCompatibility,
    ) {
        // Track the implied markers.
        if compatibility.is_compatible() {
            if !self.0.markers.is_true() {
                self.0.markers.or(implied_markers(&dist.filename));
            }
        }
        // Track the hashes.
        if !compatibility.is_excluded() {
            self.0.hashes.extend(hashes);
        }
        // Track the highest-priority wheel.
        if let Some((.., existing_compatibility)) = self.best_wheel() {
            if compatibility.is_more_compatible(existing_compatibility) {
                self.0.best_wheel_index = Some(self.0.wheels.len());
            }
        } else {
            self.0.best_wheel_index = Some(self.0.wheels.len());
        }
        self.0.wheels.push((dist, compatibility));
    }

    /// Insert the given source distribution into the [`PrioritizedDist`].
    pub fn insert_source(
        &mut self,
        dist: RegistrySourceDist,
        hashes: impl IntoIterator<Item = HashDigest>,
        compatibility: SourceDistCompatibility,
    ) {
        // Track the implied markers.
        if compatibility.is_compatible() {
            self.0.markers = MarkerTree::TRUE;
        }
        // Track the hashes.
        if !compatibility.is_excluded() {
            self.0.hashes.extend(hashes);
        }
        // Track the highest-priority source.
        if let Some((.., existing_compatibility)) = &self.0.source {
            if compatibility.is_more_compatible(existing_compatibility) {
                self.0.source = Some((dist, compatibility));
            }
        } else {
            self.0.source = Some((dist, compatibility));
        }
    }

    pub fn insert_variant_json(&mut self, variant_json: RegistryVariantsJson) {
        debug_assert!(
            self.0.variants_json.is_none(),
            "The variants.json filename is unique"
        );
        self.0.variants_json = Some(variant_json);
    }

    /// Return the variants JSON for the distribution, if any.
    pub fn variants_json(&self) -> Option<&RegistryVariantsJson> {
        self.0.variants_json.as_ref()
    }

    /// Return the index URL for the distribution, if any.
    pub fn index(&self) -> Option<&IndexUrl> {
        self.0
            .source
            .as_ref()
            .map(|(sdist, _)| &sdist.index)
            .or_else(|| self.0.wheels.first().map(|(wheel, _)| &wheel.index))
    }

    /// Return the highest-priority distribution for the package version, if any.
    pub fn get(&self, allow_all_variants: bool) -> Option<CompatibleDist<'_>> {
        let best_wheel = self.0.best_wheel_index.map(|i| &self.0.wheels[i]);
        match (&best_wheel, &self.0.source) {
            // If both are compatible, break ties based on the hash outcome. For example, prefer a
            // source distribution with a matching hash over a wheel with a mismatched hash. When
            // the outcomes are equivalent (e.g., both have a matching hash), prefer the wheel.
            (
                Some((
                    wheel,
                    WheelCompatibility::Compatible {
                        hash: wheel_hash,
                        tag_priority,
                        variant_priority,
                        ..
                    },
                )),
                Some((sdist, SourceDistCompatibility::Compatible(sdist_hash))),
            ) if matches!(
                variant_priority,
                VariantPriority::BestVariant | VariantPriority::NonVariant
            ) || allow_all_variants =>
            {
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
            (
                Some((
                    wheel,
                    WheelCompatibility::Compatible {
                        hash: _,
                        tag_priority,
                        variant_priority,
                        ..
                    },
                )),
                _,
            ) if matches!(
                variant_priority,
                VariantPriority::BestVariant | VariantPriority::NonVariant
            ) || allow_all_variants =>
            {
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
            //
            // (If the incompatible wheel should actually be ignored entirely, fall through to
            // using the source distribution.)
            (
                Some((wheel, compatibility @ WheelCompatibility::Incompatible(_))),
                Some((sdist, SourceDistCompatibility::Compatible(_))),
            ) if !compatibility.is_excluded() => Some(CompatibleDist::IncompatibleWheel {
                sdist,
                wheel,
                prioritized: self,
            }),
            // Otherwise, if we have a source distribution, return it.
            (.., Some((sdist, SourceDistCompatibility::Compatible(_)))) => {
                Some(CompatibleDist::SourceDist {
                    sdist,
                    prioritized: self,
                })
            }
            _ => None,
        }
    }

    /// Prioritize a matching variant wheel over a matching non-variant wheel.
    ///
    /// Returns `None` or there is no matching variant.
    pub fn prioritize_best_variant_wheel(
        &self,
        resolved_variants: &ResolvedVariants,
    ) -> Option<Self> {
        let mut highest_priority_variant_wheel: Option<(usize, Vec<usize>)> = None;
        for (wheel_index, (wheel, compatibility)) in self.wheels().enumerate() {
            if !compatibility.is_compatible() {
                continue;
            }

            let Some(variant) = wheel.filename.variant() else {
                // The non-variant wheel is already supported
                continue;
            };

            let Some(scores) = resolved_variants.score_variant(variant) else {
                continue;
            };

            if let Some((_, old_scores)) = &highest_priority_variant_wheel {
                if &scores > old_scores {
                    highest_priority_variant_wheel = Some((wheel_index, scores));
                }
            } else {
                highest_priority_variant_wheel = Some((wheel_index, scores));
            }
        }

        if let Some((wheel_index, _)) = highest_priority_variant_wheel {
            use owo_colors::OwoColorize;

            let inner = PrioritizedDistInner {
                best_wheel_index: Some(wheel_index),
                ..(*self.0).clone()
            };
            let compatible_wheel = &inner.wheels[wheel_index];
            debug!(
                "{} {}",
                "Using variant wheel".red(),
                compatible_wheel.0.filename
            );
            Some(Self(Box::new(inner)))
        } else {
            None
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
    pub fn incompatible_wheel(&self, allow_variants: bool) -> Option<&IncompatibleWheel> {
        self.0
            .best_wheel_index
            .map(|i| &self.0.wheels[i])
            .and_then(|(_, compatibility)| match compatibility {
                WheelCompatibility::Compatible {
                    variant_priority: VariantPriority::Unknown,
                    ..
                } if !allow_variants => Some(&IncompatibleWheel::Variant),
                WheelCompatibility::Compatible { .. } => None,
                WheelCompatibility::Incompatible(incompatibility) => Some(incompatibility),
            })
    }

    pub fn wheels(&self) -> impl Iterator<Item = &(RegistryBuiltWheel, WheelCompatibility)> {
        self.0.wheels.iter()
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

        // Remove any excluded wheels from the list of wheels, and adjust the wheel index to be
        // relative to the filtered list.
        let mut adjusted_wheels = Vec::with_capacity(self.0.wheels.len());
        let mut adjusted_best_index = 0;
        for (i, (wheel, compatibility)) in self.0.wheels.iter().enumerate() {
            if compatibility.is_excluded() {
                continue;
            }
            if i == best_wheel_index {
                adjusted_best_index = adjusted_wheels.len();
            }
            adjusted_wheels.push(wheel.clone());
        }

        let sdist = self.0.source.as_ref().map(|(sdist, _)| sdist.clone());
        Some(RegistryBuiltDist {
            wheels: adjusted_wheels,
            best_wheel_index: adjusted_best_index,
            sdist,
        })
    }

    /// If this prioritized dist has an sdist, then this creates a source
    /// distribution.
    pub fn source_dist(&self) -> Option<RegistrySourceDist> {
        let mut sdist = self
            .0
            .source
            .as_ref()
            .filter(|(_, compatibility)| !compatibility.is_excluded())
            .map(|(sdist, _)| sdist.clone())?;
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

    /// Returns an iterator of all wheels and the source distribution, if any.
    pub fn files(&self) -> impl Iterator<Item = &File> {
        self.0
            .wheels
            .iter()
            .map(|(wheel, _)| wheel.file.as_ref())
            .chain(
                self.0
                    .source
                    .as_ref()
                    .map(|(source_dist, _)| source_dist.file.as_ref()),
            )
    }

    /// Returns an iterator over all Python tags for the distribution.
    pub fn python_tags(&self) -> impl Iterator<Item = LanguageTag> + '_ {
        self.0
            .wheels
            .iter()
            .flat_map(|(wheel, _)| wheel.filename.python_tags().iter().copied())
    }

    /// Returns an iterator over all ABI tags for the distribution.
    pub fn abi_tags(&self) -> impl Iterator<Item = AbiTag> + '_ {
        self.0
            .wheels
            .iter()
            .flat_map(|(wheel, _)| wheel.filename.abi_tags().iter().copied())
    }

    /// Returns the set of platform tags for the distribution that are ABI-compatible with the given
    /// tags.
    pub fn platform_tags<'a>(
        &'a self,
        tags: &'a Tags,
    ) -> impl Iterator<Item = &'a PlatformTag> + 'a {
        self.0.wheels.iter().flat_map(move |(wheel, _)| {
            if wheel.filename.python_tags().iter().any(|wheel_py| {
                wheel
                    .filename
                    .abi_tags()
                    .iter()
                    .any(|wheel_abi| tags.is_compatible_abi(*wheel_py, *wheel_abi))
            }) {
                wheel.filename.platform_tags().iter()
            } else {
                [].iter()
            }
        })
    }
}

impl<'a> CompatibleDist<'a> {
    /// Return the [`ResolvedDistRef`] to use during resolution.
    pub fn for_resolution(&self) -> ResolvedDistRef<'a> {
        match self {
            Self::InstalledDist(dist) => ResolvedDistRef::Installed { dist },
            Self::SourceDist { sdist, prioritized } => {
                ResolvedDistRef::InstallableRegistrySourceDist { sdist, prioritized }
            }
            Self::CompatibleWheel {
                wheel, prioritized, ..
            } => ResolvedDistRef::InstallableRegistryBuiltDist { wheel, prioritized },
            Self::IncompatibleWheel {
                wheel, prioritized, ..
            } => ResolvedDistRef::InstallableRegistryBuiltDist { wheel, prioritized },
        }
    }

    /// Return the [`ResolvedDistRef`] to use during installation.
    pub fn for_installation(&self) -> ResolvedDistRef<'a> {
        match self {
            Self::InstalledDist(dist) => ResolvedDistRef::Installed { dist },
            Self::SourceDist { sdist, prioritized } => {
                ResolvedDistRef::InstallableRegistrySourceDist { sdist, prioritized }
            }
            Self::CompatibleWheel {
                wheel, prioritized, ..
            } => ResolvedDistRef::InstallableRegistryBuiltDist { wheel, prioritized },
            Self::IncompatibleWheel {
                sdist, prioritized, ..
            } => ResolvedDistRef::InstallableRegistrySourceDist { sdist, prioritized },
        }
    }

    /// Returns a [`RegistryBuiltWheel`] if the distribution includes a compatible or incompatible
    /// wheel.
    pub fn wheel(&self) -> Option<&RegistryBuiltWheel> {
        match self {
            Self::InstalledDist(_) => None,
            Self::SourceDist { .. } => None,
            Self::CompatibleWheel { wheel, .. } => Some(wheel),
            Self::IncompatibleWheel { wheel, .. } => Some(wheel),
        }
    }
}

impl WheelCompatibility {
    /// Return `true` if the distribution is compatible.
    pub fn is_compatible(&self) -> bool {
        matches!(self, Self::Compatible { .. })
    }

    /// Return `true` if the distribution is excluded.
    pub fn is_excluded(&self) -> bool {
        matches!(self, Self::Incompatible(IncompatibleWheel::ExcludeNewer(_)))
    }

    /// Return `true` if the current compatibility is more compatible than another.
    ///
    /// Compatible wheels are always higher more compatible than incompatible wheels.
    /// Compatible wheel ordering is determined by tag priority.
    pub fn is_more_compatible(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Compatible { .. }, Self::Incompatible(..)) => true,
            (
                Self::Compatible {
                    hash,
                    variant_priority,
                    tag_priority,
                    build_tag,
                },
                Self::Compatible {
                    hash: other_hash,
                    variant_priority: other_variant_priority,
                    tag_priority: other_tag_priority,
                    build_tag: other_build_tag,
                },
            ) => {
                (hash, variant_priority, tag_priority, build_tag)
                    > (
                        other_hash,
                        other_variant_priority,
                        other_tag_priority,
                        other_build_tag,
                    )
            }
            (Self::Incompatible(..), Self::Compatible { .. }) => false,
            (Self::Incompatible(incompatibility), Self::Incompatible(other_incompatibility)) => {
                incompatibility.is_more_compatible(other_incompatibility)
            }
        }
    }
}

impl SourceDistCompatibility {
    /// Return `true` if the distribution is compatible.
    pub fn is_compatible(&self) -> bool {
        matches!(self, Self::Compatible(_))
    }

    /// Return `true` if the distribution is excluded.
    pub fn is_excluded(&self) -> bool {
        matches!(
            self,
            Self::Incompatible(IncompatibleSource::ExcludeNewer(_))
        )
    }

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
    #[allow(clippy::match_like_matches_macro)]
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
                Self::MissingPlatform(_)
                | Self::NoBinary
                | Self::RequiresPython(_, _)
                | Self::Variant
                | Self::Tag(_)
                | Self::Yanked(_) => true,
            },
            Self::Variant => match other {
                Self::ExcludeNewer(_)
                | Self::Tag(_)
                | Self::RequiresPython(_, _)
                | Self::Yanked(_) => false,
                Self::Variant => false,
                Self::MissingPlatform(_) | Self::NoBinary => true,
            },
            Self::Tag(tag_self) => match other {
                Self::ExcludeNewer(_) => false,
                Self::Tag(tag_other) => tag_self > tag_other,
                Self::MissingPlatform(_)
                | Self::NoBinary
                | Self::RequiresPython(_, _)
                | Self::Variant
                | Self::Yanked(_) => true,
            },
            Self::RequiresPython(_, _) => match other {
                Self::ExcludeNewer(_) | Self::Tag(_) => false,
                // Version specifiers cannot be reasonably compared
                Self::RequiresPython(_, _) => false,
                Self::MissingPlatform(_) | Self::NoBinary | Self::Yanked(_) | Self::Variant => true,
            },
            Self::Yanked(_) => match other {
                Self::ExcludeNewer(_) | Self::Tag(_) | Self::RequiresPython(_, _) => false,
                // Yanks with a reason are more helpful for errors
                Self::Yanked(yanked_other) => matches!(yanked_other, Yanked::Reason(_)),
                Self::MissingPlatform(_) | Self::NoBinary | Self::Variant => true,
            },
            Self::NoBinary => match other {
                Self::ExcludeNewer(_)
                | Self::Tag(_)
                | Self::RequiresPython(_, _)
                | Self::Yanked(_)
                | Self::Variant => false,
                Self::NoBinary => false,
                Self::MissingPlatform(_) => true,
            },
            Self::MissingPlatform(_) => false,
        }
    }
}

/// Given a wheel filename, determine the set of supported markers.
pub fn implied_markers(filename: &WheelFilename) -> MarkerTree {
    let mut marker = implied_platform_markers(filename);
    marker.and(implied_python_markers(filename));

    marker
}

/// Given a wheel filename, determine the set of supported platforms, in terms of their markers.
///
/// This is roughly the inverse of platform tag generation: given a tag, we want to infer the
/// supported platforms (rather than generating the supported tags from a given platform).
fn implied_platform_markers(filename: &WheelFilename) -> MarkerTree {
    let mut marker = MarkerTree::FALSE;
    for platform_tag in filename.platform_tags() {
        match platform_tag {
            PlatformTag::Any => {
                return MarkerTree::TRUE;
            }

            // Windows
            PlatformTag::Win32 => {
                let mut tag_marker = MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::SysPlatform,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("win32"),
                });
                tag_marker.and(MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::PlatformMachine,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("x86"),
                }));
                marker.or(tag_marker);
            }
            PlatformTag::WinAmd64 => {
                let mut tag_marker = MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::SysPlatform,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("win32"),
                });
                tag_marker.and(MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::PlatformMachine,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("AMD64"),
                }));
                marker.or(tag_marker);
            }
            PlatformTag::WinArm64 => {
                let mut tag_marker = MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::SysPlatform,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("win32"),
                });
                tag_marker.and(MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::PlatformMachine,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("arm64"),
                }));
                marker.or(tag_marker);
            }

            // macOS
            PlatformTag::Macos { binary_format, .. } => {
                let mut tag_marker = MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::SysPlatform,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("darwin"),
                });

                // Extract the architecture from the end of the tag.
                let mut arch_marker = MarkerTree::FALSE;
                for arch in binary_format.platform_machine() {
                    arch_marker.or(MarkerTree::expression(MarkerExpression::String {
                        key: MarkerValueString::PlatformMachine,
                        operator: MarkerOperator::Equal,
                        value: ArcStr::from(arch.name()),
                    }));
                }
                tag_marker.and(arch_marker);

                marker.or(tag_marker);
            }

            // Linux
            PlatformTag::Manylinux { arch, .. }
            | PlatformTag::Manylinux1 { arch, .. }
            | PlatformTag::Manylinux2010 { arch, .. }
            | PlatformTag::Manylinux2014 { arch, .. }
            | PlatformTag::Musllinux { arch, .. }
            | PlatformTag::Linux { arch } => {
                let mut tag_marker = MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::SysPlatform,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("linux"),
                });
                tag_marker.and(MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::PlatformMachine,
                    operator: MarkerOperator::Equal,
                    value: ArcStr::from(arch.name()),
                }));
                marker.or(tag_marker);
            }

            tag => {
                debug!("Unknown platform tag in wheel tag: {tag}");
            }
        }
    }

    marker
}

/// Given a wheel filename, determine the set of supported Python versions, in terms of their markers.
///
/// This is roughly the inverse of Python tag generation: given a tag, we want to infer the
/// supported Python version (rather than generating the supported tags from a given Python version).
fn implied_python_markers(filename: &WheelFilename) -> MarkerTree {
    let mut marker = MarkerTree::FALSE;

    for python_tag in filename.python_tags() {
        // First, construct the version marker based on the tag
        let mut tree = match python_tag {
            LanguageTag::None => {
                // No Python tag means no Python version requirement.
                return MarkerTree::TRUE;
            }
            LanguageTag::Python { major, minor: None } => {
                MarkerTree::expression(MarkerExpression::Version {
                    key: uv_pep508::MarkerValueVersion::PythonVersion,
                    specifier: VersionSpecifier::equals_star_version(Version::new([u64::from(
                        *major,
                    )])),
                })
            }
            LanguageTag::Python {
                major,
                minor: Some(minor),
            }
            | LanguageTag::CPython {
                python_version: (major, minor),
            }
            | LanguageTag::PyPy {
                python_version: (major, minor),
            }
            | LanguageTag::GraalPy {
                python_version: (major, minor),
            }
            | LanguageTag::Pyston {
                python_version: (major, minor),
            } => MarkerTree::expression(MarkerExpression::Version {
                key: uv_pep508::MarkerValueVersion::PythonVersion,
                specifier: VersionSpecifier::equals_star_version(Version::new([
                    u64::from(*major),
                    u64::from(*minor),
                ])),
            }),
        };

        // Then, add implementation markers for implementation-specific tags
        match python_tag {
            LanguageTag::None | LanguageTag::Python { .. } => {
                // No implementation marker needed
            }
            LanguageTag::CPython { .. } => {
                tree.and(MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::PlatformPythonImplementation,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("CPython"),
                }));
            }
            LanguageTag::PyPy { .. } => {
                tree.and(MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::PlatformPythonImplementation,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("PyPy"),
                }));
            }
            LanguageTag::GraalPy { .. } => {
                tree.and(MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::PlatformPythonImplementation,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("GraalPy"),
                }));
            }
            LanguageTag::Pyston { .. } => {
                tree.and(MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::PlatformPythonImplementation,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("Pyston"),
                }));
            }
        }

        marker.or(tree);
    }

    marker
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[track_caller]
    fn assert_platform_markers(filename: &str, expected: &str) {
        let filename = WheelFilename::from_str(filename).unwrap();
        assert_eq!(
            implied_platform_markers(&filename),
            expected.parse::<MarkerTree>().unwrap()
        );
    }

    #[track_caller]
    fn assert_python_markers(filename: &str, expected: &str) {
        let filename = WheelFilename::from_str(filename).unwrap();
        assert_eq!(
            implied_python_markers(&filename),
            expected.parse::<MarkerTree>().unwrap()
        );
    }

    #[track_caller]
    fn assert_implied_markers(filename: &str, expected: &str) {
        let filename = WheelFilename::from_str(filename).unwrap();
        assert_eq!(
            implied_markers(&filename),
            expected.parse::<MarkerTree>().unwrap()
        );
    }

    #[test]
    fn test_implied_platform_markers() {
        let filename = WheelFilename::from_str("example-1.0-py3-none-any.whl").unwrap();
        assert_eq!(implied_platform_markers(&filename), MarkerTree::TRUE);

        assert_platform_markers(
            "example-1.0-cp310-cp310-win32.whl",
            "sys_platform == 'win32' and platform_machine == 'x86'",
        );
        assert_platform_markers(
            "numpy-2.2.1-cp313-cp313t-win_amd64.whl",
            "sys_platform == 'win32' and platform_machine == 'AMD64'",
        );
        assert_platform_markers(
            "numpy-2.2.1-cp313-cp313t-win_arm64.whl",
            "sys_platform == 'win32' and platform_machine == 'arm64'",
        );
        assert_platform_markers(
            "numpy-2.2.1-cp313-cp313t-manylinux_2_17_aarch64.manylinux2014_aarch64.whl",
            "sys_platform == 'linux' and platform_machine == 'aarch64'",
        );
        assert_platform_markers(
            "numpy-2.2.1-cp313-cp313t-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
            "sys_platform == 'linux' and platform_machine == 'x86_64'",
        );
        assert_platform_markers(
            "numpy-2.2.1-cp312-cp312-musllinux_1_2_aarch64.whl",
            "sys_platform == 'linux' and platform_machine == 'aarch64'",
        );
        assert_platform_markers(
            "numpy-2.2.1-cp310-cp310-macosx_14_0_x86_64.whl",
            "sys_platform == 'darwin' and platform_machine == 'x86_64'",
        );
        assert_platform_markers(
            "numpy-2.2.1-cp310-cp310-macosx_10_9_x86_64.whl",
            "sys_platform == 'darwin' and platform_machine == 'x86_64'",
        );
        assert_platform_markers(
            "numpy-2.2.1-cp310-cp310-macosx_11_0_arm64.whl",
            "sys_platform == 'darwin' and platform_machine == 'arm64'",
        );
    }

    #[test]
    fn test_implied_python_markers() {
        let filename = WheelFilename::from_str("example-1.0-none-none-any.whl").unwrap();
        assert_eq!(implied_python_markers(&filename), MarkerTree::TRUE);

        assert_python_markers(
            "example-1.0-cp310-cp310-any.whl",
            "python_full_version == '3.10.*' and platform_python_implementation == 'CPython'",
        );
        assert_python_markers(
            "example-1.0-cp311-cp311-any.whl",
            "python_full_version == '3.11.*' and platform_python_implementation == 'CPython'",
        );
        assert_python_markers(
            "example-1.0-cp312-cp312-any.whl",
            "python_full_version == '3.12.*' and platform_python_implementation == 'CPython'",
        );
        assert_python_markers(
            "example-1.0-cp313-cp313-any.whl",
            "python_full_version == '3.13.*' and platform_python_implementation == 'CPython'",
        );
        assert_python_markers(
            "example-1.0-cp313-cp313t-any.whl",
            "python_full_version == '3.13.*' and platform_python_implementation == 'CPython'",
        );
        assert_python_markers(
            "example-1.0-pp310-pypy310_pp73-any.whl",
            "python_full_version == '3.10.*' and platform_python_implementation == 'PyPy'",
        );
        assert_python_markers(
            "example-1.0-py310-none-any.whl",
            "python_full_version >= '3.10' and python_full_version < '3.11'",
        );
        assert_python_markers(
            "example-1.0-py3-none-any.whl",
            "python_full_version >= '3' and python_full_version < '4'",
        );
        assert_python_markers(
            "example-1.0-py311.py312-none-any.whl",
            "python_full_version >= '3.11' and python_full_version < '3.13'",
        );
    }

    #[test]
    fn test_implied_markers() {
        assert_implied_markers(
            "numpy-1.0-cp310-cp310-win32.whl",
            "python_full_version == '3.10.*' and platform_python_implementation == 'CPython' and sys_platform == 'win32' and platform_machine == 'x86'",
        );
        assert_implied_markers(
            "numpy-1.0-cp311-cp311-macosx_10_9_x86_64.whl",
            "python_full_version == '3.11.*' and platform_python_implementation == 'CPython' and sys_platform == 'darwin' and platform_machine == 'x86_64'",
        );
        assert_implied_markers(
            "numpy-1.0-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl",
            "python_full_version == '3.12.*' and platform_python_implementation == 'CPython' and sys_platform == 'linux' and platform_machine == 'aarch64'",
        );
        assert_implied_markers(
            "example-1.0-py3-none-any.whl",
            "python_full_version >= '3' and python_full_version < '4'",
        );
    }
}
