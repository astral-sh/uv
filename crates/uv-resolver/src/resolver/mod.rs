//! Given a set of requirements, find a set of compatible packages.

use std::borrow::Cow;
use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::{Display, Formatter};
use std::ops::Bound;
use std::sync::Arc;
use std::time::Instant;
use std::{mem, thread};

use futures::{FutureExt, StreamExt};
use itertools::Itertools;
use papaya::{HashMap, ResizeMode};
use pubgrub::{
    DerivationTree, External, Id, IncompId, Incompatibility, Kind, Ranges, State, Term, VersionSet,
};
use rustc_hash::{FxHashMap, FxHashSet};
use tokio::sync::mpsc::{self, Receiver};
use tokio::sync::oneshot;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{Level, debug, info, instrument, trace, warn};

use uv_configuration::{Constraints, Excludes, ForkStrategy, Overrides};
use uv_distribution::{ArchiveMetadata, DistributionDatabase, Metadata};
use uv_distribution_types::{
    BuiltDist, CompatibleDist, DependencyMetadata, Dist, DistErrorKind, Identifier,
    IncompatibleDist, IncompatibleSource, IncompatibleWheel, IndexCapabilities, IndexLocations,
    IndexMetadata, IndexUrl, InstalledDist, Name, PythonRequirementKind, RemoteSource,
    RequestedDist, Requirement, RequirementSource, ResolvedDist, ResolvedDistRef, SourceDist,
    VersionOrUrlRef, implied_markers,
};
use uv_git::GitResolver;
use uv_git_types::GitUrl;
use uv_normalize::PackageName;
use uv_pep440::{MIN_VERSION, Version, VersionSpecifiers, release_specifiers_to_ranges};
use uv_pep508::{
    MarkerEnvironment, MarkerExpression, MarkerOperator, MarkerTree, MarkerValueString,
};
use uv_platform_tags::{IncompatibleTag, Tags};
use uv_pypi_types::{
    ConflictItem, ConflictItemRef, ConflictKindRef, Conflicts, ParsedUrl, VerbatimParsedUrl, Yanked,
};
use uv_static::EnvVars;
use uv_torch::TorchStrategy;
use uv_types::{BuildContext, HashStrategy, HashStrategyError, InstalledPackagesProvider};
use uv_warnings::warn_user_once;

use crate::candidate_selector::{Candidate, CandidateDist, CandidateSelector, SelectionPolicy};
use crate::dependency_provider::UvDependencyProvider;
use crate::error::{NoSolutionError, ResolveError, derivation_tree_packages};
use crate::fork_indexes::ForkIndexes;
use crate::fork_urls::ForkUrls;
use crate::manifest::Manifest;
use crate::pins::FilePins;
use crate::preferences::{PreferenceSource, Preferences};
use crate::prerelease::contains_prerelease;
use crate::pubgrub::solver_version::{project_error, report_sources as sources_for_report};
use crate::pubgrub::{
    CandidateSet, DependencySource, IndexId, PubGrubDependency, PubGrubPackage,
    PubGrubPackageInner, PubGrubPriorities, PubGrubPython, Range, SolverSource, SolverVersion,
    SourceId,
};
use crate::python_requirement::PythonRequirement;
use crate::resolution::ResolverOutput;
use crate::resolution_mode::ResolutionStrategy;
pub(crate) use crate::resolver::availability::{
    ResolverVersion, UnavailableErrorChain, UnavailablePackage, UnavailableReason,
    UnavailableVersion, UnsatisfiableRequirement,
};
use crate::resolver::batch_prefetch::BatchPrefetcher;
use crate::resolver::derivation::DerivationChainBuilder;
pub use crate::resolver::environment::ResolverEnvironment;
use crate::resolver::environment::{
    ForkingPossibility, fork_version_by_marker, fork_version_by_python_requirement,
};
pub(crate) use crate::resolver::fork_map::{ForkMap, ForkSet};
use crate::resolver::index::DirectHashKey;
pub use crate::resolver::index::InMemoryIndex;
use crate::resolver::indexes::Indexes;
use crate::resolver::package_source::PackageSource;
pub use crate::resolver::provider::{
    DefaultResolverProvider, MetadataResponse, PackageVersionsResult, ResolverProvider,
    VersionsResponse, WheelMetadataResult,
};
pub use crate::resolver::reporter::Reporter;
use crate::resolver::requests::MetadataRequests;
use crate::resolver::requirements::{RequirementContext, RequirementExpander};
use crate::resolver::sources::{
    ActiveHashes, Grounding, SolvedDependency, SourceAssumptions, SourceDependencies,
    SourcePotential, UrlDeclaration,
};
use crate::resolver::system::SystemDependency;
pub(crate) use crate::resolver::urls::Urls;
use crate::universal_marker::UniversalMarker;
use crate::yanks::AllowedYanks;
use crate::{DependencyMode, Exclusions, FlatIndex, Options, ResolutionMode, VersionMap, marker};
pub(crate) use provider::MetadataUnavailable;
pub(crate) use resolution::{
    Resolution, ResolutionDependencyEdge, ResolutionNode, ResolutionPackage,
};

mod availability;
mod batch_prefetch;
mod derivation;
mod environment;
mod fork_map;
mod index;
mod indexes;
mod package_source;
mod provider;
mod reporter;
mod requests;
mod requirements;
mod resolution;
mod sources;
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
    excludes: Excludes,
    preferences: Preferences,
    git: GitResolver,
    capabilities: IndexCapabilities,
    locations: IndexLocations,
    exclusions: Exclusions,
    urls: Urls,
    source_potentials: Box<HashMap<SourceId, SourcePotential>>,
    indexes: Indexes,
    dependency_mode: DependencyMode,
    dependency_metadata: DependencyMetadata,
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
    // Papaya's maps are large on Windows, so box them to keep resolver futures small.
    /// Incompatibilities for packages that are entirely unavailable from the implicit registry.
    unavailable_packages: Box<HashMap<PackageName, UnavailablePackage>>,
    /// The same lookup failures for each explicitly selected registry.
    unavailable_index_packages: Box<HashMap<IndexId, HashMap<PackageName, UnavailablePackage>>>,
    /// Incompatibilities for packages that are unavailable at specific versions and sources.
    incomplete_packages:
        Box<HashMap<(PackageName, SolverSource), HashMap<Version, MetadataUnavailable>>>,
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
            build_context.locations(),
            build_context.build_options(),
            build_context.capabilities(),
        );

        let mut resolver = Self::new_custom_io(
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
        );
        resolver.state.dependency_metadata = build_context.dependency_metadata().clone();
        Ok(resolver)
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
    ) -> Self {
        let state = ResolverState {
            index: index.clone(),
            git: git.clone(),
            capabilities: capabilities.clone(),
            selector: CandidateSelector::for_resolution(&options, &manifest, &env),
            dependency_mode: options.dependency_mode,
            dependency_metadata: DependencyMetadata::default(),
            urls: Urls::from_manifest(&manifest, &env, options.dependency_mode),
            source_potentials: Box::default(),
            indexes: Indexes::from_manifest(&manifest, &env, options.dependency_mode),
            project: manifest.project,
            workspace_members: manifest.workspace_members,
            requirements: manifest.requirements,
            constraints: manifest.constraints,
            overrides: manifest.overrides,
            excludes: manifest.excludes,
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
            unavailable_packages: Box::default(),
            unavailable_index_packages: Box::default(),
            incomplete_packages: Box::default(),
            options,
            reporter: None,
        };
        Self { state, provider }
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
        self.resolve_with_hashes()
            .await
            .map(|(resolution, _)| resolution)
    }

    /// Resolve requirements and return the hash policy from the paths included in the solution.
    pub async fn resolve_with_hashes(self) -> Result<(ResolverOutput, HashStrategy), ResolveError> {
        let state = Arc::new(self.state);
        let provider = Arc::new(self.provider);

        // A channel to fetch package metadata (e.g., given `flask`, fetch all versions) and version
        // metadata (e.g., given `flask==1.0.0`, fetch the metadata for that version).
        // Channel size is set large to accommodate batch prefetching.
        let (request_sink, request_stream) = mpsc::channel(300);
        let requests = MetadataRequests::new(state.index.clone(), request_sink);

        // Run the fetcher.
        let requests_fut = state.clone().fetch(provider.clone(), request_stream).fuse();

        // Spawn the PubGrub solver on a dedicated thread.
        let solver = state.clone();
        let (tx, rx) = oneshot::channel();
        thread::Builder::new()
            .name("uv-resolver".into())
            .spawn(move || {
                let result = solver.solve(&requests);

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
        requests: &MetadataRequests,
    ) -> Result<(ResolverOutput, HashStrategy), ResolveError> {
        debug!(
            "Solving with installed Python version: {}",
            self.python_requirement.exact()
        );
        debug!(
            "Solving with target Python version: {}",
            self.python_requirement.target()
        );
        if !self.options.exclude_newer.is_empty() {
            debug!("Solving with exclude-newer: {}", self.options.exclude_newer);
        }

        let mut visited = FxHashSet::default();

        let root = PubGrubPackage::from(PubGrubPackageInner::Root(self.project.clone()));
        let pubgrub = State::init(root.clone(), SolverVersion::registry(MIN_VERSION.clone()));
        let prefetcher = BatchPrefetcher::new(self.capabilities.clone(), requests.clone());
        let state = ForkState::new(
            pubgrub,
            self.env.clone(),
            self.python_requirement.clone(),
            prefetcher,
            self.indexes.clone(),
        );
        let mut preferences = self.preferences.clone();
        let mut forked_states = self.env.initial_forked_states(state)?;

        // Apply the same Python-bound scheduling used for dependency-created forks. Since states
        // are popped from the end of the stack, sort lower Python bounds last for `fewest` and
        // higher Python bounds last for `requires-python`. There's no `cmp_upper_bounds` tiebreak
        // here: it counts upper-bounded specifiers among a fork's dependencies, which an initial
        // state doesn't have yet.
        match (self.options.fork_strategy, self.options.resolution_mode) {
            (ForkStrategy::Fewest, _) | (_, ResolutionMode::Lowest) => {
                forked_states.sort_by(|a, b| cmp_requires_python(&a.env, &b.env).reverse());
            }
            (ForkStrategy::RequiresPython, _) => {
                forked_states.sort_by(|a, b| cmp_requires_python(&a.env, &b.env));
            }
        }
        let mut forked_states = forked_states
            .into_iter()
            .map(SourceSearch::new)
            .collect::<Vec<_>>();
        let mut resolutions = vec![];
        let mut active_hashes = ActiveHashes::default();
        let mut active_policies = Vec::new();

        'FORK: while let Some(mut search) = forked_states.pop() {
            let Some(mut state) = search.states.pop() else {
                let source_error = search
                    .source_error
                    .take()
                    .or_else(|| search.failed_source_error())
                    .or_else(|| search.failed_policy_error());
                return Err(source_error
                    .or(search.directory_error)
                    .or(search.error)
                    .or(search.fallback_source_error)
                    .or_else(|| search.candidate_errors.into_values().next())
                    .or_else(|| {
                        search
                            .policy_errors
                            .into_iter()
                            .next()
                            .map(|(_, error)| error)
                    })
                    .expect("an exhausted source search has a failure"));
            };
            if let Some(split) = state.env.end_user_fork_display() {
                let requires_python = state.python_requirement.target();
                debug!("Solving {split} (requires-python: {requires_python:?})");
            }
            let start = Instant::now();
            loop {
                let continuation = mem::take(&mut state.continuation);
                let (highest_priority_pkg, initial_version) = match continuation {
                    ForkContinuation::SelectVersion { package } => (package, None),
                    ForkContinuation::UseVersion { package, version } => (package, Some(version)),
                    ForkContinuation::Propagate => {
                        if !state.directory_metadata_modes.is_empty()
                            && state.source_dependencies.has_contextual_sources()
                        {
                            let grounding = state.source_dependencies.grounding(
                                &state.pubgrub,
                                &state.env,
                                &state.python_requirement,
                                &self.urls,
                                &self.git,
                            );
                            if let Some(marker) =
                                grounding.conditional_source(&state.env, &state.python_requirement)
                                && let Some((with_source, without_source)) =
                                    fork_version_by_marker(&state.env, marker)
                            {
                                for env in [with_source, without_source] {
                                    forked_states.push(SourceSearch::new(self.fresh_source_fork(
                                        env,
                                        SourceAssumptions::default(),
                                        requests,
                                    )));
                                }
                                continue 'FORK;
                            }
                            if grounding.directory_conflicts.is_empty()
                                && let Some((source, editable)) =
                                    state.changed_directory_metadata(&grounding, &self.urls, true)
                            {
                                self.retry_directory_metadata(
                                    &state,
                                    &mut search,
                                    &grounding,
                                    source,
                                    editable,
                                    requests,
                                );
                                forked_states.push(search);
                                continue 'FORK;
                            }
                        }
                        // Run unit propagation.
                        let result = state.pubgrub.unit_propagation(state.next);
                        if state.env.fork_markers().is_some()
                            && state.source_dependencies.has_contextual_sources()
                        {
                            let grounding = state.source_dependencies.grounding(
                                &state.pubgrub,
                                &state.env,
                                &state.python_requirement,
                                &self.urls,
                                &self.git,
                            );
                            if let Some(marker) =
                                grounding.conditional_source(&state.env, &state.python_requirement)
                                && let Some((with_source, without_source)) =
                                    fork_version_by_marker(&state.env, marker)
                            {
                                for env in [with_source, without_source] {
                                    forked_states.push(SourceSearch::new(self.fresh_source_fork(
                                        env,
                                        SourceAssumptions::default(),
                                        requests,
                                    )));
                                }
                                continue 'FORK;
                            }
                        }
                        match result {
                            Err(err) => {
                                // A native conflict exhausts this retry, but other choices may
                                // supply a source that was missing from a previously stalled branch.
                                if search.error.is_none() {
                                    search.source_error = self
                                        .source_conflict(&err, &state)
                                        .or_else(|| search.failure_from_proof(&err));
                                }
                                let grounding = state.source_dependencies.grounding(
                                    &state.pubgrub,
                                    &state.env,
                                    &state.python_requirement,
                                    &self.urls,
                                    &self.git,
                                );
                                let fork_urls = grounding
                                    .iter()
                                    .filter_map(|(name, sources)| {
                                        sources.first_key_value().map(|(source, _)| {
                                            (name.clone(), grounding.url(*source, &self.urls))
                                        })
                                    })
                                    .collect();
                                let mut report_sources = sources_for_report(&err);
                                report_sources.extend(grounding.indexes.iter().filter_map(
                                    |(name, sources)| {
                                        sources.first_key_value().map(|(source, _)| {
                                            (name.clone(), SolverSource::Index(*source))
                                        })
                                    },
                                ));
                                report_sources.extend(grounding.iter().filter_map(
                                    |(name, sources)| {
                                        sources.first_key_value().map(|(source, _)| {
                                            (name.clone(), SolverSource::Url(*source))
                                        })
                                    },
                                ));
                                let fork_indexes = report_sources
                                    .iter()
                                    .filter_map(|(name, source)| match source {
                                        SolverSource::Index(index) => {
                                            Some((name.clone(), state.indexes.resource(*index)))
                                        }
                                        SolverSource::Registry | SolverSource::Url(_) => None,
                                    })
                                    .collect();
                                let known_versions = state
                                    .known_versions
                                    .for_report(&report_sources, &state.indexes);
                                let error = self.convert_no_solution_err(
                                    err,
                                    fork_urls,
                                    fork_indexes,
                                    &report_sources,
                                    &known_versions,
                                    state.env,
                                    self.current_environment.clone(),
                                    &visited,
                                );
                                if search.error.is_none() {
                                    search.error = Some(error);
                                }
                                forked_states.push(search);
                                continue 'FORK;
                            }
                            Ok(conflicts) => {
                                if !conflicts.is_empty() {
                                    state.pending_sources.clear();
                                }
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
                                state.pubgrub.partial_solution.prioritized_packages().map(
                                    |(id, range)| (id, &state.pubgrub.package_store[id], range),
                                ),
                                &mut state.pre_visited,
                                &self.urls,
                                &self.indexes,
                                &state.python_requirement,
                                requests,
                            )?;
                        }

                        state.reprioritize_conflicts();

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
                        let Some(highest_priority_pkg) = state.pick_package() else {
                            let grounding = if self.urls.has_potential()
                                || state.source_dependencies.has_urls()
                                || state.source_dependencies.has_indexes()
                            {
                                state.source_dependencies.grounding(
                                    &state.pubgrub,
                                    &state.env,
                                    &state.python_requirement,
                                    &self.urls,
                                    &self.git,
                                )
                            } else {
                                state.source_dependencies.registry_grounding(
                                    &state.pubgrub,
                                    &state.env,
                                    &state.python_requirement,
                                )
                            };
                            if state.source_dependencies.has_urls()
                                && let Some(marker) = grounding
                                    .conditional_source(&state.env, &state.python_requirement)
                                && let Some((with_source, without_source)) =
                                    fork_version_by_marker(&state.env, marker)
                            {
                                for env in [with_source, without_source] {
                                    forked_states.push(SourceSearch::new(self.fresh_source_fork(
                                        env,
                                        SourceAssumptions::default(),
                                        requests,
                                    )));
                                }
                                continue 'FORK;
                            }
                            let ungrounded = state
                                .pubgrub
                                .partial_solution
                                .extract_solution()
                                .find_map(|(id, candidate)| {
                                    let name = state.pubgrub.package_store[id].name_no_root()?;
                                    let rooted = match candidate.source {
                                        SolverSource::Registry => true,
                                        SolverSource::Index(index) => grounding
                                            .indexes
                                            .get(name)
                                            .is_some_and(|indexes| indexes.contains_key(&index)),
                                        SolverSource::Url(source) => {
                                            grounding.contains(name, source)
                                        }
                                    };
                                    (grounding.reachable.contains(&id) && !rooted).then_some(id)
                                });
                            if let Some(package) = ungrounded {
                                state.pubgrub.backtrack_package(package);
                                state.pending_sources.clear();
                                state.reschedule_sources = true;
                                continue;
                            }
                            if let Some(package) = state.pending_package() {
                                let candidates = state
                                    .pending_sources
                                    .remove(&package)
                                    .expect("the package is pending");
                                let package_name =
                                    state.pubgrub.package_store[package].name_no_root();
                                if let Some((parent, parent_candidate, dependency, source)) =
                                    grounding.untrusted.iter().find(|(_, _, dependency, _)| {
                                        state.pubgrub.package_store[*dependency].name_no_root()
                                            == package_name
                                    })
                                    && let Some(name) =
                                        state.pubgrub.package_store[*dependency].name_no_root()
                                {
                                    let url = self.urls.get(*source);
                                    let error = ResolveError::DisallowedUrl {
                                        name: name.clone(),
                                        url: url.verbatim.to_string(),
                                    };
                                    let error = enrich_dependency_error(
                                        error,
                                        *parent,
                                        parent_candidate,
                                        &state.pubgrub,
                                    );
                                    if urls::git_url(&url.parsed_url).is_some() {
                                        search.record_policy_error(
                                            vec![(
                                                state.pubgrub.package_store[*parent].clone(),
                                                parent_candidate.clone(),
                                            )],
                                            error,
                                        );
                                    } else {
                                        search.record_candidate_error(*source, error);
                                    }
                                }
                                if package_name.is_some_and(|name| {
                                    self.may_supply_url(
                                        name,
                                        &candidates,
                                        &state.env,
                                        &state.python_requirement,
                                    ) || self.may_supply_index(
                                        name,
                                        &candidates,
                                        &state.env,
                                        &state.python_requirement,
                                    )
                                }) {
                                    self.enqueue_source_alternatives(&state, &mut search, requests);
                                }
                                // Keep this branch as an ordinary restrictive solve too: PubGrub
                                // may find a valid alternative that drops the pending package.
                                state.source_assumptions.restrict(
                                    state.pubgrub.package_store[package].clone(),
                                    &candidates.complement(),
                                );
                                search.seen.insert((
                                    state.source_assumptions.clone(),
                                    state.preferred_lowest.clone(),
                                    state.preferred_editable.clone(),
                                ));
                                state.next = package;
                                state
                                    .pubgrub
                                    .add_incompatibility(Incompatibility::no_versions(
                                        package,
                                        Term::Positive(candidates),
                                    ));
                                continue;
                            }
                            let unmatched_git = grounding.untrusted.iter().find_map(
                                |(parent, parent_candidate, dependency, source)| {
                                    let url = self.urls.get(*source);
                                    urls::git_url(&url.parsed_url)?;
                                    let name =
                                        state.pubgrub.package_store[*dependency].name_no_root()?;
                                    state
                                        .pubgrub
                                        .partial_solution
                                        .extract_solution()
                                        .any(|(id, candidate)| {
                                            state.pubgrub.package_store[id].name_no_root()
                                                == Some(name)
                                                && !candidate.source.is_registry()
                                        })
                                        .then(|| {
                                            (*parent, parent_candidate.clone(), name.clone(), url)
                                        })
                                },
                            );
                            if let Some((parent, candidate, name, url)) = unmatched_git {
                                let package = state.pubgrub.package_store[parent].clone();
                                let error = enrich_dependency_error(
                                    ResolveError::DisallowedUrl {
                                        name: name.clone(),
                                        url: url.verbatim.to_string(),
                                    },
                                    parent,
                                    &candidate,
                                    &state.pubgrub,
                                );
                                search.record_policy_error(
                                    vec![(package.clone(), candidate.clone())],
                                    error,
                                );
                                if self.may_supply_url(
                                    &name,
                                    &CandidateSet::urls(Range::full()),
                                    &state.env,
                                    &state.python_requirement,
                                ) {
                                    self.enqueue_source_alternatives(&state, &mut search, requests);
                                }
                                let candidate = CandidateSet::singleton(candidate);
                                state
                                    .source_assumptions
                                    .restrict(package, &candidate.complement());
                                search.seen.insert((
                                    state.source_assumptions.clone(),
                                    state.preferred_lowest.clone(),
                                    state.preferred_editable.clone(),
                                ));
                                state.next = parent;
                                state
                                    .pubgrub
                                    .add_incompatibility(Incompatibility::no_versions(
                                        parent,
                                        Term::Positive(candidate),
                                    ));
                                continue;
                            }
                            if let Some(conflict) = grounding.directory_conflicts.first() {
                                let mut urls = conflict
                                    .origins
                                    .iter()
                                    .map(|(_, _, url)| url.clone())
                                    .collect::<Vec<_>>();
                                urls.sort();
                                let error = ResolveError::ConflictingUrls {
                                    package_name: conflict.name.clone(),
                                    urls,
                                    env: state.env.clone(),
                                };
                                let (parent, candidate, _) = &conflict.origins[1];
                                let error = if let Some(name) =
                                    state.pubgrub.package_store[*parent].name_no_root()
                                    && let Some(chain) =
                                        state.source_dependencies.chain(*parent, candidate)
                                {
                                    ResolveError::Dependencies(
                                        Box::new(error),
                                        name.clone(),
                                        candidate.version.clone(),
                                        chain.clone(),
                                    )
                                } else {
                                    enrich_dependency_error(
                                        error,
                                        *parent,
                                        candidate,
                                        &state.pubgrub,
                                    )
                                };
                                let mut origins = conflict
                                    .origins
                                    .iter()
                                    .map(|(id, candidate, _)| {
                                        (
                                            state.pubgrub.package_store[*id].clone(),
                                            candidate.clone(),
                                        )
                                    })
                                    .collect::<Vec<_>>();
                                origins.dedup();
                                // The native assignment satisfied all other requirements before
                                // this policy invalidated it, so retain that cause across retries.
                                search.directory_error.get_or_insert(error);
                                if origins.len() == 1 {
                                    state.next = *parent;
                                    state.pubgrub.add_incompatibility(
                                        Incompatibility::no_versions(
                                            *parent,
                                            Term::Positive(CandidateSet::singleton(
                                                candidate.clone(),
                                            )),
                                        ),
                                    );
                                    continue;
                                }
                                self.enqueue_policy_alternatives(
                                    &state,
                                    &mut search,
                                    origins,
                                    requests,
                                );
                                forked_states.push(search);
                                continue 'FORK;
                            }
                            if let Some((source, editable)) =
                                state.changed_directory_metadata(&grounding, &self.urls, false)
                            {
                                self.retry_directory_metadata(
                                    &state,
                                    &mut search,
                                    &grounding,
                                    source,
                                    editable,
                                    requests,
                                );
                                forked_states.push(search);
                                continue 'FORK;
                            }
                            let candidate_policy = state
                                .pubgrub
                                .partial_solution
                                .extract_solution()
                                .find_map(|(id, candidate)| {
                                    if !candidate.source.is_registry() {
                                        return None;
                                    }
                                    let name = state.pubgrub.package_store[id].name_no_root()?;
                                    let yanked = state
                                        .possible_candidates
                                        .get(&(id, candidate.clone()))
                                        .copied()
                                        .unwrap_or(false)
                                        || state
                                            .pins
                                            .get(name, &candidate)
                                            .and_then(ResolvedDist::yanked)
                                            .is_some_and(Yanked::is_yanked);
                                    if !candidate.version.any_prerelease() && !yanked {
                                        return None;
                                    }
                                    let needed = *grounding.contexts.get(&id)?;
                                    let selector =
                                        self.selected_candidate_selector(&grounding, name);
                                    if state.env.fork_markers().is_some() {
                                        let permission = state.python_requirement.simplify_markers(
                                            selector.candidate_permission_marker(
                                                name,
                                                &candidate.version,
                                                yanked,
                                            ),
                                        );
                                        if state
                                            .env
                                            .included_by_marker(needed.and(permission.negate()))
                                        {
                                            let split = state
                                                .env
                                                .included_by_marker(needed.and(permission))
                                                .then(|| {
                                                    fork_version_by_marker(&state.env, permission)
                                                })
                                                .flatten();
                                            let prerelease = candidate.version.any_prerelease()
                                                && !state.env.included_by_marker(needed.and(
                                                    selector.candidate_permission_marker(
                                                        name,
                                                        &candidate.version,
                                                        false,
                                                    ),
                                                ));
                                            return Some((id, candidate, split, prerelease));
                                        }
                                        None
                                    } else {
                                        let disallowed = !selector.allows_possible_candidate(
                                            name,
                                            &candidate.version,
                                            yanked,
                                            &state.env,
                                        );
                                        let prerelease = disallowed
                                            && candidate.version.any_prerelease()
                                            && !selector.allows_possible_candidate(
                                                name,
                                                &candidate.version,
                                                false,
                                                &state.env,
                                            );
                                        disallowed.then_some((id, candidate, None, prerelease))
                                    }
                                });
                            if let Some((package, candidate, split, prerelease)) = candidate_policy
                            {
                                if let Some((with_permission, without_permission)) = split {
                                    for env in [with_permission, without_permission] {
                                        forked_states.push(SourceSearch::new(
                                            self.fresh_source_fork(
                                                env,
                                                SourceAssumptions::default(),
                                                requests,
                                            ),
                                        ));
                                    }
                                    continue 'FORK;
                                }
                                self.enqueue_source_alternatives(&state, &mut search, requests);
                                let candidate = CandidateSet::singleton(candidate);
                                state.source_assumptions.restrict(
                                    state.pubgrub.package_store[package].clone(),
                                    &candidate.complement(),
                                );
                                search.seen.insert((
                                    state.source_assumptions.clone(),
                                    state.preferred_lowest.clone(),
                                    state.preferred_editable.clone(),
                                ));
                                state.next = package;
                                let incompatibility = if prerelease {
                                    Incompatibility::custom_term(
                                        package,
                                        Term::Positive(candidate),
                                        UnavailableReason::Version(UnavailableVersion::Prerelease),
                                    )
                                } else {
                                    Incompatibility::no_versions(package, Term::Positive(candidate))
                                };
                                state.pubgrub.add_incompatibility(incompatibility);
                                continue;
                            }
                            if matches!(self.options.resolution_mode, ResolutionMode::LowestDirect)
                            {
                                // Fewest can share the lowest preference where no dependency or
                                // source incompatibility independently forces an environment split.
                                if self.options.fork_strategy == ForkStrategy::RequiresPython
                                    && state.env.fork_markers().is_some()
                                    && let Some((direct, transitive)) = state
                                        .pubgrub
                                        .partial_solution
                                        .extract_solution()
                                        .find_map(|(id, candidate)| {
                                            if !candidate.source.is_registry() {
                                                return None;
                                            }
                                            let package = &state.pubgrub.package_store[id];
                                            let PubGrubPackageInner::Package {
                                                name,
                                                extra: None,
                                                group: None,
                                                ..
                                            } = &**package
                                            else {
                                                return None;
                                            };
                                            let needed = *grounding.contexts.get(&id)?;
                                            let direct = state.python_requirement.simplify_markers(
                                                self.selected_candidate_selector(&grounding, name)
                                                    .lowest_marker(name),
                                            );
                                            if state.env.included_by_marker(needed.and(direct))
                                                && state
                                                    .env
                                                    .included_by_marker(needed.and(direct.negate()))
                                            {
                                                fork_version_by_marker(&state.env, direct)
                                            } else {
                                                None
                                            }
                                        })
                                {
                                    for env in [direct, transitive] {
                                        forked_states.push(SourceSearch::new(
                                            self.fresh_source_fork(
                                                env,
                                                SourceAssumptions::default(),
                                                requests,
                                            ),
                                        ));
                                    }
                                    continue 'FORK;
                                }
                                let mut guarded = BTreeSet::new();
                                let invalid_guard =
                                    state.pubgrub.partial_solution.extract_solution().find_map(
                                        |(id, candidate)| {
                                            let package = &state.pubgrub.package_store[id];
                                            let PubGrubPackageInner::Package {
                                                name,
                                                extra: None,
                                                group: None,
                                                ..
                                            } = &**package
                                            else {
                                                return None;
                                            };
                                            if !state.preferred_lowest.contains(name) {
                                                return None;
                                            }
                                            guarded.insert(name.clone());
                                            self.selected_candidate_selector(&grounding, name)
                                                .use_highest_version(name, &state.env)
                                                .then_some((id, candidate))
                                        },
                                    );
                                if let Some((package, candidate)) = invalid_guard {
                                    state.next = package;
                                    state.pubgrub.add_incompatibility(
                                        Incompatibility::no_versions(
                                            package,
                                            Term::Positive(CandidateSet::singleton(candidate)),
                                        ),
                                    );
                                    continue;
                                }
                                if guarded != state.preferred_lowest {
                                    // This exploratory branch removed a package whose directness was
                                    // needed to prefer its lower version. Keep the existing alternatives.
                                    forked_states.push(search);
                                    continue 'FORK;
                                }

                                let replay = state
                                    .pubgrub
                                    .partial_solution
                                    .extract_solution()
                                    .find_map(|(id, candidate)| {
                                        let package = &state.pubgrub.package_store[id];
                                        let PubGrubPackageInner::Package {
                                            name,
                                            extra: None,
                                            group: None,
                                            ..
                                        } = &**package
                                        else {
                                            return None;
                                        };
                                        if !grounding.reachable.contains(&id)
                                            || state.selection_modes.get(&(id, candidate.clone()))
                                                != Some(&true)
                                            || self
                                                .selected_candidate_selector(&grounding, name)
                                                .use_highest_version(name, &state.env)
                                        {
                                            return None;
                                        }
                                        let mut assumptions = state.source_assumptions.clone();
                                        for (support, selected) in state
                                            .source_dependencies
                                            .lowest_support(&state.pubgrub, &grounding, name)
                                        {
                                            assumptions.restrict(
                                                support,
                                                &CandidateSet::singleton(selected),
                                            );
                                        }
                                        search
                                            .lowest_attempts
                                            .insert(LowestAttempt {
                                                package: package.clone(),
                                                candidate,
                                                assumptions: assumptions.clone(),
                                                preferred: state.preferred_lowest.clone(),
                                                editable: state.preferred_editable.clone(),
                                            })
                                            .then_some((name.clone(), assumptions))
                                    });
                                if let Some((name, assumptions)) = replay {
                                    let mut retry = self.fresh_source_fork(
                                        state.env.clone(),
                                        assumptions,
                                        requests,
                                    );
                                    retry.preferred_lowest.clone_from(&state.preferred_lowest);
                                    retry
                                        .preferred_editable
                                        .clone_from(&state.preferred_editable);
                                    retry.preferred_lowest.insert(name);
                                    search.states.push(state);
                                    search.states.push(retry);
                                    forked_states.push(search);
                                    continue 'FORK;
                                }
                            }
                            // All packages have been assigned, the fork has been successfully resolved
                            if tracing::enabled!(Level::DEBUG) {
                                state.prefetcher.log_tried_versions();
                            }
                            if let Err((source, error)) =
                                self.commit_direct_metadata(&state, &grounding, requests)
                            {
                                let error = *error;
                                if source.is_none()
                                    && let ResolveError::HashStrategy(error) = error
                                {
                                    if self.reject_hash_policy(
                                        &mut state,
                                        &mut search,
                                        &grounding,
                                        error,
                                        requests,
                                    ) {
                                        continue;
                                    }
                                    forked_states.push(search);
                                    continue 'FORK;
                                }
                                if is_source_error(&error) {
                                    if let Some(source) = source {
                                        search.record_candidate_error(source, error);
                                    } else {
                                        search.fallback_source_error.get_or_insert(error);
                                    }
                                    self.enqueue_source_alternatives(&state, &mut search, requests);
                                    forked_states.push(search);
                                    continue 'FORK;
                                }
                                return Err(error);
                            }
                            debug!(
                                "{} resolution took {:.3}s",
                                state.env,
                                start.elapsed().as_secs_f32()
                            );

                            let resolution = state.into_resolution(&self.urls, &grounding);
                            active_policies.extend(grounding.policies);
                            active_hashes.extend(grounding.hashes);

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
                                let marker = resolution
                                    .env
                                    .try_universal_markers()
                                    .unwrap_or(UniversalMarker::TRUE);
                                for (package, version) in &resolution.nodes {
                                    preferences.insert(
                                        package.name.clone(),
                                        package.index.clone(),
                                        marker,
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

                        (highest_priority_pkg, None)
                    }
                };

                state.next = highest_priority_pkg;

                // TODO(charlie): Remove as many usages of `next_package` as we can.
                let next_id = state.next;
                let next_package = &state.pubgrub.package_store[state.next];

                let candidates = state
                    .pubgrub
                    .partial_solution
                    .term_intersection_for_package(next_id)
                    .expect("a package was chosen but we don't have a term")
                    .unwrap_positive()
                    .clone();
                let grounding = if self.urls.has_potential() || state.source_dependencies.has_urls()
                {
                    state.source_dependencies.grounding(
                        &state.pubgrub,
                        &state.env,
                        &state.python_requirement,
                        &self.urls,
                        &self.git,
                    )
                } else {
                    Grounding::default()
                };
                let mut policies = grounding
                    .policies
                    .iter()
                    .filter(|(requirement, _)| {
                        next_package.name_no_root() == Some(&requirement.name)
                    })
                    .map(|(requirement, lowest)| (requirement.as_ref(), *lowest))
                    .peekable();
                let mut selector = if policies.peek().is_some() {
                    Cow::Owned(self.selector.with_requirements(policies))
                } else {
                    Cow::Borrowed(&self.selector)
                };
                if let Some(name) = next_package.name_no_root()
                    && state.preferred_lowest.contains(name)
                {
                    selector.to_mut().prefer_lowest(name);
                }
                let solver_source = initial_version.as_ref().map_or_else(
                    || {
                        if matches!(
                            &**next_package,
                            PubGrubPackageInner::Root(_)
                                | PubGrubPackageInner::Python(_)
                                | PubGrubPackageInner::System(_)
                        ) {
                            return SolverSource::Registry;
                        }
                        next_package
                            .name_no_root()
                            .and_then(|name| grounding.source(name, &candidates))
                            .map(SolverSource::Url)
                            .or_else(|| candidates.index().map(SolverSource::Index))
                            .unwrap_or(SolverSource::Registry)
                    },
                    |candidate| candidate.source,
                );
                let range = candidates.for_source(solver_source);
                if *range == Range::empty() {
                    if next_package.name_no_root().is_some_and(|name| {
                        (candidates.has_urls()
                            && (self.may_supply_url(
                                name,
                                &candidates,
                                &state.env,
                                &state.python_requirement,
                            ) || grounding.untrusted.iter().any(|(_, _, dependency, _)| {
                                state.pubgrub.package_store[*dependency].name_no_root()
                                    == Some(name)
                            })))
                            || (candidates.has_indexes()
                                && self.may_supply_index(
                                    name,
                                    &candidates,
                                    &state.env,
                                    &state.python_requirement,
                                ))
                    }) {
                        state.pending_sources.insert(next_id, candidates);
                        state.reschedule_sources = true;
                    } else {
                        state
                            .pubgrub
                            .add_incompatibility(Incompatibility::no_versions(
                                next_id,
                                Term::Positive(candidates),
                            ));
                    }
                    continue;
                }
                let url = match solver_source {
                    SolverSource::Registry | SolverSource::Index(_) => None,
                    SolverSource::Url(source) => {
                        Some(grounding.metadata_url(source, &self.urls, &state.preferred_editable))
                    }
                };
                let index = if let SolverSource::Index(index) = solver_source {
                    Some(self.indexes.resource(index))
                } else {
                    None
                };
                let source = match &url {
                    Some(url) => PackageSource::Url(url),
                    None => PackageSource::Registry(index.as_ref()),
                };

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
                let hasher = match grounding.hashes.strategy(&self.hasher) {
                    Ok(hasher) => hasher,
                    Err(error) => {
                        if self.reject_hash_policy(
                            &mut state,
                            &mut search,
                            &grounding,
                            error,
                            requests,
                        ) {
                            continue;
                        }
                        forked_states.push(search);
                        continue 'FORK;
                    }
                };
                if let Err(error) = Self::request_package(next_package, source, &hasher, requests) {
                    if let SolverSource::Url(url_source) = solver_source
                        && is_source_error(&error)
                    {
                        self.reject_source_candidate(
                            &mut state,
                            &mut search,
                            next_id,
                            &candidates,
                            url_source,
                            error,
                        );
                        continue;
                    }
                    return Err(error);
                }

                let version = if let Some(version) = initial_version {
                    version
                } else {
                    // Within a fixed resolver environment, an implicit registry candidate is
                    // stable for a given range and pre-release policy. Avoid repeating candidate
                    // selection when PubGrub revisits an identical decision after backtracking.
                    let cache_selected_version = match source {
                        PackageSource::Registry(None) => true,
                        PackageSource::Url(_) | PackageSource::Registry(Some(_)) => false,
                    };
                    let policy = (cache_selected_version && self.urls.has_potential())
                        .then(|| {
                            next_package
                                .name_no_root()
                                .map(|name| selector.selection_policy(name, &state.env))
                        })
                        .flatten();
                    let decision = if cache_selected_version
                        && let Some((selected_range, selected_policy, version)) =
                            state.selected_versions.get(&next_id)
                        && selected_range == range
                        && selected_policy == &policy
                    {
                        Some(ResolverVersion::Unforked(version.clone()))
                    } else {
                        let mut decision = match self.choose_version(
                            next_package,
                            next_id,
                            source,
                            range,
                            &mut state.pins,
                            &preferences,
                            &state.env,
                            &state.python_requirement,
                            &state.pubgrub,
                            &mut visited,
                            &selector,
                            &hasher,
                            requests,
                        ) {
                            Ok(decision) => decision,
                            Err(error) => {
                                if let SolverSource::Url(url_source) = solver_source
                                    && is_source_error(&error)
                                {
                                    self.reject_source_candidate(
                                        &mut state,
                                        &mut search,
                                        next_id,
                                        &candidates,
                                        url_source,
                                        error,
                                    );
                                    continue;
                                }
                                return Err(error);
                            }
                        };

                        if let PackageSource::Registry(_) = source
                            && let Some(name) = next_package.name_no_root()
                            && (decision.is_none()
                                || possible_yanked_version(decision.as_ref()).is_some())
                            && self.may_supply_selection_policy(
                                name,
                                &state.env,
                                &state.python_requirement,
                            )
                        {
                            let mut yanked = possible_yanked_version(decision.as_ref()).cloned();
                            let possible_selector =
                                selector.with_possible_policy(name, yanked.as_ref());
                            let mut possible = self.choose_version(
                                next_package,
                                next_id,
                                source,
                                range,
                                &mut state.pins,
                                &preferences,
                                &state.env,
                                &state.python_requirement,
                                &state.pubgrub,
                                &mut visited,
                                &possible_selector,
                                &hasher,
                                requests,
                            )?;
                            if yanked.is_none()
                                && let Some(version) = possible_yanked_version(possible.as_ref())
                            {
                                yanked = Some(version.clone());
                                let possible_selector =
                                    selector.with_possible_policy(name, yanked.as_ref());
                                possible = self.choose_version(
                                    next_package,
                                    next_id,
                                    source,
                                    range,
                                    &mut state.pins,
                                    &preferences,
                                    &state.env,
                                    &state.python_requirement,
                                    &state.pubgrub,
                                    &mut visited,
                                    &possible_selector,
                                    &hasher,
                                    requests,
                                )?;
                            }
                            let mut remember = |id, version: &Version| {
                                state
                                    .possible_candidates
                                    .entry((id, SolverVersion::new(solver_source, version.clone())))
                                    .and_modify(|is_yanked| {
                                        *is_yanked |= yanked.as_ref() == Some(version);
                                    })
                                    .or_insert_with(|| yanked.as_ref() == Some(version));
                            };
                            match &possible {
                                Some(ResolverVersion::Unforked(version)) => {
                                    remember(next_id, version);
                                }
                                Some(ResolverVersion::Forked(forks)) => {
                                    for fork in forks {
                                        if let Some(version) = &fork.version {
                                            remember(fork.id, version);
                                        }
                                    }
                                }
                                Some(ResolverVersion::Unavailable(..)) | None => {}
                            }
                            if possible.is_some() {
                                decision = possible;
                            }
                        }

                        if cache_selected_version
                            && let Some(ResolverVersion::Unforked(version)) = &decision
                        {
                            state
                                .selected_versions
                                .insert(next_id, (range.clone(), policy, version.clone()));
                        }

                        decision
                    };

                    // Pick the next compatible version.
                    let Some(version) = decision else {
                        debug!("No compatible version found for: {next_package}");

                        let ruled_out = if solver_source == SolverSource::Registry
                            && next_package.name_no_root().is_none_or(|name| {
                                !self.may_supply_url(
                                    name,
                                    &candidates,
                                    &state.env,
                                    &state.python_requirement,
                                ) && !self.may_supply_index(
                                    name,
                                    &candidates,
                                    &state.env,
                                    &state.python_requirement,
                                )
                            }) {
                            candidates.clone()
                        } else {
                            CandidateSet::source(solver_source, range.clone())
                        };

                        if solver_source.is_registry()
                            && let PubGrubPackageInner::Package { name, .. } = &**next_package
                        {
                            // Check if the decision was due to the package being unavailable
                            if let Some(reason) = self.unavailable_package(name, solver_source) {
                                state
                                    .pubgrub
                                    .add_incompatibility(Incompatibility::custom_term(
                                        next_id,
                                        Term::Positive(ruled_out),
                                        UnavailableReason::Package(reason),
                                    ));
                                continue;
                            }
                        }

                        state
                            .pubgrub
                            .add_incompatibility(Incompatibility::no_versions(
                                next_id,
                                Term::Positive(ruled_out),
                            ));
                        continue;
                    };

                    let version = match version {
                        ResolverVersion::Unforked(version) => version,
                        ResolverVersion::Forked(forks) => {
                            forked_states.extend(
                                self.version_forks_to_fork_states(state, forks, solver_source)
                                    .map(|state| self.environmental_source_search(state, requests)),
                            );
                            continue 'FORK;
                        }
                        ResolverVersion::Unavailable(version, reason) => {
                            state.add_unavailable_version(
                                SolverVersion::new(solver_source, version),
                                reason,
                                &self.index,
                                &self.installed_packages,
                            );
                            continue;
                        }
                    };

                    // Only consider registry packages for prefetch.
                    if let PackageSource::Registry(index) = source {
                        let unchanging = state
                            .pubgrub
                            .partial_solution
                            .unchanging_term_for_package(next_id)
                            .map(|term| match term {
                                Term::Positive(candidates) => {
                                    Term::Positive(candidates.for_source(solver_source).clone())
                                }
                                Term::Negative(candidates) => {
                                    Term::Negative(candidates.for_source(solver_source).clone())
                                }
                            });
                        state.prefetcher.prefetch_batches(
                            next_package,
                            index,
                            &version,
                            range,
                            unchanging.as_ref(),
                            &state.python_requirement,
                            &self.selector,
                            &state.env,
                        )?;
                    }

                    SolverVersion::new(solver_source, version)
                };

                if matches!(self.options.resolution_mode, ResolutionMode::LowestDirect)
                    && version.source.is_registry()
                    && let Some(name) = next_package.name_no_root()
                {
                    state.selection_modes.insert(
                        (next_id, version.clone()),
                        selector.use_highest_version(name, &state.env),
                    );
                }
                state
                    .prefetcher
                    .version_tried(next_package, &version.version);

                self.on_progress(next_package, &version.version);

                if state
                    .added_dependencies
                    .get(&next_id)
                    .is_some_and(|versions| versions.contains(&version))
                {
                    // `dep_incompats` are already in `incompatibilities` so we know there are not satisfied
                    // terms and can add the decision directly.
                    state.pending_sources.clear();
                    state
                        .pubgrub
                        .partial_solution
                        .add_decision(next_id, version);
                    continue;
                }

                // Retrieve that package dependencies.
                let forked_deps = match self.get_dependencies_forking(
                    next_id,
                    next_package,
                    &version,
                    &state.pins,
                    &grounding,
                    &state.preferred_editable,
                    &state.env,
                    &state.python_requirement,
                    &state.pubgrub,
                    requests,
                ) {
                    Ok(dependencies) => dependencies,
                    Err(error) => {
                        if let SolverSource::Url(url_source) = solver_source
                            && is_source_error(&error)
                        {
                            self.reject_source_candidate(
                                &mut state,
                                &mut search,
                                next_id,
                                &candidates,
                                url_source,
                                error,
                            );
                            continue;
                        }
                        return Err(error);
                    }
                };

                if let SolverSource::Url(source) = solver_source
                    && matches!(&**next_package, PubGrubPackageInner::Package { .. })
                    && let Some(VerbatimParsedUrl {
                        parsed_url: ParsedUrl::Directory(directory),
                        ..
                    }) = &url
                {
                    let (used_normal, used_editable) =
                        state.directory_metadata_modes.entry(source).or_default();
                    if directory.editable.unwrap_or(false) {
                        *used_editable = true;
                    } else {
                        *used_normal = true;
                    }
                }

                match forked_deps {
                    ForkedDependencies::Unavailable(reason) => {
                        // Then here, if we get a reason that we consider unrecoverable, we should
                        // show the derivation chain.
                        let versions = state.widen_version_to_gap(
                            &version,
                            &self.index,
                            &self.installed_packages,
                        );
                        state
                            .pubgrub
                            .add_incompatibility(Incompatibility::custom_term(
                                next_id,
                                Term::Positive(versions),
                                UnavailableReason::Version(reason),
                            ));
                    }
                    ForkedDependencies::Unforked(dependencies) => {
                        state
                            .added_dependencies
                            .entry(next_id)
                            .or_default()
                            .insert(version.clone());

                        state.visit_package_version_dependencies(
                            next_id,
                            &version,
                            &dependencies,
                            &self.workspace_members,
                            self.selector.resolution_strategy(),
                        );

                        // Emit a request to fetch the metadata for each registry package.
                        self.visit_dependencies(&dependencies, requests)
                            .map_err(|err| {
                                enrich_dependency_error(err, next_id, &version, &state.pubgrub)
                            })?;

                        self.prepare_git_dependencies(&dependencies, &grounding, requests)?;

                        // Add the dependencies to the state.
                        state.add_package_version_dependencies(
                            next_id,
                            &version,
                            dependencies,
                            &self.urls,
                            &self.git,
                            &self.index,
                            &self.installed_packages,
                        );
                    }
                    ForkedDependencies::Forked {
                        mut forks,
                        diverging_packages,
                        replay_candidates,
                    } => {
                        state
                            .added_dependencies
                            .entry(next_id)
                            .or_default()
                            .insert(version.clone());

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

                        if replay_candidates {
                            // Other selected packages may already have contributed marked edges
                            // before this declaration was discovered. Replay the root under each
                            // narrower environment so those edges and their authority are recomputed.
                            forked_states.extend(forks.into_iter().map(|fork| {
                                SourceSearch::new(self.fresh_source_fork(
                                    fork.env,
                                    SourceAssumptions::default(),
                                    requests,
                                ))
                            }));
                            continue 'FORK;
                        }
                        for new_fork_state in self.forks_to_fork_states(
                            state,
                            &version,
                            forks,
                            requests,
                            &diverging_packages,
                        ) {
                            forked_states
                                .push(self.environmental_source_search(new_fork_state?, requests));
                        }
                        continue 'FORK;
                    }
                    ForkedDependencies::RequiresPython(requires_python) => {
                        if matches!(self.options.fork_strategy, ForkStrategy::RequiresPython)
                            && state.env.marker_environment().is_none()
                        {
                            let forks = fork_version_by_python_requirement(
                                &requires_python,
                                &state.python_requirement,
                                &state.env,
                            );
                            if !forks.is_empty() {
                                debug!(
                                    "Forking Python requirement `{}` on `{}` for {}=={} ({})",
                                    state.python_requirement.target(),
                                    &requires_python,
                                    next_package,
                                    version,
                                    forks
                                        .iter()
                                        .map(ToString::to_string)
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                );

                                // Revisit the version in each fork so its dependencies are added
                                // under the narrowed Python requirement.
                                let forks = forks
                                    .into_iter()
                                    .map(|env| VersionFork {
                                        env,
                                        id: next_id,
                                        version: None,
                                    })
                                    .collect();
                                forked_states.extend(
                                    self.version_forks_to_fork_states(state, forks, version.source)
                                        .map(|state| {
                                            self.environmental_source_search(state, requests)
                                        }),
                                );
                                continue 'FORK;
                            }
                        }

                        let versions = state.widen_version_to_gap(
                            &version,
                            &self.index,
                            &self.installed_packages,
                        );
                        state
                            .pubgrub
                            .add_incompatibility(Incompatibility::custom_term(
                                next_id,
                                Term::Positive(versions),
                                UnavailableReason::Version(UnavailableVersion::RequiresPython(
                                    requires_python,
                                )),
                            ));
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
            resolution.trace_resolution();
        }
        let hasher = active_hashes.strategy(&self.hasher)?;
        let selector = self.selector.with_requirements(
            active_policies
                .iter()
                .map(|(requirement, lowest)| (requirement.as_ref(), *lowest)),
        );
        let resolution = crate::resolution::from_state(
            &resolutions,
            self.project.as_ref(),
            &self.workspace_members,
            self.requirements.clone(),
            self.constraints.clone(),
            self.overrides.clone(),
            &self.preferences,
            &hasher,
            &self.index,
            &self.git,
            self.python_requirement.target().clone(),
            &self.conflicts,
            selector.resolution_strategy(),
            self.options.clone(),
        )?;
        Ok((resolution, hasher))
    }

    /// Apply candidate policies from selected first-party paths for one package.
    fn selected_candidate_selector(
        &self,
        grounding: &Grounding,
        name: &PackageName,
    ) -> CandidateSelector {
        self.selector
            .with_requirements(
                grounding
                    .policies
                    .iter()
                    .filter_map(|(requirement, lowest)| {
                        (&requirement.name == name).then_some((requirement.as_ref(), *lowest))
                    }),
            )
    }

    /// Recheck every selected direct distribution against the completed fork's hash declarations.
    /// A later surviving parent may have added a digest after the candidate's initial fetch.
    fn commit_direct_metadata(
        &self,
        state: &ForkState,
        grounding: &Grounding,
        requests: &MetadataRequests,
    ) -> Result<(), (Option<SourceId>, Box<ResolveError>)> {
        let hasher = grounding
            .hashes
            .strategy(&self.hasher)
            .map_err(|error| (None, Box::new(error.into())))?;
        let mut direct = FxHashSet::default();
        for (id, candidate) in state.pubgrub.partial_solution.extract_solution() {
            let SolverSource::Url(source) = candidate.source else {
                continue;
            };
            if !grounding.reachable.contains(&id) || !direct.insert(source) {
                continue;
            }
            let package = &state.pubgrub.package_store[id];
            let Some(name) = package.name_no_root() else {
                continue;
            };
            let result = (|| {
                let url = grounding.url(source, &self.urls);
                Self::request_package(package, PackageSource::Url(&url), &hasher, requests)?;
                let allowed = hasher.allows_url(&url.verbatim);
                let dist = Dist::from_url(name.clone(), url)?;
                let metadata = requests.wait_for_direct(&dist, &hasher)?;
                match metadata.as_ref() {
                    MetadataResponse::Found(_) => {
                        if !allowed {
                            return Err(ResolveError::UnhashedPackage(name.clone()));
                        }
                        self.index.commit_direct(dist.distribution_id(), metadata);
                    }
                    MetadataResponse::Unavailable(_) => {
                        return Err(ResolveError::PackageUnavailable(name.clone()));
                    }
                    MetadataResponse::Error(dist, error) => {
                        return Err(ResolveError::Dist(
                            DistErrorKind::from_requested_dist(dist, error.as_ref()),
                            dist.clone(),
                            DerivationChainBuilder::from_state(id, &candidate, &state.pubgrub)
                                .unwrap_or_default(),
                            error.clone(),
                        ));
                    }
                }
                Ok(())
            })();
            result.map_err(|error| (Some(source), Box::new(error)))?;
        }
        Ok(())
    }

    /// Restart a stalled source branch under optional restrictions on previously chosen candidates.
    fn fresh_source_fork(
        &self,
        env: ResolverEnvironment,
        assumptions: SourceAssumptions,
        requests: &MetadataRequests,
    ) -> ForkState {
        let root = PubGrubPackage::from(PubGrubPackageInner::Root(self.project.clone()));
        let pubgrub = State::init(root, SolverVersion::registry(MIN_VERSION.clone()));
        let prefetcher = BatchPrefetcher::new(self.capabilities.clone(), requests.clone());
        let mut state = ForkState::new(
            pubgrub,
            self.env.clone(),
            self.python_requirement.clone(),
            prefetcher,
            self.indexes.clone(),
        )
        .with_env(env);
        for (package, allowed) in assumptions.iter() {
            let package = state.pubgrub.package_store.alloc(package.clone());
            state
                .pubgrub
                .add_incompatibility(Incompatibility::no_versions(
                    package,
                    Term::Positive(allowed.complement()),
                ));
        }
        state.source_assumptions = assumptions;
        state
    }

    /// Cover every change to the concrete selections in a stalled branch without requiring any
    /// package that might be absent from an alternative solution.
    fn enqueue_source_alternatives(
        &self,
        state: &ForkState,
        search: &mut SourceSearch,
        requests: &MetadataRequests,
    ) {
        let choices = state
            .pubgrub
            .partial_solution
            .extract_solution()
            .filter_map(|(id, candidate)| {
                let package = &state.pubgrub.package_store[id];
                package.name_no_root().map(|_| (package.clone(), candidate))
            });
        self.enqueue_policy_alternatives(state, search, choices, requests);
    }

    /// Reconsider only the candidates sufficient to make a selected-path policy invalid. Every
    /// alternative permits a candidate's package to disappear if it is no longer required.
    fn enqueue_policy_alternatives(
        &self,
        state: &ForkState,
        search: &mut SourceSearch,
        choices: impl IntoIterator<Item = (PubGrubPackage, SolverVersion)>,
        requests: &MetadataRequests,
    ) {
        self.enqueue_policy_alternatives_with_editable(
            state,
            search,
            choices,
            &state.preferred_editable,
            requests,
        );
    }

    /// Explore changed policy authors under the given directory-metadata expectations.
    fn enqueue_policy_alternatives_with_editable(
        &self,
        state: &ForkState,
        search: &mut SourceSearch,
        choices: impl IntoIterator<Item = (PubGrubPackage, SolverVersion)>,
        preferred_editable: &BTreeSet<SourceId>,
        requests: &MetadataRequests,
    ) {
        for assumptions in state.source_assumptions.alternatives(choices) {
            if search.seen.insert((
                assumptions.clone(),
                state.preferred_lowest.clone(),
                preferred_editable.clone(),
            )) {
                let mut retry = self.fresh_source_fork(state.env.clone(), assumptions, requests);
                retry.preferred_lowest.clone_from(&state.preferred_lowest);
                retry.preferred_editable.clone_from(preferred_editable);
                search.states.push(retry);
            }
        }
    }

    /// Replay a directory with its selected build mode. Keep ordinary alternatives where the
    /// declarations requesting editable metadata are changed or no longer included in the solve.
    fn retry_directory_metadata(
        &self,
        state: &ForkState,
        search: &mut SourceSearch,
        grounding: &Grounding,
        source: SourceId,
        editable: bool,
        requests: &MetadataRequests,
    ) {
        let mut fallback = state.preferred_editable.clone();
        fallback.remove(&source);
        let origins = grounding
            .directory_editable_origins(source)
            .map(|(id, candidate)| (state.pubgrub.package_store[*id].clone(), candidate.clone()));
        self.enqueue_policy_alternatives_with_editable(state, search, origins, &fallback, requests);

        let mut preferred = fallback;
        if editable {
            preferred.insert(source);
        }
        let assumptions = state.source_assumptions.clone();
        if search.seen.insert((
            assumptions.clone(),
            state.preferred_lowest.clone(),
            preferred.clone(),
        )) {
            let mut retry = self.fresh_source_fork(state.env.clone(), assumptions, requests);
            retry.preferred_lowest.clone_from(&state.preferred_lowest);
            retry.preferred_editable = preferred;
            search.states.push(retry);
        }
    }

    /// Keep policy-dependent failures pending in case another selected path supplies trusted hashes.
    /// Other direct retrieval errors reject only the specific source and allow ordinary backtracking.
    fn reject_source_candidate(
        &self,
        state: &mut ForkState,
        search: &mut SourceSearch,
        id: Id<PubGrubPackage>,
        candidates: &CandidateSet,
        source: SourceId,
        error: ResolveError,
    ) {
        let policy_dependent = is_hash_source_error(&error);
        search.record_candidate_error(source, error);
        if policy_dependent {
            state.pending_sources.insert(id, candidates.clone());
            state.reschedule_sources = true;
        } else {
            self.source_potentials
                .pin()
                .get_or_insert(source, SourcePotential::Unavailable);
            state
                .pubgrub
                .add_incompatibility(Incompatibility::no_versions(
                    id,
                    Term::Positive(CandidateSet::source(
                        SolverSource::Url(source),
                        Range::full(),
                    )),
                ));
        }
    }

    /// Exclude a candidate that independently creates an invalid hash policy. A policy that needs
    /// multiple authors instead requires reconsidering their selections together.
    fn reject_hash_policy(
        &self,
        state: &mut ForkState,
        search: &mut SourceSearch,
        grounding: &Grounding,
        error: HashStrategyError,
        requests: &MetadataRequests,
    ) -> bool {
        let origins = grounding.hash_error_origins(&state.pubgrub, &self.hasher, &error);
        search.record_policy_error(
            origins
                .iter()
                .map(|(id, candidate)| {
                    (state.pubgrub.package_store[*id].clone(), candidate.clone())
                })
                .collect(),
            error.into(),
        );
        if let [(id, candidate)] = origins.as_slice() {
            state.next = *id;
            state
                .pubgrub
                .add_incompatibility(Incompatibility::no_versions(
                    *id,
                    Term::Positive(CandidateSet::singleton(candidate.clone())),
                ));
            true
        } else {
            self.enqueue_source_alternatives(state, search, requests);
            false
        }
    }

    /// Whether an initial or as-yet-uninspected direct resource could supply an allowed URL for
    /// this package. Inspected metadata includes every extra but never grants source authority.
    fn may_supply_url(
        &self,
        name: &PackageName,
        candidates: &CandidateSet,
        env: &ResolverEnvironment,
        python_requirement: &PythonRequirement,
    ) -> bool {
        let python_marker = python_requirement.to_marker_tree();
        let mut pending = VecDeque::from(self.urls.initial().to_vec());
        let mut seen = FxHashSet::default();
        while let Some(requirement) = pending.pop_front() {
            let marker = requirement.marker.without_extras();
            if python_marker.is_disjoint(marker)
                || !env.included_by_marker(marker.and(python_marker))
                || env
                    .marker_environment()
                    .is_some_and(|environment| !marker.evaluate(environment, &[]))
            {
                continue;
            }
            let Some(url) = requirement.source.to_verbatim_parsed_url() else {
                continue;
            };
            let sources = self.urls.lookup(&requirement.name, &url, &self.git);
            let known = sources
                .iter()
                .map(|source| self.source_potentials.pin().get(source).cloned())
                .collect::<Vec<_>>();
            let metadata = known.iter().find_map(|known| match known {
                Some(SourcePotential::Metadata {
                    version,
                    dependencies,
                    ..
                }) => Some((version, dependencies)),
                Some(SourcePotential::Unavailable) | None => None,
            });
            let unknown = sources.is_empty() || known.iter().any(Option::is_none);
            if &requirement.name == name {
                if let Some((version, _)) = metadata {
                    if sources.iter().any(|source| {
                        candidates
                            .for_source(SolverSource::Url(*source))
                            .contains(version)
                    }) {
                        return true;
                    }
                } else if unknown
                    && ((sources.is_empty() && candidates.allows_unseen_url())
                        || sources.iter().zip(&known).any(|(source, known)| {
                            known.is_none()
                                && *candidates.for_source(SolverSource::Url(*source))
                                    != Range::empty()
                        }))
                {
                    return true;
                }
            }
            if self.dependency_mode.is_direct() {
                continue;
            }
            let previous = seen.len();
            seen.extend(sources.iter().copied());
            let newly_seen = seen.len() != previous;
            if !sources.is_empty() && !newly_seen {
                continue;
            }
            if let Some((_, dependencies)) = metadata {
                pending.extend(dependencies.iter().cloned());
            } else if unknown {
                return true;
            }
        }
        false
    }

    /// Whether an initial index or an uninspected first-party source can supply an explicit index.
    fn may_supply_index(
        &self,
        name: &PackageName,
        candidates: &CandidateSet,
        env: &ResolverEnvironment,
        python_requirement: &PythonRequirement,
    ) -> bool {
        if !candidates.has_indexes() {
            return false;
        }
        if self.indexes.get(name, env).into_iter().any(|index| {
            *candidates.for_source(SolverSource::Index(self.indexes.intern(index)))
                != Range::empty()
        }) {
            return true;
        }
        let python_marker = python_requirement.to_marker_tree();
        let mut pending = VecDeque::from(self.urls.initial().to_vec());
        let mut seen = FxHashSet::default();
        while let Some(requirement) = pending.pop_front() {
            let marker = requirement.marker.without_extras();
            if python_marker.is_disjoint(marker)
                || !env.included_by_marker(marker.and(python_marker))
                || env
                    .marker_environment()
                    .is_some_and(|environment| !marker.evaluate(environment, &[]))
            {
                continue;
            }
            if let RequirementSource::Registry {
                index: Some(index), ..
            } = &requirement.source
            {
                if &requirement.name == name
                    && *candidates.for_source(SolverSource::Index(self.indexes.intern(index)))
                        != Range::empty()
                {
                    return true;
                }
                continue;
            }
            let Some(url) = requirement.source.to_verbatim_parsed_url() else {
                continue;
            };
            if self.dependency_mode.is_direct() {
                continue;
            }
            let sources = self.urls.lookup(&requirement.name, &url, &self.git);
            if sources.is_empty() {
                return true;
            }
            let previous = seen.len();
            seen.extend(sources.iter().copied());
            if seen.len() == previous {
                continue;
            }
            let potentials = self.source_potentials.pin();
            let known = sources
                .iter()
                .map(|source| potentials.get(source))
                .collect::<Vec<_>>();
            if let Some(dependencies) = known.iter().find_map(|known| match known {
                Some(SourcePotential::Metadata { dependencies, .. }) => Some(dependencies),
                Some(SourcePotential::Unavailable) | None => None,
            }) {
                pending.extend(dependencies.iter().cloned());
            } else if known.iter().any(Option::is_none) {
                return true;
            }
        }
        false
    }

    /// Whether metadata the resolver has already obtained can still opt a registry candidate in.
    /// The actual declaration must be selected before any speculative candidate can be accepted.
    fn may_supply_selection_policy(
        &self,
        name: &PackageName,
        env: &ResolverEnvironment,
        python_requirement: &PythonRequirement,
    ) -> bool {
        if self.dependency_mode.is_direct() {
            return false;
        }
        let python_marker = python_requirement.to_marker_tree();
        let possible_marker = |marker: MarkerTree| {
            let marker = marker.without_extras();
            !python_marker.is_disjoint(marker)
                && env.included_by_marker(marker.and(python_marker))
                && env
                    .marker_environment()
                    .is_none_or(|environment| marker.evaluate(environment, &[]))
        };
        let mut pending = VecDeque::from(self.urls.initial().to_vec());
        let mut seen = FxHashSet::default();
        while let Some(requirement) = pending.pop_front() {
            if !possible_marker(requirement.marker) {
                continue;
            }
            let Some(url) = requirement.source.to_verbatim_parsed_url() else {
                continue;
            };
            let sources = self.urls.lookup(&requirement.name, &url, &self.git);
            if sources.is_empty() {
                return true;
            }
            let previous = seen.len();
            seen.extend(sources.iter().copied());
            let newly_seen = seen.len() != previous;
            if !newly_seen {
                continue;
            }
            let potentials = self.source_potentials.pin();
            let known = sources
                .iter()
                .map(|source| potentials.get(source))
                .collect::<Vec<_>>();
            if let Some((dependencies, policies)) = known.iter().find_map(|known| match known {
                Some(SourcePotential::Metadata {
                    dependencies,
                    policies,
                    ..
                }) => Some((dependencies, policies)),
                Some(SourcePotential::Unavailable) | None => None,
            }) {
                if policies
                    .iter()
                    .any(|policy| &policy.name == name && possible_marker(policy.marker))
                {
                    return true;
                }
                pending.extend(dependencies.iter().cloned());
            } else if known.iter().any(Option::is_none) {
                return true;
            }
        }
        false
    }

    /// Record all potentially applicable direct edges from fetched metadata. Directory build modes
    /// may supply different dependencies; either can expose a possibility, but never authorize it.
    fn remember_source_metadata(&self, source: SourceId, metadata: &Metadata) {
        let potentials = self.source_potentials.pin();
        let directory = matches!(self.urls.get(source).parsed_url, ParsedUrl::Directory(_));
        if !directory && potentials.contains_key(&source) {
            return;
        }
        let requirements = self.overrides.apply_for(
            &metadata.name,
            &metadata.version,
            metadata.requires_dist.as_ref(),
        );
        let groups = metadata
            .dependency_groups
            .values()
            .flat_map(|group| self.overrides.apply(group.as_ref()));
        let mut dependencies = Vec::new();
        let mut policies = Vec::new();
        for requirement in requirements.chain(groups).filter(|requirement| {
            !self
                .excludes
                .contains_for_package(Some((&metadata.name, &metadata.version)), &requirement.name)
        }) {
            if requirement.source.to_verbatim_parsed_url().is_some()
                || matches!(
                    &requirement.source,
                    RequirementSource::Registry { index: Some(_), .. }
                )
            {
                dependencies.push(requirement.as_ref().clone());
            }
            if let RequirementSource::Registry { specifier, .. } = &requirement.source
                && (contains_prerelease(specifier)
                    || AllowedYanks::explicit_pin(&requirement).is_some())
            {
                policies.push(requirement.into_owned());
            }
        }
        let dependencies: Arc<[Requirement]> = dependencies.into();
        let policies: Arc<[Requirement]> = policies.into();
        let incoming = SourcePotential::Metadata {
            version: metadata.version.clone(),
            dependencies: dependencies.clone(),
            policies: policies.clone(),
        };
        if !directory {
            potentials.get_or_insert(source, incoming);
            return;
        }
        if let Some(SourcePotential::Metadata {
            dependencies: known_dependencies,
            policies: known_policies,
            ..
        }) = potentials.get(&source)
            && dependencies
                .iter()
                .all(|requirement| known_dependencies.contains(requirement))
            && policies
                .iter()
                .all(|requirement| known_policies.contains(requirement))
        {
            return;
        }
        let merge = |previous: &Arc<[Requirement]>, incoming: &Arc<[Requirement]>| {
            let mut combined = previous.to_vec();
            combined.extend(
                incoming
                    .iter()
                    .filter(|requirement| !previous.contains(requirement))
                    .cloned(),
            );
            Arc::from(combined)
        };
        potentials.update_or_insert(
            source,
            |previous| match previous {
                SourcePotential::Unavailable => incoming.clone(),
                SourcePotential::Metadata {
                    version,
                    dependencies: known_dependencies,
                    policies: known_policies,
                } => SourcePotential::Metadata {
                    version: version.clone(),
                    dependencies: merge(known_dependencies, &dependencies),
                    policies: merge(known_policies, &policies),
                },
            },
            incoming.clone(),
        );
    }

    /// Source alternatives apply only within one environment. A new environmental split restarts
    /// an assumed branch without those restrictions so every subenvironment can explore all sources.
    fn environmental_source_search(
        &self,
        state: ForkState,
        requests: &MetadataRequests,
    ) -> SourceSearch {
        if state.source_assumptions.is_empty() && state.preferred_editable.is_empty() {
            SourceSearch::new(state)
        } else {
            SourceSearch::new(self.fresh_source_fork(
                state.env,
                SourceAssumptions::default(),
                requests,
            ))
        }
    }

    /// Convert the dependency [`Fork`]s into [`ForkState`]s.
    fn forks_to_fork_states<'a>(
        &'a self,
        current_state: ForkState,
        version: &'a SolverVersion,
        forks: Vec<Fork>,
        requests: &'a MetadataRequests,
        diverging_packages: &'a BTreeSet<PackageName>,
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
                forked_state.visit_package_version_dependencies(
                    package,
                    version,
                    &fork.dependencies,
                    &self.workspace_members,
                    self.selector.resolution_strategy(),
                );

                // Emit a request to fetch the metadata for each registry package.
                self.visit_dependencies(&fork.dependencies, requests)
                    .map_err(|err| {
                        enrich_dependency_error(err, package, version, &forked_state.pubgrub)
                    })?;

                if Self::has_git_dependency(&fork.dependencies) {
                    let grounding = forked_state.source_dependencies.grounding(
                        &forked_state.pubgrub,
                        &forked_state.env,
                        &forked_state.python_requirement,
                        &self.urls,
                        &self.git,
                    );
                    self.prepare_git_dependencies(&fork.dependencies, &grounding, requests)?;
                }

                // Add the dependencies to the state.
                forked_state.add_package_version_dependencies(
                    package,
                    version,
                    fork.dependencies,
                    &self.urls,
                    &self.git,
                    &self.index,
                    &self.installed_packages,
                );

                Ok(forked_state)
            })
    }

    /// Convert the dependency [`Fork`]s into [`ForkState`]s.
    #[expect(clippy::unused_self)]
    fn version_forks_to_fork_states(
        &self,
        current_state: ForkState,
        forks: Vec<VersionFork>,
        source: SolverSource,
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
            let forked_state = cur_state.take().unwrap();
            if !is_last {
                cur_state = Some(forked_state.clone());
            }
            let continuation = match fork.version {
                Some(version) => ForkContinuation::UseVersion {
                    package: fork.id,
                    version: SolverVersion::new(source, version),
                },
                None => ForkContinuation::SelectVersion { package: fork.id },
            };
            forked_state
                .with_env(fork.env)
                .with_continuation(continuation)
        })
    }

    /// Visit a set of [`PubGrubDependency`] entities prior to selection.
    fn visit_dependencies(
        &self,
        dependencies: &[PubGrubDependency],
        requests: &MetadataRequests,
    ) -> Result<(), ResolveError> {
        for dependency in dependencies {
            let PubGrubDependency {
                package,
                version: _,
                parent: _,
                source,
                policy: _,
            } = dependency;
            // An explicit URL is fetched after a currently selected trusted path authorizes it.
            if source.verbatim_url().is_some()
                || package.name().is_none_or(|name| self.urls.any_url(name))
            {
                continue;
            }
            let index = source.explicit_index();
            Self::request_package(
                package,
                PackageSource::Registry(index),
                &self.hasher,
                requests,
            )?;
        }
        Ok(())
    }

    /// Whether a dependency batch can require Git reference comparison.
    fn has_git_dependency(dependencies: &[PubGrubDependency]) -> bool {
        dependencies
            .iter()
            .any(|dependency| match &dependency.source {
                DependencySource::Url { url, .. } => urls::git_url(&url.parsed_url).is_some(),
                DependencySource::Unspecified | DependencySource::ExplicitIndex(_) => false,
            })
    }

    /// Resolve potentially equivalent Git references only when an independently trusted spelling
    /// names the same package and repository. Registry references never cause package metadata or
    /// build backends to be loaded for comparison.
    fn prepare_git_dependencies(
        &self,
        dependencies: &[PubGrubDependency],
        grounding: &Grounding,
        requests: &MetadataRequests,
    ) -> Result<(), ResolveError> {
        let mut incoming =
            BTreeMap::<PackageName, (Vec<VerbatimParsedUrl>, Vec<VerbatimParsedUrl>)>::new();
        for dependency in dependencies {
            if let DependencySource::Url { url, trusted, .. } = &dependency.source
                && urls::git_url(&url.parsed_url).is_some()
                && let Some(name) = dependency.package.name_no_root()
            {
                let (trusted_urls, registry_urls) = incoming.entry(name.clone()).or_default();
                if *trusted {
                    trusted_urls.push(url.as_ref().clone());
                } else {
                    registry_urls.push(url.as_ref().clone());
                }
            }
        }
        let mut unresolved = BTreeSet::new();
        for (name, (mut trusted, mut registry)) in incoming {
            trusted.extend(
                grounding
                    .sources_for(&name)
                    .map(|source| grounding.url(source, &self.urls)),
            );
            registry.extend(
                grounding
                    .untrusted_urls
                    .get(&name)
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
            let could_match = |a: &VerbatimParsedUrl, b: &VerbatimParsedUrl| {
                urls::could_be_same_git_resource(&a.parsed_url, &b.parsed_url)
                    && !urls::same_resource(&a.parsed_url, &b.parsed_url, &self.git)
            };
            for url in &trusted {
                if trusted
                    .iter()
                    .chain(&registry)
                    .any(|other| could_match(url, other))
                    && let Some(git) = urls::git_url(&url.parsed_url)
                    && self.git.known_precise(git).is_none()
                {
                    unresolved.insert(git.clone());
                }
            }
            for url in &registry {
                if trusted.iter().any(|other| could_match(url, other))
                    && let Some(git) = urls::git_url(&url.parsed_url)
                    && self.git.known_precise(git).is_none()
                {
                    unresolved.insert(git.clone());
                }
            }
        }
        let mut scheduled = Vec::with_capacity(unresolved.len());
        for git in unresolved {
            let completion = requests.request_git_reference(git.clone())?;
            scheduled.push((git, completion));
        }
        for (git, completion) in scheduled {
            completion
                .blocking_recv()
                .map_err(|_| ResolveError::UnregisteredTask(git.to_string()))?;
        }
        Ok(())
    }

    fn request_package(
        package: &PubGrubPackage,
        source: PackageSource<'_>,
        hasher: &HashStrategy,
        requests: &MetadataRequests,
    ) -> Result<(), ResolveError> {
        // Only request real packages.
        let Some(name) = package.name_no_root() else {
            return Ok(());
        };

        match source {
            PackageSource::Url(url) => {
                let dist = Dist::from_url(name.clone(), url.clone())?;
                // A source fetch must enforce the policy before running its backend and can give a
                // more precise error. Wheels defer byte verification to installation.
                if !hasher.allows_url(&url.verbatim) && matches!(&dist, Dist::Built(_)) {
                    return Err(ResolveError::UnhashedPackage(name.clone()));
                }

                // Emit a request to fetch the metadata for this distribution.
                requests.request_direct(dist, hasher)?;
            }
            PackageSource::Registry(index) => {
                requests.request_package(name, index)?;
            }
        }
        Ok(())
    }

    /// Visit the set of [`PubGrubPackage`] candidates prior to selection. This allows us to fetch
    /// metadata for all packages in parallel.
    fn pre_visit<'data>(
        packages: impl Iterator<
            Item = (
                Id<PubGrubPackage>,
                &'data PubGrubPackage,
                &'data CandidateSet,
            ),
        >,
        pre_visited: &mut FxHashMap<Id<PubGrubPackage>, Range<Version>>,
        urls: &Urls,
        indexes: &Indexes,
        python_requirement: &PythonRequirement,
        requests: &MetadataRequests,
    ) -> Result<(), ResolveError> {
        // Iterate over the potential packages, and fetch file metadata for any of them. These
        // represent our current best guesses for the versions that we _might_ select.
        for (id, package, range) in packages {
            let range = range.for_source(SolverSource::Registry);
            if *range == Range::empty() {
                continue;
            }
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
            // Unit propagation often leaves a package's range unchanged. Although prefetching the
            // same package and range is idempotent, selecting its candidate is not free.
            if pre_visited.get(&id) == Some(range) {
                continue;
            }
            pre_visited.insert(id, range.clone());
            requests.prefetch(name, range, python_requirement)?;
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
        source: PackageSource<'_>,
        range: &Range<Version>,
        pins: &mut FilePins,
        preferences: &Preferences,
        env: &ResolverEnvironment,
        python_requirement: &PythonRequirement,
        pubgrub: &State<UvDependencyProvider>,
        visited: &mut FxHashSet<PackageName>,
        selector: &CandidateSelector,
        hasher: &HashStrategy,
        requests: &MetadataRequests,
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
            | PubGrubPackageInner::Package { name, .. } => match source {
                PackageSource::Url(url) => self.choose_version_url(
                    id,
                    name,
                    range,
                    url,
                    env,
                    python_requirement,
                    pubgrub,
                    hasher,
                    requests,
                ),
                PackageSource::Registry(index) => self.choose_version_registry(
                    package,
                    id,
                    name,
                    index.map_or(SolverSource::Registry, |index| {
                        SolverSource::Index(self.indexes.intern(index))
                    }),
                    index,
                    range,
                    preferences,
                    env,
                    python_requirement,
                    pubgrub,
                    pins,
                    visited,
                    selector,
                    requests,
                ),
            },
        }
    }

    /// Select a version for a URL requirement. Since there is only one version per URL, we return
    /// that version if it is in range and `None` otherwise.
    fn choose_version_url(
        &self,
        id: Id<PubGrubPackage>,
        name: &PackageName,
        range: &Range<Version>,
        url: &VerbatimParsedUrl,
        env: &ResolverEnvironment,
        python_requirement: &PythonRequirement,
        pubgrub: &State<UvDependencyProvider>,
        hasher: &HashStrategy,
        requests: &MetadataRequests,
    ) -> Result<Option<ResolverVersion>, ResolveError> {
        debug!(
            "Searching for a compatible version of {name} @ {} ({range})",
            url.verbatim
        );

        let dist = Dist::from_url(name.clone(), url.clone())?;
        let response = requests.wait_for_direct(&dist, hasher)?;

        let source_error = |dist: Box<RequestedDist>, error: Arc<uv_distribution::Error>| {
            let chain = pubgrub
                .partial_solution
                .extract_solution()
                .find(|(package, _)| *package == id)
                .and_then(|(_, candidate)| {
                    DerivationChainBuilder::from_state(id, &candidate, pubgrub)
                })
                .unwrap_or_default();
            ResolveError::Dist(
                DistErrorKind::from_requested_dist(dist.as_ref(), error.as_ref()),
                dist,
                chain,
                error,
            )
        };

        // Keep the concrete error for a malformed direct candidate so a failure proof that
        // requires this URL can report the underlying mismatch.
        let metadata = match &*response {
            MetadataResponse::Found(archive) => &archive.metadata,
            MetadataResponse::Unavailable(MetadataUnavailable::InconsistentMetadata(error)) => {
                return Err(source_error(
                    Box::new(RequestedDist::Installable(dist)),
                    error.clone(),
                ));
            }
            MetadataResponse::Unavailable(_) => {
                return Ok(None);
            }
            MetadataResponse::Error(dist, err) => {
                return Err(source_error(dist.clone(), err.clone()));
            }
        };

        if !hasher.allows_url(&url.verbatim) {
            return Err(ResolveError::UnhashedPackage(name.clone()));
        }

        let version = &metadata.version;

        // The version is incompatible with the requirement.
        if !range.contains(version) {
            return Ok(None);
        }

        // If the URL points to a pre-built wheel, and the wheel's supported Python versions don't
        // match our `Requires-Python`, mark it as incompatible.
        if let Dist::Built(dist) = &dist {
            let filename = match &dist {
                BuiltDist::Registry(dist) => &dist.best_wheel().filename,
                BuiltDist::DirectUrl(dist) => &dist.filename,
                BuiltDist::GitPath(dist) => &dist.filename,
                BuiltDist::Path(dist) => &dist.filename,
            };

            // If the wheel does _not_ cover an environment that requires artifact coverage, it's
            // incompatible.
            if env.marker_environment().is_none() && !self.options.artifact_environments.is_empty()
            {
                let wheel_marker = implied_markers(filename);
                // If the caller marked an environment as requiring artifact coverage, ensure it
                // has coverage.
                for environment_marker in self.options.artifact_environments.iter().copied() {
                    // If the platform is part of the current environment...
                    if env.included_by_marker(environment_marker)
                        && env.included_by_marker(
                            find_environments(id, pubgrub).and(environment_marker),
                        )
                    {
                        // ...but the wheel doesn't support it in this fork, it's incompatible.
                        if !env.included_by_marker(wheel_marker.and(environment_marker)) {
                            return Ok(Some(ResolverVersion::Unavailable(
                                version.clone(),
                                UnavailableVersion::IncompatibleDist(IncompatibleDist::Wheel(
                                    IncompatibleWheel::MissingPlatform(environment_marker),
                                )),
                            )));
                        }
                    }
                }
            }

            // If the wheel's Python tag doesn't match the target Python, it's incompatible.
            if !python_requirement.target().matches_wheel_tag(filename) {
                return Ok(Some(ResolverVersion::Unavailable(
                    filename.version.clone(),
                    UnavailableVersion::IncompatibleDist(IncompatibleDist::Wheel(
                        IncompatibleWheel::Tag(IncompatibleTag::AbiPythonVersion),
                    )),
                )));
            }
        }

        // The version is incompatible due to its `Requires-Python` requirement.
        if let Some(requires_python) = metadata.requires_python.as_ref() {
            if !python_requirement.target().is_contained_by(requires_python) {
                let kind = if python_requirement.installed() == python_requirement.target() {
                    PythonRequirementKind::Installed
                } else {
                    PythonRequirementKind::Target
                };
                return Ok(Some(ResolverVersion::Unavailable(
                    version.clone(),
                    UnavailableVersion::IncompatibleDist(IncompatibleDist::Source(
                        IncompatibleSource::RequiresPython(requires_python.clone(), kind),
                    )),
                )));
            }
        }

        Ok(Some(ResolverVersion::Unforked(version.clone())))
    }

    /// Return a package-level lookup failure from the selected registry.
    fn unavailable_package(
        &self,
        name: &PackageName,
        source: SolverSource,
    ) -> Option<UnavailablePackage> {
        match source {
            SolverSource::Registry => self.unavailable_packages.pin().get(name).cloned(),
            SolverSource::Index(index) => self
                .unavailable_index_packages
                .pin()
                .get(&index)
                .and_then(|packages| packages.pin().get(name).cloned()),
            SolverSource::Url(_) => None,
        }
    }

    /// Record a missing package against the registry that was actually queried.
    fn record_unavailable_package(
        &self,
        name: &PackageName,
        source: SolverSource,
        reason: UnavailablePackage,
    ) {
        match source {
            SolverSource::Registry => {
                self.unavailable_packages.pin().insert(name.clone(), reason);
            }
            SolverSource::Index(index) => {
                let indexes = self.unavailable_index_packages.pin();
                let packages = indexes.get_or_insert(
                    index,
                    HashMap::builder().resize_mode(ResizeMode::Blocking).build(),
                );
                packages.pin().insert(name.clone(), reason);
            }
            SolverSource::Url(_) => {}
        }
    }

    /// Given a candidate registry requirement, choose the next version in range to try, or `None`
    /// if there is no version in this range.
    fn choose_version_registry(
        &self,
        package: &PubGrubPackage,
        id: Id<PubGrubPackage>,
        name: &PackageName,
        solver_source: SolverSource,
        index: Option<&IndexMetadata>,
        range: &Range<Version>,
        preferences: &Preferences,
        env: &ResolverEnvironment,
        python_requirement: &PythonRequirement,
        pubgrub: &State<UvDependencyProvider>,
        pins: &mut FilePins,
        visited: &mut FxHashSet<PackageName>,
        selector: &CandidateSelector,
        requests: &MetadataRequests,
    ) -> Result<Option<ResolverVersion>, ResolveError> {
        // Wait for the metadata to be available.
        let versions_response = requests.wait_for_versions(name, index)?;
        let index = index.map(IndexMetadata::url);
        visited.insert(name.clone());

        let version_maps = match *versions_response {
            VersionsResponse::Found(ref version_maps) => version_maps.as_slice(),
            VersionsResponse::NoIndex => {
                self.record_unavailable_package(name, solver_source, UnavailablePackage::NoIndex);
                &[]
            }
            VersionsResponse::Offline => {
                self.record_unavailable_package(name, solver_source, UnavailablePackage::Offline);
                &[]
            }
            VersionsResponse::NotFound => {
                self.record_unavailable_package(name, solver_source, UnavailablePackage::NotFound);
                &[]
            }
        };

        debug!("Searching for a compatible version of {package} ({range})");

        // Find a version.
        let Some(candidate) = selector.select(
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
            solver_source,
            index,
            range,
            preferences,
            env,
            pubgrub,
            pins,
            selector,
            requests,
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
        self.visit_candidate(
            &candidate,
            dist,
            package,
            name,
            solver_source,
            pins,
            requests,
        )?;

        let version = candidate.version().clone();
        Ok(Some(ResolverVersion::Unforked(version)))
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
        solver_source: SolverSource,
        index: Option<&IndexUrl>,
        range: &Range<Version>,
        preferences: &Preferences,
        env: &ResolverEnvironment,
        pubgrub: &State<UvDependencyProvider>,
        pins: &mut FilePins,
        selector: &CandidateSelector,
        requests: &MetadataRequests,
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

        // If the caller marked an environment as requiring artifact coverage, ensure it has
        // coverage.
        for marker in self.options.artifact_environments.iter().copied() {
            // If the platform is part of the current environment...
            if env.included_by_marker(marker) {
                // But isn't supported by the distribution in this fork...
                if !env.included_by_marker(dist.implied_markers().and(marker))
                    && env.included_by_marker(find_environments(id, pubgrub).and(marker))
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

        let Some(base_candidate) = selector.select(
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
            remainder = remainder.and(dist.implied_markers().negate());
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
                solver_source,
                pins,
                requests,
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
                remainder = remainder.or(sys_platform);
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
        self.visit_candidate(
            candidate,
            dist,
            package,
            name,
            solver_source,
            pins,
            requests,
        )?;
        self.visit_candidate(
            &base_candidate,
            base_dist,
            package,
            name,
            solver_source,
            pins,
            requests,
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
        solver_source: SolverSource,
        pins: &mut FilePins,
        requests: &MetadataRequests,
    ) -> Result<(), ResolveError> {
        // We want to return a package pinned to a specific version; but we _also_ want to
        // store the exact file that we selected to satisfy that version.
        pins.insert(solver_source, candidate, dist);

        // Emit a request to fetch the metadata for this version.
        if matches!(&**package, PubGrubPackageInner::Package { .. }) {
            if self.dependency_mode.is_transitive() {
                let dist = dist.for_resolution();
                requests.request_metadata(dist.distribution_id(), || {
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

                    Ok(Request::from(dist))
                })?;
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
    fn get_dependencies_forking(
        &self,
        id: Id<PubGrubPackage>,
        package: &PubGrubPackage,
        candidate: &SolverVersion,
        pins: &FilePins,
        grounding: &Grounding,
        preferred_editable: &BTreeSet<SourceId>,
        env: &ResolverEnvironment,
        python_requirement: &PythonRequirement,
        pubgrub: &State<UvDependencyProvider>,
        requests: &MetadataRequests,
    ) -> Result<ForkedDependencies, ResolveError> {
        let dependencies = self.get_dependencies(
            id,
            package,
            candidate,
            pins,
            grounding,
            preferred_editable,
            env,
            python_requirement,
            pubgrub,
            requests,
        )?;
        if env.marker_environment().is_some() {
            Ok(ForkedDependencies::from_dependencies_platform_specific(
                dependencies,
            ))
        } else {
            Ok(ForkedDependencies::from_dependencies_universal(
                dependencies,
                env,
                python_requirement,
                &self.conflicts,
            ))
        }
    }

    /// Given a candidate package and version, return its dependencies.
    #[instrument(skip_all, fields(%package, version = %candidate))]
    fn get_dependencies(
        &self,
        id: Id<PubGrubPackage>,
        package: &PubGrubPackage,
        candidate: &SolverVersion,
        pins: &FilePins,
        grounding: &Grounding,
        preferred_editable: &BTreeSet<SourceId>,
        env: &ResolverEnvironment,
        python_requirement: &PythonRequirement,
        pubgrub: &State<UvDependencyProvider>,
        requests: &MetadataRequests,
    ) -> Result<Dependencies, ResolveError> {
        let version = &candidate.version;
        let expander = RequirementExpander::new(
            &self.constraints,
            &self.overrides,
            &self.excludes,
            env,
            python_requirement,
        );
        let dependencies = match &**package {
            PubGrubPackageInner::Root(_) => {
                let requirements = expander.expand(&self.requirements, RequirementContext::Root);

                PubGrubDependency::from_requirements(
                    &self.conflicts,
                    requirements,
                    None,
                    Some(package),
                    |_| true,
                    |_| true,
                    None,
                )
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

                // Registry pins and direct resources at the same PEP 440 version can have
                // completely different metadata. Consult only the selected candidate's source.
                let direct;
                let distribution_id = match candidate.source {
                    SolverSource::Url(source) => {
                        let url = grounding.metadata_url(source, &self.urls, preferred_editable);
                        direct = Some((Dist::from_url(name.clone(), url.clone())?, url));
                        None
                    }
                    SolverSource::Registry | SolverSource::Index(_) => {
                        direct = None;
                        let Some((_, metadata_id)) = pins.dist_and_id(name, candidate) else {
                            debug_assert!(
                                false,
                                "Dependencies were requested for a package without a pinned distribution"
                            );
                            return Err(ResolveError::UnregisteredTask(format!(
                                "{name}=={version}"
                            )));
                        };
                        Some(metadata_id)
                    }
                };

                // If the package does not exist in the registry or locally, we cannot fetch its dependencies
                if candidate.source.is_registry()
                    && self.dependency_mode.is_transitive()
                    && self.unavailable_package(name, candidate.source).is_some()
                    && self.installed_packages.get_packages(name).is_empty()
                {
                    debug_assert!(
                        false,
                        "Dependencies were requested for a package that is not available"
                    );
                    return Err(ResolveError::PackageUnavailable(name.clone()));
                }

                // Wait for the metadata to be available.
                let response = if let Some((dist, url)) = direct {
                    let hasher = grounding.hashes.strategy(&self.hasher)?;
                    if !hasher.allows_url(&url.verbatim) && matches!(&dist, Dist::Built(_)) {
                        return Err(ResolveError::UnhashedPackage(name.clone()));
                    }
                    requests.request_direct(dist.clone(), &hasher)?;
                    requests.wait_for_direct(&dist, &hasher)?
                } else if let Some(distribution_id) = distribution_id {
                    requests.wait_for_metadata(distribution_id, || format!("{name}=={version}"))?
                } else {
                    return Err(ResolveError::UnregisteredTask(format!("{name}=={version}")));
                };

                let metadata = match &*response {
                    MetadataResponse::Found(archive) => {
                        if let SolverSource::Url(source) = candidate.source {
                            self.remember_source_metadata(source, &archive.metadata);
                        }
                        &archive.metadata
                    }
                    MetadataResponse::Unavailable(reason) => {
                        let unavailable_version = UnavailableVersion::from(reason);
                        let message = unavailable_version.singular_message();
                        if let Some(err) = reason.source() {
                            // Show the detailed error for metadata parse errors.
                            warn!("{name} {message}: {err}");
                        } else {
                            warn!("{name} {message}");
                        }
                        if candidate.source.is_registry() {
                            let incomplete_packages = self.incomplete_packages.pin();
                            let versions = incomplete_packages.get_or_insert(
                                (name.clone(), candidate.source),
                                HashMap::builder().resize_mode(ResizeMode::Blocking).build(),
                            );
                            versions.pin().insert(version.clone(), reason.clone());
                        }
                        return Ok(Dependencies::Unavailable(unavailable_version));
                    }
                    MetadataResponse::Error(dist, err) => {
                        let chain = DerivationChainBuilder::from_state(id, candidate, pubgrub)
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
                // distribution or need to fork.
                if let Some(requires_python) = &metadata.requires_python {
                    if !python_requirement.target().is_contained_by(requires_python) {
                        return Ok(Dependencies::RequiresPython(requires_python.clone()));
                    }
                }

                // Identify any system dependencies based on the index URL.
                let system_dependencies = self
                    .options
                    .torch_backend
                    .as_ref()
                    .filter(|torch_backend| matches!(torch_backend, TorchStrategy::Cuda { .. }))
                    .filter(|torch_backend| torch_backend.has_system_dependency(name))
                    .filter(|_| candidate.source.is_registry())
                    .and_then(|_| pins.get(name, candidate).and_then(ResolvedDist::index))
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

                let (requirements, context) = if let Some(group) = group {
                    debug_assert!(extra.is_none());
                    (
                        metadata
                            .dependency_groups
                            .get(group)
                            .map_or(&[][..], AsRef::as_ref),
                        RequirementContext::Group { name, version },
                    )
                } else if let Some(extra) = extra {
                    (
                        metadata.requires_dist.as_ref(),
                        RequirementContext::Extra {
                            name,
                            version,
                            extra,
                        },
                    )
                } else {
                    (
                        metadata.requires_dist.as_ref(),
                        RequirementContext::Package { name, version },
                    )
                };
                let user_path =
                    self.workspace_members.contains(name) || self.project.as_ref() == Some(name);
                let configured_metadata = self.dependency_metadata.get(name, Some(version));
                let policy = match candidate.source {
                    SolverSource::Registry | SolverSource::Index(_) => None,
                    SolverSource::Url(source) => Some(matches!(
                        grounding.url(source, &self.urls).parsed_url,
                        ParsedUrl::Directory(_)
                    )),
                };
                let source_context = env.fork_markers().map_or(MarkerTree::TRUE, |fork| {
                    let context = grounding
                        .contexts
                        .get(&id)
                        .copied()
                        .unwrap_or_else(|| package.marker());
                    if fork
                        .and(python_requirement.to_marker_tree())
                        .is_disjoint(context.negate())
                    {
                        MarkerTree::TRUE
                    } else {
                        python_requirement.simplify_markers(context)
                    }
                });
                let requirements =
                    expander
                        .expand(requirements, context)
                        .filter_map(|mut requirement| {
                            if user_path {
                                requirement.to_mut().set_force_relative(false);
                            }
                            let has_source = match &requirement.source {
                                RequirementSource::Registry { index, .. } => index.is_some(),
                                RequirementSource::Url { .. }
                                | RequirementSource::GitDirectory { .. }
                                | RequirementSource::GitPath { .. }
                                | RequirementSource::Path { .. }
                                | RequirementSource::Directory { .. } => true,
                            };
                            if has_source && !source_context.is_true() {
                                // A concrete source can affect unmarked requirements from other parents.
                                // Fork before applying it outside the path that reached this package or
                                // activated this extra.
                                let marker = requirement.marker.and(source_context);
                                if marker.is_false() || !env.included_by_marker(marker) {
                                    return None;
                                }
                                if marker != requirement.marker {
                                    requirement.to_mut().marker = marker;
                                }
                            }
                            Some(requirement)
                        });

                PubGrubDependency::from_requirements(
                    &self.conflicts,
                    requirements,
                    group.as_ref(),
                    Some(package),
                    |requirement| {
                        matches!(candidate.source, SolverSource::Url(_))
                            || self.urls.configuration_authorizes(
                                group.is_none().then_some((name, version)),
                                requirement,
                                &self.git,
                            )
                    },
                    |requirement| {
                        self.urls.configuration_authorizes(
                            group.is_none().then_some((name, version)),
                            requirement,
                            &self.git,
                        ) || configured_metadata.as_ref().is_some_and(|metadata| {
                            metadata.requires_dist.iter().any(|configured| {
                                if configured.name != requirement.name {
                                    return false;
                                }
                                let marker = requirement.marker.without_extras();
                                let configured_marker = configured.marker.without_extras();
                                if !marker.is_disjoint(configured_marker.negate()) {
                                    return false;
                                }
                                let configured = Requirement::from(configured.clone());
                                requirement
                                    .source
                                    .to_verbatim_parsed_url()
                                    .zip(configured.source.to_verbatim_parsed_url())
                                    .is_some_and(|(url, configured)| {
                                        urls::same_resource(
                                            &url.parsed_url,
                                            &configured.parsed_url,
                                            &self.git,
                                        )
                                    })
                            })
                        })
                    },
                    policy,
                )
                .map(|mut dependencies| {
                    dependencies.extend(system_dependencies);
                    dependencies
                })
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
                            source: DependencySource::Unspecified,
                            policy: None,
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
                                    source: DependencySource::Unspecified,
                                    policy: None,
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
                            source: DependencySource::Unspecified,
                            policy: None,
                        })
                        .collect(),
                ));
            }
        };
        Ok(match dependencies {
            Ok(dependencies) => Dependencies::Available(dependencies),
            Err(requirement) => {
                Dependencies::Unavailable(UnavailableVersion::UnsatisfiableDependency(requirement))
            }
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
                        .done(dist.distribution_id(), Arc::new(metadata));
                }
                Some(Response::Dist {
                    dist,
                    metadata,
                    direct_hashes,
                }) => {
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
                    if let Some(hashes) = direct_hashes
                        && !hashes.uses_project_cache(&dist)
                    {
                        self.index
                            .direct()
                            .done((dist.distribution_id(), hashes), Arc::new(metadata));
                    } else {
                        self.index
                            .distributions()
                            .done(dist.distribution_id(), Arc::new(metadata));
                    }
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
                    index,
                    package_versions,
                )))
            }

            Request::GitReference(git, completion) => {
                provider.resolve_git_reference(&git).await;
                let _ = completion.send(());
                Ok(None)
            }

            // Fetch distribution metadata from the distribution database.
            Request::Dist(dist, direct_hasher) => {
                let direct_hashes = direct_hasher
                    .as_ref()
                    .map(|hasher| DirectHashKey::new(&dist, hasher));
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
                                            ArchiveMetadata::from_metadata23(metadata),
                                        ),
                                        direct_hashes,
                                    }));
                                }
                            }
                        }

                        // Only Simple API version maps can contain registry-provided metadata;
                        // a flat index at the same address has a separate version map.
                        let versions_response = self
                            .index
                            .explicit()
                            .get(&(dist.name().clone(), IndexMetadata::from(index.clone())));
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
                                        ArchiveMetadata::from_metadata23(metadata),
                                    ),
                                    direct_hashes,
                                }));
                            }
                        }
                    }
                }

                let metadata = provider
                    .get_or_build_wheel_metadata(
                        &dist,
                        direct_hasher.as_ref().unwrap_or(&self.hasher),
                    )
                    .boxed_local()
                    .await;
                let metadata = match metadata {
                    Ok(metadata) => metadata,
                    Err(error) if direct_hasher.is_some() => MetadataResponse::Error(
                        Box::new(RequestedDist::Installable(dist.clone())),
                        Arc::new(error),
                    ),
                    Err(error) => return Err(error.into()),
                };

                if let MetadataResponse::Found(metadata) = &metadata {
                    if &metadata.metadata.name != dist.name() {
                        if direct_hasher.is_some() {
                            let error = uv_distribution::Error::WheelMetadataNameMismatch {
                                given: dist.name().clone(),
                                metadata: metadata.metadata.name.clone(),
                            };
                            let metadata = MetadataResponse::Error(
                                Box::new(RequestedDist::Installable(dist.clone())),
                                Arc::new(error),
                            );
                            return Ok(Some(Response::Dist {
                                dist,
                                metadata,
                                direct_hashes,
                            }));
                        }
                        return Err(ResolveError::MismatchedPackageName {
                            request: "distribution metadata",
                            expected: dist.name().clone(),
                            actual: metadata.metadata.name.clone(),
                        });
                    }
                }

                Ok(Some(Response::Dist {
                    dist,
                    metadata,
                    direct_hashes,
                }))
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
                    .map_err(|_| ResolveError::UnregisteredTask(package_name.to_string()))?;

                let version_map = match *versions_response {
                    VersionsResponse::Found(ref version_map) => version_map,
                    // Short-circuit if we did not find any versions for the package
                    VersionsResponse::NoIndex => {
                        self.unavailable_packages
                            .pin()
                            .insert(package_name.clone(), UnavailablePackage::NoIndex);

                        return Ok(None);
                    }
                    VersionsResponse::Offline => {
                        self.unavailable_packages
                            .pin()
                            .insert(package_name.clone(), UnavailablePackage::Offline);

                        return Ok(None);
                    }
                    VersionsResponse::NotFound => {
                        self.unavailable_packages
                            .pin()
                            .insert(package_name.clone(), UnavailablePackage::NotFound);

                        return Ok(None);
                    }
                };

                // We don't have access to the fork state when prefetching.
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
                let Some(dist) = candidate.compatible() else {
                    return Ok(None);
                };

                // If the registry provided metadata for this distribution, use it.
                for version_map in version_map {
                    if let Some(metadata) = version_map.get_metadata(candidate.version()) {
                        let dist = dist.for_resolution();
                        if version_map.index() == dist.index() {
                            debug!("Found registry-provided metadata for: {dist}");

                            let metadata =
                                MetadataResponse::Found(ArchiveMetadata::from_metadata23(metadata));

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
                                    direct_hashes: None,
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
                            if lower == Bound::Unbounded {
                                debug!(
                                    "Skipping prefetch for unbounded minimum-version range: {package_name} ({range})"
                                );
                                return Ok(None);
                            }
                        }
                    }
                }

                // Validate the Python requirement.
                if let Some(requires_python) = dist.requires_python() {
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
                let dist = dist.for_resolution();
                if self.index.distributions().register(dist.distribution_id()) {
                    let dist = dist.to_owned();
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
                                .get_or_build_wheel_metadata(&dist, &self.hasher)
                                .boxed_local()
                                .await?;

                            Response::Dist {
                                dist: (*dist).clone(),
                                metadata,
                                direct_hashes: None,
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
        }
    }

    fn convert_no_solution_err(
        &self,
        err: pubgrub::NoSolutionError<UvDependencyProvider>,
        fork_urls: ForkUrls,
        fork_indexes: ForkIndexes,
        report_sources: &FxHashMap<PackageName, SolverSource>,
        known_versions: &FxHashMap<PackageName, Arc<[Version]>>,
        env: ResolverEnvironment,
        current_environment: MarkerEnvironment,
        visited: &FxHashSet<PackageName>,
    ) -> ResolveError {
        let source_for = |package: &PubGrubPackage| {
            package
                .name_no_root()
                .and_then(|name| report_sources.get(name))
                .copied()
                .unwrap_or(SolverSource::Registry)
        };
        let mut err = project_error(err, source_for);
        err = NoSolutionError::collapse_local_version_segments(NoSolutionError::collapse_proxies(
            err,
        ));
        err = NoSolutionError::narrow_widened_sets(err, known_versions);
        err = NoSolutionError::collapse_source_constraints(err, &fork_urls);

        let mut unavailable_packages = FxHashMap::default();
        for package in derivation_tree_packages(&err) {
            if let PubGrubPackageInner::Package { name, .. } = &**package {
                if let Some(reason) = self.unavailable_package(name, source_for(package)) {
                    unavailable_packages.insert(name.clone(), reason);
                }
            }
        }

        let mut incomplete_packages = FxHashMap::default();
        let incomplete_packages_cache = self.incomplete_packages.pin();
        for package in derivation_tree_packages(&err) {
            if let PubGrubPackageInner::Package { name, .. } = &**package
                && let Some(versions) =
                    incomplete_packages_cache.get(&(name.clone(), source_for(package)))
            {
                for (version, reason) in &versions.pin() {
                    incomplete_packages
                        .entry(name.clone())
                        .or_insert_with(BTreeMap::default)
                        .insert(version.clone(), reason.clone());
                }
            }
        }

        let mut available_indexes = FxHashMap::default();
        let mut included_versions = FxHashMap::default();
        let mut available_versions = FxHashMap::default();

        let available_version_cutoff: Option<jiff::Timestamp> =
            std::env::var(EnvVars::UV_TEST_AVAILABLE_VERSION_CUTOFF)
                .ok()
                .and_then(|s| s.parse().ok());

        for package in derivation_tree_packages(&err) {
            let Some(name) = package.name() else { continue };
            if !visited.contains(name) {
                // Avoid including version data for packages that exist in the derivation
                // tree, but were never visited during resolution. We _may_ have metadata for
                // these packages, but it's non-deterministic, and omitting them ensures that
                // we represent the state of the resolver at the time of failure.
                continue;
            }
            let versions_response = if let Some(index) = fork_indexes.get(name) {
                self.index.explicit().get(&(name.clone(), index.clone()))
            } else {
                self.index.implicit().get(name)
            };
            if let Some(response) = versions_response {
                if let VersionsResponse::Found(ref version_maps) = *response {
                    // Track included and available versions, across all indexes.
                    for version_map in version_maps {
                        let package_included_versions = included_versions
                            .entry(name.clone())
                            .or_insert_with(BTreeSet::new);
                        let package_available_versions = available_versions
                            .entry(name.clone())
                            .or_insert_with(BTreeSet::new);

                        for (version, dists) in version_map.iter(&Ranges::full()) {
                            // Included versions are those that survive the effective
                            // `exclude-newer` filter used during resolution. Files with
                            // missing upload times are treated as excluded (matching
                            // the resolution behavior in `version_map.rs`).
                            let excluded_from_included = || {
                                let Some(included_version_cutoff) =
                                    version_map.included_version_cutoff()
                                else {
                                    return false;
                                };
                                let Some(prioritized_dist) = dists.prioritized_dist() else {
                                    return true;
                                };
                                prioritized_dist.files().all(|file| {
                                    file.upload_time_utc_ms.is_none_or(|upload_time| {
                                        upload_time >= included_version_cutoff.as_millisecond()
                                    })
                                })
                            };

                            if !excluded_from_included() {
                                package_included_versions.insert(version.clone());
                            }

                            // Available versions are used in resolver error reporting,
                            // and can be bounded by a test-only cutoff for deterministic
                            // snapshots. Files with missing upload times are *not*
                            // excluded, since we only filter versions we can confirm
                            // were published after the cutoff.
                            let excluded_from_available = || {
                                let Some(ref exclude_newer) = available_version_cutoff else {
                                    return false;
                                };
                                let Some(prioritized_dist) = dists.prioritized_dist() else {
                                    return false;
                                };
                                prioritized_dist.files().all(|file| {
                                    file.upload_time_utc_ms.is_some_and(|upload_time| {
                                        upload_time >= exclude_newer.as_millisecond()
                                    })
                                })
                            };

                            if !excluded_from_available() {
                                package_available_versions.insert(version.clone());
                            }
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
            included_versions,
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

    /// Report distinct trusted URLs or indexes only when both declarations occur in PubGrub's failure proof.
    fn source_conflict(
        &self,
        error: &pubgrub::NoSolutionError<UvDependencyProvider>,
        state: &ForkState,
    ) -> Option<ResolveError> {
        type Declaration = (SourceId, usize, Id<PubGrubPackage>, SolverVersion);
        type IndexDeclaration = (IndexId, usize, Id<PubGrubPackage>, SolverVersion);
        let mut sources = BTreeMap::<PackageName, Vec<Declaration>>::new();
        let mut indexes = BTreeMap::<PackageName, Vec<IndexDeclaration>>::new();
        let mut pending = vec![error];
        let mut seen = FxHashSet::default();
        while let Some(tree) = pending.pop() {
            if !seen.insert(std::ptr::from_ref(tree)) {
                continue;
            }
            match tree {
                DerivationTree::External(External::FromDependencyOf(
                    parent,
                    candidates,
                    dependency,
                    requirements,
                )) => {
                    let Some(name) = dependency.name_no_root() else {
                        continue;
                    };
                    for source in requirements.only_urls() {
                        for (order, id, candidate) in state
                            .source_dependencies
                            .trusted_declarations(&state.pubgrub, parent, candidates, name, source)
                        {
                            sources
                                .entry(name.clone())
                                .or_default()
                                .push((source, order, id, candidate));
                        }
                    }
                    for index in requirements.only_indexes() {
                        for (order, id, candidate) in state.source_dependencies.index_declarations(
                            &state.pubgrub,
                            parent,
                            candidates,
                            name,
                            index,
                        ) {
                            indexes
                                .entry(name.clone())
                                .or_default()
                                .push((index, order, id, candidate));
                        }
                    }
                }
                DerivationTree::External(_) => {}
                DerivationTree::Derived(derived) => {
                    pending.push(&derived.cause2);
                    pending.push(&derived.cause1);
                }
            }
        }
        for (name, declarations) in indexes {
            let pair = declarations.iter().enumerate().find_map(|(index, a)| {
                declarations[index + 1..]
                    .iter()
                    .find(|b| a.0 != b.0 && (a.2 != b.2 || a.3 == b.3))
                    .map(|b| (a, b))
            });
            let Some((a, b)) = pair else { continue };
            let mut indexes = vec![self.indexes.resource(a.0), self.indexes.resource(b.0)];
            indexes.sort();
            let error = ResolveError::ConflictingIndexesForEnvironment {
                package_name: name,
                indexes,
                env: state.env.clone(),
            };
            let (_, _, parent, candidate) = if a.1 > b.1 { a } else { b };
            if let Some(name) = state.pubgrub.package_store[*parent].name_no_root()
                && let Some(chain) = state.source_dependencies.chain(*parent, candidate)
            {
                return Some(ResolveError::Dependencies(
                    Box::new(error),
                    name.clone(),
                    candidate.version.clone(),
                    chain.clone(),
                ));
            }
            return Some(enrich_dependency_error(
                error,
                *parent,
                candidate,
                &state.pubgrub,
            ));
        }
        for (name, declarations) in sources {
            let pair = declarations.iter().enumerate().find_map(|(index, a)| {
                declarations[index + 1..]
                    .iter()
                    .find(|b| a.0 != b.0 && (a.2 != b.2 || a.3 == b.3))
                    .map(|b| (a, b))
            });
            let Some((a, b)) = pair else { continue };
            let grounding = state.source_dependencies.grounding(
                &state.pubgrub,
                &state.env,
                &state.python_requirement,
                &self.urls,
                &self.git,
            );
            let mut urls = vec![
                grounding.url(a.0, &self.urls).parsed_url,
                grounding.url(b.0, &self.urls).parsed_url,
            ];
            urls.sort();
            let error = ResolveError::ConflictingUrls {
                package_name: name,
                urls,
                env: state.env.clone(),
            };
            let (_, _, parent, candidate) = if a.1 > b.1 { a } else { b };
            if let Some(name) = state.pubgrub.package_store[*parent].name_no_root()
                && let Some(chain) = state.source_dependencies.chain(*parent, candidate)
            {
                return Some(ResolveError::Dependencies(
                    Box::new(error),
                    name.clone(),
                    candidate.version.clone(),
                    chain.clone(),
                ));
            }
            return Some(enrich_dependency_error(
                error,
                *parent,
                candidate,
                &state.pubgrub,
            ));
        }
        None
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

/// All known versions for each package, from the version maps and the installed packages,
/// used to keep the version sets in the partial solution minimal.
#[derive(Clone, Default)]
struct KnownVersions {
    by_source: FxHashMap<(PackageName, Option<IndexMetadata>), Arc<[Version]>>,
}

impl KnownVersions {
    /// Produce the listing for the source used to explain a failure. A URL's version is not part
    /// of an independently queried registry listing for a package with the same name.
    fn for_report(
        &self,
        sources: &FxHashMap<PackageName, SolverSource>,
        indexes: &Indexes,
    ) -> FxHashMap<PackageName, Arc<[Version]>> {
        self.by_source
            .iter()
            .filter_map(|((name, index), versions)| {
                let selected = match sources.get(name).copied().unwrap_or(SolverSource::Registry) {
                    SolverSource::Registry => index.is_none(),
                    SolverSource::Index(selected) => index
                        .as_ref()
                        .is_some_and(|index| *index == indexes.resource(selected)),
                    SolverSource::Url(_) => false,
                };
                selected.then(|| (name.clone(), versions.clone()))
            })
            .collect()
    }

    /// Returns the sorted, deduplicated candidate universe used to widen version sets.
    ///
    /// Results are cached on the first call per package.
    ///
    /// Every selectable version must be present: omitting one could extend an incompatibility
    /// across it, while including an unselectable version only prevents a possible simplification.
    /// The result is therefore conservative, including yanked and otherwise unavailable versions
    /// from every index plus installed versions missing from the indexes. Versions past the
    /// exclude-newer cutoff are omitted because resolution treats them as nonexistent.
    ///
    /// Non-blocking: Returns `None` if the version map hasn't been fetched yet, or if the
    /// package is not a registry package.
    fn get_or_update<'a, InstalledPackages: InstalledPackagesProvider>(
        &'a mut self,
        index: &InMemoryIndex,
        installed_packages: &InstalledPackages,
        source: PackageSource<'_>,
        package: &PubGrubPackage,
    ) -> Option<&'a [Version]> {
        let name = package.name_no_root()?;
        // Versions of packages from a URL or the workspace are not registry versions.
        let PackageSource::Registry(index_metadata) = source else {
            return None;
        };
        let key = (name.clone(), index_metadata.cloned());
        if !self.by_source.contains_key(&key) {
            let response = if let Some(index_metadata) = index_metadata {
                index
                    .explicit()
                    .get(&(name.clone(), index_metadata.clone()))?
            } else {
                index.implicit().get(name)?
            };
            let VersionsResponse::Found(ref version_maps) = *response else {
                return None;
            };
            let mut versions: Vec<Version> = version_maps
                .iter()
                .flat_map(|version_map| version_map.included_versions().cloned())
                .chain(
                    installed_packages
                        .get_packages(name)
                        .iter()
                        .map(|dist| dist.version().clone()),
                )
                .collect();
            versions.sort_unstable();
            versions.dedup();
            self.by_source.insert(key.clone(), versions.into());
        }
        Some(&self.by_source[&key][..])
    }
}

/// The operation to resume after creating a fork.
#[derive(Clone, Default)]
enum ForkContinuation {
    /// Run unit propagation and choose the next package.
    #[default]
    Propagate,
    /// Select a version for a package whose constraints have already been propagated.
    SelectVersion { package: Id<PubGrubPackage> },
    /// Use the version selected for this package by the fork operation.
    UseVersion {
        package: Id<PubGrubPackage>,
        version: SolverVersion,
    },
}

/// The alternative source solves for exactly one environmental fork.
struct SourceSearch {
    states: Vec<ForkState>,
    seen: FxHashSet<(SourceAssumptions, BTreeSet<PackageName>, BTreeSet<SourceId>)>,
    error: Option<ResolveError>,
    source_error: Option<ResolveError>,
    directory_error: Option<ResolveError>,
    fallback_source_error: Option<ResolveError>,
    candidate_errors: BTreeMap<SourceId, ResolveError>,
    failed_sources: Vec<SourceId>,
    policy_errors: Vec<(Vec<(PubGrubPackage, SolverVersion)>, ResolveError)>,
    failed_candidates: FxHashMap<PubGrubPackage, CandidateSet>,
    lowest_attempts: FxHashSet<LowestAttempt>,
}

#[derive(PartialEq, Eq, Hash)]
struct LowestAttempt {
    package: PubGrubPackage,
    candidate: SolverVersion,
    assumptions: SourceAssumptions,
    preferred: BTreeSet<PackageName>,
    editable: BTreeSet<SourceId>,
}

impl SourceSearch {
    fn new(state: ForkState) -> Self {
        let seen = FxHashSet::from_iter([(
            state.source_assumptions.clone(),
            state.preferred_lowest.clone(),
            state.preferred_editable.clone(),
        )]);
        Self {
            states: vec![state],
            seen,
            error: None,
            source_error: None,
            directory_error: None,
            fallback_source_error: None,
            candidate_errors: BTreeMap::new(),
            failed_sources: Vec::new(),
            policy_errors: Vec::new(),
            failed_candidates: FxHashMap::default(),
            lowest_attempts: FxHashSet::default(),
        }
    }

    fn record_candidate_error(&mut self, source: SourceId, error: ResolveError) {
        self.candidate_errors.entry(source).or_insert(error);
    }

    fn failed_source_error(&mut self) -> Option<ResolveError> {
        self.failed_sources
            .iter()
            .find_map(|source| self.candidate_errors.remove(source))
    }

    fn record_policy_error(
        &mut self,
        origins: Vec<(PubGrubPackage, SolverVersion)>,
        error: ResolveError,
    ) {
        if origins.is_empty() {
            self.fallback_source_error.get_or_insert(error);
        } else if !self
            .policy_errors
            .iter()
            .any(|(known, _)| *known == origins)
        {
            self.policy_errors.push((origins, error));
        }
    }

    fn failed_policy_error(&mut self) -> Option<ResolveError> {
        let index = self.policy_errors.iter().position(|(origins, _)| {
            origins.iter().all(|(package, candidate)| {
                self.failed_candidates
                    .get(package)
                    .is_some_and(|required| required.contains(candidate))
            })
        })?;
        Some(self.policy_errors.remove(index).1)
    }

    fn record_failed_candidates(&mut self, package: &PubGrubPackage, requirements: &CandidateSet) {
        self.failed_candidates
            .entry(package.clone())
            .and_modify(|known| *known = known.union(requirements))
            .or_insert_with(|| requirements.clone());
    }

    /// A direct retrieval failure explains an unsatisfiable solve only if its exact source was
    /// required in the native proof. Failures encountered on unrelated branches remain diagnostic.
    fn failure_from_proof(
        &mut self,
        error: &pubgrub::NoSolutionError<UvDependencyProvider>,
    ) -> Option<ResolveError> {
        let mut pending = vec![error];
        let mut seen = FxHashSet::default();
        while let Some(tree) = pending.pop() {
            if !seen.insert(std::ptr::from_ref(tree)) {
                continue;
            }
            match tree {
                DerivationTree::External(
                    External::FromDependencyOf(_, _, package, requirements)
                    | External::NoVersions(package, requirements),
                ) => {
                    self.record_failed_candidates(package, requirements);
                    for source in requirements.only_urls() {
                        if !self.failed_sources.contains(&source) {
                            self.failed_sources.push(source);
                        }
                    }
                }
                DerivationTree::External(External::NotRoot(package, candidate)) => {
                    self.record_failed_candidates(
                        package,
                        &CandidateSet::singleton(candidate.clone()),
                    );
                }
                DerivationTree::External(External::Custom(..)) => {}
                DerivationTree::Derived(derived) => {
                    pending.push(&derived.cause2);
                    pending.push(&derived.cause1);
                }
            }
        }
        self.failed_source_error()
            .or_else(|| self.failed_policy_error())
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
    /// The operation to resume when this fork is next visited.
    continuation: ForkContinuation,
    /// The next package on which to run unit propagation.
    next: Id<PubGrubPackage>,
    /// The set of pinned versions we accrue throughout resolution.
    ///
    /// The key of this map is a package name, and each package name maps to
    /// a set of versions for that package. Each version in turn is mapped
    /// to the concrete distribution selected for installation, along with the
    /// concrete distribution whose metadata was used during resolution.
    /// After resolution is finished, this map is consulted to recover both the
    /// locked artifact and the metadata backing the resolved dependency edges.
    pins: FilePins,
    /// Stable identities for explicit registries used by dependency edges in this branch.
    indexes: Indexes,
    /// When dependencies for a package are retrieved, this map of priorities
    /// is updated based on how each dependency was specified. Certain types
    /// of dependencies have more "priority" than others (like direct URL
    /// dependencies). These priorities help determine which package to
    /// consider next during resolution.
    priorities: PubGrubPriorities,
    /// This keeps track of the set of versions for each package that we've
    /// already visited during resolution. This avoids doing redundant work.
    added_dependencies: FxHashMap<Id<PubGrubPackage>, FxHashSet<SolverVersion>>,
    /// Dependencies with the candidate and source authority that introduced them.
    source_dependencies: SourceDependencies,
    /// Packages whose allowed sources currently have no grounded concrete candidate.
    pending_sources: FxHashMap<Id<PubGrubPackage>, CandidateSet>,
    /// Optional candidate restrictions local to one retry of a stalled source solve.
    source_assumptions: SourceAssumptions,
    /// All registered package IDs, in allocation order, for rescheduling pending packages.
    source_packages: Vec<Id<PubGrubPackage>>,
    source_package_set: FxHashSet<Id<PubGrubPackage>>,
    /// Use a stable scan after a package has been removed from PubGrub's heap as pending.
    reschedule_sources: bool,
    /// The last range scheduled for prefetch for each undecided package.
    pre_visited: FxHashMap<Id<PubGrubPackage>, Range<Version>>,
    /// The last version selected for each package and range in a specific environment.
    selected_versions:
        FxHashMap<Id<PubGrubPackage>, (Range<Version>, Option<SelectionPolicy>, Version)>,
    /// Registry candidates tentatively selected in case a first-party path opts them in.
    /// The value records whether the exact candidate was yanked by its index.
    possible_candidates: FxHashMap<(Id<PubGrubPackage>, SolverVersion), bool>,
    /// The ordering used when each registry candidate was last selected in lowest-direct mode.
    selection_modes: FxHashMap<(Id<PubGrubPackage>, SolverVersion), bool>,
    /// Late local declarations whose support must survive a preferred-lowest retry.
    preferred_lowest: BTreeSet<PackageName>,
    /// Directories whose editable metadata should be consulted before a late provider is selected.
    preferred_editable: BTreeSet<SourceId>,
    /// Whether normal and editable dependencies were incorporated into this branch for a directory.
    directory_metadata_modes: BTreeMap<SourceId, (bool, bool)>,
    /// A cache for parsed version maps.
    ///
    /// Per fork, since the index for a package can differ between forks.
    known_versions: KnownVersions,
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
        indexes: Indexes,
    ) -> Self {
        let root = pubgrub.root_package;
        Self {
            continuation: ForkContinuation::Propagate,
            next: root,
            pubgrub,
            pins: FilePins::default(),
            indexes,
            priorities: PubGrubPriorities::default(),
            added_dependencies: FxHashMap::default(),
            source_dependencies: SourceDependencies::default(),
            pending_sources: FxHashMap::default(),
            source_assumptions: SourceAssumptions::default(),
            source_packages: vec![root],
            source_package_set: FxHashSet::from_iter([root]),
            reschedule_sources: false,
            pre_visited: FxHashMap::default(),
            selected_versions: FxHashMap::default(),
            possible_candidates: FxHashMap::default(),
            selection_modes: FxHashMap::default(),
            preferred_lowest: BTreeSet::new(),
            preferred_editable: BTreeSet::new(),
            directory_metadata_modes: BTreeMap::new(),
            known_versions: KnownVersions::default(),
            env,
            python_requirement,
            conflict_tracker: ConflictTracker::default(),
            prefetcher,
        }
    }

    fn remember_source_package(&mut self, package: Id<PubGrubPackage>) {
        if self.source_package_set.insert(package) {
            self.source_packages.push(package);
        }
    }

    /// Find a selected directory whose authored build mode differs from the metadata used by this
    /// branch. Partial assignments only require an early replay when they add editable authors.
    fn changed_directory_metadata(
        &self,
        grounding: &Grounding,
        urls: &Urls,
        only_new_editable: bool,
    ) -> Option<(SourceId, bool)> {
        self.directory_metadata_modes
            .iter()
            .find_map(|(source, (used_normal, used_editable))| {
                let selected =
                    self.pubgrub
                        .partial_solution
                        .extract_solution()
                        .any(|(id, candidate)| {
                            candidate.source == SolverSource::Url(*source)
                                && grounding.reachable.contains(&id)
                                && self.pubgrub.package_store[id]
                                    .name_no_root()
                                    .is_some_and(|name| grounding.contains(name, *source))
                        });
                if !selected {
                    return None;
                }
                let editable = grounding.url(*source, urls).is_editable();
                let changed = if editable {
                    *used_normal
                } else {
                    *used_editable && !only_new_editable
                };
                changed.then_some((*source, editable))
            })
    }

    fn pick_package(&mut self) -> Option<Id<PubGrubPackage>> {
        if !self.reschedule_sources {
            return self
                .pubgrub
                .partial_solution
                .pick_highest_priority_pkg(|id, _| {
                    self.priorities.get(&self.pubgrub.package_store[id])
                })
                .map(|(id, _)| id);
        }
        let selected: FxHashSet<_> = self
            .pubgrub
            .partial_solution
            .extract_solution()
            .map(|(id, _)| id)
            .collect();
        self.source_packages
            .iter()
            .enumerate()
            .filter_map(|(index, id)| {
                if selected.contains(id) {
                    return None;
                }
                let Some(Term::Positive(range)) = self
                    .pubgrub
                    .partial_solution
                    .term_intersection_for_package(*id)
                else {
                    return None;
                };
                if self.pending_sources.get(id) == Some(range) {
                    return None;
                }
                Some((index, *id))
            })
            .max_by_key(|(index, id)| {
                (
                    self.priorities.get(&self.pubgrub.package_store[*id]),
                    Reverse(*index),
                )
            })
            .map(|(_, id)| id)
    }

    fn pending_package(&self) -> Option<Id<PubGrubPackage>> {
        let selected: FxHashSet<_> = self
            .pubgrub
            .partial_solution
            .extract_solution()
            .map(|(id, _)| id)
            .collect();
        self.source_packages.iter().copied().find(|id| {
            !selected.contains(id)
                && self.pubgrub.partial_solution.term_intersection_for_package(*id).is_some_and(|term| {
                    matches!(term, Term::Positive(range) if self.pending_sources.get(id) == Some(range))
                })
        })
    }

    /// Register dependency priorities and warn about unbounded direct requirements.
    fn visit_package_version_dependencies(
        &mut self,
        for_package: Id<PubGrubPackage>,
        for_version: &SolverVersion,
        dependencies: &[PubGrubDependency],
        workspace_members: &BTreeSet<PackageName>,
        resolution_strategy: &ResolutionStrategy,
    ) {
        for dependency in dependencies {
            let PubGrubDependency {
                package,
                version,
                parent: _,
                source,
                policy: _,
            } = dependency;

            let has_url = source.verbatim_url().is_some();

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
                    .is_none_or(|(lowest, _highest)| lowest == Bound::Unbounded);
                let strategy_lowest = matches!(
                    resolution_strategy,
                    ResolutionStrategy::Lowest | ResolutionStrategy::LowestDirect(..)
                );

                if !has_url && missing_lower_bound && strategy_lowest {
                    let name = package.name_no_root().unwrap();
                    // Handle cases where a package is listed both without and with a lower bound.
                    // Example:
                    // ```
                    // "coverage[toml] ; python_version < '3.11'",
                    // "coverage >= 7.10.0",
                    // ```
                    let bound_on_other_package = dependencies.iter().any(|other| {
                        Some(name) == other.package.name()
                            && !other
                                .version
                                .bounding_range()
                                .is_none_or(|(lowest, _highest)| lowest == Bound::Unbounded)
                    });

                    if !bound_on_other_package {
                        warn_user_once!(
                            "The direct dependency `{name}` is unpinned. \
                            Consider setting a lower bound when using `--resolution lowest` \
                            or `--resolution lowest-direct` to avoid using outdated versions.",
                        );
                    }
                }
            }

            // Update the package priorities.
            self.priorities.insert(package, version, has_url);
            // As we're adding an incompatibility from the proxy package to the base package,
            // we need to register the base package.
            if let Some(base_package) = package.base_package() {
                self.priorities.insert(&base_package, version, has_url);
            }
        }
    }

    /// Adds the dependencies for the selected version of the current package.
    ///
    /// For registry packages, the depending version is widened across gaps containing no other
    /// known version before its incompatibilities are added. Packages without a complete registry
    /// version map retain the selected version's singleton range.
    fn add_package_version_dependencies<InstalledPackages: InstalledPackagesProvider>(
        &mut self,
        for_package: Id<PubGrubPackage>,
        for_version: &SolverVersion,
        dependencies: Vec<PubGrubDependency>,
        urls: &Urls,
        git: &GitResolver,
        index: &InMemoryIndex,
        installed_packages: &InstalledPackages,
    ) {
        if dependencies.iter().any(|dependency| {
            matches!(
                &dependency.source,
                DependencySource::Url { trusted: true, .. } | DependencySource::ExplicitIndex(_)
            )
        }) && let Some(chain) =
            DerivationChainBuilder::from_state(for_package, for_version, &self.pubgrub)
        {
            self.source_dependencies
                .set_chain(for_package, for_version.clone(), chain);
        }
        let is_proxy = self.pubgrub.package_store[for_package].is_proxy();
        // Only URL declarations consume the preferred authorized identities. Ordinary registry
        // dependencies and proxy links do not need to walk the selected graph to lower their ranges.
        let mut preferred: FxHashMap<_, Vec<_>> = if !is_proxy
            && (urls.has_potential() || self.source_dependencies.has_urls())
            && dependencies
                .iter()
                .any(|dependency| match &dependency.source {
                    DependencySource::Url { .. } => true,
                    DependencySource::Unspecified | DependencySource::ExplicitIndex(_) => false,
                }) {
            self.source_dependencies
                .grounding(
                    &self.pubgrub,
                    &self.env,
                    &self.python_requirement,
                    urls,
                    git,
                )
                .iter()
                .map(|(name, sources)| (name.clone(), sources.keys().copied().collect()))
                .collect()
        } else {
            FxHashMap::default()
        };
        let mut solved_dependencies = Vec::with_capacity(dependencies.len());
        for dependency in dependencies {
            let PubGrubDependency {
                package,
                version,
                parent: _,
                source,
                policy,
            } = dependency;

            let (candidates, declaration, explicit_index) = if is_proxy {
                (
                    CandidateSet::source(for_version.source, version),
                    None,
                    None,
                )
            } else if let DependencySource::Url {
                url,
                trusted,
                hash_requirement,
                trusted_hashes,
            } = &source
                && let Some(name) = package.name_no_root()
            {
                let preferred = preferred.entry(name.clone()).or_default();
                let source = urls.intern(name, url, git, preferred);
                if *trusted && !preferred.contains(&source) {
                    preferred.push(source);
                }
                // Registry Git references cannot authorize a source. Keep the URL dimension open
                // until an independent declaration can resolve and compare the exact reference;
                // multiple aliases must not conflict before that declaration becomes available.
                let candidates = if !*trusted && urls::git_url(&url.parsed_url).is_some() {
                    CandidateSet::urls(version)
                } else {
                    CandidateSet::source(SolverSource::Url(source), version)
                };
                (
                    candidates,
                    Some(UrlDeclaration {
                        source,
                        url: url.as_ref().clone(),
                        trusted: *trusted,
                        hash_requirement: hash_requirement.clone(),
                        trusted_hashes: *trusted_hashes,
                    }),
                    None,
                )
            } else if let DependencySource::ExplicitIndex(index) = &source {
                let index = self.indexes.intern(index);
                let candidates = if urls.has_potential() {
                    CandidateSet::index_or_url(index, version)
                } else {
                    CandidateSet::source(SolverSource::Index(index), version)
                };
                (candidates, None, Some(index))
            } else if !urls.has_potential()
                && !matches!(
                    &*package,
                    PubGrubPackageInner::Root(_)
                        | PubGrubPackageInner::Python(_)
                        | PubGrubPackageInner::System(_)
                )
            {
                let candidates = if package
                    .name_no_root()
                    .is_some_and(|name| self.indexes.contains_key(name))
                {
                    CandidateSet::registries(version)
                } else {
                    CandidateSet::source(SolverSource::Registry, version)
                };
                (candidates, None, None)
            } else if matches!(
                &*package,
                PubGrubPackageInner::Root(_)
                    | PubGrubPackageInner::Python(_)
                    | PubGrubPackageInner::System(_)
            ) {
                (
                    CandidateSet::source(SolverSource::Registry, version),
                    None,
                    None,
                )
            } else {
                (CandidateSet::all(version), None, None)
            };

            let package_id = self.pubgrub.package_store.alloc(package.clone());
            self.remember_source_package(package_id);
            if let Some(base_package) = package.base_package() {
                let base_package_id = self.pubgrub.package_store.alloc(base_package);
                self.remember_source_package(base_package_id);
                self.pubgrub.add_proxy_package_incompatibility(
                    package_id,
                    base_package_id,
                    candidates.clone(),
                );
            }
            solved_dependencies.push(SolvedDependency {
                package: package_id,
                candidates,
                declaration,
                index: explicit_index,
                policy,
            });
        }

        // Widen across gaps so rejected adjacent versions merge into contiguous ranges rather
        // than leaving one hole per version.
        let versions = self.widen_version_to_gap(for_version, index, installed_packages);
        self.source_dependencies.insert(
            for_package,
            for_version.clone(),
            solved_dependencies.clone(),
            for_package != self.pubgrub.root_package,
        );
        self.pending_sources.clear();
        let native_dependencies = solved_dependencies
            .into_iter()
            .map(|dependency| {
                (
                    self.pubgrub.package_store[dependency.package].clone(),
                    dependency.candidates,
                )
            })
            .collect::<Vec<_>>();
        let conflict = self.pubgrub.add_package_version_dependencies(
            for_package,
            for_version.clone(),
            versions,
            native_dependencies,
        );

        // Conflict tracking: If the version was rejected due to its dependencies, record culprit
        // and affected.
        if let Some(incompatibility) = conflict {
            self.record_conflict(for_package, Some(&for_version.version), incompatibility);
        }
    }

    /// Widens a version of the current package to the gap around it in the known versions.
    fn widen_version_to_gap<InstalledPackages: InstalledPackagesProvider>(
        &mut self,
        candidate: &SolverVersion,
        index: &InMemoryIndex,
        installed_packages: &InstalledPackages,
    ) -> CandidateSet {
        let package = &self.pubgrub.package_store[self.next];
        let explicit = if let SolverSource::Index(index) = candidate.source {
            Some(self.indexes.resource(index))
        } else {
            None
        };
        let known_versions = match candidate.source {
            SolverSource::Url(_) => None,
            SolverSource::Registry | SolverSource::Index(_) => self.known_versions.get_or_update(
                index,
                installed_packages,
                PackageSource::Registry(explicit.as_ref()),
                package,
            ),
        };
        CandidateSet::source(
            candidate.source,
            widen_to_gap(&candidate.version, known_versions),
        )
    }

    fn record_conflict(
        &mut self,
        affected: Id<PubGrubPackage>,
        version: Option<&Version>,
        incompatibility: IncompId<PubGrubPackage, CandidateSet, UnavailableReason>,
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

    /// Change the priority of often conflicting packages and backtrack.
    ///
    /// To be called after unit propagation.
    fn reprioritize_conflicts(&mut self) {
        for package in self.conflict_tracker.prioritize.drain(..) {
            let changed = self
                .priorities
                .mark_conflict_early(&self.pubgrub.package_store[package]);
            if changed {
                debug!(
                    "Package {} has too many conflicts (affected), prioritizing",
                    &self.pubgrub.package_store[package]
                );
            } else {
                debug!(
                    "Package {} has too many conflicts (affected), already {:?}",
                    self.pubgrub.package_store[package],
                    self.priorities.get(&self.pubgrub.package_store[package])
                );
            }
        }

        for package in self.conflict_tracker.deprioritize.drain(..) {
            let changed = self
                .priorities
                .mark_conflict_late(&self.pubgrub.package_store[package]);
            if changed {
                debug!(
                    "Package {} has too many conflicts (culprit), deprioritizing and backtracking",
                    self.pubgrub.package_store[package],
                );
                let backtrack_level = self.pubgrub.backtrack_package(package);
                if let Some(backtrack_level) = backtrack_level {
                    debug!("Backtracked {backtrack_level} decisions");
                } else {
                    debug!(
                        "Package {} is not decided, cannot backtrack",
                        self.pubgrub.package_store[package]
                    );
                }
            } else {
                debug!(
                    "Package {} has too many conflicts (culprit), already {:?}",
                    self.pubgrub.package_store[package],
                    self.priorities.get(&self.pubgrub.package_store[package])
                );
            }
        }
    }

    /// Records that a version cannot be used.
    ///
    /// The rejected version is widened to the gap around it, so that a run of rejected versions
    /// excludes one contiguous range. The gap holds no known version but the rejected one, and
    /// none at all when that version is itself unknown: `--exclude-newer` drops a version whose
    /// files carry no upload time from [`KnownVersions::get_or_update`], but every one of its
    /// distributions is incompatible, so it cannot be selected either.
    fn add_unavailable_version<InstalledPackages: InstalledPackagesProvider>(
        &mut self,
        version: SolverVersion,
        reason: UnavailableVersion,
        index: &InMemoryIndex,
        installed_packages: &InstalledPackages,
    ) {
        let versions = self.widen_version_to_gap(&version, index, installed_packages);

        // Incompatible requires-python versions are special in that we track
        // them as incompatible dependencies instead of marking the package version
        // as unavailable directly.
        if let UnavailableVersion::IncompatibleDist(
            IncompatibleDist::Source(IncompatibleSource::RequiresPython(requires_python, kind))
            | IncompatibleDist::Wheel(IncompatibleWheel::RequiresPython(requires_python, kind)),
        ) = reason
        {
            let package = self.next;
            let python = self.pubgrub.package_store.alloc(PubGrubPackage::from(
                PubGrubPackageInner::Python(match kind {
                    PythonRequirementKind::Installed => PubGrubPython::Installed,
                    PythonRequirementKind::Target => PubGrubPython::Target,
                }),
            ));
            self.remember_source_package(python);
            self.pubgrub
                .add_incompatibility(Incompatibility::from_dependency(
                    package,
                    versions,
                    (
                        python,
                        CandidateSet::source(
                            SolverSource::Registry,
                            Range::from_versions(release_specifiers_to_ranges(requires_python)),
                        ),
                    ),
                ));
            self.pubgrub
                .partial_solution
                .add_decision(self.next, version);
            return;
        }
        self.pubgrub
            .add_incompatibility(Incompatibility::custom_term(
                self.next,
                Term::Positive(versions),
                UnavailableReason::Version(reason),
            ));
    }

    fn with_continuation(mut self, continuation: ForkContinuation) -> Self {
        self.continuation = continuation;
        self
    }

    /// Narrow the environment and Python requirement, invalidating candidates from the parent fork.
    fn with_env(mut self, env: ResolverEnvironment) -> Self {
        self.selected_versions.clear();
        self.pending_sources.clear();
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
        candidate: &SolverVersion,
        urls: &Urls,
        grounding: &Grounding,
    ) -> (Option<VerbatimParsedUrl>, Option<&IndexUrl>) {
        match candidate.source {
            SolverSource::Url(source) => (Some(grounding.url(source, urls)), None),
            SolverSource::Registry | SolverSource::Index(_) => (
                None,
                self.pins
                    .get(name, candidate)
                    .expect("Every registry package should be pinned")
                    .index(),
            ),
        }
    }

    fn into_resolution(mut self, urls: &Urls, grounding: &Grounding) -> Resolution {
        let solution: FxHashMap<_, _> = self.pubgrub.partial_solution.extract_solution().collect();
        for (package, candidate) in &solution {
            if grounding.reachable.contains(package)
                && let Some(name) = self.pubgrub.package_store[*package].name_no_root()
            {
                let index = match candidate.source {
                    SolverSource::Index(index) => Some(self.indexes.resource(index)),
                    SolverSource::Registry | SolverSource::Url(_) => None,
                };
                self.pins.select(name, candidate, index);
            }
        }
        let edge_count: usize = solution
            .keys()
            .map(|package| self.pubgrub.incompatibilities[package].len())
            .sum();
        let mut edges: Vec<ResolutionDependencyEdge> = Vec::with_capacity(edge_count);
        for (package, self_version) in &solution {
            if !grounding.reachable.contains(package) {
                continue;
            }
            for id in &self.pubgrub.incompatibilities[package] {
                let incompatibility = &self.pubgrub.incompatibility_store[*id];
                let pubgrub::Kind::FromDependencyOf(self_package, dependency_package) =
                    &incompatibility.kind
                else {
                    continue;
                };
                let (self_package, dependency_package) = (*self_package, *dependency_package);
                if !grounding.reachable.contains(&dependency_package) {
                    continue;
                }
                let Some((self_range, dependency_range)) =
                    incompatibility.dependency_version_sets()
                else {
                    continue;
                };
                let dependency_range = dependency_range
                    .map_or_else(|| Cow::Owned(CandidateSet::empty()), Cow::Borrowed);
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

                let (name, extra, group, marker) = match &**dependency_package {
                    PubGrubPackageInner::Package {
                        name,
                        extra,
                        group,
                        marker,
                    } => {
                        debug_assert!(extra.is_none(), "Packages should depend on an extra proxy");
                        debug_assert!(group.is_none(), "Packages should depend on a group proxy");

                        // Ignore self-dependencies (e.g., `tensorflow-macos` depends on `tensorflow-macos`),
                        // but allow groups to depend on other groups, or on the package itself.
                        if self_group.is_none() && self_name == Some(name) {
                            continue;
                        }
                        (name, extra.as_ref(), group.as_ref(), *marker)
                    }
                    PubGrubPackageInner::Marker { name, marker } => {
                        if self_group.is_none() && self_name == Some(name) {
                            continue;
                        }
                        (name, None, None, *marker)
                    }
                    PubGrubPackageInner::Extra {
                        name,
                        extra,
                        marker,
                    } => {
                        if self_group.is_none() {
                            debug_assert!(self_name != Some(name), "Extras should be flattened");
                        }
                        (name, Some(extra), None, *marker)
                    }
                    PubGrubPackageInner::Group {
                        name,
                        group,
                        marker,
                    } => {
                        debug_assert!(self_name != Some(name), "Groups should be flattened");
                        (name, None, Some(group), *marker)
                    }
                    PubGrubPackageInner::Root(_)
                    | PubGrubPackageInner::Python(_)
                    | PubGrubPackageInner::System(_) => continue,
                };
                let from = self_name.map(|name| {
                    let (url, index) = self.source(name, self_version, urls, grounding);
                    ResolutionNode {
                        package: ResolutionPackage {
                            name: name.clone(),
                            extra: self_extra.cloned(),
                            dev: self_group.cloned(),
                            url,
                            index: index.cloned(),
                        },
                        version: self_version.version.clone(),
                    }
                });

                let (url, index) = self.source(name, dependency_version, urls, grounding);
                let to = ResolutionNode {
                    package: ResolutionPackage {
                        name: name.clone(),
                        extra: extra.cloned(),
                        dev: group.cloned(),
                        url,
                        index: index.cloned(),
                    },
                    version: dependency_version.version.clone(),
                };
                let edge = ResolutionDependencyEdge { from, to, marker };

                // An extra proxy requires both the extra and its base package. A group proxy
                // only requires the group itself.
                if let PubGrubPackageInner::Extra { .. } = &**dependency_package {
                    let mut base_edge = edge.clone();
                    base_edge.to.package.extra = None;
                    edges.push(edge);
                    edges.push(base_edge);
                } else {
                    edges.push(edge);
                }
            }
        }

        let nodes = solution
            .into_iter()
            .filter_map(|(package, version)| {
                if !grounding.reachable.contains(&package) {
                    return None;
                }
                if let PubGrubPackageInner::Package {
                    name,
                    extra,
                    group,
                    marker: MarkerTree::TRUE,
                } = &*self.pubgrub.package_store[package]
                {
                    let (url, index) = self.source(name, &version, urls, grounding);
                    Some((
                        ResolutionPackage {
                            name: name.clone(),
                            extra: extra.clone(),
                            dev: group.clone(),
                            url,
                            index: index.cloned(),
                        },
                        version.version,
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

/// Widens a single version to the largest interval that contains no other known version
/// ([`Ranges::widen_versions`]).
///
/// The interval adds only versions the registry does not list, which can never be selected, so an
/// incompatibility recorded for it holds for the same selectable versions.
///
/// Returns the singleton range when the known versions are unavailable, as for a URL or workspace
/// package, and for an empty list, which would otherwise widen to the full range.
fn widen_to_gap(version: &Version, known_versions: Option<&[Version]>) -> Range<Version> {
    let versions = Range::singleton(version.clone());
    match known_versions {
        Some(known_versions) if !known_versions.is_empty() => {
            versions.widen_versions(known_versions)
        }
        _ => versions,
    }
}

/// Fetch the metadata for an item
#[derive(Debug)]
#[expect(clippy::large_enum_variant)]
pub(crate) enum Request {
    /// A request to fetch the metadata for a package.
    Package(PackageName, Option<IndexMetadata>),
    /// A request to fetch the metadata for a built or source distribution.
    Dist(Dist, Option<HashStrategy>),
    /// A request to compare a Git reference without loading package metadata.
    GitReference(Box<GitUrl>, oneshot::Sender<()>),
    /// A request to fetch the metadata from an already-installed distribution.
    Installed(InstalledDist),
    /// A request to pre-fetch the metadata for a package and the best-guess distribution.
    Prefetch(PackageName, Range<Version>, PythonRequirement),
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
                Self::Dist(Dist::Source(SourceDist::Registry(source)), None)
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
                Self::Dist(Dist::Built(BuiltDist::Registry(built)), None)
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
            Self::Dist(dist, _) => {
                write!(f, "Metadata {dist}")
            }
            Self::GitReference(git, _) => {
                write!(f, "Git reference {git}")
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
#[expect(clippy::large_enum_variant)]
enum Response {
    /// The returned metadata for a package hosted on a registry.
    Package(PackageName, Option<IndexMetadata>, VersionsResponse),
    /// The returned metadata for a distribution.
    Dist {
        dist: Dist,
        metadata: MetadataResponse,
        direct_hashes: Option<DirectHashKey>,
    },
    /// The returned metadata for an already-installed distribution.
    Installed {
        dist: InstalledDist,
        metadata: MetadataResponse,
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
    /// These conflicts are resolved via [`ForkedDependencies::from_dependencies_universal`].
    Available(Vec<PubGrubDependency>),
    /// Package metadata has a `Requires-Python` specifier that is incompatible with the target.
    RequiresPython(VersionSpecifiers),
    /// Dependencies that should never result in a fork.
    ///
    /// For example, the dependencies of a `Marker` package will have the
    /// same name and version, but differ according to marker expressions.
    /// But we never want this to result in a fork.
    Unforkable(Vec<PubGrubDependency>),
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
        diverging_packages: BTreeSet<PackageName>,
        /// A conditional URL or first-party candidate policy requires decisions to be replayed.
        replay_candidates: bool,
    },
    /// Package metadata has a `Requires-Python` specifier that is incompatible with the target.
    RequiresPython(VersionSpecifiers),
}

impl ForkedDependencies {
    /// Turn a flat list of dependencies into a potential set of forked
    /// groups of dependencies.
    ///
    /// A fork *only* occurs when there are multiple dependencies with the same
    /// name *and* those dependency specifications have corresponding marker
    /// expressions that are completely disjoint with one another.
    fn from_dependencies_universal(
        dependencies: Dependencies,
        env: &ResolverEnvironment,
        python_requirement: &PythonRequirement,
        conflicts: &Conflicts,
    ) -> Self {
        let deps = match dependencies {
            Dependencies::Available(deps) => deps,
            Dependencies::Unforkable(deps) => return Self::Unforked(deps),
            Dependencies::RequiresPython(requires_python) => {
                return Self::RequiresPython(requires_python);
            }
            Dependencies::Unavailable(err) => return Self::Unavailable(err),
        };
        let replay_candidates = deps
            .iter()
            .any(|dependency| Self::conditional_candidate(dependency, env));
        let mut name_to_deps: BTreeMap<PackageName, Vec<PubGrubDependency>> = BTreeMap::new();
        for dep in deps {
            let name = dep
                .package
                .name()
                .expect("dependency always has a name")
                .clone();
            name_to_deps.entry(name).or_default().push(dep);
        }
        let (mut forks, diverging_packages) =
            Self::fork(name_to_deps, env, python_requirement, conflicts);
        if forks.is_empty() {
            Self::Unforked(vec![])
        } else if forks.len() == 1 {
            Self::Unforked(forks.pop().unwrap().dependencies)
        } else {
            Self::Forked {
                forks,
                diverging_packages,
                replay_candidates,
            }
        }
    }

    /// A conditional concrete source can change the metadata selected for an unmarked dependency.
    /// First-party candidate policies are split only when a selected yank or prerelease requires it.
    fn conditional_candidate(dependency: &PubGrubDependency, env: &ResolverEnvironment) -> bool {
        let marker = dependency.package.marker();
        (dependency.source.verbatim_url().is_some() || dependency.source.explicit_index().is_some())
            && env
                .fork_markers()
                .is_some_and(|fork| !fork.is_disjoint(marker) && !fork.is_disjoint(marker.negate()))
    }

    /// Noop companion to [`ForkedDependencies::from_dependencies_universal`] for non-universal
    /// resolutions with a fixed marker environment.
    fn from_dependencies_platform_specific(dependencies: Dependencies) -> Self {
        match dependencies {
            Dependencies::Available(deps) | Dependencies::Unforkable(deps) => Self::Unforked(deps),
            Dependencies::RequiresPython(requires_python) => Self::RequiresPython(requires_python),
            Dependencies::Unavailable(err) => Self::Unavailable(err),
        }
    }

    /// Build a list of forks determined from the dependencies of a single package.
    ///
    /// Any time a marker expression is seen that is not true for all possible
    /// marker environments, it is possible for it to introduce a new fork.
    ///
    /// Returns the forks discovered among the dependencies and the package(s) that
    /// provoked at least one additional fork.
    fn fork(
        name_to_deps: BTreeMap<PackageName, Vec<PubGrubDependency>>,
        env: &ResolverEnvironment,
        python_requirement: &PythonRequirement,
        conflicts: &Conflicts,
    ) -> (Vec<Fork>, BTreeSet<PackageName>) {
        let python_marker = python_requirement.to_marker_tree();

        let mut forks = vec![Fork::new(env.clone())];
        let mut diverging_packages = BTreeSet::new();
        for (name, mut deps) in name_to_deps {
            assert!(!deps.is_empty(), "every name has at least one dependency");
            // A conditional source or first-party candidate policy changes what another parent may
            // require without a marker. Its true and false environments need separate decisions
            // even if this parent has only one requirement for that name.
            let guarded_candidate = deps
                .iter()
                .any(|dependency| Self::conditional_candidate(dependency, env));
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
                if !guarded_candidate
                    && marker::requires_python(dep.package.marker())
                        .is_none_or(|bound| !python_requirement.raises(&bound))
                {
                    let dep = deps.pop().unwrap();
                    let marker = dep.package.marker();
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
                        if !guarded_candidate
                            && marker::requires_python(marker)
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
                let mut forker = match ForkingPossibility::new(env, &dep) {
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
                        new_fork.set_env(fork_env);
                        // We only add the dependency to this fork if it
                        // satisfies the fork's markers. Some forks are
                        // specifically created to exclude this dependency,
                        // so this isn't always true!
                        if forker.included(&new_fork.env) {
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
                // Check if this conflict set is relevant to this fork. We need two conditions:
                //
                // 1. At least one item has dependencies in this fork (otherwise there's nothing to
                //    fork on).
                // 2. At least two items are not already excluded in this fork's environment
                //    (otherwise the conflict constraint is already satisfied and no fork is
                //    needed).
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

                // If fewer than two items in this conflict set are still possible (not already
                // excluded) in this fork, the conflict constraint is already satisfied by prior
                // forking. We can skip the full N+1 fork split if the single remaining non-excluded
                // item doesn't appear in any other conflict set (since it would never need its own
                // "excluded" variant).
                let non_excluded: Vec<_> = set
                    .iter()
                    .filter(|item| fork.env.included_by_group(item.as_ref()))
                    .collect();
                if non_excluded.len() < 2 {
                    // Check if any non-excluded item still has a live conflict in another set —
                    // i.e., another set where this item AND at least one other non-excluded item
                    // both appear. If so, we still need to fork to create the "excluded" variant
                    // for that item.
                    let dominated = non_excluded.iter().all(|item| {
                        !conflicts.iter().any(|other_set| {
                            !std::ptr::eq(set, other_set)
                                && other_set.contains(item.package(), item.kind().as_ref())
                                && other_set
                                    .iter()
                                    .filter(|other_item| {
                                        other_item.package() != item.package()
                                            || other_item.kind() != item.kind()
                                    })
                                    .any(|other_item| {
                                        fork.env.included_by_group(other_item.as_ref())
                                    })
                        })
                    });
                    if dominated {
                        // When dependencies are added to forks, we check `included_by_marker` but
                        // not on whether the dependency's conflict item is included by the fork's
                        // environment so there may be extraneous dependencies and we need to filter
                        // the fork to clean up dependencies gated on already-excluded extras.
                        let rules: Vec<_> = set
                            .iter()
                            .filter(|item| !fork.env.included_by_group(item.as_ref()))
                            .cloned()
                            .map(Err)
                            .collect();
                        if let Some(filtered) = fork.filter(rules) {
                            new.push(filtered);
                        }
                        continue;
                    }
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
        (forks, diverging_packages)
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
    fn set_env(&mut self, env: ResolverEnvironment) {
        self.env = env;
        self.dependencies.retain(|dep| {
            let marker = dep.package.marker();
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

    /// Compare forks by their lower `requires-python` bounds.
    fn cmp_requires_python(&self, other: &Self) -> Ordering {
        cmp_requires_python(&self.env, &other.env)
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

/// Compare resolver environments by their lower Python bounds.
fn cmp_requires_python(
    self_env: &ResolverEnvironment,
    other_env: &ResolverEnvironment,
) -> Ordering {
    // A higher `requires-python` requirement indicates a _higher-priority_ fork.
    //
    // This ordering ensures that we prefer choosing the highest version for each fork based on
    // its `requires-python` requirement.
    //
    // The reverse would prefer choosing fewer versions, at the cost of using older package
    // versions on newer Python versions. For example, if reversed, we'd prefer to solve `<3.7
    // before solving `>=3.7`, since the resolution produced by the former might work for the
    // latter, but the inverse is unlikely to be true.
    let self_bound = self_env.requires_python().unwrap_or_default();
    let other_bound = other_env.requires_python().unwrap_or_default();
    self_bound.lower().cmp(other_bound.lower())
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
    version: &SolverVersion,
    pubgrub: &State<UvDependencyProvider>,
) -> ResolveError {
    let Some(name) = pubgrub.package_store[id].name_no_root() else {
        return error;
    };
    let chain = DerivationChainBuilder::from_state(id, version, pubgrub).unwrap_or_default();
    ResolveError::Dependencies(
        Box::new(error),
        name.clone(),
        version.version.clone(),
        chain,
    )
}

/// Find an index candidate whose only reported incompatibility is a yank.
fn possible_yanked_version(decision: Option<&ResolverVersion>) -> Option<&Version> {
    if let Some(ResolverVersion::Unavailable(
        version,
        UnavailableVersion::IncompatibleDist(
            IncompatibleDist::Wheel(IncompatibleWheel::Yanked(_))
            | IncompatibleDist::Source(IncompatibleSource::Yanked(_)),
        ),
    )) = decision
    {
        Some(version)
    } else {
        None
    }
}

/// Failures that belong to a direct candidate rather than the whole resolver or request channel.
fn is_source_error(error: &ResolveError) -> bool {
    match error {
        ResolveError::Dependencies(error, ..) => is_source_error(error),
        ResolveError::Distribution(_)
        | ResolveError::DistributionType(_)
        | ResolveError::Dist(..)
        | ResolveError::HashStrategy(_)
        | ResolveError::UnhashedPackage(_)
        | ResolveError::PackageUnavailable(_) => true,
        ResolveError::Client(_)
        | ResolveError::ChannelClosed
        | ResolveError::UnregisteredTask(_)
        | ResolveError::ConflictingUrls { .. }
        | ResolveError::ConflictingIndexesForEnvironment { .. }
        | ResolveError::ConflictingIndexes(..)
        | ResolveError::DisallowedUrl { .. }
        | ResolveError::NoSolution(_)
        | ResolveError::InvalidVersion(_)
        | ResolveError::ConflictingDistribution(_)
        | ResolveError::ConflictMarker(_)
        | ResolveError::MismatchedPackageName { .. } => false,
    }
}

/// Whether another included path could change the hash policy for this direct resource.
fn is_hash_source_error(error: &ResolveError) -> bool {
    match error {
        ResolveError::Dependencies(error, ..) => is_hash_source_error(error),
        ResolveError::HashStrategy(_) | ResolveError::UnhashedPackage(_) => true,
        ResolveError::Dist(_, _, _, error) => matches!(
            error.as_ref(),
            uv_distribution::Error::MismatchedHashes { .. }
                | uv_distribution::Error::MissingHashes { .. }
                | uv_distribution::Error::MissingActualHashes { .. }
                | uv_distribution::Error::MissingExpectedHashes { .. }
        ),
        _ => false,
    }
}

/// Compute the set of markers for which a package is known to be relevant.
fn find_environments(id: Id<PubGrubPackage>, state: &State<UvDependencyProvider>) -> MarkerTree {
    let package = &state.package_store[id];
    if package.is_root() {
        return MarkerTree::TRUE;
    }

    // First, collect the reverse-dependency closure for the package. We limit the propagation
    // below to this subgraph so cycles in unrelated packages don't matter here.
    let mut ancestors = FxHashSet::default();
    let mut stack = vec![id];
    let mut root = None;
    ancestors.insert(id);

    while let Some(current) = stack.pop() {
        let Some(incompatibilities) = state.incompatibilities.get(&current) else {
            continue;
        };

        for index in incompatibilities {
            let incompat = &state.incompatibility_store[*index];
            if let Kind::FromDependencyOf(parent, child) = &incompat.kind {
                if current != *child {
                    continue;
                }
                if ancestors.insert(*parent) {
                    if state.package_store[*parent].is_root() {
                        root = Some(*parent);
                    }
                    stack.push(*parent);
                }
            }
        }
    }

    let Some(root) = root else {
        return MarkerTree::FALSE;
    };

    // Propagate markers forward from the root through the collected subgraph. This reaches a
    // fixpoint even in the presence of cycles, unlike the recursive reverse walk above.
    let mut environments = FxHashMap::default();
    let mut queue = VecDeque::from([root]);
    environments.insert(root, MarkerTree::TRUE);

    while let Some(current) = queue.pop_front() {
        let Some(current_environment) = environments.get(&current).copied() else {
            continue;
        };
        let Some(incompatibilities) = state.incompatibilities.get(&current) else {
            continue;
        };

        for index in incompatibilities {
            let incompat = &state.incompatibility_store[*index];
            let Kind::FromDependencyOf(parent, child) = &incompat.kind else {
                continue;
            };
            if current != *parent || !ancestors.contains(child) {
                continue;
            }

            let mut next_environment = state.package_store[*child].marker();
            next_environment = next_environment.and(current_environment);

            let entry = environments.entry(*child).or_insert(MarkerTree::FALSE);
            let mut combined = *entry;
            combined = combined.or(next_environment);
            if combined != *entry {
                *entry = combined;
                queue.push_back(*child);
            }
        }
    }

    environments.remove(&id).unwrap_or(MarkerTree::FALSE)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn versions(versions: &[&str]) -> Vec<Version> {
        versions
            .iter()
            .map(|version| version.parse().expect("valid version"))
            .collect()
    }

    #[test]
    fn widens_a_version_to_its_gap() {
        let known_versions = versions(&["1.0", "2.0", "3.0"]);
        let version: Version = "2.0".parse().expect("valid version");

        // A version between two others widens to the open interval between them.
        assert_eq!(
            widen_to_gap(&version, Some(&known_versions)).to_string(),
            ">1.0, <3.0"
        );

        // At the ends of the listing the interval is unbounded.
        let version: Version = "1.0".parse().expect("valid version");
        assert_eq!(
            widen_to_gap(&version, Some(&known_versions)).to_string(),
            "<2.0"
        );
        let version: Version = "3.0".parse().expect("valid version");
        assert_eq!(
            widen_to_gap(&version, Some(&known_versions)).to_string(),
            ">2.0"
        );
    }

    #[test]
    fn widens_a_version_without_known_versions_to_itself() {
        let version: Version = "2.0".parse().expect("valid version");

        // A URL or workspace package has no registry version map to widen against.
        assert_eq!(
            widen_to_gap(&version, None),
            Range::singleton(version.clone())
        );

        // An empty list would otherwise widen to the full range.
        assert_eq!(widen_to_gap(&version, Some(&[])), Range::singleton(version));
    }
}
