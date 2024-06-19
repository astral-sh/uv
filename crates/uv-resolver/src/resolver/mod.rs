//! Given a set of requirements, find a set of compatible packages.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::ops::Bound;
use std::sync::Arc;
use std::thread;

use dashmap::DashMap;
use futures::{FutureExt, StreamExt, TryFutureExt};
use itertools::Itertools;
use pubgrub::error::PubGrubError;
use pubgrub::range::Range;
use pubgrub::solver::{Incompatibility, State};
use rustc_hash::{FxHashMap, FxHashSet};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::sync::oneshot;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, enabled, instrument, trace, warn, Level};

use distribution_types::{
    BuiltDist, Dist, DistributionMetadata, IncompatibleDist, IncompatibleSource, IncompatibleWheel,
    InstalledDist, PythonRequirementKind, RemoteSource, ResolvedDist, ResolvedDistRef, SourceDist,
    VersionOrUrlRef,
};
pub(crate) use locals::Locals;
use pep440_rs::{Version, MIN_VERSION};
use pep508_rs::{MarkerEnvironment, MarkerTree};
use platform_tags::Tags;
use pypi_types::{Metadata23, Requirement, VerbatimParsedUrl};
pub(crate) use urls::Urls;
use uv_configuration::{Constraints, Overrides};
use uv_distribution::{ArchiveMetadata, DistributionDatabase};
use uv_git::GitResolver;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_types::{BuildContext, HashStrategy, InstalledPackagesProvider};

use crate::candidate_selector::{CandidateDist, CandidateSelector};
use crate::dependency_provider::UvDependencyProvider;
use crate::error::ResolveError;
use crate::manifest::Manifest;
use crate::pins::FilePins;
use crate::preferences::Preferences;
use crate::pubgrub::{
    PubGrubDependencies, PubGrubDistribution, PubGrubPackage, PubGrubPackageInner,
    PubGrubPriorities, PubGrubPython, PubGrubSpecifier,
};
use crate::python_requirement::PythonRequirement;
use crate::requires_python::RequiresPython;
use crate::resolution::ResolutionGraph;
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
mod index;
mod locals;
mod provider;
mod reporter;
mod urls;

pub struct Resolver<Provider: ResolverProvider, InstalledPackages: InstalledPackagesProvider> {
    state: ResolverState<InstalledPackages>,
    provider: Provider,
}

/// State that is shared between the prefetcher and the PubGrub solver during
/// resolution.
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
    /// When not set, the resolver is in "universal" mode.
    markers: Option<MarkerEnvironment>,
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
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        manifest: Manifest,
        options: Options,
        python_requirement: &'a PythonRequirement,
        markers: Option<&'a MarkerEnvironment>,
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
            python_requirement.clone(),
            AllowedYanks::from_manifest(&manifest, markers, options.dependency_mode),
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
    #[allow(clippy::too_many_arguments)]
    pub fn new_custom_io(
        manifest: Manifest,
        options: Options,
        hasher: &HashStrategy,
        markers: Option<&MarkerEnvironment>,
        python_requirement: &PythonRequirement,
        index: &InMemoryIndex,
        git: &GitResolver,
        provider: Provider,
        installed_packages: InstalledPackages,
    ) -> Result<Self, ResolveError> {
        let requires_python = if markers.is_some() {
            None
        } else {
            Some(
                python_requirement
                    .requires_python()
                    .map(RequiresPython::to_marker_tree)
                    .unwrap_or_else(|| MarkerTree::And(vec![])),
            )
        };
        let state = ResolverState {
            index: index.clone(),
            git: git.clone(),
            unavailable_packages: DashMap::default(),
            incomplete_packages: DashMap::default(),
            selector: CandidateSelector::for_resolution(options, &manifest, markers),
            dependency_mode: options.dependency_mode,
            urls: Urls::from_manifest(&manifest, markers, git, options.dependency_mode)?,
            locals: Locals::from_manifest(&manifest, markers, options.dependency_mode),
            project: manifest.project,
            requirements: manifest.requirements,
            constraints: manifest.constraints,
            overrides: manifest.overrides,
            dev: manifest.dev,
            preferences: Preferences::from_iter(manifest.preferences, markers),
            exclusions: manifest.exclusions,
            hasher: hasher.clone(),
            markers: markers.cloned(),
            python_requirement: python_requirement.clone(),
            requires_python,
            reporter: None,
            installed_packages,
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
        let requests_fut = state
            .clone()
            .fetch(provider.clone(), request_stream)
            .map_err(|err| (err, FxHashSet::default()))
            .fuse();

        // Spawn the PubGrub solver on a dedicated thread.
        let solver = state.clone();
        let (tx, rx) = oneshot::channel();
        thread::Builder::new()
            .name("uv-resolver".into())
            .spawn(move || {
                let result = solver.solve(request_sink);
                tx.send(result).unwrap();
            })
            .unwrap();

        let resolve_fut = async move {
            rx.await
                .map_err(|_| (ResolveError::ChannelClosed, FxHashSet::default()))
                .and_then(|result| result)
        };

        // Wait for both to complete.
        match tokio::try_join!(requests_fut, resolve_fut) {
            Ok(((), resolution)) => {
                state.on_complete();
                Ok(resolution)
            }
            Err((err, visited)) => {
                // Add version information to improve unsat error messages.
                Err(if let ResolveError::NoSolution(err) = err {
                    ResolveError::NoSolution(
                        err.with_available_versions(&visited, state.index.packages())
                            .with_selector(state.selector.clone())
                            .with_python_requirement(&state.python_requirement)
                            .with_index_locations(provider.index_locations())
                            .with_unavailable_packages(&state.unavailable_packages)
                            .with_incomplete_packages(&state.incomplete_packages),
                    )
                } else {
                    err
                })
            }
        }
    }
}

impl<InstalledPackages: InstalledPackagesProvider> ResolverState<InstalledPackages> {
    #[instrument(skip_all)]
    fn solve(
        self: Arc<Self>,
        request_sink: Sender<Request>,
    ) -> Result<ResolutionGraph, (ResolveError, FxHashSet<PackageName>)> {
        let mut visited = FxHashSet::default();
        self.solve_tracked(&mut visited, request_sink)
            .map_err(|err| (err, visited))
    }

    /// Run the PubGrub solver, updating the `visited` set for each package visited during
    /// resolution.
    #[instrument(skip_all)]
    fn solve_tracked(
        self: Arc<Self>,
        visited: &mut FxHashSet<PackageName>,
        request_sink: Sender<Request>,
    ) -> Result<ResolutionGraph, ResolveError> {
        let root = PubGrubPackage::from(PubGrubPackageInner::Root(self.project.clone()));
        let mut prefetcher = BatchPrefetcher::default();
        let state = SolveState {
            pubgrub: State::init(root.clone(), MIN_VERSION.clone()),
            next: root,
            pins: FilePins::default(),
            priorities: PubGrubPriorities::default(),
            added_dependencies: FxHashMap::default(),
            markers: MarkerTree::And(vec![]),
        };
        let mut forked_states = vec![state];
        let mut resolutions = vec![];

        debug!(
            "Solving with installed Python version: {}",
            self.python_requirement.installed()
        );
        if let Some(target) = self.python_requirement.target() {
            debug!("Solving with target Python version: {}", target);
        }

        'FORK: while let Some(mut state) = forked_states.pop() {
            loop {
                // Run unit propagation.
                state.pubgrub.unit_propagation(state.next.clone())?;

                // Pre-visit all candidate packages, to allow metadata to be fetched in parallel. If
                // the dependency mode is direct, we only need to visit the root package.
                if self.dependency_mode.is_transitive() {
                    Self::pre_visit(
                        state.pubgrub.partial_solution.prioritized_packages(),
                        &request_sink,
                    )?;
                }

                // Choose a package version.
                let Some(highest_priority_pkg) = state
                    .pubgrub
                    .partial_solution
                    .pick_highest_priority_pkg(|package, _range| state.priorities.get(package))
                else {
                    if enabled!(Level::DEBUG) {
                        prefetcher.log_tried_versions();
                    }
                    resolutions.push(state.into_resolution());
                    continue 'FORK;
                };
                state.next = highest_priority_pkg;

                prefetcher.version_tried(state.next.clone());

                let term_intersection = state
                    .pubgrub
                    .partial_solution
                    .term_intersection_for_package(&state.next)
                    .ok_or_else(|| {
                        PubGrubError::Failure(
                            "a package was chosen but we don't have a term.".into(),
                        )
                    })?;
                let decision = self.choose_version(
                    &state.next,
                    term_intersection.unwrap_positive(),
                    &mut state.pins,
                    visited,
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
                            .expect("a package was chosen but we don't have a term.");

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
                        // Incompatible requires-python versions are special in that we track
                        // them as incompatible dependencies instead of marking the package version
                        // as unavailable directly
                        if let UnavailableVersion::IncompatibleDist(
                            IncompatibleDist::Source(IncompatibleSource::RequiresPython(
                                requires_python,
                                kind,
                            ))
                            | IncompatibleDist::Wheel(IncompatibleWheel::RequiresPython(
                                requires_python,
                                kind,
                            )),
                        ) = reason
                        {
                            let python_version: Range<Version> =
                                PubGrubSpecifier::try_from(&requires_python)?.into();

                            let package = &state.next;
                            state
                                .pubgrub
                                .add_incompatibility(Incompatibility::from_dependency(
                                    package.clone(),
                                    Range::singleton(version.clone()),
                                    (
                                        PubGrubPackage::from(PubGrubPackageInner::Python(
                                            match kind {
                                                PythonRequirementKind::Installed => {
                                                    PubGrubPython::Installed
                                                }
                                                PythonRequirementKind::Target => {
                                                    PubGrubPython::Target
                                                }
                                            },
                                        )),
                                        python_version.clone(),
                                    ),
                                ));
                            state
                                .pubgrub
                                .partial_solution
                                .add_decision(state.next.clone(), version);
                            continue;
                        };
                        state
                            .pubgrub
                            .add_incompatibility(Incompatibility::custom_version(
                                state.next.clone(),
                                version.clone(),
                                UnavailableReason::Version(reason),
                            ));
                        continue;
                    }
                };

                prefetcher.prefetch_batches(
                    &state.next,
                    &version,
                    term_intersection.unwrap_positive(),
                    &request_sink,
                    &self.index,
                    &self.selector,
                )?;

                self.on_progress(&state.next, &version);

                if state
                    .added_dependencies
                    .entry(state.next.clone())
                    .or_default()
                    .insert(version.clone())
                {
                    // Retrieve that package dependencies.
                    let forked_deps = self.get_dependencies_forking(
                        &state.next,
                        &version,
                        &state.markers,
                        &mut state.priorities,
                        &request_sink,
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
                            forked_states.push(state);
                        }
                        ForkedDependencies::Unforked(dependencies) => {
                            state.add_package_version_dependencies(
                                &version,
                                dependencies,
                                &prefetcher,
                            )?;
                            forked_states.push(state);
                        }
                        ForkedDependencies::Forked {
                            forks,
                            diverging_packages,
                        } => {
                            debug!(
                                "Splitting resolution on {} over {}",
                                state.next,
                                diverging_packages
                                    .iter()
                                    .map(ToString::to_string)
                                    .join(", ")
                            );
                            assert!(forks.len() >= 2);
                            // This is a somewhat tortured technique to ensure
                            // that our resolver state is only cloned as much
                            // as it needs to be. We basically move the state
                            // into `forked_states`, and then only clone it if
                            // there is at least one more fork to visit.
                            let mut cur_state = Some(state);
                            let forks_len = forks.len();
                            for (i, fork) in forks.into_iter().enumerate() {
                                let is_last = i == forks_len - 1;
                                let mut forked_state = cur_state.take().unwrap();
                                if !is_last {
                                    cur_state = Some(forked_state.clone());
                                }
                                forked_state.markers.and(fork.markers);

                                forked_state.add_package_version_dependencies(
                                    &version,
                                    fork.dependencies,
                                    &prefetcher,
                                )?;
                                forked_states.push(forked_state);
                            }
                        }
                    }
                    continue 'FORK;
                }
                // `dep_incompats` are already in `incompatibilities` so we know there are not satisfied
                // terms and can add the decision directly.
                state
                    .pubgrub
                    .partial_solution
                    .add_decision(state.next.clone(), version);
            }
        }
        let mut combined = Resolution::default();
        for resolution in resolutions {
            combined.union(resolution);
        }
        ResolutionGraph::from_state(
            &self.index,
            &self.preferences,
            &self.git,
            &self.python_requirement,
            combined,
        )
    }

    /// Visit a [`PubGrubPackage`] prior to selection. This should be called on a [`PubGrubPackage`]
    /// before it is selected, to allow metadata to be fetched in parallel.
    fn visit_package(
        &self,
        package: &PubGrubPackage,
        request_sink: &Sender<Request>,
    ) -> Result<(), ResolveError> {
        match &**package {
            PubGrubPackageInner::Root(_) => {}
            PubGrubPackageInner::Python(_) => {}
            PubGrubPackageInner::Marker {
                name, url: None, ..
            }
            | PubGrubPackageInner::Extra {
                name, url: None, ..
            }
            | PubGrubPackageInner::Dev {
                name, url: None, ..
            }
            | PubGrubPackageInner::Package {
                name, url: None, ..
            } => {
                // Verify that the package is allowed under the hash-checking policy.
                if !self.hasher.allows_package(name) {
                    return Err(ResolveError::UnhashedPackage(name.clone()));
                }

                // Emit a request to fetch the metadata for this package.
                if self.index.packages().register(name.clone()) {
                    request_sink.blocking_send(Request::Package(name.clone()))?;
                }
            }
            PubGrubPackageInner::Marker {
                name,
                url: Some(url),
                ..
            }
            | PubGrubPackageInner::Extra {
                name,
                url: Some(url),
                ..
            }
            | PubGrubPackageInner::Dev {
                name,
                url: Some(url),
                ..
            }
            | PubGrubPackageInner::Package {
                name,
                url: Some(url),
                ..
            } => {
                // Verify that the package is allowed under the hash-checking policy.
                if !self.hasher.allows_url(&url.verbatim) {
                    return Err(ResolveError::UnhashedPackage(name.clone()));
                }

                // Emit a request to fetch the metadata for this distribution.
                let dist = Dist::from_url(name.clone(), url.clone())?;
                if self.index.distributions().register(dist.version_id()) {
                    request_sink.blocking_send(Request::Dist(dist))?;
                }
            }
        }
        Ok(())
    }

    /// Visit the set of [`PubGrubPackage`] candidates prior to selection. This allows us to fetch
    /// metadata for all packages in parallel.
    fn pre_visit<'data>(
        packages: impl Iterator<Item = (&'data PubGrubPackage, &'data Range<Version>)>,
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
                url: None,
            } = &**package
            else {
                continue;
            };
            request_sink.blocking_send(Request::Prefetch(name.clone(), range.clone()))?;
        }
        Ok(())
    }

    /// Given a set of candidate packages, choose the next package (and version) to add to the
    /// partial solution.
    ///
    /// Returns [None] when there are no versions in the given range.
    #[instrument(skip_all, fields(%package))]
    fn choose_version(
        &self,
        package: &PubGrubPackage,
        range: &Range<Version>,
        pins: &mut FilePins,
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
                Ok(None)
            }

            PubGrubPackageInner::Marker {
                name,
                url: Some(url),
                ..
            }
            | PubGrubPackageInner::Extra {
                name,
                url: Some(url),
                ..
            }
            | PubGrubPackageInner::Dev {
                name,
                url: Some(url),
                ..
            }
            | PubGrubPackageInner::Package {
                name,
                url: Some(url),
                ..
            } => {
                debug!(
                    "Searching for a compatible version of {package} @ {} ({range})",
                    url.verbatim
                );

                let dist = PubGrubDistribution::from_url(name, url);
                let response = self
                    .index
                    .distributions()
                    .wait_blocking(&dist.version_id())
                    .ok_or(ResolveError::Unregistered)?;

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
                    if let Some(target) = self.python_requirement.target() {
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
                    if !requires_python.contains(self.python_requirement.installed()) {
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

            PubGrubPackageInner::Marker {
                name, url: None, ..
            }
            | PubGrubPackageInner::Extra {
                name, url: None, ..
            }
            | PubGrubPackageInner::Dev {
                name, url: None, ..
            }
            | PubGrubPackageInner::Package {
                name, url: None, ..
            } => {
                // Wait for the metadata to be available.
                let versions_response = self
                    .index
                    .packages()
                    .wait_blocking(name)
                    .ok_or(ResolveError::Unregistered)?;
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
                    &self.preferences,
                    &self.installed_packages,
                    &self.exclusions,
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
                    "Selecting: {}=={} ({})",
                    package,
                    candidate.version(),
                    filename,
                );

                // We want to return a package pinned to a specific version; but we _also_ want to
                // store the exact file that we selected to satisfy that version.
                pins.insert(&candidate, dist);

                let version = candidate.version().clone();

                // Emit a request to fetch the metadata for this version.
                if matches!(&**package, PubGrubPackageInner::Package { .. }) {
                    if self.index.distributions().register(candidate.version_id()) {
                        let request = Request::from(dist.for_resolution());
                        request_sink.blocking_send(request)?;
                    }
                }

                Ok(Some(ResolverVersion::Available(version)))
            }
        }
    }

    /// Given a candidate package and version, return its dependencies.
    #[instrument(skip_all, fields(%package, %version))]
    fn get_dependencies_forking(
        &self,
        package: &PubGrubPackage,
        version: &Version,
        markers: &MarkerTree,
        priorities: &mut PubGrubPriorities,
        request_sink: &Sender<Request>,
    ) -> Result<ForkedDependencies, ResolveError> {
        let result = self.get_dependencies(package, version, markers, priorities, request_sink);
        if self.markers.is_some() {
            return result.map(|deps| match deps {
                Dependencies::Available(deps) => ForkedDependencies::Unforked(deps),
                Dependencies::Unavailable(err) => ForkedDependencies::Unavailable(err),
            });
        }
        Ok(result?.fork())
    }

    /// Given a candidate package and version, return its dependencies.
    #[instrument(skip_all, fields(%package, %version))]
    fn get_dependencies(
        &self,
        package: &PubGrubPackage,
        version: &Version,
        markers: &MarkerTree,
        priorities: &mut PubGrubPriorities,
        request_sink: &Sender<Request>,
    ) -> Result<Dependencies, ResolveError> {
        match &**package {
            PubGrubPackageInner::Root(_) => {
                // Add the root requirements.
                let dependencies = PubGrubDependencies::from_requirements(
                    &self.requirements,
                    &BTreeMap::default(),
                    &self.constraints,
                    &self.overrides,
                    None,
                    None,
                    None,
                    &self.urls,
                    &self.locals,
                    &self.git,
                    self.markers.as_ref(),
                    self.requires_python.as_ref(),
                    markers,
                );

                let dependencies = match dependencies {
                    Ok(dependencies) => dependencies,
                    Err(err) => {
                        return Ok(Dependencies::Unavailable(
                            UnavailableVersion::ResolverError(uncapitalize(err.to_string())),
                        ));
                    }
                };

                for (package, version) in dependencies.iter() {
                    debug!("Adding direct dependency: {package}{version}");

                    // Update the package priorities.
                    priorities.insert(package, version);

                    // Emit a request to fetch the metadata for this package.
                    self.visit_package(package, request_sink)?;
                }

                Ok(Dependencies::Available(dependencies.into()))
            }

            PubGrubPackageInner::Python(_) => Ok(Dependencies::Available(Vec::default())),

            PubGrubPackageInner::Package {
                name,
                extra,
                dev,
                marker,
                url,
            } => {
                // If we're excluding transitive dependencies, short-circuit.
                if self.dependency_mode.is_direct() {
                    // If an extra is provided, wait for the metadata to be available, since it's
                    // still required for generating the lock file.
                    let dist = match url {
                        Some(url) => PubGrubDistribution::from_url(name, url),
                        None => PubGrubDistribution::from_registry(name, version),
                    };
                    let version_id = dist.version_id();

                    // Wait for the metadata to be available.
                    self.index
                        .distributions()
                        .wait_blocking(&version_id)
                        .ok_or(ResolveError::Unregistered)?;

                    return Ok(Dependencies::Available(Vec::default()));
                }

                // Determine the distribution to lookup.
                let dist = match url {
                    Some(url) => PubGrubDistribution::from_url(name, url),
                    None => PubGrubDistribution::from_registry(name, version),
                };
                let version_id = dist.version_id();

                // If the package does not exist in the registry or locally, we cannot fetch its dependencies
                if self.unavailable_packages.get(name).is_some()
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
                    .ok_or(ResolveError::Unregistered)?;

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

                let mut dependencies = PubGrubDependencies::from_requirements(
                    &metadata.requires_dist,
                    &metadata.dev_dependencies,
                    &self.constraints,
                    &self.overrides,
                    Some(name),
                    extra.as_ref(),
                    dev.as_ref(),
                    &self.urls,
                    &self.locals,
                    &self.git,
                    self.markers.as_ref(),
                    self.requires_python.as_ref(),
                    markers,
                )?;

                for (dep_package, dep_version) in dependencies.iter() {
                    debug!("Adding transitive dependency for {package}=={version}: {dep_package}{dep_version}");

                    // Update the package priorities.
                    priorities.insert(dep_package, dep_version);

                    // Emit a request to fetch the metadata for this package.
                    self.visit_package(dep_package, request_sink)?;
                }

                // If a package has metadata for an enabled dependency group,
                // add a dependency from it to the same package with the group
                // enabled.
                if extra.is_none() && dev.is_none() {
                    for dev in &self.dev {
                        if metadata.dev_dependencies.contains_key(dev) {
                            dependencies.push(
                                PubGrubPackage::from(PubGrubPackageInner::Dev {
                                    name: name.clone(),
                                    dev: dev.clone(),
                                    marker: marker.clone(),
                                    url: url.clone(),
                                }),
                                Range::singleton(version.clone()),
                            );
                        }
                    }
                }

                Ok(Dependencies::Available(dependencies.into()))
            }

            // Add a dependency on both the marker and base package.
            PubGrubPackageInner::Marker { name, marker, url } => Ok(Dependencies::Available(
                [None, Some(marker)]
                    .into_iter()
                    .map(move |marker| {
                        (
                            PubGrubPackage::from(PubGrubPackageInner::Package {
                                name: name.clone(),
                                extra: None,
                                dev: None,
                                marker: marker.cloned(),
                                url: url.clone(),
                            }),
                            Range::singleton(version.clone()),
                        )
                    })
                    .collect(),
            )),

            // Add a dependency on both the extra and base package, with and without the marker.
            PubGrubPackageInner::Extra {
                name,
                extra,
                marker,
                url,
            } => Ok(Dependencies::Available(
                [None, marker.as_ref()]
                    .into_iter()
                    .dedup()
                    .flat_map(move |marker| {
                        [None, Some(extra)].into_iter().map(move |extra| {
                            (
                                PubGrubPackage::from(PubGrubPackageInner::Package {
                                    name: name.clone(),
                                    extra: extra.cloned(),
                                    dev: None,
                                    marker: marker.cloned(),
                                    url: url.clone(),
                                }),
                                Range::singleton(version.clone()),
                            )
                        })
                    })
                    .collect(),
            )),

            // Add a dependency on both the development dependency group and base package, with and
            // without the marker.
            PubGrubPackageInner::Dev {
                name,
                dev,
                marker,
                url,
            } => Ok(Dependencies::Available(
                [None, marker.as_ref()]
                    .into_iter()
                    .dedup()
                    .flat_map(move |marker| {
                        [None, Some(dev)].into_iter().map(move |dev| {
                            (
                                PubGrubPackage::from(PubGrubPackageInner::Package {
                                    name: name.clone(),
                                    extra: None,
                                    dev: dev.cloned(),
                                    marker: marker.cloned(),
                                    url: url.clone(),
                                }),
                                Range::singleton(version.clone()),
                            )
                        })
                    })
                    .collect(),
            )),
        }
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
            Request::Prefetch(package_name, range) => {
                // Wait for the package metadata to become available.
                let versions_response = self
                    .index
                    .packages()
                    .wait(&package_name)
                    .await
                    .ok_or(ResolveError::Unregistered)?;

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

                // Emit a request to fetch the metadata for this version.
                if self.index.distributions().register(candidate.version_id()) {
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

    fn on_progress(&self, package: &PubGrubPackage, version: &Version) {
        if let Some(reporter) = self.reporter.as_ref() {
            match &**package {
                PubGrubPackageInner::Root(_) => {}
                PubGrubPackageInner::Python(_) => {}
                PubGrubPackageInner::Marker { .. } => {}
                PubGrubPackageInner::Extra { .. } => {}
                PubGrubPackageInner::Dev { .. } => {}
                PubGrubPackageInner::Package {
                    name,
                    url: Some(url),
                    ..
                } => {
                    reporter.on_progress(name, &VersionOrUrlRef::Url(&url.verbatim));
                }
                PubGrubPackageInner::Package {
                    name, url: None, ..
                } => {
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

/// State that is used during unit propagation in the resolver.
#[derive(Clone)]
struct SolveState {
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
    /// to a single `ResolvedDist`. That `ResolvedDist` represents, at time
    /// of writing (2024/05/09), at most one wheel. The idea here is that
    /// `FilePins` tracks precisely which wheel was selected during resolution.
    /// After resolution is finished, this maps is consulted in order to select
    /// the wheel chosen during resolution.
    pins: FilePins,
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
    markers: MarkerTree,
}

impl SolveState {
    /// Add the dependencies for the selected version of the current package.
    fn add_package_version_dependencies(
        &mut self,
        version: &Version,
        dependencies: Vec<(PubGrubPackage, Range<Version>)>,
        prefetcher: &BatchPrefetcher,
    ) -> Result<(), ResolveError> {
        if dependencies
            .iter()
            .any(|(dependency, _)| dependency == &self.next)
        {
            if enabled!(Level::DEBUG) {
                prefetcher.log_tried_versions();
            }
            return Err(PubGrubError::SelfDependency {
                package: self.next.clone(),
                version: version.clone(),
            }
            .into());
        }
        self.pubgrub.add_package_version_dependencies(
            self.next.clone(),
            version.clone(),
            dependencies,
        );
        Ok(())
    }

    fn into_resolution(self) -> Resolution {
        let solution = self.pubgrub.partial_solution.extract_solution();
        let mut dependencies: FxHashMap<
            ResolutionDependencyNames,
            FxHashSet<ResolutionDependencyVersions>,
        > = FxHashMap::default();
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

                let PubGrubPackageInner::Package {
                    name: ref self_name,
                    extra: ref self_extra,
                    dev: ref self_dev,
                    ..
                } = &**self_package
                else {
                    continue;
                };

                match **dependency_package {
                    PubGrubPackageInner::Package {
                        name: ref dependency_name,
                        extra: ref dependency_extra,
                        dev: ref dependency_dev,
                        ..
                    } => {
                        if self_name == dependency_name {
                            continue;
                        }
                        let names = ResolutionDependencyNames {
                            from: self_name.clone(),
                            to: dependency_name.clone(),
                        };
                        let versions = ResolutionDependencyVersions {
                            from_version: self_version.clone(),
                            from_extra: self_extra.clone(),
                            from_dev: self_dev.clone(),
                            to_version: dependency_version.clone(),
                            to_extra: dependency_extra.clone(),
                            to_dev: dependency_dev.clone(),
                            marker: None,
                        };
                        dependencies.entry(names).or_default().insert(versions);
                    }

                    PubGrubPackageInner::Marker {
                        name: ref dependency_name,
                        marker: ref dependency_marker,
                        ..
                    } => {
                        if self_name == dependency_name {
                            continue;
                        }
                        let names = ResolutionDependencyNames {
                            from: self_name.clone(),
                            to: dependency_name.clone(),
                        };
                        let versions = ResolutionDependencyVersions {
                            from_version: self_version.clone(),
                            from_extra: self_extra.clone(),
                            from_dev: self_dev.clone(),
                            to_version: dependency_version.clone(),
                            to_extra: None,
                            to_dev: None,
                            marker: Some(dependency_marker.clone()),
                        };
                        dependencies.entry(names).or_default().insert(versions);
                    }

                    PubGrubPackageInner::Extra {
                        name: ref dependency_name,
                        extra: ref dependency_extra,
                        marker: ref dependency_marker,
                        ..
                    } => {
                        if self_name == dependency_name {
                            continue;
                        }
                        let names = ResolutionDependencyNames {
                            from: self_name.clone(),
                            to: dependency_name.clone(),
                        };
                        let versions = ResolutionDependencyVersions {
                            from_version: self_version.clone(),
                            from_extra: self_extra.clone(),
                            from_dev: self_dev.clone(),
                            to_version: dependency_version.clone(),
                            to_extra: Some(dependency_extra.clone()),
                            to_dev: None,
                            marker: dependency_marker.clone(),
                        };
                        dependencies.entry(names).or_default().insert(versions);
                    }

                    PubGrubPackageInner::Dev {
                        name: ref dependency_name,
                        dev: ref dependency_dev,
                        marker: ref dependency_marker,
                        ..
                    } => {
                        if self_name == dependency_name {
                            continue;
                        }
                        let names = ResolutionDependencyNames {
                            from: self_name.clone(),
                            to: dependency_name.clone(),
                        };
                        let versions = ResolutionDependencyVersions {
                            from_version: self_version.clone(),
                            from_extra: self_extra.clone(),
                            from_dev: self_dev.clone(),
                            to_version: dependency_version.clone(),
                            to_extra: None,
                            to_dev: Some(dependency_dev.clone()),
                            marker: dependency_marker.clone(),
                        };
                        dependencies.entry(names).or_default().insert(versions);
                    }

                    _ => {}
                }
            }
        }

        let packages = solution
            .into_iter()
            .filter_map(|(package, version)| {
                if let PubGrubPackageInner::Package {
                    name,
                    extra,
                    dev,
                    url,
                    marker: None,
                } = &*package
                {
                    Some((
                        ResolutionPackage {
                            name: name.clone(),
                            extra: extra.clone(),
                            dev: dev.clone(),
                            url: url.clone(),
                        },
                        FxHashSet::from_iter([version]),
                    ))
                } else {
                    None
                }
            })
            .collect();

        Resolution {
            packages,
            dependencies,
            pins: self.pins,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Resolution {
    pub(crate) packages: FxHashMap<ResolutionPackage, FxHashSet<Version>>,
    pub(crate) dependencies:
        FxHashMap<ResolutionDependencyNames, FxHashSet<ResolutionDependencyVersions>>,
    pub(crate) pins: FilePins,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResolutionPackage {
    pub(crate) name: PackageName,
    pub(crate) extra: Option<ExtraName>,
    pub(crate) dev: Option<GroupName>,
    pub(crate) url: Option<VerbatimParsedUrl>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResolutionDependencyNames {
    pub(crate) from: PackageName,
    pub(crate) to: PackageName,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResolutionDependencyVersions {
    pub(crate) from_version: Version,
    pub(crate) from_extra: Option<ExtraName>,
    pub(crate) from_dev: Option<GroupName>,
    pub(crate) to_version: Version,
    pub(crate) to_extra: Option<ExtraName>,
    pub(crate) to_dev: Option<GroupName>,
    pub(crate) marker: Option<MarkerTree>,
}

impl Resolution {
    fn union(&mut self, other: Resolution) {
        for (other_package, other_versions) in other.packages {
            self.packages
                .entry(other_package)
                .or_default()
                .extend(other_versions);
        }
        for (names, versions) in other.dependencies {
            self.dependencies.entry(names).or_default().extend(versions);
        }
        self.pins.union(other.pins);
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
    Prefetch(PackageName, Range<Version>),
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
            Self::Prefetch(package_name, range) => {
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
#[derive(Clone)]
enum Dependencies {
    /// Package dependencies are not available.
    Unavailable(UnavailableVersion),
    /// Container for all available package versions.
    ///
    /// Note that in universal mode, it is possible and allowed for multiple
    /// `PubGrubPackage` values in this list to have the same package name.
    /// These conflicts are resolved via `Dependencies::fork`.
    Available(Vec<(PubGrubPackage, Range<Version>)>),
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
        for (index, (ref pkg, _)) in deps.iter().enumerate() {
            let (name, marker) = match &**pkg {
                // A root can never be a dependency of another package, and a `Python` pubgrub
                // package is never returned by `get_dependencies`. So these cases never occur.
                PubGrubPackageInner::Root(_) | PubGrubPackageInner::Python(_) => unreachable!(),
                PubGrubPackageInner::Package { name, marker, .. }
                | PubGrubPackageInner::Extra { name, marker, .. }
                | PubGrubPackageInner::Dev { name, marker, .. } => (name, marker.as_ref()),
                PubGrubPackageInner::Marker { name, marker, .. } => (name, Some(marker)),
            };
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
            let fork_groups = match possible_forks {
                PossibleForks::PossiblyForking(fork_groups) => fork_groups,
                PossibleForks::NoForkPossible(indices) => {
                    // No fork is provoked by this package, so just add
                    // everything in this group to each of the forks.
                    for index in indices {
                        for fork in &mut forks {
                            fork.dependencies.push(deps[index].clone());
                        }
                    }
                    continue;
                }
            };
            let mut new_forks: Vec<Fork> = vec![];
            for group in fork_groups.forks {
                let mut new_forks_for_group = forks.clone();
                for (index, markers) in group.packages {
                    for fork in &mut new_forks_for_group {
                        fork.dependencies.push(deps[index].clone());
                        // Each marker expression in a single fork is,
                        // by construction, overlapping with at least
                        // one other marker expression in this fork.
                        // However, the symmetric differences may be
                        // non-empty. Therefore, the markers need to be
                        // combined on the corresponding fork
                        fork.markers.and(markers.clone());
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
    Unforked(Vec<(PubGrubPackage, Range<Version>)>),
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
    dependencies: Vec<(PubGrubPackage, Range<Version>)>,
    /// The markers that provoked this fork.
    ///
    /// So in the example above, the `a<2` fork would have
    /// `sys_platform == 'foo'`, while the `a>=2` fork would have
    /// `sys_platform == 'bar'`.
    ///
    /// (This doesn't include any marker expressions from a parent fork.)
    markers: MarkerTree,
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
}

fn uncapitalize<T: AsRef<str>>(string: T) -> String {
    let mut chars = string.as_ref().chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_lowercase().chain(chars).collect(),
    }
}
