//! Avoid cyclic crate dependencies between [resolver][`puffin_resolver`],
//! [installer][`puffin_installer`] and [build][`puffin_build`] through [`BuildDispatch`]
//! implementing [`BuildContext`].

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use anyhow::Result;
use anyhow::{bail, Context};
use itertools::{Either, Itertools};
use tracing::{debug, instrument};

use pep508_rs::Requirement;
use platform_tags::Tags;
use puffin_build::{SourceBuild, SourceBuildContext};
use puffin_client::RegistryClient;
use puffin_distribution::Metadata;
use puffin_installer::{Builder, Downloader, InstallPlan, Installer, Unzipper};
use puffin_interpreter::{InterpreterInfo, Virtualenv};
use puffin_resolver::{DistFinder, Manifest, PreReleaseMode, ResolutionMode, Resolver};
use puffin_traits::BuildContext;

/// The main implementation of [`BuildContext`], used by the CLI, see [`BuildContext`]
/// documentation.
pub struct BuildDispatch {
    client: RegistryClient,
    cache: PathBuf,
    interpreter_info: InterpreterInfo,
    base_python: PathBuf,
    source_build_context: SourceBuildContext,
    no_build: bool,
}

impl BuildDispatch {
    pub fn new(
        client: RegistryClient,
        cache: PathBuf,
        interpreter_info: InterpreterInfo,
        base_python: PathBuf,
        no_build: bool,
    ) -> Self {
        Self {
            client,
            cache,
            interpreter_info,
            base_python,
            source_build_context: SourceBuildContext::default(),
            no_build,
        }
    }
}

impl BuildContext for BuildDispatch {
    fn cache(&self) -> &Path {
        self.cache.as_path()
    }

    fn interpreter_info(&self) -> &InterpreterInfo {
        &self.interpreter_info
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
            let tags = Tags::from_env(
                self.interpreter_info.platform(),
                self.interpreter_info.simple_version(),
            )?;
            let resolver = Resolver::new(
                Manifest::new(
                    requirements.to_vec(),
                    Vec::default(),
                    Vec::default(),
                    ResolutionMode::default(),
                    PreReleaseMode::default(),
                    None, // TODO(zanieb): We may want to provide a project name here
                ),
                self.interpreter_info.markers(),
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

            let InstallPlan {
                local,
                remote,
                extraneous,
            } = InstallPlan::try_from_requirements(requirements, &self.cache, venv)?;

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
                let resolution = DistFinder::new(&tags, &self.client, self.interpreter_info())
                    .resolve(&remote)
                    .await
                    .context("Failed to resolve build dependencies")?;
                resolution.into_distributions().collect::<Vec<_>>()
            };

            // Download any missing distributions.
            let downloads = if remote.is_empty() {
                vec![]
            } else {
                debug!(
                    "Downloading build requirement{}: {}",
                    if remote.len() == 1 { "" } else { "s" },
                    remote.iter().map(ToString::to_string).join(", ")
                );
                Downloader::new(&self.client, &self.cache)
                    .with_no_build(self.no_build)
                    .download(remote)
                    .await
                    .context("Failed to download build dependencies")?
            };

            let (wheels, sdists): (Vec<_>, Vec<_>) =
                downloads
                    .into_iter()
                    .partition_map(|download| match download {
                        puffin_installer::Download::Wheel(wheel) => Either::Left(wheel),
                        puffin_installer::Download::SourceDist(sdist) => Either::Right(sdist),
                    });

            // Build any missing source distributions.
            let sdists = if sdists.is_empty() {
                vec![]
            } else {
                debug!(
                    "Building source distributions{}: {}",
                    if sdists.len() == 1 { "" } else { "s" },
                    sdists.iter().map(ToString::to_string).join(", ")
                );
                Builder::new(self)
                    .build(sdists)
                    .await
                    .context("Failed to build source distributions")?
            };

            let downloads = wheels.into_iter().chain(sdists).collect::<Vec<_>>();

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
                    .unzip(downloads, &self.cache)
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
                &self.interpreter_info,
                self,
                self.source_build_context.clone(),
                package_id,
            )
            .await?;
            Ok(builder.build(wheel_dir)?)
        })
    }
}
