//! Given a set of requirements, find a set of compatible packages.

use std::collections::hash_map::Entry;
use std::collections::BTreeMap;
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
use tracing::{debug, error, trace};
use url::Url;
use waitmap::WaitMap;

use distribution_filename::{SourceDistributionFilename, WheelFilename};
use pep508_rs::{MarkerEnvironment, Requirement};
use platform_tags::Tags;
use puffin_client::RegistryClient;
use puffin_distribution::{RemoteDistributionRef, VersionOrUrl};
use puffin_package::dist_info_name::DistInfoName;
use puffin_package::package_name::PackageName;
use puffin_package::pypi_types::{File, Metadata21, SimpleJson};
use puffin_traits::BuildContext;

use crate::candidate_selector::CandidateSelector;
use crate::error::ResolveError;
use crate::file::{DistributionFile, SdistFile, WheelFile};
use crate::manifest::Manifest;
use crate::pubgrub::{iter_requirements, version_range};
use crate::pubgrub::{PubGrubPackage, PubGrubPriorities, PubGrubVersion, MIN_VERSION};
use crate::resolution::Graph;
use crate::source_distribution::SourceDistributionBuildTree;

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
        let mut in_flight = InFlight::default();
        let mut pins = FxHashMap::default();
        let mut priorities = PubGrubPriorities::default();

        // Push all the requirements into the package sink.
        for requirement in &self.requirements {
            debug!("Adding root dependency: {requirement}");
            let package_name = PackageName::normalize(&requirement.name);
            match &requirement.version_or_url {
                // If this is a registry-based package, fetch the package metadata.
                None | Some(pep508_rs::VersionOrUrl::VersionSpecifier(_)) => {
                    if in_flight.insert_package(&package_name) {
                        priorities.add(package_name.clone());
                        request_sink.unbounded_send(Request::Package(package_name))?;
                    }
                }
                // If this is a URL-based package, fetch the source.
                Some(pep508_rs::VersionOrUrl::Url(url)) => {
                    if in_flight.insert_url(url) {
                        priorities.add(package_name.clone());
                        if WheelFilename::try_from(url).is_ok() {
                            request_sink.unbounded_send(Request::WheelUrl(
                                package_name.clone(),
                                url.clone(),
                            ))?;
                        } else {
                            request_sink.unbounded_send(Request::SdistUrl(
                                package_name.clone(),
                                url.clone(),
                            ))?;
                        }
                    }
                }
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
                &mut in_flight,
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
                    &mut in_flight,
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
                        &mut in_flight,
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

    /// Visit a [`PubGrubPackage`] prior to selection. This should be called on a [`PubGrubPackage`]
    /// before it is selected, to allow metadata to be fetched in parallel.
    fn visit_package(
        package: &PubGrubPackage,
        priorities: &mut PubGrubPriorities,
        in_flight: &mut InFlight,
        request_sink: &futures::channel::mpsc::UnboundedSender<Request>,
    ) -> Result<(), ResolveError> {
        match package {
            PubGrubPackage::Root => {}
            PubGrubPackage::Package(package_name, _extra, None) => {
                // Emit a request to fetch the metadata for this package.
                if in_flight.insert_package(package_name) {
                    priorities.add(package_name.clone());
                    request_sink.unbounded_send(Request::Package(package_name.clone()))?;
                }
            }
            PubGrubPackage::Package(package_name, _extra, Some(url)) => {
                // Emit a request to fetch the metadata for this package.
                if in_flight.insert_url(url) {
                    priorities.add(package_name.clone());
                    if WheelFilename::try_from(url).is_ok() {
                        // Kick off a request to download the wheel.
                        request_sink
                            .unbounded_send(Request::WheelUrl(package_name.clone(), url.clone()))?;
                    } else {
                        // Otherwise, assume this is a source distribution.
                        request_sink
                            .unbounded_send(Request::SdistUrl(package_name.clone(), url.clone()))?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Visit the set of [`PubGrubPackage`] candidates prior to selection. This allows us to fetch
    /// metadata for all of the packages in parallel.
    fn pre_visit(
        &self,
        packages: impl Iterator<Item = (&'a PubGrubPackage, &'a Range<PubGrubVersion>)>,
        in_flight: &mut InFlight,
        request_sink: &futures::channel::mpsc::UnboundedSender<Request>,
    ) -> Result<(), ResolveError> {
        // Iterate over the potential packages, and fetch file metadata for any of them. These
        // represent our current best guesses for the versions that we _might_ select.
        for (package, range) in packages {
            let PubGrubPackage::Package(package_name, _extra, None) = package else {
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
                    if in_flight.insert_file(&file) {
                        request_sink.unbounded_send(Request::Wheel(file.clone()))?;
                    }
                }
                DistributionFile::Sdist(file) => {
                    if in_flight.insert_file(&file) {
                        request_sink.unbounded_send(Request::Sdist(
                            candidate.package_name.clone(),
                            candidate.version.clone().into(),
                            file.clone(),
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
        in_flight: &mut InFlight,
        request_sink: &futures::channel::mpsc::UnboundedSender<Request>,
    ) -> Result<Option<PubGrubVersion>, ResolveError> {
        return match package {
            PubGrubPackage::Root => Ok(Some(MIN_VERSION.clone())),

            PubGrubPackage::Package(package_name, _extra, Some(url)) => {
                debug!("Searching for a compatible version of {package_name} @ {url} ({range})",);

                if let Ok(wheel_filename) = WheelFilename::try_from(url) {
                    // If the URL is that of a wheel, extract the version.
                    let version = PubGrubVersion::from(wheel_filename.version);
                    if range.contains(&version) {
                        Ok(Some(version))
                    } else {
                        Ok(None)
                    }
                } else {
                    // Otherwise, assume this is a source distribution.
                    let entry = self.index.versions.wait(url.as_str()).await.unwrap();
                    let metadata = entry.value();
                    let version = PubGrubVersion::from(metadata.version.clone());
                    if range.contains(&version) {
                        Ok(Some(version))
                    } else {
                        Ok(None)
                    }
                }
            }

            PubGrubPackage::Package(package_name, _extra, None) => {
                // Wait for the metadata to be available.
                let entry = self.index.packages.wait(package_name).await.unwrap();
                let version_map = entry.value();

                debug!("Searching for a compatible version of {package_name} ({range})");

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
                        if in_flight.insert_file(&file) {
                            request_sink.unbounded_send(Request::Wheel(file.clone()))?;
                        }
                    }
                    DistributionFile::Sdist(file) => {
                        if in_flight.insert_file(&file) {
                            request_sink.unbounded_send(Request::Sdist(
                                candidate.package_name.clone(),
                                candidate.version.clone().into(),
                                file.clone(),
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
        in_flight: &mut InFlight,
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
                    Self::visit_package(&package, priorities, in_flight, request_sink)?;

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
                for (package, version) in
                    iter_requirements(self.constraints.iter(), None, None, self.markers)
                {
                    if let Some(range) = constraints.get_mut(&package) {
                        *range = range.intersection(&version);
                    }
                }

                Ok(Dependencies::Known(constraints))
            }

            PubGrubPackage::Package(package_name, extra, url) => {
                // Wait for the metadata to be available.
                let entry = match url {
                    Some(url) => self.index.versions.wait(url.as_str()).await.unwrap(),
                    None => {
                        let versions = pins.get(package_name).unwrap();
                        let file = versions.get(version.into()).unwrap();
                        self.index.versions.wait(&file.hashes.sha256).await.unwrap()
                    }
                };
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
                    Self::visit_package(&package, priorities, in_flight, request_sink)?;

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
                    let package = PubGrubPackage::Package(
                        PackageName::normalize(&constraint.name),
                        None,
                        None,
                    );
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
                        PubGrubPackage::Package(package_name.clone(), None, None),
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
                    trace!("Received package metadata for: {}", package_name);

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
                    trace!("Received wheel metadata for: {}", file.filename);
                    self.index
                        .versions
                        .insert(file.hashes.sha256.clone(), metadata);
                }
                Response::Sdist(file, metadata) => {
                    trace!("Received sdist metadata for: {}", file.filename);
                    self.index
                        .versions
                        .insert(file.hashes.sha256.clone(), metadata);
                }
                Response::WheelUrl(url, metadata) => {
                    trace!("Received remote wheel metadata for: {}", url);
                    self.index.versions.insert(url.to_string(), metadata);
                }
                Response::SdistUrl(url, metadata) => {
                    trace!("Received remote source distribution metadata for: {}", url);
                    self.index.versions.insert(url.to_string(), metadata);
                }
            }
        }

        Ok::<(), ResolveError>(())
    }

    async fn process_request(&'a self, request: Request) -> Result<Response, ResolveError> {
        match request {
            // Fetch package metadata from the registry.
            Request::Package(package_name) => {
                self.client
                    .simple(package_name.clone())
                    .map_ok(move |metadata| Response::Package(package_name, metadata))
                    .map_err(ResolveError::Client)
                    .await
            }
            // Fetch wheel metadata from the registry.
            Request::Wheel(file) => {
                self.client
                    .file(file.clone().into())
                    .map_ok(move |metadata| Response::Wheel(file, metadata))
                    .map_err(ResolveError::Client)
                    .await
            }
            // Build a source distribution from the registry, returning its metadata.
            Request::Sdist(package_name, version, file) => {
                let build_tree = SourceDistributionBuildTree::new(self.build_context);
                let distribution =
                    RemoteDistributionRef::from_registry(&package_name, &version, &file);
                let metadata = match build_tree.find_dist_info(&distribution, self.tags) {
                    Ok(Some(metadata)) => metadata,
                    Ok(None) => build_tree
                        .download_and_build_sdist(&distribution, self.client)
                        .await
                        .map_err(|err| ResolveError::RegistryDistribution {
                            filename: file.filename.clone(),
                            err,
                        })?,
                    Err(err) => {
                        error!(
                            "Failed to read source distribution {distribution} from cache: {err}",
                        );
                        build_tree
                            .download_and_build_sdist(&distribution, self.client)
                            .await
                            .map_err(|err| ResolveError::RegistryDistribution {
                                filename: file.filename.clone(),
                                err,
                            })?
                    }
                };
                Ok(Response::Sdist(file, metadata))
            }
            // Build a source distribution from a remote URL, returning its metadata.
            Request::SdistUrl(package_name, url) => {
                let build_tree = SourceDistributionBuildTree::new(self.build_context);
                let distribution = RemoteDistributionRef::from_url(&package_name, &url);
                let metadata = match build_tree.find_dist_info(&distribution, self.tags) {
                    Ok(Some(metadata)) => metadata,
                    Ok(None) => build_tree
                        .download_and_build_sdist(&distribution, self.client)
                        .await
                        .map_err(|err| ResolveError::UrlDistribution {
                            url: url.clone(),
                            err,
                        })?,
                    Err(err) => {
                        error!(
                            "Failed to read source distribution {distribution} from cache: {err}",
                        );
                        build_tree
                            .download_and_build_sdist(&distribution, self.client)
                            .await
                            .map_err(|err| ResolveError::UrlDistribution {
                                url: url.clone(),
                                err,
                            })?
                    }
                };
                Ok(Response::SdistUrl(url, metadata))
            }
            // Fetch wheel metadata from a remote URL.
            Request::WheelUrl(package_name, url) => {
                let build_tree = SourceDistributionBuildTree::new(self.build_context);
                let distribution = RemoteDistributionRef::from_url(&package_name, &url);
                let metadata = match build_tree.find_dist_info(&distribution, self.tags) {
                    Ok(Some(metadata)) => metadata,
                    Ok(None) => build_tree
                        .download_wheel(&distribution, self.client)
                        .await
                        .map_err(|err| ResolveError::UrlDistribution {
                            url: url.clone(),
                            err,
                        })?,
                    Err(err) => {
                        error!(
                            "Failed to read built distribution {distribution} from cache: {err}",
                        );
                        build_tree
                            .download_wheel(&distribution, self.client)
                            .await
                            .map_err(|err| ResolveError::UrlDistribution {
                                url: url.clone(),
                                err,
                            })?
                    }
                };
                Ok(Response::WheelUrl(url, metadata))
            }
        }
    }

    fn on_progress(&self, package: &PubGrubPackage, version: &PubGrubVersion) {
        if let Some(reporter) = self.reporter.as_ref() {
            match package {
                PubGrubPackage::Root => {}
                PubGrubPackage::Package(package_name, extra, Some(url)) => {
                    reporter.on_progress(package_name, extra.as_ref(), VersionOrUrl::Url(url));
                }
                PubGrubPackage::Package(package_name, extra, None) => {
                    reporter.on_progress(
                        package_name,
                        extra.as_ref(),
                        VersionOrUrl::Version(version.into()),
                    );
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

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a dependency is resolved.
    fn on_progress(&self, name: &PackageName, extra: Option<&DistInfoName>, version: VersionOrUrl);

    /// Callback to invoke when the resolution is complete.
    fn on_complete(&self);
}

/// Fetch the metadata for an item
#[derive(Debug)]
enum Request {
    /// A request to fetch the metadata for a package.
    Package(PackageName),
    /// A request to fetch wheel metadata from a registry.
    Wheel(WheelFile),
    /// A request to fetch source distribution metadata from a registry.
    Sdist(PackageName, pep440_rs::Version, SdistFile),
    /// A request to fetch wheel metadata from a remote URL.
    WheelUrl(PackageName, Url),
    /// A request to fetch source distribution metadata from a remote URL.
    SdistUrl(PackageName, Url),
}

#[derive(Debug)]
enum Response {
    /// The returned metadata for a package hosted on a registry.
    Package(PackageName, SimpleJson),
    /// The returned metadata for a wheel hosted on a registry.
    Wheel(WheelFile, Metadata21),
    /// The returned metadata for a source distribution hosted on a registry.
    Sdist(SdistFile, Metadata21),
    /// The returned metadata for a wheel hosted on a remote URL.
    WheelUrl(Url, Metadata21),
    /// The returned metadata for a source distribution hosted on a remote URL.
    SdistUrl(Url, Metadata21),
}

pub(crate) type VersionMap = BTreeMap<PubGrubVersion, DistributionFile>;

/// In-memory index of in-flight network requests. Any request in an [`InFlight`] state will be
/// eventually be inserted into an [`Index`].
#[derive(Debug, Default)]
struct InFlight {
    /// The set of requested [`PackageName`]s.
    packages: FxHashSet<PackageName>,
    /// The set of requested registry-based files, represented by their SHAs.
    files: FxHashSet<String>,
    /// The set of requested URLs.
    urls: FxHashSet<Url>,
}

impl InFlight {
    fn insert_package(&mut self, package_name: &PackageName) -> bool {
        self.packages.insert(package_name.clone())
    }

    fn insert_file(&mut self, file: &File) -> bool {
        self.files.insert(file.hashes.sha256.clone())
    }

    fn insert_url(&mut self, url: &Url) -> bool {
        self.urls.insert(url.clone())
    }
}

/// In-memory index of package metadata.
struct Index {
    /// A map from package name to the metadata for that package.
    packages: WaitMap<PackageName, VersionMap>,

    /// A map from wheel SHA or URL to the metadata for that wheel.
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
