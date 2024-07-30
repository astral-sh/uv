//! Given a set of requirements, find a set of compatible packages.

use std::borrow::Cow;
use std::collections::hash_map::Entry;
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
use pubgrub::range::Range;
use pubgrub::solver::{Incompatibility, State};
use rustc_hash::{FxHashMap, FxHashSet};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::sync::oneshot;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, info, instrument, trace, warn, Level};

use distribution_types::{
    BuiltDist, CompatibleDist, Dist, DistributionMetadata, IncompatibleDist, IncompatibleSource,
    IncompatibleWheel, IndexLocations, InstalledDist, PythonRequirementKind, RemoteSource,
    ResolvedDist, ResolvedDistRef, SourceDist, VersionOrUrlRef,
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
use crate::marker::{normalize, requires_python_marker};
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
pub(crate) use crate::resolver::index::FxOnceMap;
pub use crate::resolver::index::InMemoryIndex;
pub use crate::resolver::provider::{
    DefaultResolverProvider, MetadataResponse, PackageVersionsResult, ResolverProvider,
    VersionsResponse, WheelMetadataResult,
};
use crate::resolver::reporter::Facade;
pub use crate::resolver::reporter::{BuildId, Reporter};
use crate::yanks::AllowedYanks;
use crate::{DependencyMode, Exclusions, FlatIndex, Options};

mod availability;
mod batch_prefetch;
mod fork_map;
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
    dev: Vec<GroupName>,
    preferences: Preferences,
    git: GitResolver,
    exclusions: Exclusions,
    urls: Urls,
    locals: Locals,
    dependency_mode: DependencyMode,
    hasher: HashStrategy,
    markers: ResolverMarkers,
    python_requirement: PythonRequirement,
    /// This is derived from `PythonRequirement` once at initialization
    /// time. It's used in universal mode to filter our dependencies with
    /// a `python_version` marker expression that has no overlap with the
    /// `Requires-Python` specifier.
    ///
    /// This is non-None if and only if the resolver is operating in
    /// universal mode. (i.e., when `markers` is `None`.)
    requires_python: Option<MarkerTree>,
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
            python_requirement
                .target()
                .and_then(|target| target.as_requires_python()),
            AllowedYanks::from_manifest(
                &manifest,
                markers.marker_environment(),
                options.dependency_mode,
            ),
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
        provider: Provider,
        installed_packages: InstalledPackages,
    ) -> Result<Self, ResolveError> {
        let state = ResolverState {
            index: index.clone(),
            git: git.clone(),
            selector: CandidateSelector::for_resolution(
                options,
                &manifest,
                markers.marker_environment(),
            ),
            dependency_mode: options.dependency_mode,
            urls: Urls::from_manifest(
                &manifest,
                markers.marker_environment(),
                git,
                options.dependency_mode,
            )?,
            locals: Locals::from_manifest(
                &manifest,
                markers.marker_environment(),
                options.dependency_mode,
            ),
            project: manifest.project,
            requirements: manifest.requirements,
            constraints: manifest.constraints,
            overrides: manifest.overrides,
            dev: manifest.dev,
            preferences: manifest.preferences,
            exclusions: manifest.exclusions,
            hasher: hasher.clone(),
            requires_python: if markers.marker_environment().is_some() {
                None
            } else {
                python_requirement.to_marker_tree()
            },
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
                tx.send(result).unwrap();
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
            self.python_requirement.installed()
        );
        if let Some(target) = self.python_requirement.target() {
            debug!("Solving with target Python version: {}", target);
        }

        let mut visited = FxHashSet::default();

        let root = PubGrubPackage::from(PubGrubPackageInner::Root(self.project.clone()));
        let mut prefetcher = BatchPrefetcher::default();
        let state = ForkState::new(
            State::init(root.clone(), MIN_VERSION.clone()),
            root,
            self.markers.clone(),
            self.python_requirement.clone(),
            self.requires_python.clone(),
        );
        let mut preferences = self.preferences.clone();
        let mut forked_states = vec![state];
        let mut resolutions = vec![];

        'FORK: while let Some(mut state) = forked_states.pop() {
            if let ResolverMarkers::Fork(markers) = &state.markers {
                if let Some(requires_python) = state.requires_python.as_ref() {
                    debug!(
                        "Solving split {} (requires-python: {})",
                        markers, requires_python
                    );
                } else {
                    debug!("Solving split {}", markers);
                }
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

                // Pre-visit all candidate packages, to allow metadata to be fetched in parallel. If
                // the dependency mode is direct, we only need to visit the root package.
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

                    // Walk over the selected versions, and mark them as preferences.
                    for (package, version) in &resolution.nodes {
                        if let Entry::Vacant(entry) = preferences.entry(package.name.clone()) {
                            entry.insert(version.clone().into());
                        }
                    }

                    // If another fork had the same resolution, merge into that fork instead.
                    if let Some(existing_resolution) = resolutions
                        .iter_mut()
                        .find(|existing_resolution| resolution.same_graph(existing_resolution))
                    {
                        let ResolverMarkers::Fork(existing_markers) = &existing_resolution.markers
                        else {
                            panic!("A non-forking resolution exists in forking mode")
                        };
                        let mut new_markers = existing_markers.clone();
                        new_markers.or(resolution
                            .markers
                            .fork_markers()
                            .expect("A non-forking resolution exists in forking mode")
                            .clone());
                        existing_resolution.markers = normalize(new_markers, None)
                            .map(ResolverMarkers::Fork)
                            .unwrap_or(ResolverMarkers::universal(None));
                        continue 'FORK;
                    }

                    Self::trace_resolution(&resolution);
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

                        // Check if the decision was due to the package being unavailable
                        if let PubGrubPackageInner::Package { ref name, .. } = &*state.next {
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
                    state.requires_python.as_ref(),
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
                    "Distinct solution for ({markers}) with {} packages",
                    resolution.nodes.len()
                );
            }
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
            self.options,
        )
    }

    /// When trace level logging is enabled, we dump the final
    /// unioned resolution, including markers, to help with
    /// debugging. Namely, this tells use precisely the state
    /// emitted by the resolver before going off to construct a
    /// resolution graph.
    fn trace_resolution(combined: &Resolution) {
        if !tracing::enabled!(Level::TRACE) {
            return;
        }
        for edge in &combined.edges {
            trace!(
                "Resolution: {} -> {}",
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
            if let Some(ref marker) = edge.marker {
                write!(msg, " ; {marker}").unwrap();
            }
            trace!("Resolution:     {msg}");
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
        forks.into_iter().enumerate().map(move |(i, fork)| {
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
            if let Some(target) = python_requirement.target() {
                if !target.is_compatible_with(requires_python) {
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
            if !requires_python.contains(python_requirement.installed()) {
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
                        if let Some(target) = python_requirement.target() {
                            if !target.is_compatible_with(requires_python) {
                                return Some(IncompatibleDist::Source(
                                    IncompatibleSource::RequiresPython(
                                        requires_python.clone(),
                                        PythonRequirementKind::Target,
                                    ),
                                ));
                            }
                        }
                        if !requires_python.contains(python_requirement.installed()) {
                            return Some(IncompatibleDist::Source(
                                IncompatibleSource::RequiresPython(
                                    requires_python.clone(),
                                    PythonRequirementKind::Installed,
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
                        if let Some(target) = python_requirement.target() {
                            if !target.is_compatible_with(requires_python) {
                                return Some(IncompatibleDist::Wheel(
                                    IncompatibleWheel::RequiresPython(
                                        requires_python.clone(),
                                        PythonRequirementKind::Target,
                                    ),
                                ));
                            }
                        } else {
                            if !requires_python.contains(python_requirement.installed()) {
                                return Some(IncompatibleDist::Wheel(
                                    IncompatibleWheel::RequiresPython(
                                        requires_python.clone(),
                                        PythonRequirementKind::Installed,
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
        requires_python: Option<&MarkerTree>,
    ) -> Result<ForkedDependencies, ResolveError> {
        let result = self.get_dependencies(package, version, fork_urls, markers, requires_python);
        match markers {
            ResolverMarkers::SpecificEnvironment(_) => result.map(|deps| match deps {
                Dependencies::Available(deps) => ForkedDependencies::Unforked(deps),
                Dependencies::Unavailable(err) => ForkedDependencies::Unavailable(err),
            }),
            ResolverMarkers::Universal { .. } | ResolverMarkers::Fork(_) => Ok(result?.fork()),
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
        requires_python: Option<&MarkerTree>,
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
                    requires_python,
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

                // If we're excluding transitive dependencies, short-circuit. (It's important that
                // we fetched the metadata, though, since we need it to validate extras.)
                if self.dependency_mode.is_direct() {
                    return Ok(Dependencies::Available(Vec::default()));
                }

                let requirements = self.flatten_requirements(
                    &metadata.requires_dist,
                    &metadata.dev_dependencies,
                    extra.as_ref(),
                    dev.as_ref(),
                    Some(name),
                    markers,
                    requires_python,
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
                    for group in &self.dev {
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
            PubGrubPackageInner::Python(_) => return Ok(Dependencies::Available(Vec::default())),

            // Add a dependency on both the marker and base package.
            PubGrubPackageInner::Marker { name, marker } => {
                return Ok(Dependencies::Available(
                    [None, Some(marker)]
                        .into_iter()
                        .map(move |marker| PubGrubDependency {
                            package: PubGrubPackage::from(PubGrubPackageInner::Package {
                                name: name.clone(),
                                extra: None,
                                dev: None,
                                marker: marker.cloned(),
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
                return Ok(Dependencies::Available(
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
                return Ok(Dependencies::Available(
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
        requires_python: Option<&'a MarkerTree>,
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
                requires_python,
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
                self.requirements_for_extra(dependencies, Some(&extra), markers, requires_python)
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
        requires_python: Option<&'parameters MarkerTree>,
    ) -> impl Iterator<Item = Cow<'data, Requirement>> + 'parameters
    where
        'data: 'parameters,
    {
        self.overrides
            .apply(dependencies)
            .filter(move |requirement| {
                // If the requirement would not be selected with any Python version
                // supported by the root, skip it.
                if !satisfies_requires_python(requires_python, requirement) {
                    trace!(
                        "skipping {requirement} because of Requires-Python {requires_python}",
                        // OK because this filter only applies when there is a present
                        // Requires-Python specifier.
                        requires_python = requires_python.unwrap()
                    );
                    return false;
                }

                // If we're in a fork in universal mode, ignore any dependency that isn't part of
                // this fork (but will be part of another fork).
                if let ResolverMarkers::Fork(markers) = markers {
                    if !possible_to_satisfy_markers(markers, requirement) {
                        trace!("skipping {requirement} because of context resolver markers {markers}");
                        return false;
                    }
                }

                // If the requirement isn't relevant for the current platform, skip it.
                match extra {
                    Some(source_extra) => {
                        // Only include requirements that are relevant for the current extra.
                        if requirement.evaluate_markers(markers.marker_environment(), &[]) {
                            return false;
                        }
                        if !requirement.evaluate_markers(
                            markers.marker_environment(),
                            std::slice::from_ref(source_extra),
                        ) {
                            return false;
                        }
                    }
                    None => {
                        if !requirement.evaluate_markers(markers.marker_environment(), &[]) {
                            return false;
                        }
                    }
                }

                true
            })
            .flat_map(move |requirement| {
                iter::once(requirement.clone()).chain(
                    self.constraints
                        .get(&requirement.name)
                        .into_iter()
                        .flatten()
                        .filter(move |constraint| {
                            if !satisfies_requires_python(requires_python, constraint) {
                                trace!(
                                    "skipping {constraint} because of Requires-Python {requires_python}",
                                    requires_python = requires_python.unwrap()
                                );
                                return false;
                            }

                            // If we're in a fork in universal mode, ignore any dependency that isn't part of
                            // this fork (but will be part of another fork).
                            if let ResolverMarkers::Fork(markers) = markers {
                                if !possible_to_satisfy_markers(markers, constraint) {
                                    trace!("skipping {constraint} because of context resolver markers {markers}");
                                    return false;
                                }
                            }

                            // If the constraint isn't relevant for the current platform, skip it.
                            match extra {
                                Some(source_extra) => {
                                    if !constraint.evaluate_markers(
                                        markers.marker_environment(),
                                        std::slice::from_ref(source_extra),
                                    ) {
                                        return false;
                                    }
                                }
                                None => {
                                    if !constraint.evaluate_markers(markers.marker_environment(), &[]) {
                                        return false;
                                    }
                                }
                            }

                            true
                        })
                        .map(move |constraint| {
                            // If the requirement is `requests ; sys_platform == 'darwin'` and the
                            // constraint is `requests ; python_version == '3.6'`, the constraint
                            // should only apply when _both_ markers are true.
                            if let Some(marker) = requirement.marker.as_ref() {
                                let marker = constraint.marker.as_ref().map(|m| {
                                    MarkerTree::And(vec![marker.clone(), m.clone()])
                                }).or_else(|| Some(marker.clone()));

                                Cow::Owned(Requirement {
                                    name: constraint.name.clone(),
                                    extras: constraint.extras.clone(),
                                    source: constraint.source.clone(),
                                    origin: constraint.origin.clone(),
                                    marker
                                })
                            } else {
                                Cow::Borrowed(constraint)
                            }
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
                            ResolveError::FetchAndBuild(Box::new(source_dist), err)
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
                    &ResolverMarkers::universal(None),
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
                if !dist.prefetchable() {
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
                            if let Some(target) = python_requirement.target() {
                                if !target.is_compatible_with(requires_python) {
                                    return Ok(None);
                                }
                            }
                            if !requires_python.contains(python_requirement.installed()) {
                                return Ok(None);
                            }
                        }
                    }
                    CompatibleDist::CompatibleWheel { wheel, .. } => {
                        // Wheels must meet the _target_ Python version.
                        if let Some(requires_python) = wheel.file.requires_python.as_ref() {
                            if let Some(target) = python_requirement.target() {
                                if !target.is_compatible_with(requires_python) {
                                    return Ok(None);
                                }
                            } else {
                                if !requires_python.contains(python_requirement.installed()) {
                                    return Ok(None);
                                }
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
                                        ResolveError::FetchAndBuild(Box::new(source_dist), err)
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
        mut err: pubgrub::error::NoSolutionError<UvDependencyProvider>,
        fork_urls: ForkUrls,
        markers: ResolverMarkers,
        visited: &FxHashSet<PackageName>,
        index_locations: &IndexLocations,
    ) -> ResolveError {
        NoSolutionError::collapse_proxies(&mut err);

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
            let PubGrubPackageInner::Package { name, .. } = &**package else {
                continue;
            };
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
                            .entry(package.clone())
                            .or_insert_with(BTreeSet::new)
                            .extend(version_map.iter().map(|(version, _)| version.clone()));
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
    /// The [`MarkerTree`] corresponding to the [`PythonRequirement`].
    requires_python: Option<MarkerTree>,
}

impl ForkState {
    fn new(
        pubgrub: State<UvDependencyProvider>,
        root: PubGrubPackage,
        markers: ResolverMarkers,
        python_requirement: PythonRequirement,
        requires_python: Option<MarkerTree>,
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
            requires_python,
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

                    // Prioritize local versions over the original version range.
                    if !locals.is_empty() {
                        *version = Range::empty();
                    }

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
                    warn_user_once!("The direct dependency `{package}` is unpinned. Consider setting a lower bound when using `--resolution-strategy lowest` to avoid using outdated versions.");
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
        let combined_markers =
            normalize(combined_markers, None).unwrap_or_else(|| MarkerTree::And(vec![]));

        // If the fork contains a narrowed Python requirement, apply it.
        let python_requirement = requires_python_marker(&combined_markers)
            .and_then(|marker| self.python_requirement.narrow(&marker));
        if let Some(python_requirement) = python_requirement {
            if let Some(target) = python_requirement.target() {
                debug!("Narrowed `requires-python` bound to: {target}");
            }
            self.requires_python = if self.requires_python.is_some() {
                python_requirement.to_marker_tree()
            } else {
                None
            };
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
                let pubgrub::solver::Kind::FromDependencyOf(
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
                            // This propagates markers from the fork to
                            // packages without any markers. These might wind
                            // up be duplicative (and are even further merged
                            // via disjunction when a ResolutionGraph is
                            // constructed), but normalization should simplify
                            // most such cases.
                            //
                            // In a previous implementation of marker
                            // propagation, markers were propagated at the
                            // time a fork was created. But this was crucially
                            // missing a key detail: the specific version of
                            // a package outside of a fork can be determined
                            // by the forks of its dependencies, even when
                            // that package is not part of a fork at the time
                            // the forks were created. In that case, it was
                            // possible for two versions of the same package
                            // to be unconditionally included in a resolution,
                            // which must never be.
                            //
                            // See https://github.com/astral-sh/uv/pull/5583
                            // for an example of where this occurs with
                            // `Sphinx`.
                            //
                            // Here, instead, we do the marker propagation
                            // after resolution has completed. This relies
                            // on the fact that the markers aren't otherwise
                            // needed during resolution (which I believe is
                            // true), but is a more robust approach that should
                            // capture all cases.
                            marker: self.markers.fork_markers().cloned(),
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
                            marker: Some(dependency_marker.clone()),
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
                            marker: dependency_marker.clone(),
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
                            marker: dependency_marker.clone(),
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
    pub(crate) marker: Option<MarkerTree>,
}

impl Resolution {
    /// Whether we got two identical resolutions in two separate forks.
    ///
    /// Ignores pins since the which distribution we prioritized for each version doesn't matter.
    fn same_graph(&self, other: &Self) -> bool {
        // TODO(konsti): The edges being equal is not a requirement for the graph being equal. While
        // an exact solution is too much here, we should ignore different in edges that point to
        // nodes that are always installed. Example: root requires foo, root requires bar. bar
        // forks, and one for the branches has bar -> foo while the other doesn't. The resolution
        // is still the same graph since the presence or absence of the bar -> foo edge cannot
        // change which packages and versions are installed.
        self.nodes == other.nodes && self.edges == other.edges
    }
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
}

impl Dependencies {
    /// Turn this flat list of dependencies into a potential set of forked
    /// groups of dependencies.
    ///
    /// A fork *only* occurs when there are multiple dependencies with the same
    /// name *and* those dependency specifications have corresponding marker
    /// expressions that are completely disjoint with one another.
    fn fork(self) -> ForkedDependencies {
        use std::collections::hash_map::Entry;

        let deps = match self {
            Dependencies::Available(deps) => deps,
            Dependencies::Unavailable(err) => return ForkedDependencies::Unavailable(err),
        };

        let mut by_name: FxHashMap<&PackageName, PossibleForks> = FxHashMap::default();
        for (index, dependency) in deps.iter().enumerate() {
            // A root can never be a dependency of another package,
            // and a `Python` pubgrub package is never returned by
            // `get_dependencies`. So a pubgrub package always has a
            // name in this context.
            let name = dependency
                .package
                .name()
                .expect("dependency always has a name");
            let marker = dependency.package.marker();
            let Some(marker) = marker else {
                // When no marker is found, it implies there is a dependency on
                // this package that is unconditional with respect to marker
                // expressions. Therefore, it should never be the cause of a
                // fork since it is necessarily overlapping with every other
                // possible marker expression that isn't pathological.
                match by_name.entry(name) {
                    Entry::Vacant(e) => {
                        e.insert(PossibleForks::NoForkPossible(vec![index]));
                    }
                    Entry::Occupied(mut e) => {
                        e.get_mut().push_unconditional_package(index);
                    }
                }
                continue;
            };
            let possible_forks = match by_name.entry(name) {
                // If one doesn't exist, then this is the first dependency
                // with this package name. And since it has a marker, we can
                // add it as the initial instance of a possibly forking set of
                // dependencies. (A fork will only actually happen if another
                // dependency is found with the same package name *and* where
                // its marker expression is disjoint with this one.)
                Entry::Vacant(e) => {
                    let possible_fork = PossibleFork {
                        packages: vec![(index, marker)],
                    };
                    let fork_groups = PossibleForkGroups {
                        forks: vec![possible_fork],
                    };
                    e.insert(PossibleForks::PossiblyForking(fork_groups));
                    continue;
                }
                // Now that we have a marker, look for an existing entry. If
                // one already exists and is "no fork possible," then we know
                // we can't fork.
                Entry::Occupied(e) => match *e.into_mut() {
                    PossibleForks::NoForkPossible(ref mut indices) => {
                        indices.push(index);
                        continue;
                    }
                    PossibleForks::PossiblyForking(ref mut possible_forks) => possible_forks,
                },
            };
            // At this point, we know we 1) have a duplicate dependency on
            // a package and 2) the original and this one both have marker
            // expressions. This still doesn't guarantee that a fork occurs
            // though. A fork can only occur when the marker expressions from
            // (2) are provably disjoint. Otherwise, we could end up with
            // a resolution that would result in installing two different
            // versions of the same package. Specifically, this could occur in
            // precisely the cases where the marker expressions intersect.
            //
            // By construction, the marker expressions *in* each fork group
            // have some non-empty intersection, and the marker expressions
            // *between* each fork group are completely disjoint. So what we do
            // is look for a group in which there is some overlap. If so, this
            // package gets added to that fork group. Otherwise, we create a
            // new fork group.
            let Some(possible_fork) = possible_forks.find_overlapping_fork_group(marker) else {
                // Create a new fork since there was no overlap.
                possible_forks.forks.push(PossibleFork {
                    packages: vec![(index, marker)],
                });
                continue;
            };
            // Add to an existing fork since there was overlap.
            possible_fork.packages.push((index, marker));
        }
        // If all possible forks have exactly 1 group, then there is no forking.
        if !by_name.values().any(PossibleForks::has_fork) {
            return ForkedDependencies::Unforked(deps);
        }
        let mut forks = vec![Fork {
            dependencies: vec![],
            markers: MarkerTree::And(vec![]),
        }];
        let mut diverging_packages = Vec::new();
        for (name, possible_forks) in by_name {
            let fork_groups = match possible_forks.finish() {
                // 'finish()' guarantees that 'PossiblyForking' implies
                // 'DefinitelyForking'.
                PossibleForks::PossiblyForking(fork_groups) => fork_groups,
                PossibleForks::NoForkPossible(indices) => {
                    // No fork is provoked by this package, so just add
                    // everything in this group to each of the forks.
                    for index in indices {
                        for fork in &mut forks {
                            fork.add_nonfork_package(deps[index].clone());
                        }
                    }
                    continue;
                }
            };
            assert!(fork_groups.forks.len() >= 2, "expected definitive fork");
            let mut new_forks: Vec<Fork> = vec![];
            if let Some(markers) = fork_groups.remaining_universe() {
                trace!("Adding split to cover possibly incomplete markers: {markers}");
                let mut new_forks_for_remaining_universe = forks.clone();
                for fork in &mut new_forks_for_remaining_universe {
                    fork.markers.and(markers.clone());
                    fork.remove_disjoint_packages();
                }
                new_forks.extend(new_forks_for_remaining_universe);
            }
            // Each group has a list of packages whose marker expressions are
            // guaranteed to be overlapping. So we must union those marker
            // expressions and then intersect them with each existing fork.
            for group in fork_groups.forks {
                let mut new_forks_for_group = forks.clone();
                for fork in &mut new_forks_for_group {
                    fork.markers.and(group.union());
                    fork.remove_disjoint_packages();
                    for &(index, _) in &group.packages {
                        fork.dependencies.push(deps[index].clone());
                    }
                }
                new_forks.extend(new_forks_for_group);
            }
            forks = new_forks;
            diverging_packages.push(name.clone());
        }
        ForkedDependencies::Forked {
            forks,
            diverging_packages,
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

/// A single fork in a list of dependencies.
///
/// A fork corresponds to the full list of dependencies for a package,
/// but with any conflicting dependency specifications omitted. For
/// example, if we have `a<2 ; sys_platform == 'foo'` and `a>=2 ;
/// sys_platform == 'bar'`, then because the dependency specifications
/// have the same name and because the marker expressions are disjoint,
/// a fork occurs. One fork will contain `a<2` but not `a>=2`, while
/// the other fork will contain `a>=2` but not `a<2`.
#[derive(Clone, Debug)]
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
    /// Add the given dependency to this fork.
    ///
    /// This works by assuming the given package did *not* provoke a fork.
    ///
    /// It is only added if the markers on the given package are not disjoint
    /// with this fork's markers.
    fn add_nonfork_package(&mut self, dependency: PubGrubDependency) {
        use crate::marker::is_disjoint;

        if dependency
            .package
            .marker()
            .map_or(true, |marker| !is_disjoint(marker, &self.markers))
        {
            self.dependencies.push(dependency);
        }
    }

    /// Removes any dependencies in this fork whose markers are disjoint with
    /// its own markers.
    fn remove_disjoint_packages(&mut self) {
        use crate::marker::is_disjoint;

        self.dependencies.retain(|dependency| {
            dependency
                .package
                .marker()
                .map_or(true, |pkg_marker| !is_disjoint(pkg_marker, &self.markers))
        });
    }
}

/// Intermediate state that represents a *possible* grouping of forks
/// for one package name.
///
/// This accumulates state while examining a `Dependencies` list. In
/// particular, it accumulates conflicting dependency specifications and marker
/// expressions. As soon as a fork can be ruled out, this state is switched to
/// `NoForkPossible`. If, at the end of visiting all `Dependencies`, we still
/// have `PossibleForks::PossiblyForking`, then a fork exists if and only if
/// one of its groups has length bigger than `1`.
///
/// One common way for a fork to be known to be impossible is if there exists
/// conflicting dependency specifications where at least one is unconditional.
/// For example, `a<2` and `a>=2 ; sys_platform == 'foo'`. In this case, `a<2`
/// has a marker expression that is always true and thus never disjoint with
/// any other marker expression. Therefore, there can be no fork for `a`.
///
/// Note that we use indices into a `Dependencies` list to represent packages.
/// This avoids excessive cloning.
#[derive(Debug)]
enum PossibleForks<'a> {
    /// A group of dependencies (all with the same package name) where it is
    /// known that no forks exist.
    NoForkPossible(Vec<usize>),
    /// A group of groups dependencies (all with the same package name) where
    /// it is possible for each group to correspond to a fork.
    PossiblyForking(PossibleForkGroups<'a>),
}

impl<'a> PossibleForks<'a> {
    /// Returns true if and only if this contains a fork assuming there are
    /// no other dependencies to be considered.
    fn has_fork(&self) -> bool {
        let PossibleForks::PossiblyForking(ref fork_groups) = *self else {
            return false;
        };
        fork_groups.forks.len() > 1
    }

    /// Consumes this possible set of forks and converts a "possibly forking"
    /// variant to a "no fork possible" variant if there are no actual forks.
    ///
    /// This should be called when all dependencies for one package have been
    /// considered. It will normalize this value such that `PossiblyForking`
    /// means `DefinitelyForking`.
    fn finish(mut self) -> PossibleForks<'a> {
        let PossibleForks::PossiblyForking(ref fork_groups) = self else {
            return self;
        };
        if fork_groups.forks.len() == 1 {
            self.make_no_forks_possible();
            return self;
        }
        self
    }

    /// Pushes an unconditional index to a package.
    ///
    /// If this previously contained possible forks, those are combined into
    /// one single set of dependencies that can never be forked.
    ///
    /// That is, adding an unconditional package means it is not disjoint with
    /// all other possible dependencies using the same package name.
    fn push_unconditional_package(&mut self, index: usize) {
        self.make_no_forks_possible();
        let PossibleForks::NoForkPossible(ref mut indices) = *self else {
            unreachable!("all forks should be eliminated")
        };
        indices.push(index);
    }

    /// Convert this set of possible forks into something that can never fork.
    ///
    /// This is useful in cases where a dependency on a package is found
    /// without any marker expressions at all. In this case, it is never
    /// possible for this package to provoke a fork. Since it is unconditional,
    /// it implies it is never disjoint with any other dependency specification
    /// on the same package. (Except for pathological cases of marker
    /// expressions that always evaluate to false. But we generally ignore
    /// those.)
    fn make_no_forks_possible(&mut self) {
        let PossibleForks::PossiblyForking(ref fork_groups) = *self else {
            return;
        };
        let mut indices = vec![];
        for possible_fork in &fork_groups.forks {
            for &(index, _) in &possible_fork.packages {
                indices.push(index);
            }
        }
        *self = PossibleForks::NoForkPossible(indices);
    }
}

/// A list of groups of dependencies (all with the same package name), where
/// each group may correspond to a fork.
#[derive(Debug)]
struct PossibleForkGroups<'a> {
    /// The list of forks.
    forks: Vec<PossibleFork<'a>>,
}

impl<'a> PossibleForkGroups<'a> {
    /// Given a marker expression, if there is a fork in this set of fork
    /// groups with non-empty overlap with it, then that fork group is
    /// returned. Otherwise, `None` is returned.
    fn find_overlapping_fork_group<'g>(
        &'g mut self,
        marker: &MarkerTree,
    ) -> Option<&'g mut PossibleFork<'a>> {
        self.forks
            .iter_mut()
            .find(|fork| fork.is_overlapping(marker))
    }

    /// Returns a marker tree corresponding to the set of marker expressions
    /// outside of this fork group.
    ///
    /// In many cases, it can be easily known that the set of marker
    /// expressions referred to by this marker tree is empty. In this case,
    /// `None` is returned. But note that if a marker tree is returned, it is
    /// still possible for it to describe exactly zero marker environments.
    fn remaining_universe(&self) -> Option<MarkerTree> {
        let have = MarkerTree::Or(self.forks.iter().map(PossibleFork::union).collect());
        let missing = have.negate();
        if crate::marker::is_definitively_empty_set(&missing) {
            return None;
        }
        Some(missing)
    }
}

/// Intermediate state representing a single possible fork.
///
/// The key invariant here is that, beyond a singleton fork, for all packages
/// in this fork, its marker expression must be overlapping with at least one
/// other package's marker expression. That is, when considering whether a
/// dependency specification with a conflicting package name provokes a fork
/// or not, one must look at the existing possible groups of forks. If any of
/// those groups have a package with an overlapping marker expression, then
/// the conflicting package name cannot possibly introduce a new fork. But if
/// there is no existing fork with an overlapping marker expression, then the
/// conflict provokes a new fork.
///
/// As with other intermediate data types, we use indices into a list of
/// `Dependencies` to represent packages to avoid excessive cloning.
#[derive(Debug)]
struct PossibleFork<'a> {
    packages: Vec<(usize, &'a MarkerTree)>,
}

impl<'a> PossibleFork<'a> {
    /// Returns true if and only if the given marker expression has a non-empty
    /// intersection with *any* of the package markers within this possible
    /// fork.
    fn is_overlapping(&self, candidate_package_markers: &MarkerTree) -> bool {
        use crate::marker::is_disjoint;

        for (_, package_markers) in &self.packages {
            if !is_disjoint(candidate_package_markers, package_markers) {
                return true;
            }
        }
        false
    }

    /// Returns the union of all the marker expressions in this possible fork.
    ///
    /// Each marker expression in the union returned is guaranteed to be overlapping
    /// with at least one other expression in the same union.
    fn union(&self) -> MarkerTree {
        let mut trees: Vec<MarkerTree> = self
            .packages
            .iter()
            .map(|&(_, tree)| (*tree).clone())
            .collect();
        if trees.len() == 1 {
            trees.pop().unwrap()
        } else {
            MarkerTree::Or(trees)
        }
    }
}

/// Returns true if and only if the given requirement's marker expression has a
/// possible true value given the `requires_python` specifier given.
///
/// While this is always called, a `requires_python` is only non-None when in
/// universal resolution mode. In non-universal mode, `requires_python` is
/// `None` and this always returns `true`.
fn satisfies_requires_python(
    requires_python: Option<&MarkerTree>,
    requirement: &Requirement,
) -> bool {
    let Some(requires_python) = requires_python else {
        return true;
    };
    possible_to_satisfy_markers(requires_python, requirement)
}

/// Returns true if and only if the given requirement's marker expression has a
/// possible true value given the `markers` expression given.
fn possible_to_satisfy_markers(markers: &MarkerTree, requirement: &Requirement) -> bool {
    let Some(marker) = requirement.marker.as_ref() else {
        return true;
    };
    !crate::marker::is_disjoint(markers, marker)
}
