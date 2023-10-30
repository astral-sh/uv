use fxhash::FxHashMap;
use pubgrub::range::Range;

use pep508_rs::{Requirement, VersionOrUrl};
use puffin_package::package_name::PackageName;

use crate::distribution::DistributionFile;
use crate::prerelease_mode::PreReleaseStrategy;
use crate::pubgrub::PubGrubVersion;
use crate::resolution_mode::ResolutionStrategy;
use crate::resolver::VersionMap;
use crate::Manifest;

#[derive(Debug)]
pub(crate) struct CandidateSelector {
    resolution_strategy: ResolutionStrategy,
    prerelease_strategy: PreReleaseStrategy,
    preferences: Preferences,
}

impl From<&Manifest> for CandidateSelector {
    /// Return a [`CandidateSelector`] for the given [`Manifest`].
    fn from(manifest: &Manifest) -> Self {
        Self {
            resolution_strategy: ResolutionStrategy::from_mode(
                manifest.resolution_mode,
                manifest.requirements.as_slice(),
            ),
            prerelease_strategy: PreReleaseStrategy::from_mode(
                manifest.prerelease_mode,
                manifest.requirements.as_slice(),
            ),
            preferences: Preferences::from(manifest.preferences.as_slice()),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum AllowPreRelease {
    Yes,
    No,
    IfNecessary,
}

impl CandidateSelector {
    /// Select a [`Candidate`] from a set of candidate versions and files.
    pub(crate) fn select(
        &self,
        package_name: &PackageName,
        range: &Range<PubGrubVersion>,
        version_map: &VersionMap,
    ) -> Option<Candidate> {
        // If the package has a preference (e.g., an existing version from an existing lockfile),
        // and the preference satisfies the current range, use that.
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

        // Determine the appropriate prerelease strategy for the current package.
        let allow_prerelease = match &self.prerelease_strategy {
            PreReleaseStrategy::Disallow => AllowPreRelease::No,
            PreReleaseStrategy::Allow => AllowPreRelease::Yes,
            PreReleaseStrategy::IfNecessary => AllowPreRelease::IfNecessary,
            PreReleaseStrategy::Explicit(packages) => {
                if packages.contains(package_name) {
                    AllowPreRelease::Yes
                } else {
                    AllowPreRelease::No
                }
            }
            PreReleaseStrategy::IfNecessaryOrExplicit(packages) => {
                if packages.contains(package_name) {
                    AllowPreRelease::Yes
                } else {
                    AllowPreRelease::IfNecessary
                }
            }
        };

        match &self.resolution_strategy {
            ResolutionStrategy::Highest => Self::select_candidate(
                version_map.iter().rev(),
                package_name,
                range,
                allow_prerelease,
            ),
            ResolutionStrategy::Lowest => {
                Self::select_candidate(version_map.iter(), package_name, range, allow_prerelease)
            }
            ResolutionStrategy::LowestDirect(direct_dependencies) => {
                if direct_dependencies.contains(package_name) {
                    Self::select_candidate(
                        version_map.iter(),
                        package_name,
                        range,
                        allow_prerelease,
                    )
                } else {
                    Self::select_candidate(
                        version_map.iter().rev(),
                        package_name,
                        range,
                        allow_prerelease,
                    )
                }
            }
        }
    }

    /// Select the first-matching [`Candidate`] from a set of candidate versions and files,
    /// preferring wheels over sdists.
    fn select_candidate<'a>(
        versions: impl Iterator<Item = (&'a PubGrubVersion, &'a DistributionFile)>,
        package_name: &PackageName,
        range: &Range<PubGrubVersion>,
        allow_prerelease: AllowPreRelease,
    ) -> Option<Candidate> {
        #[derive(Debug)]
        enum PreReleaseCandidate<'a> {
            NotNecessary,
            IfNecessary(&'a PubGrubVersion, &'a DistributionFile),
        }

        let mut prerelease = None;
        for (version, file) in versions {
            if version.any_prerelease() {
                if range.contains(version) {
                    match allow_prerelease {
                        AllowPreRelease::Yes => {
                            // If pre-releases are allowed, treat them equivalently
                            // to stable distributions.
                            return Some(Candidate {
                                package_name: package_name.clone(),
                                version: version.clone(),
                                file: file.clone(),
                            });
                        }
                        AllowPreRelease::IfNecessary => {
                            // If pre-releases are allowed as a fallback, store the
                            // first-matching prerelease.
                            if prerelease.is_none() {
                                prerelease = Some(PreReleaseCandidate::IfNecessary(version, file));
                            }
                        }
                        AllowPreRelease::No => {
                            continue;
                        }
                    }
                }
            } else {
                // If we have at least one stable release, we shouldn't allow the "if-necessary"
                // pre-release strategy, regardless of whether that stable release satisfies the
                // current range.
                prerelease = Some(PreReleaseCandidate::NotNecessary);

                // Always return the first-matching stable distribution.
                if range.contains(version) {
                    return Some(Candidate {
                        package_name: package_name.clone(),
                        version: version.clone(),
                        file: file.clone(),
                    });
                }
            }
        }
        match prerelease {
            None => None,
            Some(PreReleaseCandidate::NotNecessary) => None,
            Some(PreReleaseCandidate::IfNecessary(version, file)) => Some(Candidate {
                package_name: package_name.clone(),
                version: version.clone(),
                file: file.clone(),
            }),
        }
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
