//! Given a set of requirements, find a set of compatible packages.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::{Display, Formatter, Write};
use std::ops::Bound;
use std::sync::Arc;
use std::time::Instant;
use std::{iter, thread};

use dashmap::DashMap;
use either::Either;
use futures::{FutureExt, StreamExt};
use itertools::Itertools;
use pubgrub::{Incompatibility, Range, State};
use rustc_hash::{FxHashMap, FxHashSet};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::sync::oneshot;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, info, instrument, trace, warn, Level};

use distribution_types::{
    BuiltDist, CompatibleDist, Dist, DistributionMetadata, IncompatibleDist, IncompatibleSource,
    IncompatibleWheel, IndexCapabilities, IndexLocations, InstalledDist, PythonRequirementKind,
    RemoteSource, ResolvedDist, ResolvedDistRef, SourceDist, VersionOrUrlRef,
};
pub(crate) use fork_map::{ForkMap, ForkSet};
use locals::Locals;
use pep440_rs::{Version, MIN_VERSION};
use pep508_rs::MarkerTree;
use platform_tags::Tags;
use pypi_types::{Metadata23, Requirement, VerbatimParsedUrl};
pub use resolver_markers::ResolverMarkers;
pub(crate) use urls::Urls;
use uv_configuration::{Constraints, Overrides};
use uv_distribution::{ArchiveMetadata, DistributionDatabase};
use uv_git::GitResolver;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_types::{BuildContext, HashStrategy, InstalledPackagesProvider};
use uv_warnings::warn_user_once;

use crate::candidate_selector::{CandidateDist, CandidateSelector};
use crate::dependency_provider::UvDependencyProvider;
use crate::error::{NoSolutionError, ResolveError};
use crate::fork_urls::ForkUrls;
use crate::manifest::Manifest;
use crate::pins::FilePins;
use crate::preferences::Preferences;
use crate::pubgrub::{
    PubGrubDependency, PubGrubDistribution, PubGrubPackage, PubGrubPackageInner, PubGrubPriorities,
    PubGrubPython, PubGrubSpecifier,
};
use crate::python_requirement::PythonRequirement;
use crate::resolution::ResolutionGraph;
use crate::resolution_mode::ResolutionStrategy;
pub(crate) use crate::resolver::availability::{
    IncompletePackage, ResolverVersion, UnavailablePackage, UnavailableReason, UnavailableVersion,
};
use crate::resolver::batch_prefetch::BatchPrefetcher;
use crate::resolver::groups::Groups;
pub use crate::resolver::index::InMemoryIndex;
pub use crate::resolver::provider::{
    DefaultResolverProvider, MetadataResponse, PackageVersionsResult, ResolverProvider,
    VersionsResponse, WheelMetadataResult,
};
use crate::resolver::reporter::Facade;
pub use crate::resolver::reporter::{BuildId, Reporter};
use crate::yanks::AllowedYanks;
use crate::{marker, DependencyMode, Exclusions, FlatIndex, Options};

mod availability;
mod batch_prefetch;
mod fork_map;
mod groups;
mod index;
mod locals;
mod provider;
mod reporter;
mod resolver_markers;
mod urls;

pub struct Resolver<Provider: ResolverProvider, InstalledPackages: InstalledPackagesProvider> {
    state: ResolverState<InstalledPackages>,
    provider: Provider,
}

/// State that is shared between the prefetcher and the PubGrub solver during
/// resolution, across all forks.
struct ResolverState<InstalledPackages: InstalledPackagesProvider> {
    project: Option<PackageName>,
    requirements: Vec<Requirement>,
    constraints: Constraints,
    overrides: Overrides,
    groups: Groups,
    preferences: Preferences,
    git: GitResolver,
    capabilities: IndexCapabilities,
    exclusions: Exclusions,
    urls: Urls,
    locals: Locals,
    dependency_mode: DependencyMode,
    hasher: HashStrategy,
    markers: ResolverMarkers,
    python_requirement: PythonRequirement,
    workspace_members: BTreeSet<PackageName>,
    selector: CandidateSelector,
    index: InMemoryIndex,
    installed_packages: InstalledPackages,
    /// Incompatibilities for packages that are entirely unavailable.
    unavailable_packages: DashMap<PackageName, UnavailablePackage>,
    /// Incompatibilities for packages that are unavailable at specific versions.
    incomplete_packages: DashMap<PackageName, DashMap<Version, IncompletePackage>>,
    /// The options that were used to configure this resolver.
    options: Options,
    /// The reporter to use for this resolver.
    reporter: Option<Arc<dyn Reporter>>,
}

impl<'a, Context: BuildContext, InstalledPackages: InstalledPackagesProvider>
    Resolver<DefaultResolverProvider<'a, Context>, InstalledPackages>
{
    /// Initialize a new resolver using the default backend doing real requests.
    ///
    /// Reads the flat index entries.
    ///
    /// # Marker environment
    ///
    /// The marker environment is optional.
    ///
    /// When a marker environment is not provided, the resolver is said to be
    /// in "universal" mode. When in universal mode, the resolution produced
    /// may contain multiple versions of the same package. And thus, in order
    /// to use the resulting resolution, there must be a "universal"-aware
    /// reader of the resolution that knows to exclude distributions that can't
    /// be used in the current environment.
    ///
    /// When a marker environment is provided, the resolver is in
    /// "non-universal" mode, which corresponds to standard `pip` behavior that
    /// works only for a specific marker environment.
    pub fn new(
        manifest: Manifest,
        options: Options,
        python_requirement: &'a PythonRequirement,
        markers: ResolverMarkers,
        tags: Option<&'a Tags>,
        flat_index: &'a FlatIndex,
        index: &'a InMemoryIndex,
        hasher: &'a HashStrategy,
        build_context: &'a Context,
        installed_packages: InstalledPackages,
        database: DistributionDatabase<'a, Context>,
    ) -> Result<Self, ResolveError> {
        let provider = DefaultResolverProvider::new(
            database,
            flat_index,
            tags,
            python_requirement.target(),
            AllowedYanks::from_manifest(&manifest, &markers, options.dependency_mode),
            hasher,
            options.exclude_newer,
            build_context.build_options(),
        );

        Self::new_custom_io(
            manifest,
            options,
            hasher,
            markers,
            python_requirement,
            index,
            build_context.git(),
            build_context.capabilities(),
            provider,
            installed_packages,
        )
    }
}

impl<Provider: ResolverProvider, InstalledPackages: InstalledPackagesProvider>
    Resolver<Provider, InstalledPackages>
{
    /// Initialize a new resolver using a user provided backend.
    pub fn new_custom_io(
        manifest: Manifest,
        options: Options,
        hasher: &HashStrategy,
        markers: ResolverMarkers,
        python_requirement: &PythonRequirement,
        index: &InMemoryIndex,
        git: &GitResolver,
        capabilities: &IndexCapabilities,
        provider: Provider,
        installed_packages: InstalledPackages,
    ) -> Result<Self, ResolveError> {
        let state = ResolverState {
            index: index.clone(),
            git: git.clone(),
            capabilities: capabilities.clone(),
            selector: CandidateSelector::for_resolution(options, &manifest, &markers),
            dependency_mode: options.dependency_mode,
            urls: Urls::from_manifest(&manifest, &markers, git, options.dependency_mode)?,
            locals: Locals::from_manifest(&manifest, &markers, options.dependency_mode),
            groups: Groups::from_manifest(&manifest, &markers),
            project: manifest.project,
            workspace_members: manifest.workspace_members,
            requirements: manifest.requirements,
            constraints: manifest.constraints,
            overrides: manifest.overrides,
            preferences: manifest.preferences,
            exclusions: manifest.exclusions,
            hasher: hasher.clone(),
            markers,
            python_requirement: python_requirement.clone(),
            installed_packages,
            unavailable_packages: DashMap::default(),
            incomplete_packages: DashMap::default(),
            options,
            reporter: None,
        };
        Ok(Self { state, provider })
    }

    /// Set the [`Reporter`] to use for this installer.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        let reporter = Arc::new(reporter);

        Self {
            state: ResolverState {
                reporter: Some(reporter.clone()),
                ..self.state
            },
            provider: self.provider.with_reporter(Facade { reporter }),
        }
    }

    /// Resolve a set of requirements into a set of pinned versions.
    pub async fn resolve(self) -> Result<ResolutionGraph, ResolveError> {
        let state = Arc::new(self.state);
        let provider = Arc::new(self.provider);

        // A channel to fetch package metadata (e.g., given `flask`, fetch all versions) and version
        // metadata (e.g., given `flask==1.0.0`, fetch the metadata for that version).
        // Channel size is set large to accommodate batch prefetching.
        let (request_sink, request_stream) = mpsc::channel(300);

        // Run the fetcher.
        let requests_fut = state.clone().fetch(provider.clone(), request_stream).fuse();

        // Spawn the PubGrub solver on a dedicated thread.
        let solver = state.clone();
        let index_locations = provider.index_locations().clone();
        let (tx, rx) = oneshot::channel();
        thread::Builder::new()
            .name("uv-resolver".into())
            .spawn(move || {
                let result = solver.solve(index_locations, request_sink);

                // This may fail if the main thread returned early due to an error.
                let _ = tx.send(result);
            })
            .unwrap();

        let resolve_fut = async move { rx.await.map_err(|_| ResolveError::ChannelClosed) };

        // Wait for both to complete.
        let ((), resolution) = tokio::try_join!(requests_fut, resolve_fut)?;

        state.on_complete();
        resolution
    }
}

impl<InstalledPackages: InstalledPackagesProvider> ResolverState<InstalledPackages> {
    #[instrument(skip_all)]
    fn solve(
        self: Arc<Self>,
        // No solution error context.
        index_locations: IndexLocations,
        request_sink: Sender<Request>,
    ) -> Result<ResolutionGraph, ResolveError> {
        debug!(
            "Solving with installed Python version: {}",
            self.python_requirement.exact()
        );
        debug!(
            "Solving with target Python version: {}",
            self.python_requirement.target()
        );

        let mut visited = FxHashSet::default();

        let root = PubGrubPackage::from(PubGrubPackageInner::Root(self.project.clone()));
        let mut prefetcher = BatchPrefetcher::default();
        let state = ForkState::new(
            State::init(root.clone(), MIN_VERSION.clone()),
            root,
            self.markers.clone(),
            self.python_requirement.clone(),
        );
        let mut preferences = self.preferences.clone();
        let mut forked_states =
            if let ResolverMarkers::Universal { fork_preferences } = &self.markers {
                if fork_preferences.is_empty() {
                    vec![state]
                } else {
                    fork_preferences
                        .iter()
                        .rev()
                        .map(|fork_preference| state.clone().with_markers(fork_preference.clone()))
                        .collect()
                }
            } else {
                vec![state]
            };
        let mut resolutions = vec![];

        'FORK: while let Some(mut state) = forked_states.pop() {
            if let ResolverMarkers::Fork(markers) = &state.markers {
                let requires_python = state.python_requirement.target();
                debug!("Solving split {markers:?} (requires-python: {requires_python:?})");
            }
            let start = Instant::now();
            loop {
                // Run unit propagation.
                if let Err(err) = state.pubgrub.unit_propagation(state.next.clone()) {
                    return Err(self.convert_no_solution_err(
                        err,
                        state.fork_urls,
                        state.markers,
                        &visited,
                        &index_locations,
                    ));
                }

                // Pre-visit all candidate packages, to allow metadata to be fetched in parallel.
                if self.dependency_mode.is_transitive() {
                    Self::pre_visit(
                        state.pubgrub.partial_solution.prioritized_packages(),
                        &self.urls,
                        &state.python_requirement,
                        &request_sink,
                    )?;
                }

                // Choose a package version.
                let Some(highest_priority_pkg) = state
                    .pubgrub
                    .partial_solution
                    .pick_highest_priority_pkg(|package, _range| state.priorities.get(package))
                else {
                    if tracing::enabled!(Level::DEBUG) {
                        prefetcher.log_tried_versions();
                    }
                    debug!(
                        "Split {} resolution took {:.3}s",
                        state.markers,
                        start.elapsed().as_secs_f32()
                    );

                    let resolution = state.into_resolution();

                    // Walk over the selected versions, and mark them as preferences. We have to
                    // add forks back as to not override the preferences from the lockfile for
                    // the next fork
                    for (package, version) in &resolution.nodes {
                        preferences.insert(
                            package.name.clone(),
                            resolution.markers.fork_markers().cloned(),
                            version.clone(),
                        );
                    }

                    resolutions.push(resolution);
                    continue 'FORK;
                };
                state.next = highest_priority_pkg;
                let url = state.next.name().and_then(|name| state.fork_urls.get(name));

                // Consider:
                // ```toml
                // dependencies = [
                //   "iniconfig == 1.1.1 ; python_version < '3.12'",
                //   "iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl ; python_version >= '3.12'",
                // ]
                // ```
                // In the `python_version < '3.12'` case, we haven't pre-visited `iniconfig` yet,
                // since we weren't sure whether it might also be a URL requirement when
                // transforming the requirements. For that case, we do another request here
                // (idempotent due to caching).
                self.request_package(&state.next, url, &request_sink)?;

                prefetcher.version_tried(state.next.clone());

                let term_intersection = state
                    .pubgrub
                    .partial_solution
                    .term_intersection_for_package(&state.next)
                    .expect("a package was chosen but we don't have a term");
                let decision = self.choose_version(
                    &state.next,
                    term_intersection.unwrap_positive(),
                    &mut state.pins,
                    &preferences,
                    &state.fork_urls,
                    &state.markers,
                    &state.python_requirement,
                    &mut visited,
                    &request_sink,
                )?;

                // Pick the next compatible version.
                let version = match decision {
                    None => {
                        debug!("No compatible version found for: {next}", next = state.next);

                        let term_intersection = state
                            .pubgrub
                            .partial_solution
                            .term_intersection_for_package(&state.next)
                            .expect("a package was chosen but we don't have a term");

                        if let PubGrubPackageInner::Package { ref name, .. } = &*state.next {
                            // Check if the decision was due to the package being unavailable
                            if let Some(entry) = self.unavailable_packages.get(name) {
                                state
                                    .pubgrub
                                    .add_incompatibility(Incompatibility::custom_term(
                                        state.next.clone(),
                                        term_intersection.clone(),
                                        UnavailableReason::Package(entry.clone()),
                                    ));
                                continue;
                            }
                        }

                        state
                            .pubgrub
                            .add_incompatibility(Incompatibility::no_versions(
                                state.next.clone(),
                                term_intersection.clone(),
                            ));
                        continue;
                    }
                    Some(version) => version,
                };
                let version = match version {
                    ResolverVersion::Available(version) => version,
                    ResolverVersion::Unavailable(version, reason) => {
                        state.add_unavailable_version(version, reason)?;
                        continue;
                    }
                };

                // Only consider registry packages for prefetch.
                if url.is_none() {
                    prefetcher.prefetch_batches(
                        &state.next,
                        &version,
                        term_intersection.unwrap_positive(),
                        &state.python_requirement,
                        &request_sink,
                        &self.index,
                        &self.capabilities,
                        &self.selector,
                        &state.markers,
                    )?;
                }

                self.on_progress(&state.next, &version);

                if !state
                    .added_dependencies
                    .entry(state.next.clone())
                    .or_default()
                    .insert(version.clone())
                {
                    // `dep_incompats` are already in `incompatibilities` so we know there are not satisfied
                    // terms and can add the decision directly.
                    state
                        .pubgrub
                        .partial_solution
                        .add_decision(state.next.clone(), version);
                    continue;
                }

                let for_package = if let PubGrubPackageInner::Root(_) = &*state.next {
                    None
                } else {
                    state.next.name().map(|name| format!("{name}=={version}"))
                };
                // Retrieve that package dependencies.
                let forked_deps = self.get_dependencies_forking(
                    &state.next,
                    &version,
                    &state.fork_urls,
                    &state.markers,
                    &state.python_requirement,
                )?;
                match forked_deps {
                    ForkedDependencies::Unavailable(reason) => {
                        state
                            .pubgrub
                            .add_incompatibility(Incompatibility::custom_version(
                                state.next.clone(),
                                version.clone(),
                                UnavailableReason::Version(reason),
                            ));
                    }
                    ForkedDependencies::Unforked(dependencies) => {
                        state.add_package_version_dependencies(
                            for_package.as_deref(),
                            &version,
                            &self.urls,
                            &self.locals,
                            dependencies.clone(),
                            &self.git,
                            self.selector.resolution_strategy(),
                        )?;

                        // Emit a request to fetch the metadata for each registry package.
                        for dependency in &dependencies {
                            let PubGrubDependency {
                                package,
                                version: _,
                                specifier: _,
                                url: _,
                            } = dependency;
                            let url = package.name().and_then(|name| state.fork_urls.get(name));
                            self.visit_package(package, url, &request_sink)?;
                        }
                    }
                    ForkedDependencies::Forked {
                        forks,
                        diverging_packages,
                    } => {
                        debug!(
                            "Pre-fork split {} took {:.3}s",
                            state.markers,
                            start.elapsed().as_secs_f32()
                        );

                        for new_fork_state in self.forks_to_fork_states(
                            state,
                            &version,
                            forks,
                            &request_sink,
                            for_package.as_deref(),
                            &diverging_packages,
                        ) {
                            forked_states.push(new_fork_state?);
                        }
                        continue 'FORK;
                    }
                }
            }
        }
        if resolutions.len() > 1 {
            info!(
                "Solved your requirements for {} environments",
                resolutions.len()
            );
        }
        for resolution in &resolutions {
            if let Some(markers) = resolution.markers.fork_markers() {
                debug!(
                    "Distinct solution for ({markers:?}) with {} packages",
                    resolution.nodes.len()
                );
            }
        }
        for resolution in &resolutions {
            Self::trace_resolution(resolution);
        }
        ResolutionGraph::from_state(
            &resolutions,
            &self.requirements,
            &self.constraints,
            &self.overrides,
            &self.preferences,
            &self.index,
            &self.git,
            &self.python_requirement,
            self.selector.resolution_strategy(),
            self.options,
        )
    }

    /// When trace level logging is enabled, we dump the final
    /// set of resolutions, including markers, to help with
    /// debugging. Namely, this tells use precisely the state
    /// emitted by the resolver before going off to construct a
    /// resolution graph.
    fn trace_resolution(combined: &Resolution) {
        if !tracing::enabled!(Level::TRACE) {
            return;
        }
        if let Some(markers) = combined.markers.fork_markers() {
            trace!("Resolution: {:?}", markers);
        } else {
            trace!("Resolution: <matches all marker environments>");
        }
        for edge in &combined.edges {
            trace!(
                "Resolution edge: {} -> {}",
                edge.from
                    .as_ref()
                    .map(PackageName::as_str)
                    .unwrap_or("ROOT"),
                edge.to,
            );
            // The unwraps below are OK because `write`ing to
            // a String can never fail (except for OOM).
            let mut msg = String::new();
            write!(msg, "{}", edge.from_version).unwrap();
            if let Some(ref extra) = edge.from_extra {
                write!(msg, " (extra: {extra})").unwrap();
            }
            if let Some(ref dev) = edge.from_dev {
                write!(msg, " (group: {dev})").unwrap();
            }

            write!(msg, " -> ").unwrap();

            write!(msg, "{}", edge.to_version).unwrap();
            if let Some(ref extra) = edge.to_extra {
                write!(msg, " (extra: {extra})").unwrap();
            }
            if let Some(ref dev) = edge.to_dev {
                write!(msg, " (group: {dev})").unwrap();
            }
            if let Some(marker) = edge.marker.contents() {
                write!(msg, " ; {marker}").unwrap();
            }
            trace!("Resolution edge:     {msg}");
        }
    }

    /// Convert the dependency [`Fork`]s into [`ForkState`]s.
    fn forks_to_fork_states<'a>(
        &'a self,
        current_state: ForkState,
        version: &'a Version,
        forks: Vec<Fork>,
        request_sink: &'a Sender<Request>,
        for_package: Option<&'a str>,
        diverging_packages: &'a [PackageName],
    ) -> impl Iterator<Item = Result<ForkState, ResolveError>> + 'a {
        debug!(
            "Splitting resolution on {}=={} over {} into {} resolution with separate markers",
            current_state.next,
            version,
            diverging_packages
                .iter()
                .map(ToString::to_string)
                .join(", "),
            forks.len()
        );
        assert!(forks.len() >= 2);
        // This is a somewhat tortured technique to ensure
        // that our resolver state is only cloned as much
        // as it needs to be. We basically move the state
        // into `forked_states`, and then only clone it if
        // there is at least one more fork to visit.
        let mut cur_state = Some(current_state);
        let forks_len = forks.len();
        forks
            .into_iter()
            .enumerate()
            .map(move |(i, fork)| {
                let is_last = i == forks_len - 1;
                let forked_state = cur_state.take().unwrap();
                if !is_last {
                    cur_state = Some(forked_state.clone());
                }

                let mut forked_state = forked_state.with_markers(fork.markers);
                forked_state.add_package_version_dependencies(
                    for_package,
                    version,
                    &self.urls,
                    &self.locals,
                    fork.dependencies.clone(),
                    &self.git,
                    self.selector.resolution_strategy(),
                )?;
                // Emit a request to fetch the metadata for each registry package.
                for dependency in &fork.dependencies {
                    let PubGrubDependency {
                        package,
                        version: _,
                        specifier: _,
                        url: _,
                    } = dependency;
                    let url = package
                        .name()
                        .and_then(|name| forked_state.fork_urls.get(name));
                    self.visit_package(package, url, request_sink)?;
                }
                Ok(forked_state)
            })
            // Drop any forked states whose markers are known to never
            // match any marker environments.
            .filter(|result| {
                if let Ok(ref forked_state) = result {
                    let markers = forked_state.markers.fork_markers().expect("is a fork");
                    if markers.is_false() {
                        return false;
                    }
                }
                true
            })
    }

    /// Visit a [`PubGrubPackage`] prior to selection. This should be called on a [`PubGrubPackage`]
    /// before it is selected, to allow metadata to be fetched in parallel.
    fn visit_package(
        &self,
        package: &PubGrubPackage,
        url: Option<&VerbatimParsedUrl>,
        request_sink: &Sender<Request>,
    ) -> Result<(), ResolveError> {
        // Ignore unresolved URL packages.
        if url.is_none()
            && package
                .name()
                .map(|name| self.urls.any_url(name))
                .unwrap_or(true)
        {
            return Ok(());
        }

        self.request_package(package, url, request_sink)
    }

    fn request_package(
        &self,
        package: &PubGrubPackage,
        url: Option<&VerbatimParsedUrl>,
        request_sink: &Sender<Request>,
    ) -> Result<(), ResolveError> {
        // Only request real package
        let Some(name) = package.name_no_root() else {
            return Ok(());
        };

        if let Some(url) = url {
            // Verify that the package is allowed under the hash-checking policy.
            if !self.hasher.allows_url(&url.verbatim) {
                return Err(ResolveError::UnhashedPackage(name.clone()));
            }

            // Emit a request to fetch the metadata for this distribution.
            let dist = Dist::from_url(name.clone(), url.clone())?;
            if self.index.distributions().register(dist.version_id()) {
                request_sink.blocking_send(Request::Dist(dist))?;
            }
        } else {
            // Emit a request to fetch the metadata for this package.
            if self.index.packages().register(name.clone()) {
                request_sink.blocking_send(Request::Package(name.clone()))?;
            }
        }
        Ok(())
    }

    /// Visit the set of [`PubGrubPackage`] candidates prior to selection. This allows us to fetch
    /// metadata for all packages in parallel.
    fn pre_visit<'data>(
        packages: impl Iterator<Item = (&'data PubGrubPackage, &'data Range<Version>)>,
        urls: &Urls,
        python_requirement: &PythonRequirement,
        request_sink: &Sender<Request>,
    ) -> Result<(), ResolveError> {
        // Iterate over the potential packages, and fetch file metadata for any of them. These
        // represent our current best guesses for the versions that we _might_ select.
        for (package, range) in packages {
            let PubGrubPackageInner::Package {
                name,
                extra: None,
                dev: None,
                marker: None,
            } = &**package
            else {
                continue;
            };
            // Avoid pre-visiting packages that have any URLs in any fork. At this point we can't
            // tell whether they are registry distributions or which url they use.
            if urls.any_url(name) {
                continue;
            }
            request_sink.blocking_send(Request::Prefetch(
                name.clone(),
                range.clone(),
                python_requirement.clone(),
            ))?;
        }
        Ok(())
    }

    /// Given a candidate package, choose the next version in range to try.
    ///
    /// Returns `None` when there are no versions in the given range, rejecting the current partial
    /// solution.
    #[instrument(skip_all, fields(%package))]
    fn choose_version(
        &self,
        package: &PubGrubPackage,
        range: &Range<Version>,
        pins: &mut FilePins,
        preferences: &Preferences,
        fork_urls: &ForkUrls,
        fork_markers: &ResolverMarkers,
        python_requirement: &PythonRequirement,
        visited: &mut FxHashSet<PackageName>,
        request_sink: &Sender<Request>,
    ) -> Result<Option<ResolverVersion>, ResolveError> {
        match &**package {
            PubGrubPackageInner::Root(_) => {
                Ok(Some(ResolverVersion::Available(MIN_VERSION.clone())))
            }

            PubGrubPackageInner::Python(_) => {
                // Dependencies on Python are only added when a package is incompatible; as such,
                // we don't need to do anything here.
                // we don't need to do anything here.
                Ok(None)
            }

            PubGrubPackageInner::Marker { name, .. }
            | PubGrubPackageInner::Extra { name, .. }
            | PubGrubPackageInner::Dev { name, .. }
            | PubGrubPackageInner::Package { name, .. } => {
                if let Some(url) = package.name().and_then(|name| fork_urls.get(name)) {
                    self.choose_version_url(name, range, url, python_requirement)
                } else {
                    self.choose_version_registry(
                        name,
                        range,
                        package,
                        preferences,
                        fork_markers,
                        python_requirement,
                        pins,
                        visited,
                        request_sink,
                    )
                }
            }
        }
    }

    /// Select a version for a URL requirement. Since there is only one version per URL, we return
    /// that version if it is in range and `None` otherwise.
    fn choose_version_url(
        &self,
        name: &PackageName,
        range: &Range<Version>,
        url: &VerbatimParsedUrl,
        python_requirement: &PythonRequirement,
    ) -> Result<Option<ResolverVersion>, ResolveError> {
        debug!(
            "Searching for a compatible version of {name} @ {} ({range})",
            url.verbatim
        );

        let dist = PubGrubDistribution::from_url(name, url);
        let response = self
            .index
            .distributions()
            .wait_blocking(&dist.version_id())
            .ok_or_else(|| ResolveError::UnregisteredTask(dist.version_id().to_string()))?;

        // If we failed to fetch the metadata for a URL, we can't proceed.
        let metadata = match &*response {
            MetadataResponse::Found(archive) => &archive.metadata,
            MetadataResponse::Offline => {
                self.unavailable_packages
                    .insert(name.clone(), UnavailablePackage::Offline);
                return Ok(None);
            }
            MetadataResponse::MissingMetadata => {
                self.unavailable_packages
                    .insert(name.clone(), UnavailablePackage::MissingMetadata);
                return Ok(None);
            }
            MetadataResponse::InvalidMetadata(err) => {
                self.unavailable_packages.insert(
                    name.clone(),
                    UnavailablePackage::InvalidMetadata(err.to_string()),
                );
                return Ok(None);
            }
            MetadataResponse::InconsistentMetadata(err) => {
                self.unavailable_packages.insert(
                    name.clone(),
                    UnavailablePackage::InvalidMetadata(err.to_string()),
                );
                return Ok(None);
            }
            MetadataResponse::InvalidStructure(err) => {
                self.unavailable_packages.insert(
                    name.clone(),
                    UnavailablePackage::InvalidStructure(err.to_string()),
                );
                return Ok(None);
            }
        };

        let version = &metadata.version;

        // The version is incompatible with the requirement.
        if !range.contains(version) {
            return Ok(None);
        }

        // The version is incompatible due to its Python requirement.
        if let Some(requires_python) = metadata.requires_python.as_ref() {
            if !python_requirement
                .installed()
                .is_contained_by(requires_python)
            {
                return Ok(Some(ResolverVersion::Unavailable(
                    version.clone(),
                    UnavailableVersion::IncompatibleDist(IncompatibleDist::Source(
                        IncompatibleSource::RequiresPython(
                            requires_python.clone(),
                            PythonRequirementKind::Installed,
                        ),
                    )),
                )));
            }
            if !python_requirement.target().is_contained_by(requires_python) {
                return Ok(Some(ResolverVersion::Unavailable(
                    version.clone(),
                    UnavailableVersion::IncompatibleDist(IncompatibleDist::Source(
                        IncompatibleSource::RequiresPython(
                            requires_python.clone(),
                            PythonRequirementKind::Target,
                        ),
                    )),
                )));
            }
        }

        Ok(Some(ResolverVersion::Available(version.clone())))
    }

    /// Given a candidate registry requirement, choose the next version in range to try, or `None`
    /// if there is no version in this range.
    fn choose_version_registry(
        &self,
        name: &PackageName,
        range: &Range<Version>,
        package: &PubGrubPackage,
        preferences: &Preferences,
        fork_markers: &ResolverMarkers,
        python_requirement: &PythonRequirement,
        pins: &mut FilePins,
        visited: &mut FxHashSet<PackageName>,
        request_sink: &Sender<Request>,
    ) -> Result<Option<ResolverVersion>, ResolveError> {
        // Wait for the metadata to be available.
        let versions_response = self
            .index
            .packages()
            .wait_blocking(name)
            .ok_or_else(|| ResolveError::UnregisteredTask(name.to_string()))?;
        visited.insert(name.clone());

        let version_maps = match *versions_response {
            VersionsResponse::Found(ref version_maps) => version_maps.as_slice(),
            VersionsResponse::NoIndex => {
                self.unavailable_packages
                    .insert(name.clone(), UnavailablePackage::NoIndex);
                &[]
            }
            VersionsResponse::Offline => {
                self.unavailable_packages
                    .insert(name.clone(), UnavailablePackage::Offline);
                &[]
            }
            VersionsResponse::NotFound => {
                self.unavailable_packages
                    .insert(name.clone(), UnavailablePackage::NotFound);
                &[]
            }
        };

        debug!("Searching for a compatible version of {package} ({range})");

        // Find a version.
        let Some(candidate) = self.selector.select(
            name,
            range,
            version_maps,
            preferences,
            &self.installed_packages,
            &self.exclusions,
            fork_markers,
        ) else {
            // Short circuit: we couldn't find _any_ versions for a package.
            return Ok(None);
        };

        let dist = match candidate.dist() {
            CandidateDist::Compatible(dist) => dist,
            CandidateDist::Incompatible(incompatibility) => {
                // If the version is incompatible because no distributions are compatible, exit early.
                return Ok(Some(ResolverVersion::Unavailable(
                    candidate.version().clone(),
                    UnavailableVersion::IncompatibleDist(incompatibility.clone()),
                )));
            }
        };

        let incompatibility = match dist {
            CompatibleDist::InstalledDist(_) => None,
            CompatibleDist::SourceDist { sdist, .. }
            | CompatibleDist::IncompatibleWheel { sdist, .. } => {
                // Source distributions must meet both the _target_ Python version and the
                // _installed_ Python version (to build successfully).
                sdist
                    .file
                    .requires_python
                    .as_ref()
                    .and_then(|requires_python| {
                        if !python_requirement
                            .installed()
                            .is_contained_by(requires_python)
                        {
                            return Some(IncompatibleDist::Source(
                                IncompatibleSource::RequiresPython(
                                    requires_python.clone(),
                                    PythonRequirementKind::Installed,
                                ),
                            ));
                        }
                        if !python_requirement.target().is_contained_by(requires_python) {
                            return Some(IncompatibleDist::Source(
                                IncompatibleSource::RequiresPython(
                                    requires_python.clone(),
                                    PythonRequirementKind::Target,
                                ),
                            ));
                        }
                        None
                    })
            }
            CompatibleDist::CompatibleWheel { wheel, .. } => {
                // Wheels must meet the _target_ Python version.
                wheel
                    .file
                    .requires_python
                    .as_ref()
                    .and_then(|requires_python| {
                        if python_requirement.installed() == python_requirement.target() {
                            if !python_requirement
                                .installed()
                                .is_contained_by(requires_python)
                            {
                                return Some(IncompatibleDist::Wheel(
                                    IncompatibleWheel::RequiresPython(
                                        requires_python.clone(),
                                        PythonRequirementKind::Installed,
                                    ),
                                ));
                            }
                        } else {
                            if !python_requirement.target().is_contained_by(requires_python) {
                                return Some(IncompatibleDist::Wheel(
                                    IncompatibleWheel::RequiresPython(
                                        requires_python.clone(),
                                        PythonRequirementKind::Target,
                                    ),
                                ));
                            }
                        }
                        None
                    })
            }
        };

        // The version is incompatible due to its Python requirement.
        if let Some(incompatibility) = incompatibility {
            return Ok(Some(ResolverVersion::Unavailable(
                candidate.version().clone(),
                UnavailableVersion::IncompatibleDist(incompatibility),
            )));
        }

        let filename = match dist.for_installation() {
            ResolvedDistRef::InstallableRegistrySourceDist { sdist, .. } => sdist
                .filename()
                .unwrap_or(Cow::Borrowed("unknown filename")),
            ResolvedDistRef::InstallableRegistryBuiltDist { wheel, .. } => wheel
                .filename()
                .unwrap_or(Cow::Borrowed("unknown filename")),
            ResolvedDistRef::Installed(_) => Cow::Borrowed("installed"),
        };

        debug!(
            "Selecting: {}=={} [{}] ({})",
            name,
            candidate.version(),
            candidate.choice_kind(),
            filename,
        );

        // We want to return a package pinned to a specific version; but we _also_ want to
        // store the exact file that we selected to satisfy that version.
        pins.insert(&candidate, dist);

        let version = candidate.version().clone();

        // Emit a request to fetch the metadata for this version.
        if matches!(&**package, PubGrubPackageInner::Package { .. }) {
            if self.dependency_mode.is_transitive() {
                if self.index.distributions().register(candidate.version_id()) {
                    // Verify that the package is allowed under the hash-checking policy.
                    if !self
                        .hasher
                        .allows_package(candidate.name(), candidate.version())
                    {
                        return Err(ResolveError::UnhashedPackage(candidate.name().clone()));
                    }

                    let request = Request::from(dist.for_resolution());
                    request_sink.blocking_send(request)?;
                }
            }
        }

        Ok(Some(ResolverVersion::Available(version)))
    }

    /// Given a candidate package and version, return its dependencies.
    #[instrument(skip_all, fields(%package, %version))]
    fn get_dependencies_forking(
        &self,
        package: &PubGrubPackage,
        version: &Version,
        fork_urls: &ForkUrls,
        markers: &ResolverMarkers,
        python_requirement: &PythonRequirement,
    ) -> Result<ForkedDependencies, ResolveError> {
        let result =
            self.get_dependencies(package, version, fork_urls, markers, python_requirement);
        match markers {
            ResolverMarkers::SpecificEnvironment(_) => result.map(|deps| match deps {
                Dependencies::Available(deps) | Dependencies::Unforkable(deps) => {
                    ForkedDependencies::Unforked(deps)
                }
                Dependencies::Unavailable(err) => ForkedDependencies::Unavailable(err),
            }),
            ResolverMarkers::Universal { .. } | ResolverMarkers::Fork(_) => {
                Ok(result?.fork(python_requirement))
            }
        }
    }

    /// Given a candidate package and version, return its dependencies.
    #[instrument(skip_all, fields(%package, %version))]
    fn get_dependencies(
        &self,
        package: &PubGrubPackage,
        version: &Version,
        fork_urls: &ForkUrls,
        markers: &ResolverMarkers,
        python_requirement: &PythonRequirement,
    ) -> Result<Dependencies, ResolveError> {
        let url = package.name().and_then(|name| fork_urls.get(name));
        let dependencies = match &**package {
            PubGrubPackageInner::Root(_) => {
                let no_dev_deps = BTreeMap::default();
                let requirements = self.flatten_requirements(
                    &self.requirements,
                    &no_dev_deps,
                    None,
                    None,
                    None,
                    markers,
                    python_requirement,
                );

                requirements
                    .iter()
                    .flat_map(|requirement| PubGrubDependency::from_requirement(requirement, None))
                    .collect::<Result<Vec<_>, _>>()?
            }
            PubGrubPackageInner::Package {
                name,
                extra,
                dev,
                marker,
            } => {
                // If we're excluding transitive dependencies, short-circuit.
                if self.dependency_mode.is_direct() {
                    return Ok(Dependencies::Unforkable(Vec::default()));
                }

                // Determine the distribution to lookup.
                let dist = match url {
                    Some(url) => PubGrubDistribution::from_url(name, url),
                    None => PubGrubDistribution::from_registry(name, version),
                };
                let version_id = dist.version_id();

                // If the package does not exist in the registry or locally, we cannot fetch its dependencies
                if self.dependency_mode.is_transitive()
                    && self.unavailable_packages.get(name).is_some()
                    && self.installed_packages.get_packages(name).is_empty()
                {
                    debug_assert!(
                        false,
                        "Dependencies were requested for a package that is not available"
                    );
                    return Err(ResolveError::Failure(format!(
                        "The package is unavailable: {name}"
                    )));
                }

                // Wait for the metadata to be available.
                let response = self
                    .index
                    .distributions()
                    .wait_blocking(&version_id)
                    .ok_or_else(|| ResolveError::UnregisteredTask(version_id.to_string()))?;

                let metadata = match &*response {
                    MetadataResponse::Found(archive) => &archive.metadata,
                    MetadataResponse::Offline => {
                        self.incomplete_packages
                            .entry(name.clone())
                            .or_default()
                            .insert(version.clone(), IncompletePackage::Offline);
                        return Ok(Dependencies::Unavailable(UnavailableVersion::Offline));
                    }
                    MetadataResponse::MissingMetadata => {
                        self.incomplete_packages
                            .entry(name.clone())
                            .or_default()
                            .insert(version.clone(), IncompletePackage::MissingMetadata);
                        return Ok(Dependencies::Unavailable(
                            UnavailableVersion::MissingMetadata,
                        ));
                    }
                    MetadataResponse::InvalidMetadata(err) => {
                        warn!("Unable to extract metadata for {name}: {err}");
                        self.incomplete_packages
                            .entry(name.clone())
                            .or_default()
                            .insert(
                                version.clone(),
                                IncompletePackage::InvalidMetadata(err.to_string()),
                            );
                        return Ok(Dependencies::Unavailable(
                            UnavailableVersion::InvalidMetadata,
                        ));
                    }
                    MetadataResponse::InconsistentMetadata(err) => {
                        warn!("Unable to extract metadata for {name}: {err}");
                        self.incomplete_packages
                            .entry(name.clone())
                            .or_default()
                            .insert(
                                version.clone(),
                                IncompletePackage::InconsistentMetadata(err.to_string()),
                            );
                        return Ok(Dependencies::Unavailable(
                            UnavailableVersion::InconsistentMetadata,
                        ));
                    }
                    MetadataResponse::InvalidStructure(err) => {
                        warn!("Unable to extract metadata for {name}: {err}");
                        self.incomplete_packages
                            .entry(name.clone())
                            .or_default()
                            .insert(
                                version.clone(),
                                IncompletePackage::InvalidStructure(err.to_string()),
                            );
                        return Ok(Dependencies::Unavailable(
                            UnavailableVersion::InvalidStructure,
                        ));
                    }
                };

                let requirements = self.flatten_requirements(
                    &metadata.requires_dist,
                    &metadata.dev_dependencies,
                    extra.as_ref(),
                    dev.as_ref(),
                    Some(name),
                    markers,
                    python_requirement,
                );

                let mut dependencies = requirements
                    .iter()
                    .flat_map(|requirement| {
                        PubGrubDependency::from_requirement(requirement, Some(name))
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                // If a package has metadata for an enabled dependency group,
                // add a dependency from it to the same package with the group
                // enabled.
                if extra.is_none() && dev.is_none() {
                    for group in self.groups.get(name).into_iter().flatten() {
                        if !metadata.dev_dependencies.contains_key(group) {
                            continue;
                        }
                        dependencies.push(PubGrubDependency {
                            package: PubGrubPackage::from(PubGrubPackageInner::Dev {
                                name: name.clone(),
                                dev: group.clone(),
                                marker: marker.clone(),
                            }),
                            version: Range::singleton(version.clone()),
                            specifier: None,
                            url: None,
                        });
                    }
                }

                dependencies
            }
            PubGrubPackageInner::Python(_) => return Ok(Dependencies::Unforkable(Vec::default())),

            // Add a dependency on both the marker and base package.
            PubGrubPackageInner::Marker { name, marker } => {
                return Ok(Dependencies::Unforkable(
                    [None, Some(marker)]
                        .into_iter()
                        .map(move |marker| PubGrubDependency {
                            package: PubGrubPackage::from(PubGrubPackageInner::Package {
                                name: name.clone(),
                                extra: None,
                                dev: None,
                                marker: marker.and_then(MarkerTree::contents),
                            }),
                            version: Range::singleton(version.clone()),
                            specifier: None,
                            url: None,
                        })
                        .collect(),
                ))
            }

            // Add a dependency on both the extra and base package, with and without the marker.
            PubGrubPackageInner::Extra {
                name,
                extra,
                marker,
            } => {
                return Ok(Dependencies::Unforkable(
                    [None, marker.as_ref()]
                        .into_iter()
                        .dedup()
                        .flat_map(move |marker| {
                            [None, Some(extra)]
                                .into_iter()
                                .map(move |extra| PubGrubDependency {
                                    package: PubGrubPackage::from(PubGrubPackageInner::Package {
                                        name: name.clone(),
                                        extra: extra.cloned(),
                                        dev: None,
                                        marker: marker.cloned(),
                                    }),
                                    version: Range::singleton(version.clone()),
                                    specifier: None,
                                    url: None,
                                })
                        })
                        .collect(),
                ))
            }

            // Add a dependency on both the development dependency group and base package, with and
            // without the marker.
            PubGrubPackageInner::Dev { name, dev, marker } => {
                return Ok(Dependencies::Unforkable(
                    [None, marker.as_ref()]
                        .into_iter()
                        .dedup()
                        .flat_map(move |marker| {
                            [None, Some(dev)]
                                .into_iter()
                                .map(move |dev| PubGrubDependency {
                                    package: PubGrubPackage::from(PubGrubPackageInner::Package {
                                        name: name.clone(),
                                        extra: None,
                                        dev: dev.cloned(),
                                        marker: marker.cloned(),
                                    }),
                                    version: Range::singleton(version.clone()),
                                    specifier: None,
                                    url: None,
                                })
                        })
                        .collect(),
                ))
            }
        };
        Ok(Dependencies::Available(dependencies))
    }

    /// The regular and dev dependencies filtered by Python version and the markers of this fork,
    /// plus the extras dependencies of the current package (e.g., `black` depending on
    /// `black[colorama]`).
    fn flatten_requirements<'a>(
        &'a self,
        dependencies: &'a [Requirement],
        dev_dependencies: &'a BTreeMap<GroupName, Vec<Requirement>>,
        extra: Option<&'a ExtraName>,
        dev: Option<&'a GroupName>,
        name: Option<&PackageName>,
        markers: &'a ResolverMarkers,
        python_requirement: &'a PythonRequirement,
    ) -> Vec<Cow<'a, Requirement>> {
        // Start with the requirements for the current extra of the package (for an extra
        // requirement) or the non-extra (regular) dependencies (if extra is None), plus
        // the constraints for the current package.
        let regular_and_dev_dependencies = if let Some(dev) = dev {
            Either::Left(dev_dependencies.get(dev).into_iter().flatten())
        } else {
            Either::Right(dependencies.iter())
        };
        let mut requirements = self
            .requirements_for_extra(
                regular_and_dev_dependencies,
                extra,
                markers,
                python_requirement,
            )
            .collect::<Vec<_>>();

        // Check if there are recursive self inclusions and we need to go into the expensive branch.
        if !requirements
            .iter()
            .any(|req| name == Some(&req.name) && !req.extras.is_empty())
        {
            return requirements;
        }

        // Transitively process all extras that are recursively included, starting with the current
        // extra.
        let mut seen = FxHashSet::default();
        let mut queue: VecDeque<_> = requirements
            .iter()
            .filter(|req| name == Some(&req.name))
            .flat_map(|req| req.extras.iter().cloned())
            .collect();
        while let Some(extra) = queue.pop_front() {
            if !seen.insert(extra.clone()) {
                continue;
            }
            for requirement in
                self.requirements_for_extra(dependencies, Some(&extra), markers, python_requirement)
            {
                if name == Some(&requirement.name) {
                    // Add each transitively included extra.
                    queue.extend(requirement.extras.iter().cloned());
                } else {
                    // Add the requirements for that extra.
                    requirements.push(requirement);
                }
            }
        }

        // Drop all the self-requirements now that we flattened them out.
        requirements.retain(|req| name != Some(&req.name));

        requirements
    }

    /// The set of the regular and dev dependencies, filtered by Python version,
    /// the markers of this fork and the requested extra.
    fn requirements_for_extra<'data, 'parameters>(
        &'data self,
        dependencies: impl IntoIterator<Item = &'data Requirement> + 'parameters,
        extra: Option<&'parameters ExtraName>,
        markers: &'parameters ResolverMarkers,
        python_requirement: &'parameters PythonRequirement,
    ) -> impl Iterator<Item = Cow<'data, Requirement>> + 'parameters
    where
        'data: 'parameters,
    {
        self.overrides
            .apply(dependencies)
            .filter_map(move |requirement| {
                let python_marker = python_requirement.to_marker_tree();
                // If the requirement would not be selected with any Python version
                // supported by the root, skip it.
                if python_marker.is_disjoint(&requirement.marker) {
                    trace!(
                        "skipping {requirement} because of Requires-Python: {requires_python}",
                        requires_python = python_requirement.target(),
                    );
                    return None;
                }

                // If we're in a fork in universal mode, ignore any dependency that isn't part of
                // this fork (but will be part of another fork).
                if let ResolverMarkers::Fork(markers) = markers {
                    if markers.is_disjoint(&requirement.marker) {
                        trace!("skipping {requirement} because of context resolver markers {markers:?}");
                        return None;
                    }
                }

                // If the requirement isn't relevant for the current platform, skip it.
                match extra {
                    Some(source_extra) => {
                        // Only include requirements that are relevant for the current extra.
                        if requirement.evaluate_markers(markers.marker_environment(), &[]) {
                            return None;
                        }
                        if !requirement.evaluate_markers(
                            markers.marker_environment(),
                            std::slice::from_ref(source_extra),
                        ) {
                            return None;
                        }
                    }
                    None => {
                        if !requirement.evaluate_markers(markers.marker_environment(), &[]) {
                            return None;
                        }
                    }
                }

                Some(requirement)
            })
            .flat_map(move |requirement| {
                iter::once(requirement.clone()).chain(
                    self.constraints
                        .get(&requirement.name)
                        .into_iter()
                        .flatten()
                        .filter_map(move |constraint| {
                            // If the requirement would not be selected with any Python version
                            // supported by the root, skip it.
                            let constraint = if constraint.marker.is_true() {
                                // Additionally, if the requirement is `requests ; sys_platform == 'darwin'`
                                // and the constraint is `requests ; python_version == '3.6'`, the
                                // constraint should only apply when _both_ markers are true.
                                if requirement.marker.is_true() {
                                    Cow::Borrowed(constraint)
                                } else {
                                    let mut marker = constraint.marker.clone();
                                    marker.and(requirement.marker.clone());

                                    Cow::Owned(Requirement {
                                        name: constraint.name.clone(),
                                        extras: constraint.extras.clone(),
                                        source: constraint.source.clone(),
                                        origin: constraint.origin.clone(),
                                        marker
                                    })
                                }

                            } else {
                                let requires_python = python_requirement.target();
                                let python_marker = python_requirement.to_marker_tree();

                                let mut marker = constraint.marker.clone();
                                marker.and(requirement.marker.clone());

                                // Additionally, if the requirement is `requests ; sys_platform == 'darwin'`
                                // and the constraint is `requests ; python_version == '3.6'`, the
                                // constraint should only apply when _both_ markers are true.
                                if marker.is_false() {
                                    trace!("skipping {constraint} because of Requires-Python: {requires_python}");
                                    return None;
                                }
                                if python_marker.is_disjoint(&marker) {
                                    trace!(
                                        "skipping constraint {requirement} because of Requires-Python: {requires_python}",
                                        requires_python = python_requirement.target(),
                                    );
                                    return None;
                                }

                                if marker == constraint.marker {
                                    Cow::Borrowed(constraint)
                                } else {
                                    Cow::Owned(Requirement {
                                        name: constraint.name.clone(),
                                        extras: constraint.extras.clone(),
                                        source: constraint.source.clone(),
                                        origin: constraint.origin.clone(),
                                        marker
                                    })
                                }
                            };

                            // If we're in a fork in universal mode, ignore any dependency that isn't part of
                            // this fork (but will be part of another fork).
                            if let ResolverMarkers::Fork(markers) = markers {
                                if markers.is_disjoint(&constraint.marker) {
                                    trace!("skipping {constraint} because of context resolver markers {markers:?}");
                                    return None;
                                }
                            }

                            // If the constraint isn't relevant for the current platform, skip it.
                            match extra {
                                Some(source_extra) => {
                                    if !constraint.evaluate_markers(
                                        markers.marker_environment(),
                                        std::slice::from_ref(source_extra),
                                    ) {
                                        return None;
                                    }
                                }
                                None => {
                                    if !constraint.evaluate_markers(markers.marker_environment(), &[]) {
                                        return None;
                                    }
                                }
                            }

                            Some(constraint)
                        })
                )
            })
    }

    /// Fetch the metadata for a stream of packages and versions.
    async fn fetch<Provider: ResolverProvider>(
        self: Arc<Self>,
        provider: Arc<Provider>,
        request_stream: Receiver<Request>,
    ) -> Result<(), ResolveError> {
        let mut response_stream = ReceiverStream::new(request_stream)
            .map(|request| self.process_request(request, &*provider).boxed_local())
            // Allow as many futures as possible to start in the background.
            // Backpressure is provided by at a more granular level by `DistributionDatabase`
            // and `SourceDispatch`, as well as the bounded request channel.
            .buffer_unordered(usize::MAX);

        while let Some(response) = response_stream.next().await {
            match response? {
                Some(Response::Package(package_name, version_map)) => {
                    trace!("Received package metadata for: {package_name}");
                    self.index
                        .packages()
                        .done(package_name, Arc::new(version_map));
                }
                Some(Response::Installed { dist, metadata }) => {
                    trace!("Received installed distribution metadata for: {dist}");
                    self.index.distributions().done(
                        dist.version_id(),
                        Arc::new(MetadataResponse::Found(ArchiveMetadata::from_metadata23(
                            metadata,
                        ))),
                    );
                }
                Some(Response::Dist {
                    dist: Dist::Built(dist),
                    metadata,
                }) => {
                    trace!("Received built distribution metadata for: {dist}");
                    match &metadata {
                        MetadataResponse::InvalidMetadata(err) => {
                            warn!("Unable to extract metadata for {dist}: {err}");
                        }
                        MetadataResponse::InvalidStructure(err) => {
                            warn!("Unable to extract metadata for {dist}: {err}");
                        }
                        _ => {}
                    }
                    self.index
                        .distributions()
                        .done(dist.version_id(), Arc::new(metadata));
                }
                Some(Response::Dist {
                    dist: Dist::Source(dist),
                    metadata,
                }) => {
                    trace!("Received source distribution metadata for: {dist}");
                    match &metadata {
                        MetadataResponse::InvalidMetadata(err) => {
                            warn!("Unable to extract metadata for {dist}: {err}");
                        }
                        MetadataResponse::InvalidStructure(err) => {
                            warn!("Unable to extract metadata for {dist}: {err}");
                        }
                        _ => {}
                    }
                    self.index
                        .distributions()
                        .done(dist.version_id(), Arc::new(metadata));
                }
                None => {}
            }
        }

        Ok::<(), ResolveError>(())
    }

    #[instrument(skip_all, fields(%request))]
    async fn process_request<Provider: ResolverProvider>(
        &self,
        request: Request,
        provider: &Provider,
    ) -> Result<Option<Response>, ResolveError> {
        match request {
            // Fetch package metadata from the registry.
            Request::Package(package_name) => {
                let package_versions = provider
                    .get_package_versions(&package_name)
                    .boxed_local()
                    .await
                    .map_err(ResolveError::Client)?;

                Ok(Some(Response::Package(package_name, package_versions)))
            }

            // Fetch distribution metadata from the distribution database.
            Request::Dist(dist) => {
                let metadata = provider
                    .get_or_build_wheel_metadata(&dist)
                    .boxed_local()
                    .await
                    .map_err(|err| match dist.clone() {
                        Dist::Built(built_dist @ BuiltDist::Path(_)) => {
                            ResolveError::Read(Box::new(built_dist), err)
                        }
                        Dist::Source(source_dist @ SourceDist::Path(_)) => {
                            ResolveError::Build(Box::new(source_dist), err)
                        }
                        Dist::Source(source_dist @ SourceDist::Directory(_)) => {
                            ResolveError::Build(Box::new(source_dist), err)
                        }
                        Dist::Built(built_dist) => ResolveError::Fetch(Box::new(built_dist), err),
                        Dist::Source(source_dist) => {
                            if source_dist.is_local() {
                                ResolveError::Build(Box::new(source_dist), err)
                            } else {
                                ResolveError::FetchAndBuild(Box::new(source_dist), err)
                            }
                        }
                    })?;

                Ok(Some(Response::Dist { dist, metadata }))
            }

            Request::Installed(dist) => {
                let metadata = dist
                    .metadata()
                    .map_err(|err| ResolveError::ReadInstalled(Box::new(dist.clone()), err))?;
                Ok(Some(Response::Installed { dist, metadata }))
            }

            // Pre-fetch the package and distribution metadata.
            Request::Prefetch(package_name, range, python_requirement) => {
                // Wait for the package metadata to become available.
                let versions_response = self
                    .index
                    .packages()
                    .wait(&package_name)
                    .await
                    .ok_or_else(|| ResolveError::UnregisteredTask(package_name.to_string()))?;

                let version_map = match *versions_response {
                    VersionsResponse::Found(ref version_map) => version_map,
                    // Short-circuit if we did not find any versions for the package
                    VersionsResponse::NoIndex => {
                        self.unavailable_packages
                            .insert(package_name.clone(), UnavailablePackage::NoIndex);

                        return Ok(None);
                    }
                    VersionsResponse::Offline => {
                        self.unavailable_packages
                            .insert(package_name.clone(), UnavailablePackage::Offline);

                        return Ok(None);
                    }
                    VersionsResponse::NotFound => {
                        self.unavailable_packages
                            .insert(package_name.clone(), UnavailablePackage::NotFound);

                        return Ok(None);
                    }
                };

                // Try to find a compatible version. If there aren't any compatible versions,
                // short-circuit.
                let Some(candidate) = self.selector.select(
                    &package_name,
                    &range,
                    version_map,
                    &self.preferences,
                    &self.installed_packages,
                    &self.exclusions,
                    // We don't have access to the fork state when prefetching, so assume that
                    // pre-release versions are allowed.
                    &ResolverMarkers::universal(vec![]),
                ) else {
                    return Ok(None);
                };

                // If there is not a compatible distribution, short-circuit.
                let Some(dist) = candidate.compatible() else {
                    return Ok(None);
                };

                // Avoid prefetching source distributions with unbounded lower-bound ranges. This
                // often leads to failed attempts to build legacy versions of packages that are
                // incompatible with modern build tools.
                if dist.wheel().is_some() {
                    if !self.selector.use_highest_version(&package_name) {
                        if let Some((lower, _)) = range.iter().next() {
                            if lower == &Bound::Unbounded {
                                debug!("Skipping prefetch for unbounded minimum-version range: {package_name} ({range})");
                                return Ok(None);
                            }
                        }
                    }
                }

                match dist {
                    CompatibleDist::InstalledDist(_) => {}
                    CompatibleDist::SourceDist { sdist, .. }
                    | CompatibleDist::IncompatibleWheel { sdist, .. } => {
                        // Source distributions must meet both the _target_ Python version and the
                        // _installed_ Python version (to build successfully).
                        if let Some(requires_python) = sdist.file.requires_python.as_ref() {
                            if !python_requirement
                                .installed()
                                .is_contained_by(requires_python)
                            {
                                return Ok(None);
                            }
                            if !python_requirement.target().is_contained_by(requires_python) {
                                return Ok(None);
                            }
                        }
                    }
                    CompatibleDist::CompatibleWheel { wheel, .. } => {
                        // Wheels must meet the _target_ Python version.
                        if let Some(requires_python) = wheel.file.requires_python.as_ref() {
                            if !python_requirement.target().is_contained_by(requires_python) {
                                return Ok(None);
                            }
                        }
                    }
                };

                // Emit a request to fetch the metadata for this version.
                if self.index.distributions().register(candidate.version_id()) {
                    // Verify that the package is allowed under the hash-checking policy.
                    if !self
                        .hasher
                        .allows_package(candidate.name(), candidate.version())
                    {
                        return Err(ResolveError::UnhashedPackage(candidate.name().clone()));
                    }

                    let dist = dist.for_resolution().to_owned();

                    let response = match dist {
                        ResolvedDist::Installable(dist) => {
                            let metadata = provider
                                .get_or_build_wheel_metadata(&dist)
                                .boxed_local()
                                .await
                                .map_err(|err| match dist.clone() {
                                    Dist::Built(built_dist @ BuiltDist::Path(_)) => {
                                        ResolveError::Read(Box::new(built_dist), err)
                                    }
                                    Dist::Source(source_dist @ SourceDist::Path(_)) => {
                                        ResolveError::Build(Box::new(source_dist), err)
                                    }
                                    Dist::Source(source_dist @ SourceDist::Directory(_)) => {
                                        ResolveError::Build(Box::new(source_dist), err)
                                    }
                                    Dist::Built(built_dist) => {
                                        ResolveError::Fetch(Box::new(built_dist), err)
                                    }
                                    Dist::Source(source_dist) => {
                                        if source_dist.is_local() {
                                            ResolveError::Build(Box::new(source_dist), err)
                                        } else {
                                            ResolveError::FetchAndBuild(Box::new(source_dist), err)
                                        }
                                    }
                                })?;

                            Response::Dist { dist, metadata }
                        }
                        ResolvedDist::Installed(dist) => {
                            let metadata = dist.metadata().map_err(|err| {
                                ResolveError::ReadInstalled(Box::new(dist.clone()), err)
                            })?;
                            Response::Installed { dist, metadata }
                        }
                    };

                    Ok(Some(response))
                } else {
                    Ok(None)
                }
            }
        }
    }

    fn convert_no_solution_err(
        &self,
        mut err: pubgrub::NoSolutionError<UvDependencyProvider>,
        fork_urls: ForkUrls,
        markers: ResolverMarkers,
        visited: &FxHashSet<PackageName>,
        index_locations: &IndexLocations,
    ) -> ResolveError {
        err = NoSolutionError::collapse_proxies(err);

        let mut unavailable_packages = FxHashMap::default();
        for package in err.packages() {
            if let PubGrubPackageInner::Package { name, .. } = &**package {
                if let Some(reason) = self.unavailable_packages.get(name) {
                    unavailable_packages.insert(name.clone(), reason.clone());
                }
            }
        }

        let mut incomplete_packages = FxHashMap::default();
        for package in err.packages() {
            if let PubGrubPackageInner::Package { name, .. } = &**package {
                if let Some(versions) = self.incomplete_packages.get(name) {
                    for entry in versions.iter() {
                        let (version, reason) = entry.pair();
                        incomplete_packages
                            .entry(name.clone())
                            .or_insert_with(BTreeMap::default)
                            .insert(version.clone(), reason.clone());
                    }
                }
            }
        }

        let mut available_versions = FxHashMap::default();
        for package in err.packages() {
            let Some(name) = package.name() else { continue };
            if !visited.contains(name) {
                // Avoid including available versions for packages that exist in the derivation
                // tree, but were never visited during resolution. We _may_ have metadata for
                // these packages, but it's non-deterministic, and omitting them ensures that
                // we represent the self of the resolver at the time of failure.
                continue;
            }
            if let Some(response) = self.index.packages().get(name) {
                if let VersionsResponse::Found(ref version_maps) = *response {
                    for version_map in version_maps {
                        available_versions
                            .entry(name.clone())
                            .or_insert_with(BTreeSet::new)
                            .extend(version_map.versions().cloned());
                    }
                }
            }
        }

        ResolveError::NoSolution(NoSolutionError::new(
            err,
            available_versions,
            self.selector.clone(),
            self.python_requirement.clone(),
            index_locations.clone(),
            unavailable_packages,
            incomplete_packages,
            fork_urls,
            markers,
            self.workspace_members.clone(),
        ))
    }

    fn on_progress(&self, package: &PubGrubPackage, version: &Version) {
        if let Some(reporter) = self.reporter.as_ref() {
            match &**package {
                PubGrubPackageInner::Root(_) => {}
                PubGrubPackageInner::Python(_) => {}
                PubGrubPackageInner::Marker { .. } => {}
                PubGrubPackageInner::Extra { .. } => {}
                PubGrubPackageInner::Dev { .. } => {}
                PubGrubPackageInner::Package { name, .. } => {
                    reporter.on_progress(name, &VersionOrUrlRef::Version(version));
                }
            }
        }
    }

    fn on_complete(&self) {
        if let Some(reporter) = self.reporter.as_ref() {
            reporter.on_complete();
        }
    }
}

/// State that is used during unit propagation in the resolver, one instance per fork.
#[derive(Clone)]
struct ForkState {
    /// The internal state used by the resolver.
    ///
    /// Note that not all parts of this state are strictly internal. For
    /// example, the edges in the dependency graph generated as part of the
    /// output of resolution are derived from the "incompatibilities" tracked
    /// in this state. We also ultimately retrieve the final set of version
    /// assignments (to packages) from this state's "partial solution."
    pubgrub: State<UvDependencyProvider>,
    /// The next package on which to run unit propagation.
    next: PubGrubPackage,
    /// The set of pinned versions we accrue throughout resolution.
    ///
    /// The key of this map is a package name, and each package name maps to
    /// a set of versions for that package. Each version in turn is mapped
    /// to a single [`ResolvedDist`]. That [`ResolvedDist`] represents, at time
    /// of writing (2024/05/09), at most one wheel. The idea here is that
    /// [`FilePins`] tracks precisely which wheel was selected during resolution.
    /// After resolution is finished, this maps is consulted in order to select
    /// the wheel chosen during resolution.
    pins: FilePins,
    /// Ensure we don't have duplicate urls in any branch.
    ///
    /// Unlike [`Urls`], we add only the URLs we have seen in this branch, and there can be only
    /// one URL per package. By prioritizing direct URL dependencies over registry dependencies,
    /// this map is populated for all direct URL packages before we look at any registry packages.
    fork_urls: ForkUrls,
    /// When dependencies for a package are retrieved, this map of priorities
    /// is updated based on how each dependency was specified. Certain types
    /// of dependencies have more "priority" than others (like direct URL
    /// dependencies). These priorities help determine which package to
    /// consider next during resolution.
    priorities: PubGrubPriorities,
    /// This keeps track of the set of versions for each package that we've
    /// already visited during resolution. This avoids doing redundant work.
    added_dependencies: FxHashMap<PubGrubPackage, FxHashSet<Version>>,
    /// The marker expression that created this state.
    ///
    /// The root state always corresponds to a marker expression that is always
    /// `true` for every `MarkerEnvironment`.
    ///
    /// In non-universal mode, forking never occurs and so this marker
    /// expression is always `true`.
    ///
    /// Whenever dependencies are fetched, all requirement specifications
    /// are checked for disjointness with the marker expression of the fork
    /// in which those dependencies were fetched. If a requirement has a
    /// completely disjoint marker expression (i.e., it can never be true given
    /// that the marker expression that provoked the fork is true), then that
    /// dependency is completely ignored.
    markers: ResolverMarkers,
    /// The Python requirement for this fork. Defaults to the Python requirement for
    /// the resolution, but may be narrowed if a `python_version` marker is present
    /// in a given fork.
    ///
    /// For example, in:
    /// ```text
    /// numpy >=1.26 ; python_version >= "3.9"
    /// numpy <1.26 ; python_version < "3.9"
    /// ```
    ///
    /// The top fork has a narrower Python compatibility range, and thus can find a
    /// solution that omits Python 3.8 support.
    python_requirement: PythonRequirement,
}

impl ForkState {
    fn new(
        pubgrub: State<UvDependencyProvider>,
        root: PubGrubPackage,
        markers: ResolverMarkers,
        python_requirement: PythonRequirement,
    ) -> Self {
        Self {
            pubgrub,
            next: root,
            pins: FilePins::default(),
            fork_urls: ForkUrls::default(),
            priorities: PubGrubPriorities::default(),
            added_dependencies: FxHashMap::default(),
            markers,
            python_requirement,
        }
    }

    /// Add the dependencies for the selected version of the current package, checking for
    /// self-dependencies, and handling URLs and locals.
    fn add_package_version_dependencies(
        &mut self,
        for_package: Option<&str>,
        version: &Version,
        urls: &Urls,
        locals: &Locals,
        mut dependencies: Vec<PubGrubDependency>,
        git: &GitResolver,
        resolution_strategy: &ResolutionStrategy,
    ) -> Result<(), ResolveError> {
        for dependency in &mut dependencies {
            let PubGrubDependency {
                package,
                version,
                specifier,
                url,
            } = dependency;

            let mut has_url = false;
            if let Some(name) = package.name() {
                // From the [`Requirement`] to [`PubGrubDependency`] conversion, we get a URL if the
                // requirement was a URL requirement. `Urls` applies canonicalization to this and
                // override URLs to both URL and registry requirements, which we then check for
                // conflicts using [`ForkUrl`].
                if let Some(url) = urls.get_url(name, url.as_ref(), git)? {
                    self.fork_urls.insert(name, url, &self.markers)?;
                    has_url = true;
                };

                // If the specifier is an exact version and the user requested a local version for this
                // fork that's more precise than the specifier, use the local version instead.
                if let Some(specifier) = specifier {
                    let locals = locals.get(name, &self.markers);

                    // It's possible that there are multiple matching local versions requested with
                    // different marker expressions. All of these are potentially compatible until we
                    // narrow to a specific fork.
                    for local in locals {
                        let local = specifier
                            .iter()
                            .map(|specifier| {
                                Locals::map(local, specifier)
                                    .map_err(ResolveError::InvalidVersion)
                                    .and_then(|specifier| {
                                        Ok(PubGrubSpecifier::from_pep440_specifier(&specifier)?)
                                    })
                            })
                            .fold_ok(Range::full(), |range, specifier| {
                                range.intersection(&specifier.into())
                            })?;

                        // Add the local version.
                        *version = version.union(&local);
                    }
                }
            }

            if let Some(for_package) = for_package {
                debug!("Adding transitive dependency for {for_package}: {package}{version}");
            } else {
                // A dependency from the root package or requirements.txt.
                debug!("Adding direct dependency: {package}{version}");

                // Warn the user if a direct dependency lacks a lower bound in `--lowest` resolution.
                let missing_lower_bound = version
                    .bounding_range()
                    .map(|(lowest, _highest)| lowest == Bound::Unbounded)
                    .unwrap_or(true);
                let strategy_lowest = matches!(
                    resolution_strategy,
                    ResolutionStrategy::Lowest | ResolutionStrategy::LowestDirect(..)
                );
                if !has_url && missing_lower_bound && strategy_lowest {
                    warn_user_once!(
                        "The direct dependency `{package}` is unpinned. \
                        Consider setting a lower bound when using `--resolution-strategy lowest` \
                        to avoid using outdated versions."
                    );
                }
            }

            // Update the package priorities.
            self.priorities.insert(package, version, &self.fork_urls);
        }

        self.pubgrub.add_package_version_dependencies(
            self.next.clone(),
            version.clone(),
            dependencies.into_iter().map(|dependency| {
                let PubGrubDependency {
                    package,
                    version,
                    specifier: _,
                    url: _,
                } = dependency;
                (package, version)
            }),
        );
        Ok(())
    }

    fn add_unavailable_version(
        &mut self,
        version: Version,
        reason: UnavailableVersion,
    ) -> Result<(), ResolveError> {
        // Incompatible requires-python versions are special in that we track
        // them as incompatible dependencies instead of marking the package version
        // as unavailable directly.
        if let UnavailableVersion::IncompatibleDist(
            IncompatibleDist::Source(IncompatibleSource::RequiresPython(requires_python, kind))
            | IncompatibleDist::Wheel(IncompatibleWheel::RequiresPython(requires_python, kind)),
        ) = reason
        {
            let python_version: Range<Version> =
                PubGrubSpecifier::from_release_specifiers(&requires_python)?.into();

            let package = &self.next;
            self.pubgrub
                .add_incompatibility(Incompatibility::from_dependency(
                    package.clone(),
                    Range::singleton(version.clone()),
                    (
                        PubGrubPackage::from(PubGrubPackageInner::Python(match kind {
                            PythonRequirementKind::Installed => PubGrubPython::Installed,
                            PythonRequirementKind::Target => PubGrubPython::Target,
                        })),
                        python_version.clone(),
                    ),
                ));
            self.pubgrub
                .partial_solution
                .add_decision(self.next.clone(), version);
            return Ok(());
        };
        self.pubgrub
            .add_incompatibility(Incompatibility::custom_version(
                self.next.clone(),
                version.clone(),
                UnavailableReason::Version(reason),
            ));
        Ok(())
    }

    /// Subset the current markers with the new markers and update the python requirements fields
    /// accordingly.
    fn with_markers(mut self, markers: MarkerTree) -> Self {
        let combined_markers = self.markers.and(markers);

        // If the fork contains a narrowed Python requirement, apply it.
        let python_requirement = marker::requires_python(&combined_markers)
            .and_then(|marker| self.python_requirement.narrow(&marker));
        if let Some(python_requirement) = python_requirement {
            debug!(
                "Narrowed `requires-python` bound to: {}",
                python_requirement.target()
            );
            self.python_requirement = python_requirement;
        }

        self.markers = ResolverMarkers::Fork(combined_markers);
        self
    }

    fn into_resolution(self) -> Resolution {
        let solution = self.pubgrub.partial_solution.extract_solution();
        let mut edges: FxHashSet<ResolutionDependencyEdge> = FxHashSet::default();
        for (package, self_version) in &solution {
            for id in &self.pubgrub.incompatibilities[package] {
                let pubgrub::Kind::FromDependencyOf(
                    ref self_package,
                    ref self_range,
                    ref dependency_package,
                    ref dependency_range,
                ) = self.pubgrub.incompatibility_store[*id].kind
                else {
                    continue;
                };
                if package != self_package {
                    continue;
                }
                if !self_range.contains(self_version) {
                    continue;
                }
                let Some(dependency_version) = solution.get(dependency_package) else {
                    continue;
                };
                if !dependency_range.contains(dependency_version) {
                    continue;
                }

                let (self_name, self_extra, self_dev) = match &**self_package {
                    PubGrubPackageInner::Package {
                        name: self_name,
                        extra: self_extra,
                        dev: self_dev,
                        ..
                    } => (Some(self_name), self_extra.as_ref(), self_dev.as_ref()),

                    PubGrubPackageInner::Root(_) => (None, None, None),

                    _ => continue,
                };
                let self_url = self_name.as_ref().and_then(|name| self.fork_urls.get(name));

                match **dependency_package {
                    PubGrubPackageInner::Package {
                        name: ref dependency_name,
                        extra: ref dependency_extra,
                        dev: ref dependency_dev,
                        ..
                    } => {
                        if self_name.is_some_and(|self_name| self_name == dependency_name) {
                            continue;
                        }
                        let to_url = self.fork_urls.get(dependency_name);
                        let edge = ResolutionDependencyEdge {
                            from: self_name.cloned(),
                            from_version: self_version.clone(),
                            from_url: self_url.cloned(),
                            from_extra: self_extra.cloned(),
                            from_dev: self_dev.cloned(),
                            to: dependency_name.clone(),
                            to_version: dependency_version.clone(),
                            to_url: to_url.cloned(),
                            to_extra: dependency_extra.clone(),
                            to_dev: dependency_dev.clone(),
                            marker: MarkerTree::TRUE,
                        };
                        edges.insert(edge);
                    }

                    PubGrubPackageInner::Marker {
                        name: ref dependency_name,
                        marker: ref dependency_marker,
                        ..
                    } => {
                        if self_name.is_some_and(|self_name| self_name == dependency_name) {
                            continue;
                        }
                        let to_url = self.fork_urls.get(dependency_name);
                        let edge = ResolutionDependencyEdge {
                            from: self_name.cloned(),
                            from_version: self_version.clone(),
                            from_url: self_url.cloned(),
                            from_extra: self_extra.cloned(),
                            from_dev: self_dev.cloned(),
                            to: dependency_name.clone(),
                            to_version: dependency_version.clone(),
                            to_url: to_url.cloned(),
                            to_extra: None,
                            to_dev: None,
                            marker: dependency_marker.clone(),
                        };
                        edges.insert(edge);
                    }

                    PubGrubPackageInner::Extra {
                        name: ref dependency_name,
                        extra: ref dependency_extra,
                        marker: ref dependency_marker,
                        ..
                    } => {
                        if self_name.is_some_and(|self_name| self_name == dependency_name) {
                            continue;
                        }
                        let to_url = self.fork_urls.get(dependency_name);
                        let edge = ResolutionDependencyEdge {
                            from: self_name.cloned(),
                            from_version: self_version.clone(),
                            from_url: self_url.cloned(),
                            from_extra: self_extra.cloned(),
                            from_dev: self_dev.cloned(),
                            to: dependency_name.clone(),
                            to_version: dependency_version.clone(),
                            to_url: to_url.cloned(),
                            to_extra: Some(dependency_extra.clone()),
                            to_dev: None,
                            marker: MarkerTree::from(dependency_marker.clone()),
                        };
                        edges.insert(edge);
                    }

                    PubGrubPackageInner::Dev {
                        name: ref dependency_name,
                        dev: ref dependency_dev,
                        marker: ref dependency_marker,
                        ..
                    } => {
                        if self_name.is_some_and(|self_name| self_name == dependency_name) {
                            continue;
                        }
                        let to_url = self.fork_urls.get(dependency_name);
                        let edge = ResolutionDependencyEdge {
                            from: self_name.cloned(),
                            from_version: self_version.clone(),
                            from_url: self_url.cloned(),
                            from_extra: self_extra.cloned(),
                            from_dev: self_dev.cloned(),
                            to: dependency_name.clone(),
                            to_version: dependency_version.clone(),
                            to_url: to_url.cloned(),
                            to_extra: None,
                            to_dev: Some(dependency_dev.clone()),
                            marker: MarkerTree::from(dependency_marker.clone()),
                        };
                        edges.insert(edge);
                    }

                    _ => {}
                }
            }
        }

        let nodes = solution
            .into_iter()
            .filter_map(|(package, version)| {
                if let PubGrubPackageInner::Package {
                    name,
                    extra,
                    dev,
                    marker: None,
                } = &*package
                {
                    Some((
                        ResolutionPackage {
                            name: name.clone(),
                            extra: extra.clone(),
                            dev: dev.clone(),
                            url: self.fork_urls.get(name).cloned(),
                        },
                        version,
                    ))
                } else {
                    None
                }
            })
            .collect();

        Resolution {
            nodes,
            edges,
            pins: self.pins,
            markers: self.markers,
        }
    }
}

/// The resolution from a single fork including the virtual packages and the edges between them.
#[derive(Debug)]
pub(crate) struct Resolution {
    pub(crate) nodes: FxHashMap<ResolutionPackage, Version>,
    /// The directed connections between the nodes, where the marker is the node weight. We don't
    /// store the requirement itself, but it can be retrieved from the package metadata.
    pub(crate) edges: FxHashSet<ResolutionDependencyEdge>,
    /// Map each package name, version tuple from `packages` to a distribution.
    pub(crate) pins: FilePins,
    /// The marker setting this resolution was found under.
    pub(crate) markers: ResolverMarkers,
}

/// Package representation we used during resolution where each extra and also the dev-dependencies
/// group are their own package.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResolutionPackage {
    pub(crate) name: PackageName,
    pub(crate) extra: Option<ExtraName>,
    pub(crate) dev: Option<GroupName>,
    /// For index packages, this is `None`.
    pub(crate) url: Option<VerbatimParsedUrl>,
}

/// The `from_` fields and the `to_` fields allow mapping to the originating and target
///  [`ResolutionPackage`] respectively. The `marker` is the edge weight.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResolutionDependencyEdge {
    /// This value is `None` if the dependency comes from the root package.
    pub(crate) from: Option<PackageName>,
    pub(crate) from_version: Version,
    pub(crate) from_url: Option<VerbatimParsedUrl>,
    pub(crate) from_extra: Option<ExtraName>,
    pub(crate) from_dev: Option<GroupName>,
    pub(crate) to: PackageName,
    pub(crate) to_version: Version,
    pub(crate) to_url: Option<VerbatimParsedUrl>,
    pub(crate) to_extra: Option<ExtraName>,
    pub(crate) to_dev: Option<GroupName>,
    pub(crate) marker: MarkerTree,
}

impl ResolutionPackage {
    pub(crate) fn is_base(&self) -> bool {
        self.extra.is_none() && self.dev.is_none()
    }
}

/// Fetch the metadata for an item
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum Request {
    /// A request to fetch the metadata for a package.
    Package(PackageName),
    /// A request to fetch the metadata for a built or source distribution.
    Dist(Dist),
    /// A request to fetch the metadata from an already-installed distribution.
    Installed(InstalledDist),
    /// A request to pre-fetch the metadata for a package and the best-guess distribution.
    Prefetch(PackageName, Range<Version>, PythonRequirement),
}

impl<'a> From<ResolvedDistRef<'a>> for Request {
    fn from(dist: ResolvedDistRef<'a>) -> Request {
        // N.B. This is almost identical to `ResolvedDistRef::to_owned`, but
        // creates a `Request` instead of a `ResolvedDist`. There's probably
        // some room for DRYing this up a bit. The obvious way would be to
        // add a method to create a `Dist`, but a `Dist` cannot be represented
        // as an installed dist.
        match dist {
            ResolvedDistRef::InstallableRegistrySourceDist { sdist, prioritized } => {
                // This is okay because we're only here if the prioritized dist
                // has an sdist, so this always succeeds.
                let source = prioritized.source_dist().expect("a source distribution");
                assert_eq!(
                    (&sdist.name, &sdist.version),
                    (&source.name, &source.version),
                    "expected chosen sdist to match prioritized sdist"
                );
                Request::Dist(Dist::Source(SourceDist::Registry(source)))
            }
            ResolvedDistRef::InstallableRegistryBuiltDist {
                wheel, prioritized, ..
            } => {
                assert_eq!(
                    Some(&wheel.filename),
                    prioritized.best_wheel().map(|(wheel, _)| &wheel.filename),
                    "expected chosen wheel to match best wheel"
                );
                // This is okay because we're only here if the prioritized dist
                // has at least one wheel, so this always succeeds.
                let built = prioritized.built_dist().expect("at least one wheel");
                Request::Dist(Dist::Built(BuiltDist::Registry(built)))
            }
            ResolvedDistRef::Installed(dist) => Request::Installed(dist.clone()),
        }
    }
}

impl Display for Request {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Package(package_name) => {
                write!(f, "Versions {package_name}")
            }
            Self::Dist(dist) => {
                write!(f, "Metadata {dist}")
            }
            Self::Installed(dist) => {
                write!(f, "Installed metadata {dist}")
            }
            Self::Prefetch(package_name, range, _) => {
                write!(f, "Prefetch {package_name} {range}")
            }
        }
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum Response {
    /// The returned metadata for a package hosted on a registry.
    Package(PackageName, VersionsResponse),
    /// The returned metadata for a distribution.
    Dist {
        dist: Dist,
        metadata: MetadataResponse,
    },
    /// The returned metadata for an already-installed distribution.
    Installed {
        dist: InstalledDist,
        metadata: Metadata23,
    },
}

/// Information about the dependencies for a particular package.
///
/// This effectively distills the dependency metadata of a package down into
/// its pubgrub specific constituent parts: each dependency package has a range
/// of possible versions.
enum Dependencies {
    /// Package dependencies are not available.
    Unavailable(UnavailableVersion),
    /// Container for all available package versions.
    ///
    /// Note that in universal mode, it is possible and allowed for multiple
    /// `PubGrubPackage` values in this list to have the same package name.
    /// These conflicts are resolved via `Dependencies::fork`.
    Available(Vec<PubGrubDependency>),
    /// Dependencies that should never result in a fork.
    ///
    /// For example, the dependencies of a `Marker` package will have the
    /// same name and version, but differ according to marker expressions.
    /// But we never want this to result in a fork.
    Unforkable(Vec<PubGrubDependency>),
}

impl Dependencies {
    /// Turn this flat list of dependencies into a potential set of forked
    /// groups of dependencies.
    ///
    /// A fork *only* occurs when there are multiple dependencies with the same
    /// name *and* those dependency specifications have corresponding marker
    /// expressions that are completely disjoint with one another.
    fn fork(self, python_requirement: &PythonRequirement) -> ForkedDependencies {
        let deps = match self {
            Dependencies::Available(deps) => deps,
            Dependencies::Unforkable(deps) => return ForkedDependencies::Unforked(deps),
            Dependencies::Unavailable(err) => return ForkedDependencies::Unavailable(err),
        };
        let mut name_to_deps: BTreeMap<PackageName, Vec<PubGrubDependency>> = BTreeMap::new();
        for dep in deps {
            let name = dep
                .package
                .name()
                .expect("dependency always has a name")
                .clone();
            name_to_deps.entry(name).or_default().push(dep);
        }
        let Forks {
            mut forks,
            diverging_packages,
        } = Forks::new(name_to_deps, python_requirement);
        if forks.is_empty() {
            ForkedDependencies::Unforked(vec![])
        } else if forks.len() == 1 {
            ForkedDependencies::Unforked(forks.pop().unwrap().dependencies)
        } else {
            // Prioritize the forks. Prefer solving forks with lower Python
            // bounds, since they're more likely to produce solutions that work
            // for forks with higher Python bounds (whereas the inverse is not
            // true).
            forks.sort();
            ForkedDependencies::Forked {
                forks,
                diverging_packages: diverging_packages.into_iter().collect(),
            }
        }
    }
}

/// Information about the (possibly forked) dependencies for a particular
/// package.
///
/// This is like `Dependencies` but with an extra variant that only occurs when
/// a `Dependencies` list has multiple dependency specifications with the same
/// name and non-overlapping marker expressions (i.e., a fork occurs).
#[derive(Debug)]
enum ForkedDependencies {
    /// Package dependencies are not available.
    Unavailable(UnavailableVersion),
    /// No forking occurred.
    ///
    /// This is the same as `Dependencies::Available`.
    Unforked(Vec<PubGrubDependency>),
    /// Forked containers for all available package versions.
    ///
    /// Note that there is always at least two forks. If there would
    /// be fewer than 2 forks, then there is no fork at all and the
    /// `Unforked` variant is used instead.
    Forked {
        forks: Vec<Fork>,
        /// The package(s) with different requirements for disjoint markers.
        diverging_packages: Vec<PackageName>,
    },
}

/// A list of forks determined from the dependencies of a single package.
///
/// Any time a marker expression is seen that is not true for all possible
/// marker environments, it is possible for it to introduce a new fork.
#[derive(Debug, Default)]
struct Forks {
    /// The forks discovered among the dependencies.
    forks: Vec<Fork>,
    /// The package(s) that provoked at least one additional fork.
    diverging_packages: BTreeSet<PackageName>,
}

impl Forks {
    fn new(
        name_to_deps: BTreeMap<PackageName, Vec<PubGrubDependency>>,
        python_requirement: &PythonRequirement,
    ) -> Forks {
        let python_marker = python_requirement.to_marker_tree();

        let mut forks = vec![Fork {
            dependencies: vec![],
            markers: MarkerTree::TRUE,
        }];
        let mut diverging_packages = BTreeSet::new();
        for (name, mut deps) in name_to_deps {
            assert!(!deps.is_empty(), "every name has at least one dependency");
            // We never fork if there's only one dependency
            // specification for a given package name. This particular
            // strategy results in a "conservative" approach to forking
            // that gives up correctness in some cases in exchange for
            // more limited forking. More limited forking results in
            // simpler-and-easier-to-understand lock files and faster
            // resolving. The correctness we give up manifests when
            // two transitive non-sibling dependencies conflict. In
            // that case, we don't detect the fork ahead of time (at
            // present).
            if deps.len() == 1 {
                let dep = deps.pop().unwrap();
                let markers = dep.package.marker().cloned().unwrap_or(MarkerTree::TRUE);
                for fork in &mut forks {
                    if !fork.markers.is_disjoint(&markers) {
                        fork.dependencies.push(dep.clone());
                    }
                }
                continue;
            }
            for dep in deps {
                let mut markers = dep.package.marker().cloned().unwrap_or(MarkerTree::TRUE);
                if markers.is_false() {
                    // If the markers can never be satisfied, then we
                    // can drop this dependency unceremoniously.
                    continue;
                }
                if markers.is_true() {
                    // Or, if the markers are always true, then we just
                    // add the dependency to every fork unconditionally.
                    for fork in &mut forks {
                        if !fork.markers.is_disjoint(&markers) {
                            fork.dependencies.push(dep.clone());
                        }
                    }
                    continue;
                }
                // Otherwise, we *should* need to add a new fork...
                diverging_packages.insert(name.clone());

                let mut new = vec![];
                for mut fork in std::mem::take(&mut forks) {
                    if fork.markers.is_disjoint(&markers) {
                        new.push(fork);
                        continue;
                    }

                    let not_markers = markers.negate();
                    let mut new_markers = markers.clone();
                    new_markers.and(fork.markers.negate());
                    if !fork.markers.is_disjoint(&not_markers) {
                        let mut new_fork = fork.clone();
                        new_fork.intersect(not_markers);
                        // Filter out any forks we created that are disjoint with our
                        // Python requirement.
                        if !new_fork.markers.is_disjoint(&python_marker) {
                            new.push(new_fork);
                        }
                    }
                    fork.dependencies.push(dep.clone());
                    fork.intersect(markers);
                    // Filter out any forks we created that are disjoint with our
                    // Python requirement.
                    if !fork.markers.is_disjoint(&python_marker) {
                        new.push(fork);
                    }
                    markers = new_markers;
                }
                forks = new;
            }
        }
        Forks {
            forks,
            diverging_packages,
        }
    }
}

/// A single fork in a list of dependencies.
///
/// A fork corresponds to the full list of dependencies for a package,
/// but with any conflicting dependency specifications omitted. For
/// example, if we have `a<2 ; sys_platform == 'foo'` and `a>=2 ;
/// sys_platform == 'bar'`, then because the dependency specifications
/// have the same name and because the marker expressions are disjoint,
/// a fork occurs. One fork will contain `a<2` but not `a>=2`, while
/// the other fork will contain `a>=2` but not `a<2`.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Fork {
    /// The list of dependencies for this fork, guaranteed to be conflict
    /// free. (i.e., There are no two packages with the same name with
    /// non-overlapping marker expressions.)
    ///
    /// Note that callers shouldn't mutate this sequence directly. Instead,
    /// they should use `add_forked_package` or `add_nonfork_package`. Namely,
    /// it should be impossible for a package with a marker expression that is
    /// disjoint from the marker expression on this fork to be added.
    dependencies: Vec<PubGrubDependency>,
    /// The markers that provoked this fork.
    ///
    /// So in the example above, the `a<2` fork would have
    /// `sys_platform == 'foo'`, while the `a>=2` fork would have
    /// `sys_platform == 'bar'`.
    ///
    /// (This doesn't include any marker expressions from a parent fork.)
    markers: MarkerTree,
}

impl Fork {
    fn intersect(&mut self, markers: MarkerTree) {
        self.markers.and(markers);
        self.dependencies.retain(|dep| {
            let Some(markers) = dep.package.marker() else {
                return true;
            };
            !self.markers.is_disjoint(markers)
        });
    }
}

impl Ord for Fork {
    fn cmp(&self, other: &Self) -> Ordering {
        // A higher `requires-python` requirement indicates a _lower-priority_ fork. We'd prefer
        // to solve `<3.7` before solving `>=3.7`, since the resolution produced by the former might
        // work for the latter, but the inverse is unlikely to be true.
        let self_bound = marker::requires_python(&self.markers).unwrap_or_default();
        let other_bound = marker::requires_python(&other.markers).unwrap_or_default();

        other_bound.lower().cmp(self_bound.lower()).then_with(|| {
            // If there's no difference, prioritize forks with upper bounds. We'd prefer to solve
            // `numpy <= 2` before solving `numpy >= 1`, since the resolution produced by the former
            // might work for the latter, but the inverse is unlikely to be true due to maximum
            // version selection. (Selecting `numpy==2.0.0` would satisfy both forks, but selecting
            // the latest `numpy` would not.)
            let self_upper_bounds = self
                .dependencies
                .iter()
                .filter(|dep| {
                    dep.version
                        .bounding_range()
                        .is_some_and(|(_, upper)| !matches!(upper, Bound::Unbounded))
                })
                .count();
            let other_upper_bounds = other
                .dependencies
                .iter()
                .filter(|dep| {
                    dep.version
                        .bounding_range()
                        .is_some_and(|(_, upper)| !matches!(upper, Bound::Unbounded))
                })
                .count();

            self_upper_bounds.cmp(&other_upper_bounds)
        })
    }
}

impl PartialOrd for Fork {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
