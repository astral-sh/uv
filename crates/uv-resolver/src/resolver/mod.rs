//! Given a set of requirements, find a set of compatible packages.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::{Display, Formatter, Write};
use std::ops::Bound;
use std::sync::Arc;
use std::time::Instant;
use std::{iter, slice, thread};

use dashmap::DashMap;
use either::Either;
use futures::{FutureExt, StreamExt};
use itertools::Itertools;
use pubgrub::{Id, IncompId, Incompatibility, Kind, Range, Ranges, State};
use rustc_hash::{FxHashMap, FxHashSet};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::sync::oneshot;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{Level, debug, info, instrument, trace, warn};

use uv_configuration::{Constraints, Overrides};
use uv_distribution::{ArchiveMetadata, DistributionDatabase};
use uv_distribution_types::{
    BuiltDist, CompatibleDist, DerivationChain, Dist, DistErrorKind, DistributionMetadata,
    GlobalVersionId, IncompatibleDist, IncompatibleSource, IncompatibleWheel, IndexCapabilities,
    IndexLocations, IndexMetadata, IndexUrl, InstalledDist, Name, PrioritizedDist,
    PythonRequirementKind, RegistryVariantsJson, RemoteSource, Requirement, ResolvedDist,
    ResolvedDistRef, SourceDist, VersionId, VersionOrUrlRef,
};
use uv_git::GitResolver;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::{MIN_VERSION, Version, VersionSpecifiers, release_specifiers_to_ranges};
use uv_pep508::{
    MarkerEnvironment, MarkerExpression, MarkerOperator, MarkerTree, MarkerValueString,
    MarkerVariantsEnvironment, MarkerVariantsUniversal,
};
use uv_platform_tags::Tags;
use uv_pypi_types::{ConflictItem, ConflictItemRef, ConflictKindRef, Conflicts, VerbatimParsedUrl};
use uv_types::{BuildContext, HashStrategy, InstalledPackagesProvider};
use uv_warnings::warn_user_once;

use crate::candidate_selector::{Candidate, CandidateDist, CandidateSelector};
use crate::dependency_provider::UvDependencyProvider;
use crate::error::{NoSolutionError, ResolveError};
use crate::fork_indexes::ForkIndexes;
use crate::fork_strategy::ForkStrategy;
use crate::fork_urls::ForkUrls;
use crate::manifest::Manifest;
use crate::pins::FilePins;
use crate::preferences::{PreferenceSource, Preferences};
use crate::pubgrub::{
    PubGrubDependency, PubGrubDistribution, PubGrubPackage, PubGrubPackageInner, PubGrubPriorities,
    PubGrubPython,
};
use crate::python_requirement::PythonRequirement;
use crate::resolution::ResolverOutput;
use crate::resolution_mode::ResolutionStrategy;
pub(crate) use crate::resolver::availability::{
    ResolverVersion, UnavailableErrorChain, UnavailablePackage, UnavailableReason,
    UnavailableVersion,
};
use crate::resolver::batch_prefetch::BatchPrefetcher;
pub use crate::resolver::derivation::DerivationChainBuilder;
pub use crate::resolver::environment::ResolverEnvironment;
use crate::resolver::environment::{
    ForkingPossibility, fork_version_by_marker, fork_version_by_python_requirement,
};
pub(crate) use crate::resolver::fork_map::{ForkMap, ForkSet};
pub use crate::resolver::index::InMemoryIndex;
use crate::resolver::indexes::Indexes;
pub use crate::resolver::provider::{
    DefaultResolverProvider, MetadataResponse, PackageVersionsResult, ResolverProvider,
    VariantProviderResult, VersionsResponse, WheelMetadataResult,
};
pub use crate::resolver::reporter::{BuildId, Reporter};
use crate::resolver::system::SystemDependency;
pub(crate) use crate::resolver::urls::Urls;
use crate::universal_marker::{ConflictMarker, UniversalMarker};
use crate::yanks::AllowedYanks;
use crate::{
    DependencyMode, ExcludeNewer, Exclusions, FlatIndex, Options, ResolutionMode, VersionMap,
    marker,
};
pub(crate) use provider::MetadataUnavailable;
use uv_torch::TorchStrategy;
use uv_variants::resolved_variants::ResolvedVariants;
use uv_variants::variants_json::Variant;

mod availability;
mod batch_prefetch;
mod derivation;
mod environment;
mod fork_map;
mod index;
mod indexes;
mod provider;
mod reporter;
mod system;
mod urls;

/// The number of conflicts a package may accumulate before we re-prioritize and backtrack.
const CONFLICT_THRESHOLD: usize = 5;

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
    preferences: Preferences,
    git: GitResolver,
    capabilities: IndexCapabilities,
    locations: IndexLocations,
    exclusions: Exclusions,
    urls: Urls,
    indexes: Indexes,
    dependency_mode: DependencyMode,
    hasher: HashStrategy,
    env: ResolverEnvironment,
    // The environment of the current Python interpreter.
    current_environment: MarkerEnvironment,
    tags: Option<Tags>,
    python_requirement: PythonRequirement,
    conflicts: Conflicts,
    workspace_members: BTreeSet<PackageName>,
    selector: CandidateSelector,
    index: InMemoryIndex,
    installed_packages: InstalledPackages,
    /// Incompatibilities for packages that are entirely unavailable.
    unavailable_packages: DashMap<PackageName, UnavailablePackage>,
    /// Incompatibilities for packages that are unavailable at specific versions.
    incomplete_packages: DashMap<PackageName, DashMap<Version, MetadataUnavailable>>,
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
        env: ResolverEnvironment,
        current_environment: &MarkerEnvironment,
        conflicts: Conflicts,
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
            AllowedYanks::from_manifest(&manifest, &env, options.dependency_mode),
            hasher,
            options.exclude_newer.clone(),
            build_context.build_options(),
            build_context.capabilities(),
        );

        Self::new_custom_io(
            manifest,
            options,
            hasher,
            env,
            current_environment,
            tags.cloned(),
            python_requirement,
            conflicts,
            index,
            build_context.git(),
            build_context.capabilities(),
            build_context.locations(),
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
        env: ResolverEnvironment,
        current_environment: &MarkerEnvironment,
        tags: Option<Tags>,
        python_requirement: &PythonRequirement,
        conflicts: Conflicts,
        index: &InMemoryIndex,
        git: &GitResolver,
        capabilities: &IndexCapabilities,
        locations: &IndexLocations,
        provider: Provider,
        installed_packages: InstalledPackages,
    ) -> Result<Self, ResolveError> {
        let state = ResolverState {
            index: index.clone(),
            git: git.clone(),
            capabilities: capabilities.clone(),
            selector: CandidateSelector::for_resolution(&options, &manifest, &env),
            dependency_mode: options.dependency_mode,
            urls: Urls::from_manifest(&manifest, &env, git, options.dependency_mode),
            indexes: Indexes::from_manifest(&manifest, &env, options.dependency_mode),
            project: manifest.project,
            workspace_members: manifest.workspace_members,
            requirements: manifest.requirements,
            constraints: manifest.constraints,
            overrides: manifest.overrides,
            preferences: manifest.preferences,
            exclusions: manifest.exclusions,
            hasher: hasher.clone(),
            locations: locations.clone(),
            env,
            current_environment: current_environment.clone(),
            tags,
            python_requirement: python_requirement.clone(),
            conflicts,
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
    pub fn with_reporter(self, reporter: Arc<dyn Reporter>) -> Self {
        Self {
            state: ResolverState {
                reporter: Some(reporter.clone()),
                ..self.state
            },
            provider: self
                .provider
                .with_reporter(reporter.into_distribution_reporter()),
        }
    }

    /// Resolve a set of requirements into a set of pinned versions.
    pub async fn resolve(self) -> Result<ResolverOutput, ResolveError> {
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
        let (tx, rx) = oneshot::channel();
        thread::Builder::new()
            .name("uv-resolver".into())
            .spawn(move || {
                let result = solver.solve(&request_sink);

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
        request_sink: &Sender<Request>,
    ) -> Result<ResolverOutput, ResolveError> {
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
        let pubgrub = State::init(root.clone(), MIN_VERSION.clone());
        let prefetcher = BatchPrefetcher::new(
            self.capabilities.clone(),
            self.index.clone(),
            request_sink.clone(),
        );
        let state = ForkState::new(
            pubgrub,
            self.env.clone(),
            self.python_requirement.clone(),
            prefetcher,
        );
        let mut preferences = self.preferences.clone();
        let mut forked_states = self.env.initial_forked_states(state)?;
        let mut resolutions = vec![];

        'FORK: while let Some(mut state) = forked_states.pop() {
            if let Some(split) = state.env.end_user_fork_display() {
                let requires_python = state.python_requirement.target();
                debug!("Solving {split} (requires-python: {requires_python:?})");
            }
            let start = Instant::now();
            loop {
                let highest_priority_pkg =
                    if let Some(initial) = state.initial_id.take() {
                        // If we just forked based on `requires-python`, we can skip unit
                        // propagation, since we already propagated the package that initiated
                        // the fork.
                        initial
                    } else {
                        // Run unit propagation.
                        let result = state.pubgrub.unit_propagation(state.next);
                        match result {
                            Err(err) => {
                                // If unit propagation failed, there is no solution.
                                return Err(self.convert_no_solution_err(
                                    err,
                                    state.fork_urls,
                                    state.fork_indexes,
                                    state.env,
                                    self.current_environment.clone(),
                                    Some(&self.options.exclude_newer),
                                    &visited,
                                ));
                            }
                            Ok(conflicts) => {
                                for (affected, incompatibility) in conflicts {
                                    // Conflict tracking: If there was a conflict, track affected and
                                    // culprit for all root cause incompatibilities
                                    state.record_conflict(affected, None, incompatibility);
                                }
                            }
                        }

                        // Pre-visit all candidate packages, to allow metadata to be fetched in parallel.
                        if self.dependency_mode.is_transitive() {
                            Self::pre_visit(
                                state
                                    .pubgrub
                                    .partial_solution
                                    .prioritized_packages()
                                    .map(|(id, range)| (&state.pubgrub.package_store[id], range)),
                                &self.urls,
                                &self.indexes,
                                &state.python_requirement,
                                request_sink,
                            )?;
                        }

                        Self::reprioritize_conflicts(&mut state);

                        trace!(
                            "Assigned packages: {}",
                            state
                                .pubgrub
                                .partial_solution
                                .extract_solution()
                                .filter(|(p, _)| !state.pubgrub.package_store[*p].is_proxy())
                                .map(|(p, v)| format!("{}=={}", state.pubgrub.package_store[p], v))
                                .join(", ")
                        );
                        // Choose a package.
                        // We aren't allowed to use the term intersection as it would extend the
                        // mutable borrow of `state`.
                        let Some((highest_priority_pkg, _)) =
                            state.pubgrub.partial_solution.pick_highest_priority_pkg(
                                |id, _range| state.priorities.get(&state.pubgrub.package_store[id]),
                            )
                        else {
                            // All packages have been assigned, the fork has been successfully resolved
                            if tracing::enabled!(Level::DEBUG) {
                                state.prefetcher.log_tried_versions();
                            }
                            debug!(
                                "{} resolution took {:.3}s",
                                state.env,
                                start.elapsed().as_secs_f32()
                            );

                            let resolution = state.into_resolution();

                            // Walk over the selected versions, and mark them as preferences. We have to
                            // add forks back as to not override the preferences from the lockfile for
                            // the next fork
                            //
                            // If we're using a resolution mode that varies based on whether a dependency is
                            // direct or transitive, skip preferences, as we risk adding a preference from
                            // one fork (in which it's a transitive dependency) to another fork (in which
                            // it's direct).
                            if matches!(
                                self.options.resolution_mode,
                                ResolutionMode::Lowest | ResolutionMode::Highest
                            ) {
                                for (package, version) in &resolution.nodes {
                                    preferences.insert(
                                        package.name.clone(),
                                        package.index.clone(),
                                        resolution
                                            .env
                                            .try_universal_markers()
                                            .unwrap_or(UniversalMarker::TRUE),
                                        version.clone(),
                                        PreferenceSource::Resolver,
                                    );
                                }
                            }

                            resolutions.push(resolution);
                            continue 'FORK;
                        };
                        trace!(
                            "Chose package for decision: {}. remaining choices: {}",
                            state.pubgrub.package_store[highest_priority_pkg],
                            state
                                .pubgrub
                                .partial_solution
                                .undecided_packages()
                                .filter(|(p, _)| !state.pubgrub.package_store[**p].is_proxy())
                                .map(|(p, _)| state.pubgrub.package_store[*p].to_string())
                                .join(", ")
                        );

                        highest_priority_pkg
                    };

                state.next = highest_priority_pkg;

                // TODO(charlie): Remove as many usages of `next_package` as we can.
                let next_id = state.next;
                let next_package = &state.pubgrub.package_store[state.next];

                let url = next_package
                    .name()
                    .and_then(|name| state.fork_urls.get(name));
                let index = next_package
                    .name()
                    .and_then(|name| state.fork_indexes.get(name));

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
                self.request_package(next_package, url, index, request_sink)?;

                let version = if let Some(version) = state.initial_version.take() {
                    // If we just forked based on platform support, we can skip version selection,
                    // since the fork operation itself already selected the appropriate version for
                    // the platform.
                    version
                } else {
                    let term_intersection = state
                        .pubgrub
                        .partial_solution
                        .term_intersection_for_package(next_id)
                        .expect("a package was chosen but we don't have a term");
                    let decision = self.choose_version(
                        next_package,
                        next_id,
                        index.map(IndexMetadata::url),
                        term_intersection.unwrap_positive(),
                        &mut state.pins,
                        &preferences,
                        &state.fork_urls,
                        &state.env,
                        &state.python_requirement,
                        &state.pubgrub,
                        &mut visited,
                        request_sink,
                    )?;

                    // Pick the next compatible version.
                    let Some(version) = decision else {
                        debug!("No compatible version found for: {next_package}");

                        let term_intersection = state
                            .pubgrub
                            .partial_solution
                            .term_intersection_for_package(next_id)
                            .expect("a package was chosen but we don't have a term");

                        if let PubGrubPackageInner::Package { name, .. } = &**next_package {
                            // Check if the decision was due to the package being unavailable
                            if let Some(entry) = self.unavailable_packages.get(name) {
                                state
                                    .pubgrub
                                    .add_incompatibility(Incompatibility::custom_term(
                                        next_id,
                                        term_intersection.clone(),
                                        UnavailableReason::Package(entry.clone()),
                                    ));
                                continue;
                            }
                        }

                        state
                            .pubgrub
                            .add_incompatibility(Incompatibility::no_versions(
                                next_id,
                                term_intersection.clone(),
                            ));
                        continue;
                    };

                    let version = match version {
                        ResolverVersion::Unforked(version) => version,
                        ResolverVersion::Forked(forks) => {
                            forked_states.extend(self.version_forks_to_fork_states(state, forks));
                            continue 'FORK;
                        }
                        ResolverVersion::Unavailable(version, reason) => {
                            state.add_unavailable_version(version, reason);
                            continue;
                        }
                    };

                    // Only consider registry packages for prefetch.
                    if url.is_none() {
                        state.prefetcher.prefetch_batches(
                            next_package,
                            index,
                            &version,
                            term_intersection.unwrap_positive(),
                            state
                                .pubgrub
                                .partial_solution
                                .unchanging_term_for_package(next_id),
                            &state.python_requirement,
                            &self.selector,
                            &state.env,
                        )?;
                    }

                    version
                };

                state.prefetcher.version_tried(next_package, &version);

                self.on_progress(next_package, &version);

                if !state
                    .added_dependencies
                    .entry(next_id)
                    .or_default()
                    .insert(version.clone())
                {
                    // `dep_incompats` are already in `incompatibilities` so we know there are not satisfied
                    // terms and can add the decision directly.
                    state
                        .pubgrub
                        .partial_solution
                        .add_decision(next_id, version);
                    continue;
                }

                // Retrieve that package dependencies.
                let forked_deps = self.get_dependencies_forking(
                    next_id,
                    next_package,
                    &version,
                    &state.pins,
                    &state.fork_urls,
                    &state.env,
                    &self.index,
                    &state.python_requirement,
                    &state.pubgrub,
                )?;

                match forked_deps {
                    ForkedDependencies::Unavailable(reason) => {
                        // Then here, if we get a reason that we consider unrecoverable, we should
                        // show the derivation chain.
                        state
                            .pubgrub
                            .add_incompatibility(Incompatibility::custom_version(
                                next_id,
                                version.clone(),
                                UnavailableReason::Version(reason),
                            ));
                    }
                    ForkedDependencies::Unforked(dependencies) => {
                        // Enrich the state with any URLs, etc.
                        state
                            .visit_package_version_dependencies(
                                next_id,
                                &version,
                                &self.urls,
                                &self.indexes,
                                &dependencies,
                                &self.git,
                                &self.workspace_members,
                                self.selector.resolution_strategy(),
                            )
                            .map_err(|err| {
                                enrich_dependency_error(err, next_id, &version, &state.pubgrub)
                            })?;

                        // Emit a request to fetch the metadata for each registry package.
                        self.visit_dependencies(&dependencies, &state, request_sink)
                            .map_err(|err| {
                                enrich_dependency_error(err, next_id, &version, &state.pubgrub)
                            })?;

                        // Add the dependencies to the state.
                        state.add_package_version_dependencies(next_id, &version, dependencies);
                    }
                    ForkedDependencies::Forked {
                        mut forks,
                        diverging_packages,
                    } => {
                        debug!(
                            "Pre-fork {} took {:.3}s",
                            state.env,
                            start.elapsed().as_secs_f32()
                        );

                        // Prioritize the forks.
                        match (self.options.fork_strategy, self.options.resolution_mode) {
                            (ForkStrategy::Fewest, _) | (_, ResolutionMode::Lowest) => {
                                // Prefer solving forks with lower Python bounds, since they're more
                                // likely to produce solutions that work for forks with higher
                                // Python bounds (whereas the inverse is not true).
                                forks.sort_by(|a, b| {
                                    a.cmp_requires_python(b)
                                        .reverse()
                                        .then_with(|| a.cmp_upper_bounds(b))
                                });
                            }
                            (ForkStrategy::RequiresPython, _) => {
                                // Otherwise, prefer solving forks with higher Python bounds, since
                                // we want to prioritize choosing the latest-compatible package
                                // version for each Python version.
                                forks.sort_by(|a, b| {
                                    a.cmp_requires_python(b).then_with(|| a.cmp_upper_bounds(b))
                                });
                            }
                        }

                        for new_fork_state in self.forks_to_fork_states(
                            state,
                            &version,
                            forks,
                            request_sink,
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
        if tracing::enabled!(Level::DEBUG) {
            for resolution in &resolutions {
                if let Some(env) = resolution.env.end_user_fork_display() {
                    let packages: FxHashSet<_> = resolution
                        .nodes
                        .keys()
                        .map(|package| &package.name)
                        .collect();
                    debug!(
                        "Distinct solution for {env} with {} package(s)",
                        packages.len()
                    );
                }
            }
        }
        for resolution in &resolutions {
            Self::trace_resolution(resolution);
        }
        ResolverOutput::from_state(
            &resolutions,
            &self.requirements,
            &self.constraints,
            &self.overrides,
            &self.preferences,
            &self.index,
            &self.git,
            &self.python_requirement,
            &self.conflicts,
            self.selector.resolution_strategy(),
            self.options.clone(),
        )
    }

    /// Change the priority of often conflicting packages and backtrack.
    ///
    /// To be called after unit propagation.
    fn reprioritize_conflicts(state: &mut ForkState) {
        for package in state.conflict_tracker.prioritize.drain(..) {
            let changed = state
                .priorities
                .mark_conflict_early(&state.pubgrub.package_store[package]);
            if changed {
                debug!(
                    "Package {} has too many conflicts (affected), prioritizing",
                    &state.pubgrub.package_store[package]
                );
            } else {
                debug!(
                    "Package {} has too many conflicts (affected), already {:?}",
                    state.pubgrub.package_store[package],
                    state.priorities.get(&state.pubgrub.package_store[package])
                );
            }
        }

        for package in state.conflict_tracker.deprioritize.drain(..) {
            let changed = state
                .priorities
                .mark_conflict_late(&state.pubgrub.package_store[package]);
            if changed {
                debug!(
                    "Package {} has too many conflicts (culprit), deprioritizing and backtracking",
                    state.pubgrub.package_store[package],
                );
                let backtrack_level = state.pubgrub.backtrack_package(package);
                if let Some(backtrack_level) = backtrack_level {
                    debug!("Backtracked {backtrack_level} decisions");
                } else {
                    debug!(
                        "Package {} is not decided, cannot backtrack",
                        state.pubgrub.package_store[package]
                    );
                }
            } else {
                debug!(
                    "Package {} has too many conflicts (culprit), already {:?}",
                    state.pubgrub.package_store[package],
                    state.priorities.get(&state.pubgrub.package_store[package])
                );
            }
        }
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
        trace!("Resolution: {:?}", combined.env);
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
            if let Some(ref dev) = edge.from_group {
                write!(msg, " (group: {dev})").unwrap();
            }

            write!(msg, " -> ").unwrap();

            write!(msg, "{}", edge.to_version).unwrap();
            if let Some(ref extra) = edge.to_extra {
                write!(msg, " (extra: {extra})").unwrap();
            }
            if let Some(ref dev) = edge.to_group {
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
        diverging_packages: &'a [PackageName],
    ) -> impl Iterator<Item = Result<ForkState, ResolveError>> + 'a {
        debug!(
            "Splitting resolution on {}=={} over {} into {} resolution{} with separate markers",
            current_state.pubgrub.package_store[current_state.next],
            version,
            diverging_packages
                .iter()
                .map(ToString::to_string)
                .join(", "),
            forks.len(),
            if forks.len() == 1 { "" } else { "s" }
        );
        assert!(forks.len() >= 2);
        // This is a somewhat tortured technique to ensure
        // that our resolver state is only cloned as much
        // as it needs to be. We basically move the state
        // into `forked_states`, and then only clone it if
        // there is at least one more fork to visit.
        let package = current_state.next;
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

                let env = fork.env.clone();
                (fork, forked_state.with_env(env))
            })
            .map(move |(fork, mut forked_state)| {
                // Enrich the state with any URLs, etc.
                forked_state
                    .visit_package_version_dependencies(
                        package,
                        version,
                        &self.urls,
                        &self.indexes,
                        &fork.dependencies,
                        &self.git,
                        &self.workspace_members,
                        self.selector.resolution_strategy(),
                    )
                    .map_err(|err| {
                        enrich_dependency_error(err, package, version, &forked_state.pubgrub)
                    })?;

                // Emit a request to fetch the metadata for each registry package.
                self.visit_dependencies(&fork.dependencies, &forked_state, request_sink)
                    .map_err(|err| {
                        enrich_dependency_error(err, package, version, &forked_state.pubgrub)
                    })?;

                // Add the dependencies to the state.
                forked_state.add_package_version_dependencies(package, version, fork.dependencies);

                Ok(forked_state)
            })
    }

    /// Convert the dependency [`Fork`]s into [`ForkState`]s.
    #[allow(clippy::unused_self)]
    fn version_forks_to_fork_states(
        &self,
        current_state: ForkState,
        forks: Vec<VersionFork>,
    ) -> impl Iterator<Item = ForkState> + '_ {
        // This is a somewhat tortured technique to ensure
        // that our resolver state is only cloned as much
        // as it needs to be. We basically move the state
        // into `forked_states`, and then only clone it if
        // there is at least one more fork to visit.
        let mut cur_state = Some(current_state);
        let forks_len = forks.len();
        forks.into_iter().enumerate().map(move |(i, fork)| {
            let is_last = i == forks_len - 1;
            let mut forked_state = cur_state.take().unwrap();
            if !is_last {
                cur_state = Some(forked_state.clone());
            }
            forked_state.initial_id = Some(fork.id);
            forked_state.initial_version = fork.version;
            forked_state.with_env(fork.env)
        })
    }

    /// Visit a set of [`PubGrubDependency`] entities prior to selection.
    fn visit_dependencies(
        &self,
        dependencies: &[PubGrubDependency],
        state: &ForkState,
        request_sink: &Sender<Request>,
    ) -> Result<(), ResolveError> {
        for dependency in dependencies {
            let PubGrubDependency {
                package,
                version: _,
                parent: _,
                url: _,
            } = dependency;
            let url = package.name().and_then(|name| state.fork_urls.get(name));
            let index = package.name().and_then(|name| state.fork_indexes.get(name));
            self.visit_package(package, url, index, request_sink)?;
        }
        Ok(())
    }

    /// Visit a [`PubGrubPackage`] prior to selection. This should be called on a [`PubGrubPackage`]
    /// before it is selected, to allow metadata to be fetched in parallel.
    fn visit_package(
        &self,
        package: &PubGrubPackage,
        url: Option<&VerbatimParsedUrl>,
        index: Option<&IndexMetadata>,
        request_sink: &Sender<Request>,
    ) -> Result<(), ResolveError> {
        // Ignore unresolved URL packages, i.e., packages that use a direct URL in some forks.
        if url.is_none()
            && package
                .name()
                .map(|name| self.urls.any_url(name))
                .unwrap_or(true)
        {
            return Ok(());
        }

        self.request_package(package, url, index, request_sink)
    }

    fn request_package(
        &self,
        package: &PubGrubPackage,
        url: Option<&VerbatimParsedUrl>,
        index: Option<&IndexMetadata>,
        request_sink: &Sender<Request>,
    ) -> Result<(), ResolveError> {
        // Only request real packages.
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
        } else if let Some(index) = index {
            // Emit a request to fetch the metadata for this package on the index.
            if self
                .index
                .explicit()
                .register((name.clone(), index.url().clone()))
            {
                request_sink.blocking_send(Request::Package(name.clone(), Some(index.clone())))?;
            }
        } else {
            // Emit a request to fetch the metadata for this package.
            if self.index.implicit().register(name.clone()) {
                request_sink.blocking_send(Request::Package(name.clone(), None))?;
            }
        }
        Ok(())
    }

    /// Visit the set of [`PubGrubPackage`] candidates prior to selection. This allows us to fetch
    /// metadata for all packages in parallel.
    fn pre_visit<'data>(
        packages: impl Iterator<Item = (&'data PubGrubPackage, &'data Range<Version>)>,
        urls: &Urls,
        indexes: &Indexes,
        python_requirement: &PythonRequirement,
        request_sink: &Sender<Request>,
    ) -> Result<(), ResolveError> {
        // Iterate over the potential packages, and fetch file metadata for any of them. These
        // represent our current best guesses for the versions that we _might_ select.
        for (package, range) in packages {
            let PubGrubPackageInner::Package {
                name,
                extra: None,
                group: None,
                marker: MarkerTree::TRUE,
            } = &**package
            else {
                continue;
            };
            // Avoid pre-visiting packages that have any URLs in any fork. At this point we can't
            // tell whether they are registry distributions or which url they use.
            if urls.any_url(name) {
                continue;
            }
            // Avoid visiting packages that may use an explicit index.
            if indexes.contains_key(name) {
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
    // TODO(konsti): re-enable tracing. This trace is crucial to understanding the
    // tracing-durations-export diagrams, but it took ~5% resolver thread runtime for apache-airflow
    // when I last measured.
    #[cfg_attr(feature = "tracing-durations-export", instrument(skip_all, fields(%package)))]
    fn choose_version(
        &self,
        package: &PubGrubPackage,
        id: Id<PubGrubPackage>,
        index: Option<&IndexUrl>,
        range: &Range<Version>,
        pins: &mut FilePins,
        preferences: &Preferences,
        fork_urls: &ForkUrls,
        env: &ResolverEnvironment,
        python_requirement: &PythonRequirement,
        pubgrub: &State<UvDependencyProvider>,
        visited: &mut FxHashSet<PackageName>,
        request_sink: &Sender<Request>,
    ) -> Result<Option<ResolverVersion>, ResolveError> {
        match &**package {
            PubGrubPackageInner::Root(_) => {
                Ok(Some(ResolverVersion::Unforked(MIN_VERSION.clone())))
            }

            PubGrubPackageInner::Python(_) => {
                // Dependencies on Python are only added when a package is incompatible; as such,
                // we don't need to do anything here.
                Ok(None)
            }

            PubGrubPackageInner::System(_) => {
                // We don't care what the actual version is here, just that it's consistent across
                // the dependency graph.
                let Some(version) = range.as_singleton() else {
                    return Ok(None);
                };
                Ok(Some(ResolverVersion::Unforked(version.clone())))
            }

            PubGrubPackageInner::Marker { name, .. }
            | PubGrubPackageInner::Extra { name, .. }
            | PubGrubPackageInner::Group { name, .. }
            | PubGrubPackageInner::Package { name, .. } => {
                if let Some(url) = package.name().and_then(|name| fork_urls.get(name)) {
                    self.choose_version_url(name, range, url, python_requirement)
                } else {
                    self.choose_version_registry(
                        package,
                        id,
                        name,
                        index,
                        range,
                        preferences,
                        env,
                        python_requirement,
                        pubgrub,
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
            MetadataResponse::Unavailable(reason) => {
                self.unavailable_packages
                    .insert(name.clone(), reason.into());
                return Ok(None);
            }
            // TODO(charlie): Add derivation chain for URL dependencies. In practice, this isn't
            // critical since we fetch URL dependencies _prior_ to invoking the resolver.
            MetadataResponse::Error(dist, err) => {
                return Err(ResolveError::Dist(
                    DistErrorKind::from_requested_dist(dist, &**err),
                    dist.clone(),
                    DerivationChain::default(),
                    err.clone(),
                ));
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

        Ok(Some(ResolverVersion::Unforked(version.clone())))
    }

    /// Given a candidate registry requirement, choose the next version in range to try, or `None`
    /// if there is no version in this range.
    fn choose_version_registry(
        &self,
        package: &PubGrubPackage,
        id: Id<PubGrubPackage>,
        name: &PackageName,
        index: Option<&IndexUrl>,
        range: &Range<Version>,
        preferences: &Preferences,
        env: &ResolverEnvironment,
        python_requirement: &PythonRequirement,
        pubgrub: &State<UvDependencyProvider>,
        pins: &mut FilePins,
        visited: &mut FxHashSet<PackageName>,
        request_sink: &Sender<Request>,
    ) -> Result<Option<ResolverVersion>, ResolveError> {
        // Wait for the metadata to be available.
        let versions_response = if let Some(index) = index {
            self.index
                .explicit()
                .wait_blocking(&(name.clone(), index.clone()))
                .ok_or_else(|| ResolveError::UnregisteredTask(name.to_string()))?
        } else {
            self.index
                .implicit()
                .wait_blocking(name)
                .ok_or_else(|| ResolveError::UnregisteredTask(name.to_string()))?
        };
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
            index,
            env,
            self.tags.as_ref(),
        ) else {
            // Short circuit: we couldn't find _any_ versions for a package.
            return Ok(None);
        };

        // TODO(konsti): Can we make this an option so we don't pay any allocations?
        let mut variant_prioritized_dist_binding = PrioritizedDist::default();
        let candidate = self.variant_candidate(
            candidate,
            env,
            request_sink,
            &mut variant_prioritized_dist_binding,
        )?;

        let dist = match candidate.dist() {
            CandidateDist::Compatible(dist) => dist,
            CandidateDist::Incompatible {
                incompatible_dist: incompatibility,
                prioritized_dist: _,
            } => {
                // If the version is incompatible because no distributions are compatible, exit early.
                return Ok(Some(ResolverVersion::Unavailable(
                    candidate.version().clone(),
                    // TODO(charlie): We can avoid this clone; the candidate is dropped here and
                    // owns the incompatibility.
                    UnavailableVersion::IncompatibleDist(incompatibility.clone()),
                )));
            }
        };

        // Check whether the version is incompatible due to its Python requirement.
        if let Some((requires_python, incompatibility)) =
            Self::check_requires_python(dist, python_requirement)
        {
            if matches!(self.options.fork_strategy, ForkStrategy::RequiresPython) {
                if env.marker_environment().is_none() {
                    let forks = fork_version_by_python_requirement(
                        requires_python,
                        python_requirement,
                        env,
                    );
                    if !forks.is_empty() {
                        debug!(
                            "Forking Python requirement `{}` on `{}` for {}=={} ({})",
                            python_requirement.target(),
                            requires_python,
                            name,
                            candidate.version(),
                            forks
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                        let forks = forks
                            .into_iter()
                            .map(|env| VersionFork {
                                env,
                                id,
                                version: None,
                            })
                            .collect();
                        return Ok(Some(ResolverVersion::Forked(forks)));
                    }
                }
            }

            return Ok(Some(ResolverVersion::Unavailable(
                candidate.version().clone(),
                UnavailableVersion::IncompatibleDist(incompatibility),
            )));
        }

        // Check whether this version covers all supported platforms; and, if not, generate a fork.
        if let Some(forked) = self.fork_version_registry(
            &candidate,
            dist,
            version_maps,
            package,
            id,
            name,
            index,
            range,
            preferences,
            env,
            pubgrub,
            pins,
            request_sink,
        )? {
            return Ok(Some(forked));
        }

        let filename = match dist.for_installation() {
            ResolvedDistRef::InstallableRegistrySourceDist { sdist, .. } => sdist
                .filename()
                .unwrap_or(Cow::Borrowed("unknown filename")),
            ResolvedDistRef::InstallableRegistryBuiltDist { wheel, .. } => wheel
                .filename()
                .unwrap_or(Cow::Borrowed("unknown filename")),
            ResolvedDistRef::Installed { .. } => Cow::Borrowed("installed"),
        };

        debug!(
            "Selecting: {}=={} [{}] ({})",
            name,
            candidate.version(),
            candidate.choice_kind(),
            filename,
        );
        self.visit_candidate(&candidate, dist, package, name, pins, request_sink)?;

        let version = candidate.version().clone();
        Ok(Some(ResolverVersion::Unforked(version)))
    }

    fn variant_candidate<'prioritized>(
        &self,
        candidate: Candidate<'prioritized>,
        env: &ResolverEnvironment,
        request_sink: &Sender<Request>,
        variant_prioritized_dist_binding: &'prioritized mut PrioritizedDist,
    ) -> Result<Candidate<'prioritized>, ResolveError> {
        let candidate = if env.marker_environment().is_some() {
            // When solving for a specific environment, check if there is a matching variant wheel
            // for the current environment.
            // TODO(konsti): When solving for an environment that is not the current host, don't
            // consider variants unless a static variant is given.
            let Some(prioritized_dist) = candidate.prioritized() else {
                return Ok(candidate);
            };

            // No `variants.json`, no variants.
            // TODO(konsti): Be more lenient, e.g. parse the wheel itself?
            let Some(variants_json) = prioritized_dist.variants_json() else {
                return Ok(candidate);
            };

            // If the distribution is not indexed, we can't resolve variants.
            let Some(index) = prioritized_dist.index() else {
                return Ok(candidate);
            };

            // Query the host for the applicable features and properties.
            let version_id = GlobalVersionId::new(
                VersionId::NameVersion(candidate.name().clone(), candidate.version().clone()),
                index.clone(),
            );
            if self.index.variant_priorities().register(version_id.clone()) {
                request_sink
                    .blocking_send(Request::Variants(version_id.clone(), variants_json.clone()))?;
            }

            let resolved_variants = self.index.variant_priorities().wait_blocking(&version_id);
            let Some(resolved_variants) = &resolved_variants else {
                panic!("We have variants, why didn't they resolve?");
            };

            let Some(variant_prioritized_dist) =
                prioritized_dist.prioritize_best_variant_wheel(resolved_variants)
            else {
                return Ok(candidate);
            };

            *variant_prioritized_dist_binding = variant_prioritized_dist;
            candidate.prioritize_best_variant_wheel(variant_prioritized_dist_binding)
        } else {
            // In universal mode, a variant wheel with an otherwise compatible tag is acceptable.
            candidate.allow_variant_wheels()
        };
        Ok(candidate)
    }

    /// Determine whether a candidate covers all supported platforms; and, if not, generate a fork.
    ///
    /// This only ever applies to versions that lack source distributions And, for now, we only
    /// apply it in two cases:
    ///
    /// 1. Local versions, where the non-local version has greater platform coverage. The intent is
    ///    such that, if we're resolving PyTorch, and we choose `torch==2.5.2+cpu`, we want to
    ///    fork so that we can select `torch==2.5.2` on macOS (since the `+cpu` variant doesn't
    ///    include any macOS wheels).
    /// 2. Platforms that the user explicitly marks as "required" (opt-in). For example, the user
    ///    might require that the generated resolution always includes wheels for x86 macOS, and
    ///    fails entirely if the platform is unsupported.
    fn fork_version_registry(
        &self,
        candidate: &Candidate,
        dist: &CompatibleDist,
        version_maps: &[VersionMap],
        package: &PubGrubPackage,
        id: Id<PubGrubPackage>,
        name: &PackageName,
        index: Option<&IndexUrl>,
        range: &Range<Version>,
        preferences: &Preferences,
        env: &ResolverEnvironment,
        pubgrub: &State<UvDependencyProvider>,
        pins: &mut FilePins,
        request_sink: &Sender<Request>,
    ) -> Result<Option<ResolverVersion>, ResolveError> {
        // This only applies to universal resolutions.
        if env.marker_environment().is_some() {
            return Ok(None);
        }

        // If the package is already compatible with all environments (as is the case for
        // packages that include a source distribution), we don't need to fork.
        if dist.implied_markers().is_true() {
            return Ok(None);
        }

        let variant_base = candidate.package_id().to_string();

        // If the user explicitly marked a platform as required, ensure it has coverage.
        for marker in self.options.required_environments.iter().copied() {
            // If the platform is part of the current environment...
            if env.included_by_marker(marker) {
                // But isn't supported by the distribution...
                if dist.implied_markers().is_disjoint(marker)
                    && !find_environments(id, pubgrub, &variant_base).is_disjoint(marker)
                {
                    // Then we need to fork.
                    let Some((left, right)) = fork_version_by_marker(env, marker) else {
                        return Ok(Some(ResolverVersion::Unavailable(
                            candidate.version().clone(),
                            UnavailableVersion::IncompatibleDist(IncompatibleDist::Wheel(
                                IncompatibleWheel::MissingPlatform(marker),
                            )),
                        )));
                    };

                    debug!(
                        "Forking on required platform `{}` for {}=={} ({})",
                        marker.try_to_string().unwrap_or_else(|| "true".to_string()),
                        name,
                        candidate.version(),
                        [&left, &right]
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    let forks = vec![
                        VersionFork {
                            env: left,
                            id,
                            version: None,
                        },
                        VersionFork {
                            env: right,
                            id,
                            version: None,
                        },
                    ];
                    return Ok(Some(ResolverVersion::Forked(forks)));
                }
            }
        }

        // For now, we only apply this to local versions.
        if !candidate.version().is_local() {
            return Ok(None);
        }

        debug!(
            "Looking at local version: {}=={}",
            name,
            candidate.version()
        );

        // If there's a non-local version...
        let range = range.clone().intersection(&Range::singleton(
            candidate.version().clone().without_local(),
        ));

        let Some(base_candidate) = self.selector.select(
            name,
            &range,
            version_maps,
            preferences,
            &self.installed_packages,
            &self.exclusions,
            index,
            env,
            self.tags.as_ref(),
        ) else {
            return Ok(None);
        };

        let CandidateDist::Compatible(base_dist) = base_candidate.dist() else {
            return Ok(None);
        };

        // ...and the non-local version has greater platform support...
        let mut remainder = {
            let mut remainder = base_dist.implied_markers();
            remainder.and(dist.implied_markers().negate());
            remainder
        };
        if remainder.is_false() {
            return Ok(None);
        }

        // If the remainder isn't relevant to the current environment, there's no need to fork.
        // For example, if we're solving for `sys_platform == 'darwin'` but the remainder is
        // `sys_platform == 'linux'`, we don't need to fork.
        if !env.included_by_marker(remainder) {
            return Ok(None);
        }

        // Similarly, if the local distribution is incompatible with the current environment, then
        // use the base distribution instead (but don't fork).
        if !env.included_by_marker(dist.implied_markers()) {
            let filename = match dist.for_installation() {
                ResolvedDistRef::InstallableRegistrySourceDist { sdist, .. } => sdist
                    .filename()
                    .unwrap_or(Cow::Borrowed("unknown filename")),
                ResolvedDistRef::InstallableRegistryBuiltDist { wheel, .. } => wheel
                    .filename()
                    .unwrap_or(Cow::Borrowed("unknown filename")),
                ResolvedDistRef::Installed { .. } => Cow::Borrowed("installed"),
            };

            debug!(
                "Preferring non-local candidate: {}=={} [{}] ({})",
                name,
                base_candidate.version(),
                base_candidate.choice_kind(),
                filename,
            );
            self.visit_candidate(
                &base_candidate,
                base_dist,
                package,
                name,
                pins,
                request_sink,
            )?;

            return Ok(Some(ResolverVersion::Unforked(
                base_candidate.version().clone(),
            )));
        }

        // If the implied markers includes _some_ macOS environments, but the remainder doesn't,
        // then we can extend the implied markers to include _all_ macOS environments. Same goes for
        // Linux and Windows.
        //
        // The idea here is that the base version could support (e.g.) ARM macOS, but not Intel
        // macOS. But if _neither_ version supports Intel macOS, we'd rather use `sys_platform == 'darwin'`
        // instead of `sys_platform == 'darwin' and platform_machine == 'arm64'`, since it's much
        // simpler, and _neither_ version will succeed with Intel macOS anyway.
        for value in [
            arcstr::literal!("darwin"),
            arcstr::literal!("linux"),
            arcstr::literal!("win32"),
        ] {
            let sys_platform = MarkerTree::expression(MarkerExpression::String {
                key: MarkerValueString::SysPlatform,
                operator: MarkerOperator::Equal,
                value,
            });
            if dist.implied_markers().is_disjoint(sys_platform)
                && !remainder.is_disjoint(sys_platform)
            {
                remainder.or(sys_platform);
            }
        }

        // Otherwise, we need to fork.
        let Some((base_env, local_env)) = fork_version_by_marker(env, remainder) else {
            return Ok(None);
        };

        debug!(
            "Forking platform for {}=={} ({})",
            name,
            candidate.version(),
            [&base_env, &local_env]
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );
        self.visit_candidate(candidate, dist, package, name, pins, request_sink)?;
        self.visit_candidate(
            &base_candidate,
            base_dist,
            package,
            name,
            pins,
            request_sink,
        )?;

        let forks = vec![
            VersionFork {
                env: base_env.clone(),
                id,
                version: Some(base_candidate.version().clone()),
            },
            VersionFork {
                env: local_env.clone(),
                id,
                version: Some(candidate.version().clone()),
            },
        ];
        Ok(Some(ResolverVersion::Forked(forks)))
    }

    /// Visit a selected candidate.
    fn visit_candidate(
        &self,
        candidate: &Candidate,
        dist: &CompatibleDist,
        package: &PubGrubPackage,
        name: &PackageName,
        pins: &mut FilePins,
        request_sink: &Sender<Request>,
    ) -> Result<(), ResolveError> {
        // We want to return a package pinned to a specific version; but we _also_ want to
        // store the exact file that we selected to satisfy that version.
        pins.insert(candidate, dist);

        // Emit a request to fetch the metadata for this version.
        if matches!(&**package, PubGrubPackageInner::Package { .. }) {
            if self.dependency_mode.is_transitive() {
                if self.index.distributions().register(candidate.version_id()) {
                    if name != dist.name() {
                        return Err(ResolveError::MismatchedPackageName {
                            request: "distribution",
                            expected: name.clone(),
                            actual: dist.name().clone(),
                        });
                    }
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

        Ok(())
    }

    /// Check if the distribution is incompatible with the Python requirement, and if so, return
    /// the incompatibility.
    fn check_requires_python<'dist>(
        dist: &'dist CompatibleDist,
        python_requirement: &PythonRequirement,
    ) -> Option<(&'dist VersionSpecifiers, IncompatibleDist)> {
        let requires_python = dist.requires_python()?;
        if python_requirement.target().is_contained_by(requires_python) {
            None
        } else {
            let incompatibility = if matches!(dist, CompatibleDist::CompatibleWheel { .. }) {
                IncompatibleDist::Wheel(IncompatibleWheel::RequiresPython(
                    requires_python.clone(),
                    if python_requirement.installed() == python_requirement.target() {
                        PythonRequirementKind::Installed
                    } else {
                        PythonRequirementKind::Target
                    },
                ))
            } else {
                IncompatibleDist::Source(IncompatibleSource::RequiresPython(
                    requires_python.clone(),
                    if python_requirement.installed() == python_requirement.target() {
                        PythonRequirementKind::Installed
                    } else {
                        PythonRequirementKind::Target
                    },
                ))
            };
            Some((requires_python, incompatibility))
        }
    }

    /// Given a candidate package and version, return its dependencies.
    #[instrument(skip_all, fields(%package, %version))]
    fn get_dependencies_forking(
        &self,
        id: Id<PubGrubPackage>,
        package: &PubGrubPackage,
        version: &Version,
        pins: &FilePins,
        fork_urls: &ForkUrls,
        env: &ResolverEnvironment,
        in_memory_index: &InMemoryIndex,
        python_requirement: &PythonRequirement,
        pubgrub: &State<UvDependencyProvider>,
    ) -> Result<ForkedDependencies, ResolveError> {
        let result = self.get_dependencies(
            id,
            package,
            version,
            pins,
            fork_urls,
            env,
            in_memory_index,
            python_requirement,
            pubgrub,
        );
        if env.marker_environment().is_some() {
            result.map(|deps| match deps {
                Dependencies::Available(deps) | Dependencies::Unforkable(deps) => {
                    ForkedDependencies::Unforked(deps)
                }
                Dependencies::Unavailable(err) => ForkedDependencies::Unavailable(err),
            })
        } else {
            // Grab the pinned distribution for the given name and version.
            let variant_base = package.name().map(|name| format!("{name}=={version}"));
            Ok(result?.fork(
                env,
                python_requirement,
                &self.conflicts,
                variant_base.as_deref(),
            ))
        }
    }

    /// Given a candidate package and version, return its dependencies.
    #[instrument(skip_all, fields(%package, %version))]
    fn get_dependencies(
        &self,
        id: Id<PubGrubPackage>,
        package: &PubGrubPackage,
        version: &Version,
        pins: &FilePins,
        fork_urls: &ForkUrls,
        env: &ResolverEnvironment,
        in_memory_index: &InMemoryIndex,
        python_requirement: &PythonRequirement,
        pubgrub: &State<UvDependencyProvider>,
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
                    env,
                    &MarkerVariantsUniversal,
                    python_requirement,
                );

                requirements
                    .flat_map(move |requirement| {
                        PubGrubDependency::from_requirement(
                            &self.conflicts,
                            requirement,
                            None,
                            Some(package),
                        )
                    })
                    .collect()
            }

            PubGrubPackageInner::Package {
                name,
                extra,
                group,
                marker: _,
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

                // If we're resolving for a specific environment, use the host variants, otherwise resolve
                // for all variants.
                let variant = Self::variant_properties(name, version, pins, env, in_memory_index);

                // If the package does not exist in the registry or locally, we cannot fetch its dependencies
                if self.dependency_mode.is_transitive()
                    && self.unavailable_packages.get(name).is_some()
                    && self.installed_packages.get_packages(name).is_empty()
                {
                    debug_assert!(
                        false,
                        "Dependencies were requested for a package that is not available"
                    );
                    return Err(ResolveError::PackageUnavailable(name.clone()));
                }

                // Wait for the metadata to be available.
                let response = self
                    .index
                    .distributions()
                    .wait_blocking(&version_id)
                    .ok_or_else(|| ResolveError::UnregisteredTask(version_id.to_string()))?;

                let metadata = match &*response {
                    MetadataResponse::Found(archive) => &archive.metadata,
                    MetadataResponse::Unavailable(reason) => {
                        let unavailable_version = UnavailableVersion::from(reason);
                        let message = unavailable_version.singular_message();
                        if let Some(err) = reason.source() {
                            // Show the detailed error for metadata parse errors.
                            warn!("{name} {message}: {err}");
                        } else {
                            warn!("{name} {message}");
                        }
                        self.incomplete_packages
                            .entry(name.clone())
                            .or_default()
                            .insert(version.clone(), reason.clone());
                        return Ok(Dependencies::Unavailable(unavailable_version));
                    }
                    MetadataResponse::Error(dist, err) => {
                        let chain = DerivationChainBuilder::from_state(id, version, pubgrub)
                            .unwrap_or_default();
                        return Err(ResolveError::Dist(
                            DistErrorKind::from_requested_dist(dist, &**err),
                            dist.clone(),
                            chain,
                            err.clone(),
                        ));
                    }
                };

                // If there was no requires-python on the index page, we may have an incompatible
                // distribution.
                if let Some(requires_python) = &metadata.requires_python {
                    if !python_requirement.target().is_contained_by(requires_python) {
                        return Ok(Dependencies::Unavailable(
                            UnavailableVersion::RequiresPython(requires_python.clone()),
                        ));
                    }
                }

                // Identify any system dependencies based on the index URL.
                let system_dependencies = self
                    .options
                    .torch_backend
                    .as_ref()
                    .filter(|torch_backend| matches!(torch_backend, TorchStrategy::Cuda { .. }))
                    .filter(|torch_backend| torch_backend.has_system_dependency(name))
                    .and_then(|_| pins.get(name, version).and_then(ResolvedDist::index))
                    .map(IndexUrl::url)
                    .and_then(SystemDependency::from_index)
                    .into_iter()
                    .inspect(|system_dependency| {
                        debug!(
                            "Adding system dependency `{}` for `{package}@{version}`",
                            system_dependency
                        );
                    })
                    .map(PubGrubDependency::from);

                let requirements = self.flatten_requirements(
                    &metadata.requires_dist,
                    &metadata.dependency_groups,
                    extra.as_ref(),
                    group.as_ref(),
                    Some(name),
                    env,
                    &variant,
                    python_requirement,
                );

                requirements
                    .flat_map(|requirement| {
                        PubGrubDependency::from_requirement(
                            &self.conflicts,
                            requirement,
                            group.as_ref(),
                            Some(package),
                        )
                    })
                    .chain(system_dependencies)
                    .collect()
            }

            PubGrubPackageInner::Python(_) => return Ok(Dependencies::Unforkable(Vec::default())),

            PubGrubPackageInner::System(_) => return Ok(Dependencies::Unforkable(Vec::default())),

            // Add a dependency on both the marker and base package.
            PubGrubPackageInner::Marker { name, marker } => {
                return Ok(Dependencies::Unforkable(
                    [MarkerTree::TRUE, *marker]
                        .into_iter()
                        .map(move |marker| PubGrubDependency {
                            package: PubGrubPackage::from(PubGrubPackageInner::Package {
                                name: name.clone(),
                                extra: None,
                                group: None,
                                marker,
                            }),
                            version: Range::singleton(version.clone()),
                            parent: None,
                            url: None,
                        })
                        .collect(),
                ));
            }

            // Add a dependency on both the extra and base package, with and without the marker.
            PubGrubPackageInner::Extra {
                name,
                extra,
                marker,
            } => {
                return Ok(Dependencies::Unforkable(
                    [MarkerTree::TRUE, *marker]
                        .into_iter()
                        .dedup()
                        .flat_map(move |marker| {
                            [None, Some(extra)]
                                .into_iter()
                                .map(move |extra| PubGrubDependency {
                                    package: PubGrubPackage::from(PubGrubPackageInner::Package {
                                        name: name.clone(),
                                        extra: extra.cloned(),
                                        group: None,
                                        marker,
                                    }),
                                    version: Range::singleton(version.clone()),
                                    parent: None,
                                    url: None,
                                })
                        })
                        .collect(),
                ));
            }

            // Add a dependency on the dependency group, with and without the marker.
            PubGrubPackageInner::Group {
                name,
                group,
                marker,
            } => {
                return Ok(Dependencies::Unforkable(
                    [MarkerTree::TRUE, *marker]
                        .into_iter()
                        .dedup()
                        .map(|marker| PubGrubDependency {
                            package: PubGrubPackage::from(PubGrubPackageInner::Package {
                                name: name.clone(),
                                extra: None,
                                group: Some(group.clone()),
                                marker,
                            }),
                            version: Range::singleton(version.clone()),
                            parent: None,
                            url: None,
                        })
                        .collect(),
                ));
            }
        };
        Ok(Dependencies::Available(dependencies))
    }

    fn variant_properties(
        name: &PackageName,
        version: &Version,
        pins: &FilePins,
        env: &ResolverEnvironment,
        in_memory_index: &InMemoryIndex,
    ) -> Variant {
        // TODO(konsti): Perf/Caching with version selection: This is in the hot path!

        if env.marker_environment().is_none() {
            return Variant::default();
        }

        // Grab the pinned distribution for the given name and version.
        let Some(dist) = pins.get(name, version) else {
            return Variant::default();
        };

        let Some(filename) = dist.wheel_filename() else {
            // TODO(konsti): Handle installed dists too
            return Variant::default();
        };

        let Some(variant_label) = filename.variant() else {
            return Variant::default();
        };

        let Some(index) = dist.index() else {
            warn!("Wheel variant has no index: {filename}");
            return Variant::default();
        };

        let version_id = GlobalVersionId::new(
            VersionId::NameVersion(name.clone(), version.clone()),
            index.clone(),
        );

        let Some(resolved_variants) = in_memory_index.variant_priorities().get(&version_id) else {
            return Variant::default();
        };

        // Collect the host properties for marker filtering.
        // TODO(konsti): We shouldn't need to clone
        let variant = resolved_variants
            .variants_json
            .variants
            .get(variant_label)
            .expect("Missing previously select variant label");
        variant.clone()
    }

    /// The regular and dev dependencies filtered by Python version and the markers of this fork,
    /// plus the extras dependencies of the current package (e.g., `black` depending on
    /// `black[colorama]`).
    fn flatten_requirements<'a>(
        &'a self,
        dependencies: &'a [Requirement],
        dev_dependencies: &'a BTreeMap<GroupName, Box<[Requirement]>>,
        extra: Option<&'a ExtraName>,
        dev: Option<&'a GroupName>,
        name: Option<&PackageName>,
        env: &'a ResolverEnvironment,
        variants: &'a impl MarkerVariantsEnvironment,
        python_requirement: &'a PythonRequirement,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> {
        let python_marker = python_requirement.to_marker_tree();

        if let Some(dev) = dev {
            // Dependency groups can include the project itself, so no need to flatten recursive
            // dependencies.
            Either::Left(Either::Left(self.requirements_for_extra(
                dev_dependencies.get(dev).into_iter().flatten(),
                extra,
                env,
                variants,
                python_marker,
                python_requirement,
            )))
        } else if !dependencies
            .iter()
            .any(|req| name == Some(&req.name) && !req.extras.is_empty())
        {
            // If the project doesn't define any recursive dependencies, take the fast path.
            Either::Left(Either::Right(self.requirements_for_extra(
                dependencies.iter(),
                extra,
                env,
                variants,
                python_marker,
                python_requirement,
            )))
        } else {
            let mut requirements = self
                .requirements_for_extra(
                    dependencies.iter(),
                    extra,
                    env,
                    variants,
                    python_marker,
                    python_requirement,
                )
                .collect::<Vec<_>>();

            // Transitively process all extras that are recursively included, starting with the current
            // extra.
            let mut seen = FxHashSet::<(ExtraName, MarkerTree)>::default();
            let mut queue: VecDeque<_> = requirements
                .iter()
                .filter(|req| name == Some(&req.name))
                .flat_map(|req| req.extras.iter().cloned().map(|extra| (extra, req.marker)))
                .collect();
            while let Some((extra, marker)) = queue.pop_front() {
                if !seen.insert((extra.clone(), marker)) {
                    continue;
                }
                for requirement in self.requirements_for_extra(
                    dependencies,
                    Some(&extra),
                    env,
                    &variants,
                    python_marker,
                    python_requirement,
                ) {
                    let requirement = match requirement {
                        Cow::Owned(mut requirement) => {
                            requirement.marker.and(marker);
                            requirement
                        }
                        Cow::Borrowed(requirement) => {
                            let mut marker = marker;
                            marker.and(requirement.marker);
                            Requirement {
                                name: requirement.name.clone(),
                                extras: requirement.extras.clone(),
                                groups: requirement.groups.clone(),
                                source: requirement.source.clone(),
                                origin: requirement.origin.clone(),
                                marker: marker.simplify_extras(slice::from_ref(&extra)),
                            }
                        }
                    };
                    if name == Some(&requirement.name) {
                        // Add each transitively included extra.
                        queue.extend(
                            requirement
                                .extras
                                .iter()
                                .cloned()
                                .map(|extra| (extra, requirement.marker)),
                        );
                    } else {
                        // Add the requirements for that extra.
                        requirements.push(Cow::Owned(requirement));
                    }
                }
            }

            // Retain any self-constraints for that extra, e.g., if `project[foo]` includes
            // `project[bar]>1.0`, as a dependency, we need to propagate `project>1.0`, in addition to
            // transitively expanding `project[bar]`.
            let mut self_constraints = vec![];
            for req in &requirements {
                if name == Some(&req.name) && !req.source.is_empty() {
                    self_constraints.push(Requirement {
                        name: req.name.clone(),
                        extras: Box::new([]),
                        groups: req.groups.clone(),
                        source: req.source.clone(),
                        origin: req.origin.clone(),
                        marker: req.marker,
                    });
                }
            }

            // Drop all the self-requirements now that we flattened them out.
            requirements.retain(|req| name != Some(&req.name) || req.extras.is_empty());
            requirements.extend(self_constraints.into_iter().map(Cow::Owned));

            Either::Right(requirements.into_iter())
        }
    }

    /// The set of the regular and dev dependencies, filtered by Python version,
    /// the markers of this fork and the requested extra.
    fn requirements_for_extra<'data, 'parameters>(
        &'data self,
        dependencies: impl IntoIterator<Item = &'data Requirement> + 'parameters,
        extra: Option<&'parameters ExtraName>,
        env: &'parameters ResolverEnvironment,
        variants: &'parameters impl MarkerVariantsEnvironment,
        python_marker: MarkerTree,
        python_requirement: &'parameters PythonRequirement,
    ) -> impl Iterator<Item = Cow<'data, Requirement>> + 'parameters
    where
        'data: 'parameters,
    {
        self.overrides
            .apply(dependencies)
            .filter(move |requirement| {
                Self::is_requirement_applicable(
                    requirement,
                    extra,
                    env,
                    variants,
                    python_marker,
                    python_requirement,
                )
            })
            .flat_map(move |requirement| {
                iter::once(requirement.clone()).chain(self.constraints_for_requirement(
                    requirement,
                    extra,
                    env,
                    variants,
                    python_marker,
                    python_requirement,
                ))
            })
    }

    /// Whether a requirement is applicable for the Python version, the markers of this fork, the
    /// host variants if applicable and the requested extra.
    fn is_requirement_applicable(
        requirement: &Requirement,
        extra: Option<&ExtraName>,
        env: &ResolverEnvironment,
        variants: impl MarkerVariantsEnvironment,
        python_marker: MarkerTree,
        python_requirement: &PythonRequirement,
    ) -> bool {
        // If the requirement isn't relevant for the current platform, skip it.
        match extra {
            Some(source_extra) => {
                // Only include requirements that are relevant for the current extra.
                if requirement.evaluate_markers(env.marker_environment(), &variants, &[]) {
                    return false;
                }
                if !requirement.evaluate_markers(
                    env.marker_environment(),
                    &variants,
                    slice::from_ref(source_extra),
                ) {
                    return false;
                }
                if !env.included_by_group(ConflictItemRef::from((&requirement.name, source_extra)))
                {
                    return false;
                }
            }
            None => {
                if !requirement.evaluate_markers(env.marker_environment(), variants, &[]) {
                    return false;
                }
            }
        }

        // If the requirement would not be selected with any Python version
        // supported by the root, skip it.
        if python_marker.is_disjoint(requirement.marker) {
            trace!(
                "Skipping {requirement} because of Requires-Python: {requires_python}",
                requires_python = python_requirement.target(),
            );
            return false;
        }

        // If we're in a fork in universal mode, ignore any dependency that isn't part of
        // this fork (but will be part of another fork).
        if !env.included_by_marker(requirement.marker) {
            trace!("Skipping {requirement} because of {env}");
            return false;
        }

        true
    }

    /// The constraints applicable to the requirement, filtered by Python version, the markers of
    /// this fork and the requested extra.
    fn constraints_for_requirement<'data, 'parameters>(
        &'data self,
        requirement: Cow<'data, Requirement>,
        extra: Option<&'parameters ExtraName>,
        env: &'parameters ResolverEnvironment,
        variants: impl MarkerVariantsEnvironment + 'parameters,
        python_marker: MarkerTree,
        python_requirement: &'parameters PythonRequirement,
    ) -> impl Iterator<Item = Cow<'data, Requirement>> + 'parameters
    where
        'data: 'parameters,
    {
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
                        let mut marker = constraint.marker;
                        marker.and(requirement.marker);

                        if marker.is_false() {
                            trace!(
                                "Skipping {constraint} because of disjoint markers: `{}` vs. `{}`",
                                constraint.marker.try_to_string().unwrap(),
                                requirement.marker.try_to_string().unwrap(),
                            );
                            return None;
                        }

                        Cow::Owned(Requirement {
                            name: constraint.name.clone(),
                            extras: constraint.extras.clone(),
                            groups: constraint.groups.clone(),
                            source: constraint.source.clone(),
                            origin: constraint.origin.clone(),
                            marker,
                        })
                    }
                } else {
                    let requires_python = python_requirement.target();

                    let mut marker = constraint.marker;
                    marker.and(requirement.marker);

                    if marker.is_false() {
                        trace!(
                            "Skipping {constraint} because of disjoint markers: `{}` vs. `{}`",
                            constraint.marker.try_to_string().unwrap(),
                            requirement.marker.try_to_string().unwrap(),
                        );
                        return None;
                    }

                    // Additionally, if the requirement is `requests ; sys_platform == 'darwin'`
                    // and the constraint is `requests ; python_version == '3.6'`, the
                    // constraint should only apply when _both_ markers are true.
                    if python_marker.is_disjoint(marker) {
                        trace!(
                            "Skipping constraint {requirement} \
                            because of Requires-Python: {requires_python}"
                        );
                        return None;
                    }

                    if marker == constraint.marker {
                        Cow::Borrowed(constraint)
                    } else {
                        Cow::Owned(Requirement {
                            name: constraint.name.clone(),
                            extras: constraint.extras.clone(),
                            groups: constraint.groups.clone(),
                            source: constraint.source.clone(),
                            origin: constraint.origin.clone(),
                            marker,
                        })
                    }
                };

                // If we're in a fork in universal mode, ignore any dependency that isn't part of
                // this fork (but will be part of another fork).
                if !env.included_by_marker(constraint.marker) {
                    trace!("Skipping {constraint} because of {env}");
                    return None;
                }

                // If the constraint isn't relevant for the current platform, skip it.
                match extra {
                    Some(source_extra) => {
                        if !constraint.evaluate_markers(
                            env.marker_environment(),
                            &variants,
                            slice::from_ref(source_extra),
                        ) {
                            return None;
                        }
                        if !env.included_by_group(ConflictItemRef::from((
                            &requirement.name,
                            source_extra,
                        ))) {
                            return None;
                        }
                    }
                    None => {
                        if !constraint.evaluate_markers(env.marker_environment(), &variants, &[]) {
                            return None;
                        }
                    }
                }

                Some(constraint)
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
                Some(Response::Package(name, index, version_map)) => {
                    trace!("Received package metadata for: {name}");
                    if let Some(index) = index {
                        self.index
                            .explicit()
                            .done((name, index), Arc::new(version_map));
                    } else {
                        self.index.implicit().done(name, Arc::new(version_map));
                    }
                }
                Some(Response::Installed { dist, metadata }) => {
                    trace!("Received installed distribution metadata for: {dist}");
                    self.index
                        .distributions()
                        .done(dist.version_id(), Arc::new(metadata));
                }
                Some(Response::Dist { dist, metadata }) => {
                    let dist_kind = match dist {
                        Dist::Built(_) => "built",
                        Dist::Source(_) => "source",
                    };
                    trace!("Received {dist_kind} distribution metadata for: {dist}");
                    if let MetadataResponse::Unavailable(reason) = &metadata {
                        let message = UnavailableVersion::from(reason).singular_message();
                        if let Some(err) = reason.source() {
                            // Show the detailed error for metadata parse errors.
                            warn!("{dist} {message}: {err}");
                        } else {
                            warn!("{dist} {message}");
                        }
                    }
                    self.index
                        .distributions()
                        .done(dist.version_id(), Arc::new(metadata));
                }
                Some(Response::Variants {
                    version_id,
                    resolved_variants,
                }) => {
                    trace!("Received variant metadata for: {version_id}");
                    self.index
                        .variant_priorities()
                        .done(version_id, Arc::new(resolved_variants));
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
            Request::Package(package_name, index) => {
                let package_versions = provider
                    .get_package_versions(&package_name, index.as_ref())
                    .boxed_local()
                    .await
                    .map_err(ResolveError::Client)?;

                Ok(Some(Response::Package(
                    package_name,
                    index.map(IndexMetadata::into_url),
                    package_versions,
                )))
            }

            // Fetch distribution metadata from the distribution database.
            Request::Dist(dist) => {
                if let Some(version) = dist.version() {
                    if let Some(index) = dist.index() {
                        // Check the implicit indexes for pre-provided metadata.
                        let versions_response = self.index.implicit().get(dist.name());
                        if let Some(VersionsResponse::Found(version_maps)) =
                            versions_response.as_deref()
                        {
                            for version_map in version_maps {
                                if version_map.index() == Some(index) {
                                    let Some(metadata) = version_map.get_metadata(version) else {
                                        continue;
                                    };
                                    debug!("Found registry-provided metadata for: {dist}");
                                    return Ok(Some(Response::Dist {
                                        dist,
                                        metadata: MetadataResponse::Found(
                                            ArchiveMetadata::from_metadata23(metadata.clone()),
                                        ),
                                    }));
                                }
                            }
                        }

                        // Check the explicit indexes for pre-provided metadata.
                        let versions_response = self
                            .index
                            .explicit()
                            .get(&(dist.name().clone(), index.clone()));
                        if let Some(VersionsResponse::Found(version_maps)) =
                            versions_response.as_deref()
                        {
                            for version_map in version_maps {
                                let Some(metadata) = version_map.get_metadata(version) else {
                                    continue;
                                };
                                debug!("Found registry-provided metadata for: {dist}");
                                return Ok(Some(Response::Dist {
                                    dist,
                                    metadata: MetadataResponse::Found(
                                        ArchiveMetadata::from_metadata23(metadata.clone()),
                                    ),
                                }));
                            }
                        }
                    }
                }

                let metadata = provider
                    .get_or_build_wheel_metadata(&dist)
                    .boxed_local()
                    .await?;

                if let MetadataResponse::Found(metadata) = &metadata {
                    if &metadata.metadata.name != dist.name() {
                        return Err(ResolveError::MismatchedPackageName {
                            request: "distribution metadata",
                            expected: dist.name().clone(),
                            actual: metadata.metadata.name.clone(),
                        });
                    }
                }

                Ok(Some(Response::Dist { dist, metadata }))
            }

            Request::Installed(dist) => {
                let metadata = provider.get_installed_metadata(&dist).boxed_local().await?;

                if let MetadataResponse::Found(metadata) = &metadata {
                    if &metadata.metadata.name != dist.name() {
                        return Err(ResolveError::MismatchedPackageName {
                            request: "installed metadata",
                            expected: dist.name().clone(),
                            actual: metadata.metadata.name.clone(),
                        });
                    }
                }

                Ok(Some(Response::Installed { dist, metadata }))
            }

            // Pre-fetch the package and distribution metadata.
            Request::Prefetch(package_name, range, python_requirement) => {
                // Wait for the package metadata to become available.
                let versions_response = self
                    .index
                    .implicit()
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

                // We don't have access to the fork state when prefetching, so assume that
                // pre-release versions are allowed.
                let env = ResolverEnvironment::universal(vec![]);

                // Try to find a compatible version. If there aren't any compatible versions,
                // short-circuit.
                let Some(candidate) = self.selector.select(
                    &package_name,
                    &range,
                    version_map,
                    &self.preferences,
                    &self.installed_packages,
                    &self.exclusions,
                    None,
                    &env,
                    self.tags.as_ref(),
                ) else {
                    return Ok(None);
                };

                // If there is not a compatible distribution, short-circuit.
                // TODO(konsti): Consider prefetching variants instead.
                let Some(dist) = candidate.compatible() else {
                    return Ok(None);
                };

                // If the registry provided metadata for this distribution, use it.
                for version_map in version_map {
                    if let Some(metadata) = version_map.get_metadata(candidate.version()) {
                        let dist = dist.for_resolution();
                        if version_map.index() == dist.index() {
                            debug!("Found registry-provided metadata for: {dist}");

                            let metadata = MetadataResponse::Found(
                                ArchiveMetadata::from_metadata23(metadata.clone()),
                            );

                            let dist = dist.to_owned();
                            if &package_name != dist.name() {
                                return Err(ResolveError::MismatchedPackageName {
                                    request: "distribution",
                                    expected: package_name,
                                    actual: dist.name().clone(),
                                });
                            }

                            let response = match dist {
                                ResolvedDist::Installable { dist, .. } => Response::Dist {
                                    dist: (*dist).clone(),
                                    metadata,
                                },
                                ResolvedDist::Installed { dist } => Response::Installed {
                                    dist: (*dist).clone(),
                                    metadata,
                                },
                            };

                            return Ok(Some(response));
                        }
                    }
                }

                // Avoid prefetching source distributions with unbounded lower-bound ranges. This
                // often leads to failed attempts to build legacy versions of packages that are
                // incompatible with modern build tools.
                if dist.wheel().is_none() {
                    if !self.selector.use_highest_version(&package_name, &env) {
                        if let Some((lower, _)) = range.iter().next() {
                            if lower == &Bound::Unbounded {
                                debug!(
                                    "Skipping prefetch for unbounded minimum-version range: {package_name} ({range})"
                                );
                                return Ok(None);
                            }
                        }
                    }
                }

                // Validate the Python requirement.
                let requires_python = match dist {
                    CompatibleDist::InstalledDist(_) => None,
                    CompatibleDist::SourceDist { sdist, .. }
                    | CompatibleDist::IncompatibleWheel { sdist, .. } => {
                        sdist.file.requires_python.as_ref()
                    }
                    CompatibleDist::CompatibleWheel { wheel, .. } => {
                        wheel.file.requires_python.as_ref()
                    }
                };
                if let Some(requires_python) = requires_python.as_ref() {
                    if !python_requirement.target().is_contained_by(requires_python) {
                        return Ok(None);
                    }
                }

                // Verify that the package is allowed under the hash-checking policy.
                if !self
                    .hasher
                    .allows_package(candidate.name(), candidate.version())
                {
                    return Ok(None);
                }

                // Emit a request to fetch the metadata for this version.
                if self.index.distributions().register(candidate.version_id()) {
                    let dist = dist.for_resolution().to_owned();
                    if &package_name != dist.name() {
                        return Err(ResolveError::MismatchedPackageName {
                            request: "distribution",
                            expected: package_name,
                            actual: dist.name().clone(),
                        });
                    }

                    let response = match dist {
                        ResolvedDist::Installable { dist, .. } => {
                            let metadata = provider
                                .get_or_build_wheel_metadata(&dist)
                                .boxed_local()
                                .await?;

                            Response::Dist {
                                dist: (*dist).clone(),
                                metadata,
                            }
                        }
                        ResolvedDist::Installed { dist } => {
                            let metadata =
                                provider.get_installed_metadata(&dist).boxed_local().await?;

                            Response::Installed {
                                dist: (*dist).clone(),
                                metadata,
                            }
                        }
                    };

                    Ok(Some(response))
                } else {
                    Ok(None)
                }
            }
            Request::Variants(version_id, variants_json) => self
                .fetch_and_query_variants(variants_json, provider)
                .await
                .map(|resolved_variants| {
                    Some(Response::Variants {
                        version_id,
                        resolved_variants,
                    })
                }),
        }
    }

    async fn fetch_and_query_variants<Provider: ResolverProvider>(
        &self,
        variants_json: RegistryVariantsJson,
        provider: &Provider,
    ) -> Result<ResolvedVariants, ResolveError> {
        let Some(marker_env) = self.env.marker_environment() else {
            unreachable!("Variants should only be queried in non-universal resolution")
        };
        provider
            .fetch_and_query_variants(&variants_json, marker_env)
            .await
            .map_err(ResolveError::VariantFrontend)
    }

    fn convert_no_solution_err(
        &self,
        mut err: pubgrub::NoSolutionError<UvDependencyProvider>,
        fork_urls: ForkUrls,
        fork_indexes: ForkIndexes,
        env: ResolverEnvironment,
        current_environment: MarkerEnvironment,
        exclude_newer: Option<&ExcludeNewer>,
        visited: &FxHashSet<PackageName>,
    ) -> ResolveError {
        err = NoSolutionError::collapse_local_version_segments(NoSolutionError::collapse_proxies(
            err,
        ));

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

        let mut available_indexes = FxHashMap::default();
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
            let versions_response = if let Some(index) = fork_indexes.get(name) {
                self.index
                    .explicit()
                    .get(&(name.clone(), index.url().clone()))
            } else {
                self.index.implicit().get(name)
            };
            if let Some(response) = versions_response {
                if let VersionsResponse::Found(ref version_maps) = *response {
                    // Track the available versions, across all indexes.
                    for version_map in version_maps {
                        let package_versions = available_versions
                            .entry(name.clone())
                            .or_insert_with(BTreeSet::new);

                        for (version, dists) in version_map.iter(&Ranges::full()) {
                            // Don't show versions removed by excluded-newer in hints.
                            if let Some(exclude_newer) =
                                exclude_newer.and_then(|en| en.exclude_newer_package(name))
                            {
                                let Some(prioritized_dist) = dists.prioritized_dist() else {
                                    continue;
                                };
                                if prioritized_dist.files().all(|file| {
                                    file.upload_time_utc_ms.is_none_or(|upload_time| {
                                        upload_time >= exclude_newer.timestamp_millis()
                                    })
                                }) {
                                    continue;
                                }
                            }

                            package_versions.insert(version.clone());
                        }
                    }

                    // Track the indexes in which the package is available.
                    available_indexes
                        .entry(name.clone())
                        .or_insert(BTreeSet::new())
                        .extend(
                            version_maps
                                .iter()
                                .filter_map(|version_map| version_map.index().cloned()),
                        );
                }
            }
        }

        ResolveError::NoSolution(Box::new(NoSolutionError::new(
            err,
            self.index.clone(),
            available_versions,
            available_indexes,
            self.selector.clone(),
            self.python_requirement.clone(),
            self.locations.clone(),
            self.capabilities.clone(),
            unavailable_packages,
            incomplete_packages,
            fork_urls,
            fork_indexes,
            env,
            current_environment,
            self.tags.clone(),
            self.workspace_members.clone(),
            self.options.clone(),
        )))
    }

    fn on_progress(&self, package: &PubGrubPackage, version: &Version) {
        if let Some(reporter) = self.reporter.as_ref() {
            match &**package {
                PubGrubPackageInner::Root(_) => {}
                PubGrubPackageInner::Python(_) => {}
                PubGrubPackageInner::System(_) => {}
                PubGrubPackageInner::Marker { .. } => {}
                PubGrubPackageInner::Extra { .. } => {}
                PubGrubPackageInner::Group { .. } => {}
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
pub(crate) struct ForkState {
    /// The internal state used by the resolver.
    ///
    /// Note that not all parts of this state are strictly internal. For
    /// example, the edges in the dependency graph generated as part of the
    /// output of resolution are derived from the "incompatibilities" tracked
    /// in this state. We also ultimately retrieve the final set of version
    /// assignments (to packages) from this state's "partial solution."
    pubgrub: State<UvDependencyProvider>,
    /// The initial package to select. If set, the first iteration over this state will avoid
    /// asking PubGrub for the highest-priority package, and will instead use the provided package.
    initial_id: Option<Id<PubGrubPackage>>,
    /// The initial version to select. If set, the first iteration over this state will avoid
    /// asking PubGrub for the highest-priority version, and will instead use the provided version.
    initial_version: Option<Version>,
    /// The next package on which to run unit propagation.
    next: Id<PubGrubPackage>,
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
    /// Ensure we don't have duplicate URLs in any branch.
    ///
    /// Unlike [`Urls`], we add only the URLs we have seen in this branch, and there can be only
    /// one URL per package. By prioritizing direct URL dependencies over registry dependencies,
    /// this map is populated for all direct URL packages before we look at any registry packages.
    fork_urls: ForkUrls,
    /// Ensure we don't have duplicate indexes in any branch.
    ///
    /// Unlike [`Indexes`], we add only the indexes we have seen in this branch, and there can be
    /// only one index per package.
    fork_indexes: ForkIndexes,
    /// When dependencies for a package are retrieved, this map of priorities
    /// is updated based on how each dependency was specified. Certain types
    /// of dependencies have more "priority" than others (like direct URL
    /// dependencies). These priorities help determine which package to
    /// consider next during resolution.
    priorities: PubGrubPriorities,
    /// This keeps track of the set of versions for each package that we've
    /// already visited during resolution. This avoids doing redundant work.
    added_dependencies: FxHashMap<Id<PubGrubPackage>, FxHashSet<Version>>,
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
    env: ResolverEnvironment,
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
    conflict_tracker: ConflictTracker,
    /// Prefetch package versions for packages with many rejected versions.
    ///
    /// Tracked on the fork state to avoid counting each identical version between forks as new try.
    prefetcher: BatchPrefetcher,
}

impl ForkState {
    fn new(
        pubgrub: State<UvDependencyProvider>,
        env: ResolverEnvironment,
        python_requirement: PythonRequirement,
        prefetcher: BatchPrefetcher,
    ) -> Self {
        Self {
            initial_id: None,
            initial_version: None,
            next: pubgrub.root_package,
            pubgrub,
            pins: FilePins::default(),
            fork_urls: ForkUrls::default(),
            fork_indexes: ForkIndexes::default(),
            priorities: PubGrubPriorities::default(),
            added_dependencies: FxHashMap::default(),
            env,
            python_requirement,
            conflict_tracker: ConflictTracker::default(),
            prefetcher,
        }
    }

    /// Visit the dependencies for the selected version of the current package, incorporating any
    /// relevant URLs and pinned indexes into the [`ForkState`].
    fn visit_package_version_dependencies(
        &mut self,
        for_package: Id<PubGrubPackage>,
        for_version: &Version,
        urls: &Urls,
        indexes: &Indexes,
        dependencies: &[PubGrubDependency],
        git: &GitResolver,
        workspace_members: &BTreeSet<PackageName>,
        resolution_strategy: &ResolutionStrategy,
    ) -> Result<(), ResolveError> {
        for dependency in dependencies {
            let PubGrubDependency {
                package,
                version,
                parent: _,
                url,
            } = dependency;

            let mut has_url = false;
            if let Some(name) = package.name() {
                // From the [`Requirement`] to [`PubGrubDependency`] conversion, we get a URL if the
                // requirement was a URL requirement. `Urls` applies canonicalization to this and
                // override URLs to both URL and registry requirements, which we then check for
                // conflicts using [`ForkUrl`].
                for url in urls.get_url(&self.env, name, url.as_ref(), git)? {
                    self.fork_urls.insert(name, url, &self.env)?;
                    has_url = true;
                }

                // If the package is pinned to an exact index, add it to the fork.
                for index in indexes.get(name, &self.env) {
                    self.fork_indexes.insert(name, index, &self.env)?;
                }
            }

            if let Some(name) = self.pubgrub.package_store[for_package]
                .name_no_root()
                .filter(|name| !workspace_members.contains(name))
            {
                debug!(
                    "Adding transitive dependency for {name}=={for_version}: {package}{version}"
                );
            } else {
                // A dependency from the root package or `requirements.txt`.
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
                        "The direct dependency `{name}` is unpinned. \
                        Consider setting a lower bound when using `--resolution lowest` \
                        or `--resolution lowest-direct` to avoid using outdated versions.",
                        name = package.name_no_root().unwrap(),
                    );
                }
            }

            // Update the package priorities.
            self.priorities.insert(package, version, &self.fork_urls);
        }

        Ok(())
    }

    /// Add the dependencies for the selected version of the current package.
    fn add_package_version_dependencies(
        &mut self,
        for_package: Id<PubGrubPackage>,
        for_version: &Version,
        dependencies: Vec<PubGrubDependency>,
    ) {
        let conflict = self.pubgrub.add_package_version_dependencies(
            self.next,
            for_version.clone(),
            dependencies.into_iter().map(|dependency| {
                let PubGrubDependency {
                    package,
                    version,
                    parent: _,
                    url: _,
                } = dependency;
                (package, version)
            }),
        );

        // Conflict tracking: If the version was rejected due to its dependencies, record culprit
        // and affected.
        if let Some(incompatibility) = conflict {
            self.record_conflict(for_package, Some(for_version), incompatibility);
        }
    }

    fn record_conflict(
        &mut self,
        affected: Id<PubGrubPackage>,
        version: Option<&Version>,
        incompatibility: IncompId<PubGrubPackage, Ranges<Version>, UnavailableReason>,
    ) {
        let mut culprit_is_real = false;
        for (incompatible, _term) in self.pubgrub.incompatibility_store[incompatibility].iter() {
            if incompatible == affected {
                continue;
            }
            if self.pubgrub.package_store[affected].name()
                == self.pubgrub.package_store[incompatible].name()
            {
                // Don't track conflicts between a marker package and the main package, when the
                // marker is "copying" the obligations from the main package through conflicts.
                continue;
            }
            culprit_is_real = true;
            let culprit_count = self
                .conflict_tracker
                .culprit
                .entry(incompatible)
                .or_default();
            *culprit_count += 1;
            if *culprit_count == CONFLICT_THRESHOLD {
                self.conflict_tracker.deprioritize.push(incompatible);
            }
        }
        // Don't track conflicts between a marker package and the main package, when the
        // marker is "copying" the obligations from the main package through conflicts.
        if culprit_is_real {
            if tracing::enabled!(Level::DEBUG) {
                let incompatibility = self.pubgrub.incompatibility_store[incompatibility]
                    .iter()
                    .map(|(package, _term)| &self.pubgrub.package_store[package])
                    .join(", ");
                if let Some(version) = version {
                    debug!(
                        "Recording dependency conflict of {}=={} from incompatibility of ({})",
                        self.pubgrub.package_store[affected], version, incompatibility
                    );
                } else {
                    debug!(
                        "Recording unit propagation conflict of {} from incompatibility of ({})",
                        self.pubgrub.package_store[affected], incompatibility
                    );
                }
            }

            let affected_count = self.conflict_tracker.affected.entry(self.next).or_default();
            *affected_count += 1;
            if *affected_count == CONFLICT_THRESHOLD {
                self.conflict_tracker.prioritize.push(self.next);
            }
        }
    }

    fn add_unavailable_version(&mut self, version: Version, reason: UnavailableVersion) {
        // Incompatible requires-python versions are special in that we track
        // them as incompatible dependencies instead of marking the package version
        // as unavailable directly.
        if let UnavailableVersion::IncompatibleDist(
            IncompatibleDist::Source(IncompatibleSource::RequiresPython(requires_python, kind))
            | IncompatibleDist::Wheel(IncompatibleWheel::RequiresPython(requires_python, kind)),
        ) = reason
        {
            let package = &self.next;
            let python = self.pubgrub.package_store.alloc(PubGrubPackage::from(
                PubGrubPackageInner::Python(match kind {
                    PythonRequirementKind::Installed => PubGrubPython::Installed,
                    PythonRequirementKind::Target => PubGrubPython::Target,
                }),
            ));
            self.pubgrub
                .add_incompatibility(Incompatibility::from_dependency(
                    *package,
                    Range::singleton(version.clone()),
                    (python, release_specifiers_to_ranges(requires_python)),
                ));
            self.pubgrub
                .partial_solution
                .add_decision(self.next, version);
            return;
        }
        self.pubgrub
            .add_incompatibility(Incompatibility::custom_version(
                self.next,
                version.clone(),
                UnavailableReason::Version(reason),
            ));
    }

    /// Subset the current markers with the new markers and update the python requirements fields
    /// accordingly.
    ///
    /// If the fork should be dropped (e.g., because its markers can never be true for its
    /// Python requirement), then this returns `None`.
    fn with_env(mut self, env: ResolverEnvironment) -> Self {
        self.env = env;
        // If the fork contains a narrowed Python requirement, apply it.
        if let Some(req) = self.env.narrow_python_requirement(&self.python_requirement) {
            debug!("Narrowed `requires-python` bound to: {}", req.target());
            self.python_requirement = req;
        }
        self
    }

    /// Returns the URL or index for a package and version.
    ///
    /// In practice, exactly one of the returned values will be `Some`.
    fn source(
        &self,
        name: &PackageName,
        version: &Version,
    ) -> (Option<&VerbatimParsedUrl>, Option<&IndexUrl>) {
        let url = self.fork_urls.get(name);
        let index = url
            .is_none()
            .then(|| {
                self.pins
                    .get(name, version)
                    .expect("Every package should be pinned")
                    .index()
            })
            .flatten();
        (url, index)
    }

    fn into_resolution(self) -> Resolution {
        let solution: FxHashMap<_, _> = self.pubgrub.partial_solution.extract_solution().collect();
        let edge_count: usize = solution
            .keys()
            .map(|package| self.pubgrub.incompatibilities[package].len())
            .sum();
        let mut edges: Vec<ResolutionDependencyEdge> = Vec::with_capacity(edge_count);
        for (package, self_version) in &solution {
            for id in &self.pubgrub.incompatibilities[package] {
                let pubgrub::Kind::FromDependencyOf(
                    self_package,
                    ref self_range,
                    dependency_package,
                    ref dependency_range,
                ) = self.pubgrub.incompatibility_store[*id].kind
                else {
                    continue;
                };
                if *package != self_package {
                    continue;
                }
                if !self_range.contains(self_version) {
                    continue;
                }
                let Some(dependency_version) = solution.get(&dependency_package) else {
                    continue;
                };
                if !dependency_range.contains(dependency_version) {
                    continue;
                }

                let self_package = &self.pubgrub.package_store[self_package];
                let dependency_package = &self.pubgrub.package_store[dependency_package];

                let (self_name, self_extra, self_group) = match &**self_package {
                    PubGrubPackageInner::Package {
                        name: self_name,
                        extra: self_extra,
                        group: self_group,
                        marker: _,
                    } => (Some(self_name), self_extra.as_ref(), self_group.as_ref()),

                    PubGrubPackageInner::Root(_) => (None, None, None),

                    _ => continue,
                };

                let (self_url, self_index) = self_name
                    .map(|self_name| self.source(self_name, self_version))
                    .unwrap_or((None, None));

                match **dependency_package {
                    PubGrubPackageInner::Package {
                        name: ref dependency_name,
                        extra: ref dependency_extra,
                        group: ref dependency_dev,
                        marker: ref dependency_marker,
                    } => {
                        debug_assert!(
                            dependency_extra.is_none(),
                            "Packages should depend on an extra proxy"
                        );
                        debug_assert!(
                            dependency_dev.is_none(),
                            "Packages should depend on a group proxy"
                        );

                        // Ignore self-dependencies (e.g., `tensorflow-macos` depends on `tensorflow-macos`),
                        // but allow groups to depend on other groups, or on the package itself.
                        if self_group.is_none() {
                            if self_name == Some(dependency_name) {
                                continue;
                            }
                        }

                        let (to_url, to_index) = self.source(dependency_name, dependency_version);

                        let edge = ResolutionDependencyEdge {
                            from: self_name.cloned(),
                            from_version: self_version.clone(),
                            from_url: self_url.cloned(),
                            from_index: self_index.cloned(),
                            from_extra: self_extra.cloned(),
                            from_group: self_group.cloned(),
                            to: dependency_name.clone(),
                            to_version: dependency_version.clone(),
                            to_url: to_url.cloned(),
                            to_index: to_index.cloned(),
                            to_extra: dependency_extra.clone(),
                            to_group: dependency_dev.clone(),
                            marker: *dependency_marker,
                        };
                        edges.push(edge);
                    }

                    PubGrubPackageInner::Marker {
                        name: ref dependency_name,
                        marker: ref dependency_marker,
                    } => {
                        // Ignore self-dependencies (e.g., `tensorflow-macos` depends on `tensorflow-macos`),
                        // but allow groups to depend on other groups, or on the package itself.
                        if self_group.is_none() {
                            if self_name == Some(dependency_name) {
                                continue;
                            }
                        }

                        let (to_url, to_index) = self.source(dependency_name, dependency_version);

                        let edge = ResolutionDependencyEdge {
                            from: self_name.cloned(),
                            from_version: self_version.clone(),
                            from_url: self_url.cloned(),
                            from_index: self_index.cloned(),
                            from_extra: self_extra.cloned(),
                            from_group: self_group.cloned(),
                            to: dependency_name.clone(),
                            to_version: dependency_version.clone(),
                            to_url: to_url.cloned(),
                            to_index: to_index.cloned(),
                            to_extra: None,
                            to_group: None,
                            marker: *dependency_marker,
                        };
                        edges.push(edge);
                    }

                    PubGrubPackageInner::Extra {
                        name: ref dependency_name,
                        extra: ref dependency_extra,
                        marker: ref dependency_marker,
                    } => {
                        if self_group.is_none() {
                            debug_assert!(
                                self_name != Some(dependency_name),
                                "Extras should be flattened"
                            );
                        }
                        let (to_url, to_index) = self.source(dependency_name, dependency_version);

                        // Insert an edge from the dependent package to the extra package.
                        let edge = ResolutionDependencyEdge {
                            from: self_name.cloned(),
                            from_version: self_version.clone(),
                            from_url: self_url.cloned(),
                            from_index: self_index.cloned(),
                            from_extra: self_extra.cloned(),
                            from_group: self_group.cloned(),
                            to: dependency_name.clone(),
                            to_version: dependency_version.clone(),
                            to_url: to_url.cloned(),
                            to_index: to_index.cloned(),
                            to_extra: Some(dependency_extra.clone()),
                            to_group: None,
                            marker: *dependency_marker,
                        };
                        edges.push(edge);

                        // Insert an edge from the dependent package to the base package.
                        let edge = ResolutionDependencyEdge {
                            from: self_name.cloned(),
                            from_version: self_version.clone(),
                            from_url: self_url.cloned(),
                            from_index: self_index.cloned(),
                            from_extra: self_extra.cloned(),
                            from_group: self_group.cloned(),
                            to: dependency_name.clone(),
                            to_version: dependency_version.clone(),
                            to_url: to_url.cloned(),
                            to_index: to_index.cloned(),
                            to_extra: None,
                            to_group: None,
                            marker: *dependency_marker,
                        };
                        edges.push(edge);
                    }

                    PubGrubPackageInner::Group {
                        name: ref dependency_name,
                        group: ref dependency_group,
                        marker: ref dependency_marker,
                    } => {
                        debug_assert!(
                            self_name != Some(dependency_name),
                            "Groups should be flattened"
                        );

                        let (to_url, to_index) = self.source(dependency_name, dependency_version);

                        // Add an edge from the dependent package to the dev package, but _not_ the
                        // base package.
                        let edge = ResolutionDependencyEdge {
                            from: self_name.cloned(),
                            from_version: self_version.clone(),
                            from_url: self_url.cloned(),
                            from_index: self_index.cloned(),
                            from_extra: self_extra.cloned(),
                            from_group: self_group.cloned(),
                            to: dependency_name.clone(),
                            to_version: dependency_version.clone(),
                            to_url: to_url.cloned(),
                            to_index: to_index.cloned(),
                            to_extra: None,
                            to_group: Some(dependency_group.clone()),
                            marker: *dependency_marker,
                        };
                        edges.push(edge);
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
                    group,
                    marker: MarkerTree::TRUE,
                } = &*self.pubgrub.package_store[package]
                {
                    let (url, index) = self.source(name, &version);
                    Some((
                        ResolutionPackage {
                            name: name.clone(),
                            extra: extra.clone(),
                            dev: group.clone(),
                            url: url.cloned(),
                            index: index.cloned(),
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
            env: self.env,
        }
    }
}

/// The resolution from a single fork including the virtual packages and the edges between them.
#[derive(Debug)]
pub(crate) struct Resolution {
    pub(crate) nodes: FxHashMap<ResolutionPackage, Version>,
    /// The directed connections between the nodes, where the marker is the node weight. We don't
    /// store the requirement itself, but it can be retrieved from the package metadata.
    pub(crate) edges: Vec<ResolutionDependencyEdge>,
    /// Map each package name, version tuple from `packages` to a distribution.
    pub(crate) pins: FilePins,
    /// The environment setting this resolution was found under.
    pub(crate) env: ResolverEnvironment,
}

/// Package representation we used during resolution where each extra and also the dev-dependencies
/// group are their own package.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResolutionPackage {
    pub(crate) name: PackageName,
    pub(crate) extra: Option<ExtraName>,
    pub(crate) dev: Option<GroupName>,
    /// For registry packages, this is `None`; otherwise, the direct URL of the distribution.
    pub(crate) url: Option<VerbatimParsedUrl>,
    /// For URL packages, this is `None`; otherwise, the index URL of the distribution.
    pub(crate) index: Option<IndexUrl>,
}

/// The `from_` fields and the `to_` fields allow mapping to the originating and target
///  [`ResolutionPackage`] respectively. The `marker` is the edge weight.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResolutionDependencyEdge {
    /// This value is `None` if the dependency comes from the root package.
    pub(crate) from: Option<PackageName>,
    pub(crate) from_version: Version,
    pub(crate) from_url: Option<VerbatimParsedUrl>,
    pub(crate) from_index: Option<IndexUrl>,
    pub(crate) from_extra: Option<ExtraName>,
    pub(crate) from_group: Option<GroupName>,
    pub(crate) to: PackageName,
    pub(crate) to_version: Version,
    pub(crate) to_url: Option<VerbatimParsedUrl>,
    pub(crate) to_index: Option<IndexUrl>,
    pub(crate) to_extra: Option<ExtraName>,
    pub(crate) to_group: Option<GroupName>,
    pub(crate) marker: MarkerTree,
}

impl ResolutionDependencyEdge {
    pub(crate) fn universal_marker(&self) -> UniversalMarker {
        // We specifically do not account for conflict
        // markers here. Instead, those are computed via
        // a traversal on the resolution graph.
        UniversalMarker::new(self.marker, ConflictMarker::TRUE)
    }
}

/// Fetch the metadata for an item
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum Request {
    /// A request to fetch the metadata for a package.
    Package(PackageName, Option<IndexMetadata>),
    /// A request to fetch the metadata for a built or source distribution.
    Dist(Dist),
    /// A request to fetch the metadata from an already-installed distribution.
    Installed(InstalledDist),
    /// A request to pre-fetch the metadata for a package and the best-guess distribution.
    Prefetch(PackageName, Range<Version>, PythonRequirement),
    /// Resolve the variants for a package
    Variants(GlobalVersionId, RegistryVariantsJson),
}

impl<'a> From<ResolvedDistRef<'a>> for Request {
    fn from(dist: ResolvedDistRef<'a>) -> Self {
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
                Self::Dist(Dist::Source(SourceDist::Registry(source)))
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
                Self::Dist(Dist::Built(BuiltDist::Registry(built)))
            }
            ResolvedDistRef::Installed { dist } => Self::Installed(dist.clone()),
        }
    }
}

impl Display for Request {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Package(package_name, _) => {
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
            Self::Variants(version_id, _) => {
                write!(f, "Variants {version_id}")
            }
        }
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum Response {
    /// The returned metadata for a package hosted on a registry.
    Package(PackageName, Option<IndexUrl>, VersionsResponse),
    /// The returned metadata for a distribution.
    Dist {
        dist: Dist,
        metadata: MetadataResponse,
    },
    /// The returned metadata for an already-installed distribution.
    Installed {
        dist: InstalledDist,
        metadata: MetadataResponse,
    },
    /// The returned variant compatibility.
    Variants {
        version_id: GlobalVersionId,
        resolved_variants: ResolvedVariants,
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
    fn fork(
        self,
        env: &ResolverEnvironment,
        python_requirement: &PythonRequirement,
        conflicts: &Conflicts,
        variant_base: Option<&str>,
    ) -> ForkedDependencies {
        let deps = match self {
            Self::Available(deps) => deps,
            Self::Unforkable(deps) => return ForkedDependencies::Unforked(deps),
            Self::Unavailable(err) => return ForkedDependencies::Unavailable(err),
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
        } = Forks::new(
            name_to_deps,
            env,
            python_requirement,
            conflicts,
            variant_base,
        );
        if forks.is_empty() {
            ForkedDependencies::Unforked(vec![])
        } else if forks.len() == 1 {
            ForkedDependencies::Unforked(forks.pop().unwrap().dependencies)
        } else {
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
        env: &ResolverEnvironment,
        python_requirement: &PythonRequirement,
        conflicts: &Conflicts,
        variant_base: Option<&str>,
    ) -> Self {
        let python_marker = python_requirement.to_marker_tree();

        let mut forks = vec![Fork::new(env.clone())];
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
            if let [dep] = deps.as_slice() {
                // There's one exception: if the requirement increases the minimum-supported Python
                // version, we also fork in order to respect that minimum in the subsequent
                // resolution.
                //
                // For example, given `requires-python = ">=3.7"` and `uv ; python_version >= "3.8"`,
                // where uv itself only supports Python 3.8 and later, we need to fork to ensure
                // that the resolution can find a solution.
                if marker::requires_python(dep.package.marker())
                    .is_none_or(|bound| !python_requirement.raises(&bound))
                {
                    let dep = deps.pop().unwrap();
                    let marker = if let Some(variant_base) = variant_base {
                        dep.package.marker().with_variant_base(variant_base)
                    } else {
                        dep.package.marker()
                    };
                    for fork in &mut forks {
                        if fork.env.included_by_marker(marker) {
                            fork.add_dependency(dep.clone());
                        }
                    }
                    continue;
                }
            } else {
                // If all dependencies have the same markers, we should also avoid forking.
                if let Some(dep) = deps.first() {
                    let marker = dep.package.marker();
                    if deps.iter().all(|dep| marker == dep.package.marker()) {
                        // Unless that "same marker" is a Python requirement that is stricter than
                        // the current Python requirement. In that case, we need to fork to respect
                        // the stricter requirement.
                        if marker::requires_python(marker)
                            .is_none_or(|bound| !python_requirement.raises(&bound))
                        {
                            for dep in deps {
                                for fork in &mut forks {
                                    if fork.env.included_by_marker(marker) {
                                        fork.add_dependency(dep.clone());
                                    }
                                }
                            }
                            continue;
                        }
                    }
                }
            }
            for dep in deps {
                let mut forker = match ForkingPossibility::new(env, &dep, variant_base) {
                    ForkingPossibility::Possible(forker) => forker,
                    ForkingPossibility::DependencyAlwaysExcluded => {
                        // If the markers can never be satisfied by the parent
                        // fork, then we can drop this dependency unceremoniously.
                        continue;
                    }
                    ForkingPossibility::NoForkingPossible => {
                        // Or, if the markers are always true, then we just
                        // add the dependency to every fork unconditionally.
                        for fork in &mut forks {
                            fork.add_dependency(dep.clone());
                        }
                        continue;
                    }
                };
                // Otherwise, we *should* need to add a new fork...
                diverging_packages.insert(name.clone());

                let mut new = vec![];
                for fork in std::mem::take(&mut forks) {
                    let Some((remaining_forker, envs)) = forker.fork(&fork.env) else {
                        new.push(fork);
                        continue;
                    };
                    forker = remaining_forker;

                    for fork_env in envs {
                        let mut new_fork = fork.clone();
                        new_fork.set_env(fork_env, variant_base);
                        // We only add the dependency to this fork if it
                        // satisfies the fork's markers. Some forks are
                        // specifically created to exclude this dependency,
                        // so this isn't always true!
                        if forker.included(&new_fork.env, variant_base) {
                            new_fork.add_dependency(dep.clone());
                        }
                        // Filter out any forks we created that are disjoint with our
                        // Python requirement.
                        if new_fork.env.included_by_marker(python_marker) {
                            new.push(new_fork);
                        }
                    }
                }
                forks = new;
            }
        }
        // When there is a conflicting group configuration, we need
        // to potentially add more forks. Each fork added contains an
        // exclusion list of conflicting groups where dependencies with
        // the corresponding package and extra name are forcefully
        // excluded from that group.
        //
        // We specifically iterate on conflicting groups and
        // potentially re-generate all forks for each one. We do it
        // this way in case there are multiple sets of conflicting
        // groups that impact the forks here.
        //
        // For example, if we have conflicting groups {x1, x2} and {x3,
        // x4}, we need to make sure the forks generated from one set
        // also account for the other set.
        for set in conflicts.iter() {
            let mut new = vec![];
            for fork in std::mem::take(&mut forks) {
                let mut has_conflicting_dependency = false;
                for item in set.iter() {
                    if fork.contains_conflicting_item(item.as_ref()) {
                        has_conflicting_dependency = true;
                        diverging_packages.insert(item.package().clone());
                        break;
                    }
                }
                if !has_conflicting_dependency {
                    new.push(fork);
                    continue;
                }

                // Create a fork that excludes ALL conflicts.
                if let Some(fork_none) = fork.clone().filter(set.iter().cloned().map(Err)) {
                    new.push(fork_none);
                }

                // Now create a fork for each conflicting group, where
                // that fork excludes every *other* conflicting group.
                //
                // So if we have conflicting extras foo, bar and baz,
                // then this creates three forks: one that excludes
                // {foo, bar}, one that excludes {foo, baz} and one
                // that excludes {bar, baz}.
                for (i, _) in set.iter().enumerate() {
                    let fork_allows_group = fork.clone().filter(
                        set.iter()
                            .cloned()
                            .enumerate()
                            .map(|(j, group)| if i == j { Ok(group) } else { Err(group) }),
                    );
                    if let Some(fork_allows_group) = fork_allows_group {
                        new.push(fork_allows_group);
                    }
                }
            }
            forks = new;
        }
        Self {
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
    /// The conflicting groups in this fork.
    ///
    /// This exists to make some access patterns more efficient. Namely,
    /// it makes it easy to check whether there's a dependency with a
    /// particular conflicting group in this fork.
    conflicts: crate::FxHashbrownSet<ConflictItem>,
    /// The resolver environment for this fork.
    ///
    /// Principally, this corresponds to the markers in this for. So in the
    /// example above, the `a<2` fork would have `sys_platform == 'foo'`, while
    /// the `a>=2` fork would have `sys_platform == 'bar'`.
    ///
    /// If this fork was generated from another fork, then this *includes*
    /// the criteria from its parent. i.e., Its marker expression represents
    /// the intersection of the marker expression from its parent and any
    /// additional marker expression generated by addition forking based on
    /// conflicting dependency specifications.
    env: ResolverEnvironment,
}

impl Fork {
    /// Create a new fork with no dependencies with the given resolver
    /// environment.
    fn new(env: ResolverEnvironment) -> Self {
        Self {
            dependencies: vec![],
            conflicts: crate::FxHashbrownSet::default(),
            env,
        }
    }

    /// Add a dependency to this fork.
    fn add_dependency(&mut self, dep: PubGrubDependency) {
        if let Some(conflicting_item) = dep.conflicting_item() {
            self.conflicts.insert(conflicting_item.to_owned());
        }
        self.dependencies.push(dep);
    }

    /// Sets the resolver environment to the one given.
    ///
    /// Any dependency in this fork that does not satisfy the given environment
    /// is removed.
    fn set_env(&mut self, env: ResolverEnvironment, variant_base: Option<&str>) {
        self.env = env;
        self.dependencies.retain(|dep| {
            let marker = if let Some(variant_base) = variant_base {
                dep.package.marker().with_variant_base(variant_base)
            } else {
                dep.package.marker()
            };
            if self.env.included_by_marker(marker) {
                return true;
            }
            if let Some(conflicting_item) = dep.conflicting_item() {
                self.conflicts.remove(&conflicting_item);
            }
            false
        });
    }

    /// Returns true if any of the dependencies in this fork contain a
    /// dependency with the given package and extra values.
    fn contains_conflicting_item(&self, item: ConflictItemRef<'_>) -> bool {
        self.conflicts.contains(&item)
    }

    /// Include or Exclude the given groups from this fork.
    ///
    /// This removes all dependencies matching the given conflicting groups.
    ///
    /// If the exclusion rules would result in a fork with an unsatisfiable
    /// resolver environment, then this returns `None`.
    fn filter(
        mut self,
        rules: impl IntoIterator<Item = Result<ConflictItem, ConflictItem>>,
    ) -> Option<Self> {
        self.env = self.env.filter_by_group(rules)?;
        self.dependencies.retain(|dep| {
            let Some(conflicting_item) = dep.conflicting_item() else {
                return true;
            };
            if self.env.included_by_group(conflicting_item) {
                return true;
            }
            match conflicting_item.kind() {
                // We should not filter entire projects unless they're a top-level dependency
                // Otherwise, we'll fail to solve for children of the project, like extras
                ConflictKindRef::Project => {
                    if dep.parent.is_some() {
                        return true;
                    }
                }
                ConflictKindRef::Group(_) => {}
                ConflictKindRef::Extra(_) => {}
            }
            self.conflicts.remove(&conflicting_item);
            false
        });
        Some(self)
    }

    /// Compare forks, preferring forks with g `requires-python` requirements.
    fn cmp_requires_python(&self, other: &Self) -> Ordering {
        // A higher `requires-python` requirement indicates a _higher-priority_ fork.
        //
        // This ordering ensures that we prefer choosing the highest version for each fork based on
        // its `requires-python` requirement.
        //
        // The reverse would prefer choosing fewer versions, at the cost of using older package
        // versions on newer Python versions. For example, if reversed, we'd prefer to solve `<3.7
        // before solving `>=3.7`, since the resolution produced by the former might work for the
        // latter, but the inverse is unlikely to be true.
        let self_bound = self.env.requires_python().unwrap_or_default();
        let other_bound = other.env.requires_python().unwrap_or_default();
        self_bound.lower().cmp(other_bound.lower())
    }

    /// Compare forks, preferring forks with upper bounds.
    fn cmp_upper_bounds(&self, other: &Self) -> Ordering {
        // We'd prefer to solve `numpy <= 2` before solving `numpy >= 1`, since the resolution
        // produced by the former might work for the latter, but the inverse is unlikely to be true
        // due to maximum version selection. (Selecting `numpy==2.0.0` would satisfy both forks, but
        // selecting the latest `numpy` would not.)
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
    }
}

impl Eq for Fork {}

impl PartialEq for Fork {
    fn eq(&self, other: &Self) -> bool {
        self.dependencies == other.dependencies && self.env == other.env
    }
}

#[derive(Debug, Clone)]
pub(crate) struct VersionFork {
    /// The environment to use in the fork.
    env: ResolverEnvironment,
    /// The initial package to select in the fork.
    id: Id<PubGrubPackage>,
    /// The initial version to set for the selected package in the fork.
    version: Option<Version>,
}

/// Enrich a [`ResolveError`] with additional information about why a given package was included.
fn enrich_dependency_error(
    error: ResolveError,
    id: Id<PubGrubPackage>,
    version: &Version,
    pubgrub: &State<UvDependencyProvider>,
) -> ResolveError {
    let Some(name) = pubgrub.package_store[id].name_no_root() else {
        return error;
    };
    let chain = DerivationChainBuilder::from_state(id, version, pubgrub).unwrap_or_default();
    ResolveError::Dependencies(Box::new(error), name.clone(), version.clone(), chain)
}

/// Compute the set of markers for which a package is known to be relevant.
fn find_environments(
    id: Id<PubGrubPackage>,
    state: &State<UvDependencyProvider>,
    variant_base: &str,
) -> MarkerTree {
    let package = &state.package_store[id];
    if package.is_root() {
        return MarkerTree::TRUE;
    }

    // Retrieve the incompatibilities for the current package.
    let Some(incompatibilities) = state.incompatibilities.get(&id) else {
        return MarkerTree::FALSE;
    };

    // Find all dependencies on the current package.
    let mut marker = MarkerTree::FALSE;
    for index in incompatibilities {
        let incompat = &state.incompatibility_store[*index];
        if let Kind::FromDependencyOf(id1, _, id2, _) = &incompat.kind {
            if id == *id2 {
                marker.or({
                    let mut marker = package.marker().with_variant_base(variant_base);
                    marker.and(find_environments(*id1, state, variant_base));
                    marker
                });
            }
        }
    }
    marker
}

#[derive(Debug, Default, Clone)]
struct ConflictTracker {
    /// How often a decision on the package was discarded due to another package decided earlier.
    affected: FxHashMap<Id<PubGrubPackage>, usize>,
    /// Package(s) to be prioritized after the next unit propagation
    ///
    /// Distilled from `affected` for fast checking in the hot loop.
    prioritize: Vec<Id<PubGrubPackage>>,
    /// How often a package was decided earlier and caused another package to be discarded.
    culprit: FxHashMap<Id<PubGrubPackage>, usize>,
    /// Package(s) to be de-prioritized after the next unit propagation
    ///
    /// Distilled from `culprit` for fast checking in the hot loop.
    deprioritize: Vec<Id<PubGrubPackage>>,
}
