//! Given a set of requirements, find a set of compatible packages.

use std::borrow::Borrow;
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

use crate::distribution::{DistributionFile, SdistFile, WheelFile};
use crate::error::ResolveError;
use crate::pubgrub::package::PubGrubPackage;
use crate::pubgrub::version::{PubGrubVersion, MIN_VERSION};
use crate::pubgrub::{iter_requirements, version_range};
use crate::resolution::{Graph, Resolution};
use crate::selector::{CandidateSelector, ResolutionMode};
use crate::source_distribution::{download_and_build_sdist, read_dist_info};
use crate::BuiltSourceDistributionCache;

pub struct Resolver<'a, Context: BuildContext> {
    requirements: Vec<Requirement>,
    constraints: Vec<Requirement>,
    resolution: Option<Resolution>,
    markers: &'a MarkerEnvironment,
    tags: &'a Tags,
    client: &'a RegistryClient,
    selector: CandidateSelector,
    index: Arc<Index>,
    build_context: &'a Context,
}

impl<'a, Context: BuildContext> Resolver<'a, Context> {
    /// Initialize a new resolver.
    pub fn new(
        requirements: Vec<Requirement>,
        constraints: Vec<Requirement>,
        mode: ResolutionMode,
        markers: &'a MarkerEnvironment,
        tags: &'a Tags,
        client: &'a RegistryClient,
        build_context: &'a Context,
    ) -> Self {
        Self {
            selector: CandidateSelector::from_mode(mode, &requirements),
            index: Arc::new(Index::default()),
            resolution: None,
            requirements,
            constraints,
            markers,
            tags,
            client,
            build_context,
        }
    }

    #[must_use]
    pub fn with_resolution(mut self, resolution: Resolution) -> Self {
        self.resolution = Some(resolution);
        self
    }

    /// Resolve a set of requirements into a set of pinned versions.
    pub async fn resolve(self) -> Result<Graph, ResolveError> {
        // A channel to fetch package metadata (e.g., given `flask`, fetch all versions) and version
        // metadata (e.g., given `flask==1.0.0`, fetch the metadata for that version).
        let (request_sink, request_stream) = futures::channel::mpsc::unbounded();

        // Push all the requirements into the package sink.
        for requirement in &self.requirements {
            debug!("Adding root dependency: {}", requirement);
            let package_name = PackageName::normalize(&requirement.name);
            request_sink.unbounded_send(Request::Package(package_name))?;
        }

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

        // Start the solve.
        let mut state = State::init(root.clone(), MIN_VERSION.clone());
        let mut added_dependencies: FxHashMap<PubGrubPackage, FxHashSet<PubGrubVersion>> =
            FxHashMap::default();
        let mut next = root;

        loop {
            // Run unit propagation.
            state.unit_propagation(next)?;

            // Fetch the list of candidates.
            let Some(potential_packages) = state.partial_solution.potential_packages() else {
                let Some(selection) = state.partial_solution.extract_solution() else {
                    return Err(PubGrubError::Failure(
                        "How did we end up with no package to choose but no solution?".into(),
                    )
                    .into());
                };

                return Ok(Graph::from_state(&selection, &pins, &state));
            };

            // Choose a package version.
            let potential_packages = potential_packages.collect::<Vec<_>>();
            let decision = self
                .choose_package_version(
                    potential_packages,
                    &mut pins,
                    &mut requested_versions,
                    request_sink,
                )
                .await?;
            next = decision.0.clone();

            // Pick the next compatible version.
            let version = match decision.1 {
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
                    Dependencies::Known(constraints) => {
                        if constraints.contains_key(package) {
                            return Err(PubGrubError::SelfDependency {
                                package: package.clone(),
                                version: version.clone(),
                            }
                            .into());
                        }
                        if let Some((dependent, _)) = constraints
                            .iter()
                            .find(|(_, r)| r == &&Range::<PubGrubVersion>::empty())
                        {
                            return Err(PubGrubError::DependencyOnTheEmptySet {
                                package: package.clone(),
                                version: version.clone(),
                                dependent: dependent.clone(),
                            }
                            .into());
                        }
                        constraints
                    }
                };

                // Add that package and version if the dependencies are not problematic.
                let dep_incompats = state.add_incompatibility_from_dependencies(
                    package.clone(),
                    version.clone(),
                    &dependencies,
                );

                if state.incompatibility_store[dep_incompats.clone()]
                    .iter()
                    .any(|incompat| state.is_terminal(incompat))
                {
                    // For a dependency incompatibility to be terminal,
                    // it can only mean that root depend on not root?
                    return Err(PubGrubError::Failure(
                        "Root package depends on itself at a different version?".into(),
                    )
                    .into());
                }
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

    /// Given a set of candidate packages, choose the next package (and version) to add to the
    /// partial solution.
    async fn choose_package_version<T: Borrow<PubGrubPackage>, U: Borrow<Range<PubGrubVersion>>>(
        &self,
        mut potential_packages: Vec<(T, U)>,
        pins: &mut FxHashMap<PackageName, FxHashMap<pep440_rs::Version, File>>,
        in_flight: &mut FxHashSet<String>,
        request_sink: &futures::channel::mpsc::UnboundedSender<Request>,
    ) -> Result<(T, Option<PubGrubVersion>), ResolveError> {
        let mut selection = 0usize;

        // Iterate over the potential packages, and fetch file metadata for any of them. These
        // represent our current best guesses for the versions that we _might_ select.
        for (index, (package, range)) in potential_packages.iter().enumerate() {
            let PubGrubPackage::Package(package_name, _) = package.borrow() else {
                continue;
            };

            // If we don't have metadata for this package, we can't make an early decision.
            let Some(entry) = self.index.packages.get(package_name) else {
                continue;
            };
            let version_map = entry.value();

            // Try to find a compatible version. If there aren't any compatible versions,
            // short-circuit and return `None`.
            let Some(candidate) = self
                .selector
                .select(package_name, range.borrow(), version_map)
            else {
                // Short circuit: we couldn't find _any_ compatible versions for a package.
                let (package, _range) = potential_packages.swap_remove(index);
                return Ok((package, None));
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

            selection = index;
        }

        // TODO(charlie): This is really ugly, but we need to return `T`, not `&T` (and yet
        // we also need to iterate over `potential_packages` multiple times, so we can't
        // use `into_iter()`.)
        let (package, range) = potential_packages.swap_remove(selection);

        return match package.borrow() {
            PubGrubPackage::Root => Ok((package, Some(MIN_VERSION.clone()))),
            PubGrubPackage::Package(package_name, _) => {
                // Wait for the metadata to be available.
                let entry = self.index.packages.wait(package_name).await.unwrap();
                let version_map = entry.value();

                debug!(
                    "Searching for a compatible version of {} ({})",
                    package_name,
                    range.borrow(),
                );

                // Find a compatible version.
                let Some(candidate) =
                    self.selector
                        .select(package_name, range.borrow(), version_map)
                else {
                    // Short circuit: we couldn't find _any_ compatible versions for a package.
                    return Ok((package, None));
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
                Ok((package, Some(version)))
            }
        };
    }

    /// Given a candidate package and version, return its dependencies.
    async fn get_dependencies(
        &self,
        package: &PubGrubPackage,
        version: &PubGrubVersion,
        pins: &mut FxHashMap<PackageName, FxHashMap<pep440_rs::Version, File>>,
        requested_packages: &mut FxHashSet<PackageName>,
        request_sink: &futures::channel::mpsc::UnboundedSender<Request>,
    ) -> Result<Dependencies, ResolveError> {
        match package {
            PubGrubPackage::Root => {
                let mut constraints =
                    DependencyConstraints::<PubGrubPackage, Range<PubGrubVersion>>::default();

                // Add the root requirements.
                for (package, version) in
                    iter_requirements(self.requirements.iter(), None, self.markers)
                {
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
                    debug!("Fetching dependencies for {}[{:?}]", package_name, extra);
                } else {
                    debug!("Fetching dependencies for {}", package_name);
                }

                // Wait for the metadata to be available.
                let versions = pins.get(package_name).unwrap();
                let file = versions.get(version.into()).unwrap();
                let entry = self.index.versions.wait(&file.hashes.sha256).await.unwrap();
                let metadata = entry.value();

                let mut constraints =
                    DependencyConstraints::<PubGrubPackage, Range<PubGrubVersion>>::default();

                for (package, version) in
                    iter_requirements(metadata.requires_dist.iter(), extra.as_ref(), self.markers)
                {
                    debug!("Adding transitive dependency: {package} {version}");

                    // Emit a request to fetch the metadata for this package.
                    if let PubGrubPackage::Package(package_name, None) = &package {
                        if requested_packages.insert(package_name.clone()) {
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
            .buffer_unordered(32)
            .ready_chunks(32);

        while let Some(chunk) = response_stream.next().await {
            for response in chunk {
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
                                    if let std::collections::btree_map::Entry::Vacant(entry) =
                                        version_map.entry(version)
                                    {
                                        entry.insert(DistributionFile::from(WheelFile::from(file)));
                                    }
                                }
                            } else if let Ok(name) = SourceDistributionFilename::parse(
                                file.filename.as_str(),
                                &package_name,
                            ) {
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
        }

        Ok::<(), ResolveError>(())
    }

    fn process_request(
        &'a self,
        request: Request,
    ) -> Pin<Box<dyn Future<Output = Result<Response, ResolveError>> + 'a>> {
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
