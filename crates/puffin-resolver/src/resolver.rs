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
use tracing::{debug, error, trace};
use url::Url;
use waitmap::WaitMap;

use distribution_filename::{SourceDistFilename, WheelFilename};
use pep508_rs::{MarkerEnvironment, Requirement};
use platform_tags::Tags;
use puffin_cache::CanonicalUrl;
use puffin_client::RegistryClient;
use puffin_distribution::{
    BuiltDist, DirectUrlSourceDist, Dist, GitSourceDist, Identifier, Metadata, SourceDist,
    VersionOrUrl,
};
use puffin_normalize::{ExtraName, PackageName};
use puffin_traits::BuildContext;
use pypi_types::{File, Metadata21, SimpleJson};

use crate::candidate_selector::CandidateSelector;
use crate::distribution::{BuiltDistFetcher, SourceDistFetcher, SourceDistributionReporter};
use crate::error::ResolveError;
use crate::file::{DistFile, SdistFile, WheelFile};
use crate::locks::Locks;
use crate::manifest::Manifest;
use crate::pubgrub::{
    PubGrubDependencies, PubGrubPackage, PubGrubPriorities, PubGrubVersion, MIN_VERSION,
};
use crate::resolution::Graph;

pub struct Resolver<'a, Context: BuildContext + Sync> {
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
    reporter: Option<Arc<dyn Reporter>>,
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
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        Self {
            reporter: Some(Arc::new(reporter)),
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
                // Emit a request to fetch the metadata for this distribution.
                if in_flight.insert_url(url) {
                    priorities.add(package_name.clone());
                    let distribution = Dist::from_url(package_name.clone(), url.clone());
                    request_sink.unbounded_send(Request::Dist(distribution))?;
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
            if in_flight.insert_file(&candidate.file) {
                let distribution = Dist::from_registry(
                    candidate.package_name.clone(),
                    candidate.version.clone().into(),
                    candidate.file.clone().into(),
                );
                request_sink.unbounded_send(Request::Dist(distribution))?;
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
                    let entry = self
                        .index
                        .distributions
                        .wait(&url.distribution_id())
                        .await
                        .unwrap();
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
                if in_flight.insert_file(&candidate.file) {
                    let distribution = Dist::from_registry(
                        candidate.package_name.clone(),
                        candidate.version.clone().into(),
                        candidate.file.clone().into(),
                    );
                    request_sink.unbounded_send(Request::Dist(distribution))?;
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
                    debug!("Adding direct dependency: {package}{version}");

                    // Emit a request to fetch the metadata for this package.
                    Self::visit_package(package, priorities, in_flight, request_sink)?;
                }

                Ok(Dependencies::Known(constraints.into()))
            }

            PubGrubPackage::Package(package_name, extra, url) => {
                // Wait for the metadata to be available.
                let entry = match url {
                    Some(url) => self
                        .index
                        .distributions
                        .wait(&url.distribution_id())
                        .await
                        .unwrap(),
                    None => {
                        let versions = pins.get(package_name).unwrap();
                        let file = versions.get(version.into()).unwrap();
                        self.index
                            .distributions
                            .wait(&file.distribution_id())
                            .await
                            .unwrap()
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
                    debug!("Adding transitive dependency: {package}{version}");

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
                                        if matches!(entry.get(), DistFile::Sdist(_)) {
                                            // Wheels get precedence over source distributions.
                                            entry.insert(DistFile::from(WheelFile(file)));
                                        }
                                    }
                                    std::collections::btree_map::Entry::Vacant(entry) => {
                                        entry.insert(DistFile::from(WheelFile(file)));
                                    }
                                }
                            }
                        } else if let Ok(filename) =
                            SourceDistFilename::parse(file.filename.as_str(), &package_name)
                        {
                            // Only add source dists compatible with the python version
                            // TODO(konstin): https://github.com/astral-sh/puffin/issues/406
                            if file
                                .requires_python
                                .as_ref()
                                .map_or(true, |requires_python| {
                                    requires_python
                                        .contains(self.build_context.interpreter_info().version())
                                })
                            {
                                let version = PubGrubVersion::from(filename.version.clone());
                                if let std::collections::btree_map::Entry::Vacant(entry) =
                                    version_map.entry(version)
                                {
                                    entry.insert(DistFile::from(SdistFile(file)));
                                }
                            }
                        }
                    }

                    self.index
                        .packages
                        .insert(package_name.clone(), version_map);
                }
                Response::Dist(Dist::Built(distribution), metadata, ..) => {
                    trace!("Received built distribution metadata for: {distribution}");
                    self.index
                        .distributions
                        .insert(distribution.distribution_id(), metadata);
                }
                Response::Dist(Dist::Source(distribution), metadata, precise) => {
                    trace!("Received source distribution metadata for: {distribution}");
                    self.index
                        .distributions
                        .insert(distribution.distribution_id(), metadata);
                    if let Some(precise) = precise {
                        match distribution {
                            SourceDist::DirectUrl(sdist) => {
                                self.index.redirects.insert(sdist.url.clone(), precise);
                            }
                            SourceDist::Git(sdist) => {
                                self.index.redirects.insert(sdist.url.clone(), precise);
                            }
                            SourceDist::Registry(_) => {}
                        }
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

            // Fetch wheel metadata.
            Request::Dist(Dist::Built(distribution)) => {
                let metadata =
                    match &distribution {
                        BuiltDist::Registry(wheel) => {
                            self.client
                                .wheel_metadata(wheel.file.clone())
                                .map_err(ResolveError::Client)
                                .await?
                        }
                        BuiltDist::DirectUrl(wheel) => {
                            let fetcher = BuiltDistFetcher::new(self.build_context.cache());
                            match fetcher.find_dist_info(wheel, self.tags) {
                                Ok(Some(metadata)) => {
                                    debug!("Found wheel metadata in cache: {wheel}");
                                    metadata
                                }
                                Ok(None) => {
                                    debug!("Downloading wheel: {wheel}");
                                    fetcher.download_wheel(wheel, self.client).await.map_err(
                                        |err| {
                                            ResolveError::from_built_dist(distribution.clone(), err)
                                        },
                                    )?
                                }
                                Err(err) => {
                                    error!("Failed to read wheel from cache: {err}");
                                    fetcher.download_wheel(wheel, self.client).await.map_err(
                                        |err| {
                                            ResolveError::from_built_dist(distribution.clone(), err)
                                        },
                                    )?
                                }
                            }
                        }
                    };

                if metadata.name != *distribution.name() {
                    return Err(ResolveError::NameMismatch {
                        metadata: metadata.name,
                        given: distribution.name().clone(),
                    });
                }

                Ok(Response::Dist(Dist::Built(distribution), metadata, None))
            }

            // Fetch source distribution metadata.
            Request::Dist(Dist::Source(sdist)) => {
                let lock = self.locks.acquire(&sdist).await;
                let _guard = lock.lock().await;

                let fetcher = if let Some(reporter) = &self.reporter {
                    SourceDistFetcher::new(self.build_context).with_reporter(Facade {
                        reporter: reporter.clone(),
                    })
                } else {
                    SourceDistFetcher::new(self.build_context)
                };

                let precise = fetcher
                    .precise(&sdist)
                    .await
                    .map_err(|err| ResolveError::from_source_dist(sdist.clone(), err))?;

                let task = self
                    .reporter
                    .as_ref()
                    .map(|reporter| reporter.on_build_start(&sdist));

                let metadata = {
                    // Insert the `precise`, if it exists.
                    let sdist = match sdist.clone() {
                        SourceDist::DirectUrl(sdist) => {
                            SourceDist::DirectUrl(DirectUrlSourceDist {
                                url: precise.clone().unwrap_or_else(|| sdist.url.clone()),
                                ..sdist
                            })
                        }
                        SourceDist::Git(sdist) => SourceDist::Git(GitSourceDist {
                            url: precise.clone().unwrap_or_else(|| sdist.url.clone()),
                            ..sdist
                        }),
                        sdist @ SourceDist::Registry(_) => sdist,
                    };

                    match fetcher.find_dist_info(&sdist, self.tags) {
                        Ok(Some(metadata)) => {
                            debug!("Found source distribution metadata in cache: {sdist}");
                            metadata
                        }
                        Ok(None) => {
                            debug!("Downloading source distribution: {sdist}");
                            fetcher
                                .download_and_build_sdist(&sdist, self.client)
                                .await
                                .map_err(|err| ResolveError::from_source_dist(sdist.clone(), err))?
                        }
                        Err(err) => {
                            error!("Failed to read source distribution from cache: {err}",);
                            fetcher
                                .download_and_build_sdist(&sdist, self.client)
                                .await
                                .map_err(|err| ResolveError::from_source_dist(sdist.clone(), err))?
                        }
                    }
                };

                if metadata.name != *sdist.name() {
                    return Err(ResolveError::NameMismatch {
                        metadata: metadata.name,
                        given: sdist.name().clone(),
                    });
                }

                if let Some(task) = task {
                    if let Some(reporter) = self.reporter.as_ref() {
                        reporter.on_build_complete(&sdist, task);
                    }
                }

                Ok(Response::Dist(Dist::Source(sdist), metadata, precise))
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

pub type BuildId = usize;

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a dependency is resolved.
    fn on_progress(&self, name: &PackageName, extra: Option<&ExtraName>, version: VersionOrUrl);

    /// Callback to invoke when the resolution is complete.
    fn on_complete(&self);

    /// Callback to invoke when a source distribution build is kicked off.
    fn on_build_start(&self, dist: &SourceDist) -> usize;

    /// Callback to invoke when a source distribution build is complete.
    fn on_build_complete(&self, dist: &SourceDist, id: usize);

    /// Callback to invoke when a repository checkout begins.
    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize;

    /// Callback to invoke when a repository checkout completes.
    fn on_checkout_complete(&self, url: &Url, rev: &str, index: usize);
}

/// A facade for converting from [`Reporter`] to  [`puffin_git::Reporter`].
struct Facade {
    reporter: Arc<dyn Reporter>,
}

impl SourceDistributionReporter for Facade {
    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize {
        self.reporter.on_checkout_start(url, rev)
    }

    fn on_checkout_complete(&self, url: &Url, rev: &str, index: usize) {
        self.reporter.on_checkout_complete(url, rev, index);
    }
}

/// Fetch the metadata for an item
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum Request {
    /// A request to fetch the metadata for a package.
    Package(PackageName),
    /// A request to fetch the metadata for a built or source distribution.
    Dist(Dist),
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum Response {
    /// The returned metadata for a package hosted on a registry.
    Package(PackageName, SimpleJson),
    /// The returned metadata for a distribution.
    Dist(Dist, Metadata21, Option<Url>),
}

pub(crate) type VersionMap = BTreeMap<PubGrubVersion, DistFile>;

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

    fn insert_file(&mut self, file: &DistFile) -> bool {
        match file {
            DistFile::Wheel(file) => self.files.insert(file.hashes.sha256.clone()),
            DistFile::Sdist(file) => self.files.insert(file.hashes.sha256.clone()),
        }
    }

    fn insert_url(&mut self, url: &Url) -> bool {
        self.urls.insert(url.clone())
    }
}

/// In-memory index of package metadata.
struct Index {
    /// A map from package name to the metadata for that package.
    packages: WaitMap<PackageName, VersionMap>,

    /// A map from distribution SHA to metadata for that distribution.
    distributions: WaitMap<String, Metadata21>,

    /// A map from source URL to precise URL.
    redirects: WaitMap<Url, Url>,
}

impl Default for Index {
    fn default() -> Self {
        Self {
            packages: WaitMap::new(),
            distributions: WaitMap::new(),
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
