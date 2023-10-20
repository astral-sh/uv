//! Given a set of requirements, find a set of compatible packages.

use std::borrow::Borrow;
use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use futures::channel::mpsc::UnboundedReceiver;
use futures::future::Either;
use futures::{pin_mut, FutureExt, StreamExt, TryFutureExt};
use petgraph::visit::Walker;
use pubgrub::error::PubGrubError;
use pubgrub::range::Range;
use pubgrub::solver::{DependencyConstraints, Incompatibility, State};
use pubgrub::type_aliases::SelectedDependencies;
use tokio::select;
use tracing::{debug, trace};
use waitmap::WaitMap;

use pep508_rs::{MarkerEnvironment, Requirement};
use platform_tags::Tags;
use puffin_client::{File, PypiClient, SimpleJson};
use puffin_package::dist_info_name::DistInfoName;
use puffin_package::metadata::Metadata21;
use puffin_package::package_name::PackageName;
use wheel_filename::WheelFilename;

use crate::error::ResolveError;
use crate::pubgrub::package::PubGrubPackage;
use crate::pubgrub::version::{PubGrubVersion, MIN_VERSION};
use crate::pubgrub::{iter_requirements, version_range};
use crate::resolution::{PinnedPackage, Resolution};

pub struct Resolver<'a> {
    requirements: Vec<Requirement>,
    constraints: Vec<Requirement>,
    markers: &'a MarkerEnvironment,
    tags: &'a Tags,
    client: &'a PypiClient,
    cache: Arc<SolverCache>,
}

impl<'a> Resolver<'a> {
    /// Initialize a new resolver.
    pub fn new(
        requirements: Vec<Requirement>,
        constraints: Vec<Requirement>,
        markers: &'a MarkerEnvironment,
        tags: &'a Tags,
        client: &'a PypiClient,
    ) -> Self {
        Self {
            requirements,
            constraints,
            markers,
            tags,
            client,
            cache: Arc::new(SolverCache::default()),
        }
    }

    /// Resolve a set of requirements into a set of pinned versions.
    pub async fn resolve(self) -> Result<Resolution, ResolveError> {
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

        Ok(Resolution::from(resolution))
    }

    /// Run the `PubGrub` solver.
    async fn solve(
        &self,
        request_sink: &futures::channel::mpsc::UnboundedSender<Request>,
    ) -> Result<PubGrubResolution, ResolveError> {
        let root = PubGrubPackage::Root;

        // Keep track of the packages for which we've requested metadata.
        let mut requested_packages = HashSet::new();
        let mut requested_versions = HashSet::new();
        let mut pins = HashMap::new();

        // Start the solve.
        let mut state = State::init(root.clone(), MIN_VERSION.clone());
        let mut added_dependencies: HashMap<PubGrubPackage, HashSet<PubGrubVersion>> =
            HashMap::default();
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

                for (package, version) in selection.iter() {
                    let x = &state.incompatibilities[package];
                    for incompat_id in x.iter() {
                        let inc = &state.incompatibility_store[*incompat_id];
                        match &inc.kind {
                            Kind::NotRoot(_, _) => {}
                            Kind::NoVersions(_, _) => {}
                            Kind::UnavailableDependencies(_, _) => {}
                            Kind::FromDependencyOf(a, b, c, d) => {
                                if b.contains(version) {
                                    println!("{} {} {} {}", a, b, c, d);
                                }
                            }
                            Kind::DerivedFrom(_, _) => {}
                        }
                        // for y in inc.iter() {
                        //     println!("{} {}", package, y.0);
                        // }
                        // if state.is_terminal(incompat) {
                        //     return Err(PubGrubError::Failure(
                        //         "How did we end up with a terminal incompatibility?".into(),
                        //     )
                        //     .into());
                        // }
                    }

                }


                return Ok(PubGrubResolution { selection, pins });
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
        pins: &mut HashMap<PackageName, HashMap<pep440_rs::Version, File>>,
        in_flight: &mut HashSet<String>,
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

            // Select the latest compatible version.
            let Some(file) = simple_json.files.iter().rev().find(|file| {
                let Ok(name) = WheelFilename::from_str(file.filename.as_str()) else {
                    return false;
                };

                let Ok(version) = pep440_rs::Version::from_str(&name.version) else {
                    return false;
                };

                if !name.is_compatible(self.tags) {
                    return false;
                }

                if !range
                    .borrow()
                    .contains(&PubGrubVersion::from(version.clone()))
                {
                    return false;
                };

                true
            }) else {
                // Short circuit: we couldn't find _any_ compatible versions for a package.
                let (package, _range) = potential_packages.remove(selection);
                return Ok((package, None));
            };

            // Emit a request to fetch the metadata for this version.
            if in_flight.insert(file.hashes.sha256.clone()) {
                request_sink.unbounded_send(Request::Version(file.clone()))?;
            }

            selection = index;
        }

        // TODO(charlie): This is really ugly, but we need to return `T`, not `&T` (and yet
        // we also need to iterate over `potential_packages` multiple times, so we can't
        // use `into_iter()`.)
        let (package, range) = potential_packages.remove(selection);

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
                let Some(wheel) = simple_json.files.iter().rev().find_map(|file| {
                    let Ok(name) = WheelFilename::from_str(file.filename.as_str()) else {
                        return None;
                    };

                    let Ok(version) = pep440_rs::Version::from_str(&name.version) else {
                        return None;
                    };

                    if !name.is_compatible(self.tags) {
                        return None;
                    }

                    if !range
                        .borrow()
                        .contains(&PubGrubVersion::from(version.clone()))
                    {
                        return None;
                    };

                    Some(Wheel {
                        file: file.clone(),
                        name: package_name.clone(),
                        version: version.clone(),
                    })
                }) else {
                    // Short circuit: we couldn't find _any_ compatible versions for a package.
                    return Ok((package, None));
                };

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
                    request_sink.unbounded_send(Request::Version(wheel.file.clone()))?;
                }

                Ok((package, Some(PubGrubVersion::from(wheel.version))))
            }
        };
    }

    /// Given a candidate package and version, return its dependencies.
    async fn get_dependencies(
        &self,
        package: &PubGrubPackage,
        version: &PubGrubVersion,
        pins: &mut HashMap<PackageName, HashMap<pep440_rs::Version, File>>,
        requested_packages: &mut HashSet<PackageName>,
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
            .map({
                |request: Request| match request {
                    Request::Package(package_name) => Either::Left(
                        self.client
                            .simple(package_name.clone())
                            .map_ok(move |metadata| Response::Package(package_name, metadata)),
                    ),
                    Request::Version(file) => Either::Right(
                        self.client
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
                        trace!("Received package metadata for {}", package_name);
                        self.cache.packages.insert(package_name.clone(), metadata);
                    }
                    Response::Version(file, metadata) => {
                        trace!("Received file metadata for {}", file.filename);
                        self.cache
                            .versions
                            .insert(file.hashes.sha256.clone(), metadata);
                    }
                }
            }
        }

        Ok::<(), ResolveError>(())
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

#[derive(Debug)]
struct PubGrubResolution {
    /// The selected dependencies.
    selection: SelectedDependencies<PubGrubPackage, PubGrubVersion>,
    /// The selected file (source or built distribution) for each package.
    pins: HashMap<PackageName, HashMap<pep440_rs::Version, File>>,
}

struct Node<'a> {
    package: &'a PubGrubPackage,
    version: &'a PubGrubVersion,
}

impl From<PubGrubResolution> for Resolution {
    fn from(value: PubGrubResolution) -> Self {
        let mut graph = petgraph::graph::Graph::<Node, File, petgraph::Directed>::with_capacity(
            value.selection.len(),
            value.selection.len(),
        );

        // Add every package to the graph.
        let mut inverse = HashMap::with_capacity(value.selection.len());
        for (package, version) in value.selection.iter() {
            let index = graph.add_node(Node { package, version });
            inverse.insert(package, index);
        }

        // Add edges between dependencies.
        for index in graph.node_indices() {
            let node = &graph[index];

            let package = match node.package {
                PubGrubPackage::Package(package_name, None) => package_name,
                _ => continue,
            };

            let file = value
                .pins
                .get(package)
                .and_then(|versions| versions.get(node.version.into()))
                .unwrap()
                .clone();

            // for requirement in file.

            // graph.add_edge(index, inverse[&PubGrubPackage::Root], file);

            // println!("node: {:?}", node);
        }


        let mut packages = BTreeMap::new();
        for (package, version) in value.selection {
            let PubGrubPackage::Package(package_name, None) = package else {
                continue;
            };

            let version = pep440_rs::Version::from(version);
            let file = value
                .pins
                .get(&package_name)
                .and_then(|versions| versions.get(&version))
                .unwrap()
                .clone();
            let pinned_package = PinnedPackage::new(package_name.clone(), version, file);
            packages.insert(package_name, pinned_package);
        }
        Resolution::new(packages)
    }
}
