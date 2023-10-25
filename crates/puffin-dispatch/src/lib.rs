//! Avoid cyclic crate dependencies between [resolver][`puffin_resolver`],
//! [installer][`puffin_installer`] and [build][`puffin_build`] through [`BuildDispatch`]
//! implementing [`BuildContext`].

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use anyhow::Context;
use itertools::Itertools;
use tempfile::tempdir;

use gourgeist::Venv;
use pep508_rs::Requirement;
use platform_tags::Tags;
use puffin_build::SourceDistributionBuilder;
use puffin_client::RegistryClient;
use puffin_installer::{
    uninstall, Downloader, Installer, PartitionedRequirements, RemoteDistribution, Unzipper,
};
use puffin_interpreter::PythonExecutable;
use puffin_resolver::{ResolutionMode, Resolver, WheelFinder};
use puffin_traits::BuildContext;
use tracing::debug;

/// The main implementation of [`BuildContext`], used by the CLI, see [`BuildContext`]
/// documentation.
pub struct BuildDispatch {
    client: RegistryClient,
    python: PythonExecutable,
    cache: Option<PathBuf>,
}

impl BuildDispatch {
    pub fn new<T>(client: RegistryClient, python: PythonExecutable, cache: Option<T>) -> Self
    where
        T: Into<PathBuf>,
    {
        Self {
            client,
            python,
            cache: cache.map(Into::into),
        }
    }
}

impl BuildContext for BuildDispatch {
    fn cache(&self) -> Option<&Path> {
        self.cache.as_deref()
    }

    fn python(&self) -> &PythonExecutable {
        &self.python
    }

    fn resolve<'a>(
        &'a self,
        requirements: &'a [Requirement],
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<Requirement>>> + 'a>> {
        Box::pin(async {
            let tags = Tags::from_env(self.python.platform(), self.python.simple_version())?;
            let resolver = Resolver::new(
                requirements.to_vec(),
                Vec::default(),
                ResolutionMode::Highest,
                self.python.markers(),
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

    fn install<'a>(
        &'a self,
        requirements: &'a [Requirement],
        venv: &'a Venv,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + 'a>> {
        Box::pin(async move {
            debug!(
                "Install in {} requirements {}",
                venv.as_str(),
                requirements.iter().map(ToString::to_string).join(", ")
            );
            let python = self.python().with_venv(venv.as_std_path());

            let PartitionedRequirements {
                local,
                remote,
                extraneous,
            } = PartitionedRequirements::try_from_requirements(
                requirements,
                self.cache(),
                &python,
            )?;

            if !extraneous.is_empty() {
                debug!(
                    "Removing {:?}",
                    extraneous
                        .iter()
                        .map(puffin_installer::InstalledDistribution::id)
                        .join(", ")
                );

                for dist_info in extraneous {
                    uninstall(&dist_info).await?;
                }
            }

            debug!(
                "Fetching {}",
                remote.iter().map(ToString::to_string).join(", ")
            );

            let tags = Tags::from_env(python.platform(), python.simple_version())?;
            let resolution = WheelFinder::new(&tags, &self.client)
                .resolve(&remote)
                .await?;

            let uncached = resolution
                .into_files()
                .map(RemoteDistribution::from_file)
                .collect::<anyhow::Result<Vec<_>>>()?;
            let staging = tempdir()?;
            let downloads = Downloader::new(&self.client, self.cache.as_deref())
                .download(&uncached, self.cache.as_deref().unwrap_or(staging.path()))
                .await?;
            let unzips = Unzipper::default()
                .download(downloads, self.cache.as_deref().unwrap_or(staging.path()))
                .await
                .context("Failed to download and unpack wheels")?;

            debug!(
                "Fetching {}",
                unzips
                    .iter()
                    .chain(&local)
                    .map(puffin_installer::CachedDistribution::id)
                    .join(", ")
            );
            let wheels = unzips.into_iter().chain(local).collect::<Vec<_>>();
            Installer::new(&python).install(&wheels)?;
            Ok(())
        })
    }

    fn build_source_distribution<'a>(
        &'a self,
        sdist: &'a Path,
        wheel_dir: &'a Path,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + 'a>> {
        Box::pin(async move {
            let interpreter_info = gourgeist::get_interpreter_info(self.python.executable())?;
            let builder = SourceDistributionBuilder::setup(sdist, &interpreter_info, self).await?;
            Ok(builder.build(wheel_dir)?)
        })
    }
}
