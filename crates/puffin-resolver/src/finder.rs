//! Given a set of selected packages, find a compatible set of distributions to install.
//!
//! This is similar to running `pip install` with the `--no-deps` flag.

use std::hash::BuildHasherDefault;
use std::str::FromStr;

use anyhow::Result;
use futures::{StreamExt, TryFutureExt};
use fxhash::FxHashMap;

use distribution_filename::{SourceDistFilename, WheelFilename};
use pep508_rs::{Requirement, VersionOrUrl};
use platform_tags::Tags;
use puffin_client::RegistryClient;
use puffin_distribution::Dist;
use puffin_interpreter::InterpreterInfo;
use puffin_normalize::PackageName;
use pypi_types::{File, SimpleJson};

use crate::error::ResolveError;
use crate::resolution::Resolution;

pub struct DistFinder<'a> {
    tags: &'a Tags,
    client: &'a RegistryClient,
    reporter: Option<Box<dyn Reporter>>,
    interpreter_info: &'a InterpreterInfo,
}

impl<'a> DistFinder<'a> {
    /// Initialize a new distribution finder.
    pub fn new(
        tags: &'a Tags,
        client: &'a RegistryClient,
        interpreter_info: &'a InterpreterInfo,
    ) -> Self {
        Self {
            tags,
            client,
            reporter: None,
            interpreter_info,
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

        // A channel to fetch package metadata (e.g., given `flask`, fetch all versions).
        let (package_sink, package_stream) = futures::channel::mpsc::unbounded();

        // Initialize the package stream.
        let mut package_stream = package_stream
            .map(|request: Request| match request {
                Request::Package(requirement) => self
                    .client
                    .simple(requirement.name.clone())
                    .map_ok(move |metadata| Response::Package(requirement, metadata)),
            })
            .buffer_unordered(32)
            .ready_chunks(32);

        // Resolve the requirements.
        let mut resolution: FxHashMap<PackageName, Dist> =
            FxHashMap::with_capacity_and_hasher(requirements.len(), BuildHasherDefault::default());

        // Push all the requirements into the package sink.
        for requirement in requirements {
            match requirement.version_or_url.as_ref() {
                None | Some(VersionOrUrl::VersionSpecifier(_)) => {
                    package_sink.unbounded_send(Request::Package(requirement.clone()))?;
                }
                Some(VersionOrUrl::Url(url)) => {
                    let package_name = requirement.name.clone();
                    let package = Dist::from_url(package_name.clone(), url.clone());
                    resolution.insert(package_name, package);
                }
            }
        }

        // If all the dependencies were already resolved, we're done.
        if resolution.len() == requirements.len() {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_complete();
            }
            return Ok(Resolution::new(resolution));
        }

        // Otherwise, wait for the package stream to complete.
        while let Some(chunk) = package_stream.next().await {
            for result in chunk {
                let result: Response = result?;
                match result {
                    Response::Package(requirement, metadata) => {
                        // Pick a version that satisfies the requirement.
                        let Some(distribution) = self.select(&requirement, metadata.files) else {
                            return Err(ResolveError::NotFound(requirement));
                        };

                        if let Some(reporter) = self.reporter.as_ref() {
                            reporter.on_progress(&distribution);
                        }

                        // Add to the resolved set.
                        let normalized_name = requirement.name.clone();
                        resolution.insert(normalized_name, distribution);
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

    /// select a version that satisfies the requirement, preferring wheels to source distributions.
    fn select(&self, requirement: &Requirement, files: Vec<File>) -> Option<Dist> {
        let mut fallback = None;
        for file in files.into_iter().rev() {
            if let Ok(wheel) = WheelFilename::from_str(file.filename.as_str()) {
                if !wheel.is_compatible(self.tags) {
                    continue;
                }
                if requirement.is_satisfied_by(&wheel.version) {
                    return Some(Dist::from_registry(wheel.name, wheel.version, file));
                }
            } else if let Ok(sdist) =
                SourceDistFilename::parse(file.filename.as_str(), &requirement.name)
            {
                // Only add source dists compatible with the python version
                // TODO(konstin): https://github.com/astral-sh/puffin/issues/406
                if file
                    .requires_python
                    .as_ref()
                    .map_or(true, |requires_python| {
                        requires_python.contains(self.interpreter_info.version())
                    })
                {
                    if requirement.is_satisfied_by(&sdist.version) {
                        fallback = Some(Dist::from_registry(sdist.name, sdist.version, file));
                    }
                }
            }
        }
        fallback
    }
}

#[derive(Debug)]
enum Request {
    /// A request to fetch the metadata for a package.
    Package(Requirement),
}

#[derive(Debug)]
enum Response {
    /// The returned metadata for a package.
    Package(Requirement, SimpleJson),
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a package is resolved to a specific distribution.
    fn on_progress(&self, wheel: &Dist);

    /// Callback to invoke when the resolution is complete.
    fn on_complete(&self);
}
