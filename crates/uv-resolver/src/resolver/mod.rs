//! Given a set of requirements, find a set of compatible packages.

use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::sync::Arc;

use anyhow::Result;
use dashmap::{DashMap, DashSet};
use futures::{FutureExt, StreamExt};
use itertools::Itertools;
use pubgrub::error::PubGrubError;
use pubgrub::range::Range;
use pubgrub::solver::{Incompatibility, State};
use rustc_hash::{FxHashMap, FxHashSet};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, enabled, info_span, instrument, trace, warn, Instrument, Level};

use distribution_types::{
    BuiltDist, Dist, DistributionMetadata, IncompatibleDist, IncompatibleSource, IncompatibleWheel,
    InstalledDist, RemoteSource, ResolvedDist, ResolvedDistRef, SourceDist, UvRequirement,
    VersionOrUrl,
};
pub(crate) use locals::Locals;
use pep440_rs::{Version, MIN_VERSION};
use pep508_rs::MarkerEnvironment;
use platform_tags::Tags;
use pypi_types::Metadata23;
pub(crate) use urls::Urls;
use uv_client::RegistryClient;
use uv_configuration::{Constraints, Overrides};
use uv_distribution::{ArchiveMetadata, DistributionDatabase};
use uv_interpreter::Interpreter;
use uv_normalize::PackageName;
use uv_types::{BuildContext, HashStrategy, InstalledPackagesProvider};

use crate::candidate_selector::{CandidateDist, CandidateSelector};
use crate::editables::Editables;
use crate::error::ResolveError;
use crate::manifest::Manifest;
use crate::pins::FilePins;
use crate::preferences::Preferences;
use crate::pubgrub::{
    PubGrubDependencies, PubGrubDistribution, PubGrubPackage, PubGrubPriorities, PubGrubPython,
    PubGrubSpecifier,
};
use crate::python_requirement::PythonRequirement;
use crate::resolution::ResolutionGraph;
use crate::resolver::batch_prefetch::BatchPrefetcher;
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

/// The package version is unavailable and cannot be used
/// Unlike [`PackageUnavailable`] this applies to a single version of the package
#[derive(Debug, Clone)]
pub(crate) enum UnavailableVersion {
    /// Version is incompatible because it has no usable distributions
    IncompatibleDist(IncompatibleDist),
}

/// The package is unavailable and cannot be used.
#[derive(Debug, Clone)]
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

pub struct Resolver<
    'a,
    Provider: ResolverProvider,
    InstalledPackages: InstalledPackagesProvider + Send + Sync,
> {
    project: Option<PackageName>,
    requirements: Vec<UvRequirement>,
    constraints: Constraints,
    overrides: Overrides,
    preferences: Preferences,
    exclusions: Exclusions,
    editables: Editables,
    urls: Urls,
    locals: Locals,
    dependency_mode: DependencyMode,
    hasher: &'a HashStrategy,
    markers: &'a MarkerEnvironment,
    python_requirement: PythonRequirement,
    selector: CandidateSelector,
    index: &'a InMemoryIndex,
    installed_packages: &'a InstalledPackages,
    /// Incompatibilities for packages that are entirely unavailable.
    unavailable_packages: DashMap<PackageName, UnavailablePackage>,
    /// Incompatibilities for packages that are unavailable at specific versions.
    incomplete_packages: DashMap<PackageName, DashMap<Version, IncompletePackage>>,
    /// The set of all registry-based packages visited during resolution.
    visited: DashSet<PackageName>,
    reporter: Option<Arc<dyn Reporter>>,
    provider: Provider,
}

impl<
        'a,
        Context: BuildContext + Send + Sync,
        InstalledPackages: InstalledPackagesProvider + Send + Sync,
    > Resolver<'a, DefaultResolverProvider<'a, Context>, InstalledPackages>
{
    /// Initialize a new resolver using the default backend doing real requests.
    ///
    /// Reads the flat index entries.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        manifest: Manifest,
        options: Options,
        markers: &'a MarkerEnvironment,
        interpreter: &'a Interpreter,
        tags: &'a Tags,
        client: &'a RegistryClient,
        flat_index: &'a FlatIndex,
        index: &'a InMemoryIndex,
        hasher: &'a HashStrategy,
        build_context: &'a Context,
        installed_packages: &'a InstalledPackages,
    ) -> Result<Self, ResolveError> {
        let provider = DefaultResolverProvider::new(
            client,
            DistributionDatabase::new(client, build_context),
            flat_index,
            tags,
            PythonRequirement::new(interpreter, markers),
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
            PythonRequirement::new(interpreter, markers),
            index,
            provider,
            installed_packages,
        )
    }
}

impl<
        'a,
        Provider: ResolverProvider,
        InstalledPackages: InstalledPackagesProvider + Send + Sync,
    > Resolver<'a, Provider, InstalledPackages>
{
    /// Initialize a new resolver using a user provided backend.
    #[allow(clippy::too_many_arguments)]
    pub fn new_custom_io(
        manifest: Manifest,
        options: Options,
        hasher: &'a HashStrategy,
        markers: &'a MarkerEnvironment,
        python_requirement: PythonRequirement,
        index: &'a InMemoryIndex,
        provider: Provider,
        installed_packages: &'a InstalledPackages,
    ) -> Result<Self, ResolveError> {
        Ok(Self {
            index,
            unavailable_packages: DashMap::default(),
            incomplete_packages: DashMap::default(),
            visited: DashSet::default(),
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
            hasher,
            markers,
            python_requirement,
            reporter: None,
            provider,
            installed_packages,
        })
    }

    /// Set the [`Reporter`] to use for this installer.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        let reporter = Arc::new(reporter);
        Self {
            reporter: Some(reporter.clone()),
            provider: self.provider.with_reporter(Facade { reporter }),
            ..self
        }
    }

    /// Resolve a set of requirements into a set of pinned versions.
    pub async fn resolve(self) -> Result<ResolutionGraph, ResolveError> {
        // A channel to fetch package metadata (e.g., given `flask`, fetch all versions) and version
        // metadata (e.g., given `flask==1.0.0`, fetch the metadata for that version).
        // Channel size is set large to accommodate batch prefetching.
        let (request_sink, request_stream) = tokio::sync::mpsc::channel(300);

        // Run the fetcher.
        let requests_fut = self.fetch(request_stream).fuse();

        // Run the solver.
        let resolve_fut = self.solve(request_sink).boxed().fuse();

        // Wait for both to complete.
        match tokio::try_join!(requests_fut, resolve_fut) {
            Ok(((), resolution)) => {
                self.on_complete();
                Ok(resolution)
            }
            Err(err) => {
                // Add version information to improve unsat error messages.
                Err(if let ResolveError::NoSolution(err) = err {
                    ResolveError::NoSolution(
                        err.with_available_versions(
                            &self.python_requirement,
                            &self.visited,
                            &self.index.packages,
                        )
                        .with_selector(self.selector.clone())
                        .with_python_requirement(&self.python_requirement)
                        .with_index_locations(self.provider.index_locations())
                        .with_unavailable_packages(&self.unavailable_packages)
                        .with_incomplete_packages(&self.incomplete_packages),
                    )
                } else {
                    err
                })
            }
        }
    }

    /// Run the `PubGrub` solver.
    #[instrument(skip_all)]
    async fn solve(
        &self,
        request_sink: tokio::sync::mpsc::Sender<Request>,
    ) -> Result<ResolutionGraph, ResolveError> {
        let root = PubGrubPackage::Root(self.project.clone());
        let mut prefetcher = BatchPrefetcher::default();

        // Keep track of the packages for which we've requested metadata.
        let mut pins = FilePins::default();
        let mut priorities = PubGrubPriorities::default();

        // Start the solve.
        let mut state = State::init(root.clone(), MIN_VERSION.clone());
        let mut added_dependencies: FxHashMap<PubGrubPackage, FxHashSet<Version>> =
            FxHashMap::default();
        let mut next = root;

        debug!(
            "Solving with target Python version {}",
            self.python_requirement.target()
        );

        loop {
            // Run unit propagation.
            state.unit_propagation(next)?;

            // Pre-visit all candidate packages, to allow metadata to be fetched in parallel. If
            // the dependency mode is direct, we only need to visit the root package.
            if self.dependency_mode.is_transitive() {
                Self::pre_visit(state.partial_solution.prioritized_packages(), &request_sink)
                    .await?;
            }

            // Choose a package version.
            let Some(highest_priority_pkg) = state
                .partial_solution
                .pick_highest_priority_pkg(|package, _range| priorities.get(package))
            else {
                if enabled!(Level::DEBUG) {
                    prefetcher.log_tried_versions();
                }
                let selection = state.partial_solution.extract_solution();
                return ResolutionGraph::from_state(
                    &selection,
                    &pins,
                    &self.index.packages,
                    &self.index.distributions,
                    &state,
                    &self.preferences,
                    self.editables.clone(),
                );
            };
            next = highest_priority_pkg;

            prefetcher.version_tried(next.clone());

            let term_intersection = state
                .partial_solution
                .term_intersection_for_package(&next)
                .ok_or_else(|| {
                    PubGrubError::Failure("a package was chosen but we don't have a term.".into())
                })?;
            let decision = self
                .choose_version(
                    &next,
                    term_intersection.unwrap_positive(),
                    &mut pins,
                    &request_sink,
                )
                .await?;

            // Pick the next compatible version.
            let version = match decision {
                None => {
                    debug!("No compatible version found for: {next}");

                    let term_intersection = state
                        .partial_solution
                        .term_intersection_for_package(&next)
                        .expect("a package was chosen but we don't have a term.");

                    let reason = {
                        if let PubGrubPackage::Package(ref package_name, _, _) = next {
                            // Check if the decision was due to the package being unavailable
                            self.unavailable_packages
                                .get(package_name)
                                .map(|entry| match *entry {
                                    UnavailablePackage::NoIndex => {
                                        "was not found in the provided package locations"
                                    }
                                    UnavailablePackage::Offline => "was not found in the cache",
                                    UnavailablePackage::NotFound => {
                                        "was not found in the package registry"
                                    }
                                    UnavailablePackage::InvalidMetadata(_) => {
                                        "was found, but the metadata could not be parsed"
                                    }
                                    UnavailablePackage::InvalidStructure(_) => {
                                        "was found, but has an invalid format"
                                    }
                                })
                        } else {
                            None
                        }
                    };

                    let inc = Incompatibility::no_versions(
                        next.clone(),
                        term_intersection.clone(),
                        reason.map(ToString::to_string),
                    );

                    state.add_incompatibility(inc);
                    continue;
                }
                Some(version) => version,
            };
            let version = match version {
                ResolverVersion::Available(version) => version,
                ResolverVersion::Unavailable(version, unavailable) => {
                    let reason = match unavailable {
                        // Incompatible requires-python versions are special in that we track
                        // them as incompatible dependencies instead of marking the package version
                        // as unavailable directly
                        UnavailableVersion::IncompatibleDist(
                            IncompatibleDist::Source(IncompatibleSource::RequiresPython(
                                requires_python,
                            ))
                            | IncompatibleDist::Wheel(IncompatibleWheel::RequiresPython(
                                requires_python,
                            )),
                        ) => {
                            let python_version = requires_python
                                .iter()
                                .map(PubGrubSpecifier::try_from)
                                .fold_ok(Range::full(), |range, specifier| {
                                    range.intersection(&specifier.into())
                                })?;

                            let package = &next;
                            for kind in [PubGrubPython::Installed, PubGrubPython::Target] {
                                state.add_incompatibility(Incompatibility::from_dependency(
                                    package.clone(),
                                    Range::singleton(version.clone()),
                                    (PubGrubPackage::Python(kind), python_version.clone()),
                                ));
                            }
                            state.partial_solution.add_decision(next.clone(), version);
                            continue;
                        }
                        UnavailableVersion::IncompatibleDist(incompatibility) => {
                            incompatibility.to_string()
                        }
                    };
                    state.add_incompatibility(Incompatibility::unavailable(
                        next.clone(),
                        version.clone(),
                        reason,
                    ));
                    continue;
                }
            };

            prefetcher
                .prefetch_batches(
                    &next,
                    &version,
                    term_intersection.unwrap_positive(),
                    &request_sink,
                    self.index,
                    &self.selector,
                )
                .await?;

            self.on_progress(&next, &version);

            if added_dependencies
                .entry(next.clone())
                .or_default()
                .insert(version.clone())
            {
                // Retrieve that package dependencies.
                let package = &next;
                let dependencies = match self
                    .get_dependencies(package, &version, &mut priorities, &request_sink)
                    .await?
                {
                    Dependencies::Unavailable(reason) => {
                        state.add_incompatibility(Incompatibility::unavailable(
                            package.clone(),
                            version.clone(),
                            reason.clone(),
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
                let dep_incompats = state.add_incompatibility_from_dependencies(
                    package.clone(),
                    version.clone(),
                    dependencies,
                );

                state.partial_solution.add_version(
                    package.clone(),
                    version,
                    dep_incompats,
                    &state.incompatibility_store,
                );
            } else {
                // `dep_incompats` are already in `incompatibilities` so we know there are not satisfied
                // terms and can add the decision directly.
                state.partial_solution.add_decision(next.clone(), version);
            }
        }
    }

    /// Visit a [`PubGrubPackage`] prior to selection. This should be called on a [`PubGrubPackage`]
    /// before it is selected, to allow metadata to be fetched in parallel.
    async fn visit_package(
        &self,
        package: &PubGrubPackage,
        request_sink: &tokio::sync::mpsc::Sender<Request>,
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
                if self.index.packages.register(name.clone()) {
                    request_sink.send(Request::Package(name.clone())).await?;
                }
            }
            PubGrubPackage::Package(name, _extra, Some(url)) => {
                // Verify that the package is allowed under the hash-checking policy.
                if !self.hasher.allows_url(url) {
                    return Err(ResolveError::UnhashedPackage(name.clone()));
                }

                // Emit a request to fetch the metadata for this distribution.
                let dist = Dist::from_url(name.clone(), url.clone())?;
                if self.index.distributions.register(dist.version_id()) {
                    request_sink.send(Request::Dist(dist)).await?;
                }
            }
        }
        Ok(())
    }

    /// Visit the set of [`PubGrubPackage`] candidates prior to selection. This allows us to fetch
    /// metadata for all of the packages in parallel.
    async fn pre_visit<'data>(
        packages: impl Iterator<Item = (&'data PubGrubPackage, &'data Range<Version>)>,
        request_sink: &tokio::sync::mpsc::Sender<Request>,
    ) -> Result<(), ResolveError> {
        // Iterate over the potential packages, and fetch file metadata for any of them. These
        // represent our current best guesses for the versions that we _might_ select.
        for (package, range) in packages {
            let PubGrubPackage::Package(package_name, None, None) = package else {
                continue;
            };
            request_sink
                .send(Request::Prefetch(package_name.clone(), range.clone()))
                .await?;
        }
        Ok(())
    }

    /// Given a set of candidate packages, choose the next package (and version) to add to the
    /// partial solution.
    ///
    /// Returns [None] when there are no versions in the given range.
    #[instrument(skip_all, fields(%package))]
    async fn choose_version(
        &self,
        package: &'a PubGrubPackage,
        range: &Range<Version>,
        pins: &mut FilePins,
        request_sink: &tokio::sync::mpsc::Sender<Request>,
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
                debug!("Searching for a compatible version of {package} @ {url} ({range})");

                // If the dist is an editable, return the version from the editable metadata.
                if let Some((_local, metadata, _)) = self.editables.get(package_name) {
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

                    return Ok(Some(ResolverVersion::Available(version.clone())));
                }

                let dist = PubGrubDistribution::from_url(package_name, url);
                let response = self
                    .index
                    .distributions
                    .wait(&dist.version_id())
                    .await
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
                    .packages
                    .wait(package_name)
                    .instrument(info_span!("package_wait", %package_name))
                    .await
                    .ok_or(ResolveError::Unregistered)?;
                self.visited.insert(package_name.clone());

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
                    self.installed_packages,
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
                    ResolvedDistRef::Installable(dist) => {
                        dist.filename().unwrap_or(Cow::Borrowed("unknown filename"))
                    }
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
                    if self.index.distributions.register(candidate.version_id()) {
                        let request = match dist.for_resolution() {
                            ResolvedDistRef::Installable(dist) => Request::Dist(dist.clone()),
                            ResolvedDistRef::Installed(dist) => Request::Installed(dist.clone()),
                        };
                        request_sink.send(request).await?;
                    }
                }

                Ok(Some(ResolverVersion::Available(version)))
            }
        }
    }

    /// Given a candidate package and version, return its dependencies.
    #[instrument(skip_all, fields(%package, %version))]
    async fn get_dependencies(
        &self,
        package: &PubGrubPackage,
        version: &Version,
        priorities: &mut PubGrubPriorities,
        request_sink: &tokio::sync::mpsc::Sender<Request>,
    ) -> Result<Dependencies, ResolveError> {
        match package {
            PubGrubPackage::Root(_) => {
                // Add the root requirements.
                let constraints = PubGrubDependencies::from_requirements(
                    &self.requirements,
                    &self.constraints,
                    &self.overrides,
                    None,
                    None,
                    &self.urls,
                    &self.locals,
                    self.markers,
                );

                let mut constraints = match constraints {
                    Ok(constraints) => constraints,
                    Err(err) => {
                        return Ok(Dependencies::Unavailable(uncapitalize(err.to_string())));
                    }
                };

                for (package, version) in constraints.iter() {
                    debug!("Adding direct dependency: {package}{version}");

                    // Update the package priorities.
                    priorities.insert(package, version);

                    // Emit a request to fetch the metadata for this package.
                    self.visit_package(package, request_sink).await?;
                }

                // Add a dependency on each editable.
                for (editable, metadata, _) in self.editables.iter() {
                    let package =
                        PubGrubPackage::from_package(metadata.name.clone(), None, &self.urls);
                    let version = Range::singleton(metadata.version.clone());

                    // Update the package priorities.
                    priorities.insert(&package, &version);

                    // Add the editable as a direct dependency.
                    constraints.push(package, version);

                    // Add a dependency on each extra.
                    for extra in &editable.extras {
                        constraints.push(
                            PubGrubPackage::from_package(
                                metadata.name.clone(),
                                Some(extra.clone()),
                                &self.urls,
                            ),
                            Range::singleton(metadata.version.clone()),
                        );
                    }
                }

                Ok(Dependencies::Available(constraints.into()))
            }

            PubGrubPackage::Python(_) => Ok(Dependencies::Available(Vec::default())),

            PubGrubPackage::Package(package_name, extra, url) => {
                // If we're excluding transitive dependencies, short-circuit.
                if self.dependency_mode.is_direct() {
                    // If an extra is provided, wait for the metadata to be available, since it's
                    // still required for reporting diagnostics.
                    if extra.is_some() && self.editables.get(package_name).is_none() {
                        // Determine the distribution to lookup.
                        let dist = match url {
                            Some(url) => PubGrubDistribution::from_url(package_name, url),
                            None => PubGrubDistribution::from_registry(package_name, version),
                        };
                        let version_id = dist.version_id();

                        // Wait for the metadata to be available.
                        self.index
                            .distributions
                            .wait(&version_id)
                            .instrument(info_span!("distributions_wait", %version_id))
                            .await
                            .ok_or(ResolveError::Unregistered)?;
                    }

                    return Ok(Dependencies::Available(Vec::default()));
                }

                // Determine if the distribution is editable.
                if let Some((_local, metadata, _)) = self.editables.get(package_name) {
                    let requirements: Vec<_> = metadata
                        .requires_dist
                        .iter()
                        .cloned()
                        .map(UvRequirement::from_requirement)
                        .collect::<Result<_, _>>()
                        .map_err(Box::new)?;
                    let constraints = PubGrubDependencies::from_requirements(
                        &requirements,
                        &self.constraints,
                        &self.overrides,
                        Some(package_name),
                        extra.as_ref(),
                        &self.urls,
                        &self.locals,
                        self.markers,
                    )?;

                    for (dep_package, dep_version) in constraints.iter() {
                        debug!("Adding transitive dependency for {package}=={version}: {dep_package}{dep_version}");

                        // Update the package priorities.
                        priorities.insert(dep_package, dep_version);

                        // Emit a request to fetch the metadata for this package.
                        self.visit_package(dep_package, request_sink).await?;
                    }

                    return Ok(Dependencies::Available(constraints.into()));
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
                    return Ok(Dependencies::Unavailable(
                        "The package is unavailable".to_string(),
                    ));
                }

                // Wait for the metadata to be available.
                let response = self
                    .index
                    .distributions
                    .wait(&version_id)
                    .instrument(info_span!("distributions_wait", %version_id))
                    .await
                    .ok_or(ResolveError::Unregistered)?;

                let metadata = match &*response {
                    MetadataResponse::Found(archive) => &archive.metadata,
                    MetadataResponse::Offline => {
                        self.incomplete_packages
                            .entry(package_name.clone())
                            .or_default()
                            .insert(version.clone(), IncompletePackage::Offline);
                        return Ok(Dependencies::Unavailable(
                            "network connectivity is disabled, but the metadata wasn't found in the cache"
                                .to_string(),
                        ));
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
                            "the package metadata could not be parsed".to_string(),
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
                            "the package metadata was inconsistent".to_string(),
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
                            "the package has an invalid format".to_string(),
                        ));
                    }
                };

                let requirements: Vec<_> = metadata
                    .requires_dist
                    .iter()
                    .cloned()
                    .map(UvRequirement::from_requirement)
                    .collect::<Result<_, _>>()
                    .map_err(Box::new)?;
                let constraints = PubGrubDependencies::from_requirements(
                    &requirements,
                    &self.constraints,
                    &self.overrides,
                    Some(package_name),
                    extra.as_ref(),
                    &self.urls,
                    &self.locals,
                    self.markers,
                )?;

                for (dep_package, dep_version) in constraints.iter() {
                    debug!("Adding transitive dependency for {package}=={version}: {dep_package}{dep_version}");

                    // Update the package priorities.
                    priorities.insert(dep_package, dep_version);

                    // Emit a request to fetch the metadata for this package.
                    self.visit_package(dep_package, request_sink).await?;
                }

                Ok(Dependencies::Available(constraints.into()))
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
    async fn fetch(
        &self,
        request_stream: tokio::sync::mpsc::Receiver<Request>,
    ) -> Result<(), ResolveError> {
        let mut response_stream = ReceiverStream::new(request_stream)
            .map(|request| self.process_request(request).boxed())
            .buffer_unordered(50);

        while let Some(response) = response_stream.next().await {
            match response? {
                Some(Response::Package(package_name, version_map)) => {
                    trace!("Received package metadata for: {package_name}");
                    self.index
                        .packages
                        .done(package_name, Arc::new(version_map));
                }
                Some(Response::Installed { dist, metadata }) => {
                    trace!("Received installed distribution metadata for: {dist}");
                    self.index.distributions.done(
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
                        .distributions
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
                        .distributions
                        .done(dist.version_id(), Arc::new(metadata));
                }
                None => {}
            }
        }

        Ok::<(), ResolveError>(())
    }

    #[instrument(skip_all, fields(%request))]
    async fn process_request(&self, request: Request) -> Result<Option<Response>, ResolveError> {
        match request {
            // Fetch package metadata from the registry.
            Request::Package(package_name) => {
                let package_versions = self
                    .provider
                    .get_package_versions(&package_name)
                    .boxed()
                    .await
                    .map_err(ResolveError::Client)?;

                Ok(Some(Response::Package(package_name, package_versions)))
            }

            // Fetch distribution metadata from the distribution database.
            Request::Dist(dist) => {
                let metadata = self
                    .provider
                    .get_or_build_wheel_metadata(&dist)
                    .boxed()
                    .await
                    .map_err(|err| match dist.clone() {
                        Dist::Built(BuiltDist::Path(built_dist)) => {
                            ResolveError::Read(Box::new(built_dist), err)
                        }
                        Dist::Source(SourceDist::Path(source_dist)) => {
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
                    .packages
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
                    self.installed_packages,
                    &self.exclusions,
                ) else {
                    return Ok(None);
                };

                // If there is not a compatible distribution, short-circuit.
                let Some(dist) = candidate.compatible() else {
                    return Ok(None);
                };

                // Emit a request to fetch the metadata for this version.
                if self.index.distributions.register(candidate.version_id()) {
                    let dist = dist.for_resolution().to_owned();

                    let response = match dist {
                        ResolvedDist::Installable(dist) => {
                            let metadata = self
                                .provider
                                .get_or_build_wheel_metadata(&dist)
                                .boxed()
                                .await
                                .map_err(|err| match dist.clone() {
                                    Dist::Built(BuiltDist::Path(built_dist)) => {
                                        ResolveError::Read(Box::new(built_dist), err)
                                    }
                                    Dist::Source(SourceDist::Path(source_dist)) => {
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
                    reporter.on_progress(package_name, &VersionOrUrl::Url(url));
                }
                PubGrubPackage::Package(package_name, _extra, None) => {
                    reporter.on_progress(package_name, &VersionOrUrl::Version(version));
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
    Unavailable(String),
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
