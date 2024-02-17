//! Given a set of requirements, find a set of compatible packages.

use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

use anyhow::Result;
use dashmap::{DashMap, DashSet};
use futures::{FutureExt, StreamExt};
use itertools::Itertools;
use pubgrub::error::PubGrubError;
use pubgrub::range::Range;
use pubgrub::solver::{Incompatibility, State};
use pubgrub::type_aliases::DependencyConstraints;
use rustc_hash::{FxHashMap, FxHashSet};

use tokio::select;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, info_span, instrument, trace, warn, Instrument};
use url::Url;

use crate::candidate_selector::{CandidateDist, CandidateSelector};
use crate::error::ResolveError;
use crate::manifest::Manifest;
use crate::overrides::Overrides;
use crate::pins::FilePins;
use crate::pubgrub::{
    PubGrubDependencies, PubGrubDistribution, PubGrubPackage, PubGrubPriorities, PubGrubPython,
    PubGrubSpecifier,
};
use crate::python_requirement::PythonRequirement;
use crate::resolution::ResolutionGraph;
use crate::resolver::allowed_urls::AllowedUrls;
pub use crate::resolver::index::InMemoryIndex;
use crate::resolver::provider::DefaultResolverProvider;
pub use crate::resolver::provider::ResolverProvider;
pub(crate) use crate::resolver::provider::VersionsResponse;
use crate::resolver::reporter::Facade;
pub use crate::resolver::reporter::{BuildId, Reporter};
use crate::yanks::AllowedYanks;
use crate::{DependencyMode, Options};
use distribution_filename::WheelFilename;
use distribution_types::{
    BuiltDist, Dist, DistributionMetadata, IncompatibleWheel, LocalEditable, Name, RemoteSource,
    SourceDist, VersionOrUrl,
};
use pep440_rs::{Version, VersionSpecifiers, MIN_VERSION};
use pep508_rs::{MarkerEnvironment, Requirement};
use platform_tags::{IncompatibleTag, Tags};
use pypi_types::{Metadata21, Yanked};
use uv_client::{FlatIndex, RegistryClient};
use uv_distribution::DistributionDatabase;
use uv_interpreter::Interpreter;
use uv_normalize::PackageName;
use uv_traits::BuildContext;

mod allowed_urls;
mod index;
mod provider;
mod reporter;

/// The package version is unavailable and cannot be used
/// Unlike [`PackageUnavailable`] this applies to a single version of the package
#[derive(Debug, Clone)]
pub(crate) enum UnavailableVersion {
    /// Version is incompatible due to the `Requires-Python` version specifiers for that package.
    RequiresPython(VersionSpecifiers),
    /// Version is incompatible because it is yanked
    Yanked(Yanked),
    /// Version is incompatible because it has no usable distributions
    NoDistributions(Option<IncompatibleWheel>),
}

/// The package is unavailable and cannot be used
#[derive(Debug, Clone)]
pub(crate) enum UnavailablePackage {
    /// Index lookups were disabled (i.e., `--no-index`) and the package was not found in a flat index (i.e. from `--find-links`)
    NoIndex,
    /// Network requests were disabled (i.e., `--offline`), and the package was not found in the cache.
    Offline,
    /// The package was not found in the registry
    NotFound,
}

enum ResolverVersion {
    /// A usable version
    Available(Version),
    /// A version that is not usable for some reaosn
    Unavailable(Version, UnavailableVersion),
}

pub struct Resolver<'a, Provider: ResolverProvider> {
    project: Option<PackageName>,
    requirements: Vec<Requirement>,
    constraints: Vec<Requirement>,
    overrides: Overrides,
    allowed_yanks: AllowedYanks,
    allowed_urls: AllowedUrls,
    dependency_mode: DependencyMode,
    markers: &'a MarkerEnvironment,
    python_requirement: PythonRequirement,
    selector: CandidateSelector,
    index: &'a InMemoryIndex,
    /// Incompatibilities for packages that are entirely unavailable
    unavailable_packages: DashMap<PackageName, UnavailablePackage>,
    /// The set of all registry-based packages visited during resolution.
    visited: DashSet<PackageName>,
    editables: FxHashMap<PackageName, (LocalEditable, Metadata21)>,
    reporter: Option<Arc<dyn Reporter>>,
    provider: Provider,
}

impl<'a, Context: BuildContext + Send + Sync> Resolver<'a, DefaultResolverProvider<'a, Context>> {
    /// Initialize a new resolver using the default backend doing real requests.
    ///
    /// Reads the flat index entries.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        manifest: Manifest,
        options: Options,
        markers: &'a MarkerEnvironment,
        interpreter: &'a Interpreter,
        tags: &'a Tags,
        client: &'a RegistryClient,
        flat_index: &'a FlatIndex,
        index: &'a InMemoryIndex,
        build_context: &'a Context,
    ) -> Self {
        let provider = DefaultResolverProvider::new(
            client,
            DistributionDatabase::new(build_context.cache(), tags, client, build_context),
            flat_index,
            tags,
            PythonRequirement::new(interpreter, markers),
            options.exclude_newer,
            build_context.no_binary(),
        );
        Self::new_custom_io(
            manifest,
            options,
            markers,
            PythonRequirement::new(interpreter, markers),
            index,
            provider,
        )
    }
}

impl<'a, Provider: ResolverProvider> Resolver<'a, Provider> {
    /// Initialize a new resolver using a user provided backend.
    pub fn new_custom_io(
        manifest: Manifest,
        options: Options,
        markers: &'a MarkerEnvironment,
        python_requirement: PythonRequirement,
        index: &'a InMemoryIndex,
        provider: Provider,
    ) -> Self {
        let selector = CandidateSelector::for_resolution(&manifest, options);

        // Determine all the editable requirements.
        let mut editables = FxHashMap::default();
        for (editable_requirement, metadata) in &manifest.editables {
            editables.insert(
                metadata.name.clone(),
                (editable_requirement.clone(), metadata.clone()),
            );
        }

        // Determine the list of allowed URLs.
        let allowed_urls: AllowedUrls = manifest
            .requirements
            .iter()
            .chain(manifest.constraints.iter())
            .chain(manifest.overrides.iter())
            .filter_map(|req| {
                if let Some(pep508_rs::VersionOrUrl::Url(url)) = &req.version_or_url {
                    Some(url.raw())
                } else {
                    None
                }
            })
            .chain(
                manifest
                    .editables
                    .iter()
                    .map(|(editable, _)| editable.raw()),
            )
            .collect();

        // Determine the allowed yanked package versions
        let allowed_yanks = manifest
            .requirements
            .iter()
            .chain(manifest.constraints.iter())
            .collect();

        Self {
            index,
            unavailable_packages: DashMap::default(),
            visited: DashSet::default(),
            selector,
            allowed_urls,
            allowed_yanks,
            dependency_mode: options.dependency_mode,
            project: manifest.project,
            requirements: manifest.requirements,
            constraints: manifest.constraints,
            overrides: Overrides::from_requirements(manifest.overrides),
            markers,
            python_requirement,
            editables,
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
    pub async fn resolve(self) -> Result<ResolutionGraph, ResolveError> {
        // A channel to fetch package metadata (e.g., given `flask`, fetch all versions) and version
        // metadata (e.g., given `flask==1.0.0`, fetch the metadata for that version).
        // Channel size is set to the same size as the task buffer for simplicity.
        let (request_sink, request_stream) = tokio::sync::mpsc::channel(50);

        // Run the fetcher.
        let requests_fut = self.fetch(request_stream).fuse();

        // Run the solver.
        let resolve_fut = self.solve(&request_sink).fuse();

        let resolution = select! {
            result = requests_fut => {
                result?;
                return Err(ResolveError::ChannelClosed);
            }
            resolution = resolve_fut => {
                resolution.map_err(|err| {
                    // Add version information to improve unsat error messages.
                    if let ResolveError::NoSolution(err) = err {
                        ResolveError::NoSolution(
                            err
                            .with_available_versions(&self.python_requirement, &self.visited, &self.index.packages)
                            .with_selector(self.selector.clone())
                            .with_python_requirement(&self.python_requirement)
                            .with_index_locations(self.provider.index_locations())
                            .with_unavailable_packages(&self.unavailable_packages)
                        )
                    } else {
                        err
                    }
                })?
            }
        };

        self.on_complete();

        Ok(resolution)
    }

    /// Run the `PubGrub` solver.
    #[instrument(skip_all)]
    async fn solve(
        &self,
        request_sink: &tokio::sync::mpsc::Sender<Request>,
    ) -> Result<ResolutionGraph, ResolveError> {
        let root = PubGrubPackage::Root(self.project.clone());

        // Keep track of the packages for which we've requested metadata.
        let mut pins = FilePins::default();
        let mut priorities = PubGrubPriorities::default();

        // Start the solve.
        let mut state = State::init(root.clone(), MIN_VERSION.clone());
        let mut added_dependencies: FxHashMap<PubGrubPackage, FxHashSet<Version>> =
            FxHashMap::default();
        let mut next = root;

        debug!(
            "Solving with target Python version {}",
            self.python_requirement.target()
        );

        loop {
            // Run unit propagation.
            state.unit_propagation(next)?;

            // Pre-visit all candidate packages, to allow metadata to be fetched in parallel.
            Self::pre_visit(state.partial_solution.prioritized_packages(), request_sink).await?;

            // Choose a package version.
            let Some(highest_priority_pkg) =
                state
                    .partial_solution
                    .pick_highest_priority_pkg(|package, _range| {
                        priorities.get(package).unwrap_or_default()
                    })
            else {
                let selection = state.partial_solution.extract_solution();
                return ResolutionGraph::from_state(
                    &selection,
                    &pins,
                    &self.index.packages,
                    &self.index.distributions,
                    &self.index.redirects,
                    &state,
                    self.editables.clone(),
                );
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
                    debug!("No compatible version found for: {next}");

                    let term_intersection = state
                        .partial_solution
                        .term_intersection_for_package(&next)
                        .expect("a package was chosen but we don't have a term.");

                    let reason = {
                        if let PubGrubPackage::Package(ref package_name, _, _) = next {
                            // Check if the decision was due to the package being unavailable
                            self.unavailable_packages
                                .get(package_name)
                                .map(|entry| match *entry {
                                    UnavailablePackage::NoIndex => {
                                        "was not found in the provided package locations"
                                    }
                                    UnavailablePackage::Offline => "was not found in the cache",
                                    UnavailablePackage::NotFound => {
                                        "was not found in the package registry"
                                    }
                                })
                        } else {
                            None
                        }
                    };

                    let inc = Incompatibility::no_versions(
                        next.clone(),
                        term_intersection.clone(),
                        reason.map(ToString::to_string),
                    );

                    state.add_incompatibility(inc);
                    continue;
                }
                Some(version) => version,
            };
            let version = match version {
                ResolverVersion::Available(version) => version,
                ResolverVersion::Unavailable(version, unavailable) => {
                    let reason = match unavailable {
                        UnavailableVersion::RequiresPython(requires_python) => {
                            // Incompatible requires-python versions are special in that we track
                            // them as incompatible dependencies instead of marking the package version
                            // as unavailable directly
                            let python_version = requires_python
                                .iter()
                                .map(PubGrubSpecifier::try_from)
                                .fold_ok(Range::full(), |range, specifier| {
                                    range.intersection(&specifier.into())
                                })?;

                            let package = &next;
                            for kind in [PubGrubPython::Installed, PubGrubPython::Target] {
                                state.add_incompatibility(Incompatibility::from_dependency(
                                    package.clone(),
                                    Range::singleton(version.clone()),
                                    (&PubGrubPackage::Python(kind), &python_version),
                                ));
                            }
                            state.partial_solution.add_decision(next.clone(), version);
                            continue;
                        }
                        UnavailableVersion::Yanked(yanked) => match yanked {
                            Yanked::Bool(_) => "it was yanked".to_string(),
                            Yanked::Reason(reason) => format!(
                                "it was yanked (reason: {})",
                                reason.trim().trim_end_matches('.')
                            ),
                        },
                        UnavailableVersion::NoDistributions(best_incompatible) => {
                            if let Some(best_incompatible) = best_incompatible {
                                match best_incompatible {
                                    IncompatibleWheel::NoBinary => "no source distribution is available and using wheels is disabled".to_string(),
                                    IncompatibleWheel::RequiresPython => "no wheels are available that meet your required Python version".to_string(),
                                    IncompatibleWheel::Tag(tag) => {
                                        match tag {
                                            IncompatibleTag::Invalid => "no wheels are available with valid tags".to_string(),
                                            IncompatibleTag::Python => "no wheels are available with a matching Python implementation".to_string(),
                                            IncompatibleTag::Abi => "no wheels are available with a matching Python ABI".to_string(),
                                            IncompatibleTag::Platform => "no wheels are available with a matching platform".to_string(),
                                        }
                                    }
                                }
                            } else {
                                // TODO(zanieb): It's unclear why we would encounter this case still
                                "no wheels are available for your system".to_string()
                            }
                        }
                    };
                    state.add_incompatibility(Incompatibility::unavailable(
                        next.clone(),
                        version.clone(),
                        reason,
                    ));
                    continue;
                }
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
                    .get_dependencies(package, &version, &mut priorities, request_sink)
                    .await?
                {
                    Dependencies::Unavailable(reason) => {
                        let message = {
                            if matches!(package, PubGrubPackage::Root(_)) {
                                // Including front-matter for the root package is redundant
                                reason.clone()
                            } else {
                                format!("its dependencies are unusable because {reason}")
                            }
                        };
                        state.add_incompatibility(Incompatibility::unavailable(
                            package.clone(),
                            version.clone(),
                            message,
                        ));
                        continue;
                    }
                    Dependencies::Available(constraints) if constraints.contains_key(package) => {
                        return Err(PubGrubError::SelfDependency {
                            package: package.clone(),
                            version: version.clone(),
                        }
                        .into());
                    }
                    Dependencies::Available(constraints) => constraints,
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
        &self,
        package: &PubGrubPackage,
        priorities: &mut PubGrubPriorities,
        request_sink: &tokio::sync::mpsc::Sender<Request>,
    ) -> Result<(), ResolveError> {
        match package {
            PubGrubPackage::Root(_) => {}
            PubGrubPackage::Python(_) => {}
            PubGrubPackage::Package(package_name, _extra, None) => {
                // Emit a request to fetch the metadata for this package.
                if self.index.packages.register(package_name.clone()) {
                    priorities.add(package_name.clone());
                    request_sink
                        .send(Request::Package(package_name.clone()))
                        .await?;
                }
            }
            PubGrubPackage::Package(package_name, _extra, Some(url)) => {
                // Emit a request to fetch the metadata for this distribution.
                let dist = Dist::from_url(package_name.clone(), url.clone())?;
                if self.index.distributions.register(dist.package_id()) {
                    priorities.add(dist.name().clone());
                    request_sink.send(Request::Dist(dist)).await?;
                }
            }
        }
        Ok(())
    }

    /// Visit the set of [`PubGrubPackage`] candidates prior to selection. This allows us to fetch
    /// metadata for all of the packages in parallel.
    async fn pre_visit<'data>(
        packages: impl Iterator<Item = (&'data PubGrubPackage, &'data Range<Version>)>,
        request_sink: &tokio::sync::mpsc::Sender<Request>,
    ) -> Result<(), ResolveError> {
        // Iterate over the potential packages, and fetch file metadata for any of them. These
        // represent our current best guesses for the versions that we _might_ select.
        for (package, range) in packages {
            let PubGrubPackage::Package(package_name, _extra, None) = package else {
                continue;
            };
            request_sink
                .send(Request::Prefetch(package_name.clone(), range.clone()))
                .await?;
        }
        Ok(())
    }

    /// Given a set of candidate packages, choose the next package (and version) to add to the
    /// partial solution.
    ///
    /// Returns [None] when there are no versions in the given range.
    #[instrument(skip_all, fields(%package))]
    async fn choose_version(
        &self,
        package: &PubGrubPackage,
        range: &Range<Version>,
        pins: &mut FilePins,
        request_sink: &tokio::sync::mpsc::Sender<Request>,
    ) -> Result<Option<ResolverVersion>, ResolveError> {
        match package {
            PubGrubPackage::Root(_) => Ok(Some(ResolverVersion::Available(MIN_VERSION.clone()))),

            PubGrubPackage::Python(PubGrubPython::Installed) => {
                let version = self.python_requirement.installed();
                if range.contains(version) {
                    Ok(Some(ResolverVersion::Available(version.clone())))
                } else {
                    Ok(None)
                }
            }

            PubGrubPackage::Python(PubGrubPython::Target) => {
                let version = self.python_requirement.target();
                if range.contains(version) {
                    Ok(Some(ResolverVersion::Available(version.clone())))
                } else {
                    Ok(None)
                }
            }

            PubGrubPackage::Package(package_name, extra, Some(url)) => {
                if let Some(extra) = extra {
                    debug!(
                        "Searching for a compatible version of {package_name}[{extra}] @ {url} ({range})",
                    );
                } else {
                    debug!(
                        "Searching for a compatible version of {package_name} @ {url} ({range})"
                    );
                }

                // If the URL wasn't declared in the direct dependencies or constraints, reject it.
                if !self.allowed_urls.contains(url) {
                    return Err(ResolveError::DisallowedUrl(
                        package_name.clone(),
                        url.to_url(),
                    ));
                }

                if let Ok(wheel_filename) = WheelFilename::try_from(url.raw()) {
                    // If the URL is that of a wheel, extract the version.
                    let version = wheel_filename.version;
                    if range.contains(&version) {
                        Ok(Some(ResolverVersion::Available(version)))
                    } else {
                        Ok(None)
                    }
                } else {
                    // Otherwise, assume this is a source distribution.
                    let dist = PubGrubDistribution::from_url(package_name, url);
                    let metadata = self
                        .index
                        .distributions
                        .wait(&dist.package_id())
                        .await
                        .ok_or(ResolveError::Unregistered)?;
                    let version = &metadata.version;
                    if range.contains(version) {
                        Ok(Some(ResolverVersion::Available(version.clone())))
                    } else {
                        Ok(None)
                    }
                }
            }

            PubGrubPackage::Package(package_name, extra, None) => {
                // If the dist is an editable, return the version from the editable metadata.
                if let Some((_local, metadata)) = self.editables.get(package_name) {
                    let version = metadata.version.clone();
                    return if range.contains(&version) {
                        Ok(Some(ResolverVersion::Available(version)))
                    } else {
                        Ok(None)
                    };
                }

                // Wait for the metadata to be available.
                let versions_response = self
                    .index
                    .packages
                    .wait(package_name)
                    .instrument(info_span!("package_wait", %package_name))
                    .await
                    .ok_or(ResolveError::Unregistered)?;
                self.visited.insert(package_name.clone());

                let version_map = match *versions_response {
                    VersionsResponse::Found(ref version_map) => version_map,
                    // Short-circuit if we do not find any versions for the package
                    VersionsResponse::NoIndex => {
                        self.unavailable_packages
                            .insert(package_name.clone(), UnavailablePackage::NoIndex);

                        return Ok(None);
                    }
                    VersionsResponse::Offline => {
                        self.unavailable_packages
                            .insert(package_name.clone(), UnavailablePackage::Offline);

                        return Ok(None);
                    }
                    VersionsResponse::NotFound => {
                        self.unavailable_packages
                            .insert(package_name.clone(), UnavailablePackage::NotFound);

                        return Ok(None);
                    }
                };

                if let Some(extra) = extra {
                    debug!(
                        "Searching for a compatible version of {package_name}[{extra}] ({range})",
                    );
                } else {
                    debug!("Searching for a compatible version of {package_name} ({range})");
                }

                // Find a version.
                let Some(candidate) = self.selector.select(package_name, range, version_map) else {
                    // Short circuit: we couldn't find _any_ versions for a package.
                    return Ok(None);
                };

                let dist = match candidate.dist() {
                    CandidateDist::Compatible(dist) => dist,
                    CandidateDist::ExcludeNewer => {
                        // If the version is incomatible because of `exclude_newer`, pretend the versions do not exist
                        return Ok(None);
                    }
                    CandidateDist::Incompatible(incompatibility) => {
                        // If the version is incompatible because no distributions match, exit early.
                        return Ok(Some(ResolverVersion::Unavailable(
                            candidate.version().clone(),
                            UnavailableVersion::NoDistributions(incompatibility.cloned()),
                        )));
                    }
                };

                // If the version is incompatible because it was yanked, exit early.
                if dist.yanked().is_yanked() {
                    if self
                        .allowed_yanks
                        .allowed(package_name, candidate.version())
                    {
                        warn!("Allowing yanked version: {}", candidate.package_id());
                    } else {
                        return Ok(Some(ResolverVersion::Unavailable(
                            candidate.version().clone(),
                            UnavailableVersion::Yanked(dist.yanked().clone()),
                        )));
                    }
                }

                // If the version is incompatible because of its Python requirement
                if let Some(requires_python) = self.python_requirement.validate_dist(dist) {
                    return Ok(Some(ResolverVersion::Unavailable(
                        candidate.version().clone(),
                        UnavailableVersion::RequiresPython(requires_python.clone()),
                    )));
                }

                if let Some(extra) = extra {
                    debug!(
                        "Selecting: {}[{}]=={} ({})",
                        candidate.name(),
                        extra,
                        candidate.version(),
                        dist.for_resolution()
                            .dist
                            .filename()
                            .unwrap_or(Cow::Borrowed("unknown filename"))
                    );
                } else {
                    debug!(
                        "Selecting: {}=={} ({})",
                        candidate.name(),
                        candidate.version(),
                        dist.for_resolution()
                            .dist
                            .filename()
                            .unwrap_or(Cow::Borrowed("unknown filename"))
                    );
                }

                // We want to return a package pinned to a specific version; but we _also_ want to
                // store the exact file that we selected to satisfy that version.
                pins.insert(&candidate, dist);

                let version = candidate.version().clone();

                // Emit a request to fetch the metadata for this version.
                if self.index.distributions.register(candidate.package_id()) {
                    let dist = dist.for_resolution().dist.clone();
                    request_sink.send(Request::Dist(dist)).await?;
                }

                Ok(Some(ResolverVersion::Available(version)))
            }
        }
    }

    /// Given a candidate package and version, return its dependencies.
    #[instrument(skip_all, fields(%package, %version))]
    async fn get_dependencies(
        &self,
        package: &PubGrubPackage,
        version: &Version,
        priorities: &mut PubGrubPriorities,
        request_sink: &tokio::sync::mpsc::Sender<Request>,
    ) -> Result<Dependencies, ResolveError> {
        match package {
            PubGrubPackage::Root(_) => {
                // Add the root requirements.
                let constraints = PubGrubDependencies::from_requirements(
                    &self.requirements,
                    &self.constraints,
                    &self.overrides,
                    None,
                    None,
                    self.markers,
                );
                if let Err(
                    err @ (ResolveError::ConflictingVersions(..)
                    | ResolveError::ConflictingUrls(..)),
                ) = constraints
                {
                    return Ok(Dependencies::Unavailable(uncapitalize(err.to_string())));
                }
                let mut constraints = constraints?;

                for (package, version) in constraints.iter() {
                    debug!("Adding direct dependency: {package}{version}");

                    // Emit a request to fetch the metadata for this package.
                    self.visit_package(package, priorities, request_sink)
                        .await?;
                }

                // Add a dependency on each editable.
                for (editable, metadata) in self.editables.values() {
                    constraints.insert(
                        PubGrubPackage::Package(metadata.name.clone(), None, None),
                        Range::singleton(metadata.version.clone()),
                    );
                    for extra in &editable.extras {
                        constraints.insert(
                            PubGrubPackage::Package(
                                metadata.name.clone(),
                                Some(extra.clone()),
                                None,
                            ),
                            Range::singleton(metadata.version.clone()),
                        );
                    }
                }

                Ok(Dependencies::Available(constraints.into()))
            }

            PubGrubPackage::Python(_) => {
                Ok(Dependencies::Available(DependencyConstraints::default()))
            }

            PubGrubPackage::Package(package_name, extra, url) => {
                // If we're excluding transitive dependencies, short-circuit.
                if self.dependency_mode.is_direct() {
                    return Ok(Dependencies::Available(DependencyConstraints::default()));
                }

                // Determine if the distribution is editable.
                if let Some((_local, metadata)) = self.editables.get(package_name) {
                    let mut constraints = PubGrubDependencies::from_requirements(
                        &metadata.requires_dist,
                        &self.constraints,
                        &self.overrides,
                        Some(package_name),
                        extra.as_ref(),
                        self.markers,
                    )?;

                    for (package, version) in constraints.iter() {
                        debug!("Adding transitive dependency: {package}{version}");

                        // Emit a request to fetch the metadata for this package.
                        self.visit_package(package, priorities, request_sink)
                            .await?;
                    }

                    // If a package has an extra, insert a constraint on the base package.
                    if extra.is_some() {
                        constraints.insert(
                            PubGrubPackage::Package(package_name.clone(), None, None),
                            Range::singleton(version.clone()),
                        );
                    }

                    return Ok(Dependencies::Available(constraints.into()));
                }

                // Determine the distribution to lookup.
                let dist = match url {
                    Some(url) => PubGrubDistribution::from_url(package_name, url),
                    None => PubGrubDistribution::from_registry(package_name, version),
                };
                let package_id = dist.package_id();

                // If the package does not exist in the registry, we cannot fetch its dependencies
                if self.unavailable_packages.get(package_name).is_some() {
                    debug_assert!(
                        false,
                        "Dependencies were requested for a package that is not available"
                    );
                    return Ok(Dependencies::Unavailable(
                        "The package is unavailable".to_string(),
                    ));
                }

                // Wait for the metadata to be available.
                let metadata = self
                    .index
                    .distributions
                    .wait(&package_id)
                    .instrument(info_span!("distributions_wait", %package_id))
                    .await
                    .ok_or(ResolveError::Unregistered)?;

                let mut constraints = PubGrubDependencies::from_requirements(
                    &metadata.requires_dist,
                    &self.constraints,
                    &self.overrides,
                    Some(package_name),
                    extra.as_ref(),
                    self.markers,
                )?;

                for (package, version) in constraints.iter() {
                    debug!("Adding transitive dependency: {package}{version}");

                    // Emit a request to fetch the metadata for this package.
                    self.visit_package(package, priorities, request_sink)
                        .await?;
                }

                // If a package has an extra, insert a constraint on the base package.
                if extra.is_some() {
                    constraints.insert(
                        PubGrubPackage::Package(package_name.clone(), None, None),
                        Range::singleton(version.clone()),
                    );
                }

                Ok(Dependencies::Available(constraints.into()))
            }
        }
    }

    /// Fetch the metadata for a stream of packages and versions.
    async fn fetch(
        &self,
        request_stream: tokio::sync::mpsc::Receiver<Request>,
    ) -> Result<(), ResolveError> {
        let mut response_stream = ReceiverStream::new(request_stream)
            .map(|request| self.process_request(request).boxed())
            .buffer_unordered(50);

        while let Some(response) = response_stream.next().await {
            match response? {
                Some(Response::Package(package_name, version_map)) => {
                    trace!("Received package metadata for: {package_name}");
                    self.index.packages.done(package_name, version_map);
                }
                Some(Response::Dist {
                    dist: Dist::Built(dist),
                    metadata,
                    precise: _,
                }) => {
                    trace!("Received built distribution metadata for: {dist}");
                    self.index.distributions.done(dist.package_id(), metadata);
                }
                Some(Response::Dist {
                    dist: Dist::Source(distribution),
                    metadata,
                    precise,
                }) => {
                    trace!("Received source distribution metadata for: {distribution}");
                    self.index
                        .distributions
                        .done(distribution.package_id(), metadata);
                    if let Some(precise) = precise {
                        match distribution {
                            SourceDist::DirectUrl(sdist) => {
                                self.index.redirects.insert(sdist.url.to_url(), precise);
                            }
                            SourceDist::Git(sdist) => {
                                self.index.redirects.insert(sdist.url.to_url(), precise);
                            }
                            SourceDist::Path(sdist) => {
                                self.index.redirects.insert(sdist.url.to_url(), precise);
                            }
                            SourceDist::Registry(_) => {}
                        }
                    }
                }
                None => {}
            }
        }

        Ok::<(), ResolveError>(())
    }

    #[instrument(skip_all, fields(%request))]
    async fn process_request(&self, request: Request) -> Result<Option<Response>, ResolveError> {
        match request {
            // Fetch package metadata from the registry.
            Request::Package(package_name) => {
                let package_versions = self
                    .provider
                    .get_package_versions(&package_name)
                    .boxed()
                    .await
                    .map_err(ResolveError::Client)?;

                Ok(Some(Response::Package(package_name, package_versions)))
            }

            // Fetch distribution metadata from the distribution database.
            Request::Dist(dist) => {
                let (metadata, precise) = self
                    .provider
                    .get_or_build_wheel_metadata(&dist)
                    .boxed()
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
                Ok(Some(Response::Dist {
                    dist,
                    metadata,
                    precise,
                }))
            }

            // Pre-fetch the package and distribution metadata.
            Request::Prefetch(package_name, range) => {
                // Ignore editables.
                if self.editables.contains_key(&package_name) {
                    return Ok(None);
                }

                // Wait for the package metadata to become available.
                let versions_response = self
                    .index
                    .packages
                    .wait(&package_name)
                    .await
                    .ok_or(ResolveError::Unregistered)?;

                let version_map = match *versions_response {
                    VersionsResponse::Found(ref version_map) => version_map,
                    // Short-circuit if we did not find any versions for the package
                    VersionsResponse::NoIndex => {
                        self.unavailable_packages
                            .insert(package_name.clone(), UnavailablePackage::NoIndex);

                        return Ok(None);
                    }
                    VersionsResponse::Offline => {
                        self.unavailable_packages
                            .insert(package_name.clone(), UnavailablePackage::Offline);

                        return Ok(None);
                    }
                    VersionsResponse::NotFound => {
                        self.unavailable_packages
                            .insert(package_name.clone(), UnavailablePackage::NotFound);

                        return Ok(None);
                    }
                };

                // Try to find a compatible version. If there aren't any compatible versions,
                // short-circuit and return `None`.
                let Some(candidate) = self.selector.select(&package_name, &range, version_map)
                else {
                    return Ok(None);
                };

                // If there is not a compatible distribution, short-circuit.
                let Some(dist) = candidate.compatible() else {
                    return Ok(None);
                };

                // If the Python version is incompatible, short-circuit.
                if self.python_requirement.validate_dist(dist).is_some() {
                    return Ok(None);
                }

                // Emit a request to fetch the metadata for this version.
                if self.index.distributions.register(candidate.package_id()) {
                    let dist = dist.for_resolution().dist.clone();

                    let (metadata, precise) = self
                        .provider
                        .get_or_build_wheel_metadata(&dist)
                        .boxed()
                        .await
                        .map_err(|err| match dist.clone() {
                            Dist::Built(BuiltDist::Path(built_dist)) => {
                                ResolveError::Read(Box::new(built_dist), err)
                            }
                            Dist::Source(SourceDist::Path(source_dist)) => {
                                ResolveError::Build(Box::new(source_dist), err)
                            }
                            Dist::Built(built_dist) => {
                                ResolveError::Fetch(Box::new(built_dist), err)
                            }
                            Dist::Source(source_dist) => {
                                ResolveError::FetchAndBuild(Box::new(source_dist), err)
                            }
                        })?;

                    Ok(Some(Response::Dist {
                        dist,
                        metadata,
                        precise,
                    }))
                } else {
                    Ok(None)
                }
            }
        }
    }

    fn on_progress(&self, package: &PubGrubPackage, version: &Version) {
        if let Some(reporter) = self.reporter.as_ref() {
            match package {
                PubGrubPackage::Root(_) => {}
                PubGrubPackage::Python(_) => {}
                PubGrubPackage::Package(package_name, _extra, Some(url)) => {
                    reporter.on_progress(package_name, VersionOrUrl::Url(url));
                }
                PubGrubPackage::Package(package_name, _extra, None) => {
                    reporter.on_progress(package_name, VersionOrUrl::Version(version));
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

/// Fetch the metadata for an item
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum Request {
    /// A request to fetch the metadata for a package.
    Package(PackageName),
    /// A request to fetch the metadata for a built or source distribution.
    Dist(Dist),
    /// A request to pre-fetch the metadata for a package and the best-guess distribution.
    Prefetch(PackageName, Range<Version>),
}

impl Display for Request {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Request::Package(package_name) => {
                write!(f, "Versions {package_name}")
            }
            Request::Dist(dist) => {
                write!(f, "Metadata {dist}")
            }
            Request::Prefetch(package_name, range) => {
                write!(f, "Prefetch {package_name} {range}")
            }
        }
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum Response {
    /// The returned metadata for a package hosted on a registry.
    Package(PackageName, VersionsResponse),
    /// The returned metadata for a distribution.
    Dist {
        dist: Dist,
        metadata: Metadata21,
        precise: Option<Url>,
    },
}

/// An enum used by [`DependencyProvider`] that holds information about package dependencies.
/// For each [Package] there is a set of versions allowed as a dependency.
#[derive(Clone)]
enum Dependencies {
    /// Package dependencies are not available.
    Unavailable(String),
    /// Container for all available package versions.
    Available(DependencyConstraints<PubGrubPackage, Range<Version>>),
}

fn uncapitalize<T: AsRef<str>>(string: T) -> String {
    let mut chars = string.as_ref().chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_lowercase().chain(chars).collect(),
    }
}
