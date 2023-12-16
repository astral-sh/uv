//! Avoid cyclic crate dependencies between [resolver][`puffin_resolver`],
//! [installer][`puffin_installer`] and [build][`puffin_build`] through [`BuildDispatch`]
//! implementing [`BuildContext`].

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use anyhow::{bail, Context, Result};
use itertools::Itertools;
use tracing::{debug, instrument};

use distribution_types::{CachedDist, Metadata, Resolution};
use pep508_rs::Requirement;
use platform_tags::Tags;
use puffin_build::{SourceBuild, SourceBuildContext};
use puffin_cache::Cache;
use puffin_client::RegistryClient;
use puffin_installer::{Downloader, InstallPlan, Installer, Reinstall};
use puffin_interpreter::{Interpreter, Virtualenv};
use puffin_resolver::{Manifest, ResolutionOptions, Resolver};
use puffin_traits::{BuildContext, BuildKind, OnceMap};
use pypi_types::IndexUrls;

/// The main implementation of [`BuildContext`], used by the CLI, see [`BuildContext`]
/// documentation.
pub struct BuildDispatch {
    client: RegistryClient,
    cache: Cache,
    interpreter: Interpreter,
    base_python: PathBuf,
    no_build: bool,
    source_build_context: SourceBuildContext,
    options: ResolutionOptions,
    index_urls: IndexUrls,
    in_flight_unzips: OnceMap<PathBuf, Result<CachedDist, String>>,
}

impl BuildDispatch {
    pub fn new(
        client: RegistryClient,
        cache: Cache,
        interpreter: Interpreter,
        base_python: PathBuf,
        no_build: bool,
        index_urls: IndexUrls,
    ) -> Self {
        Self {
            client,
            cache,
            interpreter,
            base_python,
            no_build,
            source_build_context: SourceBuildContext::default(),
            options: ResolutionOptions::default(),
            index_urls,
            in_flight_unzips: OnceMap::default(),
        }
    }

    #[must_use]
    pub fn with_options(mut self, options: ResolutionOptions) -> Self {
        self.options = options;
        self
    }
}

impl BuildContext for BuildDispatch {
    type SourceDistBuilder = SourceBuild;

    fn cache(&self) -> &Cache {
        &self.cache
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
    ) -> Pin<Box<dyn Future<Output = Result<Resolution>> + Send + 'a>> {
        Box::pin(async {
            let tags = Tags::from_interpreter(&self.interpreter)?;
            let resolver = Resolver::new(
                Manifest::simple(requirements.to_vec()),
                self.options,
                self.interpreter.markers(),
                &tags,
                &self.client,
                self,
            );
            let graph = resolver.resolve().await.with_context(|| {
                format!(
                    "No solution found when resolving: {}",
                    requirements.iter().map(ToString::to_string).join(", "),
                )
            })?;
            Ok(Resolution::from(graph))
        })
    }

    #[instrument(
        skip(self, resolution, venv),
        fields(
            resolution = resolution.distributions().map(ToString::to_string).join(", "),
            venv = ?venv.root()
        )
    )]
    fn install<'a>(
        &'a self,
        resolution: &'a Resolution,
        venv: &'a Virtualenv,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            debug!(
                "Installing in {} in {}",
                resolution
                    .distributions()
                    .map(ToString::to_string)
                    .join(", "),
                venv.root().display(),
            );

            let tags = Tags::from_interpreter(&self.interpreter)?;

            let InstallPlan {
                local,
                remote,
                reinstalls,
                extraneous,
                editables: _,
            } = InstallPlan::from_requirements(
                &resolution.requirements(),
                &Reinstall::None,
                &[],
                &self.index_urls,
                self.cache(),
                venv,
                &tags,
            )?;

            // Resolve any registry-based requirements.
            let remote = remote
                .iter()
                .map(|dist| {
                    resolution
                        .get(&dist.name)
                        .cloned()
                        .expect("Resolution should contain all packages")
                })
                .collect::<Vec<_>>();

            // Download any missing distributions.
            let wheels = if remote.is_empty() {
                vec![]
            } else {
                // TODO(konstin): Check that there is no endless recursion.
                let downloader = Downloader::new(self.cache(), &tags, &self.client, self);
                debug!(
                    "Downloading and building requirement{} for build: {}",
                    if remote.len() == 1 { "" } else { "s" },
                    remote.iter().map(ToString::to_string).join(", ")
                );

                downloader
                    .download(remote, &self.in_flight_unzips)
                    .await
                    .context("Failed to download and build distributions")?
            };

            // Remove any unnecessary packages.
            if !extraneous.is_empty() || !reinstalls.is_empty() {
                for dist_info in extraneous.iter().chain(reinstalls.iter()) {
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
            let wheels = wheels.into_iter().chain(local).collect::<Vec<_>>();
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

    #[instrument(skip_all, fields(package_id = package_id, subdirectory = ?subdirectory))]
    fn setup_build<'a>(
        &'a self,
        source: &'a Path,
        subdirectory: Option<&'a Path>,
        package_id: &'a str,
        build_kind: BuildKind,
    ) -> Pin<Box<dyn Future<Output = Result<SourceBuild>> + Send + 'a>> {
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
                package_id.to_string(),
                build_kind,
            )
            .await?;
            Ok(builder)
        })
    }
}
