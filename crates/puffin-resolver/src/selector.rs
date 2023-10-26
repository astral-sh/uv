use fxhash::{FxHashMap, FxHashSet};
use pubgrub::range::Range;

use pep508_rs::{Requirement, VersionOrUrl};
use puffin_package::package_name::PackageName;

use crate::distribution::DistributionFile;
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

/// Like [`ResolutionMode`], but with any additional information required to select a candidate,
/// like the set of direct dependencies.
#[derive(Debug)]
enum ResolutionStrategy {
    /// Resolve the highest compatible version of each package.
    Highest,
    /// Resolve the lowest compatible version of each package.
    Lowest,
    /// Resolve the lowest compatible version of any direct dependencies, and the highest
    /// compatible version of any transitive dependencies.
    LowestDirect(FxHashSet<PackageName>),
}

impl ResolutionStrategy {
    fn from_mode(mode: ResolutionMode, direct_dependencies: &[Requirement]) -> Self {
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

/// A set of pinned packages that should be preserved during resolution, if possible.
#[derive(Debug)]
struct Preferences(FxHashMap<PackageName, PubGrubVersion>);

impl Preferences {
    fn get(&self, package_name: &PackageName) -> Option<&PubGrubVersion> {
        self.0.get(package_name)
    }
}

impl From<&[Requirement]> for Preferences {
    fn from(requirements: &[Requirement]) -> Self {
        Self(
            requirements
                .iter()
                .filter_map(|requirement| {
                    let Some(VersionOrUrl::VersionSpecifier(version_specifiers)) =
                        requirement.version_or_url.as_ref()
                    else {
                        return None;
                    };
                    let [version_specifier] = &**version_specifiers else {
                        return None;
                    };
                    let package_name = PackageName::normalize(&requirement.name);
                    let version = PubGrubVersion::from(version_specifier.version().clone());
                    Some((package_name, version))
                })
                .collect(),
        )
    }
}

#[derive(Debug)]
pub(crate) struct CandidateSelector {
    strategy: ResolutionStrategy,
    preferences: Preferences,
}

impl CandidateSelector {
    /// Return a candidate selector for the given resolution mode.
    pub(crate) fn from_mode(
        mode: ResolutionMode,
        direct_dependencies: &[Requirement],
        resolution: &[Requirement],
    ) -> Self {
        Self {
            strategy: ResolutionStrategy::from_mode(mode, direct_dependencies),
            preferences: Preferences::from(resolution),
        }
    }
}

impl CandidateSelector {
    /// Select a [`Candidate`] from a set of candidate versions and files.
    pub(crate) fn select(
        &self,
        package_name: &PackageName,
        range: &Range<PubGrubVersion>,
        version_map: &VersionMap,
    ) -> Option<Candidate> {
        if let Some(version) = self.preferences.get(package_name) {
            if range.contains(version) {
                if let Some(file) = version_map.get(version) {
                    return Some(Candidate {
                        package_name: package_name.clone(),
                        version: version.clone(),
                        file: file.clone(),
                    });
                }
            }
        }

        match &self.strategy {
            ResolutionStrategy::Highest => Self::select_highest(package_name, range, version_map),
            ResolutionStrategy::Lowest => Self::select_lowest(package_name, range, version_map),
            ResolutionStrategy::LowestDirect(direct_dependencies) => {
                if direct_dependencies.contains(package_name) {
                    Self::select_lowest(package_name, range, version_map)
                } else {
                    Self::select_highest(package_name, range, version_map)
                }
            }
        }
    }

    /// Select the highest-compatible [`Candidate`] from a set of candidate versions and files,
    /// preferring wheels over sdists.
    fn select_highest(
        package_name: &PackageName,
        range: &Range<PubGrubVersion>,
        version_map: &VersionMap,
    ) -> Option<Candidate> {
        let mut sdist = None;
        for (version, file) in version_map.iter().rev() {
            if range.contains(version) {
                match file {
                    DistributionFile::Wheel(_) => {
                        return Some(Candidate {
                            package_name: package_name.clone(),
                            version: version.clone(),
                            file: file.clone(),
                        });
                    }
                    DistributionFile::Sdist(_) if sdist.is_none() => {
                        sdist = Some(Candidate {
                            package_name: package_name.clone(),
                            version: version.clone(),
                            file: file.clone(),
                        });
                    }
                    DistributionFile::Sdist(_) => {
                        // We already selected a more recent source distribution
                    }
                }
            }
        }
        sdist
    }

    /// Select the highest-compatible [`Candidate`] from a set of candidate versions and files,
    /// preferring wheels over sdists.
    fn select_lowest(
        package_name: &PackageName,
        range: &Range<PubGrubVersion>,
        version_map: &VersionMap,
    ) -> Option<Candidate> {
        let mut sdist = None;
        for (version, file) in version_map {
            if range.contains(version) {
                match file {
                    DistributionFile::Wheel(_) => {
                        return Some(Candidate {
                            package_name: package_name.clone(),
                            version: version.clone(),
                            file: file.clone(),
                        });
                    }
                    DistributionFile::Sdist(_) => {
                        sdist = Some(Candidate {
                            package_name: package_name.clone(),
                            version: version.clone(),
                            file: file.clone(),
                        });
                    }
                }
            }
        }
        sdist
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Candidate {
    /// The name of the package.
    pub(crate) package_name: PackageName,
    /// The version of the package.
    pub(crate) version: PubGrubVersion,
    /// The file of the package.
    pub(crate) file: DistributionFile,
}
