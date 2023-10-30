//! Given a set of requirements, find a set of compatible packages.

use std::collections::hash_map::Entry;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use futures::channel::mpsc::UnboundedReceiver;
use futures::{pin_mut, FutureExt, StreamExt, TryFutureExt};
use fxhash::{FxHashMap, FxHashSet};
use pubgrub::error::PubGrubError;
use pubgrub::range::Range;
use pubgrub::solver::{Incompatibility, State};
use pubgrub::type_aliases::DependencyConstraints;
use tokio::select;
use tracing::{debug, trace};
use waitmap::WaitMap;

use distribution_filename::{SourceDistributionFilename, WheelFilename};
use pep508_rs::{MarkerEnvironment, Requirement};
use platform_tags::Tags;
use puffin_client::{File, RegistryClient, SimpleJson};
use puffin_package::dist_info_name::DistInfoName;
use puffin_package::metadata::Metadata21;
use puffin_package::package_name::PackageName;
use puffin_traits::BuildContext;

use crate::candidate_selector::CandidateSelector;
use crate::distribution::{DistributionFile, SdistFile, WheelFile};
use crate::error::ResolveError;
use crate::manifest::Manifest;
use crate::pubgrub::{iter_requirements, version_range};
use crate::pubgrub::{PubGrubPackage, PubGrubPriorities, PubGrubVersion, MIN_VERSION};
use crate::resolution::Graph;
use crate::source_distribution::{download_and_build_sdist, read_dist_info};
use crate::BuiltSourceDistributionCache;

pub struct Resolver<'a, Context: BuildContext + Sync> {
    requirements: Vec<Requirement>,
    constraints: Vec<Requirement>,
    markers: &'a MarkerEnvironment,
    tags: &'a Tags,
    client: &'a RegistryClient,
    selector: CandidateSelector,
    index: Arc<Index>,
    build_context: &'a Context,
    reporter: Option<Box<dyn Reporter>>,
}

impl<'a, Context: BuildContext + Sync> Resolver<'a, Context> {
    /// Initialize a new resolver.
    pub fn new(
        manifest: Manifest,
        markers: &'a MarkerEnvironment,
        tags: &'a Tags,
        client: &'a RegistryClient,
        build_context: &'a Context,
    ) -> Self {
        Self {
            selector: CandidateSelector::from(&manifest),
            index: Arc::new(Index::default()),
            requirements: manifest.requirements,
            constraints: manifest.constraints,
            markers,
            tags,
            client,
            build_context,
            reporter: None,
        }
    }

    /// Set the [`Reporter`] to use for this installer.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        Self {
            reporter: Some(Box::new(reporter)),
            ..self
        }
    }

    /// Resolve a set of requirements into a set of pinned versions.
    pub async fn resolve(self) -> Result<Graph, ResolveError> {
        // A channel to fetch package metadata (e.g., given `flask`, fetch all versions) and version
        // metadata (e.g., given `flask==1.0.0`, fetch the metadata for that version).
        let (request_sink, request_stream) = futures::channel::mpsc::unbounded();

        // Run the fetcher.
        let requests_fut = self.fetch(request_stream);

        // Run the solver.
        let resolve_fut = self.solve(&request_sink);

        let requests_fut = requests_fut.fuse();
        let resolve_fut = resolve_fut.fuse();
        pin_mut!(requests_fut, resolve_fut);

        let resolution = select! {
            result = requests_fut => {
                result?;
                return Err(ResolveError::StreamTermination);
            }
            resolution = resolve_fut => {
                resolution?
            }
        };

        self.on_complete();

        Ok(resolution)
    }

    /// Run the `PubGrub` solver.
    async fn solve(
        &self,
        request_sink: &futures::channel::mpsc::UnboundedSender<Request>,
    ) -> Result<Graph, ResolveError> {
        let root = PubGrubPackage::Root;

        // Keep track of the packages for which we've requested metadata.
        let mut requested_packages = FxHashSet::default();
        let mut requested_versions = FxHashSet::default();
        let mut pins = FxHashMap::default();
        let mut priorities = PubGrubPriorities::default();

        // Push all the requirements into the package sink.
        for requirement in &self.requirements {
            debug!("Adding root dependency: {}", requirement);
            let package_name = PackageName::normalize(&requirement.name);
            if requested_packages.insert(package_name.clone()) {
                priorities.add(package_name.clone());
                request_sink.unbounded_send(Request::Package(package_name))?;
            }
        }

        // Start the solve.
        let mut state = State::init(root.clone(), MIN_VERSION.clone());
        let mut added_dependencies: FxHashMap<PubGrubPackage, FxHashSet<PubGrubVersion>> =
            FxHashMap::default();
        let mut next = root;

        loop {
            // Run unit propagation.
            state.unit_propagation(next)?;

            // Pre-visit all candidate packages, to allow metadata to be fetched in parallel.
            self.pre_visit(
                state.partial_solution.prioritized_packages(),
                &mut requested_versions,
                request_sink,
            )?;

            // Choose a package version.
            let Some(highest_priority_pkg) =
                state
                    .partial_solution
                    .pick_highest_priority_pkg(|package, _range| {
                        priorities.get(package).unwrap_or_default()
                    })
            else {
                let selection = state.partial_solution.extract_solution();
                return Ok(Graph::from_state(&selection, &pins, &state));
            };
            next = highest_priority_pkg;

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
                    &mut requested_versions,
                    request_sink,
                )
                .await?;

            // Pick the next compatible version.
            let version = match decision {
                None => {
                    debug!("No compatible version found for: {}", next);

                    let term_intersection = state
                        .partial_solution
                        .term_intersection_for_package(&next)
                        .expect("a package was chosen but we don't have a term.");

                    let inc = Incompatibility::no_versions(next.clone(), term_intersection.clone());
                    state.add_incompatibility(inc);
                    continue;
                }
                Some(version) => version,
            };

            self.on_progress(&next, &version);

            if added_dependencies
                .entry(next.clone())
                .or_default()
                .insert(version.clone())
            {
                // Retrieve that package dependencies.
                let package = &next;
                let dependencies = match self
                    .get_dependencies(
                        package,
                        &version,
                        &mut pins,
                        &mut priorities,
                        &mut requested_packages,
                        request_sink,
                    )
                    .await?
                {
                    Dependencies::Unknown => {
                        state.add_incompatibility(Incompatibility::unavailable_dependencies(
                            package.clone(),
                            version.clone(),
                        ));
                        continue;
                    }
                    Dependencies::Known(constraints) if constraints.contains_key(package) => {
                        return Err(PubGrubError::SelfDependency {
                            package: package.clone(),
                            version: version.clone(),
                        }
                        .into());
                    }
                    Dependencies::Known(constraints) => constraints,
                };

                // Add that package and version if the dependencies are not problematic.
                let dep_incompats = state.add_incompatibility_from_dependencies(
                    package.clone(),
                    version.clone(),
                    &dependencies,
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

    /// Visit the set of candidate packages prior to selection. This allows us to fetch metadata for
    /// all of the packages in parallel.
    fn pre_visit(
        &self,
        packages: impl Iterator<Item = (&'a PubGrubPackage, &'a Range<PubGrubVersion>)>,
        in_flight: &mut FxHashSet<String>,
        request_sink: &futures::channel::mpsc::UnboundedSender<Request>,
    ) -> Result<(), ResolveError> {
        // Iterate over the potential packages, and fetch file metadata for any of them. These
        // represent our current best guesses for the versions that we _might_ select.
        for (package, range) in packages {
            let PubGrubPackage::Package(package_name, _) = package else {
                continue;
            };

            // If we don't have metadata for this package, we can't make an early decision.
            let Some(entry) = self.index.packages.get(package_name) else {
                continue;
            };
            let version_map = entry.value();

            // Try to find a compatible version. If there aren't any compatible versions,
            // short-circuit and return `None`.
            let Some(candidate) = self.selector.select(package_name, range, version_map) else {
                // Short-circuit: we couldn't find _any_ compatible versions for a package.
                return Ok(());
            };

            // Emit a request to fetch the metadata for this version.
            match candidate.file {
                DistributionFile::Wheel(file) => {
                    if in_flight.insert(file.hashes.sha256.clone()) {
                        request_sink.unbounded_send(Request::Wheel(file.clone()))?;
                    }
                }
                DistributionFile::Sdist(file) => {
                    if in_flight.insert(file.hashes.sha256.clone()) {
                        request_sink.unbounded_send(Request::Sdist(
                            file.clone(),
                            candidate.package_name.clone(),
                            candidate.version.clone().into(),
                        ))?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Given a set of candidate packages, choose the next package (and version) to add to the
    /// partial solution.
    async fn choose_version(
        &self,
        package: &PubGrubPackage,
        range: &Range<PubGrubVersion>,
        pins: &mut FxHashMap<PackageName, FxHashMap<pep440_rs::Version, File>>,
        in_flight: &mut FxHashSet<String>,
        request_sink: &futures::channel::mpsc::UnboundedSender<Request>,
    ) -> Result<Option<PubGrubVersion>, ResolveError> {
        return match package {
            PubGrubPackage::Root => Ok(Some(MIN_VERSION.clone())),
            PubGrubPackage::Package(package_name, _) => {
                // Wait for the metadata to be available.
                let entry = self.index.packages.wait(package_name).await.unwrap();
                let version_map = entry.value();

                debug!(
                    "Searching for a compatible version of {} ({})",
                    package_name, range,
                );

                // Find a compatible version.
                let Some(candidate) = self.selector.select(package_name, range, version_map) else {
                    // Short circuit: we couldn't find _any_ compatible versions for a package.
                    return Ok(None);
                };

                debug!(
                    "Selecting: {}=={} ({})",
                    candidate.package_name,
                    candidate.version,
                    candidate.file.filename()
                );

                // We want to return a package pinned to a specific version; but we _also_ want to
                // store the exact file that we selected to satisfy that version.
                pins.entry(candidate.package_name.clone())
                    .or_default()
                    .insert(
                        candidate.version.clone().into(),
                        candidate.file.clone().into(),
                    );

                // Emit a request to fetch the metadata for this version.
                match candidate.file {
                    DistributionFile::Wheel(file) => {
                        if in_flight.insert(file.hashes.sha256.clone()) {
                            request_sink.unbounded_send(Request::Wheel(file.clone()))?;
                        }
                    }
                    DistributionFile::Sdist(file) => {
                        if in_flight.insert(file.hashes.sha256.clone()) {
                            request_sink.unbounded_send(Request::Sdist(
                                file.clone(),
                                candidate.package_name.clone(),
                                candidate.version.clone().into(),
                            ))?;
                        }
                    }
                }

                let version = candidate.version.clone();
                Ok(Some(version))
            }
        };
    }

    /// Given a candidate package and version, return its dependencies.
    async fn get_dependencies(
        &self,
        package: &PubGrubPackage,
        version: &PubGrubVersion,
        pins: &mut FxHashMap<PackageName, FxHashMap<pep440_rs::Version, File>>,
        priorities: &mut PubGrubPriorities,
        requested_packages: &mut FxHashSet<PackageName>,
        request_sink: &futures::channel::mpsc::UnboundedSender<Request>,
    ) -> Result<Dependencies, ResolveError> {
        match package {
            PubGrubPackage::Root => {
                let mut constraints =
                    DependencyConstraints::<PubGrubPackage, Range<PubGrubVersion>>::default();

                // Add the root requirements.
                for (package, version) in
                    iter_requirements(self.requirements.iter(), None, None, self.markers)
                {
                    // Emit a request to fetch the metadata for this package.
                    if let PubGrubPackage::Package(package_name, None) = &package {
                        if requested_packages.insert(package_name.clone()) {
                            priorities.add(package_name.clone());
                            request_sink.unbounded_send(Request::Package(package_name.clone()))?;
                        }
                    }

                    // Add it to the constraints.
                    match constraints.entry(package) {
                        Entry::Occupied(mut entry) => {
                            entry.insert(entry.get().intersection(&version));
                        }
                        Entry::Vacant(entry) => {
                            entry.insert(version);
                        }
                    }
                }

                // If any requirements were further constrained by the user, add those constraints.
                for constraint in &self.constraints {
                    let package =
                        PubGrubPackage::Package(PackageName::normalize(&constraint.name), None);
                    if let Some(range) = constraints.get_mut(&package) {
                        *range = range.intersection(
                            &version_range(constraint.version_or_url.as_ref()).unwrap(),
                        );
                    }
                }

                Ok(Dependencies::Known(constraints))
            }
            PubGrubPackage::Package(package_name, extra) => {
                if let Some(extra) = extra.as_ref() {
                    debug!(
                        "Fetching dependencies for {}[{:?}]@{}",
                        package_name, extra, version
                    );
                } else {
                    debug!("Fetching dependencies for {}@{}", package_name, version);
                }

                // Wait for the metadata to be available.
                let versions = pins.get(package_name).unwrap();
                let file = versions.get(version.into()).unwrap();
                let entry = self.index.versions.wait(&file.hashes.sha256).await.unwrap();
                let metadata = entry.value();

                let mut constraints =
                    DependencyConstraints::<PubGrubPackage, Range<PubGrubVersion>>::default();

                for (package, version) in iter_requirements(
                    metadata.requires_dist.iter(),
                    extra.as_ref(),
                    Some(package_name),
                    self.markers,
                ) {
                    debug!("Adding transitive dependency: {package} {version}");

                    // Emit a request to fetch the metadata for this package.
                    if let PubGrubPackage::Package(package_name, None) = &package {
                        if requested_packages.insert(package_name.clone()) {
                            priorities.add(package_name.clone());
                            request_sink.unbounded_send(Request::Package(package_name.clone()))?;
                        }
                    }

                    // Add it to the constraints.
                    match constraints.entry(package) {
                        Entry::Occupied(mut entry) => {
                            entry.insert(entry.get().intersection(&version));
                        }
                        Entry::Vacant(entry) => {
                            entry.insert(version);
                        }
                    }
                }

                // If any packages were further constrained by the user, add those constraints.
                for constraint in &self.constraints {
                    let package =
                        PubGrubPackage::Package(PackageName::normalize(&constraint.name), None);
                    if let Some(range) = constraints.get_mut(&package) {
                        *range = range.intersection(
                            &version_range(constraint.version_or_url.as_ref()).unwrap(),
                        );
                    }
                }

                if let Some(extra) = extra {
                    if !metadata
                        .provides_extras
                        .iter()
                        .any(|provided_extra| DistInfoName::normalize(provided_extra) == *extra)
                    {
                        return Ok(Dependencies::Unknown);
                    }
                    constraints.insert(
                        PubGrubPackage::Package(package_name.clone(), None),
                        Range::singleton(version.clone()),
                    );
                }

                Ok(Dependencies::Known(constraints))
            }
        }
    }

    /// Fetch the metadata for a stream of packages and versions.
    async fn fetch(&self, request_stream: UnboundedReceiver<Request>) -> Result<(), ResolveError> {
        let mut response_stream = request_stream
            .map(|request| self.process_request(request))
            .buffer_unordered(50);

        while let Some(response) = response_stream.next().await {
            match response? {
                Response::Package(package_name, metadata) => {
                    trace!("Received package metadata for {}", package_name);

                    // Group the distributions by version and kind, discarding any incompatible
                    // distributions.
                    let mut version_map: VersionMap = BTreeMap::new();
                    for file in metadata.files {
                        if let Ok(name) = WheelFilename::from_str(file.filename.as_str()) {
                            if name.is_compatible(self.tags) {
                                let version = PubGrubVersion::from(name.version);
                                match version_map.entry(version) {
                                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                                        if matches!(entry.get(), DistributionFile::Sdist(_)) {
                                            // Wheels get precedence over source distributions.
                                            entry.insert(DistributionFile::from(WheelFile::from(
                                                file,
                                            )));
                                        }
                                    }
                                    std::collections::btree_map::Entry::Vacant(entry) => {
                                        entry.insert(DistributionFile::from(WheelFile::from(file)));
                                    }
                                }
                            }
                        } else if let Ok(name) =
                            SourceDistributionFilename::parse(file.filename.as_str(), &package_name)
                        {
                            let version = PubGrubVersion::from(name.version);
                            if let std::collections::btree_map::Entry::Vacant(entry) =
                                version_map.entry(version)
                            {
                                entry.insert(DistributionFile::from(SdistFile::from(file)));
                            }
                        }
                    }

                    self.index
                        .packages
                        .insert(package_name.clone(), version_map);
                }
                Response::Wheel(file, metadata) => {
                    trace!("Received file metadata for {}", file.filename);
                    self.index
                        .versions
                        .insert(file.hashes.sha256.clone(), metadata);
                }
                Response::Sdist(file, metadata) => {
                    trace!("Received sdist build metadata for {}", file.filename);
                    self.index
                        .versions
                        .insert(file.hashes.sha256.clone(), metadata);
                }
            }
        }

        Ok::<(), ResolveError>(())
    }

    fn process_request(
        &'a self,
        request: Request,
    ) -> Pin<Box<dyn Future<Output = Result<Response, ResolveError>> + Send + 'a>> {
        match request {
            Request::Package(package_name) => Box::pin(
                self.client
                    .simple(package_name.clone())
                    .map_ok(move |metadata| Response::Package(package_name, metadata))
                    .map_err(ResolveError::Client),
            ),
            Request::Wheel(file) => Box::pin(
                self.client
                    .file(file.clone().into())
                    .map_ok(move |metadata| Response::Wheel(file, metadata))
                    .map_err(ResolveError::Client),
            ),
            Request::Sdist(file, package_name, version) => Box::pin(async move {
                let cached_wheel = self.build_context.cache().and_then(|cache| {
                    BuiltSourceDistributionCache::new(cache).find_wheel(
                        &package_name,
                        &version,
                        self.tags,
                    )
                });
                let metadata21 = if let Some(cached_wheel) = cached_wheel {
                    read_dist_info(cached_wheel).await
                } else {
                    download_and_build_sdist(
                        &file,
                        &package_name,
                        &version,
                        self.client,
                        self.build_context,
                    )
                    .await
                }
                .map_err(|err| ResolveError::SourceDistribution {
                    filename: file.filename.clone(),
                    err,
                })?;

                Ok(Response::Sdist(file, metadata21))
            }),
        }
    }

    fn on_progress(&self, package: &PubGrubPackage, version: &PubGrubVersion) {
        if let Some(reporter) = self.reporter.as_ref() {
            if let PubGrubPackage::Package(package_name, _) = package {
                reporter.on_progress(package_name, version.into());
            }
        }
    }

    fn on_complete(&self) {
        if let Some(reporter) = self.reporter.as_ref() {
            reporter.on_complete();
        }
    }
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a dependency is resolved.
    fn on_progress(&self, name: &PackageName, version: &pep440_rs::Version);

    /// Callback to invoke when the resolution is complete.
    fn on_complete(&self);
}

/// Fetch the metadata for an item
#[derive(Debug)]
enum Request {
    /// A request to fetch the metadata for a package.
    Package(PackageName),
    /// A request to fetch and build the source distribution for a specific package version
    Sdist(SdistFile, PackageName, pep440_rs::Version),
    /// A request to fetch the metadata for a specific version of a package.
    Wheel(WheelFile),
}

#[derive(Debug)]
enum Response {
    /// The returned metadata for a package.
    Package(PackageName, SimpleJson),
    /// The returned metadata for a specific version of a package.
    Wheel(WheelFile, Metadata21),
    /// The returned metadata for an sdist build.
    Sdist(SdistFile, Metadata21),
}

pub(crate) type VersionMap = BTreeMap<PubGrubVersion, DistributionFile>;

/// In-memory index of package metadata.
struct Index {
    /// A map from package name to the metadata for that package.
    packages: WaitMap<PackageName, VersionMap>,

    /// A map from wheel SHA to the metadata for that wheel.
    versions: WaitMap<String, Metadata21>,
}

impl Default for Index {
    fn default() -> Self {
        Self {
            packages: WaitMap::new(),
            versions: WaitMap::new(),
        }
    }
}

/// An enum used by [`DependencyProvider`] that holds information about package dependencies.
/// For each [Package] there is a set of versions allowed as a dependency.
#[derive(Clone)]
enum Dependencies {
    /// Package dependencies are unavailable.
    Unknown,
    /// Container for all available package versions.
    Known(DependencyConstraints<PubGrubPackage, Range<PubGrubVersion>>),
}
