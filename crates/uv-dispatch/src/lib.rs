//! Avoid cyclic crate dependencies between [resolver][`uv_resolver`],
//! [installer][`uv_installer`] and [build][`uv_build`] through [`BuildDispatch`]
//! implementing [`BuildContext`].

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use futures::FutureExt;
use itertools::Itertools;
use rustc_hash::FxHashMap;
use thiserror::Error;
use tracing::{debug, instrument, trace};

use uv_build_backend::check_direct_build;
use uv_build_frontend::{SourceBuild, SourceBuildContext};
use uv_cache::Cache;
use uv_cache_info::CacheInfoError;
use uv_client::RegistryClient;
use uv_configuration::{
    BuildKind, BuildOptions, Constraints, HashCheckingMode, IndexStrategy, NoSources, Overrides,
    Reinstall,
};
use uv_configuration::{BuildOutput, Concurrency, Excludes};
use uv_distribution::DistributionDatabase;
use uv_distribution_filename::DistFilename;
use uv_distribution_types::{
    CachedDist, ConfigSettings, DependencyMetadata, ExtraBuildRequires, ExtraBuildVariables,
    Identifier, IndexCapabilities, IndexLocations, IsBuildBackendError, Name,
    PackageConfigSettings, Requirement, RequirementSource, RequiresPython, Resolution, SourceDist,
    VersionOrUrlRef,
};
use uv_git::GitResolver;
use uv_installer::{InstallationStrategy, Installer, Plan, Planner, Preparer, SitePackages};
use uv_pep508::MarkerTree;
use uv_preview::Preview;
use uv_pypi_types::{Conflicts, SupportedEnvironments};
use uv_python::{Interpreter, PythonEnvironment};
use uv_requirements::LookaheadResolver;
use uv_resolver::{
    ExcludeNewer, FlatIndex, Flexibility, InMemoryIndex, Manifest, OptionsBuilder, Preference,
    Preferences, PythonRequirement, Resolver, ResolverEnvironment,
};
use uv_types::{
    AnyErrorBuild, BuildArena, BuildContext, BuildIsolation, BuildPackageKey, BuildPreferences,
    BuildResolutionGraphKey, BuildResolutionStage, BuildResolutions, BuildStack,
    EmptyInstalledPackages, HashStrategy, InFlight, LockedBuildDependency, LockedBuildResolution,
    LockedBuildResolutions, ResolvedRequirements, SourceTreeEditablePolicy, build_keys_match,
};
use uv_workspace::WorkspaceCache;

#[derive(Debug, Error)]
pub enum BuildDispatchError {
    #[error(transparent)]
    BuildFrontend(#[from] AnyErrorBuild),

    #[error(transparent)]
    Tags(#[from] uv_platform_tags::TagsError),

    #[error(transparent)]
    Resolve(#[from] uv_resolver::ResolveError),

    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),

    #[error(transparent)]
    Prepare(#[from] uv_installer::PrepareError),

    #[error(transparent)]
    Lookahead(#[from] uv_requirements::Error),
}

impl uv_errors::Hint for BuildDispatchError {
    fn hints(&self) -> uv_errors::Hints<'_> {
        match self {
            Self::BuildFrontend(err) => err.hints(),
            Self::Resolve(err) => err.hints(),
            Self::Anyhow(err) => {
                // Walk the anyhow error chain to find hint-bearing errors
                // (e.g., ResolveError wrapped via `with_context`).
                for cause in err.chain() {
                    if let Some(resolve_err) = cause.downcast_ref::<uv_resolver::ResolveError>() {
                        let hints = resolve_err.hints();
                        if !hints.is_empty() {
                            return hints;
                        }
                    }
                }
                uv_errors::Hints::none()
            }
            _ => uv_errors::Hints::none(),
        }
    }
}

impl IsBuildBackendError for BuildDispatchError {
    fn is_build_backend_error(&self) -> bool {
        match self {
            Self::Tags(_)
            | Self::Resolve(_)
            | Self::Join(_)
            | Self::Anyhow(_)
            | Self::Prepare(_)
            | Self::Lookahead(_) => false,
            Self::BuildFrontend(err) => err.is_build_backend_error(),
        }
    }
}

/// The main implementation of [`BuildContext`], used by the CLI, see [`BuildContext`]
/// documentation.
pub struct BuildDispatch<'a> {
    client: &'a RegistryClient,
    cache: &'a Cache,
    constraints: &'a Constraints,
    interpreter: &'a Interpreter,
    index_locations: &'a IndexLocations,
    index_strategy: IndexStrategy,
    flat_index: &'a FlatIndex,
    shared_state: SharedState,
    dependency_metadata: &'a DependencyMetadata,
    build_isolation: BuildIsolation<'a>,
    extra_build_requires: &'a ExtraBuildRequires,
    extra_build_variables: &'a ExtraBuildVariables,
    link_mode: uv_install_wheel::LinkMode,
    build_options: &'a BuildOptions,
    config_settings: &'a ConfigSettings,
    config_settings_package: &'a PackageConfigSettings,
    hasher: &'a HashStrategy,
    exclude_newer: ExcludeNewer,
    source_build_context: SourceBuildContext,
    build_extra_env_vars: FxHashMap<OsString, OsString>,
    sources: NoSources,
    source_tree_editable_policy: SourceTreeEditablePolicy,
    workspace_cache: WorkspaceCache,
    concurrency: Concurrency,
    preview: Preview,
    build_resolutions: BuildResolutions,
    /// Active build resolution contexts for source packages being resolved.
    build_resolution_contexts:
        Mutex<BTreeMap<BuildPackageKey, BTreeMap<BuildResolutionStage, BuildResolutionGraphKey>>>,
    /// Complete build dependency resolutions reconstructed from the lock file.
    locked_build_resolutions: LockedBuildResolutions,
    build_preferences: BuildPreferences,
    /// Whether to use universal resolution for build dependencies (for lock files).
    universal_build_resolution: bool,
    /// The supported Python range to use when resolving universal build dependencies.
    universal_build_requires_python: Option<RequiresPython>,
    /// The supported marker environments to use when resolving universal build dependencies.
    universal_build_environments: SupportedEnvironments,
    /// The marker environments that require artifact coverage for universal build dependencies.
    universal_build_artifact_environments: SupportedEnvironments,
    /// The environments in which individual source packages can require builds.
    universal_build_markers: Mutex<BTreeMap<BuildPackageKey, MarkerTree>>,
    /// The environments in which individual build resolution contexts can require builds.
    universal_build_context_markers: Mutex<BTreeMap<BuildResolutionGraphKey, MarkerTree>>,
}

impl<'a> BuildDispatch<'a> {
    pub fn new(
        client: &'a RegistryClient,
        cache: &'a Cache,
        constraints: &'a Constraints,
        interpreter: &'a Interpreter,
        index_locations: &'a IndexLocations,
        flat_index: &'a FlatIndex,
        dependency_metadata: &'a DependencyMetadata,
        shared_state: SharedState,
        index_strategy: IndexStrategy,
        config_settings: &'a ConfigSettings,
        config_settings_package: &'a PackageConfigSettings,
        build_isolation: BuildIsolation<'a>,
        extra_build_requires: &'a ExtraBuildRequires,
        extra_build_variables: &'a ExtraBuildVariables,
        link_mode: uv_install_wheel::LinkMode,
        build_options: &'a BuildOptions,
        hasher: &'a HashStrategy,
        exclude_newer: ExcludeNewer,
        sources: NoSources,
        source_tree_editable_policy: SourceTreeEditablePolicy,
        workspace_cache: WorkspaceCache,
        concurrency: Concurrency,
        preview: Preview,
    ) -> Self {
        Self {
            client,
            cache,
            constraints,
            interpreter,
            index_locations,
            flat_index,
            shared_state,
            dependency_metadata,
            index_strategy,
            config_settings,
            config_settings_package,
            build_isolation,
            extra_build_requires,
            extra_build_variables,
            link_mode,
            build_options,
            hasher,
            exclude_newer,
            source_build_context: SourceBuildContext::new(concurrency.builds_semaphore.clone()),
            build_extra_env_vars: FxHashMap::default(),
            sources,
            source_tree_editable_policy,
            workspace_cache,
            concurrency,
            preview,
            build_resolutions: BuildResolutions::default(),
            build_resolution_contexts: Mutex::new(BTreeMap::new()),
            locked_build_resolutions: LockedBuildResolutions::default(),
            build_preferences: BuildPreferences::default(),
            universal_build_resolution: false,
            universal_build_requires_python: None,
            universal_build_environments: SupportedEnvironments::default(),
            universal_build_artifact_environments: SupportedEnvironments::default(),
            universal_build_markers: Mutex::new(BTreeMap::new()),
            universal_build_context_markers: Mutex::new(BTreeMap::new()),
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

    /// Set the locked build resolutions from a previous lock file.
    ///
    /// When set, build dependency resolution is skipped entirely for packages
    /// that have locked build resolutions, and the pre-built resolution is
    /// returned directly.
    #[must_use]
    pub fn with_locked_build_resolutions(mut self, resolutions: LockedBuildResolutions) -> Self {
        self.locked_build_resolutions = resolutions;
        self
    }

    /// Return the locked build resolutions used when installing source distributions.
    pub fn locked_build_resolutions(&self) -> &LockedBuildResolutions {
        &self.locked_build_resolutions
    }

    /// Set the build dependency preferences from a previous lock file.
    ///
    /// Used during `uv lock` so the resolver prefers previously locked build dep
    /// versions but can deviate if needed.
    #[must_use]
    pub fn with_build_preferences(mut self, preferences: BuildPreferences) -> Self {
        self.build_preferences = preferences;
        self
    }

    /// Use universal resolution for build dependencies.
    ///
    /// When enabled, build dependencies are resolved for all platforms rather
    /// than just the current one. This is needed for lock files.
    #[must_use]
    pub fn with_universal_build_resolution(
        mut self,
        requires_python: RequiresPython,
        environments: SupportedEnvironments,
        artifact_environments: SupportedEnvironments,
    ) -> Self {
        self.universal_build_resolution = true;
        self.universal_build_requires_python = Some(requires_python);
        self.universal_build_environments = environments;
        self.universal_build_artifact_environments = artifact_environments;
        self
    }

    /// Record the marker environments in which a source package can require building.
    ///
    /// Returns `true` if the marker environments for the package expanded.
    pub fn add_universal_build_marker(&self, package: BuildPackageKey, marker: MarkerTree) -> bool {
        let mut markers = self
            .universal_build_markers
            .lock()
            .expect("universal build marker lock poisoned");
        merge_marker(markers.entry(package), marker)
    }

    /// Record the marker environments in which a build resolution context can require building.
    ///
    /// Returns `true` if the marker environments for the context expanded.
    pub fn add_universal_build_context_marker(
        &self,
        context: BuildResolutionGraphKey,
        marker: MarkerTree,
    ) -> bool {
        let mut markers = self
            .universal_build_context_markers
            .lock()
            .expect("universal build context marker lock poisoned");
        merge_marker(markers.entry(context), marker)
    }

    fn universal_build_marker(
        &self,
        package: &BuildPackageKey,
        stage: BuildResolutionStage,
    ) -> Option<MarkerTree> {
        if let Some(key) = self.build_resolution_context(package, stage) {
            if let Some(marker) = self
                .universal_build_context_markers
                .lock()
                .expect("universal build context marker lock poisoned")
                .get(&key)
                .copied()
            {
                return Some(marker);
            }
        }

        self.universal_build_markers
            .lock()
            .expect("universal build marker lock poisoned")
            .get(package)
            .copied()
    }

    /// Return the collected build resolutions.
    pub fn build_resolutions(&self) -> &BuildResolutions {
        &self.build_resolutions
    }

    /// Record the active build resolution context for a source package.
    ///
    /// The context is assigned by the lockfile layer, which owns the serialized
    /// resolution identity. Build dispatch only preserves the association while
    /// backend setup resolves build requirements.
    pub fn set_build_resolution_context(&self, context: BuildResolutionGraphKey) {
        let stage = context.stage.unwrap_or(BuildResolutionStage::Build);
        let mut contexts = self
            .build_resolution_contexts
            .lock()
            .expect("build resolution context lock poisoned");
        contexts
            .entry(context.package.clone())
            .or_default()
            .insert(stage, context);
    }

    /// Record both active PEP 517 stage contexts for a source package.
    pub fn set_build_resolution_stage_contexts(
        &self,
        package: BuildPackageKey,
        bootstrap: BuildResolutionGraphKey,
        build: BuildResolutionGraphKey,
    ) {
        let mut contexts = self
            .build_resolution_contexts
            .lock()
            .expect("build resolution context lock poisoned");
        let package_contexts = contexts.entry(package).or_default();
        package_contexts.insert(BuildResolutionStage::Bootstrap, bootstrap);
        package_contexts.insert(BuildResolutionStage::Build, build);
    }

    fn build_resolution_context(
        &self,
        package: &BuildPackageKey,
        stage: BuildResolutionStage,
    ) -> Option<BuildResolutionGraphKey> {
        let contexts = self
            .build_resolution_contexts
            .lock()
            .expect("build resolution context lock poisoned");
        let package_contexts = contexts.get(package).or_else(|| {
            let mut matching = contexts
                .iter()
                .filter(|(candidate, _)| build_keys_match(candidate, package))
                .map(|(_, contexts)| contexts);
            let first = matching.next()?;
            matching.next().is_none().then_some(first)
        })?;
        package_contexts
            .get(&stage)
            .or_else(|| package_contexts.get(&BuildResolutionStage::Build))
            .cloned()
    }

    fn universal_environments_for_package(
        &self,
        environments: &SupportedEnvironments,
        package: Option<&BuildPackageKey>,
        stage: BuildResolutionStage,
        restrict_unconstrained: bool,
    ) -> SupportedEnvironments {
        let Some(marker) = package.and_then(|package| self.universal_build_marker(package, stage))
        else {
            return environments.clone();
        };

        if environments.is_empty() {
            return if restrict_unconstrained && !marker.is_true() {
                SupportedEnvironments::from_markers(vec![marker])
            } else {
                SupportedEnvironments::default()
            };
        }

        SupportedEnvironments::from_markers(
            environments
                .iter()
                .copied()
                .filter_map(|mut environment| {
                    environment.and(marker);
                    (!environment.is_false()).then_some(environment)
                })
                .collect(),
        )
    }

    fn locked_resolution_satisfies(
        &self,
        resolution: &LockedBuildResolution,
        requirements: &[Requirement],
    ) -> bool {
        let markers = self.interpreter.to_resolver_marker_environment();
        requirements
            .iter()
            .filter(|requirement| requirement.evaluate_markers(Some(&markers), &[]))
            .all(|requirement| {
                resolution
                    .direct_dependencies()
                    .iter()
                    .any(|dependency| locked_dependency_satisfies(requirement, dependency))
            })
    }

    fn locked_initial_resolution(
        &self,
        resolution: &LockedBuildResolution,
        requirements: &[Requirement],
    ) -> Option<Resolution> {
        let direct_dependencies = resolution
            .bootstrap_direct_dependencies()
            .unwrap_or_else(|| resolution.direct_dependencies());
        let markers = self.interpreter.to_resolver_marker_environment();
        let initial_requirements = resolution.initial_requirements().unwrap_or(requirements);
        let mut selected = Resolution::default();
        for initial_requirement in initial_requirements
            .iter()
            .filter(|requirement| requirement.evaluate_markers(Some(&markers), &[]))
        {
            let dependency = direct_dependencies
                .iter()
                .find(|dependency| locked_dependency_satisfies(initial_requirement, dependency))
                .or_else(|| {
                    // Match-runtime requirements are source-matched when replay begins, while
                    // the stored initial requirement retains its original registry source. Use
                    // current requirements only to find the source-matched counterpart of a
                    // locked initial root; otherwise mutable source changes could bypass the lock.
                    requirements
                        .iter()
                        .filter(|requirement| {
                            requirement.name == initial_requirement.name
                                && requirement.extras == initial_requirement.extras
                                && requirement.evaluate_markers(Some(&markers), &[])
                        })
                        .find_map(|requirement| {
                            direct_dependencies.iter().find(|dependency| {
                                locked_dependency_satisfies(requirement, dependency)
                            })
                        })
                })?;
            selected.extend(dependency.resolution());
        }
        Some(selected)
    }
}

fn merge_marker<K: Ord>(entry: Entry<'_, K, MarkerTree>, marker: MarkerTree) -> bool {
    match entry {
        Entry::Vacant(entry) => {
            entry.insert(marker);
            true
        }
        Entry::Occupied(mut entry) => {
            let existing = *entry.get();
            let mut combined = existing;
            combined.or(marker);
            if combined == existing {
                false
            } else {
                entry.insert(combined);
                true
            }
        }
    }
}

fn locked_dependency_satisfies(
    requirement: &Requirement,
    dependency: &LockedBuildDependency,
) -> bool {
    if requirement.name != *dependency.dist.name()
        || !requirement
            .extras
            .iter()
            .all(|extra| dependency.extras.contains(extra))
    {
        return false;
    }

    let selected_source = RequirementSource::from(&dependency.dist);
    match (&requirement.source, &selected_source) {
        (
            RequirementSource::Registry {
                specifier, index, ..
            },
            RequirementSource::Registry {
                index: selected_index,
                ..
            },
        ) => {
            dependency
                .dist
                .version()
                .is_some_and(|version| specifier.contains(version))
                && index
                    .as_ref()
                    .is_none_or(|index| selected_index.as_ref() == Some(index))
        }
        (source, selected_source) => source == selected_source,
    }
}

#[allow(refining_impl_trait)]
impl BuildContext for BuildDispatch<'_> {
    type SourceDistBuilder = SourceBuild;

    async fn interpreter(&self) -> &Interpreter {
        self.interpreter
    }

    fn cache(&self) -> &Cache {
        self.cache
    }

    fn git(&self) -> &GitResolver {
        &self.shared_state.git
    }

    fn build_arena(&self) -> &BuildArena<SourceBuild> {
        &self.shared_state.build_arena
    }

    fn capabilities(&self) -> &IndexCapabilities {
        &self.shared_state.capabilities
    }

    fn dependency_metadata(&self) -> &DependencyMetadata {
        self.dependency_metadata
    }

    fn build_options(&self) -> &BuildOptions {
        self.build_options
    }

    fn build_isolation(&self) -> BuildIsolation<'_> {
        self.build_isolation
    }

    fn config_settings(&self) -> &ConfigSettings {
        self.config_settings
    }

    fn config_settings_package(&self) -> &PackageConfigSettings {
        self.config_settings_package
    }

    fn sources(&self) -> &NoSources {
        &self.sources
    }

    fn source_tree_editable_policy(&self) -> SourceTreeEditablePolicy {
        self.source_tree_editable_policy
    }

    fn locations(&self) -> &IndexLocations {
        self.index_locations
    }

    fn workspace_cache(&self) -> &WorkspaceCache {
        &self.workspace_cache
    }

    fn extra_build_requires(&self) -> &ExtraBuildRequires {
        self.extra_build_requires
    }

    fn extra_build_variables(&self) -> &ExtraBuildVariables {
        self.extra_build_variables
    }

    fn has_locked_build_resolution(&self, package: &BuildPackageKey) -> bool {
        self.locked_build_resolutions.get(package).is_some()
    }

    fn locked_build_resolution_cache_key(
        &self,
        package: &BuildPackageKey,
    ) -> Result<Option<String>, CacheInfoError> {
        self.locked_build_resolutions.cache_key(
            package,
            self.config_settings,
            self.config_settings_package,
            self.extra_build_requires,
            self.extra_build_variables,
        )
    }

    async fn resolve<'data>(
        &'data self,
        requirements: &'data [Requirement],
        package: Option<&'data BuildPackageKey>,
        build_stack: &'data BuildStack,
        validate_locked_requirements: Option<&'data [Requirement]>,
    ) -> Result<ResolvedRequirements, BuildDispatchError> {
        let stage = if validate_locked_requirements.is_some() {
            BuildResolutionStage::Build
        } else {
            BuildResolutionStage::Bootstrap
        };

        // If we have a locked build resolution for this package, replay the appropriate stage
        // without running the resolver. Backend hook requirements are validated separately from
        // `build-system.requires`, which remains fixed by a frozen lock.
        if let Some(package) = package
            && let Some(locked_resolution) = self.locked_build_resolutions.get(package)
        {
            let resolution = if let Some(requirements) = validate_locked_requirements {
                if !self.locked_resolution_satisfies(locked_resolution, requirements) {
                    return Err(BuildDispatchError::Anyhow(anyhow::anyhow!(
                        "The build requirements returned by the backend for `{}` do not match the locked build environment",
                        package.name
                    )));
                }
                locked_resolution.resolution().clone()
            } else {
                self.locked_initial_resolution(locked_resolution, requirements)
                    .ok_or_else(|| {
                        BuildDispatchError::Anyhow(anyhow::anyhow!(
                            "The initial build requirements for `{}` do not match the locked bootstrap environment",
                            package.name
                        ))
                    })?
            };
            debug!(
                "Using locked build resolution for `{}=={:?}` (skipping resolver)",
                package.name, package.version
            );
            let hasher = HashStrategy::from_resolution(&resolution, HashCheckingMode::Verify)
                .map_err(anyhow::Error::from)?;
            return Ok(ResolvedRequirements::new(resolution, hasher));
        }

        let marker_env = self.interpreter.to_resolver_marker_environment();
        let python_requirement = if self.universal_build_resolution {
            if let Some(requires_python) = self.universal_build_requires_python.clone() {
                PythonRequirement::from_marker_environment(&marker_env, requires_python)
            } else {
                PythonRequirement::from_interpreter(self.interpreter)
            }
        } else {
            PythonRequirement::from_interpreter(self.interpreter)
        };
        let universal_build_environments = self.universal_environments_for_package(
            &self.universal_build_environments,
            package,
            stage,
            true,
        );
        let universal_build_artifact_environments = self.universal_environments_for_package(
            &self.universal_build_artifact_environments,
            package,
            stage,
            false,
        );
        let resolver_env = if self.universal_build_resolution {
            ResolverEnvironment::universal(universal_build_environments.clone().into_markers())
        } else {
            ResolverEnvironment::specific(marker_env)
        };
        let tags = if self.universal_build_resolution {
            None
        } else {
            Some(self.interpreter.tags()?)
        };

        // When the lock file has stored build dependencies for this package, use
        // them as preferences so the resolver prefers the same versions.
        let preferences = package
            .and_then(|package| self.build_preferences.get(package))
            .map(|deps| {
                Preferences::from_iter(
                    deps.iter().map(|(name, version)| {
                        Preference::from_package_build(name.clone(), version.clone())
                    }),
                    &resolver_env,
                )
            })
            .unwrap_or_default();

        // Walk any URL requirements transitively so their sub-URLs (for example, a workspace
        // member that depends on another workspace member) are known before the resolver runs
        // its URL allow-list check. This mirrors what the project resolver does in
        // `uv_requirements::LookaheadResolver` and prevents a `DisallowedUrl` error when one
        // `build-system.requires` entry pulls in another URL dependency.
        let hasher = self
            .hasher
            .clone()
            .augment_with_requirements(requirements.iter())
            .map_err(uv_requirements::Error::from)?;
        let overrides = Overrides::default();
        let excludes = Excludes::default();
        let (lookaheads, hasher) = LookaheadResolver::new(
            requirements,
            self.constraints,
            &overrides,
            &excludes,
            &hasher,
            &self.shared_state.index,
            DistributionDatabase::new(
                self.client,
                self,
                self.concurrency.downloads_semaphore.clone(),
            )
            .with_build_stack(build_stack),
        )
        .resolve(&resolver_env)
        .await?;

        let manifest = Manifest::simple(requirements.to_vec())
            .with_constraints(self.constraints.clone())
            .with_preferences(preferences)
            .with_lookaheads(lookaheads);

        let resolver = Resolver::new(
            manifest,
            OptionsBuilder::new()
                .exclude_newer(self.exclude_newer.clone())
                .index_strategy(self.index_strategy)
                .build_options(self.build_options.clone())
                .artifact_environments(universal_build_artifact_environments)
                .flexibility(Flexibility::Fixed)
                .build(),
            &python_requirement,
            resolver_env,
            self.interpreter.markers(),
            // Conflicting groups only make sense when doing universal resolution.
            Conflicts::empty(),
            tags,
            self.flat_index,
            &self.shared_state.index,
            &hasher,
            self,
            EmptyInstalledPackages,
            DistributionDatabase::new(
                self.client,
                self,
                self.concurrency.downloads_semaphore.clone(),
            )
            .with_build_stack(build_stack),
        )?;
        let resolver_output = resolver.resolve().boxed_local().await.with_context(|| {
            format!(
                "No solution found when resolving: {}",
                requirements
                    .iter()
                    .map(|requirement| format!("`{requirement}`"))
                    .join(", ")
            )
        })?;

        // If doing universal resolution, capture the build resolution graph
        // (direct requirements + all packages with their dependency edges).
        let build_resolution_graph = if self.universal_build_resolution {
            Some(resolver_output.build_resolution_graph())
        } else {
            None
        };

        if let (Some(package), Some(graph)) = (package, build_resolution_graph.clone()) {
            if let Some(key) = self.build_resolution_context(package, stage) {
                self.build_resolutions.insert_key(key, graph.clone());
                if stage == BuildResolutionStage::Bootstrap
                    && let Some(build_key) =
                        self.build_resolution_context(package, BuildResolutionStage::Build)
                {
                    self.build_resolutions.insert_key(build_key, graph);
                }
            } else {
                self.build_resolutions.insert(package.clone(), graph);
            }
        }

        let resolution = if self.universal_build_resolution {
            let markers = if universal_build_environments.is_empty()
                || universal_build_environments
                    .iter()
                    .any(|environment| environment.evaluate(self.interpreter.markers(), &[]))
            {
                Some(self.interpreter.markers())
            } else {
                None
            };
            resolver_output.into_build_resolution(markers)
        } else {
            Resolution::from(resolver_output)
        };
        let requirements = ResolvedRequirements::new(resolution, hasher);
        Ok(if let Some(graph) = build_resolution_graph {
            requirements.with_build_resolution_graph(graph)
        } else {
            requirements
        })
    }

    #[instrument(
        skip(self, requirements, venv),
        fields(
            resolution = requirements.resolution().distributions().map(ToString::to_string).join(", "),
            venv = ?venv.root()
        )
    )]
    async fn install<'data>(
        &'data self,
        requirements: &'data ResolvedRequirements,
        venv: &'data PythonEnvironment,
        build_stack: &'data BuildStack,
    ) -> Result<Vec<CachedDist>, BuildDispatchError> {
        let resolution = requirements.resolution();
        let hasher = requirements.hasher();

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
        } = Planner::new(resolution)
            .with_locked_build_resolutions(&self.locked_build_resolutions)
            .build(
                site_packages,
                InstallationStrategy::Permissive,
                &Reinstall::default(),
                self.build_options,
                hasher,
                self.index_locations,
                self.config_settings,
                self.config_settings_package,
                self.extra_build_requires(),
                self.extra_build_variables,
                self.cache(),
                venv,
                tags,
            )?;

        // Nothing to do.
        if remote.is_empty() && cached.is_empty() && reinstalls.is_empty() {
            debug!("No build requirements to install for build");
            return Ok(vec![]);
        }

        // Verify that none of the missing distributions are already in the build stack.
        for dist in &remote {
            let id = dist.distribution_id();
            if build_stack.contains(&id) {
                return Err(BuildDispatchError::BuildFrontend(
                    uv_build_frontend::Error::CyclicBuildDependency(dist.name().clone()).into(),
                ));
            }
        }

        // Download any missing distributions.
        let wheels = if remote.is_empty() {
            vec![]
        } else {
            let preparer = Preparer::new(
                self.cache,
                tags,
                hasher,
                self.build_options,
                DistributionDatabase::new(
                    self.client,
                    self,
                    self.concurrency.downloads_semaphore.clone(),
                )
                .with_build_stack(build_stack),
            );

            debug!(
                "Downloading and building requirement{} for build: {}",
                if remote.len() == 1 { "" } else { "s" },
                remote.iter().map(ToString::to_string).join(", ")
            );

            preparer
                .prepare(remote, &self.shared_state.in_flight, resolution)
                .await?
        };

        // Remove any unnecessary packages.
        if !reinstalls.is_empty() {
            let layout = venv.interpreter().layout();
            for dist_info in &reinstalls {
                let summary = uv_installer::uninstall(dist_info, &layout)
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
            wheels = Installer::new(venv, self.preview)
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
        stop_discovery_at: Option<&'data Path>,
        version_id: Option<&'data str>,
        dist: Option<&'data SourceDist>,
        sources: &'data NoSources,
        build_kind: BuildKind,
        build_output: BuildOutput,
        mut build_stack: BuildStack,
    ) -> Result<SourceBuild, uv_build_frontend::Error> {
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
            let err = if let Some(dist) = dist {
                uv_build_frontend::Error::NoSourceDistBuild(dist.name().clone())
            } else {
                uv_build_frontend::Error::NoSourceDistBuilds
            };
            return Err(err);
        }

        // Push the current distribution onto the build stack, to prevent cyclic dependencies.
        if let Some(dist) = dist {
            build_stack.insert(dist.distribution_id());
        }

        // Get package-specific config settings if available; otherwise, use global settings.
        let config_settings = if let Some(name) = dist_name {
            if let Some(package_settings) = self.config_settings_package.get(name) {
                package_settings.clone().merge(self.config_settings.clone())
            } else {
                self.config_settings.clone()
            }
        } else {
            self.config_settings.clone()
        };

        // Get package-specific environment variables if available.
        let mut environment_variables = self.build_extra_env_vars.clone();
        if let Some(name) = dist_name {
            if let Some(package_vars) = self.extra_build_variables.get(name) {
                environment_variables.extend(
                    package_vars
                        .iter()
                        .map(|(key, value)| (OsString::from(key), OsString::from(value))),
                );
            }
        }

        let builder = SourceBuild::setup(
            source,
            subdirectory,
            install_path,
            dist,
            stop_discovery_at,
            dist_name,
            dist_version,
            self.interpreter,
            self,
            self.source_build_context.clone(),
            version_id,
            self.index_locations,
            sources.clone(),
            self.workspace_cache(),
            config_settings,
            self.build_isolation,
            self.extra_build_requires,
            &build_stack,
            build_kind,
            environment_variables,
            build_output,
            self.client.credentials_cache(),
        )
        .boxed_local()
        .await?;
        Ok(builder)
    }

    async fn direct_build<'data>(
        &'data self,
        source: &'data Path,
        subdirectory: Option<&'data Path>,
        output_dir: &'data Path,
        sources: NoSources,
        build_kind: BuildKind,
        version_id: Option<&'data str>,
    ) -> Result<Option<DistFilename>, BuildDispatchError> {
        let source_tree = if let Some(subdir) = subdirectory {
            source.join(subdir)
        } else {
            source.to_path_buf()
        };

        // Only perform the direct build if the backend is uv in a compatible version.
        let source_tree_str = source_tree.display().to_string();
        let identifier = version_id.unwrap_or_else(|| &source_tree_str);
        if let Err(reason) = check_direct_build(&source_tree, uv_version::version()) {
            trace!("Requirements for direct build not matched because {reason}");
            return Ok(None);
        }

        debug!("Performing direct build for {identifier}");

        let output_dir = output_dir.to_path_buf();
        let filename = tokio::task::spawn_blocking(move || -> Result<_> {
            let filename = match build_kind {
                BuildKind::Wheel => {
                    let wheel = uv_build_backend::build_wheel(
                        &source_tree,
                        &output_dir,
                        None,
                        uv_version::version(),
                        sources.is_none(),
                    )?;
                    DistFilename::WheelFilename(wheel)
                }
                BuildKind::Sdist => {
                    let source_dist = uv_build_backend::build_source_dist(
                        &source_tree,
                        &output_dir,
                        uv_version::version(),
                        sources.is_none(),
                    )?;
                    DistFilename::SourceDistFilename(source_dist)
                }
                BuildKind::Editable => {
                    let wheel = uv_build_backend::build_editable(
                        &source_tree,
                        &output_dir,
                        None,
                        uv_version::version(),
                        sources.is_none(),
                    )?;
                    DistFilename::WheelFilename(wheel)
                }
            };
            Ok(filename)
        })
        .await??;

        Ok(Some(filename))
    }
}

/// Shared state used during resolution and installation.
///
/// All elements are `Arc`s, so we can clone freely.
#[derive(Default, Clone)]
pub struct SharedState {
    /// The resolved Git references.
    git: GitResolver,
    /// The discovered capabilities for each registry index.
    capabilities: IndexCapabilities,
    /// The fetched package versions and metadata.
    index: InMemoryIndex,
    /// The downloaded distributions.
    in_flight: InFlight,
    /// Build directories for any PEP 517 builds executed during resolution or installation.
    build_arena: BuildArena<SourceBuild>,
}

impl SharedState {
    /// Fork the [`SharedState`], creating a new in-memory index and in-flight cache.
    ///
    /// State that is universally applicable (like the Git resolver and index capabilities)
    /// are retained.
    #[must_use]
    pub fn fork(&self) -> Self {
        Self {
            git: self.git.clone(),
            capabilities: self.capabilities.clone(),
            build_arena: self.build_arena.clone(),
            ..Default::default()
        }
    }

    /// Return the [`GitResolver`] used by the [`SharedState`].
    pub fn git(&self) -> &GitResolver {
        &self.git
    }

    /// Return the [`InMemoryIndex`] used by the [`SharedState`].
    pub fn index(&self) -> &InMemoryIndex {
        &self.index
    }

    /// Return the [`InFlight`] used by the [`SharedState`].
    pub fn in_flight(&self) -> &InFlight {
        &self.in_flight
    }
}
