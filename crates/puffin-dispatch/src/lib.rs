//! Avoid cyclic crate dependencies between [resolver][`puffin_resolver`],
//! [installer][`puffin_installer`] and [build][`puffin_build`] through [`BuildDispatch`]
//! implementing [`BuildContext`].

use std::future::Future;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use itertools::Itertools;
use tracing::{debug, instrument};

use distribution_types::{IndexLocations, Name, Resolution};
use pep508_rs::Requirement;
use puffin_build::{SourceBuild, SourceBuildContext};
use puffin_cache::Cache;
use puffin_client::{FlatIndex, RegistryClient};
use puffin_installer::{Downloader, Installer, NoBinary, Plan, Planner, Reinstall, SitePackages};
use puffin_interpreter::{Interpreter, Virtualenv};
use puffin_resolver::{InMemoryIndex, Manifest, Options, Resolver};
use puffin_traits::{BuildContext, BuildKind, InFlight, SetupPyStrategy};

/// The main implementation of [`BuildContext`], used by the CLI, see [`BuildContext`]
/// documentation.
pub struct BuildDispatch<'a> {
    client: &'a RegistryClient,
    cache: &'a Cache,
    interpreter: &'a Interpreter,
    index_locations: &'a IndexLocations,
    flat_index: &'a FlatIndex,
    index: &'a InMemoryIndex,
    in_flight: &'a InFlight,
    base_python: PathBuf,
    setup_py: SetupPyStrategy,
    no_build: bool,
    no_binary: &'a NoBinary,
    source_build_context: SourceBuildContext,
    options: Options,
}

impl<'a> BuildDispatch<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client: &'a RegistryClient,
        cache: &'a Cache,
        interpreter: &'a Interpreter,
        index_locations: &'a IndexLocations,
        flat_index: &'a FlatIndex,
        index: &'a InMemoryIndex,
        in_flight: &'a InFlight,
        base_python: PathBuf,
        setup_py: SetupPyStrategy,
        no_build: bool,
        no_binary: &'a NoBinary,
    ) -> Self {
        Self {
            client,
            cache,
            interpreter,
            index_locations,
            flat_index,
            index,
            in_flight,
            base_python,
            setup_py,
            no_build,
            no_binary,
            source_build_context: SourceBuildContext::default(),
            options: Options::default(),
        }
    }

    #[must_use]
    pub fn with_options(mut self, options: Options) -> Self {
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

    fn no_binary(&self) -> &NoBinary {
        self.no_binary
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
            self.flat_index,
            self.index,
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

            let Plan {
                local,
                remote,
                reinstalls,
                extraneous,
            } = Planner::with_requirements(&resolution.requirements()).build(
                site_packages,
                &Reinstall::None,
                &NoBinary::None,
                self.index_locations,
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
                    .download(remote, self.in_flight)
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
