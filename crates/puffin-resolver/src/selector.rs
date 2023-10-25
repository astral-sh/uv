use fxhash::FxHashSet;
use pubgrub::range::Range;

use crate::distribution::DistributionFile;
use pep508_rs::Requirement;

use puffin_package::package_name::PackageName;

use crate::pubgrub::version::PubGrubVersion;
use crate::resolver::VersionMap;

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
    /// Select a [`Candidate`] from a set of candidate versions and files.
    pub(crate) fn select<'a>(
        &self,
        package_name: &'a PackageName,
        range: &'a Range<PubGrubVersion>,
        version_map: &'a VersionMap,
    ) -> Option<Candidate<'a>> {
        match self {
            CandidateSelector::Highest => {
                CandidateSelector::select_highest(package_name, range, version_map)
            }
            CandidateSelector::Lowest => {
                CandidateSelector::select_lowest(package_name, range, version_map)
            }
            CandidateSelector::LowestDirect(direct_dependencies) => {
                if direct_dependencies.contains(package_name) {
                    CandidateSelector::select_lowest(package_name, range, version_map)
                } else {
                    CandidateSelector::select_highest(package_name, range, version_map)
                }
            }
        }
    }

    /// Select the highest-compatible [`Candidate`] from a set of candidate versions and files,
    /// preferring wheels over sdists.
    fn select_highest<'a>(
        package_name: &'a PackageName,
        range: &'a Range<PubGrubVersion>,
        version_map: &'a VersionMap,
    ) -> Option<Candidate<'a>> {
        let mut sdist = None;
        for (version, file) in version_map.iter().rev() {
            if range.contains(version) {
                match file {
                    DistributionFile::Wheel(_) => {
                        return Some(Candidate {
                            package_name,
                            version,
                            file,
                        });
                    }
                    DistributionFile::Sdist(_) => {
                        sdist = Some(Candidate {
                            package_name,
                            version,
                            file,
                        });
                    }
                }
            }
        }
        sdist
    }

    /// Select the highest-compatible [`Candidate`] from a set of candidate versions and files,
    /// preferring wheels over sdists.
    fn select_lowest<'a>(
        package_name: &'a PackageName,
        range: &'a Range<PubGrubVersion>,
        version_map: &'a VersionMap,
    ) -> Option<Candidate<'a>> {
        let mut sdist = None;
        for (version, file) in version_map {
            if range.contains(version) {
                match file {
                    DistributionFile::Wheel(_) => {
                        return Some(Candidate {
                            package_name,
                            version,
                            file,
                        });
                    }
                    DistributionFile::Sdist(_) => {
                        sdist = Some(Candidate {
                            package_name,
                            version,
                            file,
                        });
                    }
                }
            }
        }
        sdist
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Candidate<'a> {
    /// The name of the package.
    pub(crate) package_name: &'a PackageName,
    /// The version of the package.
    pub(crate) version: &'a PubGrubVersion,
    /// The file of the package.
    pub(crate) file: &'a DistributionFile,
}
