//! Given a set of selected packages, find a compatible set of distributions to install.
//!
//! This is similar to running `pip install` with the `--no-deps` flag.

use std::hash::BuildHasherDefault;
use std::str::FromStr;

use anyhow::Result;
use futures::{StreamExt, TryFutureExt};
use fxhash::FxHashMap;

use distribution_filename::{SourceDistFilename, WheelFilename};
use distribution_types::Dist;
use pep440_rs::Version;
use pep508_rs::{Requirement, VersionOrUrl};
use platform_tags::{TagPriority, Tags};
use puffin_client::RegistryClient;
use puffin_interpreter::Interpreter;
use puffin_normalize::PackageName;
use pypi_types::{File, IndexUrl, SimpleJson};

use crate::error::ResolveError;
use crate::resolution::Resolution;

pub struct DistFinder<'a> {
    tags: &'a Tags,
    client: &'a RegistryClient,
    reporter: Option<Box<dyn Reporter>>,
    interpreter: &'a Interpreter,
}

impl<'a> DistFinder<'a> {
    /// Initialize a new distribution finder.
    pub fn new(tags: &'a Tags, client: &'a RegistryClient, interpreter: &'a Interpreter) -> Self {
        Self {
            tags,
            client,
            reporter: None,
            interpreter,
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
                    .map_ok(move |(index, metadata)| {
                        Response::Package(requirement, index, metadata)
                    }),
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
                    let package = Dist::from_url(package_name.clone(), url.clone())?;
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
                    Response::Package(requirement, index, metadata) => {
                        // Pick a version that satisfies the requirement.
                        let Some(distribution) = self.select(&requirement, &index, metadata.files)
                        else {
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
    fn select(
        &self,
        requirement: &Requirement,
        index: &IndexUrl,
        files: Vec<File>,
    ) -> Option<Dist> {
        let mut best_version: Option<Version> = None;
        let mut best_wheel: Option<(Dist, TagPriority)> = None;
        let mut best_sdist: Option<Dist> = None;

        for file in files.into_iter().rev() {
            // Only add dists compatible with the python version.
            // This is relevant for source dists which give no other indication of their
            // compatibility and wheels which may be tagged `py3-none-any` but
            // have `requires-python: ">=3.9"`
            // TODO(konstin): https://github.com/astral-sh/puffin/issues/406
            if !file
                .requires_python
                .as_ref()
                .map_or(true, |requires_python| {
                    requires_python.contains(self.interpreter.version())
                })
            {
                continue;
            }

            // Find the most-compatible wheel.
            if let Ok(wheel) = WheelFilename::from_str(file.filename.as_str()) {
                // If we iterated past the first-compatible version, break.
                if best_version
                    .as_ref()
                    .is_some_and(|version| *version != wheel.version)
                {
                    break;
                }

                if requirement.is_satisfied_by(&wheel.version) {
                    best_version = Some(wheel.version.clone());
                    if let Some(priority) = wheel.compatibility(self.tags) {
                        if best_wheel
                            .as_ref()
                            .map_or(true, |(.., existing)| priority > *existing)
                        {
                            best_wheel = Some((
                                Dist::from_registry(wheel.name, wheel.version, file, index.clone()),
                                priority,
                            ));
                        }
                    }
                }
                continue;
            }

            // Find the most-compatible sdist, if no wheel was found.
            if best_wheel.is_none() {
                if let Ok(sdist) =
                    SourceDistFilename::parse(file.filename.as_str(), &requirement.name)
                {
                    // If we iterated past the first-compatible version, break.
                    if best_version
                        .as_ref()
                        .is_some_and(|version| *version != sdist.version)
                    {
                        break;
                    }

                    if requirement.is_satisfied_by(&sdist.version) {
                        best_version = Some(sdist.version.clone());
                        best_sdist = Some(Dist::from_registry(
                            sdist.name,
                            sdist.version,
                            file,
                            index.clone(),
                        ));
                    }
                }
            }
        }

        best_wheel.map_or(best_sdist, |(wheel, ..)| Some(wheel))
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
    Package(Requirement, IndexUrl, SimpleJson),
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a package is resolved to a specific distribution.
    fn on_progress(&self, wheel: &Dist);

    /// Callback to invoke when the resolution is complete.
    fn on_complete(&self);
}
