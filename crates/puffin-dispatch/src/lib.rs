//! Avoid cyclic crate dependencies between [resolver][`puffin_resolver`],
//! [installer][`puffin_installer`] and [build][`puffin_build`] through [`BuildDispatch`]
//! implementing [`BuildContext`].

use std::future::Future;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use itertools::Itertools;
use tracing::{debug, instrument};

use distribution_types::{CachedDist, IndexUrls, Name, Resolution};
use pep508_rs::Requirement;
use puffin_build::{SourceBuild, SourceBuildContext};
use puffin_cache::Cache;
use puffin_client::RegistryClient;
use puffin_installer::{Downloader, InstallPlan, Installer, Reinstall, SitePackages};
use puffin_interpreter::{Interpreter, Virtualenv};
use puffin_resolver::{Manifest, ResolutionOptions, Resolver};
use puffin_traits::{BuildContext, BuildKind, OnceMap, SetupPyStrategy};

/// The main implementation of [`BuildContext`], used by the CLI, see [`BuildContext`]
/// documentation.
pub struct BuildDispatch<'a> {
    client: &'a RegistryClient,
    cache: &'a Cache,
    interpreter: &'a Interpreter,
    index_urls: &'a IndexUrls,
    base_python: PathBuf,
    setup_py: SetupPyStrategy,
    no_build: bool,
    source_build_context: SourceBuildContext,
    options: ResolutionOptions,
    in_flight_unzips: OnceMap<PathBuf, Result<CachedDist, String>>,
}

impl<'a> BuildDispatch<'a> {
    pub fn new(
        client: &'a RegistryClient,
        cache: &'a Cache,
        interpreter: &'a Interpreter,
        index_urls: &'a IndexUrls,
        base_python: PathBuf,
        setup_py: SetupPyStrategy,
        no_build: bool,
    ) -> Self {
        Self {
            client,
            cache,
            interpreter,
            index_urls,
            base_python,
            setup_py,
            no_build,
            source_build_context: SourceBuildContext::default(),
            options: ResolutionOptions::default(),
            in_flight_unzips: OnceMap::default(),
        }
    }

    #[must_use]
    pub fn with_options(mut self, options: ResolutionOptions) -> Self {
        self.options = options;
        self
    }
}

impl<'a> BuildContext for BuildDispatch<'a> {
    type SourceDistBuilder = SourceBuild;

    fn cache(&self) -> &Cache {
        self.cache
    }

    fn interpreter(&self) -> &Interpreter {
        self.interpreter
    }

    fn base_python(&self) -> &Path {
        &self.base_python
    }

    fn no_build(&self) -> bool {
        self.no_build
    }

    fn setup_py_strategy(&self) -> SetupPyStrategy {
        self.setup_py
    }

    async fn resolve<'data>(&'data self, requirements: &'data [Requirement]) -> Result<Resolution> {
        let markers = self.interpreter.markers();
        let tags = self.interpreter.tags()?;
        let resolver = Resolver::new(
            Manifest::simple(requirements.to_vec()),
            self.options,
            markers,
            self.interpreter,
            tags,
            self.client,
            self,
        );
        let graph = resolver.resolve().await.with_context(|| {
            format!(
                "No solution found when resolving: {}",
                requirements.iter().map(ToString::to_string).join(", "),
            )
        })?;
        Ok(Resolution::from(graph))
    }

    #[allow(clippy::manual_async_fn)] // TODO(konstin): rustc 1.75 gets into a type inference cycle with async fn
    #[instrument(
        skip(self, resolution, venv),
        fields(
            resolution = resolution.distributions().map(ToString::to_string).join(", "),
            venv = ?venv.root()
        )
    )]
    fn install<'data>(
        &'data self,
        resolution: &'data Resolution,
        venv: &'data Virtualenv,
    ) -> impl Future<Output = Result<()>> + Send + 'data {
        async move {
            debug!(
                "Installing in {} in {}",
                resolution
                    .distributions()
                    .map(ToString::to_string)
                    .join(", "),
                venv.root().display(),
            );

            // Determine the current environment markers.
            let tags = self.interpreter.tags()?;

            // Determine the set of installed packages.
            let site_packages =
                SitePackages::from_executable(venv).context("Failed to list installed packages")?;

            let InstallPlan {
                local,
                remote,
                reinstalls,
                extraneous,
            } = InstallPlan::from_requirements(
                &resolution.requirements(),
                Vec::new(),
                site_packages,
                &Reinstall::None,
                self.index_urls,
                self.cache(),
                venv,
                tags,
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
                let downloader = Downloader::new(self.cache(), tags, self.client, self);
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
        }
    }

    #[allow(clippy::manual_async_fn)] // TODO(konstin): rustc 1.75 gets into a type inference cycle with async fn
    #[instrument(skip_all, fields(package_id = package_id, subdirectory = ?subdirectory))]
    fn setup_build<'data>(
        &'data self,
        source: &'data Path,
        subdirectory: Option<&'data Path>,
        package_id: &'data str,
        build_kind: BuildKind,
    ) -> impl Future<Output = Result<SourceBuild>> + Send + 'data {
        async move {
            if self.no_build {
                bail!("Building source distributions is disabled");
            }

            let builder = SourceBuild::setup(
                source,
                subdirectory,
                self.interpreter,
                self,
                self.source_build_context.clone(),
                package_id.to_string(),
                self.setup_py,
                build_kind,
            )
            .await?;
            Ok(builder)
        }
    }
}
