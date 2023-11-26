//! Avoid cyclic crate dependencies between [resolver][`puffin_resolver`],
//! [installer][`puffin_installer`] and [build][`puffin_build`] through [`BuildDispatch`]
//! implementing [`BuildContext`].

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use anyhow::Result;
use anyhow::{bail, Context};
use itertools::Itertools;
use tracing::{debug, instrument};

use distribution_types::Metadata;
use pep508_rs::Requirement;
use platform_tags::Tags;
use puffin_build::{SourceBuild, SourceBuildContext};
use puffin_client::RegistryClient;
use puffin_distribution::DistributionDatabase;
use puffin_installer::{InstallPlan, Installer, Unzipper};
use puffin_interpreter::{Interpreter, Virtualenv};
use puffin_resolver::{DistFinder, Manifest, ResolutionOptions, Resolver};
use puffin_traits::BuildContext;

/// The main implementation of [`BuildContext`], used by the CLI, see [`BuildContext`]
/// documentation.
pub struct BuildDispatch {
    client: RegistryClient,
    cache: PathBuf,
    interpreter: Interpreter,
    base_python: PathBuf,
    no_build: bool,
    source_build_context: SourceBuildContext,
    options: ResolutionOptions,
}

impl BuildDispatch {
    pub fn new(
        client: RegistryClient,
        cache: PathBuf,
        interpreter: Interpreter,
        base_python: PathBuf,
        no_build: bool,
    ) -> Self {
        Self {
            client,
            cache,
            interpreter,
            base_python,
            no_build,
            source_build_context: SourceBuildContext::default(),
            options: ResolutionOptions::default(),
        }
    }

    #[must_use]
    pub fn with_options(mut self, options: ResolutionOptions) -> Self {
        self.options = options;
        self
    }
}

impl BuildContext for BuildDispatch {
    fn cache(&self) -> &Path {
        self.cache.as_path()
    }

    fn interpreter(&self) -> &Interpreter {
        &self.interpreter
    }

    fn base_python(&self) -> &Path {
        &self.base_python
    }

    fn no_build(&self) -> bool {
        self.no_build
    }

    #[instrument(skip(self, requirements), fields(requirements = requirements.iter().map(ToString::to_string).join(", ")))]
    fn resolve<'a>(
        &'a self,
        requirements: &'a [Requirement],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Requirement>>> + Send + 'a>> {
        Box::pin(async {
            let tags = Tags::from_interpreter(&self.interpreter)?;
            let resolver = Resolver::new(
                Manifest::new(requirements.to_vec(), Vec::default(), Vec::default(), None),
                self.options,
                self.interpreter.markers(),
                &tags,
                &self.client,
                self,
            );
            let resolution_graph = resolver.resolve().await.with_context(|| {
                format!(
                    "No solution found when resolving: {}",
                    requirements.iter().map(ToString::to_string).join(", "),
                )
            })?;
            Ok(resolution_graph.requirements())
        })
    }

    #[instrument(skip(self))]
    fn install<'a>(
        &'a self,
        requirements: &'a [Requirement],
        venv: &'a Virtualenv,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            debug!(
                "Installing in {} in {}",
                requirements.iter().map(ToString::to_string).join(", "),
                venv.root().display(),
            );

            let tags = Tags::from_interpreter(&self.interpreter)?;

            let InstallPlan {
                local,
                remote,
                extraneous,
            } = InstallPlan::try_from_requirements(requirements, &self.cache, venv, &tags)?;

            // Resolve the dependencies.
            let remote = if remote.is_empty() {
                Vec::new()
            } else {
                debug!(
                    "Fetching build requirement{}: {}",
                    if remote.len() == 1 { "" } else { "s" },
                    remote.iter().map(ToString::to_string).join(", ")
                );
                let resolution = DistFinder::new(&tags, &self.client, self.interpreter())
                    .resolve(&remote)
                    .await
                    .context("Failed to resolve build dependencies")?;
                resolution.into_distributions().collect::<Vec<_>>()
            };

            // Download any missing distributions.
            let wheels = if remote.is_empty() {
                vec![]
            } else {
                // TODO(konstin): Check that there is no endless recursion
                let fetcher = DistributionDatabase::new(self.cache(), &tags, &self.client, self);
                debug!(
                    "Downloading and building requirement{} for build: {}",
                    if remote.len() == 1 { "" } else { "s" },
                    remote.iter().map(ToString::to_string).join(", ")
                );

                fetcher
                    .get_wheels(remote)
                    .await
                    .context("Failed to download and build distributions")?
            };

            // Unzip any downloaded distributions.
            let unzips = if wheels.is_empty() {
                vec![]
            } else {
                debug!(
                    "Unzipping build requirement{}: {}",
                    if wheels.len() == 1 { "" } else { "s" },
                    wheels.iter().map(ToString::to_string).join(", ")
                );
                Unzipper::default()
                    .unzip(wheels, &self.cache)
                    .await
                    .context("Failed to unpack build dependencies")?
            };

            // Remove any unnecessary packages.
            if !extraneous.is_empty() {
                for dist_info in &extraneous {
                    let summary = puffin_installer::uninstall(dist_info)
                        .await
                        .context("Failed to uninstall build dependencies")?;
                    debug!(
                        "Uninstalled {} ({} file{}, {} director{})",
                        dist_info.name(),
                        summary.file_count,
                        if summary.file_count == 1 { "" } else { "s" },
                        summary.dir_count,
                        if summary.dir_count == 1 { "y" } else { "ies" },
                    );
                }
            }

            // Install the resolved distributions.
            let wheels = unzips.into_iter().chain(local).collect::<Vec<_>>();
            if !wheels.is_empty() {
                debug!(
                    "Installing build requirement{}: {}",
                    if wheels.len() == 1 { "" } else { "s" },
                    wheels.iter().map(ToString::to_string).join(", ")
                );
                Installer::new(venv)
                    .install(&wheels)
                    .context("Failed to install build dependencies")?;
            }

            Ok(())
        })
    }

    #[instrument(skip(self))]
    fn build_source<'a>(
        &'a self,
        source: &'a Path,
        subdirectory: Option<&'a Path>,
        wheel_dir: &'a Path,
        package_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(async move {
            if self.no_build {
                bail!("Building source distributions is disabled");
            }
            let builder = SourceBuild::setup(
                source,
                subdirectory,
                &self.interpreter,
                self,
                self.source_build_context.clone(),
                package_id,
            )
            .await?;
            Ok(builder.build(wheel_dir)?)
        })
    }
}
