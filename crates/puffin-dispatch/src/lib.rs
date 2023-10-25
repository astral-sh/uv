// `Itertools::intersperse` could be shadowed by an unstable std intersperse, but that
// https://github.com/rust-lang/rust/issues/79524 has no activity and would only move itertools'
// feature to std.
#![allow(unstable_name_collisions)]

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use anyhow::Context;
use itertools::{Either, Itertools};
use tempfile::tempdir;

use gourgeist::Venv;
use log::debug;
use pep508_rs::Requirement;
use platform_tags::Tags;
use puffin_build::SourceDistributionBuilder;
use puffin_client::RegistryClient;
use puffin_installer::{
    uninstall, CachedDistribution, Downloader, Installer, LocalIndex, RemoteDistribution,
    SitePackages, Unzipper,
};
use puffin_interpreter::PythonExecutable;
use puffin_package::package_name::PackageName;
use puffin_resolver::{ResolutionMode, Resolver, WheelFinder};
use puffin_traits::PuffinCtx;

pub struct PuffinDispatch {
    client: RegistryClient,
    python: PythonExecutable,
    cache: Option<PathBuf>,
}

impl PuffinDispatch {
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

impl PuffinCtx for PuffinDispatch {
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
                // TODO: nested builds are not supported yet
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
                requirements
                    .iter()
                    .map(std::string::ToString::to_string)
                    .intersperse(", ".to_string())
                    .collect::<String>()
            );
            let python = PythonExecutable::from_venv_with_base(venv.as_std_path(), &self.python);

            let site_packages = SitePackages::try_from_executable(&python)?;

            // We have three buckets:
            // * Not installed
            // * Installed and a matching version
            // * Install but an incorrect version, we need to remove it first
            let mut to_remove = Vec::new();
            let mut to_install = Vec::new();
            for requirement in requirements {
                if let Some((_name, installed)) =
                    site_packages.iter().find(|(name, _distribution)| {
                        name == &&PackageName::normalize(&requirement.name)
                    })
                {
                    if requirement.is_satisfied_by(installed.version()) {
                        // Nothing to do
                    } else {
                        to_remove.push(installed);
                        to_install.push(requirement.clone());
                    }
                } else {
                    to_install.push(requirement.clone());
                }
            }

            if !to_remove.is_empty() {
                debug!(
                    "Removing {:?}",
                    to_remove
                        .iter()
                        .map(|dist| dist.id())
                        .intersperse(", ".to_string())
                        .collect::<String>()
                );

                for dist_info in to_remove {
                    uninstall(dist_info).await?;
                }
            }

            debug!(
                "Installing {}",
                to_install
                    .iter()
                    .map(std::string::ToString::to_string)
                    .intersperse(", ".to_string())
                    .collect::<String>()
            );

            let local_index = if let Some(cache) = &self.cache {
                LocalIndex::try_from_directory(cache)?
            } else {
                LocalIndex::default()
            };

            let (cached, uncached): (Vec<CachedDistribution>, Vec<Requirement>) =
                to_install.iter().partition_map(|requirement| {
                    let package = PackageName::normalize(&requirement.name);
                    if let Some(distribution) = local_index
                        .get(&package)
                        .filter(|dist| requirement.is_satisfied_by(dist.version()))
                    {
                        Either::Left(distribution.clone())
                    } else {
                        Either::Right(requirement.clone())
                    }
                });

            let tags = Tags::from_env(python.platform(), python.simple_version())?;
            let resolution = WheelFinder::new(&tags, &self.client)
                .resolve(&uncached)
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
            let wheels = unzips.into_iter().chain(cached).collect::<Vec<_>>();
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
            // TODO: Merge this with PythonExecutable
            let interpreter_info = gourgeist::get_interpreter_info(self.python.executable())?;

            let builder = SourceDistributionBuilder::setup(sdist, &interpreter_info, self).await?;
            Ok(builder.build(wheel_dir)?)
        })
    }
}
