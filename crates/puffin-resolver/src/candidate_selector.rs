use pubgrub::range::Range;
use pypi_types::Yanked;
use rustc_hash::FxHashMap;

use distribution_types::{Dist, DistributionMetadata, Name};
use distribution_types::{DistMetadata, ResolvableDist};
use pep440_rs::{Version, VersionSpecifiers};
use pep508_rs::{Requirement, VersionOrUrl};
use puffin_normalize::PackageName;

use crate::prerelease_mode::PreReleaseStrategy;
use crate::python_requirement::PythonRequirement;
use crate::resolution_mode::ResolutionStrategy;
use crate::version_map::VersionMap;
use crate::{Manifest, Options};

#[derive(Debug, Clone)]
pub(crate) struct CandidateSelector {
    resolution_strategy: ResolutionStrategy,
    prerelease_strategy: PreReleaseStrategy,
    preferences: Preferences,
}

impl CandidateSelector {
    /// Return a [`CandidateSelector`] for the given [`Manifest`].
    pub(crate) fn for_resolution(manifest: &Manifest, options: Options) -> Self {
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

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn resolution_strategy(&self) -> &ResolutionStrategy {
        &self.resolution_strategy
    }

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn prerelease_strategy(&self) -> &PreReleaseStrategy {
        &self.prerelease_strategy
    }
}

/// A set of pinned packages that should be preserved during resolution, if possible.
#[derive(Debug, Clone)]
struct Preferences(FxHashMap<PackageName, Version>);

impl Preferences {
    fn get(&self, package_name: &PackageName) -> Option<&Version> {
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
                    Some((
                        requirement.name.clone(),
                        version_specifier.version().clone(),
                    ))
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
        range: &Range<Version>,
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

        tracing::trace!(
            "selecting candidate for package {:?} with range {:?} with {} versions",
            package_name,
            range,
            version_map.len()
        );
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
    /// preferring wheels over source distributions.
    fn select_candidate<'a>(
        versions: impl Iterator<Item = (&'a Version, ResolvableDist<'a>)>,
        package_name: &'a PackageName,
        range: &Range<Version>,
        allow_prerelease: AllowPreRelease,
    ) -> Option<Candidate<'a>> {
        #[derive(Debug)]
        enum PreReleaseCandidate<'a> {
            NotNecessary,
            IfNecessary(&'a Version, ResolvableDist<'a>),
        }

        let mut prerelease = None;
        let mut steps = 0;
        for (version, file) in versions {
            steps += 1;
            if version.any_prerelease() {
                if range.contains(version) {
                    match allow_prerelease {
                        AllowPreRelease::Yes => {
                            tracing::trace!(
                                "found candidate for package {:?} with range {:?} \
                                 after {} steps: {:?} version",
                                package_name,
                                range,
                                steps,
                                version,
                            );
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
                    tracing::trace!(
                        "found candidate for package {:?} with range {:?} \
                         after {} steps: {:?} version",
                        package_name,
                        range,
                        steps,
                        version,
                    );
                    return Some(Candidate::new(package_name, version, file));
                }
            }
        }
        tracing::trace!(
            "exhausted all candidates for package {:?} with range {:?} \
             after {} steps",
            package_name,
            range,
            steps,
        );
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
    version: &'a Version,
    /// The file to use for resolving and installing the package.
    dist: ResolvableDist<'a>,
}

impl<'a> Candidate<'a> {
    fn new(name: &'a PackageName, version: &'a Version, dist: ResolvableDist<'a>) -> Self {
        Self {
            name,
            version,
            dist,
        }
    }

    /// Return the name of the package.
    pub(crate) fn name(&self) -> &PackageName {
        self.name
    }

    /// Return the version of the package.
    pub(crate) fn version(&self) -> &Version {
        self.version
    }

    /// Return the [`DistFile`] to use when resolving the package.
    pub(crate) fn resolution_dist(&self) -> &DistMetadata {
        self.dist.for_resolution()
    }

    /// Return the [`DistFile`] to use when installing the package.
    pub(crate) fn installation_dist(&self) -> &DistMetadata {
        self.dist.for_installation()
    }

    /// If the candidate doesn't match the given Python requirement, return the version specifiers.
    pub(crate) fn validate_python(
        &self,
        requirement: &PythonRequirement,
    ) -> Option<&VersionSpecifiers> {
        // Validate the _installed_ file.
        let requires_python = self.installation_dist().requires_python.as_ref()?;

        // If the candidate doesn't support the target Python version, return the failing version
        // specifiers.
        if !requires_python.contains(requirement.target()) {
            return Some(requires_python);
        }

        // If the candidate is a source distribution, and doesn't support the installed Python
        // version, return the failing version specifiers, since we won't be able to build it.
        if matches!(self.installation_dist().dist, Dist::Source(_)) {
            if !requires_python.contains(requirement.installed()) {
                return Some(requires_python);
            }
        }

        // Validate the resolved file.
        let requires_python = self.resolution_dist().requires_python.as_ref()?;

        // If the candidate is a source distribution, and doesn't support the installed Python
        // version, return the failing version specifiers, since we won't be able to build it.
        // This isn't strictly necessary, since if `self.resolve()` is a source distribution, it
        // should be the same file as `self.install()` (validated above).
        if matches!(self.resolution_dist().dist, Dist::Source(_)) {
            if !requires_python.contains(requirement.installed()) {
                return Some(requires_python);
            }
        }

        None
    }

    /// If the distribution that would be installed is yanked.
    pub(crate) fn yanked(&self) -> &Yanked {
        self.dist.yanked()
    }
}

impl Name for Candidate<'_> {
    fn name(&self) -> &PackageName {
        self.name
    }
}

impl DistributionMetadata for Candidate<'_> {
    fn version_or_url(&self) -> distribution_types::VersionOrUrl {
        distribution_types::VersionOrUrl::Version(self.version)
    }
}
