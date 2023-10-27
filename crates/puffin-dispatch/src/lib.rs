//! Avoid cyclic crate dependencies between [resolver][`puffin_resolver`],
//! [installer][`puffin_installer`] and [build][`puffin_build`] through [`BuildDispatch`]
//! implementing [`BuildContext`].

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use anyhow::Context;
use anyhow::Result;
use itertools::Itertools;
use tracing::{debug, instrument};

use pep508_rs::Requirement;
use platform_tags::Tags;
use puffin_build::SourceDistributionBuilder;
use puffin_client::RegistryClient;
use puffin_installer::{
    Downloader, Installer, PartitionedRequirements, RemoteDistribution, Unzipper,
};
use puffin_interpreter::{InterpreterInfo, Virtualenv};
use puffin_resolver::{ResolutionManifest, ResolutionMode, Resolver, WheelFinder};
use puffin_traits::BuildContext;

/// The main implementation of [`BuildContext`], used by the CLI, see [`BuildContext`]
/// documentation.
pub struct BuildDispatch {
    client: RegistryClient,
    cache: Option<PathBuf>,
    interpreter_info: InterpreterInfo,
    base_python: PathBuf,
}

impl BuildDispatch {
    pub fn new(
        client: RegistryClient,
        cache: Option<PathBuf>,
        interpreter_info: InterpreterInfo,
        base_python: PathBuf,
    ) -> Self {
        Self {
            client,
            cache,
            interpreter_info,
            base_python,
        }
    }
}

impl BuildContext for BuildDispatch {
    fn cache(&self) -> Option<&Path> {
        self.cache.as_deref()
    }

    fn interpreter_info(&self) -> &InterpreterInfo {
        &self.interpreter_info
    }

    fn base_python(&self) -> &Path {
        &self.base_python
    }

    #[instrument(skip(self))]
    fn resolve<'a>(
        &'a self,
        requirements: &'a [Requirement],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Requirement>>> + Send + 'a>> {
        Box::pin(async {
            let tags = Tags::from_env(
                self.interpreter_info.platform(),
                self.interpreter_info.simple_version(),
            )?;
            let resolver = Resolver::new(
                ResolutionManifest::new(
                    requirements.to_vec(),
                    Vec::default(),
                    // TODO(charlie): Include locally-available wheels in the list of preferred
                    // versions.
                    Vec::default(),
                    ResolutionMode::default(),
                ),
                self.interpreter_info.markers(),
                &tags,
                &self.client,
                self,
            );
            let resolution_graph = resolver.resolve().await.context(
                "No solution found when resolving build dependencies for source distribution build",
            )?;
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

            let PartitionedRequirements {
                local,
                remote,
                extraneous,
            } = PartitionedRequirements::try_from_requirements(
                requirements,
                self.cache.as_deref(),
                venv,
            )?;

            let tags = Tags::from_env(
                self.interpreter_info.platform(),
                self.interpreter_info.simple_version(),
            )?;

            // Resolve the dependencies.
            let remote = if remote.is_empty() {
                Vec::new()
            } else {
                debug!(
                    "Fetching build requirement{}: {}",
                    if remote.len() == 1 { "" } else { "s" },
                    remote.iter().map(ToString::to_string).join(", ")
                );
                let resolution = WheelFinder::new(&tags, &self.client)
                    .resolve(&remote)
                    .await
                    .context("Failed to resolve build dependencies")?;
                resolution
                    .into_files()
                    .map(RemoteDistribution::from_file)
                    .collect::<Result<Vec<_>>>()?
            };

            // Download any missing distributions.
            let staging = tempfile::tempdir()?;
            let downloads = if remote.is_empty() {
                vec![]
            } else {
                debug!(
                    "Downloading build requirement{}: {}",
                    if remote.len() == 1 { "" } else { "s" },
                    remote.iter().map(ToString::to_string).join(", ")
                );
                Downloader::new(&self.client, self.cache.as_deref())
                    .download(&remote, self.cache.as_deref().unwrap_or(staging.path()))
                    .await
                    .context("Failed to download build dependencies")?
            };

            // Unzip any downloaded distributions.
            let unzips = if downloads.is_empty() {
                vec![]
            } else {
                debug!(
                    "Unzipping build requirement{}: {}",
                    if downloads.len() == 1 { "" } else { "s" },
                    downloads.iter().map(ToString::to_string).join(", ")
                );
                Unzipper::default()
                    .download(downloads, self.cache.as_deref().unwrap_or(staging.path()))
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
    fn build_source_distribution<'a>(
        &'a self,
        sdist: &'a Path,
        wheel_dir: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(async move {
            let builder =
                SourceDistributionBuilder::setup(sdist, &self.interpreter_info, self).await?;
            Ok(builder.build(wheel_dir)?)
        })
    }
}
