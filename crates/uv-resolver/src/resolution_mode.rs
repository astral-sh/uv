use rustc_hash::FxHashSet;

use uv_normalize::PackageName;

use crate::{DependencyMode, Manifest, ResolverMarkers};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ResolutionMode {
    /// Resolve the highest compatible version of each package.
    #[default]
    Highest,
    /// Resolve the lowest compatible version of each package.
    Lowest,
    /// Resolve the lowest compatible version of any direct dependencies, and the highest
    /// compatible version of any transitive dependencies.
    LowestDirect,
}

impl std::fmt::Display for ResolutionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Highest => write!(f, "highest"),
            Self::Lowest => write!(f, "lowest"),
            Self::LowestDirect => write!(f, "lowest-direct"),
        }
    }
}

/// Like [`ResolutionMode`], but with any additional information required to select a candidate,
/// like the set of direct dependencies.
#[derive(Debug, Clone)]
pub(crate) enum ResolutionStrategy {
    /// Resolve the highest compatible version of each package.
    Highest,
    /// Resolve the lowest compatible version of each package.
    Lowest,
    /// Resolve the lowest compatible version of any direct dependencies, and the highest
    /// compatible version of any transitive dependencies.
    LowestDirect(FxHashSet<PackageName>),
}

impl ResolutionStrategy {
    pub(crate) fn from_mode(
        mode: ResolutionMode,
        manifest: &Manifest,
        markers: &ResolverMarkers,
        dependencies: DependencyMode,
    ) -> Self {
        match mode {
            ResolutionMode::Highest => Self::Highest,
            ResolutionMode::Lowest => Self::Lowest,
            ResolutionMode::LowestDirect => Self::LowestDirect(
                manifest
                    .user_requirements(markers, dependencies)
                    .map(|requirement| requirement.name.clone())
                    .collect(),
            ),
        }
    }
}
