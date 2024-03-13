//! Avoid cyclic crate dependencies between [resolver][`uv_resolver`],
//! [installer][`uv_installer`] and [build][`uv_build`] through [`BuildDispatch`]
//! implementing [`BuildContext`].

use std::ffi::OsStr;
use std::path::Path;
use std::{ffi::OsString, future::Future};

use anyhow::{bail, Context, Result};
use futures::FutureExt;
use itertools::Itertools;
use rustc_hash::FxHashMap;
use tracing::{debug, instrument};

use distribution_types::{IndexLocations, Name, Resolution, SourceDist};
use pep508_rs::Requirement;
use uv_build::{SourceBuild, SourceBuildContext};
use uv_cache::Cache;
use uv_client::{FlatIndex, RegistryClient};
use uv_installer::{Downloader, Installer, NoBinary, Plan, Planner, Reinstall, SitePackages};
use uv_interpreter::{Interpreter, PythonEnvironment};
use uv_resolver::{InMemoryIndex, Manifest, Options, Resolver};
use uv_traits::{
    BuildContext, BuildIsolation, BuildKind, ConfigSettings, InFlight, NoBuild, SetupPyStrategy,
};

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
    setup_py: SetupPyStrategy,
    build_isolation: BuildIsolation<'a>,
    no_build: &'a NoBuild,
    no_binary: &'a NoBinary,
    config_settings: &'a ConfigSettings,
    source_build_context: SourceBuildContext,
    options: Options,
    build_extra_env_vars: FxHashMap<OsString, OsString>,
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
        setup_py: SetupPyStrategy,
        config_settings: &'a ConfigSettings,
        build_isolation: BuildIsolation<'a>,
        no_build: &'a NoBuild,
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
            setup_py,
            config_settings,
            build_isolation,
            no_build,
            no_binary,
            source_build_context: SourceBuildContext::default(),
            options: Options::default(),
            build_extra_env_vars: FxHashMap::default(),
        }
    }

    #[must_use]
    pub fn with_options(mut self, options: Options) -> Self {
        self.options = options;
        self
    }

    /// Set the environment variables to be used when building a source distribution.
    #[must_use]
    pub fn with_build_extra_env_vars<I, K, V>(mut self, sdist_build_env_variables: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.build_extra_env_vars = sdist_build_env_variables
            .into_iter()
            .map(|(key, value)| (key.as_ref().to_owned(), value.as_ref().to_owned()))
            .collect();
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

    fn build_isolation(&self) -> BuildIsolation {
        self.build_isolation
    }

    fn no_build(&self) -> &NoBuild {
        self.no_build
    }

    fn no_binary(&self) -> &NoBinary {
        self.no_binary
    }

    fn index_locations(&self) -> &IndexLocations {
        self.index_locations
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
        )?;
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
        venv: &'data PythonEnvironment,
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
            let site_packages = SitePackages::from_executable(venv)?;

            let Plan {
                local,
                remote,
                reinstalls,
                extraneous: _,
            } = Planner::with_requirements(&resolution.requirements()).build(
                site_packages,
                &Reinstall::None,
                &NoBinary::None,
                self.index_locations,
                self.cache(),
                venv,
                tags,
            )?;

            // Nothing to do.
            if remote.is_empty() && local.is_empty() && reinstalls.is_empty() {
                debug!("No build requirements to install for build");
                return Ok(());
            }

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
                let downloader = Downloader::new(self.cache, tags, self.client, self);
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
            if !reinstalls.is_empty() {
                for dist_info in &reinstalls {
                    let summary = uv_installer::uninstall(dist_info)
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
    async fn setup_build<'data>(
        &'data self,
        source: &'data Path,
        subdirectory: Option<&'data Path>,
        package_id: &'data str,
        dist: Option<&'data SourceDist>,
        build_kind: BuildKind,
    ) -> Result<SourceBuild> {
        match self.no_build {
            NoBuild::All => debug_assert!(
                matches!(build_kind, BuildKind::Editable),
                "Only editable builds are exempt from 'no build' checks"
            ),
            NoBuild::None => {}
            NoBuild::Packages(packages) => {
                if let Some(dist) = dist {
                    // We can only prevent builds by name for packages with names
                    // which is unknown before build of editable source distributions
                    if packages.contains(dist.name()) {
                        bail!(
                            "Building source distributions for {} is disabled",
                            dist.name()
                        );
                    }
                } else {
                    debug_assert!(
                        matches!(build_kind, BuildKind::Editable),
                        "Only editable builds are exempt from 'no build' checks"
                    );
                }
            }
        }

        let builder = SourceBuild::setup(
            source,
            subdirectory,
            self.interpreter,
            self,
            self.source_build_context.clone(),
            package_id.to_string(),
            self.setup_py,
            self.config_settings.clone(),
            self.build_isolation,
            build_kind,
            self.build_extra_env_vars.clone(),
        )
        .boxed()
        .await?;
        Ok(builder)
    }
}
