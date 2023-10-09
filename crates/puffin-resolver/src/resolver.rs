use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use anyhow::Result;
use bitflags::bitflags;
use futures::future::Either;
use futures::{StreamExt, TryFutureExt};
use tracing::debug;

use pep440_rs::Version;
use pep508_rs::{MarkerEnvironment, Requirement};
use platform_tags::Tags;
use puffin_client::{File, PypiClient, SimpleJson};
use puffin_package::metadata::Metadata21;
use puffin_package::package_name::PackageName;
use wheel_filename::WheelFilename;

use crate::error::ResolveError;
use crate::resolution::{PinnedPackage, Resolution};

pub struct Resolver<'a> {
    markers: &'a MarkerEnvironment,
    tags: &'a Tags,
    client: &'a PypiClient,

    /// Callback to invoke when a dependency is added to the resolution.
    on_dependency_added: Option<Box<dyn Fn() + Send + Sync>>,

    /// Callback to invoke when a dependency is resolved.
    on_resolve_progress: Option<Box<dyn Fn(PinnedPackage) + Send>>,

    reporter: Option<Box<dyn Reporter>>,
}

pub trait Reporter: Send + Sync {
    fn on_dependency_added(&self);
    fn on_resolve_progress(&self, package: &PinnedPackage);
    fn on_resolve_complete(&self);
}

bitflags! {
    #[derive(Debug, Copy, Clone, Default)]
    pub struct ResolveFlags: u8 {
        /// Don't install package dependencies.
        const NO_DEPS = 1 << 0;
    }
}

impl<'a> Resolver<'a> {
    /// Initialize a new resolver.
    pub fn new(markers: &'a MarkerEnvironment, tags: &'a Tags, client: &'a PypiClient) -> Self {
        Self {
            markers,
            tags,
            client,
            on_dependency_added: None,
            on_resolve_progress: None,
            reporter: None,
        }
    }

    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        Self {
            markers: self.markers,
            tags: self.tags,
            client: self.client,
            on_dependency_added: None,
            on_resolve_progress: None,
            reporter: Some(Box::new(reporter)),
        }
    }

    /// Register a callback to invoke when a dependency is added to the resolution.
    pub fn on_dependency_added<F>(&mut self, callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_dependency_added = Some(Box::new(callback));
    }

    /// Register a callback to invoke when a dependency is resolved.
    pub fn on_resolve_progress<F>(&mut self, callback: F)
    where
        F: Fn(PinnedPackage) + Send + 'static,
    {
        self.on_resolve_progress = Some(Box::new(callback));
    }

    /// Resolve a set of requirements into a set of pinned versions.
    pub async fn resolve(
        &self,
        requirements: impl Iterator<Item = &Requirement>,
        flags: ResolveFlags,
    ) -> Result<Resolution, ResolveError> {
        // A channel to fetch package metadata (e.g., given `flask`, fetch all versions) and version
        // metadata (e.g., given `flask==1.0.0`, fetch the metadata for that version).
        let (package_sink, package_stream) = futures::channel::mpsc::unbounded();

        // Initialize the package stream.
        let mut package_stream = package_stream
            .map(|request: Request| match request {
                Request::Package(requirement) => Either::Left(
                    self.client
                        // TODO(charlie): Remove this clone.
                        .simple(requirement.name.clone())
                        .map_ok(move |metadata| Response::Package(requirement, metadata)),
                ),
                Request::Version(requirement, file) => Either::Right(
                    self.client
                        // TODO(charlie): Remove this clone.
                        .file(file.clone())
                        .map_ok(move |metadata| Response::Version(requirement, file, metadata)),
                ),
            })
            .buffer_unordered(32)
            .ready_chunks(32);

        // Push all the requirements into the package sink.
        let mut in_flight: HashSet<PackageName> = HashSet::new();
        for requirement in requirements {
            debug!("Adding root dependency: {}", requirement);
            package_sink.unbounded_send(Request::Package(requirement.clone()))?;
            in_flight.insert(PackageName::normalize(&requirement.name));

            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_dependency_added();
            }
        }

        if in_flight.is_empty() {
            return Ok(Resolution::default());
        }

        // Resolve the requirements.
        let mut resolution: HashMap<PackageName, PinnedPackage> =
            HashMap::with_capacity(in_flight.len());

        while let Some(chunk) = package_stream.next().await {
            for result in chunk {
                let result: Response = result?;
                match result {
                    Response::Package(requirement, metadata) => {
                        // Pick a version that satisfies the requirement.
                        let Some(file) = metadata.files.iter().rev().find(|file| {
                            // We only support wheels for now.
                            let Ok(name) = WheelFilename::from_str(file.filename.as_str()) else {
                                return false;
                            };

                            let Ok(version) = Version::from_str(&name.version) else {
                                return false;
                            };

                            if !name.is_compatible(self.tags) {
                                return false;
                            }

                            requirement.is_satisfied_by(&version)
                        }) else {
                            return Err(ResolveError::NotFound(requirement));
                        };

                        package_sink.unbounded_send(Request::Version(requirement, file.clone()))?;
                    }
                    Response::Version(requirement, file, metadata) => {
                        debug!(
                            "Selecting: {}=={} ({})",
                            metadata.name, metadata.version, file.filename
                        );

                        let package = PinnedPackage {
                            metadata: metadata.clone(),
                            file,
                        };

                        if let Some(reporter) = self.reporter.as_ref() {
                            reporter.on_resolve_progress(&package);
                        }

                        // Add to the resolved set.
                        let normalized_name = PackageName::normalize(&requirement.name);
                        in_flight.remove(&normalized_name);
                        resolution.insert(normalized_name, package);

                        if !flags.intersects(ResolveFlags::NO_DEPS) {
                            // Enqueue its dependencies.
                            for dependency in metadata.requires_dist {
                                if !dependency.evaluate_markers(
                                    self.markers,
                                    requirement.extras.as_ref().map_or(&[], Vec::as_slice),
                                ) {
                                    debug!("Ignoring {dependency} due to environment mismatch");
                                    continue;
                                }

                                let normalized_name = PackageName::normalize(&dependency.name);

                                if resolution.contains_key(&normalized_name) {
                                    continue;
                                }

                                if !in_flight.insert(normalized_name) {
                                    continue;
                                }

                                debug!("Adding transitive dependency: {}", dependency);

                                package_sink.unbounded_send(Request::Package(dependency))?;

                                if let Some(reporter) = self.reporter.as_ref() {
                                    reporter.on_dependency_added();
                                }
                            }
                        };
                    }
                }
            }

            if in_flight.is_empty() {
                break;
            }
        }

        if let Some(reporter) = self.reporter.as_ref() {
            reporter.on_resolve_complete();
        }

        Ok(Resolution::new(resolution))
    }
}

#[derive(Debug)]
enum Request {
    /// A request to fetch the metadata for a package.
    Package(Requirement),
    /// A request to fetch the metadata for a specific version of a package.
    Version(Requirement, File),
}

#[derive(Debug)]
enum Response {
    /// The returned metadata for a package.
    Package(Requirement, SimpleJson),
    /// The returned metadata for a specific version of a package.
    Version(Requirement, File, Metadata21),
}
