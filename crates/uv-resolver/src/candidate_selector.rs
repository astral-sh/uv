use itertools::Itertools;
use pubgrub::range::Range;
use std::fmt::{Display, Formatter};
use tracing::debug;

use distribution_types::{CompatibleDist, IncompatibleDist, IncompatibleSource};
use distribution_types::{DistributionMetadata, IncompatibleWheel, Name, PrioritizedDist};
use pep440_rs::Version;
use pep508_rs::MarkerEnvironment;
use uv_configuration::IndexStrategy;
use uv_normalize::PackageName;
use uv_types::InstalledPackagesProvider;

use crate::preferences::Preferences;
use crate::prerelease_mode::{AllowPreRelease, PreReleaseStrategy};
use crate::resolution_mode::ResolutionStrategy;
use crate::version_map::{VersionMap, VersionMapDistHandle};
use crate::{Exclusions, Manifest, Options, ResolverMarkers};

#[derive(Debug, Clone)]
#[allow(clippy::struct_field_names)]
pub(crate) struct CandidateSelector {
    resolution_strategy: ResolutionStrategy,
    prerelease_strategy: PreReleaseStrategy,
    index_strategy: IndexStrategy,
}

impl CandidateSelector {
    /// Return a [`CandidateSelector`] for the given [`Manifest`].
    pub(crate) fn for_resolution(
        options: Options,
        manifest: &Manifest,
        markers: Option<&MarkerEnvironment>,
    ) -> Self {
        Self {
            resolution_strategy: ResolutionStrategy::from_mode(
                options.resolution_mode,
                manifest,
                markers,
                options.dependency_mode,
            ),
            prerelease_strategy: PreReleaseStrategy::from_mode(
                options.prerelease_mode,
                manifest,
                markers,
                options.dependency_mode,
            ),
            index_strategy: options.index_strategy,
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

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn index_strategy(&self) -> &IndexStrategy {
        &self.index_strategy
    }
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
        range: &Range<Version>,
        version_maps: &'a [VersionMap],
        preferences: &'a Preferences,
        installed_packages: &'a InstalledPackages,
        exclusions: &'a Exclusions,
        markers: &ResolverMarkers,
    ) -> Option<Candidate<'a>> {
        if let Some(preferred) = self.get_preferred(
            package_name,
            range,
            version_maps,
            preferences,
            installed_packages,
            exclusions,
            markers,
        ) {
            return Some(preferred);
        }

        if !exclusions.contains(package_name) {
            if let Some(installed) = Self::get_installed(package_name, range, installed_packages) {
                return Some(installed);
            }
        }

        self.select_no_preference(package_name, range, version_maps, markers)
    }

    /// Check for a preference (e.g., an existing version from an existing lockfile or
    /// from a previous fork) that satisfies the current range.
    fn get_preferred<'a, InstalledPackages: InstalledPackagesProvider>(
        &self,
        package_name: &'a PackageName,
        range: &Range<Version>,
        version_maps: &'a [VersionMap],
        preferences: &'a Preferences,
        installed_packages: &'a InstalledPackages,
        exclusions: &'a Exclusions,
        markers: &ResolverMarkers,
    ) -> Option<Candidate<'a>> {
        let version = preferences.version(package_name)?;

        // Respect the version range for this requirement.
        if !range.contains(version) {
            return None;
        }

        // Check for a locally installed distribution that matches the preferred version.
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
                            dist: CandidateDist::Compatible(CompatibleDist::InstalledDist(dist)),
                            choice_kind: VersionChoiceKind::Preference,
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

        // Respect the pre-release strategy for this fork.
        if version.any_prerelease()
            && self.prerelease_strategy.allows(package_name, markers) != AllowPreRelease::Yes
        {
            return None;
        }

        // Check for a remote distribution that matches the preferred version
        if let Some(file) = version_maps
            .iter()
            .find_map(|version_map| version_map.get(version))
        {
            return Some(Candidate::new(
                package_name,
                version,
                file,
                VersionChoiceKind::Preference,
            ));
        }

        None
    }

    /// Check for a locally installed distribution that satisfies the range.
    fn get_installed<'a, InstalledPackages: InstalledPackagesProvider>(
        package_name: &'a PackageName,
        range: &Range<Version>,
        installed_packages: &'a InstalledPackages,
    ) -> Option<Candidate<'a>> {
        let installed_dists = installed_packages.get_packages(package_name);
        match installed_dists.as_slice() {
            [] => {}
            [dist] => {
                let version = dist.version();

                // Respect the version range for this requirement.
                if !range.contains(version) {
                    return None;
                }

                debug!("Found installed version of {dist} that satisfies {range}");
                return Some(Candidate {
                    name: package_name,
                    version,
                    dist: CandidateDist::Compatible(CompatibleDist::InstalledDist(dist)),
                    choice_kind: VersionChoiceKind::Installed,
                });
            }
            // We do not consider installed distributions with multiple versions because
            // during installation these must be reinstalled from the remote
            _ => {
                debug!(
                    "Ignoring installed versions of {package_name}: multiple distributions found"
                );
            }
        }
        None
    }

    /// Select a [`Candidate`] without checking for version preference such as an existing
    /// lockfile.
    pub(crate) fn select_no_preference<'a>(
        &'a self,
        package_name: &'a PackageName,
        range: &Range<Version>,
        version_maps: &'a [VersionMap],
        markers: &ResolverMarkers,
    ) -> Option<Candidate> {
        tracing::trace!(
            "selecting candidate for package {package_name} with range {range:?} with {} remote versions",
            version_maps.iter().map(VersionMap::len).sum::<usize>(),
        );
        let highest = self.use_highest_version(package_name);
        let allow_prerelease = self.prerelease_strategy.allows(package_name, markers);

        if self.index_strategy == IndexStrategy::UnsafeBestMatch {
            if highest {
                Self::select_candidate(
                    version_maps
                        .iter()
                        .enumerate()
                        .map(|(map_index, version_map)| {
                            version_map.iter().rev().map(move |item| (map_index, item))
                        })
                        .kmerge_by(
                            |(index1, (version1, _)), (index2, (version2, _))| match version1
                                .cmp(version2)
                            {
                                std::cmp::Ordering::Equal => index1 < index2,
                                std::cmp::Ordering::Less => false,
                                std::cmp::Ordering::Greater => true,
                            },
                        )
                        .map(|(_, item)| item),
                    package_name,
                    range,
                    allow_prerelease,
                )
            } else {
                Self::select_candidate(
                    version_maps
                        .iter()
                        .enumerate()
                        .map(|(map_index, version_map)| {
                            version_map.iter().map(move |item| (map_index, item))
                        })
                        .kmerge_by(
                            |(index1, (version1, _)), (index2, (version2, _))| match version1
                                .cmp(version2)
                            {
                                std::cmp::Ordering::Equal => index1 < index2,
                                std::cmp::Ordering::Less => true,
                                std::cmp::Ordering::Greater => false,
                            },
                        )
                        .map(|(_, item)| item),
                    package_name,
                    range,
                    allow_prerelease,
                )
            }
        } else {
            if highest {
                version_maps.iter().find_map(|version_map| {
                    Self::select_candidate(
                        version_map.iter().rev(),
                        package_name,
                        range,
                        allow_prerelease,
                    )
                })
            } else {
                version_maps.iter().find_map(|version_map| {
                    Self::select_candidate(
                        version_map.iter(),
                        package_name,
                        range,
                        allow_prerelease,
                    )
                })
            }
        }
    }

    /// By default, we select the latest version, but we also allow using the lowest version instead
    /// to check the lower bounds.
    pub(crate) fn use_highest_version(&self, package_name: &PackageName) -> bool {
        match &self.resolution_strategy {
            ResolutionStrategy::Highest => true,
            ResolutionStrategy::Lowest => false,
            ResolutionStrategy::LowestDirect(direct_dependencies) => {
                !direct_dependencies.contains(package_name)
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
        let mut steps = 0usize;
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
                            Candidate::new(
                                package_name,
                                version,
                                dist,
                                VersionChoiceKind::Compatible,
                            )
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
                    Candidate::new(package_name, version, dist, VersionChoiceKind::Compatible)
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
            Some(PreReleaseCandidate::IfNecessary(version, dist)) => Some(Candidate::new(
                package_name,
                version,
                dist,
                VersionChoiceKind::Compatible,
            )),
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
            let dist = if let Some(incompatibility) = value.incompatible_source() {
                IncompatibleDist::Source(incompatibility.clone())
            } else if let Some(incompatibility) = value.incompatible_wheel() {
                IncompatibleDist::Wheel(incompatibility.clone())
            } else {
                IncompatibleDist::Unavailable
            };
            CandidateDist::Incompatible(dist)
        }
    }
}

/// The reason why we selected the version of the candidate version, either a preference or being
/// compatible.
#[derive(Debug, Clone, Copy)]
pub(crate) enum VersionChoiceKind {
    /// A preference from an output file such as `-o requirements.txt` or `uv.lock`.
    Preference,
    /// A preference from an installed version.
    Installed,
    /// The next compatible version in a version map
    Compatible,
}

impl Display for VersionChoiceKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionChoiceKind::Preference => f.write_str("preference"),
            VersionChoiceKind::Installed => f.write_str("installed"),
            VersionChoiceKind::Compatible => f.write_str("compatible"),
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
    /// Whether this candidate was selected from a preference.
    choice_kind: VersionChoiceKind,
}

impl<'a> Candidate<'a> {
    fn new(
        name: &'a PackageName,
        version: &'a Version,
        dist: &'a PrioritizedDist,
        choice_kind: VersionChoiceKind,
    ) -> Self {
        Self {
            name,
            version,
            dist: CandidateDist::from(dist),
            choice_kind,
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

    /// Return this candidate was selected from a preference.
    pub(crate) fn choice_kind(&self) -> VersionChoiceKind {
        self.choice_kind
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
    fn version_or_url(&self) -> distribution_types::VersionOrUrlRef {
        distribution_types::VersionOrUrlRef::Version(self.version)
    }
}
