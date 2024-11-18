//! Avoid cyclic crate dependencies between [resolver][`uv_resolver`],
//! [installer][`uv_installer`] and [build][`uv_build`] through [`BuildDispatch`]
//! implementing [`BuildContext`].

use std::ffi::{OsStr, OsString};
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use futures::FutureExt;
use itertools::Itertools;
use rustc_hash::FxHashMap;
use tracing::{debug, instrument};

use uv_build_frontend::{SourceBuild, SourceBuildContext};
use uv_cache::Cache;
use uv_client::RegistryClient;
use uv_configuration::{
    BuildKind, BuildOptions, ConfigSettings, Constraints, IndexStrategy, LowerBound, Reinstall,
    SourceStrategy,
};
use uv_configuration::{BuildOutput, Concurrency};
use uv_distribution::DistributionDatabase;
use uv_distribution_types::{
    CachedDist, DependencyMetadata, IndexCapabilities, IndexLocations, Name, Resolution,
    SourceDist, VersionOrUrlRef,
};
use uv_git::GitResolver;
use uv_installer::{Installer, Plan, Planner, Preparer, SitePackages};
use uv_pypi_types::{Conflicts, Requirement};
use uv_python::{Interpreter, PythonEnvironment};
use uv_resolver::{
    DerivationChainBuilder, ExcludeNewer, FlatIndex, Flexibility, InMemoryIndex, Manifest,
    OptionsBuilder, PythonRequirement, Resolver, ResolverEnvironment,
};
use uv_types::{BuildContext, BuildIsolation, EmptyInstalledPackages, HashStrategy, InFlight};

/// The main implementation of [`BuildContext`], used by the CLI, see [`BuildContext`]
/// documentation.
pub struct BuildDispatch<'a> {
    client: &'a RegistryClient,
    cache: &'a Cache,
    constraints: Constraints,
    interpreter: &'a Interpreter,
    index_locations: &'a IndexLocations,
    index_strategy: IndexStrategy,
    flat_index: &'a FlatIndex,
    index: &'a InMemoryIndex,
    git: &'a GitResolver,
    capabilities: &'a IndexCapabilities,
    dependency_metadata: &'a DependencyMetadata,
    in_flight: &'a InFlight,
    build_isolation: BuildIsolation<'a>,
    link_mode: uv_install_wheel::linker::LinkMode,
    build_options: &'a BuildOptions,
    config_settings: &'a ConfigSettings,
    hasher: &'a HashStrategy,
    exclude_newer: Option<ExcludeNewer>,
    source_build_context: SourceBuildContext,
    build_extra_env_vars: FxHashMap<OsString, OsString>,
    bounds: LowerBound,
    sources: SourceStrategy,
    concurrency: Concurrency,
}

impl<'a> BuildDispatch<'a> {
    pub fn new(
        client: &'a RegistryClient,
        cache: &'a Cache,
        constraints: Constraints,
        interpreter: &'a Interpreter,
        index_locations: &'a IndexLocations,
        flat_index: &'a FlatIndex,
        dependency_metadata: &'a DependencyMetadata,
        index: &'a InMemoryIndex,
        git: &'a GitResolver,
        capabilities: &'a IndexCapabilities,
        in_flight: &'a InFlight,
        index_strategy: IndexStrategy,
        config_settings: &'a ConfigSettings,
        build_isolation: BuildIsolation<'a>,
        link_mode: uv_install_wheel::linker::LinkMode,
        build_options: &'a BuildOptions,
        hasher: &'a HashStrategy,
        exclude_newer: Option<ExcludeNewer>,
        bounds: LowerBound,
        sources: SourceStrategy,
        concurrency: Concurrency,
    ) -> Self {
        Self {
            client,
            cache,
            constraints,
            interpreter,
            index_locations,
            flat_index,
            index,
            git,
            capabilities,
            dependency_metadata,
            in_flight,
            index_strategy,
            config_settings,
            build_isolation,
            link_mode,
            build_options,
            hasher,
            exclude_newer,
            source_build_context: SourceBuildContext::default(),
            build_extra_env_vars: FxHashMap::default(),
            bounds,
            sources,
            concurrency,
        }
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

    fn interpreter(&self) -> &Interpreter {
        self.interpreter
    }

    fn cache(&self) -> &Cache {
        self.cache
    }

    fn git(&self) -> &GitResolver {
        self.git
    }

    fn capabilities(&self) -> &IndexCapabilities {
        self.capabilities
    }

    fn dependency_metadata(&self) -> &DependencyMetadata {
        self.dependency_metadata
    }

    fn build_options(&self) -> &BuildOptions {
        self.build_options
    }

    fn config_settings(&self) -> &ConfigSettings {
        self.config_settings
    }

    fn bounds(&self) -> LowerBound {
        self.bounds
    }

    fn sources(&self) -> SourceStrategy {
        self.sources
    }

    fn locations(&self) -> &IndexLocations {
        self.index_locations
    }

    async fn resolve<'data>(&'data self, requirements: &'data [Requirement]) -> Result<Resolution> {
        let python_requirement = PythonRequirement::from_interpreter(self.interpreter);
        let marker_env = self.interpreter.resolver_marker_environment();
        let tags = self.interpreter.tags()?;

        let resolver = Resolver::new(
            Manifest::simple(requirements.to_vec()).with_constraints(self.constraints.clone()),
            OptionsBuilder::new()
                .exclude_newer(self.exclude_newer)
                .index_strategy(self.index_strategy)
                .flexibility(Flexibility::Fixed)
                .build(),
            &python_requirement,
            ResolverEnvironment::specific(marker_env),
            // Conflicting groups only make sense when doing
            // universal resolution.
            Conflicts::empty(),
            Some(tags),
            self.flat_index,
            self.index,
            self.hasher,
            self,
            EmptyInstalledPackages,
            DistributionDatabase::new(self.client, self, self.concurrency.downloads),
        )?;
        let resolution = Resolution::from(resolver.resolve().await.with_context(|| {
            format!(
                "No solution found when resolving: {}",
                requirements
                    .iter()
                    .map(|requirement| format!("`{requirement}`"))
                    .join(", ")
            )
        })?);
        Ok(resolution)
    }

    #[instrument(
        skip(self, resolution, venv),
        fields(
            resolution = resolution.distributions().map(ToString::to_string).join(", "),
            venv = ?venv.root()
        )
    )]
    async fn install<'data>(
        &'data self,
        resolution: &'data Resolution,
        venv: &'data PythonEnvironment,
    ) -> Result<Vec<CachedDist>> {
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
        let site_packages = SitePackages::from_environment(venv)?;

        let Plan {
            cached,
            remote,
            reinstalls,
            extraneous: _,
        } = Planner::new(resolution).build(
            site_packages,
            &Reinstall::default(),
            self.build_options,
            self.hasher,
            self.index_locations,
            self.config_settings,
            self.cache(),
            venv,
            tags,
        )?;

        // Nothing to do.
        if remote.is_empty() && cached.is_empty() && reinstalls.is_empty() {
            debug!("No build requirements to install for build");
            return Ok(vec![]);
        }

        // Download any missing distributions.
        let wheels = if remote.is_empty() {
            vec![]
        } else {
            // TODO(konstin): Check that there is no endless recursion.
            let preparer = Preparer::new(
                self.cache,
                tags,
                self.hasher,
                self.build_options,
                DistributionDatabase::new(self.client, self, self.concurrency.downloads),
            );

            debug!(
                "Downloading and building requirement{} for build: {}",
                if remote.len() == 1 { "" } else { "s" },
                remote.iter().map(ToString::to_string).join(", ")
            );

            preparer
                .prepare(remote, self.in_flight)
                .await
                .map_err(|err| match err {
                    uv_installer::PrepareError::DownloadAndBuild(dist, chain, err) => {
                        debug_assert!(chain.is_empty());
                        let chain =
                            DerivationChainBuilder::from_resolution(resolution, (&*dist).into())
                                .unwrap_or_default();
                        uv_installer::PrepareError::DownloadAndBuild(dist, chain, err)
                    }
                    uv_installer::PrepareError::Download(dist, chain, err) => {
                        debug_assert!(chain.is_empty());
                        let chain =
                            DerivationChainBuilder::from_resolution(resolution, (&*dist).into())
                                .unwrap_or_default();
                        uv_installer::PrepareError::Download(dist, chain, err)
                    }
                    uv_installer::PrepareError::Build(dist, chain, err) => {
                        debug_assert!(chain.is_empty());
                        let chain =
                            DerivationChainBuilder::from_resolution(resolution, (&*dist).into())
                                .unwrap_or_default();
                        uv_installer::PrepareError::Build(dist, chain, err)
                    }
                    _ => err,
                })?
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
        let mut wheels = wheels.into_iter().chain(cached).collect::<Vec<_>>();
        if !wheels.is_empty() {
            debug!(
                "Installing build requirement{}: {}",
                if wheels.len() == 1 { "" } else { "s" },
                wheels.iter().map(ToString::to_string).join(", ")
            );
            wheels = Installer::new(venv)
                .with_link_mode(self.link_mode)
                .with_cache(self.cache)
                .install(wheels)
                .await
                .context("Failed to install build dependencies")?;
        }

        Ok(wheels)
    }

    #[instrument(skip_all, fields(version_id = version_id, subdirectory = ?subdirectory))]
    async fn setup_build<'data>(
        &'data self,
        source: &'data Path,
        subdirectory: Option<&'data Path>,
        install_path: &'data Path,
        version_id: Option<String>,
        dist: Option<&'data SourceDist>,
        sources: SourceStrategy,
        build_kind: BuildKind,
        build_output: BuildOutput,
    ) -> Result<SourceBuild> {
        let dist_name = dist.map(uv_distribution_types::Name::name);
        let dist_version = dist
            .map(uv_distribution_types::DistributionMetadata::version_or_url)
            .and_then(|version| match version {
                VersionOrUrlRef::Version(version) => Some(version),
                VersionOrUrlRef::Url(_) => None,
            });

        // Note we can only prevent builds by name for packages with names
        // unless all builds are disabled.
        if self
            .build_options
            .no_build_requirement(dist_name)
            // We always allow editable builds
            && !matches!(build_kind, BuildKind::Editable)
        {
            if let Some(dist) = dist {
                return Err(anyhow!(
                    "Building source distributions for {} is disabled",
                    dist.name()
                ));
            }
            return Err(anyhow!("Building source distributions is disabled"));
        }

        let builder = SourceBuild::setup(
            source,
            subdirectory,
            install_path,
            dist_name,
            dist_version,
            self.interpreter,
            self,
            self.source_build_context.clone(),
            version_id,
            self.index_locations,
            sources,
            self.config_settings.clone(),
            self.build_isolation,
            build_kind,
            self.build_extra_env_vars.clone(),
            build_output,
            self.concurrency.builds,
        )
        .boxed_local()
        .await?;
        Ok(builder)
    }
}
