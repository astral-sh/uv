//! Given a set of requirements, find a set of compatible packages.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use futures::channel::mpsc::UnboundedReceiver;
use futures::{pin_mut, FutureExt, StreamExt, TryFutureExt};
use pubgrub::error::PubGrubError;
use pubgrub::range::Range;
use pubgrub::solver::{Incompatibility, State};
use pubgrub::type_aliases::DependencyConstraints;
use rustc_hash::{FxHashMap, FxHashSet};
use tokio::select;
use tracing::{debug, trace};
use url::Url;

use distribution_filename::WheelFilename;
use distribution_types::{BuiltDist, Dist, Metadata, SourceDist, VersionOrUrl};
use pep508_rs::{MarkerEnvironment, Requirement};
use platform_tags::Tags;
use puffin_cache::CanonicalUrl;
use puffin_client::RegistryClient;
use puffin_distribution::{DistributionDatabase, DistributionDatabaseError};
use puffin_normalize::{ExtraName, PackageName};
use puffin_traits::{BuildContext, OnceMap};
use pypi_types::{IndexUrl, Metadata21};

use crate::candidate_selector::CandidateSelector;
use crate::error::ResolveError;
use crate::manifest::Manifest;
use crate::pins::FilePins;
use crate::pubgrub::{
    PubGrubDependencies, PubGrubPackage, PubGrubPriorities, PubGrubVersion, MIN_VERSION,
};
use crate::resolution::Graph;
use crate::version_map::VersionMap;
use crate::yanks::AllowedYanks;
use crate::ResolutionOptions;

type VersionMapResponse = Result<(IndexUrl, VersionMap), puffin_client::Error>;
type WheelMetadataResponse = Result<(Metadata21, Option<Url>), DistributionDatabaseError>;

pub trait ResolverProvider: Send + Sync {
    /// Get the version map for a package.
    fn get_version_map<'io>(
        &'io self,
        package_name: &'io PackageName,
    ) -> Pin<Box<dyn Future<Output = VersionMapResponse> + Send + 'io>>;

    /// Get the metadata for a distribution.
    ///
    /// For a wheel, this is done by querying it's (remote) metadata, for a source dist we
    /// (fetch and) build the source distribution and return the metadata from the built
    /// distribution.
    fn get_or_build_wheel_metadata<'io>(
        &'io self,
        dist: &'io Dist,
    ) -> Pin<Box<dyn Future<Output = WheelMetadataResponse> + Send + 'io>>;

    /// Set the [`Reporter`] to use for this installer.
    #[must_use]
    fn with_reporter(self, reporter: impl puffin_distribution::Reporter + 'static) -> Self;
}

/// The main IO backend for the resolver, which does cached requests network requests using the
/// [`RegistryClient`] and [`DistributionDatabase`].
pub struct DefaultResolverProvider<'a, Context: BuildContext + Send + Sync> {
    client: &'a RegistryClient,
    fetcher: DistributionDatabase<'a, Context>,
    build_context: &'a Context,
    tags: &'a Tags,
    markers: &'a MarkerEnvironment,
    exclude_newer: Option<DateTime<Utc>>,
    allowed_yanks: AllowedYanks,
}

impl<'a, Context: BuildContext + Send + Sync> DefaultResolverProvider<'a, Context> {
    pub fn new(
        client: &'a RegistryClient,
        fetcher: DistributionDatabase<'a, Context>,
        build_context: &'a Context,
        tags: &'a Tags,
        markers: &'a MarkerEnvironment,
        exclude_newer: Option<DateTime<Utc>>,
        allowed_yanks: AllowedYanks,
    ) -> Self {
        Self {
            client,
            fetcher,
            build_context,
            tags,
            markers,
            exclude_newer,
            allowed_yanks,
        }
    }
}

impl<'a, Context: BuildContext + Send + Sync> ResolverProvider
    for DefaultResolverProvider<'a, Context>
{
    fn get_version_map<'io>(
        &'io self,
        package_name: &'io PackageName,
    ) -> Pin<Box<dyn Future<Output = VersionMapResponse> + Send + 'io>> {
        Box::pin(
            self.client
                .simple(package_name)
                .map_ok(move |(index, metadata)| {
                    // TODO(konstin): I think the client should return something in between
                    // `SimpleJson` and `VersionMap`, with source dists and wheels grouped by
                    // version, but python version and exclude newer not yet applied. This should
                    // work well with caching, testing and PEP 503 html APIs.
                    // (https://github.com/astral-sh/puffin/issues/412)
                    (
                        index,
                        VersionMap::from_metadata(
                            metadata,
                            package_name,
                            self.tags,
                            self.markers,
                            self.build_context.interpreter(),
                            &self.allowed_yanks,
                            self.exclude_newer.as_ref(),
                        ),
                    )
                }),
        )
    }

    fn get_or_build_wheel_metadata<'io>(
        &'io self,
        dist: &'io Dist,
    ) -> Pin<Box<dyn Future<Output = WheelMetadataResponse> + Send + 'io>> {
        Box::pin(self.fetcher.get_or_build_wheel_metadata(dist))
    }

    /// Set the [`puffin_distribution::Reporter`] to use for this installer.
    #[must_use]
    fn with_reporter(self, reporter: impl puffin_distribution::Reporter + 'static) -> Self {
        Self {
            fetcher: self.fetcher.with_reporter(reporter),
            ..self
        }
    }
}

pub struct Resolver<'a, Provider: ResolverProvider> {
    project: Option<PackageName>,
    requirements: Vec<Requirement>,
    constraints: Vec<Requirement>,
    allowed_urls: AllowedUrls,
    markers: &'a MarkerEnvironment,
    selector: CandidateSelector,
    index: Arc<Index>,
    reporter: Option<Arc<dyn Reporter>>,
    provider: Provider,
}

impl<'a, Context: BuildContext + Send + Sync> Resolver<'a, DefaultResolverProvider<'a, Context>> {
    /// Initialize a new resolver using the default backend doing real requests.
    pub fn new(
        manifest: Manifest,
        options: ResolutionOptions,
        markers: &'a MarkerEnvironment,
        tags: &'a Tags,
        client: &'a RegistryClient,
        build_context: &'a Context,
    ) -> Self {
        let provider = DefaultResolverProvider::new(
            client,
            DistributionDatabase::new(build_context.cache(), tags, client, build_context),
            build_context,
            tags,
            markers,
            options.exclude_newer,
            manifest
                .requirements
                .iter()
                .chain(manifest.constraints.iter())
                .collect(),
        );
        Self::new_custom_io(manifest, options, markers, provider)
    }
}

impl<'a, Provider: ResolverProvider> Resolver<'a, Provider> {
    /// Initialize a new resolver using a user provided backend.
    pub fn new_custom_io(
        manifest: Manifest,
        options: ResolutionOptions,
        markers: &'a MarkerEnvironment,
        provider: Provider,
    ) -> Self {
        Self {
            index: Arc::new(Index::default()),
            selector: CandidateSelector::for_resolution(&manifest, options),
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
            reporter: None,
            provider,
        }
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
        let index = Index::default();
        let mut pins = FilePins::default();
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
            self.pre_visit(state.partial_solution.prioritized_packages(), request_sink)
                .await?;

            // Choose a package version.
            let Some(highest_priority_pkg) =
                state
                    .partial_solution
                    .pick_highest_priority_pkg(|package, _range| {
                        priorities.get(package).unwrap_or_default()
                    })
            else {
                let selection = state.partial_solution.extract_solution();
                return Graph::from_state(&selection, &pins, &self.index.redirects, &state);
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
                    .get_dependencies(package, &version, &mut priorities, &index, request_sink)
                    .await?
                {
                    Dependencies::Unknown => {
                        state.add_incompatibility(Incompatibility::unavailable_dependencies(
                            package.clone(),
                            version.clone(),
                        ));
                        continue;
                    }
                    Dependencies::Unusable(reason) => {
                        state.add_incompatibility(Incompatibility::unusable_dependencies(
                            package.clone(),
                            version.clone(),
                            reason.clone(),
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
    async fn visit_package(
        package: &PubGrubPackage,
        priorities: &mut PubGrubPriorities,
        index: &Index,
        request_sink: &futures::channel::mpsc::UnboundedSender<Request>,
    ) -> Result<(), ResolveError> {
        match package {
            PubGrubPackage::Root(_) => {}
            PubGrubPackage::Package(package_name, _extra, None) => {
                // Emit a request to fetch the metadata for this package.
                if index.packages.register(package_name).await {
                    priorities.add(package_name.clone());
                    request_sink.unbounded_send(Request::Package(package_name.clone()))?;
                }
            }
            PubGrubPackage::Package(package_name, _extra, Some(url)) => {
                // Emit a request to fetch the metadata for this distribution.
                if index.redirects.register(url).await {
                    let distribution = Dist::from_url(package_name.clone(), url.clone())?;
                    priorities.add(distribution.name().clone());
                    request_sink.unbounded_send(Request::Dist(distribution))?;
                }
            }
        }
        Ok(())
    }

    /// Visit the set of [`PubGrubPackage`] candidates prior to selection. This allows us to fetch
    /// metadata for all of the packages in parallel.
    async fn pre_visit(
        &self,
        packages: impl Iterator<Item = (&'a PubGrubPackage, &'a Range<PubGrubVersion>)>,
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
            let (index, version_map) = entry.value();

            // Try to find a compatible version. If there aren't any compatible versions,
            // short-circuit and return `None`.
            let Some(candidate) = self.selector.select(package_name, range, version_map) else {
                // Short-circuit: we couldn't find _any_ compatible versions for a package.
                return Ok(());
            };

            // Emit a request to fetch the metadata for this version.
            if self
                .index
                .distributions
                .register(candidate.resolve().sha256())
                .await
            {
                let distribution = candidate.into_distribution(index.clone());
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
        pins: &mut FilePins,
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
                    let dist = PubGrubDistribution::from_url(package_name, url);
                    let entry = self.index.distributions.wait(&dist.package_id()).await;
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
                let entry = self.index.packages.wait(package_name).await;
                let (index, version_map) = entry.value();

                debug!("Searching for a compatible version of {package_name} ({range})");

                // Find a compatible version.
                let Some(candidate) = self.selector.select(package_name, range, version_map) else {
                    // Short circuit: we couldn't find _any_ compatible versions for a package.
                    return Ok(None);
                };

                debug!(
                    "Selecting: {}=={} ({})",
                    candidate.name(),
                    candidate.version(),
                    candidate.resolve().filename()
                );

                // We want to return a package pinned to a specific version; but we _also_ want to
                // store the exact file that we selected to satisfy that version.
                pins.insert(&candidate, index);

                let version = candidate.version().clone();

                // Emit a request to fetch the metadata for this version.
                if self
                    .index
                    .distributions
                    .register(candidate.resolve().sha256())
                    .await
                {
                    let distribution = candidate.into_distribution(index.clone());
                    request_sink.unbounded_send(Request::Dist(distribution))?;
                }

                Ok(Some(version))
            }
        };
    }

    /// Given a candidate package and version, return its dependencies.
    async fn get_dependencies(
        &self,
        package: &PubGrubPackage,
        version: &PubGrubVersion,
        priorities: &mut PubGrubPriorities,
        index: &Index,
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
                );
                if let Err(
                    err @ (ResolveError::ConflictingVersions(..)
                    | ResolveError::ConflictingUrls(..)),
                ) = constraints
                {
                    return Ok(Dependencies::Unusable(Some(err.to_string())));
                }
                let constraints = constraints?;

                for (package, version) in constraints.iter() {
                    debug!("Adding direct dependency: {package}{version}");

                    // Emit a request to fetch the metadata for this package.
                    Self::visit_package(package, priorities, index, request_sink).await?;
                }

                Ok(Dependencies::Known(constraints.into()))
            }

            PubGrubPackage::Package(package_name, extra, url) => {
                // Wait for the metadata to be available.
                let dist = match url {
                    Some(url) => PubGrubDistribution::from_url(package_name, url),
                    None => PubGrubDistribution::from_registry(package_name, version),
                };
                let entry = self.index.distributions.wait(&dist.package_id()).await;
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
                    Self::visit_package(package, priorities, index, request_sink).await?;
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
                Response::Package(package_name, index, version_map) => {
                    trace!("Received package metadata for: {package_name}");
                    self.index.packages.done(package_name, (index, version_map));
                }
                Response::Dist(Dist::Built(distribution), metadata, ..) => {
                    trace!("Received built distribution metadata for: {distribution}");
                    self.index
                        .distributions
                        .done(distribution.package_id(), metadata);
                }
                Response::Dist(Dist::Source(distribution), metadata, precise) => {
                    trace!("Received source distribution metadata for: {distribution}");
                    self.index
                        .distributions
                        .done(distribution.package_id(), metadata);
                    if let Some(precise) = precise {
                        match distribution {
                            SourceDist::DirectUrl(sdist) => {
                                self.index.redirects.done(sdist.url.clone(), precise);
                            }
                            SourceDist::Git(sdist) => {
                                self.index.redirects.done(sdist.url.clone(), precise);
                            }
                            SourceDist::Path(sdist) => {
                                self.index.redirects.done(sdist.url.clone(), precise);
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
                let (index, metadata) = self
                    .provider
                    .get_version_map(&package_name)
                    .await
                    .map_err(ResolveError::Client)?;
                Ok(Response::Package(package_name, index, metadata))
            }

            Request::Dist(dist) => {
                let (metadata, precise) = self
                    .provider
                    .get_or_build_wheel_metadata(&dist)
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
                Ok(Response::Dist(dist, metadata, precise))
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

/// A facade for converting from [`Reporter`] to [`puffin_distribution::Reporter`].
struct Facade {
    reporter: Arc<dyn Reporter>,
}

impl puffin_distribution::Reporter for Facade {
    fn on_build_start(&self, dist: &SourceDist) -> usize {
        self.reporter.on_build_start(dist)
    }

    fn on_build_complete(&self, dist: &SourceDist, id: usize) {
        self.reporter.on_build_complete(dist, id);
    }

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
    Package(PackageName, IndexUrl, VersionMap),
    /// The returned metadata for a distribution.
    Dist(Dist, Metadata21, Option<Url>),
}

/// In-memory index of package metadata.
#[derive(Default)]
pub(crate) struct Index {
    /// A map from package name to the metadata for that package and the index where the metadata
    /// came from.
    pub(crate) packages: OnceMap<PackageName, (IndexUrl, VersionMap)>,

    /// A map from distribution SHA to metadata for that distribution.
    pub(crate) distributions: OnceMap<String, Metadata21>,

    /// A map from source URL to precise URL.
    pub(crate) redirects: OnceMap<Url, Url>,
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
    /// Package dependencies are not usable
    Unusable(Option<String>),
    /// Container for all available package versions.
    Known(DependencyConstraints<PubGrubPackage, Range<PubGrubVersion>>),
}

#[derive(Debug)]
enum PubGrubDistribution<'a> {
    Registry(&'a PackageName, &'a PubGrubVersion),
    Url(&'a PackageName, &'a Url),
}

impl<'a> PubGrubDistribution<'a> {
    fn from_registry(name: &'a PackageName, version: &'a PubGrubVersion) -> Self {
        Self::Registry(name, version)
    }

    fn from_url(name: &'a PackageName, url: &'a Url) -> Self {
        Self::Url(name, url)
    }
}

impl Metadata for PubGrubDistribution<'_> {
    fn name(&self) -> &PackageName {
        match self {
            Self::Registry(name, _) => name,
            Self::Url(name, _) => name,
        }
    }

    fn version_or_url(&self) -> VersionOrUrl {
        match self {
            Self::Registry(_, version) => VersionOrUrl::Version((*version).into()),
            Self::Url(_, url) => VersionOrUrl::Url(url),
        }
    }
}
