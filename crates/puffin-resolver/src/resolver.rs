//! Given a set of requirements, find a set of compatible packages.

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
use tokio::sync::Mutex;
use tracing::{debug, error, trace};
use url::Url;
use waitmap::WaitMap;

use distribution_filename::{SourceDistributionFilename, WheelFilename};
use pep508_rs::{MarkerEnvironment, Requirement};
use platform_tags::Tags;
use puffin_cache::{CanonicalUrl, RepositoryUrl};
use puffin_client::RegistryClient;
use puffin_distribution::{RemoteDistributionRef, VersionOrUrl};
use puffin_normalize::{ExtraName, PackageName};
use puffin_traits::BuildContext;
use pypi_types::{File, Metadata21, SimpleJson};

use crate::candidate_selector::CandidateSelector;
use crate::distribution::{SourceDistributionFetcher, WheelFetcher};
use crate::error::ResolveError;
use crate::file::{DistributionFile, SdistFile, WheelFile};
use crate::manifest::Manifest;
use crate::pubgrub::{
    PubGrubDependencies, PubGrubPackage, PubGrubPriorities, PubGrubVersion, MIN_VERSION,
};
use crate::resolution::Graph;

pub struct Resolver<'a, Context: BuildContext + Sync, Task> {
    project: Option<PackageName>,
    requirements: Vec<Requirement>,
    constraints: Vec<Requirement>,
    allowed_urls: AllowedUrls,
    markers: &'a MarkerEnvironment,
    tags: &'a Tags,
    client: &'a RegistryClient,
    selector: CandidateSelector,
    index: Arc<Index>,
    locks: Arc<Locks>,
    build_context: &'a Context,
    reporter: Option<Box<dyn Reporter<Task>>>,
}

impl<'a, Context: BuildContext + Sync, Task> Resolver<'a, Context, Task> {
    /// Initialize a new resolver.
    pub fn new(
        manifest: Manifest,
        markers: &'a MarkerEnvironment,
        tags: &'a Tags,
        client: &'a RegistryClient,
        build_context: &'a Context,
    ) -> Self {
        Self {
            index: Arc::new(Index::default()),
            locks: Arc::new(Locks::default()),
            selector: CandidateSelector::from(&manifest),
            allowed_urls: manifest
                .requirements
                .iter()
                .chain(manifest.constraints.iter())
                .filter_map(|req| {
                    if let Some(pep508_rs::VersionOrUrl::Url(url)) = &req.version_or_url {
                        Some(url)
                    } else {
                        None
                    }
                })
                .collect(),
            project: manifest.project,
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
    pub fn with_reporter(self, reporter: impl Reporter<Task> + 'static) -> Self {
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
        let root = PubGrubPackage::Root(self.project.clone());

        // Keep track of the packages for which we've requested metadata.
        let mut in_flight = InFlight::default();
        let mut pins = FxHashMap::default();
        let mut priorities = PubGrubPriorities::default();

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
                return Ok(Graph::from_state(
                    &selection,
                    &pins,
                    &self.index.redirects,
                    &state,
                ));
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
            PubGrubPackage::Root(_) => {}
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
                        request_sink.unbounded_send(Request::Wheel(
                            candidate.package_name.clone(),
                            file.clone(),
                        ))?;
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
            PubGrubPackage::Root(_) => Ok(Some(MIN_VERSION.clone())),

            PubGrubPackage::Package(package_name, _extra, Some(url)) => {
                debug!("Searching for a compatible version of {package_name} @ {url} ({range})",);

                // If the URL wasn't declared in the direct dependencies or constraints, reject it.
                if !self.allowed_urls.contains(url) {
                    return Err(ResolveError::DisallowedUrl(
                        package_name.clone(),
                        url.clone(),
                    ));
                }

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
                            request_sink.unbounded_send(Request::Wheel(
                                candidate.package_name.clone(),
                                file.clone(),
                            ))?;
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
            PubGrubPackage::Root(_) => {
                // Add the root requirements.
                let constraints = PubGrubDependencies::try_from_requirements(
                    &self.requirements,
                    &self.constraints,
                    None,
                    None,
                    self.markers,
                )?;

                for (package, version) in constraints.iter() {
                    debug!("Adding direct dependency: {package:?} {version:?}");

                    // Emit a request to fetch the metadata for this package.
                    Self::visit_package(package, priorities, in_flight, request_sink)?;
                }

                Ok(Dependencies::Known(constraints.into()))
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

                let mut constraints = PubGrubDependencies::try_from_requirements(
                    &metadata.requires_dist,
                    &self.constraints,
                    extra.as_ref(),
                    Some(package_name),
                    self.markers,
                )?;

                for (package, version) in constraints.iter() {
                    debug!("Adding transitive dependency: {package} {version}");

                    // Emit a request to fetch the metadata for this package.
                    Self::visit_package(package, priorities, in_flight, request_sink)?;
                }

                if let Some(extra) = extra {
                    if !metadata
                        .provides_extras
                        .iter()
                        .any(|provided_extra| provided_extra == extra)
                    {
                        return Ok(Dependencies::Unknown);
                    }
                    constraints.insert(
                        PubGrubPackage::Package(package_name.clone(), None, None),
                        Range::singleton(version.clone()),
                    );
                }

                Ok(Dependencies::Known(constraints.into()))
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
                    trace!("Received package metadata for: {package_name}");

                    // Group the distributions by version and kind, discarding any incompatible
                    // distributions.
                    let mut version_map: VersionMap = BTreeMap::new();
                    for file in metadata.files {
                        if let Ok(filename) = WheelFilename::from_str(file.filename.as_str()) {
                            if filename.is_compatible(self.tags) {
                                let version = PubGrubVersion::from(filename.version.clone());
                                match version_map.entry(version) {
                                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                                        if matches!(entry.get(), DistributionFile::Sdist(_)) {
                                            // Wheels get precedence over source distributions.
                                            entry.insert(DistributionFile::from(WheelFile(
                                                file, filename,
                                            )));
                                        }
                                    }
                                    std::collections::btree_map::Entry::Vacant(entry) => {
                                        entry.insert(DistributionFile::from(WheelFile(
                                            file, filename,
                                        )));
                                    }
                                }
                            }
                        } else if let Ok(filename) =
                            SourceDistributionFilename::parse(file.filename.as_str(), &package_name)
                        {
                            let version = PubGrubVersion::from(filename.version.clone());
                            if let std::collections::btree_map::Entry::Vacant(entry) =
                                version_map.entry(version)
                            {
                                entry.insert(DistributionFile::from(SdistFile(file, filename)));
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
                Response::WheelUrl(url, precise, metadata) => {
                    trace!("Received remote wheel metadata for: {url}");
                    self.index.versions.insert(url.to_string(), metadata);
                    if let Some(precise) = precise {
                        self.index.redirects.insert(url, precise);
                    }
                }
                Response::SdistUrl(url, precise, metadata) => {
                    trace!("Received remote source distribution metadata for: {url}");
                    self.index.versions.insert(url.to_string(), metadata);
                    if let Some(precise) = precise {
                        self.index.redirects.insert(url, precise);
                    }
                }
            }
        }

        Ok::<(), ResolveError>(())
    }

    async fn process_request(&self, request: Request) -> Result<Response, ResolveError> {
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
            Request::Wheel(package_name, file) => {
                let metadata = self
                    .client
                    .wheel_metadata(file.0.clone(), file.1.clone())
                    .map_err(ResolveError::Client)
                    .await?;

                if metadata.name != package_name {
                    return Err(ResolveError::NameMismatch {
                        metadata: metadata.name,
                        given: package_name,
                    });
                }

                Ok(Response::Wheel(file, metadata))
            }
            // Build a source distribution from the registry, returning its metadata.
            Request::Sdist(package_name, version, file) => {
                let builder = SourceDistributionFetcher::new(self.build_context);
                let distribution =
                    RemoteDistributionRef::from_registry(&package_name, &version, &file);
                let metadata = match builder.find_dist_info(&distribution, self.tags) {
                    Ok(Some(metadata)) => metadata,
                    Ok(None) => builder
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
                        builder
                            .download_and_build_sdist(&distribution, self.client)
                            .await
                            .map_err(|err| ResolveError::RegistryDistribution {
                                filename: file.filename.clone(),
                                err,
                            })?
                    }
                };

                if metadata.name != package_name {
                    return Err(ResolveError::NameMismatch {
                        metadata: metadata.name,
                        given: package_name,
                    });
                }

                Ok(Response::Sdist(file, metadata))
            }
            // Build a source distribution from a remote URL, returning its metadata.
            Request::SdistUrl(package_name, url) => {
                let lock = self.locks.acquire(&url).await;
                let _guard = lock.lock().await;

                let fetcher = SourceDistributionFetcher::new(self.build_context);
                let precise = fetcher
                    .precise(&RemoteDistributionRef::from_url(&package_name, &url))
                    .await
                    .map_err(|err| ResolveError::UrlDistribution {
                        url: url.clone(),
                        err,
                    })?;

                let distribution = RemoteDistributionRef::from_url(
                    &package_name,
                    precise.as_ref().unwrap_or(&url),
                );

                let task = self
                    .reporter
                    .as_ref()
                    .map(|reporter| reporter.on_build_start(&distribution));

                let metadata = match fetcher.find_dist_info(&distribution, self.tags) {
                    Ok(Some(metadata)) => {
                        debug!("Found source distribution metadata in cache: {url}");
                        metadata
                    }
                    Ok(None) => {
                        debug!("Downloading source distribution from: {url}");
                        fetcher
                            .download_and_build_sdist(&distribution, self.client)
                            .await
                            .map_err(|err| ResolveError::UrlDistribution {
                                url: url.clone(),
                                err,
                            })?
                    }
                    Err(err) => {
                        error!(
                            "Failed to read source distribution {distribution} from cache: {err}",
                        );
                        fetcher
                            .download_and_build_sdist(&distribution, self.client)
                            .await
                            .map_err(|err| ResolveError::UrlDistribution {
                                url: url.clone(),
                                err,
                            })?
                    }
                };

                if metadata.name != package_name {
                    return Err(ResolveError::NameMismatch {
                        metadata: metadata.name,
                        given: package_name,
                    });
                }

                if let Some(task) = task {
                    if let Some(reporter) = self.reporter.as_ref() {
                        reporter.on_build_complete(&distribution, task);
                    }
                }

                Ok(Response::SdistUrl(url, precise, metadata))
            }
            // Fetch wheel metadata from a remote URL.
            Request::WheelUrl(package_name, url) => {
                let lock = self.locks.acquire(&url).await;
                let _guard = lock.lock().await;

                let fetcher = WheelFetcher::new(self.build_context.cache());
                let distribution = RemoteDistributionRef::from_url(&package_name, &url);
                let metadata = match fetcher.find_dist_info(&distribution, self.tags) {
                    Ok(Some(metadata)) => {
                        debug!("Found wheel metadata in cache: {url}");
                        metadata
                    }
                    Ok(None) => {
                        debug!("Downloading wheel from: {url}");
                        fetcher
                            .download_wheel(&distribution, self.client)
                            .await
                            .map_err(|err| ResolveError::UrlDistribution {
                                url: url.clone(),
                                err,
                            })?
                    }
                    Err(err) => {
                        error!("Failed to read wheel {distribution} from cache: {err}",);
                        fetcher
                            .download_wheel(&distribution, self.client)
                            .await
                            .map_err(|err| ResolveError::UrlDistribution {
                                url: url.clone(),
                                err,
                            })?
                    }
                };

                if metadata.name != package_name {
                    return Err(ResolveError::NameMismatch {
                        metadata: metadata.name,
                        given: package_name,
                    });
                }

                Ok(Response::WheelUrl(url, None, metadata))
            }
        }
    }

    fn on_progress(&self, package: &PubGrubPackage, version: &PubGrubVersion) {
        if let Some(reporter) = self.reporter.as_ref() {
            match package {
                PubGrubPackage::Root(_) => {}
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

pub trait Reporter<Task>: Send + Sync {
    /// Callback to invoke when a dependency is resolved.
    fn on_progress(&self, name: &PackageName, extra: Option<&ExtraName>, version: VersionOrUrl);

    /// Callback to invoke when the resolution is complete.
    fn on_complete(&self);

    /// Callback to invoke when a source distribution build is kicked off.
    fn on_build_start(&self, distribution: &RemoteDistributionRef<'_>) -> Task;

    /// Callback to invoke when a source distribution build is complete.
    fn on_build_complete(&self, distribution: &RemoteDistributionRef<'_>, task: Task);
}

/// Fetch the metadata for an item
#[derive(Debug)]
enum Request {
    /// A request to fetch the metadata for a package.
    Package(PackageName),
    /// A request to fetch wheel metadata from a registry.
    Wheel(PackageName, WheelFile),
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
    WheelUrl(Url, Option<Url>, Metadata21),
    /// The returned metadata for a source distribution hosted on a remote URL.
    SdistUrl(Url, Option<Url>, Metadata21),
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

/// A set of locks used to prevent concurrent access to the same resource.
#[derive(Debug, Default)]
struct Locks(Mutex<FxHashMap<String, Arc<Mutex<()>>>>);

impl Locks {
    /// Acquire a lock on the given resource.
    async fn acquire(&self, url: &Url) -> Arc<Mutex<()>> {
        let mut map = self.0.lock().await;
        map.entry(puffin_cache::digest(&RepositoryUrl::new(url)))
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }
}

/// In-memory index of package metadata.
struct Index {
    /// A map from package name to the metadata for that package.
    packages: WaitMap<PackageName, VersionMap>,

    /// A map from wheel SHA or URL to the metadata for that wheel.
    versions: WaitMap<String, Metadata21>,

    /// A map from source URL to precise URL.
    redirects: WaitMap<Url, Url>,
}

impl Default for Index {
    fn default() -> Self {
        Self {
            packages: WaitMap::new(),
            versions: WaitMap::new(),
            redirects: WaitMap::new(),
        }
    }
}

#[derive(Debug, Default)]
struct AllowedUrls(FxHashSet<CanonicalUrl>);

impl AllowedUrls {
    fn contains(&self, url: &Url) -> bool {
        self.0.contains(&CanonicalUrl::new(url))
    }
}

impl<'a> FromIterator<&'a Url> for AllowedUrls {
    fn from_iter<T: IntoIterator<Item = &'a Url>>(iter: T) -> Self {
        Self(iter.into_iter().map(CanonicalUrl::new).collect())
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
