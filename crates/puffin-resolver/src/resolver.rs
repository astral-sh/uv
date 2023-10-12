use std::borrow::Borrow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use bitflags::bitflags;
use futures::future::Either;
use futures::{Stream, StreamExt, TryFutureExt};
use pubgrub::error::PubGrubError;
use pubgrub::range::Range;
use pubgrub::solver::{DependencyConstraints, Incompatibility, State};
use pubgrub::type_aliases::SelectedDependencies;
use tracing::debug;

use pep508_rs::{MarkerEnvironment, Requirement};
use platform_tags::Tags;
use puffin_client::{File, PypiClient, PypiClientError, SimpleJson};
use puffin_package::dist_info_name::DistInfoName;
use puffin_package::metadata::Metadata21;
use puffin_package::package_name::PackageName;
use wheel_filename::WheelFilename;

use crate::error::ResolveError;
use crate::facade::{pubgrub_requirements, PubGrubPackage, PubGrubVersion, VERSION_ZERO};
use crate::resolution::{PinnedPackage, Resolution};

pub struct Resolver<'a> {
    requirements: Vec<Requirement>,
    markers: &'a MarkerEnvironment,
    tags: &'a Tags,
    client: &'a PypiClient,
    reporter: Option<Box<dyn Reporter>>,
    // cache: Mutex<SolverCache>,
}

impl<'a> Resolver<'a> {
    /// Initialize a new resolver.
    pub fn new(
        requirements: Vec<Requirement>,
        markers: &'a MarkerEnvironment,
        tags: &'a Tags,
        client: &'a PypiClient,
    ) -> Self {
        Self {
            requirements,
            markers,
            tags,
            client,
            reporter: None,
            // cache: Mutex::new( SolverCache::default()),
        }
    }

    /// Set the [`Reporter`] to use for this resolver.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        Self {
            reporter: Some(Box::new(reporter)),
            ..self
        }
    }

    /// Resolve a set of requirements into a set of pinned versions.
    pub async fn resolve(self, _flags: ResolveFlags) -> Result<Resolution, ResolveError> {
        // A channel to fetch package metadata (e.g., given `flask`, fetch all versions) and version
        // metadata (e.g., given `flask==1.0.0`, fetch the metadata for that version).
        let (request_sink, request_stream) = futures::channel::mpsc::unbounded();

        let (package_sink, mut package_stream) = futures::channel::mpsc::unbounded();
        let (file_sink, mut file_stream) = futures::channel::mpsc::unbounded();

        // Initialize the package stream.
        let client = Arc::new(self.client.clone());

        // Kick off a thread to handle the responses.
        tokio::spawn({
            let client = client.clone();
            async move {
                let mut response_stream = request_stream
                    .map({
                        |request: Request| match request {
                            Request::Package(package_name) => Either::Left(
                                client
                                    // TODO(charlie): Remove this clone.
                                    .simple(package_name.clone())
                                    .map_ok(move |metadata| {
                                        Response::Package(package_name, metadata)
                                    }),
                            ),
                            Request::Version(file) => Either::Right(
                                client
                                    // TODO(charlie): Remove this clone.
                                    .file(file.clone())
                                    .map_ok(move |metadata| Response::Version(file, metadata)),
                            ),
                        }
                    })
                    .buffer_unordered(32)
                    .ready_chunks(32);

                while let Some(chunk) = response_stream.next().await {
                    for response in chunk {
                        match response? {
                            Response::Package(package_name, metadata) => {
                                // debug!("Received metadata for {}", package_name);
                                // {
                                //     let mut cache = cache.lock().unwrap();
                                //     cache.packages.insert(package_name.clone(), metadata);
                                // }
                                // debug!("Sending on sink {}", package_name);
                                package_sink.unbounded_send((package_name, metadata))?;
                            }
                            Response::Version(file, metadata) => {
                                // debug!("Received metadata for {}", file.filename);
                                // {
                                //     debug!("Inserting into cache: {}", file.hashes.sha256);
                                //     let mut cache = cache.lock().unwrap();
                                //     debug!("Inserting into cache: {}", file.hashes.sha256);
                                //     cache.versions.insert(file.hashes.sha256.clone(), metadata);
                                // }
                                // debug!("Sending on sink {}", file.hashes.sha256);
                                file_sink.unbounded_send((file, metadata))?;
                            }
                        }
                    }
                }

                Ok::<(), anyhow::Error>(())
            }
        });

        // Push all the requirements into the package sink.
        for requirement in &self.requirements {
            debug!("Adding root dependency: {}", requirement);
            let package_name = PackageName::normalize(&requirement.name);
            request_sink.unbounded_send(Request::Package(package_name))?;
        }

        let mut cache = SolverCache::default();
        let selected_dependencies = self
            .solve(&mut cache, &request_sink, &mut package_stream, &mut file_stream)
            .await?;

        // Map to our own internal resolution type.
        let mut resolution = BTreeMap::new();
        for (package, version) in selected_dependencies {
            let PubGrubPackage::Package(package_name, None) = package else {
                continue;
            };

            let version = pep440_rs::Version::from(version);
            let file = cache
                .files
                .get(&package_name)
                .and_then(|versions| versions.get(&version))
                .unwrap()
                .clone();
            let pinned_package = PinnedPackage::new(package_name.clone(), version, file);
            resolution.insert(package_name, pinned_package);
        }

        Ok(Resolution::new(resolution))
    }

    async fn solve(
        &self,
        cache: &mut SolverCache,
        request_sink: &futures::channel::mpsc::UnboundedSender<Request>,
        package_stream: &mut (impl Stream<Item=(PackageName, SimpleJson)> + Unpin),
        file_stream: &mut (impl Stream<Item=(File, Metadata21)> + Unpin),
    ) -> Result<SelectedDependencies<PubGrubPackage, PubGrubVersion>, ResolveError> {
        let root = PubGrubPackage::Root;

        // Start the solve.
        let mut state = State::init(root.clone(), VERSION_ZERO.clone());
        let mut added_dependencies: HashMap<PubGrubPackage, HashSet<PubGrubVersion>> =
            HashMap::default();
        let mut next = root;

        loop {
            // Run unit propagation.
            state.unit_propagation(next)?;

            // Fetch the list of candidates.
            let Some(potential_packages) = state.partial_solution.potential_packages() else {
                return state.partial_solution.extract_solution().ok_or_else(|| {
                    PubGrubError::Failure(
                        "How did we end up with no package to choose but no solution?".into(),
                    )
                        .into()
                });
            };

            // Choose a package version.
            let potential_packages = potential_packages.collect::<Vec<_>>();
            let decision = self
                .choose_package_version(
                    potential_packages,
                    cache,
                    request_sink,
                    package_stream,
                    file_stream,
                )
                .await?;
            next = decision.0.clone();

            // Pick the next compatible version.
            let v = match decision.1 {
                None => {
                    let term_intersection = state
                        .partial_solution
                        .term_intersection_for_package(&next)
                        .expect("a package was chosen but we don't have a term.");

                    let inc = Incompatibility::no_versions(next.clone(), term_intersection.clone());
                    state.add_incompatibility(inc);
                    continue;
                }
                Some(x) => x,
            };

            if added_dependencies
                .entry(next.clone())
                .or_default()
                .insert(v.clone())
            {
                // Retrieve that package dependencies.
                let p = &next;
                let dependencies = match self
                    .get_dependencies(p, &v, cache, request_sink, package_stream, file_stream)
                    .await?
                {
                    Dependencies::Unknown => {
                        state.add_incompatibility(Incompatibility::unavailable_dependencies(
                            p.clone(),
                            v.clone(),
                        ));
                        continue;
                    }
                    Dependencies::Known(x) => {
                        if x.contains_key(p) {
                            return Err(PubGrubError::SelfDependency {
                                package: p.clone(),
                                version: v.clone(),
                            }
                                .into());
                        }
                        if let Some((dependent, _)) = x.iter().find(|(_, r)| r == &&Range::none()) {
                            return Err(PubGrubError::DependencyOnTheEmptySet {
                                package: p.clone(),
                                version: v.clone(),
                                dependent: dependent.clone(),
                            }
                                .into());
                        }
                        x
                    }
                };

                // Add that package and version if the dependencies are not problematic.
                let dep_incompats = state.add_incompatibility_from_dependencies(
                    p.clone(),
                    v.clone(),
                    &dependencies,
                );

                // TODO: I don't think this check can actually happen.
                // We might want to put it under #[cfg(debug_assertions)].
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
                    p.clone(),
                    v,
                    dep_incompats,
                    &state.incompatibility_store,
                );
            } else {
                // `dep_incompats` are already in `incompatibilities` so we know there are not satisfied
                // terms and can add the decision directly.
                state.partial_solution.add_decision(next.clone(), v);
            }
        }
    }

    async fn choose_package_version<T: Borrow<PubGrubPackage>, U: Borrow<Range<PubGrubVersion>>>(
        &self,
        mut potential_packages: Vec<(T, U)>,
        cache: &mut SolverCache,
        request_sink: &futures::channel::mpsc::UnboundedSender<Request>,
        package_stream: &mut (impl Stream<Item=(PackageName, SimpleJson)> + Unpin),
        file_stream: &mut (impl Stream<Item=(File, Metadata21)> + Unpin),
    ) -> Result<(T, Option<PubGrubVersion>), ResolveError> {
        loop {

            // Check if any of the potential packages are available in the cache.
            if let Some(index) =
                potential_packages
                    .iter()
                    .enumerate()
                    .find_map(|(index, (package, _range))| {
                        let PubGrubPackage::Package(package_name, _) = package.borrow() else {
                            return Some(index);
                        };
                        cache.packages.get(package_name).map(|_| index)
                    })
            {
                // TODO(charlie): This is really ugly, but we need to return `T`, not `&T` (and yet
                // we also need to iterate over `potential_packages` multiple times, so we can't
                // use `into_iter()`.)
                let (package, range) = potential_packages.remove(index);

                return match package.borrow() {
                    PubGrubPackage::Root => Ok((package, Some(VERSION_ZERO.clone()))),
                    PubGrubPackage::Package(package_name, _) => {
                        let simple_json = &cache.packages[package_name];

                        // Find a compatible version.
                        let name_version_file = simple_json.files.iter().rev().find_map(|file| {
                            let Ok(name) = WheelFilename::from_str(file.filename.as_str()) else {
                                // debug!("Ignoring non-wheel: {}", file.filename);
                                return None;
                            };

                            let Ok(version) = pep440_rs::Version::from_str(&name.version) else {
                                // debug!("Ignoring invalid version: {}", name.version);
                                return None;
                            };

                            if !name.is_compatible(self.tags) {
                                // debug!("Ignoring incompatible wheel: {}", file.filename);
                                return None;
                            }

                            if !range
                                .borrow()
                                .contains(&PubGrubVersion::from(version.clone()))
                            {
                                // debug!("Ignoring incompatible version: {}", version);
                                return None;
                            };

                            Some((package_name.clone(), version.clone(), file.clone()))
                        });

                        if let Some((name, version, file)) = name_version_file {
                            debug!("Selecting: {}=={} ({})", name, version, file.filename);

                            // Emit a request to fetch the metadata for this version.
                            if !cache.versions.contains_key(&file.hashes.sha256) {
                                request_sink.unbounded_send(Request::Version(file.clone()))?;
                            }

                            // We want to return a package pinned to a specific version; but we _also_ want to
                            // store the exact file that we selected to satisfy that version.
                            cache
                                .files
                                .entry(name)
                                .or_default()
                                .insert(version.clone(), file);

                            Ok((package, Some(PubGrubVersion::from(version))))
                        } else {
                            // We have metadata for the package, but no compatible version.
                            Ok((package, None))
                        }
                    }
                };
            }

            // Otherwise, wait for the next available package.
            let (package, metadata) = package_stream.next().await.unwrap();
            println!("Received package: {:?}", package);
            cache.packages.insert(package, metadata);

        }
    }

    async fn get_dependencies(
        &self,
        package: &PubGrubPackage,
        version: &PubGrubVersion,
        cache: &mut SolverCache,
        request_sink: &futures::channel::mpsc::UnboundedSender<Request>,
        package_stream: &mut (impl Stream<Item=(PackageName, SimpleJson)> + Unpin),
        file_stream: &mut (impl Stream<Item=(File, Metadata21)> + Unpin),
    ) -> Result<Dependencies, ResolveError> {
        match package {
            PubGrubPackage::Root => {
                let mut constraints = DependencyConstraints::default();
                for (package, version) in
                pubgrub_requirements(self.requirements.iter(), None, self.markers)
                {
                    constraints.insert(package, version);
                }
                Ok(Dependencies::Known(constraints))
            }
            PubGrubPackage::Package(package_name, extra) => {
                debug!("Fetching dependencies for {}[{:?}]", package_name, extra);

                loop {
                    // Check if the dependencies are available in the cache.
                    if let Some(metadata) = cache
                        .files
                        .get(package_name)
                        .and_then(|versions| versions.get(version.into()))
                        .and_then(|file| cache.versions.get(&file.hashes.sha256))
                    {
                        let mut constraints = DependencyConstraints::default();

                        for (package, version) in pubgrub_requirements(
                            metadata.requires_dist.iter(),
                            extra.as_ref(),
                            self.markers,
                        ) {
                            debug!("Adding transitive dependency: {package} {version}");

                            // Emit a request to fetch the metadata for this package.
                            if let PubGrubPackage::Package(package_name, None) = &package {
                                if !cache.packages.contains_key(package_name) {
                                    debug!("Requesting metadata for {}", package_name);
                                    let package_name = package_name.clone();
                                    request_sink.unbounded_send(Request::Package(package_name))?;
                                }
                            }

                            // Add it to the constraints.
                            constraints.insert(package, version);
                        }

                        if let Some(extra) = extra {
                            if !metadata.provides_extras.iter().any(|provided_extra| {
                                DistInfoName::normalize(provided_extra) == *extra
                            }) {
                                return Ok(Dependencies::Unknown);
                            }
                            constraints.insert(
                                PubGrubPackage::Package(package_name.clone(), None),
                                Range::exact(version.clone()),
                            );
                        }

                        return Ok(Dependencies::Known(constraints));
                    }

                    debug!("Waiting for metadata for {}[{:?}]", package_name, extra);

                    // Otherwise, wait for the next available file.
                    let (file, metadata) = file_stream.next().await.unwrap();
                    cache.versions.insert(file.hashes.sha256, metadata);
                }
            }
        }
    }
}

#[derive(Debug)]
enum Request {
    /// A request to fetch the metadata for a package.
    Package(PackageName),
    /// A request to fetch the metadata for a specific version of a package.
    Version(File),
}

#[derive(Debug)]
enum Response {
    /// The returned metadata for a package.
    Package(PackageName, SimpleJson),
    /// The returned metadata for a specific version of a package.
    Version(File, Metadata21),
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a dependency is added to the resolution.
    fn on_dependency_added(&self);

    /// Callback to invoke when a dependency is resolved.
    fn on_resolve_progress(&self, package: &PinnedPackage);

    /// Callback to invoke when the resolution is complete.
    fn on_resolve_complete(&self);
}

bitflags! {
    #[derive(Debug, Copy, Clone, Default)]
    pub struct ResolveFlags: u8 {
        /// Don't install package dependencies.
        const NO_DEPS = 1 << 0;
    }
}

#[derive(Debug, Clone, Default)]
struct SolverCache {
    /// A map from package name to the metadata for that package.
    packages: HashMap<PackageName, SimpleJson>,

    files: HashMap<PackageName, HashMap<pep440_rs::Version, File>>,

    /// A map from wheel SHA to the metadata for that wheel.
    versions: HashMap<String, Metadata21>,
}

/// An enum used by [`DependencyProvider`] that holds information about package dependencies.
/// For each [Package] there is a [Range] of concrete versions it allows as a dependency.
#[derive(Clone)]
enum Dependencies {
    /// Package dependencies are unavailable.
    Unknown,
    /// Container for all available package versions.
    Known(DependencyConstraints<PubGrubPackage, PubGrubVersion>),
}
