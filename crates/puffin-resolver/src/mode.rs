use fxhash::FxHashSet;
use itertools::Either;

use pep508_rs::Requirement;
use puffin_client::File;
use puffin_package::package_name::PackageName;

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

#[derive(Debug, Clone)]
pub(crate) enum CandidateSelector {
    /// Resolve the highest compatible version of each package.
    Highest,
    /// Resolve the lowest compatible version of each package.
    Lowest,
    /// Resolve the lowest compatible version of any direct dependencies, and the highest
    /// compatible version of any transitive dependencies.
    LowestDirect(FxHashSet<PackageName>),
}

impl CandidateSelector {
    /// Return a candidate selector for the given resolution mode.
    pub(crate) fn from_mode(mode: ResolutionMode, direct_dependencies: &[Requirement]) -> Self {
        match mode {
            ResolutionMode::Highest => Self::Highest,
            ResolutionMode::Lowest => Self::Lowest,
            ResolutionMode::LowestDirect => Self::LowestDirect(
                direct_dependencies
                    .iter()
                    .map(|requirement| PackageName::normalize(&requirement.name))
                    .collect(),
            ),
        }
    }
}

impl CandidateSelector {
    /// Return an iterator over the candidates for the given package name.
    pub(crate) fn iter_candidates<'a>(
        &self,
        package_name: &PackageName,
        candidates: &'a [File],
    ) -> impl Iterator<Item = &'a File> {
        match self {
            CandidateSelector::Highest => Either::Left(candidates.iter().rev()),
            CandidateSelector::Lowest => Either::Right(candidates.iter()),
            CandidateSelector::LowestDirect(direct_dependencies) => {
                if direct_dependencies.contains(package_name) {
                    Either::Right(candidates.iter())
                } else {
                    Either::Left(candidates.iter().rev())
                }
            }
        }
    }
}
