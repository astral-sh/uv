use pubgrub::range::Range;

use distribution_types::{CompatibleDist, IncompatibleDist, IncompatibleSource};
use distribution_types::{DistributionMetadata, IncompatibleWheel, Name, PrioritizedDist};
use pep440_rs::Version;
use pep508_rs::MarkerEnvironment;
use tracing::debug;
use uv_normalize::PackageName;
use uv_types::InstalledPackagesProvider;

use crate::preferences::Preferences;
use crate::prerelease_mode::PreReleaseStrategy;
use crate::resolution_mode::ResolutionStrategy;
use crate::version_map::{VersionMap, VersionMapDistHandle};
use crate::{Exclusions, Manifest, Options};

#[derive(Debug, Clone)]
pub(crate) struct CandidateSelector {
    resolution_strategy: ResolutionStrategy,
    prerelease_strategy: PreReleaseStrategy,
}

impl CandidateSelector {
    /// Return a [`CandidateSelector`] for the given [`Manifest`].
    pub(crate) fn for_resolution(
        options: Options,
        manifest: &Manifest,
        markers: &MarkerEnvironment,
    ) -> Self {
        Self {
            resolution_strategy: ResolutionStrategy::from_mode(
                options.resolution_mode,
                manifest,
                markers,
            ),
            prerelease_strategy: PreReleaseStrategy::from_mode(
                options.prerelease_mode,
                manifest,
                markers,
            ),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum AllowPreRelease {
    Yes,
    No,
    IfNecessary,
}

impl CandidateSelector {
    /// Select a [`Candidate`] from a set of candidate versions and files.
    ///
    /// Unless present in the provided [`Exclusions`], local distributions from the
    /// [`InstalledPackagesProvider`] are preferred over remote distributions in
    /// the [`VersionMap`].
    pub(crate) fn select<'a, InstalledPackages: InstalledPackagesProvider>(
        &'a self,
        package_name: &'a PackageName,
        range: &'a Range<Version>,
        version_map: &'a VersionMap,
        preferences: &'a Preferences,
        installed_packages: &'a InstalledPackages,
        exclusions: &'a Exclusions,
    ) -> Option<Candidate<'a>> {
        // If the package has a preference (e.g., an existing version from an existing lockfile),
        // and the preference satisfies the current range, use that.
        if let Some(version) = preferences.version(package_name) {
            if range.contains(version) {
                // Check for a locally installed distribution that matches the preferred version
                if !exclusions.contains(package_name) {
                    let installed_dists = installed_packages.get_packages(package_name);
                    match installed_dists.as_slice() {
                        [] => {}
                        [dist] => {
                            if dist.version() == version {
                                debug!("Found installed version of {dist} that satisfies preference in {range}");

                                return Some(Candidate {
                                    name: package_name,
                                    version,
                                    dist: CandidateDist::Compatible(CompatibleDist::InstalledDist(
                                        dist,
                                    )),
                                });
                            }
                        }
                        // We do not consider installed distributions with multiple versions because
                        // during installation these must be reinstalled from the remote
                        _ => {
                            debug!("Ignoring installed versions of {package_name}: multiple distributions found");
                        }
                    }
                }

                // Check for a remote distribution that matches the preferred version
                if let Some(file) = version_map.get(version) {
                    return Some(Candidate::new(package_name, version, file));
                }
            }
        }

        // Check for a locally installed distribution that satisfies the range
        if !exclusions.contains(package_name) {
            let installed_dists = installed_packages.get_packages(package_name);
            match installed_dists.as_slice() {
                [] => {}
                [dist] => {
                    let version = dist.version();
                    if range.contains(version) {
                        debug!("Found installed version of {dist} that satisfies {range}");

                        return Some(Candidate {
                            name: package_name,
                            version,
                            dist: CandidateDist::Compatible(CompatibleDist::InstalledDist(dist)),
                        });
                    }
                }
                // We do not consider installed distributions with multiple versions because
                // during installation these must be reinstalled from the remote
                _ => {
                    debug!("Ignoring installed versions of {package_name}: multiple distributions found");
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
            "selecting candidate for package {:?} with range {:?} with {} remote versions",
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
        versions: impl Iterator<Item = (&'a Version, VersionMapDistHandle<'a>)>,
        package_name: &'a PackageName,
        range: &Range<Version>,
        allow_prerelease: AllowPreRelease,
    ) -> Option<Candidate<'a>> {
        #[derive(Debug)]
        enum PreReleaseCandidate<'a> {
            NotNecessary,
            IfNecessary(&'a Version, &'a PrioritizedDist),
        }

        let mut prerelease = None;
        let mut steps = 0;
        for (version, maybe_dist) in versions {
            steps += 1;

            let candidate = if version.any_prerelease() {
                if range.contains(version) {
                    match allow_prerelease {
                        AllowPreRelease::Yes => {
                            let Some(dist) = maybe_dist.prioritized_dist() else {
                                continue;
                            };
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
                            Candidate::new(package_name, version, dist)
                        }
                        AllowPreRelease::IfNecessary => {
                            let Some(dist) = maybe_dist.prioritized_dist() else {
                                continue;
                            };
                            // If pre-releases are allowed as a fallback, store the
                            // first-matching prerelease.
                            if prerelease.is_none() {
                                prerelease = Some(PreReleaseCandidate::IfNecessary(version, dist));
                            }
                            continue;
                        }
                        AllowPreRelease::No => {
                            continue;
                        }
                    }
                } else {
                    continue;
                }
            } else {
                // If we have at least one stable release, we shouldn't allow the "if-necessary"
                // pre-release strategy, regardless of whether that stable release satisfies the
                // current range.
                prerelease = Some(PreReleaseCandidate::NotNecessary);

                // Return the first-matching stable distribution.
                if range.contains(version) {
                    let Some(dist) = maybe_dist.prioritized_dist() else {
                        continue;
                    };
                    tracing::trace!(
                        "found candidate for package {:?} with range {:?} \
                         after {} steps: {:?} version",
                        package_name,
                        range,
                        steps,
                        version,
                    );
                    Candidate::new(package_name, version, dist)
                } else {
                    continue;
                }
            };

            // If candidate is not compatible due to exclude newer, continue searching.
            // This is a special case — we pretend versions with exclude newer incompatibilities
            // do not exist so that they are not present in error messages in our test suite.
            // TODO(zanieb): Now that `--exclude-newer` is user facing we may want to consider
            // flagging this behavior such that we _will_ report filtered distributions due to
            // exclude-newer in our error messages.
            if matches!(
                candidate.dist(),
                CandidateDist::Incompatible(
                    IncompatibleDist::Source(IncompatibleSource::ExcludeNewer(_))
                        | IncompatibleDist::Wheel(IncompatibleWheel::ExcludeNewer(_))
                )
            ) {
                continue;
            }

            return Some(candidate);
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
            Some(PreReleaseCandidate::IfNecessary(version, dist)) => {
                Some(Candidate::new(package_name, version, dist))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum CandidateDist<'a> {
    Compatible(CompatibleDist<'a>),
    Incompatible(IncompatibleDist),
}

impl<'a> From<&'a PrioritizedDist> for CandidateDist<'a> {
    fn from(value: &'a PrioritizedDist) -> Self {
        if let Some(dist) = value.get() {
            CandidateDist::Compatible(dist)
        } else {
            // TODO(zanieb)
            // We always return the source distribution (if one exists) instead of the wheel
            // but in the future we may want to return both so the resolver can explain
            // why neither distribution kind can be used.
            let dist = if let Some((_, incompatibility)) = value.incompatible_source() {
                IncompatibleDist::Source(incompatibility.clone())
            } else if let Some((_, incompatibility)) = value.incompatible_wheel() {
                IncompatibleDist::Wheel(incompatibility.clone())
            } else {
                IncompatibleDist::Unavailable
            };
            CandidateDist::Incompatible(dist)
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Candidate<'a> {
    /// The name of the package.
    name: &'a PackageName,
    /// The version of the package.
    version: &'a Version,
    /// The distributions to use for resolving and installing the package.
    dist: CandidateDist<'a>,
}

impl<'a> Candidate<'a> {
    fn new(name: &'a PackageName, version: &'a Version, dist: &'a PrioritizedDist) -> Self {
        Self {
            name,
            version,
            dist: CandidateDist::from(dist),
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

    /// Return the distribution for the package, if compatible.
    pub(crate) fn compatible(&self) -> Option<&CompatibleDist<'a>> {
        if let CandidateDist::Compatible(ref dist) = self.dist {
            Some(dist)
        } else {
            None
        }
    }

    /// Return the distribution for the candidate.
    pub(crate) fn dist(&self) -> &CandidateDist<'a> {
        &self.dist
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
