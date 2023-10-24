//! Given a set of requirements, find a set of compatible packages.

use std::borrow::Borrow;
use std::collections::hash_map::Entry;
use std::future::Future;
use std::path::{Path, PathBuf};
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
use puffin_traits::PuffinCtx;

use crate::error::ResolveError;
use crate::mode::{CandidateSelector, ResolutionMode};
use crate::pubgrub::package::PubGrubPackage;
use crate::pubgrub::version::{PubGrubVersion, MIN_VERSION};
use crate::pubgrub::{iter_requirements, version_range};
use crate::resolution::Graph;
use crate::source_distribution::{download_and_build_sdist, read_dist_info};
use crate::BuiltSourceDistributionCache;

pub struct Resolver<'a, Ctx: PuffinCtx> {
    requirements: Vec<Requirement>,
    constraints: Vec<Requirement>,
    markers: &'a MarkerEnvironment,
    tags: &'a Tags,
    client: &'a RegistryClient,
    selector: CandidateSelector,
    cache: Arc<SolverCache>,
    puffin_ctx: &'a Ctx,
}

impl<'a, Ctx: PuffinCtx> Resolver<'a, Ctx> {
    /// Initialize a new resolver.
    pub fn new(
        requirements: Vec<Requirement>,
        constraints: Vec<Requirement>,
        mode: ResolutionMode,
        markers: &'a MarkerEnvironment,
        tags: &'a Tags,
        client: &'a RegistryClient,
        puffin_ctx: &'a Ctx,
    ) -> Self {
        Self {
            selector: CandidateSelector::from_mode(mode, &requirements),
            cache: Arc::new(SolverCache::default()),
            requirements,
            constraints,
            markers,
            tags,
            client,
            puffin_ctx,
        }
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
            let Some(entry) = self.cache.packages.get(package_name) else {
                continue;
            };
            let simple_json = entry.value();

            // Try to find a wheel. If there isn't any, to a find a source distribution. If there
            // isn't any either, short circuit and fail the resolution.
            // TODO: Group files by version, then check for each version first for compatible wheels
            // and then for a compatible sdist. This is required to still select the most recent
            // version.
            let Some((file, request)) = self
                .selector
                .iter_candidates(package_name, &simple_json.files)
                .find_map(|file| {
                    let wheel_filename = WheelFilename::from_str(file.filename.as_str()).ok()?;
                    if !wheel_filename.is_compatible(self.tags) {
                        return None;
                    }

                    if range
                        .borrow()
                        .contains(&PubGrubVersion::from(wheel_filename.version.clone()))
                    {
                        Some((file, Request::WheelVersion(file.clone())))
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    self.selector
                        .iter_candidates(package_name, &simple_json.files)
                        .find_map(|file| {
                            let sdist_filename =
                                SourceDistributionFilename::parse(&file.filename, package_name)
                                    .ok()?;

                            if range
                                .borrow()
                                .contains(&PubGrubVersion::from(sdist_filename.version.clone()))
                            {
                                Some((file, Request::SdistVersion((file.clone(), sdist_filename))))
                            } else {
                                None
                            }
                        })
                })
            else {
                // Short circuit: we couldn't find _any_ compatible versions for a package.
                let (package, _range) = potential_packages.swap_remove(index);
                return Ok((package, None));
            };

            // Emit a request to fetch the metadata for this version.
            if in_flight.insert(file.hashes.sha256.clone()) {
                request_sink.unbounded_send(request)?;
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
                let entry = self.cache.packages.wait(package_name).await.unwrap();
                let simple_json = entry.value();

                debug!(
                    "Searching for a compatible version of {} ({})",
                    package_name,
                    range.borrow(),
                );

                // Find a compatible version.
                let mut wheel = self
                    .selector
                    .iter_candidates(package_name, &simple_json.files)
                    .find_map(|file| {
                        let Ok(name) = WheelFilename::from_str(file.filename.as_str()) else {
                            return None;
                        };

                        if !name.is_compatible(self.tags) {
                            return None;
                        }

                        if !range
                            .borrow()
                            .contains(&PubGrubVersion::from(name.version.clone()))
                        {
                            return None;
                        };

                        Some(Wheel {
                            file: file.clone(),
                            name: package_name.clone(),
                            version: name.version.clone(),
                        })
                    });

                if wheel.is_none() {
                    if let Some((sdist_file, parsed_filename)) = simple_json
                        .files
                        .iter()
                        .rev()
                        .filter_map(|file| {
                            let Ok(parsed_filename) =
                                SourceDistributionFilename::parse(&file.filename, package_name)
                            else {
                                return None;
                            };

                            if !range
                                .borrow()
                                .contains(&PubGrubVersion::from(parsed_filename.version.clone()))
                            {
                                return None;
                            };

                            Some((file, parsed_filename))
                        })
                        .max_by(|left, right| left.1.version.cmp(&right.1.version))
                    {
                        // Emit a request to fetch the metadata for this version.
                        if in_flight.insert(sdist_file.hashes.sha256.clone()) {
                            request_sink.unbounded_send(Request::SdistVersion((
                                sdist_file.clone(),
                                parsed_filename.clone(),
                            )))?;
                        }
                        // TODO(konstin): That's not a wheel
                        wheel = Some(Wheel {
                            file: sdist_file.clone(),
                            name: package_name.clone(),
                            version: parsed_filename.version.clone(),
                        });
                    }
                }

                if let Some(wheel) = wheel {
                    debug!(
                        "Selecting: {}=={} ({})",
                        wheel.name, wheel.version, wheel.file.filename
                    );

                    // We want to return a package pinned to a specific version; but we _also_ want to
                    // store the exact file that we selected to satisfy that version.
                    pins.entry(wheel.name)
                        .or_default()
                        .insert(wheel.version.clone(), wheel.file.clone());

                    // Emit a request to fetch the metadata for this version.
                    if in_flight.insert(wheel.file.hashes.sha256.clone()) {
                        request_sink.unbounded_send(Request::WheelVersion(wheel.file.clone()))?;
                    }

                    Ok((package, Some(PubGrubVersion::from(wheel.version))))
                } else {
                    // Short circuit: we couldn't find _any_ compatible versions for a package.
                    Ok((package, None))
                }
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
                let entry = self.cache.versions.wait(&file.hashes.sha256).await.unwrap();
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
                        self.cache.packages.insert(package_name.clone(), metadata);
                    }
                    Response::Version(file, metadata) => {
                        trace!("Received file metadata for {}", file.filename);
                        self.cache
                            .versions
                            .insert(file.hashes.sha256.clone(), metadata);
                    }
                    Response::Sdist(file, metadata) => {
                        trace!("Received sdist build metadata for {}", file.filename);
                        self.cache
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
            Request::WheelVersion(file) => Box::pin(
                self.client
                    .file(file.clone())
                    .map_ok(move |metadata| Response::Version(file, metadata))
                    .map_err(ResolveError::Client),
            ),
            Request::SdistVersion((file, filename)) => Box::pin(async move {
                let cached_wheel = self.find_cached_built_wheel(self.puffin_ctx.cache(), &filename);
                let metadata21 = if let Some(cached_wheel) = cached_wheel {
                    read_dist_info(cached_wheel).await
                } else {
                    download_and_build_sdist(&file, self.client, self.puffin_ctx, &filename).await
                }
                .map_err(|err| ResolveError::SourceDistribution {
                    filename: file.filename.clone(),
                    err,
                })?;

                Ok(Response::Sdist(file, metadata21))
            }),
        }
    }

    fn find_cached_built_wheel(
        &self,
        cache: Option<&Path>,
        filename: &SourceDistributionFilename,
    ) -> Option<PathBuf> {
        let Some(cache) = cache else {
            return None;
        };
        let cache = BuiltSourceDistributionCache::new(cache);
        let Ok(read_dir) = fs_err::read_dir(cache.version(&filename.name, &filename.version))
        else {
            return None;
        };

        for entry in read_dir {
            let Ok(entry) = entry else { continue };
            let Ok(wheel) = WheelFilename::from_str(entry.file_name().to_string_lossy().as_ref())
            else {
                continue;
            };

            if wheel.is_compatible(self.tags) {
                return Some(entry.path().clone());
            }
        }
        None
    }
}

#[derive(Debug, Clone)]
struct Wheel {
    /// The underlying [`File`] for this wheel.
    file: File,
    /// The normalized name of the package.
    name: PackageName,
    /// The version of the package.
    version: pep440_rs::Version,
}

#[derive(Debug)]
enum Request {
    /// A request to fetch the metadata for a package.
    Package(PackageName),
    /// A request to fetch and build the source distribution for a specific package version
    SdistVersion((File, SourceDistributionFilename)),
    /// A request to fetch the metadata for a specific version of a package.
    WheelVersion(File),
}

#[derive(Debug)]
enum Response {
    /// The returned metadata for a package.
    Package(PackageName, SimpleJson),
    /// The returned metadata for a specific version of a package.
    Version(File, Metadata21),
    /// The returned metadata for an sdist build.
    Sdist(File, Metadata21),
}

struct SolverCache {
    /// A map from package name to the metadata for that package.
    packages: WaitMap<PackageName, SimpleJson>,

    /// A map from wheel SHA to the metadata for that wheel.
    versions: WaitMap<String, Metadata21>,
}

impl Default for SolverCache {
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
