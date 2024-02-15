use rustc_hash::FxHashSet;

use pep508_rs::Requirement;
use uv_normalize::PackageName;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
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
    pub(crate) fn from_mode(mode: ResolutionMode, direct_dependencies: &[Requirement]) -> Self {
        match mode {
            ResolutionMode::Highest => Self::Highest,
            ResolutionMode::Lowest => Self::Lowest,
            ResolutionMode::LowestDirect => Self::LowestDirect(
                direct_dependencies
                    .iter()
                    .map(|requirement| requirement.name.clone())
                    .collect(),
            ),
        }
    }
}
