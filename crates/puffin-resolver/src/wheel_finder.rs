//! Given a set of selected packages, find a compatible set of wheels to install.
//!
//! This is similar to running `pip install` with the `--no-deps` flag.

use std::hash::BuildHasherDefault;
use std::str::FromStr;

use anyhow::Result;
use futures::future::Either;
use futures::{StreamExt, TryFutureExt};
use fxhash::FxHashMap;
use tracing::debug;

use distribution_filename::WheelFilename;
use pep508_rs::Requirement;
use platform_tags::Tags;
use puffin_client::RegistryClient;
use puffin_package::package_name::PackageName;
use puffin_package::pypi_types::{File, Metadata21, SimpleJson};

use crate::error::ResolveError;
use crate::resolution::{PinnedPackage, Resolution};

pub struct WheelFinder<'a> {
    tags: &'a Tags,
    client: &'a RegistryClient,
    reporter: Option<Box<dyn Reporter>>,
}

impl<'a> WheelFinder<'a> {
    /// Initialize a new wheel finder.
    pub fn new(tags: &'a Tags, client: &'a RegistryClient) -> Self {
        Self {
            tags,
            client,
            reporter: None,
        }
    }

    /// Set the [`Reporter`] to use for this resolution.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        Self {
            reporter: Some(Box::new(reporter)),
            ..self
        }
    }

    /// Resolve a set of pinned packages into a set of wheels.
    pub async fn resolve(&self, requirements: &[Requirement]) -> Result<Resolution, ResolveError> {
        if requirements.is_empty() {
            return Ok(Resolution::default());
        }

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
        for requirement in requirements {
            package_sink.unbounded_send(Request::Package(requirement.clone()))?;
        }

        // Resolve the requirements.
        let mut resolution: FxHashMap<PackageName, PinnedPackage> =
            FxHashMap::with_capacity_and_hasher(requirements.len(), BuildHasherDefault::default());

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

                            if !name.is_compatible(self.tags) {
                                return false;
                            }

                            requirement.is_satisfied_by(&name.version)
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

                        let package = PinnedPackage::new(
                            PackageName::normalize(&metadata.name),
                            metadata.version,
                            file,
                        );

                        if let Some(reporter) = self.reporter.as_ref() {
                            reporter.on_progress(&package);
                        }

                        // Add to the resolved set.
                        let normalized_name = PackageName::normalize(&requirement.name);
                        resolution.insert(normalized_name, package);
                    }
                }
            }

            if resolution.len() == requirements.len() {
                break;
            }
        }

        if let Some(reporter) = self.reporter.as_ref() {
            reporter.on_complete();
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

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a package is resolved to a wheel.
    fn on_progress(&self, package: &PinnedPackage);

    /// Callback to invoke when the resolution is complete.
    fn on_complete(&self);
}
