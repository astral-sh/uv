use itertools::Itertools;
use pubgrub::Range;
use std::fmt::{Display, Formatter};
use tracing::{debug, trace};

use distribution_types::{CompatibleDist, IncompatibleDist, IncompatibleSource};
use distribution_types::{DistributionMetadata, IncompatibleWheel, Name, PrioritizedDist};
use pep440_rs::Version;
use pep508_rs::MarkerTree;
use uv_configuration::IndexStrategy;
use uv_normalize::PackageName;
use uv_types::InstalledPackagesProvider;

use crate::preferences::Preferences;
use crate::prerelease::{AllowPrerelease, PrereleaseStrategy};
use crate::resolution_mode::ResolutionStrategy;
use crate::version_map::{VersionMap, VersionMapDistHandle};
use crate::{Exclusions, Manifest, Options, ResolverMarkers};

#[derive(Debug, Clone)]
#[allow(clippy::struct_field_names)]
pub(crate) struct CandidateSelector {
    resolution_strategy: ResolutionStrategy,
    prerelease_strategy: PrereleaseStrategy,
    index_strategy: IndexStrategy,
}

impl CandidateSelector {
    /// Return a [`CandidateSelector`] for the given [`Manifest`].
    pub(crate) fn for_resolution(
        options: Options,
        manifest: &Manifest,
        markers: &ResolverMarkers,
    ) -> Self {
        Self {
            resolution_strategy: ResolutionStrategy::from_mode(
                options.resolution_mode,
                manifest,
                markers,
                options.dependency_mode,
            ),
            prerelease_strategy: PrereleaseStrategy::from_mode(
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
    pub(crate) fn prerelease_strategy(&self) -> &PrereleaseStrategy {
        &self.prerelease_strategy
    }

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn index_strategy(&self) -> &IndexStrategy {
        &self.index_strategy
    }

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
        let is_excluded = exclusions.contains(package_name);

        // Check for a preference from a lockfile or a previous fork  that satisfies the range and
        // is allowed.
        if let Some(preferred) = self.get_preferred(
            package_name,
            range,
            version_maps,
            preferences,
            installed_packages,
            is_excluded,
            markers,
        ) {
            trace!("Using preference {} {}", preferred.name, preferred.version);
            return Some(preferred);
        }

        // Check for a locally installed distribution that satisfies the range and is allowed.
        if !is_excluded {
            if let Some(installed) = Self::get_installed(package_name, range, installed_packages) {
                trace!(
                    "Using preference {} {} from installed package",
                    installed.name,
                    installed.version,
                );
                return Some(installed);
            }
        }

        self.select_no_preference(package_name, range, version_maps, markers)
    }

    /// If the package has a preference, an existing version from an existing lockfile or a version
    /// from a sibling fork, and the preference satisfies the current range, use that.
    ///
    /// We try to find a resolution that, depending on the input, does not diverge from the
    /// lockfile or matches a sibling fork. We try an exact match for the current markers (fork
    /// or specific) first, to ensure stability with repeated locking. If that doesn't work, we
    /// fall back to preferences that don't match in hopes of still resolving different forks into
    /// the same version; A solution with less different versions is more desirable than one where
    /// we may have more recent version in some cases, but overall more versions.
    fn get_preferred<'a, InstalledPackages: InstalledPackagesProvider>(
        &'a self,
        package_name: &'a PackageName,
        range: &Range<Version>,
        version_maps: &'a [VersionMap],
        preferences: &'a Preferences,
        installed_packages: &'a InstalledPackages,
        is_excluded: bool,
        resolver_markers: &ResolverMarkers,
    ) -> Option<Candidate> {
        // In the branches, we "sort" the preferences by marker-matching through an iterator that
        // first has the matching half and then the mismatching half.
        match resolver_markers {
            ResolverMarkers::SpecificEnvironment(env) => {
                // We may hit a combination of fork markers preferences with specific environment
                // output in the future when adding support for the PEP 665 successor.
                let preferences_match =
                    preferences.get(package_name).filter(|(marker, _version)| {
                        // `.unwrap_or(true)` because the universal marker is considered matching.
                        marker
                            .map(|marker| marker.evaluate(env, &[]))
                            .unwrap_or(true)
                    });
                let preferences_mismatch =
                    preferences.get(package_name).filter(|(marker, _version)| {
                        marker
                            .map(|marker| !marker.evaluate(env, &[]))
                            .unwrap_or(false)
                    });
                self.get_preferred_from_iter(
                    preferences_match.chain(preferences_mismatch),
                    package_name,
                    range,
                    version_maps,
                    installed_packages,
                    is_excluded,
                    resolver_markers,
                )
            }
            ResolverMarkers::Universal { .. } => {
                // In universal mode, all preferences are matching.
                self.get_preferred_from_iter(
                    preferences.get(package_name),
                    package_name,
                    range,
                    version_maps,
                    installed_packages,
                    is_excluded,
                    resolver_markers,
                )
            }
            ResolverMarkers::Fork(fork_markers) => {
                let preferences_match =
                    preferences.get(package_name).filter(|(marker, _version)| {
                        // `.unwrap_or(true)` because the universal marker is considered matching.
                        marker.map(|marker| marker == fork_markers).unwrap_or(true)
                    });
                let preferences_mismatch =
                    preferences.get(package_name).filter(|(marker, _version)| {
                        marker.map(|marker| marker != fork_markers).unwrap_or(false)
                    });
                self.get_preferred_from_iter(
                    preferences_match.chain(preferences_mismatch),
                    package_name,
                    range,
                    version_maps,
                    installed_packages,
                    is_excluded,
                    resolver_markers,
                )
            }
        }
    }

    /// Return the first preference that satisfies the current range and is allowed.
    fn get_preferred_from_iter<'a, InstalledPackages: InstalledPackagesProvider>(
        &'a self,
        preferences: impl Iterator<Item = (Option<&'a MarkerTree>, &'a Version)>,
        package_name: &'a PackageName,
        range: &Range<Version>,
        version_maps: &'a [VersionMap],
        installed_packages: &'a InstalledPackages,
        is_excluded: bool,
        resolver_markers: &ResolverMarkers,
    ) -> Option<Candidate<'a>> {
        for (marker, version) in preferences {
            // Respect the version range for this requirement.
            if !range.contains(version) {
                continue;
            }

            // Check for a locally installed distribution that matches the preferred version.
            if !is_excluded {
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
            if version.any_prerelease() {
                let allow = match self
                    .prerelease_strategy
                    .allows(package_name, resolver_markers)
                {
                    AllowPrerelease::Yes => true,
                    AllowPrerelease::No => false,
                    // If the pre-release is "global" (i.e., provided via a lockfile, rather than
                    // a fork), accept it unless pre-releases are completely banned.
                    AllowPrerelease::IfNecessary => marker.is_none(),
                };
                if !allow {
                    continue;
                }
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
        }
        None
    }

    /// Check for an installed distribution that satisfies the current range and is allowed.
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
        trace!(
            "Selecting candidate for {package_name} with range {range} with {} remote versions",
            version_maps.iter().map(VersionMap::len).sum::<usize>(),
        );
        let highest = self.use_highest_version(package_name);

        let allow_prerelease = match self.prerelease_strategy.allows(package_name, markers) {
            AllowPrerelease::Yes => true,
            AllowPrerelease::No => false,
            // Allow pre-releases if there are no stable versions available.
            AllowPrerelease::IfNecessary => !version_maps.iter().any(VersionMap::stable),
        };

        if self.index_strategy == IndexStrategy::UnsafeBestMatch {
            if highest {
                Self::select_candidate(
                    version_maps
                        .iter()
                        .enumerate()
                        .map(|(map_index, version_map)| {
                            version_map
                                .iter(range)
                                .rev()
                                .map(move |item| (map_index, item))
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
                            version_map.iter(range).map(move |item| (map_index, item))
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
                        version_map.iter(range).rev(),
                        package_name,
                        range,
                        allow_prerelease,
                    )
                })
            } else {
                version_maps.iter().find_map(|version_map| {
                    Self::select_candidate(
                        version_map.iter(range),
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
        allow_prerelease: bool,
    ) -> Option<Candidate<'a>> {
        let mut steps = 0usize;
        for (version, maybe_dist) in versions {
            steps += 1;

            let candidate = {
                if version.any_prerelease() && !allow_prerelease {
                    continue;
                }
                if !range.contains(version) {
                    continue;
                };
                let Some(dist) = maybe_dist.prioritized_dist() else {
                    continue;
                };
                trace!("found candidate for package {package_name:?} with range {range:?} after {steps} steps: {version:?} version");
                Candidate::new(package_name, version, dist, VersionChoiceKind::Compatible)
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
        trace!("Exhausted all candidates for package {package_name} with range {range} after {steps} steps");
        None
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
