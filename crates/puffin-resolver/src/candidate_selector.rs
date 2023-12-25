use pubgrub::range::Range;
use rustc_hash::FxHashMap;

use distribution_types::{Dist, DistributionMetadata, IndexUrl, Name};
use pep508_rs::{Requirement, VersionOrUrl};
use puffin_normalize::PackageName;

use crate::file::DistFile;
use crate::prerelease_mode::PreReleaseStrategy;
use crate::pubgrub::PubGrubVersion;
use crate::resolution_mode::ResolutionStrategy;
use crate::version_map::{ResolvableFile, VersionMap};
use crate::{Manifest, ResolutionOptions};

#[derive(Debug)]
pub(crate) struct CandidateSelector {
    resolution_strategy: ResolutionStrategy,
    prerelease_strategy: PreReleaseStrategy,
    preferences: Preferences,
}

impl CandidateSelector {
    /// Return a [`CandidateSelector`] for the given [`Manifest`].
    pub(crate) fn for_resolution(manifest: &Manifest, options: ResolutionOptions) -> Self {
        Self {
            resolution_strategy: ResolutionStrategy::from_mode(
                options.resolution_mode,
                manifest.requirements.as_slice(),
            ),
            prerelease_strategy: PreReleaseStrategy::from_mode(
                options.prerelease_mode,
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
                    let [version_specifier] = version_specifiers.as_ref() else {
                        return None;
                    };
                    let version = PubGrubVersion::from(version_specifier.version().clone());
                    Some((requirement.name.clone(), version))
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
    pub(crate) fn select<'a>(
        &'a self,
        package_name: &'a PackageName,
        range: &Range<PubGrubVersion>,
        version_map: &'a VersionMap,
    ) -> Option<Candidate<'a>> {
        // If the package has a preference (e.g., an existing version from an existing lockfile),
        // and the preference satisfies the current range, use that.
        if let Some(version) = self.preferences.get(package_name) {
            if range.contains(version) {
                if let Some(file) = version_map.get(version) {
                    return Some(Candidate::new(package_name, version, file));
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
        versions: impl Iterator<Item = (&'a PubGrubVersion, ResolvableFile<'a>)>,
        package_name: &'a PackageName,
        range: &Range<PubGrubVersion>,
        allow_prerelease: AllowPreRelease,
    ) -> Option<Candidate<'a>> {
        #[derive(Debug)]
        enum PreReleaseCandidate<'a> {
            NotNecessary,
            IfNecessary(&'a PubGrubVersion, ResolvableFile<'a>),
        }

        let mut prerelease = None;
        for (version, file) in versions {
            if version.any_prerelease() {
                if range.contains(version) {
                    match allow_prerelease {
                        AllowPreRelease::Yes => {
                            // If pre-releases are allowed, treat them equivalently
                            // to stable distributions.
                            return Some(Candidate::new(package_name, version, file));
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
                    return Some(Candidate::new(package_name, version, file));
                }
            }
        }
        match prerelease {
            None => None,
            Some(PreReleaseCandidate::NotNecessary) => None,
            Some(PreReleaseCandidate::IfNecessary(version, file)) => {
                Some(Candidate::new(package_name, version, file))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Candidate<'a> {
    /// The name of the package.
    name: &'a PackageName,
    /// The version of the package.
    version: &'a PubGrubVersion,
    /// The file to use for resolving and installing the package.
    file: ResolvableFile<'a>,
}

impl<'a> Candidate<'a> {
    fn new(name: &'a PackageName, version: &'a PubGrubVersion, file: ResolvableFile<'a>) -> Self {
        Self {
            name,
            version,
            file,
        }
    }

    /// Return the name of the package.
    pub(crate) fn name(&self) -> &PackageName {
        self.name
    }

    /// Return the version of the package.
    pub(crate) fn version(&self) -> &PubGrubVersion {
        self.version
    }

    /// Return the [`DistFile`] to use when resolving the package.
    pub(crate) fn resolve(&self) -> &DistFile {
        self.file.resolve()
    }

    /// Return the [`DistFile`] to use when installing the package.
    pub(crate) fn install(&self) -> &DistFile {
        self.file.install()
    }

    /// Return the [`Dist`] to use when resolving the candidate.
    pub(crate) fn into_distribution(self, index: IndexUrl) -> Dist {
        Dist::from_registry(
            self.name().clone(),
            self.version().clone().into(),
            self.resolve().clone().into(),
            index,
        )
    }
}

impl Name for Candidate<'_> {
    fn name(&self) -> &PackageName {
        self.name
    }
}

impl DistributionMetadata for Candidate<'_> {
    fn version_or_url(&self) -> distribution_types::VersionOrUrl {
        distribution_types::VersionOrUrl::Version(self.version.into())
    }
}
