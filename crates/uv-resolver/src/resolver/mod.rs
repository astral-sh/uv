//! Given a set of requirements, find a set of compatible packages.

use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::sync::Arc;
use std::thread;

use anyhow::Result;
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
    InstalledDist, RemoteSource, Requirement, ResolvedDist, ResolvedDistRef, SourceDist,
    VersionOrUrlRef,
};
pub(crate) use locals::Locals;
use pep440_rs::{Version, MIN_VERSION};
use pep508_rs::MarkerEnvironment;
use platform_tags::Tags;
use pypi_types::Metadata23;
pub(crate) use urls::Urls;
use uv_configuration::{Constraints, Overrides};
use uv_distribution::{ArchiveMetadata, DistributionDatabase};
use uv_normalize::PackageName;
use uv_types::{BuildContext, HashStrategy, InstalledPackagesProvider};

use crate::candidate_selector::{CandidateDist, CandidateSelector};
use crate::dependency_provider::UvDependencyProvider;
use crate::editables::Editables;
use crate::error::ResolveError;
use crate::manifest::Manifest;
use crate::pins::FilePins;
use crate::preferences::Preferences;
use crate::pubgrub::{
    PubGrubDependencies, PubGrubDistribution, PubGrubPackage, PubGrubPriorities, PubGrubPython,
    PubGrubRequirement, PubGrubSpecifier,
};
use crate::python_requirement::PythonRequirement;
use crate::resolution::ResolutionGraph;
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

mod batch_prefetch;
mod index;
mod locals;
mod provider;
mod reporter;
mod urls;

/// The reason why a package or a version cannot be used.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum UnavailableReason {
    /// The entire package cannot be used.
    Package(UnavailablePackage),
    /// A single version cannot be used.
    Version(UnavailableVersion),
}

impl Display for UnavailableReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Version(version) => Display::fmt(version, f),
            Self::Package(package) => Display::fmt(package, f),
        }
    }
}

/// The package version is unavailable and cannot be used. Unlike [`PackageUnavailable`], this
/// applies to a single version of the package.
///
/// Most variant are from [`MetadataResponse`] without the error source (since we don't format
/// the source).
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum UnavailableVersion {
    /// Version is incompatible because it has no usable distributions
    IncompatibleDist(IncompatibleDist),
    /// The wheel metadata was found, but could not be parsed.
    InvalidMetadata,
    /// The wheel metadata was found, but the metadata was inconsistent.
    InconsistentMetadata,
    /// The wheel has an invalid structure.
    InvalidStructure,
    /// The wheel metadata was not found in the cache and the network is not available.
    Offline,
    /// Forward any kind of resolver error.
    ResolverError(String),
}

impl Display for UnavailableVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            UnavailableVersion::IncompatibleDist(invalid_dist) => Display::fmt(invalid_dist, f),
            UnavailableVersion::InvalidMetadata => f.write_str("has invalid metadata"),
            UnavailableVersion::InconsistentMetadata => f.write_str("has inconsistent metadata"),
            UnavailableVersion::InvalidStructure => f.write_str("has an invalid package format"),
            UnavailableVersion::Offline => f.write_str(
                "network connectivity is disabled, but the metadata wasn't found in the cache",
            ),
            UnavailableVersion::ResolverError(err) => f.write_str(err),
        }
    }
}

/// The package is unavailable and cannot be used.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum UnavailablePackage {
    /// Index lookups were disabled (i.e., `--no-index`) and the package was not found in a flat index (i.e. from `--find-links`).
    NoIndex,
    /// Network requests were disabled (i.e., `--offline`), and the package was not found in the cache.
    Offline,
    /// The package was not found in the registry.
    NotFound,
    /// The package metadata was found, but could not be parsed.
    InvalidMetadata(String),
    /// The package has an invalid structure.
    InvalidStructure(String),
}

impl UnavailablePackage {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            UnavailablePackage::NoIndex => "was not found in the provided package locations",
            UnavailablePackage::Offline => "was not found in the cache",
            UnavailablePackage::NotFound => "was not found in the package registry",
            UnavailablePackage::InvalidMetadata(_) => "has invalid metadata",
            UnavailablePackage::InvalidStructure(_) => "has an invalid package format",
        }
    }
}

impl Display for UnavailablePackage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The package is unavailable at specific versions.
#[derive(Debug, Clone)]
pub(crate) enum IncompletePackage {
    /// Network requests were disabled (i.e., `--offline`), and the wheel metadata was not found in the cache.
    Offline,
    /// The wheel metadata was found, but could not be parsed.
    InvalidMetadata(String),
    /// The wheel metadata was found, but the metadata was inconsistent.
    InconsistentMetadata(String),
    /// The wheel has an invalid structure.
    InvalidStructure(String),
}

enum ResolverVersion {
    /// A usable version
    Available(Version),
    /// A version that is not usable for some reason
    Unavailable(Version, UnavailableVersion),
}

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
    preferences: Preferences,
    exclusions: Exclusions,
    editables: Editables,
    urls: Urls,
    locals: Locals,
    dependency_mode: DependencyMode,
    hasher: HashStrategy,
    /// When not set, the resolver is in "universal" mode.
    markers: Option<MarkerEnvironment>,
    python_requirement: PythonRequirement,
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
        tags: &'a Tags,
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
            build_context.no_binary(),
            build_context.no_build(),
        );

        Self::new_custom_io(
            manifest,
            options,
            hasher,
            markers,
            python_requirement,
            index,
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
        provider: Provider,
        installed_packages: InstalledPackages,
    ) -> Result<Self, ResolveError> {
        let state = ResolverState {
            index: index.clone(),
            unavailable_packages: DashMap::default(),
            incomplete_packages: DashMap::default(),
            selector: CandidateSelector::for_resolution(options, &manifest, markers),
            dependency_mode: options.dependency_mode,
            urls: Urls::from_manifest(&manifest, markers, options.dependency_mode)?,
            locals: Locals::from_manifest(&manifest, markers, options.dependency_mode),
            project: manifest.project,
            requirements: manifest.requirements,
            constraints: manifest.constraints,
            overrides: manifest.overrides,
            preferences: Preferences::from_iter(manifest.preferences, markers),
            exclusions: manifest.exclusions,
            editables: Editables::from_requirements(manifest.editables),
            hasher: hasher.clone(),
            markers: markers.cloned(),
            python_requirement: python_requirement.clone(),
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
                        err.with_available_versions(
                            &state.python_requirement,
                            &visited,
                            state.index.packages(),
                        )
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
        let root = PubGrubPackage::Root(self.project.clone());
        let mut prefetcher = BatchPrefetcher::default();
        let mut state = SolveState {
            pubgrub: State::init(root.clone(), MIN_VERSION.clone()),
            next: root,
            pins: FilePins::default(),
            priorities: PubGrubPriorities::default(),
            added_dependencies: FxHashMap::default(),
        };

        debug!(
            "Solving with target Python version {}",
            self.python_requirement.target()
        );

        loop {
            // Run unit propagation.
            state.pubgrub.unit_propagation(state.next)?;

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
                let selection = state.pubgrub.partial_solution.extract_solution();
                return ResolutionGraph::from_state(
                    &selection,
                    &state.pins,
                    self.index.packages(),
                    self.index.distributions(),
                    &state.pubgrub,
                    &self.preferences,
                    self.editables.clone(),
                );
            };
            state.next = highest_priority_pkg;

            prefetcher.version_tried(state.next.clone());

            let term_intersection = state
                .pubgrub
                .partial_solution
                .term_intersection_for_package(&state.next)
                .ok_or_else(|| {
                    PubGrubError::Failure("a package was chosen but we don't have a term.".into())
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
                    if let PubGrubPackage::Package(ref package_name, _, _) = state.next {
                        if let Some(entry) = self.unavailable_packages.get(package_name) {
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
                        ))
                        | IncompatibleDist::Wheel(IncompatibleWheel::RequiresPython(requires_python)),
                    ) = reason
                    {
                        let python_version = requires_python
                            .iter()
                            .map(PubGrubSpecifier::try_from)
                            .fold_ok(Range::full(), |range, specifier| {
                                range.intersection(&specifier.into())
                            })?;

                        let package = &state.next;
                        for kind in [PubGrubPython::Installed, PubGrubPython::Target] {
                            state
                                .pubgrub
                                .add_incompatibility(Incompatibility::from_dependency(
                                    package.clone(),
                                    Range::singleton(version.clone()),
                                    (PubGrubPackage::Python(kind), python_version.clone()),
                                ));
                        }
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
                let package = &state.next;
                let dependencies = match self.get_dependencies(
                    package,
                    &version,
                    &mut state.priorities,
                    &request_sink,
                )? {
                    Dependencies::Unavailable(reason) => {
                        state
                            .pubgrub
                            .add_incompatibility(Incompatibility::custom_version(
                                package.clone(),
                                version.clone(),
                                UnavailableReason::Version(reason),
                            ));
                        continue;
                    }
                    Dependencies::Available(constraints)
                        if constraints
                            .iter()
                            .any(|(dependency, _)| dependency == package) =>
                    {
                        if enabled!(Level::DEBUG) {
                            prefetcher.log_tried_versions();
                        }
                        return Err(PubGrubError::SelfDependency {
                            package: package.clone(),
                            version: version.clone(),
                        }
                        .into());
                    }
                    Dependencies::Available(constraints) => constraints,
                };

                // Add that package and version if the dependencies are not problematic.
                let dep_incompats = state.pubgrub.add_incompatibility_from_dependencies(
                    package.clone(),
                    version.clone(),
                    dependencies,
                );

                state.pubgrub.partial_solution.add_version(
                    package.clone(),
                    version,
                    dep_incompats,
                    &state.pubgrub.incompatibility_store,
                );
            } else {
                // `dep_incompats` are already in `incompatibilities` so we know there are not satisfied
                // terms and can add the decision directly.
                state
                    .pubgrub
                    .partial_solution
                    .add_decision(state.next.clone(), version);
            }
        }
    }

    /// Visit a [`PubGrubPackage`] prior to selection. This should be called on a [`PubGrubPackage`]
    /// before it is selected, to allow metadata to be fetched in parallel.
    fn visit_package(
        &self,
        package: &PubGrubPackage,
        request_sink: &Sender<Request>,
    ) -> Result<(), ResolveError> {
        match package {
            PubGrubPackage::Root(_) => {}
            PubGrubPackage::Python(_) => {}
            PubGrubPackage::Extra(_, _, _) => {}
            PubGrubPackage::Package(name, _extra, None) => {
                // Verify that the package is allowed under the hash-checking policy.
                if !self.hasher.allows_package(name) {
                    return Err(ResolveError::UnhashedPackage(name.clone()));
                }

                // Emit a request to fetch the metadata for this package.
                if self.index.packages().register(name.clone()) {
                    request_sink.blocking_send(Request::Package(name.clone()))?;
                }
            }
            PubGrubPackage::Package(name, _extra, Some(url)) => {
                // Verify that the package is allowed under the hash-checking policy.
                if !self.hasher.allows_url(&url.verbatim) {
                    return Err(ResolveError::UnhashedPackage(name.clone()));
                }

                // If the package is an editable, we don't need to fetch metadata.
                if self.editables.contains(name) {
                    return Ok(());
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
    /// metadata for all of the packages in parallel.
    fn pre_visit<'data>(
        packages: impl Iterator<Item = (&'data PubGrubPackage, &'data Range<Version>)>,
        request_sink: &Sender<Request>,
    ) -> Result<(), ResolveError> {
        // Iterate over the potential packages, and fetch file metadata for any of them. These
        // represent our current best guesses for the versions that we _might_ select.
        for (package, range) in packages {
            let PubGrubPackage::Package(package_name, None, None) = package else {
                continue;
            };
            request_sink.blocking_send(Request::Prefetch(package_name.clone(), range.clone()))?;
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
        match package {
            PubGrubPackage::Root(_) => Ok(Some(ResolverVersion::Available(MIN_VERSION.clone()))),

            PubGrubPackage::Python(PubGrubPython::Installed) => {
                let version = self.python_requirement.installed();
                if range.contains(version) {
                    Ok(Some(ResolverVersion::Available(version.deref().clone())))
                } else {
                    Ok(None)
                }
            }

            PubGrubPackage::Python(PubGrubPython::Target) => {
                let version = self.python_requirement.target();
                if range.contains(version) {
                    Ok(Some(ResolverVersion::Available(version.deref().clone())))
                } else {
                    Ok(None)
                }
            }

            PubGrubPackage::Extra(package_name, _, Some(url))
            | PubGrubPackage::Package(package_name, _, Some(url)) => {
                debug!(
                    "Searching for a compatible version of {package} @ {} ({range})",
                    url.verbatim
                );

                // If the dist is an editable, return the version from the editable metadata.
                if let Some(editable) = self.editables.get(package_name) {
                    let version = &editable.metadata.version;

                    // The version is incompatible with the requirement.
                    if !range.contains(version) {
                        return Ok(None);
                    }

                    // The version is incompatible due to its Python requirement.
                    if let Some(requires_python) = editable.metadata.requires_python.as_ref() {
                        let target = self.python_requirement.target();
                        if !requires_python.contains(target) {
                            return Ok(Some(ResolverVersion::Unavailable(
                                version.clone(),
                                UnavailableVersion::IncompatibleDist(IncompatibleDist::Source(
                                    IncompatibleSource::RequiresPython(requires_python.clone()),
                                )),
                            )));
                        }
                    }

                    return Ok(Some(ResolverVersion::Available(version.clone())));
                }

                let dist = PubGrubDistribution::from_url(package_name, url);
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
                            .insert(package_name.clone(), UnavailablePackage::Offline);
                        return Ok(None);
                    }
                    MetadataResponse::InvalidMetadata(err) => {
                        self.unavailable_packages.insert(
                            package_name.clone(),
                            UnavailablePackage::InvalidMetadata(err.to_string()),
                        );
                        return Ok(None);
                    }
                    MetadataResponse::InconsistentMetadata(err) => {
                        self.unavailable_packages.insert(
                            package_name.clone(),
                            UnavailablePackage::InvalidMetadata(err.to_string()),
                        );
                        return Ok(None);
                    }
                    MetadataResponse::InvalidStructure(err) => {
                        self.unavailable_packages.insert(
                            package_name.clone(),
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
                    let target = self.python_requirement.target();
                    if !requires_python.contains(target) {
                        return Ok(Some(ResolverVersion::Unavailable(
                            version.clone(),
                            UnavailableVersion::IncompatibleDist(IncompatibleDist::Source(
                                IncompatibleSource::RequiresPython(requires_python.clone()),
                            )),
                        )));
                    }
                }

                Ok(Some(ResolverVersion::Available(version.clone())))
            }

            PubGrubPackage::Extra(package_name, _, None)
            | PubGrubPackage::Package(package_name, _, None) => {
                // Wait for the metadata to be available.
                let versions_response = self
                    .index
                    .packages()
                    .wait_blocking(package_name)
                    .ok_or(ResolveError::Unregistered)?;
                visited.insert(package_name.clone());

                let version_maps = match *versions_response {
                    VersionsResponse::Found(ref version_maps) => version_maps.as_slice(),
                    VersionsResponse::NoIndex => {
                        self.unavailable_packages
                            .insert(package_name.clone(), UnavailablePackage::NoIndex);
                        &[]
                    }
                    VersionsResponse::Offline => {
                        self.unavailable_packages
                            .insert(package_name.clone(), UnavailablePackage::Offline);
                        &[]
                    }
                    VersionsResponse::NotFound => {
                        self.unavailable_packages
                            .insert(package_name.clone(), UnavailablePackage::NotFound);
                        &[]
                    }
                };

                debug!("Searching for a compatible version of {package} ({range})");

                // Find a version.
                let Some(candidate) = self.selector.select(
                    package_name,
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
                if matches!(package, PubGrubPackage::Package(_, _, _)) {
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
    fn get_dependencies(
        &self,
        package: &PubGrubPackage,
        version: &Version,
        priorities: &mut PubGrubPriorities,
        request_sink: &Sender<Request>,
    ) -> Result<Dependencies, ResolveError> {
        match package {
            PubGrubPackage::Root(_) => {
                // Add the root requirements.
                let dependencies = PubGrubDependencies::from_requirements(
                    &self.requirements,
                    &self.constraints,
                    &self.overrides,
                    None,
                    None,
                    &self.urls,
                    &self.locals,
                    self.markers.as_ref(),
                );

                let mut dependencies = match dependencies {
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

                // Add a dependency on each editable.
                for editable in self.editables.iter() {
                    let package = PubGrubPackage::from_package(
                        editable.metadata.name.clone(),
                        None,
                        &self.urls,
                    );
                    let version = Range::singleton(editable.metadata.version.clone());

                    // Update the package priorities.
                    priorities.insert(&package, &version);

                    // Add the editable as a direct dependency.
                    dependencies.push(package, version);

                    // Add a dependency on each extra.
                    for extra in &editable.built.extras {
                        dependencies.push(
                            PubGrubPackage::from_package(
                                editable.metadata.name.clone(),
                                Some(extra.clone()),
                                &self.urls,
                            ),
                            Range::singleton(editable.metadata.version.clone()),
                        );
                    }

                    // Add any constraints.
                    for constraint in self
                        .constraints
                        .get(&editable.metadata.name)
                        .into_iter()
                        .flatten()
                    {
                        if constraint.evaluate_markers(self.markers.as_ref(), &[]) {
                            let PubGrubRequirement { package, version } =
                                PubGrubRequirement::from_constraint(
                                    constraint,
                                    &self.urls,
                                    &self.locals,
                                )?;
                            dependencies.push(package, version);
                        }
                    }
                }

                Ok(Dependencies::Available(dependencies.into()))
            }

            PubGrubPackage::Python(_) => Ok(Dependencies::Available(Vec::default())),

            PubGrubPackage::Package(package_name, extra, url) => {
                // If we're excluding transitive dependencies, short-circuit.
                if self.dependency_mode.is_direct() {
                    // If an extra is provided, wait for the metadata to be available, since it's
                    // still required for generating the lock file.
                    if !self.editables.contains(package_name) {
                        // Determine the distribution to lookup.
                        let dist = match url {
                            Some(url) => PubGrubDistribution::from_url(package_name, url),
                            None => PubGrubDistribution::from_registry(package_name, version),
                        };
                        let version_id = dist.version_id();

                        // Wait for the metadata to be available.
                        self.index
                            .distributions()
                            .wait_blocking(&version_id)
                            .ok_or(ResolveError::Unregistered)?;
                    }

                    return Ok(Dependencies::Available(Vec::default()));
                }

                // Determine if the distribution is editable.
                if let Some(editable) = self.editables.get(package_name) {
                    let requirements: Vec<_> = editable
                        .metadata
                        .requires_dist
                        .iter()
                        .cloned()
                        .map(Requirement::from_pep508)
                        .collect::<Result<_, _>>()?;
                    let dependencies = PubGrubDependencies::from_requirements(
                        &requirements,
                        &self.constraints,
                        &self.overrides,
                        Some(package_name),
                        extra.as_ref(),
                        &self.urls,
                        &self.locals,
                        self.markers.as_ref(),
                    )?;

                    for (dep_package, dep_version) in dependencies.iter() {
                        debug!("Adding transitive dependency for {package}=={version}: {dep_package}{dep_version}");

                        // Update the package priorities.
                        priorities.insert(dep_package, dep_version);

                        // Emit a request to fetch the metadata for this package.
                        self.visit_package(dep_package, request_sink)?;
                    }

                    return Ok(Dependencies::Available(dependencies.into()));
                }

                // Determine the distribution to lookup.
                let dist = match url {
                    Some(url) => PubGrubDistribution::from_url(package_name, url),
                    None => PubGrubDistribution::from_registry(package_name, version),
                };
                let version_id = dist.version_id();

                // If the package does not exist in the registry or locally, we cannot fetch its dependencies
                if self.unavailable_packages.get(package_name).is_some()
                    && self
                        .installed_packages
                        .get_packages(package_name)
                        .is_empty()
                {
                    debug_assert!(
                        false,
                        "Dependencies were requested for a package that is not available"
                    );
                    return Err(ResolveError::Failure(format!(
                        "The package is unavailable: {package_name}"
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
                            .entry(package_name.clone())
                            .or_default()
                            .insert(version.clone(), IncompletePackage::Offline);
                        return Ok(Dependencies::Unavailable(UnavailableVersion::Offline));
                    }
                    MetadataResponse::InvalidMetadata(err) => {
                        warn!("Unable to extract metadata for {package_name}: {err}");
                        self.incomplete_packages
                            .entry(package_name.clone())
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
                        warn!("Unable to extract metadata for {package_name}: {err}");
                        self.incomplete_packages
                            .entry(package_name.clone())
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
                        warn!("Unable to extract metadata for {package_name}: {err}");
                        self.incomplete_packages
                            .entry(package_name.clone())
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

                let requirements: Vec<_> = metadata
                    .requires_dist
                    .iter()
                    .cloned()
                    .map(Requirement::from_pep508)
                    .collect::<Result<_, _>>()?;
                let dependencies = PubGrubDependencies::from_requirements(
                    &requirements,
                    &self.constraints,
                    &self.overrides,
                    Some(package_name),
                    extra.as_ref(),
                    &self.urls,
                    &self.locals,
                    self.markers.as_ref(),
                )?;

                for (dep_package, dep_version) in dependencies.iter() {
                    debug!("Adding transitive dependency for {package}=={version}: {dep_package}{dep_version}");

                    // Update the package priorities.
                    priorities.insert(dep_package, dep_version);

                    // Emit a request to fetch the metadata for this package.
                    self.visit_package(dep_package, request_sink)?;
                }

                Ok(Dependencies::Available(dependencies.into()))
            }

            // Add a dependency on both the extra and base package.
            PubGrubPackage::Extra(package_name, extra, url) => Ok(Dependencies::Available(vec![
                (
                    PubGrubPackage::Package(package_name.clone(), None, url.clone()),
                    Range::singleton(version.clone()),
                ),
                (
                    PubGrubPackage::Package(package_name.clone(), Some(extra.clone()), url.clone()),
                    Range::singleton(version.clone()),
                ),
            ])),
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
                        Arc::new(MetadataResponse::Found(ArchiveMetadata::from(metadata))),
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
            match package {
                PubGrubPackage::Root(_) => {}
                PubGrubPackage::Python(_) => {}
                PubGrubPackage::Extra(_, _, _) => {}
                PubGrubPackage::Package(package_name, _extra, Some(url)) => {
                    reporter.on_progress(package_name, &VersionOrUrlRef::Url(&url.verbatim));
                }
                PubGrubPackage::Package(package_name, _extra, None) => {
                    reporter.on_progress(package_name, &VersionOrUrlRef::Version(version));
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
        // add a method to create a `Dist`, but a `Dist` cannot reprented an
        // installed dist.
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

/// An enum used by [`DependencyProvider`] that holds information about package dependencies.
/// For each [Package] there is a set of versions allowed as a dependency.
#[derive(Clone)]
enum Dependencies {
    /// Package dependencies are not available.
    Unavailable(UnavailableVersion),
    /// Container for all available package versions.
    Available(Vec<(PubGrubPackage, Range<Version>)>),
}

fn uncapitalize<T: AsRef<str>>(string: T) -> String {
    let mut chars = string.as_ref().chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_lowercase().chain(chars).collect(),
    }
}
