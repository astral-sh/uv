//! Given a set of requirements, find a set of compatible packages.

use std::borrow::Borrow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use futures::future::Either;
use futures::{StreamExt, TryFutureExt};
use pubgrub::error::PubGrubError;
use pubgrub::range::Range;
use pubgrub::solver::{DependencyConstraints, Incompatibility, State};
use pubgrub::type_aliases::SelectedDependencies;
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
use crate::pubgrub::iter_requirements;
use crate::pubgrub::package::PubGrubPackage;
use crate::pubgrub::version::{PubGrubVersion, MIN_VERSION};
use crate::resolution::{PinnedPackage, Resolution};

pub struct Resolver<'a> {
    requirements: Vec<Requirement>,
    markers: &'a MarkerEnvironment,
    tags: &'a Tags,
    client: &'a PypiClient,
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
        }
    }

    /// Resolve a set of requirements into a set of pinned versions.
    pub async fn resolve(self) -> Result<Resolution, ResolveError> {
        let client = Arc::new(self.client.clone());
        let cache = Arc::new(SolverCache::default());

        // A channel to fetch package metadata (e.g., given `flask`, fetch all versions) and version
        // metadata (e.g., given `flask==1.0.0`, fetch the metadata for that version).
        let (request_sink, request_stream) = futures::channel::mpsc::unbounded();
        tokio::spawn({
            let cache = cache.clone();
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
                                trace!("Received package metadata for {}", package_name);
                                cache.packages.insert(package_name.clone(), metadata);
                            }
                            Response::Version(file, metadata) => {
                                trace!("Received file metadata for {}", file.filename);
                                cache.versions.insert(file.hashes.sha256.clone(), metadata);
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

        // Track the packages and versions that we've requested.
        let mut requested_packages = HashSet::new();
        let mut requested_versions = HashSet::new();
        let mut pins = HashMap::new();

        let selected_dependencies = self
            .solve(
                &cache,
                &mut pins,
                &mut requested_packages,
                &mut requested_versions,
                &request_sink,
            )
            .await?;

        // Map to our own internal resolution type.
        let mut resolution = BTreeMap::new();
        for (package, version) in selected_dependencies {
            let PubGrubPackage::Package(package_name, None) = package else {
                continue;
            };

            let version = pep440_rs::Version::from(version);
            let file = pins
                .get(&package_name)
                .and_then(|versions| versions.get(&version))
                .unwrap()
                .clone();
            let pinned_package = PinnedPackage::new(package_name.clone(), version, file);
            resolution.insert(package_name, pinned_package);
        }

        Ok(Resolution::new(resolution))
    }

    /// Run the `PubGrub` solver.
    async fn solve(
        &self,
        cache: &SolverCache,
        pins: &mut HashMap<PackageName, HashMap<pep440_rs::Version, File>>,
        requested_packages: &mut HashSet<PackageName>,
        requested_versions: &mut HashSet<String>,
        request_sink: &futures::channel::mpsc::UnboundedSender<Request>,
    ) -> Result<SelectedDependencies<PubGrubPackage, PubGrubVersion>, ResolveError> {
        let root = PubGrubPackage::Root;

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
                    pins,
                    requested_versions,
                    request_sink,
                )
                .await?;
            next = decision.0.clone();

            // Pick the next compatible version.
            let version = match decision.1 {
                None => {
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
                        cache,
                        pins,
                        requested_packages,
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
                        if let Some((dependent, _)) =
                            constraints.iter().find(|(_, r)| r == &&Range::none())
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
        cache: &SolverCache,
        pins: &mut HashMap<PackageName, HashMap<pep440_rs::Version, File>>,
        in_flight: &mut HashSet<String>,
        request_sink: &futures::channel::mpsc::UnboundedSender<Request>,
    ) -> Result<(T, Option<PubGrubVersion>), ResolveError> {
        let mut selection = 0usize;

        // Iterate over the potential packages, and fetch file metadata for any of them. These
        // represent our current best guesses for the versions that we might want to select.
        for (index, (package, range)) in potential_packages.iter().enumerate() {
            let PubGrubPackage::Package(package_name, _) = package.borrow() else {
                continue;
            };

            // If we don't have metadata for this package,  we can't make an early decision.
            let Some(entry) = cache.packages.get(package_name) else {
                continue;
            };

            // Find a compatible version.
            let simple_json = entry.value();
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
                continue;
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
                // TODO(charlie): Ideally, we'd choose the first package for which metadata is
                // available.
                let entry = cache.packages.wait(package_name).await.unwrap();
                let simple_json = entry.value();

                // Find a compatible version.
                let name_version_file = simple_json.files.iter().rev().find_map(|file| {
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

                    Some((package_name.clone(), version.clone(), file.clone()))
                });

                if let Some((name, version, file)) = name_version_file {
                    debug!("Selecting: {}=={} ({})", name, version, file.filename);

                    // We want to return a package pinned to a specific version; but we _also_ want to
                    // store the exact file that we selected to satisfy that version.
                    pins.entry(name)
                        .or_default()
                        .insert(version.clone(), file.clone());

                    // Emit a request to fetch the metadata for this version.
                    if cache.versions.get(&file.hashes.sha256).is_none() {
                        if in_flight.insert(file.hashes.sha256.clone()) {
                            request_sink.unbounded_send(Request::Version(file.clone()))?;
                        }
                    }

                    Ok((package, Some(PubGrubVersion::from(version))))
                } else {
                    // We have metadata for the package, but no compatible version.
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
        cache: &SolverCache,
        pins: &mut HashMap<PackageName, HashMap<pep440_rs::Version, File>>,
        requested_packages: &mut HashSet<PackageName>,
        request_sink: &futures::channel::mpsc::UnboundedSender<Request>,
    ) -> Result<Dependencies, ResolveError> {
        match package {
            PubGrubPackage::Root => {
                let mut constraints = DependencyConstraints::default();
                for (package, version) in
                    iter_requirements(self.requirements.iter(), None, self.markers)
                {
                    constraints.insert(package, version);
                }
                Ok(Dependencies::Known(constraints))
            }
            PubGrubPackage::Package(package_name, extra) => {
                debug!("Fetching dependencies for {}[{:?}]", package_name, extra);

                // Wait for the metadata to be available.
                let versions = pins.get(package_name).unwrap();
                let file = versions.get(version.into()).unwrap();
                let entry = cache.versions.wait(&file.hashes.sha256).await.unwrap();
                let metadata = entry.value();

                let mut constraints = DependencyConstraints::default();

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
                    constraints.insert(package, version);
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
                        Range::exact(version.clone()),
                    );
                }

                Ok(Dependencies::Known(constraints))
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
/// For each [Package] there is a [Range] of concrete versions it allows as a dependency.
#[derive(Clone)]
enum Dependencies {
    /// Package dependencies are unavailable.
    Unknown,
    /// Container for all available package versions.
    Known(DependencyConstraints<PubGrubPackage, PubGrubVersion>),
}
