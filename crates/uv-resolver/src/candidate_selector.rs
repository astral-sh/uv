use std::fmt::{Display, Formatter};

use either::Either;
use itertools::Itertools;
use pubgrub::Range;
use smallvec::SmallVec;
use tracing::{debug, trace};

use uv_configuration::IndexStrategy;
use uv_distribution_types::{CompatibleDist, IncompatibleDist, IncompatibleSource, IndexUrl};
use uv_distribution_types::{DistributionMetadata, IncompatibleWheel, Name, PrioritizedDist};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_types::InstalledPackagesProvider;

use crate::preferences::{Entry, Preferences};
use crate::prerelease::{AllowPrerelease, PrereleaseStrategy};
use crate::resolution_mode::ResolutionStrategy;
use crate::universal_marker::UniversalMarker;
use crate::version_map::{VersionMap, VersionMapDistHandle};
use crate::{Exclusions, Manifest, Options, ResolverEnvironment};

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
        options: &Options,
        manifest: &Manifest,
        env: &ResolverEnvironment,
    ) -> Self {
        Self {
            resolution_strategy: ResolutionStrategy::from_mode(
                options.resolution_mode,
                manifest,
                env,
                options.dependency_mode,
            ),
            prerelease_strategy: PrereleaseStrategy::from_mode(
                options.prerelease_mode,
                manifest,
                env,
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
        index: Option<&'a IndexUrl>,
        env: &ResolverEnvironment,
    ) -> Option<Candidate<'a>> {
        let reinstall = exclusions.reinstall(package_name);
        let upgrade = exclusions.upgrade(package_name);

        // If we have a preference (e.g., from a lockfile), search for a version matching that
        // preference.
        //
        // If `--reinstall` is provided, we should omit any already-installed packages from here,
        // since we can't reinstall already-installed packages.
        //
        // If `--upgrade` is provided, we should still search for a matching preference. In
        // practice, preferences should be empty if `--upgrade` is provided, but it's the caller's
        // responsibility to ensure that.
        if let Some(preferred) = self.get_preferred(
            package_name,
            range,
            version_maps,
            preferences,
            installed_packages,
            reinstall,
            index,
            env,
        ) {
            trace!("Using preference {} {}", preferred.name, preferred.version);
            return Some(preferred);
        }

        // If we don't have a preference, find an already-installed distribution that satisfies the
        // range.
        let installed = if reinstall {
            None
        } else {
            Self::get_installed(package_name, range, installed_packages)
        };

        // If we're not upgrading, we should prefer the already-installed distribution.
        if !upgrade {
            if let Some(installed) = installed {
                trace!(
                    "Using installed {} {} that satisfies {range}",
                    installed.name,
                    installed.version
                );
                return Some(installed);
            }
        }

        // Otherwise, find the best candidate from the version maps.
        let compatible = self.select_no_preference(package_name, range, version_maps, env);

        // Cross-reference against the already-installed distribution.
        //
        // If the already-installed version is _more_ compatible than the best candidate
        // from the version maps, use the installed version.
        if let Some(installed) = installed {
            if compatible.as_ref().is_none_or(|compatible| {
                let highest = self.use_highest_version(package_name, env);
                if highest {
                    installed.version() >= compatible.version()
                } else {
                    installed.version() <= compatible.version()
                }
            }) {
                trace!(
                    "Using installed {} {} that satisfies {range}",
                    installed.name,
                    installed.version
                );
                return Some(installed);
            }
        }

        compatible
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
        reinstall: bool,
        index: Option<&'a IndexUrl>,
        env: &ResolverEnvironment,
    ) -> Option<Candidate<'a>> {
        let preferences = preferences.get(package_name);

        // If there are multiple preferences for the same package, we need to sort them by priority.
        let preferences = match preferences {
            [] => return None,
            [entry] => {
                // Filter out preferences that map to a conflicting index.
                if index.is_some_and(|index| !entry.index().matches(index)) {
                    return None;
                }
                Either::Left(std::iter::once((entry.marker(), entry.pin().version())))
            }
            [..] => {
                type Entries<'a> = SmallVec<[&'a Entry; 3]>;

                let mut preferences = preferences.iter().collect::<Entries>();

                // Filter out preferences that map to a conflicting index.
                preferences.retain(|entry| index.is_none_or(|index| entry.index().matches(index)));

                // Sort the preferences by priority.
                let highest = self.use_highest_version(package_name, env);
                preferences.sort_by_key(|entry| {
                    let marker = entry.marker();

                    // Prefer preferences that match the current environment.
                    let matches_env = env.included_by_marker(marker.pep508());

                    // Prefer the latest (or earliest) version.
                    let version = if highest {
                        Either::Left(entry.pin().version())
                    } else {
                        Either::Right(std::cmp::Reverse(entry.pin().version()))
                    };

                    std::cmp::Reverse((matches_env, version))
                });

                Either::Right(
                    preferences
                        .into_iter()
                        .map(|entry| (entry.marker(), entry.pin().version())),
                )
            }
        };

        self.get_preferred_from_iter(
            preferences,
            package_name,
            range,
            version_maps,
            installed_packages,
            reinstall,
            env,
        )
    }

    /// Return the first preference that satisfies the current range and is allowed.
    fn get_preferred_from_iter<'a, InstalledPackages: InstalledPackagesProvider>(
        &'a self,
        preferences: impl Iterator<Item = (&'a UniversalMarker, &'a Version)>,
        package_name: &'a PackageName,
        range: &Range<Version>,
        version_maps: &'a [VersionMap],
        installed_packages: &'a InstalledPackages,
        reinstall: bool,
        env: &ResolverEnvironment,
    ) -> Option<Candidate<'a>> {
        for (marker, version) in preferences {
            // Respect the version range for this requirement.
            if !range.contains(version) {
                continue;
            }

            // Check for a locally installed distribution that matches the preferred version, unless
            // we have to reinstall, in which case we can't reuse an already-installed distribution.
            if !reinstall {
                let installed_dists = installed_packages.get_packages(package_name);
                match installed_dists.as_slice() {
                    [] => {}
                    [dist] => {
                        if dist.version() == version {
                            debug!("Found installed version of {dist} that satisfies preference in {range}");

                            return Some(Candidate {
                                name: package_name,
                                version,
                                prioritized: None,
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
                let allow = match self.prerelease_strategy.allows(package_name, env) {
                    AllowPrerelease::Yes => true,
                    AllowPrerelease::No => false,
                    // If the pre-release is "global" (i.e., provided via a lockfile, rather than
                    // a fork), accept it unless pre-releases are completely banned.
                    AllowPrerelease::IfNecessary => marker.is_true(),
                };
                if !allow {
                    continue;
                }
            }

            // Check for a remote distribution that matches the preferred version
            if let Some((version_map, file)) = version_maps
                .iter()
                .find_map(|version_map| version_map.get(version).map(|dist| (version_map, dist)))
            {
                // If the preferred version has a local variant, prefer that.
                if version_map.local() {
                    for local in version_map
                        .versions()
                        .rev()
                        .take_while(|local| *local > version)
                    {
                        if !local.is_local() {
                            continue;
                        }
                        if local.clone().without_local() != *version {
                            continue;
                        }
                        if !range.contains(local) {
                            continue;
                        }
                        if let Some(dist) = version_map.get(local) {
                            debug!("Preferring local version `{package_name}` (v{local})");
                            return Some(Candidate::new(
                                package_name,
                                local,
                                dist,
                                VersionChoiceKind::Preference,
                            ));
                        }
                    }
                }

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
                    prioritized: None,
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
        env: &ResolverEnvironment,
    ) -> Option<Candidate<'a>> {
        trace!(
            "Selecting candidate for {package_name} with range {range} with {} remote versions",
            version_maps.iter().map(VersionMap::len).sum::<usize>(),
        );
        let highest = self.use_highest_version(package_name, env);

        let allow_prerelease = match self.prerelease_strategy.allows(package_name, env) {
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
    pub(crate) fn use_highest_version(
        &self,
        package_name: &PackageName,
        env: &ResolverEnvironment,
    ) -> bool {
        match &self.resolution_strategy {
            ResolutionStrategy::Highest => true,
            ResolutionStrategy::Lowest => false,
            ResolutionStrategy::LowestDirect(direct_dependencies) => {
                !direct_dependencies.contains(package_name, env)
            }
        }
    }

    /// Select the first-matching [`Candidate`] from a set of candidate versions and files,
    /// preferring wheels to source distributions.
    ///
    /// The returned [`Candidate`] _may not_ be compatible with the current platform; in such
    /// cases, the resolver is responsible for tracking the incompatibility and re-running the
    /// selection process with additional constraints.
    fn select_candidate<'a>(
        versions: impl Iterator<Item = (&'a Version, VersionMapDistHandle<'a>)>,
        package_name: &'a PackageName,
        range: &Range<Version>,
        allow_prerelease: bool,
    ) -> Option<Candidate<'a>> {
        let mut steps = 0usize;
        let mut incompatible: Option<Candidate> = None;
        for (version, maybe_dist) in versions {
            steps += 1;

            // If we have an incompatible candidate, and we've progressed past it, return it.
            if incompatible
                .as_ref()
                .is_some_and(|incompatible| version != incompatible.version)
            {
                trace!(
                    "Returning incompatible candidate for package {package_name} with range {range} after {steps} steps",
                );
                return incompatible;
            }

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
                trace!("Found candidate for package {package_name} with range {range} after {steps} steps: {version} version");
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

            // If the candidate isn't compatible, we store it as incompatible and continue
            // searching. Typically, we want to return incompatible candidates so that PubGrub can
            // track them (then continue searching, with additional constraints). However, we may
            // see multiple entries for the same version (e.g., if the same version exists on
            // multiple indexes and `--index-strategy unsafe-best-match` is enabled), and it's
            // possible that one of them is compatible while the other is not.
            //
            // See, e.g., <https://github.com/astral-sh/uv/issues/8922>. At time of writing,
            // markupsafe==3.0.2 exists on the PyTorch index, but there's only a single wheel:
            //
            //   MarkupSafe-3.0.2-cp313-cp313-manylinux_2_17_x86_64.manylinux2014_x86_64.whl
            //
            // Meanwhile, there are a large number of wheels on PyPI for the same version. If the
            // user is on Python 3.12, and we return the incompatible PyTorch wheel without
            // considering the PyPI wheels, PubGrub will mark 3.0.2 as an incompatible version,
            // even though there are compatible wheels on PyPI. Thus, we need to ensure that we
            // return the first _compatible_ candidate across all indexes, if such a candidate
            // exists.
            if matches!(candidate.dist(), CandidateDist::Incompatible(_)) {
                if incompatible.is_none() {
                    incompatible = Some(candidate);
                }
                continue;
            }

            trace!(
                "Returning candidate for package {package_name} with range {range} after {steps} steps",
            );
            return Some(candidate);
        }

        if incompatible.is_some() {
            trace!(
                "Returning incompatible candidate for package {package_name} with range {range} after {steps} steps",
            );
            return incompatible;
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
    /// The prioritized distribution for the package.
    prioritized: Option<&'a PrioritizedDist>,
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
            prioritized: Some(dist),
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

    /// Return the prioritized distribution for the candidate.
    pub(crate) fn prioritized(&self) -> Option<&PrioritizedDist> {
        self.prioritized
    }
}

impl Name for Candidate<'_> {
    fn name(&self) -> &PackageName {
        self.name
    }
}

impl DistributionMetadata for Candidate<'_> {
    fn version_or_url(&self) -> uv_distribution_types::VersionOrUrlRef {
        uv_distribution_types::VersionOrUrlRef::Version(self.version)
    }
}
